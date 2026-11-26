//! Settings → Local AI: run a model on this computer with llama.cpp, in three steps: install
//! Takumi Anything's own pinned engine, choose a model (a .gguf file where it is, or one from the catalog,
//! marked by whether it fits the graphics card), and Start / Stop with a status line. Starting it
//! adds the `Local: <model>` AI that builds like any other, in Clean room too; Settings → AI shows
//! it as one more AI. Launch flags and sampling come from the catalog preset and are not shown.
//!
//! Everything goes through the `ai_local_*` tools, as from the CLI, except a model download: it
//! can take an hour, so it runs on its own worker rather than holding the session that whole time.

use crate::engine_ui::progress_bar;
use crate::i18n::{tr, trf};
use crate::theme::{self, Tokens};
use crate::{CraftApp, Done, Pick, widgets};
use craft_anything_engine::Progress;
use craft_anything_engines::local_llm::{self, CatalogModel, Category, Fit};
use egui::{RichText, Ui};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often the card looks again while it is shown (the server may have stopped by itself).
const LOOK_EVERY: Duration = Duration::from_secs(10);

/// The numbered circle before each step's title.
const STEP_BADGE: f32 = theme::icon::LG;

/// Where the person is with the local model.
#[derive(Debug, Default)]
pub struct LocalFlow {
    /// The last `ai_local_status` answer.
    pub status: Option<Value>,
    pub looked_at: Option<Instant>,
    pub looking: Option<u64>,
    /// The engine install, the server start or stop, as a job.
    pub install: Option<u64>,
    pub starting: Option<u64>,
    /// The catalog download running: its job and model id.
    pub download: Option<(u64, String)>,
    /// A download asked for before the models folder was chosen.
    pub pending_download: Option<String>,
    /// The catalog tab shown.
    pub tab: usize,
    /// The model Start runs.
    pub selected: Option<PathBuf>,
    /// The catalog shown though a model is already there ("Download another model").
    pub catalog_open: bool,
}

impl CraftApp {
    /// Ask what's set up (in the background).
    pub fn local_look(&mut self) {
        if self.headless || self.local.looking.is_some() {
            return;
        }
        self.local.looking = Some(self.act("ai_local_status", json!({}), tr("Look at the local model")));
    }

    fn local_busy(&self) -> bool {
        self.local.install.is_some() || self.local.starting.is_some()
    }

    pub fn local_install_engine(&mut self) {
        if !self.local_busy() {
            self.local.install = Some(self.act("ai_local_engine_install", json!({}), tr("Installing llama.cpp")));
        }
    }

    pub fn local_start(&mut self, model: PathBuf) {
        if !self.local_busy() {
            self.local.starting = Some(self.act("ai_local_start", json!({ "model": model }), tr("Starting the local model")));
        }
    }

    pub fn local_stop(&mut self) {
        self.act("ai_local_stop", json!({}), tr("Stopping the local model"));
    }

    /// Download a catalog model, asking first where models go if that was never answered.
    pub fn local_download(&mut self, id: String) {
        if self.local.download.is_some() {
            return;
        }
        if !self.settings.local_model.models_dir_asked {
            self.local.pending_download = Some(id);
            return;
        }
        let Some(model) = local_llm::catalog().iter().find(|m| m.id == id).cloned() else { return };
        let dir = self.settings.local_models_dir();
        let title = trf("Downloading {name}", &[("name", &model.name)]);
        let job = self.jobs.spawn(title, true, move |j| {
            let token = j.cancel.clone();
            let cancel: craft_anything_platform::process::Cancel = Arc::new(move || token.is_cancelled());
            let report = j.reporter();
            let mb = |n: u64| n / 1_000_000;
            let progress = |got: u64, total: Option<u64>| {
                let fraction = total.filter(|t| *t > 0).map(|t| got as f32 / t as f32);
                report(Progress { fraction, message: format!("{}: {} / {} MB", model.name, mb(got), total.map_or("?".into(), |t| mb(t).to_string())) });
            };
            let file = local_llm::download_model(&model, &dir, Some(cancel), &progress).map_err(|e| e.for_people())?;
            Ok(Done::LocalDownloaded { file })
        });
        self.local.download = Some((job, id));
    }

    /// The models folder was answered: the standard one (`None`) or one chosen.
    pub fn local_models_dir_chosen(&mut self, dir: Option<PathBuf>) {
        self.settings.local_model.models_dir = dir;
        self.settings.local_model.models_dir_asked = true;
        self.settings_changed();
        if let Some(id) = self.local.pending_download.take() {
            self.local_download(id);
        }
    }

    /// A model file was chosen: linked where it is and picked for Start.
    pub fn local_file_chosen(&mut self, file: PathBuf) {
        if !self.settings.local_model.files.contains(&file) {
            self.settings.local_model.files.push(file.clone());
            self.settings_changed();
        }
        self.local.selected = Some(file);
    }

    pub fn local_downloaded(&mut self, file: PathBuf) {
        self.local.download = None;
        self.toast(trf("{name} is downloaded.", &[("name", &local_llm::alias(&file))]), false);
        // Picked for Start only when nothing runs: a finished download never swaps the model
        // under one that's running.
        let running = self.local.status.as_ref().is_some_and(|s| s["server"]["state"].as_str() == Some("running"));
        if !running || self.local.selected.is_none() {
            self.local.selected = Some(file);
        }
        self.local_look();
    }

    /// A local-model tool finished.
    pub fn local_acted(&mut self, job: u64, tool: &str, result: Result<Value, craft_anything_engine::evidence::ToolError>) {
        let f = &mut self.local;
        for slot in [&mut f.looking, &mut f.install, &mut f.starting] {
            if *slot == Some(job) {
                *slot = None;
            }
        }
        match (tool, result) {
            ("ai_local_status", Ok(v)) => {
                self.local.status = Some(v);
                self.local.looked_at = Some(Instant::now());
                return;
            }
            ("ai_local_status", Err(_)) => {
                self.local.looked_at = Some(Instant::now());
                return;
            }
            ("ai_local_engine_install", Ok(_)) => self.toast(tr("llama.cpp is installed."), false),
            ("ai_local_start", Ok(v)) => {
                let name = v["backend"].as_str().unwrap_or_default().to_string();
                self.toast(trf("{name} is running. Choose it in Built by.", &[("name", &name)]), false);
                self.refresh_backends();
            }
            ("ai_local_stop", Ok(_)) => self.toast(tr("The local model is stopped."), false),
            (_, Err(e)) if e.code == craft_anything_engine::evidence::ErrorCode::Cancelled => {}
            (_, Err(e)) => self.toast(e.for_people(), true),
            _ => {}
        }
        self.local_look();
    }

    /// Stop the server this app started, when it closes, so the card's memory is freed.
    pub fn local_stop_on_exit(&self) {
        local_llm::stop_own();
    }
}

/// The models Start can run: linked files, then downloaded catalog models.
fn model_choices(app: &CraftApp) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = app.settings.local_model.files.clone();
    let dir = app.settings.local_models_dir();
    for m in local_llm::catalog() {
        let p = dir.join(&m.file);
        if !v.contains(&p) && std::fs::metadata(&p).is_ok_and(|f| f.len() == m.size_bytes) {
            v.push(p);
        }
    }
    v
}

fn job_progress(app: &CraftApp, job: Option<u64>) -> Option<(Option<f32>, String)> {
    let j = app.jobs.running().find(|j| Some(j.id) == job)?;
    Some(j.progress().map(|p| (p.fraction, p.message)).unwrap_or((None, tr("Starting…"))))
}

fn path_of(v: &Value) -> Option<PathBuf> {
    v.as_str().filter(|s| !s.is_empty()).map(PathBuf::from)
}

fn faint(ui: &mut Ui, text: impl Into<String>) {
    let t = Tokens::get(ui.ctx());
    ui.add(egui::Label::new(RichText::new(text.into()).color(t.text_faint).size(theme::CAPTION)).wrap());
}

/// A video-memory estimate: whole gigabytes, or one decimal when it has one (9.5).
fn gb_estimate(gb: f32) -> String {
    if gb.fract().abs() < 0.05 { format!("{gb:.0}") } else { format!("{gb:.1}") }
}

fn gb(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1e9)
}

/// Settings → Local AI: three steps, top to bottom: the engine, a model, and running it.
pub fn page(app: &mut CraftApp, ui: &mut Ui) {
    // Only looked at while this tab is shown: nothing runs for someone who never opens it.
    if app.local.looking.is_none() && app.local.looked_at.is_none_or(|at| at.elapsed() > LOOK_EVERY) {
        app.local_look();
    }
    ui.ctx().request_repaint_after(LOOK_EVERY);
    faint(ui, tr("Runs a model on this computer with llama.cpp. Nothing is sent anywhere, and nothing is added to what you ask or what it answers."));
    ui.add_space(theme::space::LG);
    let status = app.local.status.clone().unwrap_or(Value::Null);
    let has_engine = path_of(&status["engine"]["server"]).is_some();
    let choices = model_choices(app);
    let running = status["server"]["state"].as_str() == Some("running");
    // What runs (or is starting) is shown first, so it's seen after Start without scrolling.
    if running || app.local.starting.is_some() {
        running_strip(app, ui, &status);
        ui.add_space(theme::space::LG);
    }

    step(ui, 1, "Install the engine", has_engine);
    indented(ui, |ui| engine_step(app, ui, &status));
    ui.add_space(theme::space::LG);

    step(ui, 2, "Choose a model", !choices.is_empty());
    indented(ui, |ui| model_step(app, ui, &status, &choices));
    ui.add_space(theme::space::LG);

    step(ui, 3, "Start it", running);
    indented(ui, |ui| server_step(app, ui, &status));
}

/// The model running (or starting) and Stop, above the steps.
fn running_strip(app: &mut CraftApp, ui: &mut Ui, status: &Value) {
    let t = Tokens::get(ui.ctx());
    widgets::panel(&t).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if let Some((fraction, message)) = job_progress(app, app.local.starting) {
            progress_bar(ui, fraction, &message);
            return;
        }
        let model = path_of(&status["server"]["model"]).map(|p| local_llm::alias(&p)).unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            widgets::pill(ui, "Running", t.success, t.raised);
            if widgets::secondary(ui, Some("circle-stop"), "Stop").clicked() {
                app.local_stop();
            }
        });
        faint(ui, trf("{model} is running. Choose it in Built by, or on the AI tab.", &[("model", &model)]));
    });
}

/// A step's heading: its number in a circle (a check once it's done) and its title.
fn step(ui: &mut Ui, n: u8, title: &str, done: bool) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        let size = STEP_BADGE;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
        let (fill, ink) = if done { (t.success, t.on_accent) } else { (t.accent_soft, t.accent_ink) };
        ui.painter().circle_filled(rect.center(), size / 2.0, fill);
        if done {
            crate::icons::paint(ui, egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(theme::icon::SM)), "check", theme::icon::SM, ink);
        } else {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, n.to_string(), theme::semibold(theme::LABEL), ink);
        }
        ui.add_space(theme::space::XS);
        ui.label(RichText::new(tr(title)).font(theme::semibold(theme::BODY)));
    });
    ui.add_space(theme::space::XS);
}

/// A step's content, lined up under its title.
fn indented(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|ui| {
        ui.add_space(STEP_BADGE + theme::space::XS + ui.spacing().item_spacing.x);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    });
}

fn engine_step(app: &mut CraftApp, ui: &mut Ui, status: &Value) {
    let t = Tokens::get(ui.ctx());
    // Only Takumi Anything's own pinned build, installed in its data folder, is ever run.
    let server = path_of(&status["engine"]["server"]);
    if let Some((fraction, message)) = job_progress(app, app.local.install) {
        progress_bar(ui, fraction, &message);
        return;
    }
    match &server {
        Some(p) => {
            ui.horizontal_wrapped(|ui| {
                widgets::pill(ui, "Installed", t.success, t.raised);
                ui.add(egui::Label::new(RichText::new(tr("llama.cpp, the official build")).color(t.text_dim).size(theme::CAPTION)));
            });
            faint(ui, crate::short_path(p));
        }
        None if status.is_null() => {
            ui.spinner();
        }
        None => {
            faint(ui, tr("llama.cpp's server. Takumi Anything downloads the official build and checks it against its published checksum."));
            ui.add_space(theme::space::XS);
            if local_llm::can_install() {
                let label = trf("Install engine ({mb} MB)", &[("mb", &local_llm::engine_download_mb().to_string())]);
                if widgets::primary_icon(ui, Some("download"), &label, true).clicked() {
                    app.local_install_engine();
                }
            } else {
                faint(ui, tr("Takumi Anything installs its llama.cpp engine only on Windows (x64) so far."));
            }
        }
    }
}

fn model_step(app: &mut CraftApp, ui: &mut Ui, status: &Value, choices: &[PathBuf]) {
    let t = Tokens::get(ui.ctx());
    if app.local.selected.as_ref().is_none_or(|s| !choices.contains(s)) {
        app.local.selected = choices.first().cloned();
    }
    ui.horizontal_wrapped(|ui| {
        if !choices.is_empty() {
            let shown = app.local.selected.as_deref().map(local_llm::alias).unwrap_or_default();
            widgets::dropdown("local-model").selected_text(shown).show_ui(ui, |ui| {
                for c in choices {
                    let r = ui.selectable_label(app.local.selected.as_ref() == Some(c), local_llm::alias(c)).on_hover_text(c.display().to_string());
                    if r.clicked() {
                        app.local.selected = Some(c.clone());
                    }
                }
            });
        }
        if widgets::secondary(ui, Some("file"), "Choose file…").on_hover_text(tr("A .gguf file you have. It stays where it is.")).clicked() {
            app.pick(Pick::LocalModelFile);
        }
    });
    if let Some(sel) = &app.local.selected {
        faint(ui, sel.display().to_string());
    }
    ui.add_space(theme::space::SM);
    // Someone with a model already sees the catalog only when they ask for it.
    let downloading = app.local.download.is_some() || app.local.pending_download.is_some();
    let shown = choices.is_empty() || downloading || app.local.catalog_open;
    if !shown {
        if widgets::link(ui, "Download another model").clicked() {
            app.local.catalog_open = true;
        }
        return;
    }
    ui.label(
        RichText::new(tr(if choices.is_empty() { "Or download one" } else { "Download another model" })).font(theme::medium(theme::SMALL)).color(t.text_dim),
    );
    ui.add_space(theme::space::XS);
    catalog(app, ui, status);
}

fn server_step(app: &mut CraftApp, ui: &mut Ui, status: &Value) {
    let t = Tokens::get(ui.ctx());
    let server = &status["server"];
    let has_engine = path_of(&status["engine"]["server"]).is_some();
    let selected = app.local.selected.clone();
    // Its progress is shown above the steps.
    if app.local.starting.is_some() {
        return;
    }
    let state = server["state"].as_str().unwrap_or("stopped");
    ui.horizontal_wrapped(|ui| {
        match state {
            "running" => widgets::pill(ui, "Running", t.success, t.raised),
            "error" => widgets::pill(ui, "Error", t.danger, t.raised),
            _ => widgets::pill(ui, "Stopped", t.text_faint, t.raised),
        };
        let can_start = has_engine && selected.is_some();
        // A different model chosen while one runs: Start switches to it.
        let running_model = path_of(&server["model"]);
        let label = if state == "running" && running_model == selected { "Restart" } else { "Start" };
        if widgets::primary_icon(ui, Some("play"), label, can_start).clicked()
            && let Some(m) = selected.clone()
        {
            app.local_start(m);
        }
        if state == "running" && widgets::secondary(ui, Some("circle-stop"), "Stop").clicked() {
            app.local_stop();
        }
    });
    match state {
        "running" => {
            let model = path_of(&server["model"]).map(|p| local_llm::alias(&p)).unwrap_or_default();
            faint(ui, trf("{model} is running. Choose it in Built by, or on the AI tab.", &[("model", &model)]));
        }
        "error" => faint(ui, server["message"].as_str().unwrap_or_default().to_string()),
        _ if !has_engine => faint(ui, tr("Install the engine first.")),
        _ if selected.is_none() => faint(ui, tr("Choose a model file, or download one.")),
        _ => faint(ui, tr("Starting it adds it to your AIs as Local: and the model's name.")),
    }
    gpu_line(ui, status);
}

fn gpu_line(ui: &mut Ui, status: &Value) {
    let gpu = status["gpus"].as_array().and_then(|g| g.first());
    let line = match gpu {
        Some(g) => {
            let total = g["total_mb"].as_u64().unwrap_or(0);
            let free = g["free_mb"].as_u64().unwrap_or(0);
            trf(
                "Graphics card: {name}, {total} GB ({free} GB free).",
                &[
                    ("name", g["name"].as_str().unwrap_or_default()),
                    ("total", &format!("{:.0}", total as f64 / 1024.0)),
                    ("free", &format!("{:.1}", free as f64 / 1024.0)),
                ],
            )
        }
        None if status.is_null() => return,
        None => tr("No NVIDIA graphics card was found. Models run on the processor, much more slowly."),
    };
    faint(ui, line);
}

fn catalog(app: &mut CraftApp, ui: &mut Ui, status: &Value) {
    let t = Tokens::get(ui.ctx());
    let vram = status["gpus"].as_array().and_then(|g| g.first()).and_then(|g| g["total_mb"].as_u64());
    if app.local.pending_download.is_some() {
        // Asked once, the first time something is downloaded.
        widgets::card(&t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(tr("Where should downloaded models go?")).font(theme::medium(theme::BODY)));
            faint(
                ui,
                trf(
                    "The standard folder is {dir}. Models are big: pick a drive with room.",
                    &[("dir", &app.settings.local_models_dir().display().to_string())],
                ),
            );
            ui.horizontal_wrapped(|ui| {
                if widgets::primary_icon(ui, Some("check"), "Use the standard folder", true).clicked() {
                    app.local_models_dir_chosen(None);
                }
                if widgets::secondary(ui, Some("folder-open"), "Choose a folder…").clicked() {
                    app.pick(Pick::LocalModelsDir);
                }
                if widgets::ghost(ui, None, "Cancel").clicked() {
                    app.local.pending_download = None;
                }
            });
        });
        ui.add_space(theme::space::SM);
    }
    if let Some(i) = widgets::segmented(ui, &["Uncensored", "Classic"], app.local.tab, theme::ControlSize::Medium) {
        app.local.tab = i;
    }
    ui.add_space(theme::space::SM);
    let category = if app.local.tab == 0 { Category::Uncensored } else { Category::Classic };
    let dir = app.settings.local_models_dir();
    let downloading = app.local.download.clone();
    for m in local_llm::catalog().iter().filter(|m| m.category == category) {
        model_card(app, ui, m, vram, &dir, downloading.as_ref());
        ui.add_space(theme::space::XS);
    }
    if app.settings.local_model.models_dir_asked {
        ui.horizontal_wrapped(|ui| {
            faint(ui, trf("Downloads go to {dir}", &[("dir", &dir.display().to_string())]));
            if widgets::link(ui, "Change…").clicked() {
                app.pick(Pick::LocalModelsDir);
            }
        });
    }
}

fn model_card(app: &mut CraftApp, ui: &mut Ui, m: &CatalogModel, vram: Option<u64>, dir: &Path, downloading: Option<&(u64, String)>) {
    let t = Tokens::get(ui.ctx());
    let file = dir.join(&m.file);
    let have = std::fs::metadata(&file).is_ok_and(|f| f.len() == m.size_bytes);
    let fit = vram.map(|v| m.fit(v));
    widgets::panel(&t).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(&m.name).font(theme::medium(theme::BODY)));
            match fit {
                Some(Fit::Fits) => widgets::pill(ui, "Fits", t.success, t.raised),
                Some(Fit::Partly) => widgets::pill(ui, "Fits with part in system memory", t.warning, t.raised),
                Some(Fit::No) => widgets::pill(ui, "Won't fit", t.danger, t.raised),
                None => widgets::pill(ui, &trf("Needs {gb} GB", &[("gb", &gb_estimate(m.vram_gb))]), t.text_faint, t.raised),
            };
        });
        faint(
            ui,
            trf(
                "{tier} GB tier · {size} GB file · about {vram} GB of video memory with a 32k context · {license}",
                &[("tier", &m.tier_gb.to_string()), ("size", &gb(m.size_bytes)), ("vram", &gb_estimate(m.vram_gb)), ("license", &m.license)],
            ),
        );
        faint(ui, tr(&m.note));
        ui.horizontal_wrapped(|ui| {
            if let Some((job, _)) = downloading.filter(|(_, id)| *id == m.id) {
                let (fraction, message) = job_progress(app, Some(*job)).unwrap_or((None, tr("Starting…")));
                ui.vertical(|ui| {
                    progress_bar(ui, fraction, &message);
                    if widgets::secondary(ui, None, "Pause").on_hover_text(tr("It carries on from here next time.")).clicked() {
                        app.jobs.cancel(*job);
                        app.local.download = None;
                    }
                });
            } else if have {
                widgets::pill(ui, "Downloaded", t.success, t.raised).on_hover_text(file.display().to_string());
                if widgets::secondary(ui, Some("check"), "Use").clicked() {
                    app.local.selected = Some(file.clone());
                }
            } else if widgets::secondary(ui, Some("download"), "Download").clicked() && downloading.is_none() {
                app.local_download(m.id.clone());
            }
            if widgets::link(ui, "Hugging Face page").clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(m.page()));
            }
        });
    });
}
