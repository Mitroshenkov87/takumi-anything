//! A model run on this computer with llama.cpp (`craft_anything_engines::local_llm`) as tools, so
//! the app's Local model card, the CLI and an agent all do it the same way: see what is here,
//! install the engine, download a catalog model, start and stop the server. Starting it adds (or
//! updates) an OpenAI-compatible backend named `Local: <model>`, which builds like any other AI
//! and in Clean room too: Takumi Anything's own tools do all the reading.

use crate::{AiConfig, BackendConfig, BackendEntry, BackendKind, BackendOptions};
use craft_anything_engine::Settings;
use craft_anything_engine::tool::{Ctx, Effects, Requires, ToolOutput, ToolSpec, Toolset, parse_params, schema_of};
use craft_anything_engines::local_llm::{self, ServerInfo, Status};
use craft_anything_evidence::{ErrorCode, ToolError};
use craft_anything_platform::process::Cancel;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What every local backend's name starts with.
pub const NAME_PREFIX: &str = "Local: ";

pub fn specs() -> Vec<ToolSpec> {
    let spec = |id, title, summary, beginner, effects, input_schema, run| ToolSpec {
        id,
        title,
        summary,
        beginner,
        toolset: Toolset::Setup,
        effects,
        requires: Requires::Nothing,
        input_schema,
        run,
    };
    vec![
        spec(
            "ai_local_status",
            "Show the local model setup",
            "The local-model engine (Takumi Anything's own pinned llama.cpp build in its data folder, installed or not), the graphics cards and their memory, the models folder, the catalog of models to download with how each fits this card, the linked model files, and whether the server is running (its address and model).",
            "Shows what's set up for running a model on this computer.",
            Effects::READ_ONLY,
            schema_of::<NoParams>,
            ai_local_status,
        ),
        spec(
            "ai_local_engine_install",
            "Install the local model engine",
            "Download llama.cpp's official Windows CUDA release and its CUDA runtime (pinned, each checked against its published SHA-256) into Takumi Anything's local engine folder.",
            "Installs the engine that runs models on this computer.",
            Effects::ACCESSES_NETWORK.union(Effects::WRITES_FILESYSTEM),
            schema_of::<NoParams>,
            ai_local_engine_install,
        ),
        spec(
            "ai_local_model_download",
            "Download a local model",
            "Download a catalog model (`id` from ai_local_status) from Hugging Face into the models folder. It carries on from where an earlier try stopped, and its size and SHA-256 are checked.",
            "Downloads a model to run on this computer.",
            Effects::ACCESSES_NETWORK.union(Effects::WRITES_FILESYSTEM),
            schema_of::<DownloadParams>,
            ai_local_model_download,
        ),
        spec(
            "ai_local_start",
            "Start a local model",
            "Run llama-server on 127.0.0.1 (a free port, tool-call templates on) with `model` (a .gguf path) or catalog `id`, stopping any local model already running, and wait until it's loaded. Its layout between the card and system memory is chosen for the model. Then the backend `Local: <model>` points at it.",
            "Starts a model on this computer and adds it to your AIs.",
            Effects::LAUNCHES_PROCESS.union(Effects::WRITES_FILESYSTEM).union(Effects::MUTATES_SESSION),
            schema_of::<StartParams>,
            ai_local_start,
        ),
        spec(
            "ai_local_stop",
            "Stop the local model",
            "Stop the llama-server Takumi Anything started, freeing the graphics card.",
            "Stops the model running on this computer.",
            Effects::LAUNCHES_PROCESS,
            schema_of::<NoParams>,
            ai_local_stop,
        ),
    ]
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoParams {}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DownloadParams {
    /// The catalog model's id (`uncensored-16`, `classic-24`…).
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StartParams {
    /// A .gguf model file, used where it is.
    #[serde(default)]
    pub model: Option<PathBuf>,
    /// A catalog model's id, downloaded into the models folder already.
    #[serde(default)]
    pub id: Option<String>,
}

fn cancel_of(ctx: &Ctx<'_>) -> Cancel {
    let c = ctx.cancel.clone();
    Arc::new(move || c.is_cancelled())
}

fn mb(n: u64) -> u64 {
    n / 1_000_000
}

/// The server to run, or why there is none.
fn server(settings: &Settings) -> Result<PathBuf, ToolError> {
    local_llm::managed_server(&settings.local_engine_dir()).ok_or_else(|| {
        ToolError::new(ErrorCode::EngineMissing, "The local model engine (llama.cpp) isn't installed.")
            .with_remediation("Install it in Settings → AI → Local model (ai_local_engine_install).")
    })
}

/// The status as the tools report it.
pub fn status_json(status: &Status) -> Value {
    match status {
        Status::Running(info) => {
            json!({"state": "running", "port": info.port, "base_url": info.base_url(), "model": info.model, "pid": info.pid, "backend": backend_name(&info.model)})
        }
        Status::Stopped => json!({"state": "stopped"}),
        Status::Failed { message } => json!({"state": "error", "message": message}),
    }
}

fn ai_local_status(ctx: &mut Ctx<'_>, _params: Value) -> Result<ToolOutput, ToolError> {
    let s = &ctx.session.settings;
    let engine_dir = s.local_engine_dir();
    let models_dir = s.local_models_dir();
    let gpus = local_llm::gpus();
    let vram = gpus.first().map(|g| g.total_mb);
    let catalog: Vec<Value> = local_llm::catalog()
        .iter()
        .map(|m| {
            let file = models_dir.join(&m.file);
            json!({
                "id": m.id, "category": m.category, "tier_gb": m.tier_gb, "name": m.name, "page": m.page(), "file": m.file,
                "size_mb": mb(m.size_bytes), "vram_gb": m.vram_gb, "fit": vram.map(|v| m.fit(v)), "note": m.note, "license": m.license,
                "downloaded": std::fs::metadata(&file).is_ok_and(|f| f.len() == m.size_bytes).then(|| file.display().to_string()),
            })
        })
        .collect();
    Ok(ToolOutput::Value(json!({
        "engine": {
            "server": local_llm::managed_server(&engine_dir),
            "folder": engine_dir,
            "can_install": local_llm::can_install(),
            "build": local_llm::BUILD,
            "download_mb": local_llm::engine_download_mb(),
        },
        "gpus": gpus,
        "models_dir": models_dir,
        "linked": s.local_model.files,
        "catalog": catalog,
        "server": status_json(&local_llm::status(&engine_dir)),
    })))
}

fn ai_local_engine_install(ctx: &mut Ctx<'_>, _params: Value) -> Result<ToolOutput, ToolError> {
    let dir = ctx.session.settings.local_engine_dir();
    let cancel = cancel_of(ctx);
    let progress = |what: &str, got: u64, total: Option<u64>| {
        let fraction = total.filter(|t| *t > 0).map(|t| got as f32 / t as f32);
        ctx.report(fraction, format!("{what}: {} / {} MB", mb(got), total.map_or("?".into(), |t| mb(t).to_string())));
    };
    let server = local_llm::install_engine(&dir, Some(cancel), &progress)?;
    Ok(ToolOutput::Value(json!({"installed": local_llm::BUILD, "server": server})))
}

fn ai_local_model_download(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: DownloadParams = parse_params(params)?;
    let model = catalog_model(&p.id)?;
    let dir = ctx.session.settings.local_models_dir();
    let cancel = cancel_of(ctx);
    let progress = |got: u64, total: Option<u64>| {
        let fraction = total.filter(|t| *t > 0).map(|t| got as f32 / t as f32);
        ctx.report(fraction, format!("{}: {} of {} MB", model.name, mb(got), total.map_or("?".into(), |t| mb(t).to_string())));
    };
    let file = local_llm::download_model(model, &dir, Some(cancel), &progress)?;
    Ok(ToolOutput::Value(json!({"id": model.id, "file": file})))
}

fn catalog_model(id: &str) -> Result<&'static local_llm::CatalogModel, ToolError> {
    local_llm::catalog().iter().find(|m| m.id == id.trim()).ok_or_else(|| {
        let ids: Vec<&str> = local_llm::catalog().iter().map(|m| m.id.as_str()).collect();
        ToolError::invalid(format!("There's no catalog model {id:?}.")).with_remediation(format!("Use one of: {}.", ids.join(", ")))
    })
}

fn ai_local_start(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: StartParams = parse_params(params)?;
    let model = match (p.model, p.id) {
        (Some(m), _) => m,
        (None, Some(id)) => ctx.session.settings.local_models_dir().join(&catalog_model(&id)?.file),
        (None, None) => return Err(ToolError::invalid("Say which model: `model` (a .gguf file) or a catalog `id`.")),
    };
    let server = server(&ctx.session.settings)?;
    let dir = ctx.session.settings.local_engine_dir();
    let cancel = cancel_of(ctx);
    let report = |m: &str| ctx.report(None, m.to_string());
    let info = local_llm::start(&server, &model, &dir, Some(cancel), &report)?;
    let name = register(ctx, &info)?;
    Ok(ToolOutput::Value(json!({"backend": name, "base_url": info.base_url(), "model": info.model, "port": info.port, "args": info.args})))
}

/// The backend name for a model file.
pub fn backend_name(model: &Path) -> String {
    format!("{NAME_PREFIX}{}", local_llm::alias(model))
}

/// Point the backend for this model at the running server, in place of any other local one (one
/// server runs at a time); roles that used another local backend move to this one.
fn register(ctx: &mut Ctx<'_>, info: &ServerInfo) -> Result<String, ToolError> {
    let name = backend_name(&info.model);
    let mut config = AiConfig::from_table(&ctx.session.settings.ai)?;
    let old: Vec<String> = config.backends.iter().map(|b| b.config.name.clone()).filter(|n| n.starts_with(NAME_PREFIX) && *n != name).collect();
    let options = config.get(&name).map(|e| e.options.clone()).unwrap_or_else(BackendOptions::default);
    for n in &old {
        config.remove(n);
    }
    for role in config.roles.values_mut() {
        if old.contains(role) {
            *role = name.clone();
        }
    }
    config.fallback_chain.retain(|n| !old.contains(n));
    let cfg = BackendConfig {
        name: name.clone(),
        kind: BackendKind::OpenaiCompatible,
        model: info.alias.clone(),
        base_url: Some(info.base_url()),
        ssh_host: None,
        enabled: true,
    };
    config.upsert(BackendEntry { config: cfg, options });
    let mut settings = ctx.session.settings.clone();
    settings.ai = config.to_table()?;
    settings.save_to(&ctx.session.config_dir().join("config.toml"))?;
    ctx.session.settings = settings;
    Ok(name)
}

fn ai_local_stop(ctx: &mut Ctx<'_>, _params: Value) -> Result<ToolOutput, ToolError> {
    let stopped = local_llm::stop(&ctx.session.settings.local_engine_dir());
    Ok(ToolOutput::Value(json!({"stopped": stopped})))
}
