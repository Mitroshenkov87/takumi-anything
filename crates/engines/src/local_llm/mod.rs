//! Local models: llama.cpp's `llama-server` run on this computer with a GGUF model, spoken to over
//! its OpenAI-compatible API on 127.0.0.1.
//!
//! - The engine is a pinned official ggml-org release (CUDA build and its CUDA runtime for Windows
//!   x64), each zip checked against the SHA-256 GitHub publishes for it before it is unpacked; an
//!   existing `llama-server` (on PATH, or one another app installed) can be used instead.
//! - Models come from a short catalog ([`catalog`]) downloaded from Hugging Face (resumable, size
//!   and SHA-256 checked), or are any `.gguf` file the person links where it is.
//! - One server runs at a time. Its address is kept in `server.json` in the engine folder, so
//!   another Takumi Anything process (the CLI) can see and stop it.
//!
//! Nothing here reads or changes what is said to the model: prompts and answers pass through as
//! they are.

use craft_anything_evidence::{ErrorCode, ToolError};
use craft_anything_platform::process::{Cancel, Launch, Launched, Process};
use craft_anything_platform::sync::Mutex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The llama.cpp release Takumi Anything installs.
pub const BUILD: &str = "b12722";
/// Its release page, for installing by hand.
pub const PAGE: &str = "https://github.com/ggml-org/llama.cpp/releases/tag/b12722";
/// The folder inside the engine folder that the managed install is unpacked into.
pub const FOLDER: &str = "llama.cpp";
/// The context window every preset asks for (tokens).
pub const CONTEXT: u32 = 32768;
/// Free video memory (MiB) the fast looped-MoE layout needs; below it the safer layout is used.
/// Inkling at 32k with the fast layout takes about 15,000 MiB (about 39 tokens a second on a
/// 16 GB laptop card, against 35 with the safe one); this leaves about 300 MiB to spare, so it is
/// still chosen with Takumi Anything's own window open on the same card.
pub const LOOPED_FAST_FREE_MB: u64 = 15_360;

/// One pinned download of the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnginePin {
    pub url: &'static str,
    /// As GitHub publishes it for the release asset (lowercase hex).
    pub sha256: &'static str,
    pub bytes: u64,
}

/// llama.cpp b12722 for Windows x64 with CUDA 13.5, and the CUDA runtime it needs, as published on
/// the release (GitHub asset digests).
const WINDOWS_CUDA: &[EnginePin] = &[
    EnginePin {
        url: "https://github.com/ggml-org/llama.cpp/releases/download/b12722/llama-b12722-bin-win-cuda-13.5-x64.zip",
        sha256: "917b602efa78fcb4f8c6f34e0cb65e0d31aacc50a86c54508d8caa4b7698c563",
        bytes: 226_085_437,
    },
    EnginePin {
        url: "https://github.com/ggml-org/llama.cpp/releases/download/b12722/cudart-llama-bin-win-cuda-13.5-x64.zip",
        sha256: "c2bf4cc202bfbaaa7a0b75f5082457c3811326947bb216059b72839d844e9cba",
        bytes: 426_314_199,
    },
];

/// The downloads [`install_engine`] makes on this computer (empty where none is pinned).
pub fn engine_pins() -> &'static [EnginePin] {
    if cfg!(all(windows, target_arch = "x86_64")) { WINDOWS_CUDA } else { &[] }
}

/// Whether Takumi Anything can install the engine here (Windows x64 today).
pub fn can_install() -> bool {
    !engine_pins().is_empty()
}

/// What installing the engine downloads, in MB.
pub fn engine_download_mb() -> u64 {
    engine_pins().iter().map(|p| p.bytes).sum::<u64>() / 1_000_000
}

/// The server program's file name here.
pub fn server_exe() -> &'static str {
    if cfg!(windows) { "llama-server.exe" } else { "llama-server" }
}

/// Largest engine zip downloaded, and largest file unpacked from it (the CUDA libraries are big).
const MAX_ZIP: u64 = 768 * 1024 * 1024;
const MAX_MEMBER: u64 = 1024 * 1024 * 1024;

// ---- Finding the engine --------------------------------------------------------------------

/// `llama-server` in `dir` or the folders under it (a few levels: releases unpack into `build/bin`).
pub fn find_server(dir: &Path) -> Option<PathBuf> {
    find_below(dir, server_exe(), 4)
}

fn find_below(dir: &Path, exe: &str, depth: u32) -> Option<PathBuf> {
    let here = dir.join(exe);
    if here.is_file() {
        return Some(here);
    }
    if depth == 0 {
        return None;
    }
    let mut subdirs: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    // The newest build first when several are side by side (b12722 before b12600).
    subdirs.sort();
    subdirs.reverse();
    subdirs.iter().find_map(|d| find_below(d, exe, depth - 1))
}

/// The engine Takumi Anything installed in `engine_dir`, if it is there. Takumi Anything runs only this one: its
/// own pinned, checksum-checked build in its own data folder.
pub fn managed_server(engine_dir: &Path) -> Option<PathBuf> {
    find_server(&engine_dir.join(FOLDER))
}

// ---- Installing the engine -----------------------------------------------------------------

/// Download the pinned llama.cpp release and its CUDA runtime, check each against its SHA-256,
/// and unpack both into `<engine_dir>/llama.cpp`, replacing what was there only once all of it is
/// in. `progress` gets (what is downloading, bytes so far, total): the bytes count across every
/// download, so it adds up to [`engine_download_mb`], the size the Install button shows.
pub fn install_engine(engine_dir: &Path, cancel: Option<Cancel>, progress: &dyn Fn(&str, u64, Option<u64>)) -> Result<PathBuf, ToolError> {
    let pins = engine_pins();
    if pins.is_empty() {
        return Err(ToolError::new(ErrorCode::NotSupported, "Takumi Anything installs its llama.cpp engine only on Windows (x64) so far."));
    }
    std::fs::create_dir_all(engine_dir).map_err(|e| ToolError::io(engine_dir.display(), e))?;
    let incoming = engine_dir.join(format!(".{FOLDER}.incoming-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&incoming);
    std::fs::create_dir_all(&incoming).map_err(|e| ToolError::io(incoming.display(), e))?;
    let all: u64 = pins.iter().map(|p| p.bytes).sum();
    let result = (|| {
        let mut before = 0;
        for (i, pin) in pins.iter().enumerate() {
            let what = if i == 0 { format!("llama.cpp {BUILD}") } else { "CUDA runtime".to_string() };
            let zip = engine_dir.join(format!(".llama-{i}-{}.zip", std::process::id()));
            crate::install::download_verified(pin.url, &zip, pin.sha256, MAX_ZIP, cancel.as_ref(), &|got, _| progress(&what, before + got, Some(all)))?;
            before += pin.bytes;
            let data = std::fs::read(&zip).map_err(|e| ToolError::io(zip.display(), e));
            let _ = std::fs::remove_file(&zip);
            crate::install::unpack_zip_into(&data?, &incoming, MAX_MEMBER, cancel.as_ref())?;
        }
        find_server(&incoming).ok_or_else(|| ToolError::new(ErrorCode::EngineMissing, "The llama.cpp download has no llama-server in it."))
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&incoming);
        return Err(e);
    }
    let dest = engine_dir.join(FOLDER);
    let _ = std::fs::remove_dir_all(&dest);
    if let Err(e) = std::fs::rename(&incoming, &dest) {
        let _ = std::fs::remove_dir_all(&incoming);
        return Err(ToolError::io(dest.display(), e));
    }
    managed_server(engine_dir).ok_or_else(|| ToolError::new(ErrorCode::EngineMissing, "llama-server isn't where it was unpacked."))
}

// ---- The catalog ---------------------------------------------------------------------------

/// Which tab a catalog model is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Refusals removed (abliterated or uncensored fine-tunes).
    Uncensored,
    /// The publisher's own model.
    Classic,
}

/// One model in the catalog (`local_llm/catalog.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogModel {
    pub id: String,
    pub category: Category,
    /// The video-memory tier it is offered for (GB).
    pub tier_gb: u32,
    pub name: String,
    /// The Hugging Face repository.
    pub repo: String,
    /// The GGUF file in it.
    pub file: String,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub license: String,
    pub base_model: String,
    /// Video memory it needs with a 32k context fully on the card (GB, estimated).
    pub vram_gb: f32,
    /// The smallest card it still runs on with part of it in system memory (GB), if it does.
    pub runs_on_gb: Option<f32>,
    /// llama.cpp's chat template for it handles tool calls.
    pub tools: bool,
    /// Launch preset ([`Preset`]) other than the default.
    pub preset: Option<String>,
    /// The oldest llama.cpp build it runs on, when that matters.
    pub min_build: Option<u32>,
    /// Sampling the model card recommends, given to llama-server as its defaults.
    #[serde(default)]
    pub sampling: Option<Sampling>,
    /// One plain line about it (English; the UI translates it).
    pub note: String,
}

impl CatalogModel {
    /// Where the file downloads from.
    pub fn download_url(&self) -> String {
        format!("https://huggingface.co/{}/resolve/main/{}", self.repo, self.file)
    }

    /// Its page on Hugging Face.
    pub fn page(&self) -> String {
        format!("https://huggingface.co/{}", self.repo)
    }

    /// How it fits a card with `vram_mb` MiB of video memory.
    pub fn fit(&self, vram_mb: u64) -> Fit {
        let gb = vram_mb as f32 / 1024.0;
        if gb + 0.25 >= self.vram_gb {
            Fit::Fits
        } else if self.runs_on_gb.is_some_and(|r| gb + 0.25 >= r) {
            Fit::Partly
        } else {
            Fit::No
        }
    }
}

/// Sampling defaults for a model, from its card.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Sampling {
    pub temp: f32,
    pub top_p: f32,
    pub top_k: u32,
    pub min_p: f32,
}

impl Sampling {
    /// llama-server's flags for it. `--min-p` is always passed: llama.cpp's own default is 0.05.
    pub fn args(&self) -> Vec<String> {
        ["--temp", &self.temp.to_string(), "--top-p", &self.top_p.to_string(), "--top-k", &self.top_k.to_string(), "--min-p", &self.min_p.to_string()]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }
}

/// How a model fits this computer's video memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// All of it on the card, with a 32k context.
    Fits,
    /// It runs with part of it in system memory.
    Partly,
    /// Too big for this card.
    No,
}

#[derive(Deserialize)]
struct CatalogFile {
    models: Vec<CatalogModel>,
}

/// The models Takumi Anything offers to download: five with refusals removed and five official ones, one
/// per video-memory tier.
pub fn catalog() -> &'static [CatalogModel] {
    static CATALOG: OnceLock<Vec<CatalogModel>> = OnceLock::new();
    CATALOG.get_or_init(|| match serde_json::from_str::<CatalogFile>(include_str!("catalog.json")) {
        Ok(c) => c.models,
        Err(e) => {
            log::error!("local model catalog: {e}");
            Vec::new()
        }
    })
}

/// The catalog entry for a model file, by its file name.
pub fn catalog_entry_for(file: &Path) -> Option<&'static CatalogModel> {
    let name = file.file_name()?.to_string_lossy();
    catalog().iter().find(|m| m.file.eq_ignore_ascii_case(&name))
}

/// Download a catalog model into `models_dir`, carrying on from an earlier try. Returns the file.
pub fn download_model(model: &CatalogModel, models_dir: &Path, cancel: Option<Cancel>, progress: &dyn Fn(u64, Option<u64>)) -> Result<PathBuf, ToolError> {
    if model.file.contains(['/', '\\']) || model.file.starts_with('.') || !model.file.to_ascii_lowercase().ends_with(".gguf") {
        return Err(ToolError::internal(format!("catalog file name {:?} isn't a plain .gguf name", model.file)));
    }
    std::fs::create_dir_all(models_dir).map_err(|e| ToolError::io(models_dir.display(), e))?;
    let dest = models_dir.join(&model.file);
    if std::fs::metadata(&dest).is_ok_and(|m| m.len() == model.size_bytes) {
        return Ok(dest);
    }
    crate::install::download_resumable(&model.download_url(), &dest, Some(model.size_bytes), model.sha256.as_deref(), cancel.as_ref(), progress)?;
    Ok(dest)
}

// ---- Video memory --------------------------------------------------------------------------

/// A graphics card and its memory (MiB).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Gpu {
    pub name: String,
    pub total_mb: u64,
    pub free_mb: u64,
}

/// The NVIDIA cards here, as `nvidia-smi` reports them (empty without one or its driver).
pub fn gpus() -> Vec<Gpu> {
    let out = Process::new("nvidia-smi")
        .args(["--query-gpu=name,memory.total,memory.free", "--format=csv,noheader,nounits"])
        .timeout(Duration::from_secs(10))
        .label("nvidia-smi")
        .run();
    match out {
        Ok(o) if o.success => parse_gpus(&o.stdout_text()),
        _ => Vec::new(),
    }
}

fn parse_gpus(text: &str) -> Vec<Gpu> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split(',').map(str::trim);
            let name = parts.next()?.to_string();
            let total_mb = parts.next()?.parse().ok()?;
            let free_mb = parts.next()?.parse().ok()?;
            Some(Gpu { name, total_mb, free_mb })
        })
        .collect()
}

// ---- Launch presets ------------------------------------------------------------------------

/// How a model is laid out between the card and system memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// llama.cpp fits it to the free memory itself (`--fit on`).
    Auto,
    /// A looped mixture-of-experts model (Inkling): everything on the card except the experts of
    /// the outer, unlooped layers, which stay in system memory; the layers run three times stay
    /// on the card. Measured on a 16 GB card at 32k: about 15 GB used, 40 tokens a second.
    LoopedMoe,
}

impl Preset {
    /// The preset for a model file: the catalog's, else by its name.
    pub fn for_model(file: &Path) -> Preset {
        if let Some(m) = catalog_entry_for(file) {
            return if m.preset.as_deref() == Some("looped-moe") { Preset::LoopedMoe } else { Preset::Auto };
        }
        let name = file.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if name.contains("inkling") { Preset::LoopedMoe } else { Preset::Auto }
    }
}

/// The name the server answers to: the file name without `.gguf`.
pub fn alias(model: &Path) -> String {
    model.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "local".into())
}

/// `llama-server`'s arguments for `model` on `port`, given the free video memory (MiB) if known.
pub fn launch_args(model: &Path, preset: Preset, free_vram_mb: Option<u64>, port: u16) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-m".into(),
        model.display().to_string(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "--alias".into(),
        alias(model),
        "--jinja".into(),
        "-c".into(),
        CONTEXT.to_string(),
        "-np".into(),
        "1".into(),
    ];
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    match preset {
        Preset::Auto => a.extend(s(&["-fa", "on", "--fit", "on"])),
        Preset::LoopedMoe => {
            // The outer layers' experts in system memory: blocks 0-3 and 20-23 when the card has
            // room, 0-5, 18-19 and 20-23 when it is tighter (something else is using it).
            let mask = if free_vram_mb.is_some_and(|f| f >= LOOPED_FAST_FREE_MB) {
                r"blk\.([0-3]|2[0-3])\.ffn_.*_exps=CPU"
            } else {
                r"blk\.([0-5]|1[8-9]|2[0-3])\.ffn_.*_exps=CPU"
            };
            a.extend(s(&["-ngl", "99", "-fa", "on", "-ctk", "q8_0", "-ctv", "q8_0", "-ub", "512", "--fit", "off", "--load-mode", "none", "-ot", mask]));
        }
    }
    a
}

// ---- Running the server --------------------------------------------------------------------

/// The server running, as kept in `server.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub pid: u32,
    pub port: u16,
    pub model: PathBuf,
    pub alias: String,
    pub server: PathBuf,
    pub log: PathBuf,
    /// The arguments it was started with.
    pub args: Vec<String>,
}

impl ServerInfo {
    /// The OpenAI-compatible address to give a backend.
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }
}

/// Whether the server is running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Status {
    Stopped,
    Running(ServerInfo),
    /// It stopped by itself: the last lines it wrote.
    Failed {
        message: String,
    },
}

/// The server this process started.
static RUNNING: Mutex<Option<(Launched, ServerInfo)>> = Mutex::new(None);
/// How the last server this process started ended, when it ended by itself.
static FAILED: Mutex<Option<String>> = Mutex::new(None);

fn state_file(engine_dir: &Path) -> PathBuf {
    engine_dir.join("server.json")
}

/// The server's status: the one this process started, else one another Takumi Anything process started
/// that still answers on its port.
pub fn status(engine_dir: &Path) -> Status {
    {
        let mut running = RUNNING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((launched, info)) = running.as_ref() {
            if launched.is_running() {
                return Status::Running(info.clone());
            }
            let message = ended_message(&info.log);
            *running = None;
            let _ = std::fs::remove_file(state_file(engine_dir));
            *FAILED.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(message.clone());
            return Status::Failed { message };
        }
    }
    if let Some(info) = read_state(engine_dir) {
        if healthy(info.port, Duration::from_secs(1)) {
            return Status::Running(info);
        }
        let _ = std::fs::remove_file(state_file(engine_dir));
    }
    match FAILED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone() {
        Some(message) => Status::Failed { message },
        None => Status::Stopped,
    }
}

fn read_state(engine_dir: &Path) -> Option<ServerInfo> {
    serde_json::from_str(&std::fs::read_to_string(state_file(engine_dir)).ok()?).ok()
}

/// The last lines of the server's log, to say why it stopped.
fn ended_message(log: &Path) -> String {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = lines.get(lines.len().saturating_sub(6)..).unwrap_or_default().join("\n");
    if tail.is_empty() { "llama-server stopped.".into() } else { tail }
}

/// Whether llama-server on `port` says it is ready (it answers 503 while the model loads).
fn healthy(port: u16, timeout: Duration) -> bool {
    let agent = ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(timeout)).build().new_agent();
    agent.get(&format!("http://127.0.0.1:{port}/health")).call().is_ok_and(|r| r.status().as_u16() == 200)
}

/// A port on 127.0.0.1 nothing is listening on.
fn free_port() -> Result<u16, ToolError> {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|e| ToolError::io("127.0.0.1", e))?;
    l.local_addr().map(|a| a.port()).map_err(|e| ToolError::io("127.0.0.1", e))
}

/// Longest wait for a model to load (a big one from a slow disk).
const LOAD_WAIT: Duration = Duration::from_secs(15 * 60);

/// Start `server` with `model` (stopping any server running first) and wait until it is ready.
/// The free video memory is read just before, for the preset. `report` gets short lines.
pub fn start(server: &Path, model: &Path, engine_dir: &Path, cancel: Option<Cancel>, report: &dyn Fn(&str)) -> Result<ServerInfo, ToolError> {
    if !server.is_file() {
        return Err(ToolError::new(ErrorCode::EngineMissing, format!("llama-server isn't at {}.", server.display()))
            .with_remediation("Install the engine in Settings → AI → Local model."));
    }
    if !model.is_file() {
        return Err(ToolError::invalid(format!("There's no model file at {}.", model.display())).with_remediation("Choose the .gguf file again."));
    }
    let entry = catalog_entry_for(model);
    stop(engine_dir);
    *FAILED.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    std::fs::create_dir_all(engine_dir).map_err(|e| ToolError::io(engine_dir.display(), e))?;
    let free = gpus().first().map(|g| g.free_mb);
    let preset = Preset::for_model(model);
    let port = free_port()?;
    let mut args = launch_args(model, preset, free, port);
    if let Some(sampling) = entry.and_then(|m| m.sampling) {
        args.extend(sampling.args());
    }
    let log = engine_dir.join("llama-server.log");
    report(&format!("starting llama-server on port {port}"));
    let mut launch = Launch::new(server).args(&args).hide_console().log_to(&log).label("llama-server");
    if let Some(dir) = server.parent() {
        launch = launch.cwd(dir);
    }
    let launched = launch.start_watched()?;
    let info = ServerInfo { pid: launched.pid(), port, model: model.to_path_buf(), alias: alias(model), server: server.to_path_buf(), log: log.clone(), args };
    let started = Instant::now();
    loop {
        if cancel.as_ref().is_some_and(|c| c()) {
            launched.stop();
            return Err(ToolError::cancelled());
        }
        if !launched.is_running() {
            return Err(ToolError::new(ErrorCode::ProgramFailed, format!("llama-server stopped while loading the model:\n{}", ended_message(&log)))
                .with_remediation("A model too big for the card stops it. Try a smaller model."));
        }
        if healthy(port, Duration::from_secs(2)) {
            break;
        }
        if started.elapsed() > LOAD_WAIT {
            launched.stop();
            return Err(ToolError::new(ErrorCode::TimedOut, "The model took more than 15 minutes to load, so it was stopped."));
        }
        report(&format!("loading the model ({} s)", started.elapsed().as_secs()));
        std::thread::sleep(Duration::from_millis(500));
    }
    if let Ok(text) = serde_json::to_string_pretty(&info) {
        let _ = craft_anything_platform::fs::write_atomic(&state_file(engine_dir), text.as_bytes());
    }
    *RUNNING.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some((launched, info.clone()));
    report("ready");
    Ok(info)
}

/// Stop the server, the one this process started or one recorded by another Takumi Anything process.
/// Returns whether one was running.
pub fn stop(engine_dir: &Path) -> bool {
    let mine = RUNNING.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
    let mut stopped = false;
    if let Some((launched, _)) = mine {
        stopped = launched.is_running();
        launched.stop();
    } else if let Some(info) = read_state(engine_dir)
        && healthy(info.port, Duration::from_secs(1))
    {
        // Started by another Takumi Anything process and still answering on its port: end it by its id.
        stopped = kill_pid(info.pid);
    }
    let _ = std::fs::remove_file(state_file(engine_dir));
    stopped
}

/// Stop the server this process started, if it did (the app closing): one another Takumi Anything
/// process started is left alone.
pub fn stop_own() {
    if let Some((launched, info)) = RUNNING.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
        launched.stop();
        if let Some(dir) = info.log.parent() {
            let _ = std::fs::remove_file(state_file(dir));
        }
    }
}

fn kill_pid(pid: u32) -> bool {
    let out = if cfg!(windows) {
        Process::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).timeout(Duration::from_secs(15)).run()
    } else {
        Process::new("kill").arg(pid.to_string()).timeout(Duration::from_secs(15)).run()
    };
    out.is_ok_and(|o| o.success)
}

#[cfg(test)]
mod tests;
