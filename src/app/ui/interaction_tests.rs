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
fn wait_search(app: &mut Disco, ctx: &egui::Context, time: &mut f64, query: &str) {
    let started = Instant::now();
    while app.search.for_query(query).is_none() && started.elapsed() < Duration::from_secs(4) {
        frame(app, ctx, time, vec![]);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(app.search.for_query(query).is_some());
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
        fs::write(root.join("config/layout.toml"),"save_scan=false\nauto_refresh=false\nsearch_home_fallback=false\npreview_mode='inspector'\nproperties=true\nanimations=false\n").unwrap();
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
    app.preferences.layout.home_path = files.display().to_string();
    let report = crate::scan::scan(&files, &crate::scan::Control::default(), |_| {}).unwrap();
    let child_id = report.folders.iter().position(|f| f.path == child).unwrap();
    app.scan.index = crate::index::Index::new(&report);
    app.scan.report = Some(std::sync::Arc::new(report));
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
    assert!(
        tree.top() < 120.,
        "Focus replaces the SSD header with navigation"
    );
    assert_eq!(
        ctx.read_response(Id::unique("focus-statusbar"))
            .unwrap()
            .rect
            .height(),
        26.
    );
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
    assert!(toolbar.height() >= 36. && toolbar.height() < 40.);
    assert!(toolbar.bottom() <= tree.top() && refresh.right() <= 1360. - 11.);
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
    wait_search(&mut app, &ctx, &mut time, "Parent Photos");
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
    wait_search(&mut app, &ctx, &mut time, "child");
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
    assert_eq!(app.browser.table_root, Some(0));
    // A truncated read-only preview has one footer line and fits the viewport,
    // including at the end of the inner scroll area.
    app.file_preview.path = Some(root.join("partial.py"));
    app.file_preview.content = Some(crate::file_preview::Content::Text {
        lines: (0..113)
            .map(|i| format!("line {i}: example text"))
            .collect(),
        display_source: (0..113)
            .map(|i| format!("line {i}: example text"))
            .collect::<Vec<_>>()
            .join("\n"),
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

fn pointer_at(app: &mut Disco, ctx: &egui::Context, time: &mut f64, pos: Pos2, pressed: bool) {
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
fn press_key(
    app: &mut Disco,
    ctx: &egui::Context,
    time: &mut f64,
    key: egui::Key,
    modifiers: egui::Modifiers,
) {
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            time,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            }],
        );
    }
}
fn wait_document(app: &mut Disco, ctx: &egui::Context, time: &mut f64) {
    frame(app, ctx, time, vec![]);
    let start = Instant::now();
    while (app.editor.document.is_none() || app.editor.saving.is_some())
        && start.elapsed() < Duration::from_secs(4)
    {
        frame(app, ctx, time, vec![]);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(app.editor.document.is_some() && app.editor.saving.is_none());
}
fn drag_category(
    app: &mut Disco,
    ctx: &egui::Context,
    time: &mut f64,
    from: &str,
    to: &str,
    after: bool,
) {
    let source = ctx
        .read_response(Id::unique(("category", from)))
        .unwrap()
        .rect;
    let target = ctx
        .read_response(Id::unique(("category", to)))
        .unwrap()
        .rect;
    let a = source.center();
    let b = Pos2::new(
        target.center().x,
        if after {
            target.bottom() - 5.
        } else {
            target.top() + 5.
        },
    );
    frame(app, ctx, time, vec![egui::Event::PointerMoved(a)]);
    pointer_at(app, ctx, time, a, true);
    frame(
        app,
        ctx,
        time,
        vec![egui::Event::PointerMoved(a + egui::vec2(8., 0.))],
    );
    frame(app, ctx, time, vec![egui::Event::PointerMoved(b)]);
    pointer_at(app, ctx, time, b, false);
    frame(app, ctx, time, vec![]);
}

#[test]
fn category_drag_lock_blank_editor_and_ctrl_tab_use_real_window_input() {
    let Ok(root) = std::env::var("DISCO_SMALL_TASKS_FIXTURE") else {
        let root = std::env::temp_dir().join(format!("disco-small-tasks-{}", std::process::id()));
        fs::create_dir_all(root.join("config")).unwrap();
        fs::create_dir_all(root.join("files/closed")).unwrap();
        fs::write(
            root.join("files/closed/hidden.txt"),
            "do not traverse collapsed folders",
        )
        .unwrap();
        for (name, text) in [
            ("a.rs", "first\nlast λ"),
            ("b.txt", "second"),
            ("c.txt", ""),
            ("d.txt", "last\n"),
        ] {
            fs::write(root.join("files").join(name), text).unwrap();
        }
        fs::write(root.join("config/layout.toml"), "save_scan=false\nauto_refresh=false\nsearch_home_fallback=false\npreview_mode='focus'\nanimations=false\n").unwrap();
        let out = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "app::ui::interaction_tests::category_drag_lock_blank_editor_and_ctrl_tab_use_real_window_input", "--nocapture"])
            .env("DISCO_SMALL_TASKS_FIXTURE", &root).env("DISCO_CONFIG_DIR", root.join("config"))
            .env("XDG_DATA_HOME", root.join("data")).output().unwrap();
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
    let ctx = egui::Context::default();
    let mut time = 0.;
    let mut app = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    app.preferences.layout.categories_locked = false;
    let files = root.join("files");
    let report = crate::scan::scan(&files, &crate::scan::Control::default(), |_| {}).unwrap();
    app.scan.index = crate::index::Index::new(&report);
    app.scan.report = Some(std::sync::Arc::new(report));
    app.browser.root = files.display().to_string();
    app.browser.category = Category::All;
    app.browser.sort_size = false;
    app.browser.descending = false;
    app.expand_folder(0);
    for _ in 0..3 {
        frame(&mut app, &ctx, &mut time, vec![]);
    }

    let original = app.collections.config.clone();
    let child = app.collections.file_for(&original.items[0]);
    let comment = format!(
        "# Keep my custom comment\n{}",
        fs::read_to_string(&child).unwrap()
    );
    fs::write(&child, &comment).unwrap();
    app.collections.reload().unwrap();
    drag_category(&mut app, &ctx, &mut time, "games", "caches", true);
    assert_eq!(
        app.collections
            .config
            .items
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        ["development", "caches", "games", "documents", "other"]
    );
    assert_eq!(
        app.browser.category,
        Category::All,
        "a drag must not select a category"
    );
    assert_eq!(
        crate::categories::Manager::load().config,
        app.collections.config,
        "order survives restart"
    );
    assert_eq!(
        fs::read_to_string(&child).unwrap(),
        comment,
        "order only changes the manifest"
    );
    app.preferences.layout.categories_locked = true;
    crate::settings::write(&root.join("config/layout.toml"), &app.preferences.layout).unwrap();
    assert!(
        crate::settings::read_layout(&root.join("config/layout.toml"))
            .unwrap()
            .categories_locked
    );
    frame(&mut app, &ctx, &mut time, vec![]);
    let locked = app.collections.config.clone();
    drag_category(&mut app, &ctx, &mut time, "games", "development", false);
    assert_eq!(app.collections.config, locked);
    assert!(!egui::DragAndDrop::has_any_payload(&ctx));
    app.preferences.layout.categories_locked = false;
    frame(&mut app, &ctx, &mut time, vec![]);
    drag_category(&mut app, &ctx, &mut time, "games", "development", false);
    assert_eq!(app.collections.config, original);
    for _ in 0..3 {
        frame(&mut app, &ctx, &mut time, vec![]);
    }
    // Tiny movement is still a click, not a reorder; Trash remains a fixed tool.
    let rect = ctx
        .read_response(Id::unique(("category", "games")))
        .unwrap()
        .rect;
    let before = app.collections.config.clone();
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::PointerMoved(rect.center())],
    );
    pointer_at(&mut app, &ctx, &mut time, rect.center(), true);
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::PointerMoved(
            rect.center() + egui::vec2(1., 0.),
        )],
    );
    pointer_at(
        &mut app,
        &ctx,
        &mut time,
        rect.center() + egui::vec2(1., 0.),
        false,
    );
    assert_eq!(app.collections.config, before);
    assert_eq!(
        app.browser.category,
        Category::Games,
        "click rect={rect:?}; configs={:?}",
        app.collections
            .config
            .items
            .iter()
            .map(|c| &c.id)
            .collect::<Vec<_>>()
    );
    assert!(!before.items.iter().any(|c| c.id == "trash"));
    let manifest = fs::read(crate::categories::path()).unwrap();
    let revision = app.collections.revision;
    assert!(!app.collections.reorder("games", "games", false).unwrap());
    assert_eq!(app.collections.revision, revision);
    app.collections.draft = Some(app.collections.config.clone());
    assert!(app.collections.reorder("games", "other", true).is_err());
    app.collections.draft = None;
    fs::write(&child, format!("# External edit\n{comment}")).unwrap();
    assert!(app.collections.reorder("games", "other", true).is_err());
    assert_eq!(fs::read(crate::categories::path()).unwrap(), manifest);
    assert_eq!(app.collections.config, original);
    app.collections.reload().unwrap();
    app.browser.category = Category::All;
    app.browser.table_root = None;
    app.expand_folder(0);
    let a = files.join("a.rs");
    let b = files.join("b.txt");
    app.select_file(a.clone(), 4096);
    wait_document(&mut app, &ctx, &mut time);
    // A click far below a short Unicode document lands at the final character.
    let id = Id::unique(("file-editor", Some(a.clone())));
    frame(&mut app, &ctx, &mut time, vec![]);
    let area = ctx.read_response(id).unwrap().rect;
    assert!(
        area.height() > 300.,
        "the editor hit area must fill the viewport"
    );
    let pos = area.right_bottom() - egui::vec2(20., 20.);
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::PointerMoved(pos)],
    );
    pointer_at(&mut app, &ctx, &mut time, pos, true);
    pointer_at(&mut app, &ctx, &mut time, pos, false);
    assert_eq!(ctx.memory(|m| m.focused()), Some(id));
    let cursor = egui::TextEdit::load_state(&ctx, id)
        .unwrap()
        .cursor
        .char_range()
        .unwrap();
    assert_eq!(cursor.primary.index.0, "first\nlast λ".chars().count());
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::Text("!".into())],
    );
    assert_eq!(app.editor.document.as_ref().unwrap().text, "first\nlast λ!");
    // Focus switches documents without losing the other tab's draft.
    press_key(
        &mut app,
        &ctx,
        &mut time,
        egui::Key::Tab,
        egui::Modifiers::CTRL,
    );
    wait_document(&mut app, &ctx, &mut time);
    assert!(app.editor.pending.is_none());
    assert_eq!(app.editor.path.as_ref(), Some(&b));
    assert_eq!(
        app.tabs
            .inactive
            .get(&a)
            .unwrap()
            .editor
            .document
            .as_ref()
            .unwrap()
            .text,
        "first\nlast λ!"
    );
    app.activate_tab(&a, &ctx);
    wait_document(&mut app, &ctx, &mut time);
    app.save_editor(&ctx);
    wait_document(&mut app, &ctx, &mut time);
    ctx.memory_mut(|m| m.request_focus(id));
    press_key(
        &mut app,
        &ctx,
        &mut time,
        egui::Key::Tab,
        egui::Modifiers::CTRL,
    );
    wait_document(&mut app, &ctx, &mut time);
    assert_eq!(app.editor.path.as_ref(), Some(&b));
    assert_eq!(app.editor.document.as_ref().unwrap().text, "second");
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(Id::unique(("file-editor", Some(b.clone()))))
    );
    press_key(
        &mut app,
        &ctx,
        &mut time,
        egui::Key::Tab,
        egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
    );
    wait_document(&mut app, &ctx, &mut time);
    assert_eq!(app.editor.path.as_ref(), Some(&a));
    // At the first file there is no wrap, and collapsed children are skipped.
    press_key(
        &mut app,
        &ctx,
        &mut time,
        egui::Key::Tab,
        egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
    );
    assert_eq!(app.editor.path.as_ref(), Some(&a));
    assert!(!app.editor.dirty());
    for (name, expected) in [("c.txt", ""), ("d.txt", "last\n")] {
        let p = files.join(name);
        app.select_file(p.clone(), 4096);
        wait_document(&mut app, &ctx, &mut time);
        let id = Id::unique(("file-editor", Some(p)));
        let rect = ctx.read_response(id).unwrap().rect;
        let pos = rect.left_bottom() + egui::vec2(40., -15.);
        frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::PointerMoved(pos)],
        );
        pointer_at(&mut app, &ctx, &mut time, pos, true);
        pointer_at(&mut app, &ctx, &mut time, pos, false);
        let gutter = ctx.read_response(id.with("blank-gutter")).unwrap().rect;
        let gutter_pos = gutter.center();
        frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::PointerMoved(gutter_pos)],
        );
        pointer_at(&mut app, &ctx, &mut time, gutter_pos, true);
        pointer_at(&mut app, &ctx, &mut time, gutter_pos, false);
        assert_eq!(ctx.memory(|m| m.focused()), Some(id));
        assert_eq!(
            egui::TextEdit::load_state(&ctx, id)
                .unwrap()
                .cursor
                .char_range()
                .unwrap()
                .primary
                .index
                .0,
            expected.chars().count()
        );
    }
}

#[test]
fn autosave_debounces_by_category_and_preserves_conflicting_drafts() {
    let Ok(root) = std::env::var("DISCO_AUTOSAVE_FIXTURE") else {
        let root = std::env::temp_dir().join(format!("disco-autosave-{}", std::process::id()));
        fs::create_dir_all(root.join("config")).unwrap();
        fs::create_dir_all(root.join("files")).unwrap();
        fs::write(root.join("files/a.txt"), "original").unwrap();
        fs::write(
            root.join("config/layout.toml"),
            "save_scan=false\nsearch_home_fallback=false\npreview_mode='focus'\nanimations=false\n",
        )
        .unwrap();
        let out=Command::new(std::env::current_exe().unwrap())
            .args(["--exact","app::ui::interaction_tests::autosave_debounces_by_category_and_preserves_conflicting_drafts","--nocapture"])
            .env("DISCO_AUTOSAVE_FIXTURE",&root).env("DISCO_CONFIG_DIR",root.join("config"))
            .env("XDG_DATA_HOME",root.join("data")).output().unwrap();
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
    let file = files.join("a.txt");
    let ctx = egui::Context::default();
    let mut time = 0.;
    let mut app = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    let report = crate::scan::scan(&files, &crate::scan::Control::default(), |_| {}).unwrap();
    app.scan.index = crate::index::Index::new(&report);
    app.scan.report = Some(std::sync::Arc::new(report));
    app.browser.root = files.display().to_string();
    app.browser.category = Category::Documents;
    app.collections
        .config
        .items
        .iter_mut()
        .find(|c| c.category() == Category::Documents)
        .unwrap()
        .folders = vec![files.clone()];
    app.update_collections();
    app.select_file(file.clone(), 4096);
    wait_document(&mut app, &ctx, &mut time);
    let id = Id::unique(("file-editor", Some(file.clone())));
    ctx.memory_mut(|m| m.request_focus(id));
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::Text("first".into())],
    );
    for _ in 0..25 {
        frame(&mut app, &ctx, &mut time, vec![]);
    }
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "original",
        "auto-save defaults off"
    );
    app.collections
        .config
        .items
        .iter_mut()
        .find(|c| c.category() == Category::Documents)
        .unwrap()
        .auto_save = true;
    frame(&mut app, &ctx, &mut time, vec![]);
    for _ in 0..10 {
        frame(&mut app, &ctx, &mut time, vec![]);
    }
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::Text("second".into())],
    );
    for _ in 0..15 {
        frame(&mut app, &ctx, &mut time, vec![]);
    }
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "original",
        "typing resets the one-second deadline"
    );
    for _ in 0..8 {
        frame(&mut app, &ctx, &mut time, vec![]);
    }
    wait_document(&mut app, &ctx, &mut time);
    assert!(!app.editor.dirty());
    assert!(app.editor.last_saved_at.is_some());
    assert_eq!(ctx.memory(|m| m.focused()), Some(id));
    let saved = fs::read_to_string(&file).unwrap();
    assert!(saved.contains("first") && saved.contains("second"));
    // Read-only wins even when the auto-save option is enabled.
    app.editor.document.as_mut().unwrap().text.push_str("local");
    app.note_editor_change(&ctx);
    app.collections
        .config
        .items
        .iter_mut()
        .find(|c| c.category() == Category::Documents)
        .unwrap()
        .read_only = true;
    for _ in 0..25 {
        frame(&mut app, &ctx, &mut time, vec![]);
    }
    assert_eq!(fs::read_to_string(&file).unwrap(), saved);
    app.collections
        .config
        .items
        .iter_mut()
        .find(|c| c.category() == Category::Documents)
        .unwrap()
        .read_only = false;
    // A replacement on disk is never overwritten and failed writes do not loop.
    fs::write(&file, "external").unwrap();
    for _ in 0..25 {
        frame(&mut app, &ctx, &mut time, vec![]);
    }
    wait_document(&mut app, &ctx, &mut time);
    assert!(app.editor.dirty() && !app.editor.error.is_empty());
    assert!(
        app.editor
            .document
            .as_ref()
            .unwrap()
            .text
            .ends_with("local")
    );
    for _ in 0..40 {
        frame(&mut app, &ctx, &mut time, vec![]);
    }
    assert!(app.editor.saving.is_none());
    assert_eq!(fs::read_to_string(&file).unwrap(), "external");
    let mut config = app.collections.config.clone();
    config
        .items
        .iter_mut()
        .find(|c| c.category() == Category::Games)
        .unwrap()
        .auto_save = false;
    app.collections.draft = Some(config);
    app.collections.save().unwrap();
    let restored = crate::categories::Manager::load();
    assert!(restored.config.get(Category::Documents).unwrap().auto_save);
    assert!(!restored.config.get(Category::Games).unwrap().auto_save);
}

#[test]
fn detached_settings_save_cancel_and_reopen_keep_the_editor_draft() {
    let Ok(root) = std::env::var("DISCO_SETTINGS_FIXTURE") else {
        let root = std::env::temp_dir().join(format!("disco-settings-ui-{}", std::process::id()));
        fs::create_dir_all(root.join("config")).unwrap();
        fs::create_dir_all(root.join("files")).unwrap();
        fs::write(root.join("files/a.txt"), "original").unwrap();
        fs::write(
            root.join("config/layout.toml"),
            "save_scan=false\npreview_mode='focus'\nanimations=false\n",
        )
        .unwrap();
        let out=Command::new(std::env::current_exe().unwrap()).args(["--exact","app::ui::interaction_tests::detached_settings_save_cancel_and_reopen_keep_the_editor_draft","--nocapture"])
            .env("DISCO_SETTINGS_FIXTURE",&root).env("DISCO_CONFIG_DIR",root.join("config")).env("XDG_DATA_HOME",root.join("data")).output().unwrap();
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
    let file = root.join("files/a.txt");
    let ctx = egui::Context::default();
    let mut time = 0.;
    let mut app = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    app.select_file(file.clone(), 4096);
    wait_document(&mut app, &ctx, &mut time);
    app.editor.document.as_mut().unwrap().text.push_str("draft");
    let original = app.preferences.layout.clone();
    click(&mut app, &ctx, &mut time, Id::unique("open-settings"));
    assert!(app.preferences.open);
    app.preferences.layout.categories_locked = !original.categories_locked;
    app.preferences.layout.shortcuts.focus =
        crate::shortcuts::Binding::new(egui::Key::F9, egui::Modifiers::NONE);
    app.preferences.page = 1;
    frame(&mut app, &ctx, &mut time, vec![]);
    assert!(ctx.read_response(Id::unique(("settings_nav", 1))).is_some());
    click(&mut app, &ctx, &mut time, Id::unique("open-settings"));
    assert_ne!(
        app.preferences.layout.categories_locked, original.categories_locked,
        "reopening does not reset preview changes"
    );
    click(
        &mut app,
        &ctx,
        &mut time,
        Id::unique("settings-cancel-bounds"),
    );
    assert!(!app.preferences.open);
    assert_eq!(app.preferences.layout, original);
    assert!(app.editor.dirty());
    assert_eq!(app.editor.path.as_ref(), Some(&file));
    click(&mut app, &ctx, &mut time, Id::unique("open-settings"));
    app.preferences.layout.categories_locked = !original.categories_locked;
    app.preferences.layout.shortcuts.focus =
        crate::shortcuts::Binding::new(egui::Key::F9, egui::Modifiers::NONE);
    frame(&mut app, &ctx, &mut time, vec![]);
    click(
        &mut app,
        &ctx,
        &mut time,
        Id::unique("settings-save-bounds"),
    );
    assert!(!app.preferences.open);
    assert!(app.editor.dirty());
    assert_eq!(
        settings::read_layout(&root.join("config/layout.toml"))
            .unwrap()
            .shortcuts
            .focus
            .key,
        "F9"
    );
    assert_ne!(
        settings::read_layout(&root.join("config/layout.toml"))
            .unwrap()
            .categories_locked,
        original.categories_locked
    );
    click(&mut app, &ctx, &mut time, Id::unique("open-settings"));
    app.preferences.layout.categories_locked = original.categories_locked;
    frame(&mut app, &ctx, &mut time, vec![]);
    click(&mut app, &ctx, &mut time, Id::unique("close_preferences"));
    assert!(!app.preferences.open);
    assert_ne!(
        app.preferences.layout.categories_locked,
        original.categories_locked
    );
    assert_eq!(fs::read_to_string(file).unwrap(), "original");
}

#[test]
fn focus_tabs_keep_drafts_cursors_policies_and_guard_all_closes() {
    let Ok(root) = std::env::var("DISCO_TABS_FIXTURE") else {
        let root = std::env::temp_dir().join(format!("disco-tabs-ui-{}", std::process::id()));
        fs::create_dir_all(root.join("config")).unwrap();
        fs::create_dir_all(root.join("files")).unwrap();
        fs::write(root.join("files/a.rs"), "fn first() {}\n").unwrap();
        fs::write(root.join("files/b.txt"), "second\n").unwrap();
        fs::write(
            root.join("config/layout.toml"),
            "save_scan=false\nsearch_home_fallback=false\npreview_mode='focus'\nanimations=false\n",
        )
        .unwrap();
        let out=Command::new(std::env::current_exe().unwrap()).args(["--exact","app::ui::interaction_tests::focus_tabs_keep_drafts_cursors_policies_and_guard_all_closes","--nocapture"])
            .env("DISCO_TABS_FIXTURE",&root).env("DISCO_CONFIG_DIR",root.join("config")).env("XDG_DATA_HOME",root.join("data")).output().unwrap();
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
    let a = files.join("a.rs");
    let b = files.join("b.txt");
    let ctx = egui::Context::default();
    let mut time = 0.;
    let mut app = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    let report = crate::scan::scan(&files, &crate::scan::Control::default(), |_| {}).unwrap();
    app.scan.index = crate::index::Index::new(&report);
    app.scan.report = Some(std::sync::Arc::new(report));
    app.browser.root = files.display().to_string();
    app.browser.category = Category::Documents;
    app.expand_folder(0);
    app.select_file(a.clone(), 4096);
    wait_document(&mut app, &ctx, &mut time);
    let id = Id::unique(("file-editor", Some(a.clone())));
    ctx.memory_mut(|m| m.request_focus(id));
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::Text("draft-a".into())],
    );
    let cursor = egui::TextEdit::load_state(&ctx, id)
        .unwrap()
        .cursor
        .char_range();
    let draft = app.editor.document.as_ref().unwrap().text.clone();
    app.browser.category = Category::Games;
    app.select_file(b.clone(), 4096);
    wait_document(&mut app, &ctx, &mut time);
    assert!(app.editor.pending.is_none());
    assert_eq!(app.tabs.order.len(), 2);
    assert_eq!(
        app.tabs.inactive[&a].editor.document.as_ref().unwrap().text,
        draft
    );
    frame(&mut app, &ctx, &mut time, vec![]);
    click(&mut app, &ctx, &mut time, Id::unique(("document-tab", &a)));
    assert_eq!(app.editor.origin, Some(Category::Documents));
    assert_eq!(app.browser.category, Category::Documents);
    assert_eq!(app.editor.document.as_ref().unwrap().text, draft);
    assert_eq!(
        egui::TextEdit::load_state(&ctx, id)
            .unwrap()
            .cursor
            .char_range(),
        cursor
    );
    assert_eq!(
        app.tabs.order.len(),
        2,
        "opening an existing tab never duplicates it"
    );
    app.browser.category = Category::Games;
    app.browser.properties = None;
    frame(&mut app, &ctx, &mut time, vec![]);
    assert_eq!(
        app.editor.path.as_ref(),
        Some(&a),
        "browsing another category keeps the document"
    );
    app.collections
        .config
        .items
        .iter_mut()
        .find(|c| c.category() == Category::Documents)
        .unwrap()
        .read_only = true;
    assert!(
        app.editor_read_only(),
        "a tab retains its origin policy across category changes"
    );
    let before = app.editor.document.as_ref().unwrap().text.clone();
    ctx.memory_mut(|m| m.request_focus(id));
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::Paste("forbidden".into())],
    );
    assert_eq!(app.editor.document.as_ref().unwrap().text, before);
    app.collections
        .config
        .items
        .iter_mut()
        .find(|c| c.category() == Category::Documents)
        .unwrap()
        .read_only = false;
    // Closing a dirty tab offers confirmation. Escape retains both tabs and draft.
    click(
        &mut app,
        &ctx,
        &mut time,
        Id::unique(("close-document-tab", &a)),
    );
    assert!(matches!(app.editor.pending,Some(Action::CloseTab(ref path)) if path==&a));
    press_key(
        &mut app,
        &ctx,
        &mut time,
        egui::Key::Escape,
        egui::Modifiers::NONE,
    );
    assert_eq!(app.tabs.order.len(), 2);
    assert!(app.editor.dirty());
    // Auto-save also finishes for an inactive document, using its own category.
    app.collections
        .config
        .items
        .iter_mut()
        .find(|c| c.category() == Category::Documents)
        .unwrap()
        .auto_save = true;
    app.note_editor_change(&ctx);
    app.activate_tab(&b, &ctx);
    for _ in 0..25 {
        frame(&mut app, &ctx, &mut time, vec![]);
    }
    let start = Instant::now();
    while app.tabs.inactive[&a].editor.saving.is_some() && start.elapsed() < Duration::from_secs(3)
    {
        frame(&mut app, &ctx, &mut time, vec![]);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(fs::read_to_string(&a).unwrap(), draft);
    assert!(!app.tabs.inactive[&a].editor.dirty());
    app.activate_tab(&a, &ctx);
    wait_document(&mut app, &ctx, &mut time);
    app.collections
        .config
        .items
        .iter_mut()
        .find(|c| c.category() == Category::Documents)
        .unwrap()
        .auto_save = false;
    app.editor
        .document
        .as_mut()
        .unwrap()
        .text
        .push_str("unsaved");
    app.activate_tab(&b, &ctx);
    wait_document(&mut app, &ctx, &mut time);
    // Native/app close must inspect inactive drafts as well as the active file.
    app.request_close(&ctx);
    assert_eq!(app.editor.path.as_ref(), Some(&a));
    assert!(matches!(app.editor.pending, Some(Action::Close)));
    press_key(
        &mut app,
        &ctx,
        &mut time,
        egui::Key::Escape,
        egui::Modifiers::NONE,
    );
    assert!(app.editor.dirty());
    app.request_close_tab(&a, &ctx);
    app.save_editor(&ctx);
    let start = Instant::now();
    while app.tabs.order.contains(&a) && start.elapsed() < Duration::from_secs(3) {
        frame(&mut app, &ctx, &mut time, vec![]);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!app.tabs.order.contains(&a));
    assert_eq!(app.editor.path.as_ref(), Some(&b));
    assert!(fs::read_to_string(&a).unwrap().ends_with("unsaved"));
    app.request_close_tab(&b, &ctx);
    frame(&mut app, &ctx, &mut time, vec![]);
    assert!(app.tabs.order.is_empty() && app.editor.path.is_none());
}

#[test]
fn keyboard_workspace_walk_preserves_words_drafts_and_reorders_categories() {
    let Ok(root) = std::env::var("DISCO_WORKSPACE_KEYS_FIXTURE") else {
        let root =
            std::env::temp_dir().join(format!("disco-workspace-keys-{}", std::process::id()));
        fs::create_dir_all(root.join("config")).unwrap();
        fs::create_dir_all(root.join("files")).unwrap();
        fs::write(root.join("files/a.txt"), "one two three").unwrap();
        fs::write(root.join("files/b.txt"), "second").unwrap();
        fs::write(
            root.join("config/layout.toml"),
            "save_scan=false\nsearch_home_fallback=false\npreview_mode='focus'\nanimations=false\n",
        )
        .unwrap();
        let out=Command::new(std::env::current_exe().unwrap()).args(["--exact","app::ui::interaction_tests::keyboard_workspace_walk_preserves_words_drafts_and_reorders_categories","--nocapture"])
            .env("DISCO_WORKSPACE_KEYS_FIXTURE",&root).env("DISCO_CONFIG_DIR",root.join("config")).env("XDG_DATA_HOME",root.join("data")).output().unwrap();
        fs::remove_dir_all(root).unwrap();
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        return;
    };
    use crate::app::workspace_keyboard::Area;
    use egui::{Key, Modifiers};
    let root = PathBuf::from(root);
    let files = root.join("files");
    let a = files.join("a.txt");
    let b = files.join("b.txt");
    let ctx = egui::Context::default();
    let mut time = 0.;
    let mut app = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    let report = crate::scan::scan(&files, &crate::scan::Control::default(), |_| {}).unwrap();
    app.scan.index = crate::index::Index::new(&report);
    app.scan.report = Some(std::sync::Arc::new(report));
    app.browser.root = files.display().to_string();
    app.browser.category = Category::All;
    app.expand_folder(0);
    app.select_file(a.clone(), 4096);
    wait_document(&mut app, &ctx, &mut time);
    app.select_file(b.clone(), 4096);
    wait_document(&mut app, &ctx, &mut time);
    app.activate_tab(&a, &ctx);
    wait_document(&mut app, &ctx, &mut time);
    let editor_id = Id::unique(("file-editor", Some(a.clone())));
    app.focus_area(Area::Editor, &ctx);
    press_key(&mut app, &ctx, &mut time, Key::End, Modifiers::CTRL);
    let before = egui::TextEdit::load_state(&ctx, editor_id)
        .unwrap()
        .cursor
        .char_range()
        .unwrap()
        .primary
        .index
        .0;
    press_key(&mut app, &ctx, &mut time, Key::ArrowLeft, Modifiers::CTRL);
    let after = egui::TextEdit::load_state(&ctx, editor_id)
        .unwrap()
        .cursor
        .char_range()
        .unwrap()
        .primary
        .index
        .0;
    assert!(after < before, "Ctrl+Left retains standard word movement");
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::Text("draft".into())],
    );
    let draft = app.editor.document.as_ref().unwrap().text.clone();
    let cursor = egui::TextEdit::load_state(&ctx, editor_id)
        .unwrap()
        .cursor
        .char_range();
    let panel = Modifiers::CTRL | Modifiers::ALT;
    press_key(&mut app, &ctx, &mut time, Key::ArrowLeft, panel);
    assert!(app.keyboard.area == Area::Tree);
    press_key(&mut app, &ctx, &mut time, Key::ArrowLeft, panel);
    assert!(app.keyboard.area == Area::Categories);
    app.keyboard.category = Some("games".into());
    app.focus_area(Area::Categories, &ctx);
    frame(&mut app, &ctx, &mut time, vec![]);
    press_key(&mut app, &ctx, &mut time, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(app.keyboard.category.as_deref(), Some("development"));
    assert_eq!(
        app.browser.category,
        Category::All,
        "arrow navigation doesn't activate until Enter"
    );
    press_key(&mut app, &ctx, &mut time, Key::Enter, Modifiers::NONE);
    assert_eq!(app.browser.category, Category::Development);
    assert!(app.editor.pending.is_none());
    assert_eq!(app.editor.document.as_ref().unwrap().text, draft);
    press_key(
        &mut app,
        &ctx,
        &mut time,
        Key::ArrowUp,
        Modifiers::CTRL | Modifiers::SHIFT,
    );
    assert_eq!(app.collections.config.items[0].id, "development");
    assert_eq!(
        crate::categories::Manager::load().config.items[0].id,
        "development"
    );
    app.preferences.layout.categories_locked = true;
    let order = app.collections.config.clone();
    press_key(
        &mut app,
        &ctx,
        &mut time,
        Key::ArrowDown,
        Modifiers::CTRL | Modifiers::SHIFT,
    );
    assert_eq!(app.collections.config, order);
    app.preferences.layout.categories_locked = false;
    press_key(&mut app, &ctx, &mut time, Key::ArrowRight, panel);
    assert!(app.keyboard.area == Area::Tree);
    press_key(&mut app, &ctx, &mut time, Key::ArrowRight, panel);
    assert!(app.keyboard.area == Area::Editor);
    assert_eq!(ctx.memory(|m| m.focused()), Some(editor_id));
    assert_eq!(
        egui::TextEdit::load_state(&ctx, editor_id)
            .unwrap()
            .cursor
            .char_range(),
        cursor
    );
    press_key(&mut app, &ctx, &mut time, Key::F6, Modifiers::NONE);
    assert!(app.keyboard.area == Area::Search);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(Id::unique("folder-search"))
    );
    press_key(&mut app, &ctx, &mut time, Key::F6, Modifiers::SHIFT);
    assert!(app.keyboard.area == Area::Editor);
    press_key(&mut app, &ctx, &mut time, Key::ArrowUp, panel);
    assert!(app.keyboard.area == Area::Tabs);
    press_key(&mut app, &ctx, &mut time, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(app.keyboard.tab.as_ref(), Some(&b));
    press_key(&mut app, &ctx, &mut time, Key::Enter, Modifiers::NONE);
    assert_eq!(app.editor.path.as_ref(), Some(&b));
    assert!(app.keyboard.area == Area::Editor);
    press_key(&mut app, &ctx, &mut time, Key::W, Modifiers::CTRL);
    frame(&mut app, &ctx, &mut time, vec![]);
    assert!(!app.tabs.order.contains(&b));
    assert_eq!(app.editor.path.as_ref(), Some(&a));
    assert!(app.editor.dirty());
    press_key(&mut app, &ctx, &mut time, Key::W, Modifiers::CTRL);
    assert!(matches!(app.editor.pending, Some(Action::CloseTab(_))));
    press_key(&mut app, &ctx, &mut time, Key::Escape, Modifiers::NONE);
    assert!(app.editor.pending.is_none());
    press_key(&mut app, &ctx, &mut time, Key::Comma, Modifiers::CTRL);
    assert!(app.preferences.open);
    press_key(&mut app, &ctx, &mut time, Key::Escape, Modifiers::NONE);
    assert!(!app.preferences.open);
    assert!(app.editor.dirty());
    app.keyboard.category = Some("__trash".into());
    app.focus_area(Area::Categories, &ctx);
    frame(&mut app, &ctx, &mut time, vec![]);
    press_key(&mut app, &ctx, &mut time, Key::Enter, Modifiers::NONE);
    assert!(app.browser.view == crate::app::state::View::Trash);
    assert!(app.editor.pending.is_none());
    assert_eq!(app.editor.document.as_ref().unwrap().text, draft);
}

#[test]
fn keyboard_preview_reuses_one_tab_and_enter_right_keep_before_opening_or_focusing() {
    let Ok(root) = std::env::var("DISCO_PREVIEW_TAB_FIXTURE") else {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("disco-preview-tab-{}", std::process::id()));
        for dir in ["config", "files", "bin", "data/applications"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        for i in 0..30 {
            fs::write(
                root.join(format!("files/{i:02}.txt")),
                format!("document {i}"),
            )
            .unwrap();
        }
        image::RgbImage::from_pixel(12, 20, image::Rgb([40, 50, 60]))
            .save(root.join("files/z.jpg"))
            .unwrap();
        fs::write(
            root.join("config/layout.toml"),
            "save_scan=false\nsearch_home_fallback=false\npreview_mode='focus'\nanimations=false\n",
        )
        .unwrap();
        fs::write(
            root.join("data/applications/viewer.desktop"),
            "[Desktop Entry]\nType=Application\nExec=false\n",
        )
        .unwrap();
        for (name, script) in [
            (
                "xdg-mime",
                "#!/bin/sh\nif [ \"$2\" = filetype ]; then printf 'text/plain\\n'; else printf 'viewer.desktop\\n'; fi\n",
            ),
            (
                "gio",
                "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$DISCO_OPEN_LOG\"\n",
            ),
        ] {
            let p = root.join("bin").join(name);
            fs::write(&p, script).unwrap();
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let out=Command::new(std::env::current_exe().unwrap()).args(["--exact","app::ui::interaction_tests::keyboard_preview_reuses_one_tab_and_enter_right_keep_before_opening_or_focusing","--nocapture"])
            .env("DISCO_PREVIEW_TAB_FIXTURE",&root).env("DISCO_CONFIG_DIR",root.join("config")).env("XDG_DATA_HOME",root.join("data")).env("XDG_DATA_DIRS",root.join("absent"))
            .env("DISCO_OPEN_LOG",root.join("opened.log")).env("PATH",format!("{}:/usr/bin:/bin",root.join("bin").display())).output().unwrap();
        fs::remove_dir_all(root).unwrap();
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        return;
    };
    use crate::app::workspace_keyboard::Area;
    use egui::{Key, Modifiers};
    let root = PathBuf::from(root);
    let files = root.join("files");
    let a = files.join("00.txt");
    let b = files.join("01.txt");
    let log = root.join("opened.log");
    let ctx = egui::Context::default();
    let mut time = 0.;
    let mut app = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    let report = crate::scan::scan(&files, &crate::scan::Control::default(), |_| {}).unwrap();
    app.scan.index = crate::index::Index::new(&report);
    app.scan.report = Some(std::sync::Arc::new(report));
    app.browser.root = files.display().to_string();
    app.browser.category = Category::All;
    app.browser.sort_size = false;
    app.browser.descending = false;
    app.enter_folder(0);
    let start = Instant::now();
    while app.files.get(&files).is_none() && start.elapsed() < Duration::from_secs(3) {
        frame(&mut app, &ctx, &mut time, vec![]);
        std::thread::sleep(Duration::from_millis(5));
    }
    app.focus_area(Area::Tree, &ctx);
    press_key(&mut app, &ctx, &mut time, Key::Home, Modifiers::NONE);
    press_key(&mut app, &ctx, &mut time, Key::ArrowDown, Modifiers::NONE);
    wait_document(&mut app, &ctx, &mut time);
    assert_eq!(app.tabs.order, vec![a.clone()]);
    assert_eq!(app.tabs.preview.as_ref(), Some(&a));
    press_key(&mut app, &ctx, &mut time, Key::ArrowDown, Modifiers::NONE);
    wait_document(&mut app, &ctx, &mut time);
    assert_eq!(app.tabs.order, vec![b.clone()]);
    assert!(app.tabs.inactive.is_empty());
    assert!(
        egui::TextEdit::load_state(&ctx, Id::unique(("file-editor", Some(a.clone())))).is_none(),
        "discarded preview releases undo state"
    );
    for _ in 0..15 {
        press_key(&mut app, &ctx, &mut time, Key::ArrowDown, Modifiers::NONE);
        assert_eq!(app.tabs.order.len(), 1);
    }
    let before_cycle = app.editor.path.clone();
    press_key(&mut app, &ctx, &mut time, Key::Tab, Modifiers::CTRL);
    assert_eq!(app.tabs.order.len(), 1);
    assert!(app.keyboard.area == Area::Tree);
    press_key(
        &mut app,
        &ctx,
        &mut time,
        Key::Tab,
        Modifiers::CTRL | Modifiers::SHIFT,
    );
    assert_eq!(app.tabs.order.len(), 1);
    assert_eq!(app.editor.path, before_cycle);
    // Returning to the first file still uses the same temporary slot.
    press_key(&mut app, &ctx, &mut time, Key::Home, Modifiers::NONE);
    press_key(&mut app, &ctx, &mut time, Key::ArrowDown, Modifiers::NONE);
    wait_document(&mut app, &ctx, &mut time);
    assert_eq!(app.tabs.preview.as_ref(), Some(&a));
    press_key(&mut app, &ctx, &mut time, Key::Enter, Modifiers::NONE);
    assert!(app.tabs.preview.is_none());
    assert!(!log.exists());
    assert!(!app.opening_file.load(std::sync::atomic::Ordering::Relaxed));
    press_key(&mut app, &ctx, &mut time, Key::Enter, Modifiers::NONE);
    let start = Instant::now();
    while app.opening_file.load(std::sync::atomic::Ordering::Relaxed)
        && start.elapsed() < Duration::from_secs(3)
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        fs::read_to_string(&log)
            .unwrap()
            .contains(a.to_str().unwrap())
    );
    fs::remove_file(&log).unwrap();
    // Right keeps the tab before focusing it; the first press stays in the tree.
    press_key(&mut app, &ctx, &mut time, Key::ArrowDown, Modifiers::NONE);
    press_key(&mut app, &ctx, &mut time, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(app.tabs.order, vec![a.clone(), b.clone()]);
    assert!(app.tabs.preview.is_none());
    assert!(app.keyboard.area == Area::Tree);
    wait_document(&mut app, &ctx, &mut time);
    press_key(&mut app, &ctx, &mut time, Key::ArrowRight, Modifiers::NONE);
    assert!(app.keyboard.area == Area::Editor);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(Id::unique(("file-editor", Some(b.clone()))))
    );
    // Enter in the editor stays an ordinary newline.
    press_key(&mut app, &ctx, &mut time, Key::Enter, Modifiers::NONE);
    assert!(app.editor.dirty());
    assert!(!log.exists());
    app.focus_area(Area::Tree, &ctx);
    press_key(&mut app, &ctx, &mut time, Key::ArrowDown, Modifiers::NONE);
    wait_document(&mut app, &ctx, &mut time);
    let c = files.join("02.txt");
    assert_eq!(app.tabs.preview.as_ref(), Some(&c));
    app.focus_area(Area::Editor, &ctx);
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::Text("draft".into())],
    );
    assert!(
        app.tabs.preview.is_none(),
        "editing keeps a preview so it cannot be overwritten"
    );
    let draft = app.editor.document.as_ref().unwrap().text.clone();
    app.focus_area(Area::Tree, &ctx);
    press_key(&mut app, &ctx, &mut time, Key::ArrowDown, Modifiers::NONE);
    wait_document(&mut app, &ctx, &mut time);
    assert_eq!(
        app.tabs.inactive[&c].editor.document.as_ref().unwrap().text,
        draft
    );
    assert_eq!(app.tabs.order.len(), 4);
    for _ in 0..10 {
        press_key(&mut app, &ctx, &mut time, Key::ArrowDown, Modifiers::NONE);
        assert_eq!(app.tabs.order.len(), 4);
    }
    // Images use the same keep/focus rules. Without an association, second Enter is a no-op.
    fs::remove_file(root.join("data/applications/viewer.desktop")).unwrap();
    press_key(&mut app, &ctx, &mut time, Key::End, Modifiers::NONE);
    let image = files.join("z.jpg");
    assert_eq!(app.tabs.preview.as_ref(), Some(&image));
    press_key(&mut app, &ctx, &mut time, Key::Enter, Modifiers::NONE);
    assert!(app.tabs.preview.is_none());
    assert!(!log.exists());
    press_key(&mut app, &ctx, &mut time, Key::Enter, Modifiers::NONE);
    let start = Instant::now();
    while app.opening_file.load(std::sync::atomic::Ordering::Relaxed)
        && start.elapsed() < Duration::from_secs(3)
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!log.exists());
    assert!(app.tabs.order.contains(&image));
    press_key(&mut app, &ctx, &mut time, Key::ArrowRight, Modifiers::NONE);
    assert!(app.keyboard.area == Area::Editor);
    assert_eq!(app.tabs.order.len(), 4);
}

#[test]
fn focus_terminal_toggle_close_and_keyboard_isolation_use_real_pty() {
    let Ok(root) = std::env::var("DISCO_TERMINAL_FIXTURE") else {
        let root =
            std::env::temp_dir().join(format!("disco-terminal-integrated-{}", std::process::id()));
        fs::create_dir_all(root.join("config")).unwrap();
        fs::create_dir_all(root.join("files")).unwrap();
        fs::write(root.join("files/example.rs"), "fn main() {}\n").unwrap();
        fs::write(
            root.join("config/layout.toml"),
            "preview_mode='focus'\nsave_scan=false\nauto_refresh=false\nanimations=false\n",
        )
        .unwrap();
        let result=Command::new(std::env::current_exe().unwrap()).args(["--exact","app::ui::interaction_tests::focus_terminal_toggle_close_and_keyboard_isolation_use_real_pty","--nocapture"])
            .env("DISCO_TERMINAL_FIXTURE",&root).env("DISCO_CONFIG_DIR",root.join("config")).env("XDG_DATA_HOME",root.join("data")).env("SHELL","/bin/sh").env("NO_COLOR","1").output().unwrap();
        fs::remove_dir_all(root).unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    };
    use crate::app::workspace_keyboard::Area;
    use egui::{Key, Modifiers};
    use egui_term::BackendCommand;
    let root = PathBuf::from(root);
    let files = root.join("files");
    let file = files.join("example.rs");
    let ctx = egui::Context::default();
    let mut time = 0.;
    let mut app = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    let report = crate::scan::scan(&files, &crate::scan::Control::default(), |_| {}).unwrap();
    app.scan.index = crate::index::Index::new(&report);
    app.scan.report = Some(std::sync::Arc::new(report));
    app.browser.root = files.display().to_string();
    app.browser.category = Category::All;
    app.select_file(file.clone(), 4096);
    wait_document(&mut app, &ctx, &mut time);
    let width_before = ctx
        .read_response(Id::unique("focus-document-panel"))
        .unwrap()
        .rect
        .width();
    assert!(!app.terminal.exists());
    click(&mut app, &ctx, &mut time, Id::unique("toggle-terminal"));
    let pid = app.terminal.pid().unwrap();
    assert_eq!(app.terminal.cwd, files);
    assert!(app.terminal_shown());
    assert!(app.keyboard.area == Area::Terminal);
    let width_with = ctx
        .read_response(Id::unique("focus-document-panel"))
        .unwrap()
        .rect
        .width();
    assert!(width_before - width_with > 290.);
    // Local application bindings are consumed before the focused PTY.
    let chord = Modifiers::CTRL | Modifiers::ALT;
    press_key(&mut app, &ctx, &mut time, Key::V, chord);
    assert_eq!(app.preferences.layout.preview_mode, PreviewMode::Overview);
    press_key(&mut app, &ctx, &mut time, Key::F, chord);
    assert_eq!(app.preferences.layout.preview_mode, PreviewMode::Focus);
    press_key(&mut app, &ctx, &mut time, Key::T, chord);
    assert!(!app.terminal_shown());
    press_key(&mut app, &ctx, &mut time, Key::T, chord);
    assert!(app.terminal_shown());
    assert_eq!(app.terminal.pid(), Some(pid));
    for _ in 0..3 {
        frame(&mut app, &ctx, &mut time, vec![]);
    }
    app.terminal
        .backend()
        .unwrap()
        .process_command(BackendCommand::Write(
            b"test -z ${NO_COLOR+x} && printf '\\103\\117\\114\\117\\122\\137\\117\\113\\n'\r"
                .to_vec(),
        ));
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        frame(&mut app, &ctx, &mut time, vec![]);
        let output: String = app
            .terminal
            .backend()
            .unwrap()
            .sync()
            .grid
            .display_iter()
            .map(|c| c.c)
            .collect();
        if output.contains("COLOR_OK") {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output: String = app
        .terminal
        .backend()
        .unwrap()
        .sync()
        .grid
        .display_iter()
        .map(|c| c.c)
        .collect();
    assert!(output.contains("COLOR_OK"));
    for (key, mods) in [
        (Key::W, Modifiers::CTRL),
        (Key::S, Modifiers::CTRL),
        (Key::N, Modifiers::CTRL),
        (Key::Delete, Modifiers::NONE),
        (Key::Enter, Modifiers::NONE),
    ] {
        press_key(&mut app, &ctx, &mut time, key, mods);
    }
    assert_eq!(app.editor.path, Some(file.clone()));
    assert!(app.editor.pending.is_none());
    assert!(app.new_file.is_none());
    assert!(!app.deletion.confirm);
    click(&mut app, &ctx, &mut time, Id::unique("toggle-terminal"));
    assert!(!app.terminal_shown());
    assert_eq!(app.terminal.pid(), Some(pid));
    click(&mut app, &ctx, &mut time, Id::unique("toggle-terminal"));
    assert!(app.terminal_shown());
    assert_eq!(app.terminal.pid(), Some(pid));
    // Overview docks below both the category rail and the list, while the
    // inspector stays the full height and uses the same running PTY.
    click(&mut app, &ctx, &mut time, Id::unique("file-mode-overview"));
    for _ in 0..3 {
        frame(&mut app, &ctx, &mut time, vec![]);
    }
    assert!(app.terminal_shown());
    assert_eq!(app.terminal.pid(), Some(pid));
    press_key(
        &mut app,
        &ctx,
        &mut time,
        Key::ArrowUp,
        Modifiers::CTRL | Modifiers::ALT,
    );
    assert!(app.keyboard.area == Area::Tree);
    press_key(
        &mut app,
        &ctx,
        &mut time,
        Key::ArrowDown,
        Modifiers::CTRL | Modifiers::ALT,
    );
    assert!(app.keyboard.area == Area::Terminal);
    let panel = Modifiers::CTRL | Modifiers::ALT;
    // The bottom terminal spans categories and tree. Left/down are boundaries,
    // right reaches properties; none of these chords may leak into the shell.
    app.focus_area(Area::Categories, &ctx);
    frame(&mut app, &ctx, &mut time, vec![]);
    press_key(&mut app, &ctx, &mut time, Key::ArrowDown, panel);
    assert!(app.keyboard.area == Area::Terminal);
    for key in [Key::ArrowLeft, Key::ArrowDown] {
        frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: panel,
            }],
        );
        assert!(
            !ctx.input(|i| i.events.iter().any(|event| matches!(event,
                egui::Event::Key { key: pressed_key, pressed: true, .. } if *pressed_key == key
            ))),
            "boundary navigation must be consumed before PTY input"
        );
        frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: panel,
            }],
        );
        assert!(app.keyboard.area == Area::Terminal);
        assert!(app.terminal_focused(&ctx));
    }
    press_key(&mut app, &ctx, &mut time, Key::ArrowRight, panel);
    assert!(app.keyboard.area == Area::Editor);
    press_key(&mut app, &ctx, &mut time, Key::ArrowRight, panel);
    assert!(app.keyboard.area == Area::Editor);
    press_key(&mut app, &ctx, &mut time, Key::ArrowLeft, panel);
    assert!(app.keyboard.area == Area::Terminal);
    let bounds = |name: &str| ctx.read_response(Id::unique(name)).unwrap().rect;
    let terminal = bounds("terminal-panel-bounds");
    let properties = bounds("properties-panel");
    let categories = bounds("category-panel-bounds");
    let browser = bounds("browser-panel-bounds");
    assert!((terminal.left() - categories.left()).abs() < 2.);
    assert!((terminal.right() - properties.left()).abs() < 3.);
    assert!(categories.bottom() <= terminal.top() + 3.);
    assert!(browser.bottom() <= terminal.top() + 3.);
    assert!(properties.bottom() > terminal.top() + 100.);
    assert!(properties.top() < terminal.top() - 100.);
    // Resizing only the lower-left area leaves inspector geometry untouched.
    let divider = egui::pos2(terminal.center().x, terminal.top() + 2.);
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::PointerMoved(divider)],
    );
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::PointerButton {
            pos: divider,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
    );
    let target = divider - egui::vec2(0., 35.);
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::PointerMoved(target)],
    );
    frame(
        &mut app,
        &ctx,
        &mut time,
        vec![egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    frame(&mut app, &ctx, &mut time, vec![]);
    assert!(
        bounds("terminal-panel-bounds").height() > terminal.height() + 20.,
        "before={:?}, after={:?}, preferred={:?}, divider={:?}",
        terminal,
        bounds("terminal-panel-bounds"),
        app.terminal.overview_height,
        ctx.read_response(Id::unique("overview-terminal-divider"))
    );
    assert_eq!(bounds("properties-panel"), properties);
    click(&mut app, &ctx, &mut time, Id::unique("hide-terminal"));
    for _ in 0..3 {
        frame(&mut app, &ctx, &mut time, vec![]);
    }
    assert!(!app.terminal_shown());
    assert_eq!(app.terminal.pid(), Some(pid));
    assert_eq!(bounds("properties-panel"), properties);
    assert!(
        bounds("category-panel-bounds").bottom() > categories.bottom() + 100.,
        "before={:?}, hidden={:?}",
        categories,
        bounds("category-panel-bounds")
    );
    click(&mut app, &ctx, &mut time, Id::unique("toggle-terminal"));
    assert!(app.terminal_shown());
    assert_eq!(app.terminal.pid(), Some(pid));
    assert_eq!(bounds("properties-panel"), properties);
    click(&mut app, &ctx, &mut time, Id::unique("file-mode-focus"));
    assert!(app.terminal_shown());
    assert_eq!(app.terminal.pid(), Some(pid));
    // Ctrl+S above is intentionally passed to the PTY (XOFF). Resume output
    // before asking the interactive shell to draw/read its next command.
    app.terminal
        .backend()
        .unwrap()
        .process_command(BackendCommand::Write(vec![17]));
    std::thread::sleep(Duration::from_millis(30));
    let jobfile = root.join("job.pid");
    let command = format!(
        "set -m; sh -c 'trap {q}{q} HUP TERM; echo $$ > {path}; sleep 60' &{cr}",
        q = '"',
        path = jobfile.display(),
        cr = char::from(13)
    );
    app.terminal
        .backend()
        .unwrap()
        .process_command(BackendCommand::Write(command.into_bytes()));
    let start = Instant::now();
    while !jobfile.exists() && start.elapsed() < Duration::from_secs(3) {
        frame(&mut app, &ctx, &mut time, vec![]);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        jobfile.exists(),
        "background job not started: {}",
        app.terminal
            .backend()
            .unwrap()
            .sync()
            .grid
            .display_iter()
            .map(|c| c.c)
            .collect::<String>()
    );
    let job: u32 = fs::read_to_string(&jobfile)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    click(&mut app, &ctx, &mut time, Id::unique("close-terminal"));
    assert!(!app.terminal.exists());
    assert!(!app.terminal_shown());
    frame(&mut app, &ctx, &mut time, vec![]);
    assert!(
        (ctx.read_response(Id::unique("focus-document-panel"))
            .unwrap()
            .rect
            .width()
            - width_before)
            .abs()
            < 2.
    );
    let running = |pid| {
        fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .is_some_and(|s| !s.rsplit_once(')').unwrap().1.trim_start().starts_with('Z'))
    };
    let start = Instant::now();
    while (running(pid) || running(job)) && start.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!running(pid));
    assert!(!running(job));
    click(&mut app, &ctx, &mut time, Id::unique("toggle-terminal"));
    assert_ne!(app.terminal.pid(), Some(pid));
    click(&mut app, &ctx, &mut time, Id::unique("close-terminal"));
    click(&mut app, &ctx, &mut time, Id::unique("file-mode-overview"));
    click(&mut app, &ctx, &mut time, Id::unique("toggle-terminal"));
    assert!(app.terminal_shown());
    let second = app.terminal.pid().unwrap();
    click(&mut app, &ctx, &mut time, Id::unique("close-terminal"));
    assert!(!app.terminal.exists());
    let start = Instant::now();
    while running(second) && start.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!running(second));
}

#[test]
fn window_background_is_opaque_across_layouts_sizes_and_fractional_dpi() {
    let Ok(root) = std::env::var("DISCO_OPAQUE_FIXTURE") else {
        let root = std::env::temp_dir().join(format!("disco-opaque-{}", std::process::id()));
        fs::create_dir_all(root.join("config")).unwrap();
        fs::write(
            root.join("config/layout.toml"),
            "save_scan=false\nauto_refresh=false\n",
        )
        .unwrap();
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "app::ui::interaction_tests::window_background_is_opaque_across_layouts_sizes_and_fractional_dpi", "--nocapture"])
            .env("DISCO_OPAQUE_FIXTURE", &root)
            .env("DISCO_CONFIG_DIR", root.join("config"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("XDG_DATA_HOME", root.join("data"))
            .output().unwrap();
        fs::remove_dir_all(root).unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    };
    let _root = root;
    let ctx = egui::Context::default();
    let mut app = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    app.demo = true;
    let mut time = 0.;
    for scale in [1., 1.25, 1.5, 2.] {
        ctx.set_pixels_per_point(scale);
        for mode in [
            PreviewMode::Overview,
            PreviewMode::Focus,
            PreviewMode::Overview,
        ] {
            app.preferences.layout.preview_mode = mode;
            for size in [Vec2::new(1360., 800.), Vec2::new(1140., 620.)] {
                let screen = Rect::from_min_size(Pos2::ZERO, size);
                time += 0.05;
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        time: Some(time),
                        ..Default::default()
                    },
                    |ui| eframe::App::ui(&mut app, ui, &mut eframe::Frame::_new_kittest()),
                );
                assert_eq!(
                    eframe::App::clear_color(&app, &ctx.global_style().visuals)[3],
                    1.
                );
                // An opaque root must precede all panel content. Testing only
                // panel interiors missed the unpainted strip after the titlebar.
                let background = crate::settings::color(&app.preferences.theme.background);
                let first_fill = output
                    .shapes
                    .iter()
                    .find_map(|clipped| match &clipped.shape {
                        egui::epaint::Shape::Rect(rect) if rect.fill.a() > 0 => {
                            Some((clipped.clip_rect, rect.clone()))
                        }
                        _ => None,
                    })
                    .expect("root background paint");
                output.drop_without_applying_deltas();
                // set_pixels_per_point may rescale the first RawInput rect in
                // the transition frame; compare to egui's actual viewport.
                let screen = ctx.viewport_rect();
                assert_eq!(first_fill.1.fill, background);
                assert_eq!(first_fill.1.fill.a(), 255);
                assert!(
                    first_fill.1.rect.contains_rect(screen),
                    "root must cover seams during resize at {scale}x"
                );
                assert!(first_fill.0.contains_rect(screen));
            }
        }
    }
}

#[test]
fn trash_operations_and_f5_refresh_only_trash_without_replacing_the_scan() {
    let Ok(root) = std::env::var("DISCO_TRASH_SCOPE_FIXTURE") else {
        let root = std::env::temp_dir().join(format!("disco-trash-scope-{}", std::process::id()));
        fs::create_dir_all(root.join("config")).unwrap();
        fs::create_dir_all(root.join("files/unrelated")).unwrap();
        fs::write(root.join("files/unrelated/keep.txt"), b"snapshot must stay").unwrap();
        fs::write(
            root.join("config/layout.toml"),
            "save_scan=false\nauto_refresh=false\nanimations=false\nlanguage='en'\n",
        )
        .unwrap();
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "app::ui::interaction_tests::trash_operations_and_f5_refresh_only_trash_without_replacing_the_scan", "--nocapture"])
            .env("DISCO_TRASH_SCOPE_FIXTURE", &root)
            .env("DISCO_CONFIG_DIR", root.join("config"))
            .env("XDG_DATA_HOME", root.join("data"))
            .output().unwrap();
        fs::remove_dir_all(root).unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    };
    use crate::recycle::Action;
    use std::sync::Arc;
    let root = PathBuf::from(root);
    let ctx = egui::Context::default();
    let mut app = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    let files = root.join("files");
    let report =
        Arc::new(crate::scan::scan(&files, &crate::scan::Control::default(), |_| {}).unwrap());
    app.scan.index = crate::index::Index::new(&report);
    app.scan.report = Some(report.clone());
    app.browser.root = files.display().to_string();
    app.browser.view = crate::app::state::View::Trash;
    // If a full rescan is accidentally triggered, this branch disappears.
    fs::remove_dir_all(files.join("unrelated")).unwrap();
    for name in ["purge-me", "restore-me"] {
        fs::write(root.join(name), vec![1u8; 8192]).unwrap();
        trash::delete(root.join(name)).unwrap();
    }
    let mut time = 0.;
    let wait = |app: &mut Disco, time: &mut f64| {
        let started = Instant::now();
        loop {
            frame(app, &ctx, time, vec![]);
            assert!(!app.scan.busy, "Trash must never launch a root scan");
            assert!(
                !app.scoped.busy(),
                "Trash must not refresh a folder/category"
            );
            assert!(Arc::ptr_eq(app.scan.report.as_ref().unwrap(), &report));
            assert_eq!(app.scan.report_revision, 0);
            assert!(app.editor.pending.is_none());
            if !app.recycle.busy && !app.recycle.loading {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    app.recycle.refresh(&ctx);
    wait(&mut app, &mut time);
    let owned = |app: &Disco, name: &str| {
        app.recycle
            .items
            .iter()
            .find(|i| i.entry.name == name && i.stored.starts_with(root.join("data/Trash/files")))
            .unwrap()
            .clone()
    };
    let purge = owned(&app, "purge-me");
    let before = app.recycle.total().0;
    app.recycle.selected.insert(purge.entry.id.clone());
    app.recycle.start(vec![purge.clone()], Action::Purge, &ctx);
    wait(&mut app, &mut time);
    assert!(!purge.stored.exists());
    assert!(
        !app.recycle
            .items
            .iter()
            .any(|i| i.entry.id == purge.entry.id)
    );
    assert!(app.recycle.selected.is_empty());
    assert!(app.recycle.total().0 < before);
    let restore = owned(&app, "restore-me");
    // A failed restore still rereads Trash, keeps its item, and must not turn
    // a fully readable listing into an 'unavailable' disk-bar summary.
    fs::write(root.join("restore-me"), b"collision").unwrap();
    app.recycle
        .start(vec![restore.clone()], Action::Restore, &ctx);
    wait(&mut app, &mut time);
    assert!(!app.recycle.error.is_empty());
    assert!(app.recycle.listing_complete());
    assert!(app.recycle.total().1);
    assert!(restore.stored.exists());
    fs::remove_file(root.join("restore-me")).unwrap();
    app.recycle
        .start(vec![restore.clone()], Action::Restore, &ctx);
    wait(&mut app, &mut time);
    assert_eq!(fs::read(root.join("restore-me")).unwrap(), vec![1u8; 8192]);
    assert!(
        !app.recycle
            .items
            .iter()
            .any(|i| i.entry.id == restore.entry.id)
    );
    assert!(app.recycle.error.is_empty());
    // A new external arrival is read by F5 in the Trash scope only.
    fs::write(root.join("external"), b"arrived later").unwrap();
    trash::delete(root.join("external")).unwrap();
    app.focus_area(crate::app::workspace_keyboard::Area::Tree, &ctx);
    frame(&mut app, &ctx, &mut time, vec![]);
    press_key(
        &mut app,
        &ctx,
        &mut time,
        egui::Key::F5,
        egui::Modifiers::NONE,
    );
    wait(&mut app, &mut time);
    let external = owned(&app, "external");
    // Empty the fixture's remaining confirmed set, with automatic rereading.
    app.recycle
        .start(vec![external.clone()], Action::Purge, &ctx);
    wait(&mut app, &mut time);
    assert!(!external.stored.exists());
    assert!(
        !app.recycle
            .items
            .iter()
            .any(|i| i.stored.starts_with(root.join("data/Trash/files")))
    );
    assert!(app.recycle.listing_complete());
    if app.recycle.items.is_empty() {
        app.recycle.error = "fixture: prior action failed".into();
        time += 0.05;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1360., 800.))),
                time: Some(time),
                ..Default::default()
            },
            |ui| eframe::App::ui(&mut app, ui, &mut eframe::Frame::_new_kittest()),
        );
        output.textures_delta.clear();
        assert!(
            !output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::epaint::Shape::Text(text) if text.galley.text() == "Trash: unavailable"
            )),
            "a readable empty bin must not inherit an action error as listing failure"
        );
        output.drop_without_applying_deltas();
    }
}

#[test]
fn title_controls_are_compact_by_default_and_labels_can_be_restored() {
    let ctx = egui::Context::default();
    let mut app = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    app.demo = true;
    let mut time = 0.;
    for labels in [false, true] {
        app.preferences.layout.title_button_labels = labels;
        for _ in 0..3 {
            frame(&mut app, &ctx, &mut time, vec![]);
        }
        let overview = ctx
            .read_response(Id::unique("file-mode-overview"))
            .unwrap()
            .rect;
        let focus = ctx
            .read_response(Id::unique("file-mode-focus"))
            .unwrap()
            .rect;
        let terminal = ctx
            .read_response(Id::unique("toggle-terminal"))
            .unwrap()
            .rect;
        assert!(overview.right() < focus.left());
        assert!(focus.right() < terminal.left());
        assert_eq!(overview.height(), terminal.height());
        assert_eq!(overview.width(), if labels { 108. } else { 36. });
        assert_eq!(terminal.width(), if labels { 112. } else { 36. });
    }
    let defaults: crate::settings::Layout = toml::from_str("").unwrap();
    assert!(!defaults.title_button_labels);
    let saved = toml::to_string(&app.preferences.layout).unwrap();
    let restored: crate::settings::Layout = toml::from_str(&saved).unwrap();
    assert!(restored.title_button_labels);
}
