//! `rebuild`: the whole study → build → check workflow, headless. Questions and approvals are
//! answered on the terminal, or automatically with `--unattended` (first choice; approvals are
//! refused, so nothing outside the project happens without a person).

use crate::{Cli, INTERRUPTED, make_engine};
use clap::Args;
use craft_anything_ai::rebuild::{self, RebuildRun, RunEvent, RunOptions};
use craft_anything_ai::{AgentEvent, AiHub, BackendKind};
use craft_anything_engine::inbox::{Answer, Request, RequestKind};
use craft_anything_engine::workspace::Workspace;
use craft_anything_evidence::ToolError;
use std::io::{BufRead as _, Write as _};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[derive(Debug, Args)]
pub struct RebuildArgs {
    /// A web address, an absolute path to an app, game or file, or `-` for none. A folder that
    /// already holds a project keeps its saved target.
    pub target: String,
    /// What to make, in your words; a folder that already holds a project gets it as a follow-up.
    pub request: String,
    /// Where the new project folder goes (default: the projects folder from settings).
    #[arg(long, conflicts_with = "project_folder")]
    pub projects_dir: Option<PathBuf>,
    /// Start the new project in this folder itself (it's added to the project list). If it
    /// already has files, --existing says what they are; if it already holds a project, its saved
    /// target, settings and stack are kept and the request becomes a follow-up.
    #[arg(long, conflicts_with = "continue_in")]
    pub project_folder: Option<PathBuf>,
    /// With --project-folder that has files and isn't a project yet: `edit` (they're the app,
    /// edited in place) or `start-from` (material to start from; the app is built in app/ beside
    /// them).
    #[arg(long, requires = "project_folder", value_parser = ["edit", "start-from"])]
    pub existing: Option<String>,
    /// Continue an existing project folder; the request becomes a follow-up message.
    #[arg(long)]
    pub continue_in: Option<PathBuf>,
    /// With --continue-in: carry on where the project's last run stopped (paused, or the app or
    /// the computer stopped); the request is ignored.
    #[arg(long, requires = "continue_in")]
    pub resume: bool,
    /// The coding agent to use (an enabled backend name from `ai list`, or an installed agent
    /// such as `Claude Code`).
    #[arg(long)]
    pub backend: Option<String>,
    /// Refinement rounds after the first build.
    #[arg(long, default_value_t = 2)]
    pub rounds: u32,
    /// Answer questions with the first choice and refuse approvals, without asking.
    #[arg(long)]
    pub unattended: bool,
    /// At your own risk: let irreversible phone changes in this project (flashing, rooting,
    /// unlocking the bootloader, wiping partitions) go ahead without asking, also with
    /// --unattended. It stays on for the project until turned off in the app's Approvals or with
    /// `call project_approvals_set --project <folder> --irreversible-device-changes false`.
    /// Other actions keep their own approvals.
    #[arg(long)]
    pub yes_irreversible: bool,
    /// What to build with: an engine (unity, unreal, godot, rust) or a stack's full name.
    /// New projects only; otherwise it's chosen from the request and target.
    #[arg(long)]
    pub build_with: Option<String>,
    /// Let the builder generate pictures and 3D models for a new game (needs a fal.ai key; paid
    /// within the project's asset budget). Change it later with `project-assets-allow`.
    #[arg(long)]
    pub generate_assets: bool,
    /// Who makes the project's pictures, 3D models and other assets first: `builder` (the
    /// connected AI, with the asset service as its fallback; the default) or `service` (only the
    /// service). Leave out to keep the project's choice; a new project follows Settings
    /// (`assets.builder_first`).
    #[arg(long, value_name = "builder|service")]
    pub assets: Option<String>,
    /// Start from one of the app's Devices starts: `android-app`, `research` (look inside an app
    /// or firmware: the target is the file, and Deep mode is needed), `root-flash` or `utilities`
    /// (the target is `-`). Your request fills the start's blank (`[what it does]`, `[model]`) or
    /// is added to it; `-` leaves it as the app's tile fills it in. Anything that changes a phone
    /// is only prepared: each step asks, and says it can't be undone. New projects only.
    #[arg(long, value_parser = ["android-app", "research", "root-flash", "utilities"], conflicts_with = "continue_in")]
    pub device: Option<String>,
}

pub fn rebuild(cli: &Cli, a: &RebuildArgs) -> u8 {
    match run(cli, a) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(e) => {
            eprintln!("rebuild: {}", e.message);
            if let Some(r) = e.remediation {
                eprintln!("  {r}");
            }
            1
        }
    }
}

/// Add the agent app this run asks for, or the first one found when no enabled AI can build.
fn ensure_agent(hub: &AiHub, requested: Option<&str>) -> Result<(), ToolError> {
    let configured = hub.backends();
    // An enabled backend can build, so a run that names none needs no agent app.
    if requested.is_none() && configured.iter().any(|b| b.enabled) {
        return Ok(());
    }
    // A name that is configured is left alone (including a backend that is off): don't add a
    // second one under the same name; the run itself refuses a backend that is off.
    if requested.is_some_and(|name| configured.iter().any(|b| b.name == name)) {
        return Ok(());
    }
    // An explicit name may be an installed agent that isn't added yet: add only that one. With no
    // name, add the first agent found.
    if let Some(d) = hub.detect().into_iter().find(|d| d.suggestion.kind == BackendKind::AgentCli && requested.is_none_or(|name| d.suggestion.name == name)) {
        eprintln!("Using {} ({}).", d.suggestion.name, d.reason);
        hub.add_backend(d.suggestion, None)?;
    }
    Ok(())
}

/// The `rebuild` command: open or make the project, start the run, and print its events until it ends.
fn run(cli: &Cli, a: &RebuildArgs) -> Result<bool, ToolError> {
    // A Devices start: its request, and what it needs, before anything is made.
    let starter = a.device.as_deref().and_then(rebuild::devices::starter);
    let request = starter.map_or_else(|| a.request.clone(), |s| s.request_with(&a.request));
    let deep_for_run = match starter {
        Some(s) => device_start_mode(s, &a.target, cli.clean_room, cli.deep_for_run())?,
        None => cli.deep_for_run(),
    };
    let engine = make_engine(None, deep_for_run)?;
    let hub = AiHub::new(engine.clone());
    // Before the build starts, while no session lock is held (#1225).
    if let Err(e) = hub.ensure_session_id() {
        log::warn!("couldn't save the OpenCode Go session id: {e}");
    }
    if a.build_with.is_some() && a.continue_in.is_some() {
        return Err(ToolError::invalid("--build-with is for new projects; a project keeps the stack it was built with.")
            .with_remediation("Leave out --build-with, or start a new project."));
    }
    if a.generate_assets && a.continue_in.is_some() {
        return Err(ToolError::invalid("--generate-assets is for new projects.").with_remediation("Use project-assets-allow for an existing project."));
    }
    ensure_agent(&hub, a.backend.as_deref())?;
    // Refuse an unknown or switched-off name before opening or making a project: a command that
    // can't build must not leave a new folder behind.
    if let Some(name) = a.backend.as_deref() {
        rebuild::builder_backend(&hub.config(), Some(name))?;
    }
    // The same for a Clean room the builder can't hold: say so now, and name the AIs that can.
    if a.continue_in.is_none() {
        clean_room_preflight(&hub, engine.session().clean_room(), a.backend.as_deref())?;
    }
    let (ws, message) = match &a.continue_in {
        Some(dir) => (Workspace::open(&std::path::absolute(dir).map_err(|e| ToolError::io(dir.display(), e))?)?, (!a.resume).then(|| request.clone())),
        None => {
            use craft_anything_ai::rebuild::project::Place;
            use craft_anything_engine::workspace::ExistingFiles;
            let settings = engine.session().settings.clone();
            let existing = a.existing.as_deref().map(|e| if e == "edit" { ExistingFiles::Edit } else { ExistingFiles::StartFrom });
            let folder = a.project_folder.as_deref().map(std::path::absolute).transpose().map_err(|e| ToolError::io("--project-folder", e))?;
            let attached = folder.as_ref().is_some_and(|f| rebuild::project::has_project(f));
            if attached {
                if a.existing.is_some() {
                    return Err(ToolError::invalid("This folder already holds a project; use --continue-in to continue it."));
                }
                if a.build_with.is_some() {
                    return Err(ToolError::invalid("--build-with is for new projects; a project keeps the stack it was built with.")
                        .with_remediation("Leave out --build-with, or start a new project."));
                }
                if a.generate_assets {
                    return Err(
                        ToolError::invalid("--generate-assets is for new projects.").with_remediation("Use project-assets-allow for an existing project.")
                    );
                }
            }
            let place = match (&folder, &a.projects_dir) {
                (Some(f), _) => Place::At(f, existing),
                (None, Some(d)) => Place::In(d),
                (None, None) => Place::Projects,
            };
            let ws = rebuild::project::new_project(&settings, &a.target, &request, place, a.build_with.as_deref(), a.generate_assets, a.backend.as_deref())?;
            if folder.is_some() {
                engine.session().remember_project(&ws.root)?;
            }
            (ws, rebuild::project::attached_request(attached, &request))
        }
    };
    println!("Project: {}", ws.root.display());
    let mut ws = ws;
    if let Some(by_builder) = rebuild::project::assets_source(a.assets.as_deref())? {
        ws.set_person(|m| m.assets_by_builder = by_builder)?;
    }
    if a.yes_irreversible {
        allow_irreversible(&mut ws)?;
    }
    // The saved builder may be off or gone; say who builds once the run has started, so a failed
    // start doesn't claim an AI that never ran.
    let replacement = rebuild::builder_replacement(&hub.config(), &ws, a.backend.as_deref());
    // A follow-up shows in the project's feed as the person typed it.
    let shown = message.clone();
    let good_enough = engine.session().settings.build.good_enough;
    let opts = RunOptions {
        backend: a.backend.clone(),
        message,
        shown,
        continuing: a.resume,
        max_rounds: a.rounds,
        good_enough,
        beginner: !cli.json,
        clean_room: Some(engine.session().clean_room()),
        ..RunOptions::default()
    };
    let mut run = RebuildRun::start(&hub, ws, opts)?;
    if let Some(replacement) = replacement {
        eprintln!("{}", rebuild::replaced_builder_notice(&replacement.saved, &replacement.used));
    }
    let mut ok = false;
    let mut line_open = false;
    loop {
        if INTERRUPTED.load(Ordering::SeqCst) {
            run.stop();
        }
        for e in run.poll() {
            match e {
                RunEvent::Step { step, text } => {
                    end_line(&mut line_open);
                    println!("[{}] {text}", step.label());
                }
                RunEvent::Agent(AgentEvent::TextDelta { text }) => {
                    print!("{text}");
                    line_open = !text.ends_with('\n');
                }
                RunEvent::Agent(AgentEvent::ToolStarted { tool, input, .. }) => {
                    end_line(&mut line_open);
                    println!("  · {}", rebuild::activity::describe(&tool, &input, &[]).words());
                }
                RunEvent::Agent(AgentEvent::Error { message, remediation, log }) => {
                    end_line(&mut line_open);
                    println!("  ! {message}");
                    if let Some(r) = remediation {
                        println!("{r}");
                    }
                    if let Some(l) = log {
                        println!("  log: {}", l.display());
                    }
                }
                RunEvent::Agent(_) => {}
                RunEvent::Note { shown, .. } => {
                    end_line(&mut line_open);
                    println!("  + {shown}");
                }
                RunEvent::Ask(r) => {
                    end_line(&mut line_open);
                    let answer = if a.unattended { auto_answer(&r) } else { prompt_answer(&r) };
                    run.answer(&r.id, &answer)?;
                }
                RunEvent::Checked(c) => {
                    end_line(&mut line_open);
                    match c.similarity {
                        Some(s) => println!("[Check {}] {s:.0}% alike — {}", c.round, c.note),
                        None => println!("[Check {}] {}", c.round, c.note),
                    }
                }
                RunEvent::Preview(url) => println!("Preview: {url}"),
                RunEvent::Finished { ok: o, message, paused } => {
                    end_line(&mut line_open);
                    println!(
                        "{}",
                        if o {
                            format!("Done. {message}")
                        } else if paused {
                            message
                        } else {
                            format!("Stopped: {message}")
                        }
                    );
                    ok = o;
                }
            }
        }
        if !run.is_running() && run.poll().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = std::io::stdout().flush();
    Ok(ok)
}

/// What a Devices start needs: looking inside a file wants that file as the target and Deep mode
/// (so `--clean-room` is refused, as the app's tile turns Deep on); the others start from nothing.
/// Returns the mode the run asks for.
fn device_start_mode(s: &rebuild::devices::Starter, target: &str, clean_room: bool, asked: Option<bool>) -> Result<Option<bool>, ToolError> {
    let none = target.trim() == "-";
    if s.looks_inside {
        if none {
            return Err(ToolError::invalid(format!("--device {} looks inside a file: the target is the app or firmware image (a full path).", s.id))
                .with_remediation("Pass the path to the .apk, .aab or firmware image as the target."));
        }
        if clean_room {
            return Err(ToolError::new(
                craft_anything_evidence::ErrorCode::BadSettings,
                "Looking inside an app or firmware reads its code and files, which a Clean room doesn't do.",
            )
            .with_remediation("Leave out --clean-room (or add --deep) to look inside."));
        }
        return Ok(Some(true));
    }
    if !none {
        return Err(ToolError::invalid(format!("--device {} starts from nothing: the target is -.", s.id))
            .with_remediation("Pass - as the target, or leave out --device to work from a target."));
    }
    Ok(asked)
}

/// A Clean room can't hold a builder that reads files without asking (Kimi Code, Grok Build, Aider): refuse
/// it before anything is made, naming what can build there, as the app's switch offer does: the
/// local model first, then any other AI that holds a Clean room.
fn clean_room_preflight(hub: &AiHub, clean_room: bool, named: Option<&str>) -> Result<(), ToolError> {
    if !clean_room {
        return Ok(());
    }
    let cfg = hub.config();
    let entry = rebuild::builder_backend(&cfg, named)?;
    if craft_anything_ai::cli_agents::holds_clean_room(&entry.config) {
        return Ok(());
    }
    let mut offers: Vec<String> = cfg.enabled().filter(|b| craft_anything_ai::cli_agents::holds_clean_room(&b.config)).map(|b| b.config.name.clone()).collect();
    for d in hub.detect() {
        let c = &d.suggestion;
        if c.kind == BackendKind::AgentCli && craft_anything_ai::cli_agents::holds_clean_room(c) && cfg.get(&c.name).is_none() && !offers.contains(&c.name) {
            offers.push(c.name.clone());
        }
    }
    Err(clean_room_refusal(&entry.config.name, offers))
}

/// The refusal for a builder a Clean room can't hold, with the switch the app offers as a command.
fn clean_room_refusal(builder: &str, mut offers: Vec<String>) -> ToolError {
    offers.sort_by_key(|n| !n.starts_with(craft_anything_ai::local_model_tools::NAME_PREFIX));
    let fix = if offers.is_empty() {
        "Turn on Deep mode (--deep), or set up an AI that can build in a Clean room (Claude Code, Codex, an API model or the local model) in Settings → AI."
            .to_string()
    } else {
        let shown: Vec<String> = offers.iter().take(3).map(|n| format!("--backend \"{n}\" --clean-room")).collect();
        format!("Turn on Deep mode (--deep), or build in a Clean room with another AI: {}.", shown.join(" or "))
    };
    ToolError::new(
        craft_anything_evidence::ErrorCode::BadSettings,
        format!("{builder} works only in Deep mode: it reads files without asking, so a Clean room can't hold it back."),
    )
    .with_remediation(fix)
    .with_details(serde_json::json!({"reason": "clean_room_builder", "builder": builder, "clean_room_builders": offers}))
}

fn end_line(open: &mut bool) {
    if *open {
        println!();
        *open = false;
    }
}

fn auto_answer(r: &Request) -> Answer {
    match r.kind {
        RequestKind::Question => {
            println!("? {} → {} (unattended)", r.title, r.choices.first().map(String::as_str).unwrap_or("no preference"));
            Answer {
                choice: r.choices.first().cloned(),
                text: r.choices.is_empty().then(|| "No preference; use your best judgement.".to_string()),
                ..Default::default()
            }
        }
        RequestKind::Approval => {
            println!("? {} → refused (unattended)", r.title);
            Answer { allow: Some(false), text: Some("Unattended run: actions outside the project aren't allowed.".into()), ..Default::default() }
        }
    }
}

fn read_line() -> String {
    let mut s = String::new();
    let _ = std::io::stdin().lock().read_line(&mut s);
    s.trim().to_string()
}

fn prompt_answer(r: &Request) -> Answer {
    println!("\n? {}", r.title);
    if !r.detail.is_empty() {
        println!("  {}", r.detail.lines().take(12).collect::<Vec<_>>().join("\n  "));
    }
    match r.kind {
        RequestKind::Question => {
            for (i, c) in r.choices.iter().enumerate() {
                println!("  {}) {c}", i + 1);
            }
            print!("Your answer (number or text): ");
            let _ = std::io::stdout().flush();
            let line = read_line();
            match line.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).and_then(|i| r.choices.get(i)) {
                Some(c) => Answer { choice: Some(c.clone()), ..Default::default() },
                None => Answer { text: Some(line), ..Default::default() },
            }
        }
        RequestKind::Approval if is_device_change(r) => {
            println!("  ! {DEVICE_WARNING}");
            print!("Type yes to change the phone (anything else says no): ");
            let _ = std::io::stdout().flush();
            device_answer(&read_line())
        }
        RequestKind::Approval => {
            print!("Allow? [y]es / [a]lways for this project / [n]o: ");
            let _ = std::io::stdout().flush();
            let line = read_line().to_lowercase();
            Answer { allow: Some(line.starts_with('y') || line.starts_with('a')), always: line.starts_with('a'), ..Default::default() }
        }
    }
}

/// `--yes-irreversible`: says what it means, the same words as the app's warning, and turns it on
/// for the project.
fn allow_irreversible(ws: &mut craft_anything_engine::workspace::Workspace) -> Result<(), ToolError> {
    eprintln!("{}", irreversible_notice());
    ws.set_person(|m| m.irreversible_device_changes = true)
}

fn irreversible_notice() -> String {
    format!(
        "! --yes-irreversible: {} It stays on for this project until you turn it off in the app's Approvals.",
        craft_anything_engine::tools::project_tools::IRREVERSIBLE_DEVICE_WARNING
    )
}

/// What the app's card says before a phone is changed.
const DEVICE_WARNING: &str = "This can't be undone. It changes the connected phone: flashing, unlocking or wiping can erase everything on it or stop it starting. Check the device and the steps first.";

/// An approval for changing a connected phone or device (flashing, unlocking, wiping, sideloading).
fn is_device_change(r: &Request) -> bool {
    r.kind == RequestKind::Approval && r.action.as_deref() == Some("device")
}

/// The answer to a phone change: only the whole word "yes" allows it, once. There is no "always":
/// the next change asks again, as it does in the app.
fn device_answer(line: &str) -> Answer {
    Answer { allow: Some(line.trim().eq_ignore_ascii_case("yes")), always: false, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_anything_evidence::ErrorCode;

    fn approval(action: Option<&str>) -> Request {
        Request {
            id: "r1".into(),
            kind: RequestKind::Approval,
            title: "Flash the boot image".into(),
            detail: String::new(),
            choices: Vec::new(),
            allow_text: false,
            action: action.map(String::from),
            created_ms: 0,
        }
    }

    #[test]
    fn a_phone_change_needs_the_whole_word_yes_and_is_never_remembered() {
        assert!(is_device_change(&approval(Some("device"))));
        assert!(!is_device_change(&approval(Some("publish"))) && !is_device_change(&approval(None)));
        for line in ["yes", " YES ", "Yes"] {
            let a = device_answer(line);
            assert_eq!((a.allow, a.always), (Some(true), false), "{line}");
        }
        // "y", "a" (always), "ok", an empty line: none of them is a yes to changing a phone.
        for line in ["y", "a", "always", "ok", "", "yes please", "n"] {
            let a = device_answer(line);
            assert_eq!((a.allow, a.always), (Some(false), false), "{line:?}");
        }
    }

    #[test]
    fn yes_irreversible_warns_and_turns_it_on_for_the_project() {
        let note = irreversible_notice();
        assert!(note.contains("none of them can be undone"), "{note}");
        assert!(note.contains("at your own risk") && note.contains("flashing") && note.contains("bootloader"), "{note}");
        let tmp = tempfile::tempdir().unwrap();
        let mut ws =
            craft_anything_engine::workspace::Workspace::create(tmp.path(), "phone", "root it", craft_anything_engine::workspace::TargetRef::None).unwrap();
        assert!(!ws.manifest.irreversible_device_changes, "off by default");
        allow_irreversible(&mut ws).unwrap();
        let again = craft_anything_engine::workspace::Workspace::open(&ws.root).unwrap();
        assert!(again.manifest.irreversible_device_changes, "saved with the project");
    }

    #[test]
    fn an_unattended_run_refuses_a_phone_change() {
        let a = auto_answer(&approval(Some("device")));
        assert_eq!((a.allow, a.always), (Some(false), false));
    }

    #[test]
    fn a_device_start_says_what_it_needs() {
        let start = |id| rebuild::devices::starter(id).unwrap();
        // Looking inside wants a file and Deep mode, and refuses a Clean room.
        let look = start("research");
        assert_eq!(device_start_mode(look, "/tmp/app.apk", false, None).unwrap(), Some(true));
        assert!(device_start_mode(look, "-", false, None).is_err(), "no file to look inside");
        let e = device_start_mode(look, "/tmp/app.apk", true, Some(false)).unwrap_err();
        assert_eq!(e.code, ErrorCode::BadSettings);
        assert!(e.remediation.as_deref().is_some_and(|r| r.contains("--deep")), "{e:?}");
        // The others start from nothing, in whatever mode was asked.
        let plan = start("root-flash");
        assert_eq!(device_start_mode(plan, "-", false, None).unwrap(), None);
        assert_eq!(device_start_mode(plan, "-", true, Some(false)).unwrap(), Some(false));
        assert!(device_start_mode(plan, "https://example.com", false, None).is_err());
    }

    #[test]
    fn a_clean_room_the_builder_cannot_hold_offers_the_ais_that_can() {
        let e = clean_room_refusal("Kimi Code CLI", vec!["Claude Code".into(), "Local: Inkling".into()]);
        assert!(e.message.contains("Kimi Code CLI works only in Deep mode"), "{}", e.message);
        let fix = e.remediation.unwrap();
        // The local model comes first (nothing leaves the computer), and the fix is a command.
        assert!(fix.contains("--deep") && fix.find("Local: Inkling") < fix.find("Claude Code"), "{fix}");
        assert!(fix.contains("--backend \"Local: Inkling\" --clean-room"), "{fix}");
        let none = clean_room_refusal("Kimi Code CLI", Vec::new());
        assert!(none.remediation.unwrap().contains("Settings → AI"));
    }
}
