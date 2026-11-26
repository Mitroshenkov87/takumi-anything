//! Starting a project from a request: the target, the folder, the stack the person asked for, the
//! asset budget from settings and whether assets may be generated. Shared by the CLI's `rebuild` and the `rebuild_start` tool.

use super::*;
use craft_anything_engine::Settings;
use craft_anything_engine::workspace::{ExistingFiles, suggest_name, unique_name};

/// Where a new project goes.
#[derive(Debug, Clone, Copy, Default)]
pub enum Place<'a> {
    /// A new folder in the projects folder from settings.
    #[default]
    Projects,
    /// A new folder inside this one.
    In(&'a Path),
    /// This folder itself ([`Workspace::create_at`]), with what its files are if it has any.
    At(&'a Path, Option<ExistingFiles>),
}

/// A stack picked by name for `--build-with` / `build_with` (an engine id or a stack's full name).
pub fn build_with(name: &str) -> Result<&'static stacks::Stack, ToolError> {
    stacks::resolve(name).ok_or_else(|| {
        let known = stacks::catalogue().iter().map(|s| s.engine.clone().unwrap_or_else(|| s.name.clone())).collect::<Vec<_>>().join(", ");
        ToolError::invalid(format!("Unknown build-with {name:?}.")).with_remediation(format!("Use one of: {known}."))
    })
}

/// The stack a project starts with. One edited in place keeps what its files are built with,
/// unless its request names another; any other goes by its request and target.
pub(super) fn first_stack(kinds: &KindRegistry, ws: &Workspace, deep: bool) -> Option<&'static str> {
    let m = &ws.manifest;
    let own = m.app_in_root && kinds.named(&m.request, &m.target).is_none();
    let found = own.then(|| stacks::found_in(&ws.app_dir())).flatten();
    found.map(|s| s.name.as_str()).or_else(|| kinds.choose(&m.request, &m.target, deep).map(|(_, stack)| stack))
}

/// Whether `folder` already holds a project (its `craft.json` is there).
pub fn has_project(folder: &Path) -> bool {
    folder.join(craft_anything_engine::workspace::MANIFEST).is_file()
}

/// What a run says after opening a folder: a request typed for one that already held a project
/// is a follow-up, not a new project's request.
pub fn attached_request(attached: bool, request: &str) -> Option<String> {
    attached.then(|| request.to_string())
}

/// What a created project starts with: its stack, the asset budget from settings, whether its
/// pictures and models may be generated, and its builder. A folder that already held a project
/// keeps all of them; attaching never rewrites what the person set.
pub fn apply_new_project_fields(
    ws: &mut Workspace,
    attached: bool,
    stack: Option<&str>,
    budget: f64,
    generate_assets: bool,
    assets_by_builder: bool,
    builder: Option<&str>,
) -> Result<(), ToolError> {
    if attached {
        return Ok(());
    }
    if let Some(name) = stack {
        ws.manifest.stack = name.to_string();
    }
    ws.set_person(|m| {
        m.asset_budget_usd = budget;
        m.generate_assets = generate_assets;
        m.assets_by_builder = assets_by_builder;
        m.builder = builder.map(str::to_string);
    })
}

/// Who makes a project's assets first, as the person said it: `builder` (the connected AI, with
/// the asset service as its fallback) or `service` (only the service). `None` keeps the project's
/// own choice.
pub fn assets_source(word: Option<&str>) -> Result<Option<bool>, ToolError> {
    match word.map(|w| w.trim().to_ascii_lowercase()).as_deref() {
        None => Ok(None),
        Some("builder") => Ok(Some(true)),
        Some("service") => Ok(Some(false)),
        Some(other) => Err(ToolError::invalid(format!("assets must be `builder` or `service`, not {other:?}."))
            .with_remediation("`builder`: the connected AI makes assets first; `service`: only the asset service does.")),
    }
}

/// A new project for `request` about `target` (a web address, an absolute path, or `-` for none),
/// at `place`, with the stack the person
/// picked (`build_with`), the asset budget from settings, and `generate_assets`: the person lets the
/// builder generate pictures and 3D models.
pub fn new_project(
    settings: &Settings,
    target: &str,
    request: &str,
    place: Place,
    build_with: Option<&str>,
    generate_assets: bool,
    builder: Option<&str>,
) -> Result<Workspace, ToolError> {
    let stack = build_with.map(self::build_with).transpose()?;
    let target_ref = if target.trim() == "-" { TargetRef::None } else { TargetRef::parse(target) };
    if target_ref == TargetRef::None && target.trim() != "-" {
        return Err(ToolError::invalid(format!("{target:?} is neither a web address nor an existing absolute path"))
            .with_remediation("Pass https://… for a website, a full path for a program, or - for none."));
    }
    let listed = Workspace::list_in(&settings.project_locations());
    let name = unique_name(&suggest_name(request, &target_ref), listed.iter().map(|w| w.manifest.name.as_str()));
    // A folder that already holds a project is attached, not recreated: its saved stack and the
    // fields the person owns stay as they are.
    let attached = matches!(&place, Place::At(folder, _) if has_project(folder));
    let mut ws = match place {
        Place::Projects => Workspace::create(&settings.projects_dir(), &name, request, target_ref)?,
        Place::In(dir) => Workspace::create(dir, &name, request, target_ref)?,
        Place::At(folder, existing) => {
            if attached {
                Workspace::open(folder)?
            } else {
                Workspace::create_at(folder, &name, request, target_ref, existing)?
            }
        }
    };
    apply_new_project_fields(&mut ws, attached, stack.map(|s| s.name.as_str()), settings.assets.budget(), generate_assets, settings.assets.builder_first, builder)?;
    Ok(ws)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_source_accepts_builder_or_service() {
        assert_eq!(assets_source(None).unwrap(), None);
        assert_eq!(assets_source(Some("builder")).unwrap(), Some(true));
        assert_eq!(assets_source(Some("Service")).unwrap(), Some(false));
        assert!(assets_source(Some("both")).is_err());
    }
}
