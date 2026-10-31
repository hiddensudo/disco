//! Restore the saved workspace only when leaving a directly opened document.
//! This worker never scans and never owns the current document/category state.
use super::Disco;
use crate::{index::Index, snapshot::SavedScan};
use eframe::egui;
use std::sync::{
    Arc,
    mpsc::{self, Receiver},
};
type Loaded = Result<Option<(SavedScan, Index)>, String>;
#[derive(Default)]
pub(super) struct Cache {
    pub needed: bool,
    pub rx: Option<Receiver<Loaded>>,
}
impl Cache {
    pub fn loading(&self) -> bool {
        self.needed || self.rx.is_some()
    }
    pub fn cancel(&mut self) {
        self.needed = false;
        self.rx = None;
    }
}
impl Disco {
    pub(in crate::app) fn poll_path_snapshot(&mut self, ctx: &egui::Context) {
        if self.scan.busy || self.scan.report.is_some() || self.paths.standalone.is_some() {
            self.paths.snapshot.cancel();
            return;
        }
        if self.scan.restore_requested_root.is_some() {
            // The ordinary startup worker is already reading this same file.
            self.paths.snapshot.needed = false;
            return;
        }
        if self.paths.snapshot.needed && self.paths.snapshot.rx.is_none() {
            self.paths.snapshot.needed = false;
            let (tx, rx) = mpsc::channel();
            self.paths.snapshot.rx = Some(rx);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                crate::scan::background_priority();
                let result = crate::snapshot::load().map(|saved| {
                    saved.map(|saved| {
                        let index = Index::new(&saved.report);
                        (saved, index)
                    })
                });
                let _ = tx.send(result);
                ctx.request_repaint();
            });
            return;
        }
        let result = self
            .paths
            .snapshot
            .rx
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(crate::tr!(
                    "Не вдалося прочитати збережений знімок",
                    "Could not read the saved snapshot"
                )
                .into())),
            });
        let Some(result) = result else { return };
        self.paths.snapshot.rx = None;
        match result {
            Ok(Some((saved, index))) => {
                let root = saved.report.folders[0].path.clone();
                self.scan.index = index;
                self.scan.report = Some(Arc::new(saved.report));
                self.scan.report_revision = self.scan.report_revision.wrapping_add(1);
                // Folder metadata changed; the open file itself did not. Keep
                // its decoded preview visible instead of flashing/reloading it.
                self.editor.revision = self.scan.report_revision;
                self.file_preview.adopt_revision(self.scan.report_revision);
                self.scan.saved_snapshot_at = Some(saved.saved_at);
                self.scan.restored_snapshot = true;
                self.scan.tracker = None;
                self.collections.indexed = None;
                self.update_collections();
                self.browser.rows_cache = None;
                self.browser.tree_revision = self.browser.tree_revision.wrapping_add(1);
                if let Some(directory) = self.paths.directory.as_ref() {
                    if let Some(id) = self
                        .scan
                        .report
                        .as_ref()
                        .unwrap()
                        .folders
                        .iter()
                        .position(|f| &f.path == directory)
                    {
                        self.paths.directory = None;
                        self.browser.table_root = Some(id);
                        self.expand_folder(id);
                    }
                } else {
                    self.browser.root = root.display().to_string();
                    // Apply the latest selected category, including choices made while loading.
                    if let Some(id) = self
                        .scan
                        .index
                        .roots(self.browser.category)
                        .first()
                        .copied()
                    {
                        self.expand_folder(id);
                    }
                }
            }
            Ok(None) => {}
            Err(error) => {
                self.message = crate::tr!(
                    "Не вдалося відновити знімок: {error}",
                    "Could not restore the snapshot: {error}",
                    error = error
                )
            }
        }
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::workspace_keyboard::Area, categories::Collection};
    use egui::{Id, Key, Modifiers, Pos2, Rect, Vec2};
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
            |ui| {
                eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest());
            },
        )
        .drop_without_applying_deltas();
    }
    fn wait(app: &mut Disco, ctx: &egui::Context, time: &mut f64, ready: impl Fn(&Disco) -> bool) {
        let end = Instant::now() + Duration::from_secs(5);
        while !ready(app) && Instant::now() < end {
            frame(app, ctx, time, vec![]);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(ready(app), "{}", app.message);
        frame(app, ctx, time, vec![]);
    }
    fn click(app: &mut Disco, ctx: &egui::Context, time: &mut f64, id: Id) {
        let pos = ctx
            .read_response(id)
            .expect("visible control")
            .rect
            .center();
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
                    modifiers: Modifiers::NONE,
                }],
            );
        }
        frame(app, ctx, time, vec![]);
    }
    fn category(app: &mut Disco, name: &str, path: &Path) -> crate::index::Category {
        let c = Collection {
            id: name.into(),
            name: name.into(),
            icon: "monitor".into(),
            folders: vec![path.into()],
            auto_refresh: false,
            read_only: false,
            auto_save: false,
        };
        let key = c.category();
        app.collections.config.items.push(c);
        app.collections.revision += 1;
        key
    }
    #[test]
    fn cli_categories_restore_saved_sizes_keep_drafts_and_offer_a_scoped_empty_action() {
        let Ok(root) = std::env::var("DISCO_LAZY_SNAPSHOT_FIXTURE") else {
            let root =
                std::env::temp_dir().join(format!("disco-lazy-snapshot-{}", std::process::id()));
            for dir in [
                "config",
                "loose",
                "workspace/practice/sub",
                "workspace/second",
            ] {
                fs::create_dir_all(root.join(dir)).unwrap();
            }
            fs::write(root.join("loose/README.md"), "# Original\n").unwrap();
            fs::write(
                root.join("workspace/practice/sub/example.rs"),
                "fn main() {}\n",
            )
            .unwrap();
            fs::write(root.join("workspace/second/notes.txt"), "notes\n").unwrap();
            fs::write(root.join("config/layout.toml"),"language='en'\ntext_scale_version=1\nsave_scan=true\nauto_refresh=false\nanimations=false\n").unwrap();
            let out=Command::new(std::env::current_exe().unwrap()).args(["--exact","app::path_snapshot::tests::cli_categories_restore_saved_sizes_keep_drafts_and_offer_a_scoped_empty_action","--nocapture"])
                .env("DISCO_LAZY_SNAPSHOT_FIXTURE",&root).env("DISCO_CONFIG_DIR",root.join("config")).env("XDG_DATA_HOME",root.join("data")).output().unwrap();
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
        let report =
            crate::scan::scan(&root.join("workspace"), &Default::default(), |_| {}).unwrap();
        crate::snapshot::save(&report).unwrap();
        let stamp = fs::metadata(crate::snapshot::file())
            .unwrap()
            .modified()
            .unwrap();
        let source = root.join("loose/README.md");
        let ctx = egui::Context::default();
        let mut time = 0.;
        let mut app = Disco::new_with_path(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            Some(source.clone()),
        );
        let desktop = category(
            &mut app,
            "desktop-fixture",
            &root.join("workspace/practice"),
        );
        let second = category(&mut app, "second-fixture", &root.join("workspace/second"));
        wait(&mut app, &ctx, &mut time, |a| a.editor.document.is_some());
        assert!(
            app.scan.report.is_none(),
            "CLI opening alone must stay lightweight"
        );
        app.editor.markdown.editing = true;
        app.focus_area(Area::Editor, &ctx);
        frame(&mut app, &ctx, &mut time, vec![]);
        frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::Paste("draft ".into())],
        );
        let draft = app.editor.document.as_ref().unwrap().text.clone();
        assert!(app.editor.dirty());
        app.editor.zoom.factor = 1.4;
        let editor_id = Id::unique(("file-editor", &app.editor.path));
        let cursor = egui::TextEdit::load_state(&ctx, editor_id)
            .unwrap()
            .cursor
            .char_range();
        click(
            &mut app,
            &ctx,
            &mut time,
            Id::unique(("category", "desktop-fixture")),
        );
        wait(&mut app, &ctx, &mut time, |a| a.scan.report.is_some());
        assert_eq!(app.browser.category, desktop);
        assert!(!app.scan.busy && !app.scoped.busy());
        assert!(app.scan.rx.is_none());
        assert!(app.scan.saved_snapshot_at.is_some());
        assert!(!app.scan.index.roots(desktop).is_empty());
        assert_eq!(app.editor.path.as_ref(), Some(&source));
        assert_eq!(app.editor.document.as_ref().unwrap().text, draft);
        assert_eq!(app.editor.zoom.factor, 1.4);
        assert_eq!(
            egui::TextEdit::load_state(&ctx, editor_id)
                .unwrap()
                .cursor
                .char_range(),
            cursor
        );
        assert_eq!(
            fs::metadata(crate::snapshot::file())
                .unwrap()
                .modified()
                .unwrap(),
            stamp
        );
        let cached = app.scan.report.clone().unwrap();
        click(
            &mut app,
            &ctx,
            &mut time,
            Id::unique(("category", "second-fixture")),
        );
        assert_eq!(app.browser.category, second);
        assert!(Arc::ptr_eq(&cached, app.scan.report.as_ref().unwrap()));
        app.focus_area(Area::Editor, &ctx);
        frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::Key {
                key: Key::Z,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::CTRL | Modifiers::COMMAND,
            }],
        );
        assert_eq!(app.editor.document.as_ref().unwrap().text, "# Original\n");
        assert_eq!(fs::read_to_string(&source).unwrap(), "# Original\n");

        // A category chosen before normal startup restore finishes also survives.
        let mut normal = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let selected = category(
            &mut normal,
            "startup-fixture",
            &root.join("workspace/practice"),
        );
        normal.select_collection_keyboard("startup-fixture");
        wait(&mut normal, &ctx, &mut time, |a| a.scan.report.is_some());
        assert_eq!(normal.browser.category, selected);
        assert!(!normal.scan.index.roots(selected).is_empty());
        assert!(!normal.paths.snapshot.loading());

        let saved = crate::snapshot::load().unwrap().unwrap();
        let index = Index::new(&saved.report);
        fs::remove_file(crate::snapshot::file()).unwrap();
        let mut empty = Disco::new_with_path(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            Some(source.clone()),
        );
        category(
            &mut empty,
            "empty-fixture",
            &root.join("workspace/practice"),
        );
        wait(&mut empty, &ctx, &mut time, |a| a.editor.document.is_some());
        empty.select_collection_keyboard("empty-fixture");
        wait(&mut empty, &ctx, &mut time, |a| !a.paths.snapshot.loading());
        assert!(empty.scan.report.is_none());
        let action = ctx
            .read_response(Id::unique("folder-empty-action"))
            .unwrap()
            .rect;
        let description = ctx
            .read_response(Id::unique("folder-empty-description"))
            .unwrap()
            .rect;
        let tree = ctx
            .read_response(Id::unique("focus-tree-panel"))
            .unwrap()
            .rect;
        assert!(action.top() > description.bottom() + 10.);
        assert!(tree.contains_rect(action) && tree.contains_rect(description));
        assert_eq!(
            ctx.read_response(Id::unique("folder-refresh-bounds"))
                .unwrap()
                .rect
                .width(),
            0.
        );
        // Hold a real channel pending: loading has a small indicator and a late
        // cache result cannot change a newly opened standalone document.
        let (tx, rx) = mpsc::channel();
        empty.paths.snapshot.rx = Some(rx);
        frame(&mut empty, &ctx, &mut time, vec![]);
        assert!(
            ctx.read_response(Id::unique("folder-loading-indicator"))
                .unwrap()
                .rect
                .width()
                <= 16.
        );
        empty.open_resolved_path(
            crate::path_input::Resolved {
                path: source.clone(),
                directory: false,
                bytes: 10,
            },
            &ctx,
        );
        assert!(tx.send(Ok(Some((saved, index)))).is_err());
        assert!(!empty.paths.snapshot.loading());
        assert!(empty.scan.report.is_none());
        empty.select_collection_keyboard("empty-fixture");
        wait(&mut empty, &ctx, &mut time, |a| !a.paths.snapshot.loading());
        click(
            &mut empty,
            &ctx,
            &mut time,
            Id::unique("folder-empty-action"),
        );
        wait(&mut empty, &ctx, &mut time, |a| {
            !a.scoped.busy() && a.scan.report.is_some()
        });
        assert!(empty.scan.report.as_ref().unwrap().complete);
        assert!(
            crate::snapshot::file().exists(),
            "explicit scanning creates a reusable cache"
        );
        assert_eq!(
            empty.scan.report.as_ref().unwrap().folders[0].path,
            root.join("workspace/practice")
        );
    }
}
