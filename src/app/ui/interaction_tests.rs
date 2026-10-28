//! Regressions exercise the real window, not a standalone toolbar.
use crate::{
    app::{Disco, editor::Action},
    index::Category,
    settings::{self, PreviewMode},
};
use eframe::egui::{self, Id, Pos2, Rect, Vec2};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
fn frame(app: &mut Disco, ctx: &egui::Context, time: &mut f64, events: Vec<egui::Event>) {
    *time += 0.05;
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1360., 800.))),
            time: Some(*time),
            events,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest()),
    )
    .drop_without_applying_deltas();
}
fn click(app: &mut Disco, ctx: &egui::Context, time: &mut f64, id: Id) {
    let response = ctx.read_response(id).expect("visible widget");
    assert!(response.rect.is_positive());
    let pos = response.rect.center();
    frame(app, ctx, time, vec![egui::Event::PointerMoved(pos)]);
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            time,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    frame(app, ctx, time, vec![]);
}
#[test]
fn full_window_view_switches_properties_and_breadcrumbs_work_with_dirty_text() {
    let Ok(root) = std::env::var("DISCO_BROWSE_FIXTURE") else {
        let root = std::env::temp_dir().join(format!("disco-browse-{}", std::process::id()));
        fs::create_dir_all(root.join("config")).unwrap();
        fs::create_dir_all(root.join("files/Parent Photos/child")).unwrap();
        fs::create_dir_all(root.join("files/Outside/child")).unwrap();
        fs::write(
            root.join("files/Parent Photos/child/test.json"),
            "{\"example\": true}\n",
        )
        .unwrap();
        fs::write(root.join("config/layout.toml"),"save_scan=false\nauto_refresh=false\npreview_mode='inspector'\nproperties=true\nanimations=false\n").unwrap();
        let out=Command::new(std::env::current_exe().unwrap()).args(["--exact","app::ui::interaction_tests::full_window_view_switches_properties_and_breadcrumbs_work_with_dirty_text","--nocapture"])
            .env("DISCO_BROWSE_FIXTURE",&root).env("DISCO_CONFIG_DIR",root.join("config")).env("XDG_DATA_HOME",root.join("data")).output().unwrap();
        fs::remove_dir_all(root).unwrap();
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let files = root.join("files");
    let child = files.join("Parent Photos/child");
    let file = child.join("test.json");
    let ctx = egui::Context::default();
    let mut time = 0.;
    let mut app = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    let report = crate::scan::scan(&files, &crate::scan::Control::default(), |_| {}).unwrap();
    let child_id = report.folders.iter().position(|f| f.path == child).unwrap();
    app.scan.index = crate::index::Index::new(&report);
    app.scan.report = Some(report);
    app.browser.root = files.display().to_string();
    app.browser.category = Category::All;
    app.enter_folder(child_id);
    app.demo = true;
    for _ in 0..3 {
        frame(&mut app, &ctx, &mut time, vec![]);
    }
    click(&mut app, &ctx, &mut time, Id::unique("file-mode-focus"));
    assert_eq!(app.preferences.layout.preview_mode, PreviewMode::Focus);
    assert!(app.browser.properties.as_ref().unwrap().is_dir);
    assert!(
        app.browser.table_root.is_none(),
        "Focus starts from the complete folder tree"
    );
    let tree = ctx
        .read_response(Id::unique("focus-tree-panel"))
        .unwrap()
        .rect;
    let document = ctx
        .read_response(Id::unique("focus-document-panel"))
        .unwrap()
        .rect;
    assert!(tree.width() >= 240. && tree.width() <= 480.);
    assert!(document.left() >= tree.right() - 1. && document.width() > tree.width());
    assert!(ctx.read_response(Id::unique("properties-panel")).is_none());
    let search = ctx
        .read_response(Id::unique("folder-search-bounds"))
        .unwrap()
        .rect;
    let refresh = ctx
        .read_response(Id::unique("folder-refresh-bounds"))
        .unwrap()
        .rect;
    let toolbar = ctx
        .read_response(Id::unique("folder-toolbar"))
        .unwrap()
        .rect;
    assert!(toolbar.height() > 70.);
    assert!(search.left() >= tree.left() + 11. && refresh.right() <= tree.right() - 11.);
    assert!(search.right() + 7. <= refresh.left());
    assert_eq!(app.breadcrumb_directory(), child);
    let parent = child.parent().unwrap().to_path_buf();
    let parent_id = app
        .scan
        .report
        .as_ref()
        .unwrap()
        .folders
        .iter()
        .position(|f| f.path == parent)
        .unwrap();
    // Reproduce searching from the root, entering the found folder and
    // searching again inside it. Clearing the query cannot return to `/`.
    app.browser.table_root = None;
    app.browser.filter.clear();
    ctx.memory_mut(|m| m.request_focus(Id::unique("folder-search")));
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::Paste("Parent Photos".into())],
    );
    frame(&mut app, &ctx, &mut time, vec![]);
    assert_eq!(app.browser.filter, "Parent Photos");
    assert_eq!(
        app.build_visible_rows().len(),
        1,
        "match folder names, not all descendants"
    );
    click(
        &mut app,
        &ctx,
        &mut time,
        Id::unique(("folder-row", &parent)),
    );
    assert!(app.browser.filter.is_empty());
    assert_eq!(app.browser.table_root, Some(parent_id));
    assert_eq!(app.breadcrumb_directory(), parent);
    ctx.memory_mut(|m| m.request_focus(Id::unique("folder-search")));
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::Paste("child".into())],
    );
    frame(&mut app, &ctx, &mut time, vec![]);
    assert_eq!(
        app.build_visible_rows().len(),
        1,
        "exclude the child in Outside"
    );
    click(
        &mut app,
        &ctx,
        &mut time,
        Id::unique(("folder-row", &child)),
    );
    assert!(app.browser.filter.is_empty());
    assert_eq!(app.browser.table_root, Some(child_id));
    assert_eq!(app.breadcrumb_directory(), child);
    assert!(
        matches!(app.build_visible_rows()[0], crate::app::state::TreeRow::Folder(id, 0) if id == child_id)
    );
    click(&mut app, &ctx, &mut time, Id::unique("sort-name"));
    assert!(!app.browser.sort_size && !app.browser.descending);
    click(&mut app, &ctx, &mut time, Id::unique("sort-name"));
    assert!(!app.browser.sort_size && app.browser.descending);
    click(&mut app, &ctx, &mut time, Id::unique("sort-size"));
    assert!(app.browser.sort_size && app.browser.descending);
    // Esc clears a query without losing a keyboard-selected result hidden
    // behind previously collapsed ancestors.
    app.browser.table_root = None;
    app.browser.expanded.clear();
    app.expand_folder(0);
    app.select_folder(child_id);
    app.browser.filter = "child".into();
    ctx.memory_mut(|m| m.request_focus(Id::unique("folder-search")));
    frame(&mut app, &ctx, &mut time, vec![]);
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    frame(&mut app, &ctx, &mut time, vec![]);
    assert!(app.browser.filter.is_empty());
    assert!(app.browser.table_root.is_none());
    assert_eq!(app.breadcrumb_directory(), child);
    assert!(
        app.build_visible_rows()
            .iter()
            .any(|row| matches!(row, crate::app::state::TreeRow::Folder(id, _) if *id == child_id))
    );
    app.select_folder(parent_id);
    frame(&mut app, &ctx, &mut time, vec![]);
    assert_eq!(app.breadcrumb_directory(), parent);
    assert!(
        ctx.read_response(Id::unique(("breadcrumb", &parent)))
            .is_some()
    );
    app.browser.properties = Some(crate::filesystem::Properties::load(
        file.clone(),
        4096,
        None,
        false,
    ));
    let start = Instant::now();
    while app.editor.document.is_none() && start.elapsed() < Duration::from_secs(3) {
        frame(&mut app, &ctx, &mut time, vec![]);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(app.editor.document.is_some());
    assert_eq!(
        app.breadcrumb_directory(),
        child,
        "files show their containing directory"
    );
    for (mode, id) in [
        (PreviewMode::Overview, "file-mode-overview"),
        (PreviewMode::Focus, "file-mode-focus"),
        (PreviewMode::Overview, "file-mode-overview"),
    ] {
        click(&mut app, &ctx, &mut time, Id::unique(id));
        assert_eq!(app.preferences.layout.preview_mode, mode);
        assert_eq!(
            app.preferences.layout.properties,
            mode == PreviewMode::Overview
        );
        assert_eq!(
            settings::read_layout(&root.join("config/layout.toml"))
                .unwrap()
                .preview_mode,
            mode
        );
        assert_eq!(
            ctx.read_response(Id::unique("properties-panel")).is_some(),
            mode == PreviewMode::Overview
        );
        assert!(ctx.read_response(Id::unique("toggle-properties")).is_none());
        assert!(ctx.read_response(Id::unique("toggle-file-view")).is_none());
    }
    app.editor.document.as_mut().unwrap().text.push_str("draft");
    for id in ["file-mode-focus", "file-mode-overview"] {
        click(&mut app, &ctx, &mut time, Id::unique(id));
        assert!(app.editor.dirty());
        assert!(app.editor.pending.is_none());
    }
    click(
        &mut app,
        &ctx,
        &mut time,
        Id::unique(("breadcrumb", &parent)),
    );
    assert!(matches!(app.editor.pending,Some(Action::Navigate(ref p)) if p==&parent));
    assert_eq!(app.browser.properties.as_ref().unwrap().path, file);
    assert!(app.editor.dirty());
    // Cancel keeps the draft; discard is simulated without touching the file.
    app.editor.pending = None;
    app.editor.document = None;
    app.navigate_path(parent.clone());
    frame(&mut app, &ctx, &mut time, vec![]);
    assert_eq!(app.browser.properties.as_ref().unwrap().path, parent);
    assert_eq!(
        app.browser.table_root,
        app.scan
            .report
            .as_ref()
            .unwrap()
            .folders
            .iter()
            .position(|f| f.path == parent)
    );
    assert_eq!(fs::read_to_string(file).unwrap(), "{\"example\": true}\n");
    click(&mut app, &ctx, &mut time, Id::unique("folder-root"));
    assert_eq!(app.breadcrumb_directory(), files);
    assert!(app.browser.table_root.is_none());
    // A truncated read-only preview has one footer line and fits the viewport,
    // including at the end of the inner scroll area.
    app.file_preview.path = Some(root.join("partial.py"));
    app.file_preview.content = Some(crate::file_preview::Content::Text {
        lines: (0..113)
            .map(|i| format!("line {i}: example text"))
            .collect(),
        encoding: "UTF-8",
        truncated: true,
        document: None,
    });
    app.editor.document = None;
    for wheel in [false, true, false] {
        time += 0.05;
        let mut area = Rect::NOTHING;
        let events = if wheel {
            vec![
                egui::Event::PointerMoved(Pos2::new(300., 300.)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    source: egui::MouseWheelSource::Wheel,
                    phase: egui::TouchPhase::Move,
                    delta: Vec2::new(0., -10000.),
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        } else {
            vec![]
        };
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 660.))),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().inner_margin(12.))
                    .show(ui, |ui| {
                        area = ui.available_rect_before_wrap();
                        app.file_preview_ui(ui, false, area.height());
                    });
            },
        )
        .drop_without_applying_deltas();
        let status = ctx
            .read_response(Id::unique("preview-status"))
            .unwrap()
            .rect;
        let viewport = ctx
            .read_response(Id::unique("preview-viewport"))
            .unwrap()
            .rect;
        assert!(
            status.bottom() <= area.bottom() + 1.,
            "{status:?} outside {area:?}"
        );
        assert!(
            (area.bottom() - status.bottom()).abs() < 3.,
            "unused footer space: {status:?} in {area:?}"
        );
        assert!((status.top() - viewport.bottom() - 6.).abs() < 2.);
    }
    assert!(Path::new(&app.browser.root).starts_with(&root));
}
