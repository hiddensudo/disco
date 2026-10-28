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
    frame(&mut app, &ctx, &mut time, vec![]);
    click(
        &mut app,
        &ctx,
        &mut time,
        Id::unique("settings-save-bounds"),
    );
    assert!(!app.preferences.open);
    assert!(app.editor.dirty());
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
