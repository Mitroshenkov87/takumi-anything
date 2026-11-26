//! The project screen's side panel stays inside its column in a long language (#takumi-anything layout).
//! Optional evidence: OSC_LAYOUT_SHOTS=<dir> cargo test -p craft-anything-ui-egui --test responsive_project

use craft_anything_engine::Mode;
use craft_anything_engine::workspace::{Status, TargetRef};
use craft_anything_ui_egui::state::Screen;
use craft_anything_ui_egui::{CraftApp, demo, i18n};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

/// The side panel's width on the project screen (project.rs `SIDE_W`).
const SIDE_W: f32 = 300.0;

#[test]
fn the_side_panel_keeps_its_rows_inside_it_in_russian() {
    i18n::set_language("ru");
    let mut failures = Vec::new();
    for size in [egui::vec2(1360.0, 860.0), egui::vec2(1100.0, 760.0), egui::vec2(900.0, 700.0)] {
        let mut h = Harness::builder().with_size(size).with_max_steps(32).build_eframe(|cc| {
            CraftApp::setup_context(&cc.egui_ctx);
            let mut a = demo::app(demo::engine(Mode::Experienced, true));
            a.settings.language = "ru".into();
            // The app sets its language when made; the test's choice comes after.
            i18n::set_language("ru");
            demo::fill_project(&mut a);
            if let Some(ws) = a.workspace.as_mut() {
                // A program to recreate, waiting on the person: the panel's longest rows show.
                ws.manifest.target = TargetRef::None;
                ws.manifest.status = Status::NeedsYou;
                ws.manifest.name = "Наковальня с головами гидры — Kimi Code".into();
            }
            a.state.screen = Screen::Project;
            a
        });
        h.run_steps(6);
        if let Some(dir) = std::env::var_os("OSC_LAYOUT_SHOTS") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            h.render().expect("render").save(dir.join(format!("project-{}x{}.png", size.x, size.y))).unwrap();
        }
        let label = i18n::tr("Add pictures of the original");
        // Scrolled below the panel's fold, or (in a narrow window) folded into the feed's top.
        let Some(row) = h.query_by_label(&label).map(|n| n.rect()) else {
            assert!(size.x < 1360.0, "{size:?}: the row shows in the default window");
            continue;
        };
        // The panel's frame keeps a margin: a row wider than the panel spills into the feed.
        if row.width() > SIDE_W || row.right() > size.x || row.left() < size.x - SIDE_W {
            failures.push(format!("{size:?}: \"{label}\" is at {row:?}, outside the {SIDE_W} px panel"));
        }
    }
    i18n::set_language("en");
    assert!(failures.is_empty(), "{failures:#?}");
}
