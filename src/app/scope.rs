use super::{Disco, state::View};
use crate::{
    filesystem::Properties,
    index::{Category, Index, RefreshPaths},
    scan::{Control, Report},
    scoped_refresh,
};
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
};
struct Finished {
    revision: u64,
    report: Arc<Report>,
    index: Index,
    saved: Option<u64>,
    id_map: Vec<Option<usize>>,
    roots: Vec<PathBuf>,
    shallow: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        process::Command,
        time::{Duration, Instant},
    };
    fn tick(app: &mut Disco, ctx: &egui::Context, time: &mut f64, events: Vec<egui::Event>) {
        *time += 0.05;
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1360., 800.),
                )),
                time: Some(*time),
                events,
                ..Default::default()
            },
            |ui| eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest()),
        )
        .drop_without_applying_deltas();
    }
    fn wait(app: &mut Disco, ctx: &egui::Context, time: &mut f64, ready: impl Fn(&Disco) -> bool) {
        let start = Instant::now();
        while !ready(app) && start.elapsed() < Duration::from_secs(6) {
            tick(app, ctx, time, vec![]);
            std::thread::sleep(Duration::from_millis(15));
        }
        assert!(
            ready(app),
            "message={} editor={} refresh={}",
            app.message,
            app.editor.error,
            app.scoped.busy()
        );
    }
    #[test]
    fn scoped_refresh_live_updates_and_read_only_preserve_drafts_and_other_categories() {
        let Ok(root) = std::env::var("DISCO_SCOPE_FIXTURE") else {
            let root = std::env::temp_dir().join(format!("disco-scope-ui-{}", std::process::id()));
            fs::create_dir_all(root.join("config")).unwrap();
            fs::create_dir_all(root.join("files/a")).unwrap();
            fs::create_dir_all(root.join("files/b")).unwrap();
            fs::write(root.join("files/a/config.rs"), "fn before() {}\n").unwrap();
            fs::write(root.join("files/b/stay.txt"), vec![1u8; 9000]).unwrap();
            fs::write(root.join("config/layout.toml"),"save_scan=false\nauto_refresh=false\nsearch_home_fallback=false\npreview_mode='focus'\nanimations=false\nlanguage='en'\n").unwrap();
            let output=Command::new(std::env::current_exe().unwrap()).args(["--exact","app::scope::tests::scoped_refresh_live_updates_and_read_only_preserve_drafts_and_other_categories","--nocapture"])
                .env("DISCO_SCOPE_FIXTURE",&root).env("DISCO_CONFIG_DIR",root.join("config")).env("XDG_DATA_HOME",root.join("data")).output().unwrap();
            fs::remove_dir_all(root).unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        };
        let root = PathBuf::from(root);
        let files = root.join("files");
        let a = files.join("a");
        let file = a.join("config.rs");
        let ctx = egui::Context::default();
        let mut time = 0.;
        let mut app = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let report = crate::scan::scan(&files, &Control::default(), |_| {}).unwrap();
        let other = report
            .folders
            .iter()
            .find(|f| f.path == files.join("b"))
            .unwrap()
            .bytes;
        let a_id = report.folders.iter().position(|f| f.path == a).unwrap();
        app.scan.index = Index::new(&report);
        app.scan.report = Some(Arc::new(report));
        app.browser.root = files.display().to_string();
        app.collections
            .config
            .items
            .iter_mut()
            .find(|c| c.id == "documents")
            .unwrap()
            .folders = vec![a.clone()];
        app.collections.indexed = None;
        app.update_collections();
        app.browser.category = Category::Documents;
        app.enter_folder(a_id);
        app.select_file(file.clone(), 4096);
        wait(&mut app, &ctx, &mut time, |app| {
            app.editor.document.is_some()
        });
        fs::remove_dir_all(files.join("b")).unwrap();
        fs::write(&file, "fn external() {}\n").unwrap();
        app.refresh_scope(&ctx);
        wait(&mut app, &ctx, &mut time, |app| {
            !app.scoped.busy()
                && app
                    .editor
                    .document
                    .as_ref()
                    .is_some_and(|d| d.text.contains("external"))
        });
        assert_eq!(
            app.scan
                .report
                .as_ref()
                .unwrap()
                .folders
                .iter()
                .find(|f| f.path == files.join("b"))
                .unwrap()
                .bytes,
            other
        );
        assert_eq!(app.browser.root, files.display().to_string());
        app.editor
            .document
            .as_mut()
            .unwrap()
            .text
            .push_str("// unsaved\n");
        let draft = app.editor.document.as_ref().unwrap().text.clone();
        fs::write(&file, "fn newer() {}\n").unwrap();
        app.refresh_scope(&ctx);
        wait(&mut app, &ctx, &mut time, |app| !app.scoped.busy());
        assert_eq!(app.editor.document.as_ref().unwrap().text, draft);
        assert!(app.editor.dirty() && app.editor.pending.is_none());
        app.editor.document = None;
        app.file_preview.reload();
        wait(&mut app, &ctx, &mut time, |app| {
            app.editor.document.is_some()
        });
        app.collections
            .config
            .items
            .iter_mut()
            .find(|c| c.id == "documents")
            .unwrap()
            .read_only = true;
        app.editor.error.clear();
        let before = app.editor.document.as_ref().unwrap().text.clone();
        ctx.memory_mut(|m| m.request_focus(egui::Id::unique(("file-editor", Some(file.clone())))));
        tick(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::Paste("forbidden".into())],
        );
        assert_eq!(app.editor.document.as_ref().unwrap().text, before);
        app.request_new_file(&ctx);
        assert!(app.new_file.is_none());
        app.request_delete(vec![(file.clone(), 4096)]);
        assert!(!app.deletion.confirm);
        app.save_editor(&ctx);
        assert!(!app.editor.error.is_empty());
        assert_eq!(fs::read_to_string(&file).unwrap(), before);
        {
            let c = app
                .collections
                .config
                .items
                .iter_mut()
                .find(|c| c.id == "documents")
                .unwrap();
            c.read_only = false;
            c.auto_refresh = true;
        }
        app.editor.error.clear();
        let revision = app.scan.report_revision;
        wait(&mut app, &ctx, &mut time, |app| {
            app.live.ready() && app.scan.report_revision > revision && !app.scoped.busy()
        });
        fs::write(a.join("new.txt"), "new").unwrap();
        fs::write(&file, "fn watched() {}\n").unwrap();
        wait(&mut app, &ctx, &mut time, |app| {
            app.editor
                .document
                .as_ref()
                .is_some_and(|d| d.text.contains("watched"))
                && app
                    .scan
                    .report
                    .as_ref()
                    .unwrap()
                    .folders
                    .iter()
                    .find(|f| f.path == a)
                    .unwrap()
                    .files
                    == 2
        });
        fs::create_dir_all(a.join("new/deep")).unwrap();
        let nested = a.join("new/deep/code.ts");
        fs::write(&nested, "const x = 1;\n").unwrap();
        wait(&mut app, &ctx, &mut time, |app| {
            app.scan
                .report
                .as_ref()
                .unwrap()
                .folders
                .iter()
                .any(|f| f.path == a.join("new/deep"))
        });
        let before = app
            .scan
            .report
            .as_ref()
            .unwrap()
            .folders
            .iter()
            .find(|f| f.path == a.join("new/deep"))
            .unwrap()
            .bytes;
        fs::write(&nested, vec![b'x'; 17000]).unwrap();
        wait(&mut app, &ctx, &mut time, |app| {
            app.scan
                .report
                .as_ref()
                .unwrap()
                .folders
                .iter()
                .find(|f| f.path == a.join("new/deep"))
                .is_some_and(|f| f.bytes > before)
        });
        app.editor
            .document
            .as_mut()
            .unwrap()
            .text
            .push_str("// keep me\n");
        let draft = app.editor.document.as_ref().unwrap().text.clone();
        fs::write(&file, "fn changed_again() {}\n").unwrap();
        wait(&mut app, &ctx, &mut time, |app| {
            !app.editor.error.is_empty()
        });
        assert_eq!(app.editor.document.as_ref().unwrap().text, draft);
        wait(&mut app, &ctx, &mut time, |app| !app.scoped.busy());
        let revision = app.scan.report_revision;
        fs::remove_file(&file).unwrap();
        wait(&mut app, &ctx, &mut time, |app| {
            app.scan.report_revision > revision && !app.scoped.busy()
        });
        assert_eq!(app.editor.document.as_ref().unwrap().text, draft);
        assert!(
            app.editor.pending.is_none(),
            "external deletion must not interrupt editing with a modal"
        );
        app.collections
            .config
            .items
            .iter_mut()
            .find(|c| c.id == "documents")
            .unwrap()
            .auto_refresh = false;
        tick(&mut app, &ctx, &mut time, vec![]);
        let revision = app.scan.report_revision;
        fs::write(a.join("not_watched.txt"), "off").unwrap();
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(1000) {
            tick(&mut app, &ctx, &mut time, vec![]);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            app.scan.report_revision, revision,
            "disabled categories perform no background refresh"
        );
        // A category can start with multiple roots and no global snapshot.
        // Reading their shared ancestor would include this unrelated directory.
        fs::create_dir_all(root.join("external")).unwrap();
        fs::create_dir_all(root.join("unrelated")).unwrap();
        fs::write(root.join("external/extra.txt"), "extra").unwrap();
        let mut fresh = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        fresh
            .collections
            .config
            .items
            .iter_mut()
            .find(|c| c.id == "documents")
            .unwrap()
            .folders = vec![a.clone(), root.join("external")];
        fresh.browser.category = Category::Documents;
        fresh.refresh_scope(&ctx);
        wait(&mut fresh, &ctx, &mut time, |app| !app.scoped.busy());
        assert!(
            fresh
                .scan
                .report
                .as_ref()
                .unwrap()
                .folders
                .iter()
                .any(|f| f.path == root.join("external"))
        );
        assert!(
            !fresh
                .scan
                .report
                .as_ref()
                .unwrap()
                .folders
                .iter()
                .any(|f| f.path == root.join("unrelated"))
        );
        fresh
            .collections
            .config
            .items
            .iter_mut()
            .find(|c| c.id == "documents")
            .unwrap()
            .auto_refresh = true;
        tick(&mut fresh, &ctx, &mut time, vec![]);
        fresh.scoped.mark_dirty(a.clone());
        {
            let c = fresh
                .collections
                .config
                .items
                .iter_mut()
                .find(|c| c.id == "games")
                .unwrap();
            c.auto_refresh = true;
            c.folders = vec![root.join("external")];
        }
        fresh.collections.indexed = None;
        fresh.browser.category = Category::Games;
        tick(&mut fresh, &ctx, &mut time, vec![]);
        assert!(
            !fresh.scoped.dirty.contains(&a),
            "switching scopes discards old queued directories"
        );
    }
}
#[derive(Default)]
pub(super) struct Scope {
    rx: Option<Receiver<Result<Finished, String>>>,
    control: Arc<Control>,
    dirty: std::collections::HashSet<PathBuf>,
    full: bool,
    plan: Option<((Category, Option<usize>, u64, u64), Arc<Vec<PathBuf>>)>,
    home: Option<(String, PathBuf)>,
    last_auto: Option<std::time::Instant>,
    watch_error: Option<String>,
}
impl Scope {
    fn mark_dirty(&mut self, path: PathBuf) {
        if self.dirty.len() < 2048 {
            self.dirty.insert(path);
        } else {
            self.full = true;
        }
    }
    pub fn cancel(&self) {
        self.control
            .cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
    pub fn busy(&self) -> bool {
        self.rx.is_some()
    }
}
impl Disco {
    pub(in crate::app) fn scope_filter(&self) -> Option<Category> {
        self.collections
            .config
            .get(self.browser.category)
            .filter(|c| c.builtin() && c.folders.is_empty())
            .map(|c| c.category())
    }
    pub(in crate::app) fn category_read_only(&self) -> bool {
        self.collections
            .config
            .get(self.browser.category)
            .map(|c| c.read_only)
            .unwrap_or(self.preferences.layout.all_files_read_only)
    }
    fn category_live(&self) -> bool {
        self.collections
            .config
            .get(self.browser.category)
            .map(|c| c.auto_refresh)
            .unwrap_or(self.preferences.layout.all_files_auto_refresh)
    }
    pub(in crate::app) fn scope_roots(&mut self) -> Arc<Vec<PathBuf>> {
        let key = (
            self.browser.category,
            self.browser.table_root,
            self.scan.report_revision,
            self.collections.revision,
        );
        if let Some((old, roots)) = &self.scoped.plan {
            if *old == key {
                return roots.clone();
            }
        }
        let roots = Arc::new(scoped_refresh::normalize_roots(self.build_scope_roots()));
        self.scoped.plan = Some((key, roots.clone()));
        roots
    }
    fn build_scope_roots(&self) -> Vec<PathBuf> {
        if let Some(path) = self
            .browser
            .table_root
            .and_then(|id| self.scan.report.as_ref().and_then(|r| r.folders.get(id)))
            .map(|f| f.path.clone())
        {
            return vec![path];
        }
        if let Some(c) = self.collections.config.get(self.browser.category) {
            if !c.folders.is_empty() {
                return scoped_refresh::normalize_roots(c.folders.clone());
            }
        }
        if let Some(report) = &self.scan.report {
            if let Some(category) = self.scope_filter() {
                return self
                    .scan
                    .index
                    .category_scope_roots(report, category)
                    .into_iter()
                    .map(|id| report.folders[id].path.clone())
                    .collect();
            }
            return self
                .scan
                .index
                .roots(self.browser.category)
                .iter()
                .map(|id| report.folders[*id].path.clone())
                .collect();
        }
        if self.browser.category == Category::All {
            vec![PathBuf::from(&self.browser.root)]
        } else {
            vec![]
        }
    }
    pub(in crate::app) fn refresh_scope(&mut self, ctx: &egui::Context) {
        if self.browser.view == View::Trash {
            if !self.demo && !self.recycle.busy {
                self.recycle.refresh(ctx);
                self.refresh_drive();
            }
            return;
        }
        if self.scoped.busy() || self.scan.busy || self.scan.restore_requested_root.is_some() {
            return;
        }
        let roots = self.scope_roots();
        if self.scan.report.is_none() {
            let Some(first) = roots.first() else {
                return;
            };
            let mut root = first.clone();
            for path in roots.iter() {
                while !path.starts_with(&root) {
                    if !root.pop() {
                        break;
                    }
                }
            }
            let report = Report {
                folders: vec![crate::scan::Folder {
                    path: root.clone(),
                    parent: None,
                    bytes: 0,
                    files: 0,
                }],
                ..Default::default()
            };
            self.browser.root = root.display().to_string();
            self.scan.index = Index::new(&report);
            self.scan.report = Some(Arc::new(report));
            self.collections.indexed = None;
        }
        self.begin_scope_refresh(roots.as_ref().clone(), false, ctx);
    }
    fn begin_scope_refresh(&mut self, roots: Vec<PathBuf>, shallow: bool, ctx: &egui::Context) {
        if self.demo
            || self.scoped.busy()
            || self.scan.busy
            || self.deletion.rx.is_some()
            || roots.is_empty()
            || self.scan.restore_requested_root.is_some()
        {
            return;
        }
        let Some(base) = self.scan.report.clone() else {
            return;
        };
        let revision = self.scan.report_revision;
        let category = self.scope_filter();
        let persist = self.preferences.layout.save_scan && !shallow;
        self.scoped.control = Arc::new(Control::default());
        let control = self.scoped.control.clone();
        let (tx, rx) = mpsc::channel();
        self.scoped.rx = Some(rx);
        ctx.request_repaint();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            crate::scan::background_priority();
            let result = scoped_refresh::refresh_category(
                &base, &roots, shallow, &control, category,
            )
            .map(|report| {
                let saved = if persist && report.complete {
                    crate::snapshot::save(&report).ok()
                } else {
                    None
                };
                let index = Index::new(&report);
                let paths: std::collections::HashMap<_, _> = report
                    .folders
                    .iter()
                    .enumerate()
                    .map(|(id, f)| (f.path.as_path(), id))
                    .collect();
                let id_map = base
                    .folders
                    .iter()
                    .map(|f| paths.get(f.path.as_path()).copied())
                    .collect();
                Finished {
                    revision,
                    report: Arc::new(report),
                    index,
                    saved,
                    id_map,
                    roots,
                    shallow,
                }
            });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub(in crate::app) fn poll_scope(&mut self, ctx: &egui::Context) {
        if self.editor.saving.is_none() && self.editor.pending.is_none() {
            let result = self.scoped.rx.as_ref().and_then(|rx| match rx.try_recv() {
                Ok(r) => Some(r),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Scope refresh worker stopped".into()))
                }
            });
            if let Some(result) = result {
                self.scoped.rx = None;
                match result {
                    Ok(result) if result.revision == self.scan.report_revision => {
                        let paths = self.scan.report.as_ref().map(|r| {
                            RefreshPaths::capture(
                                r,
                                self.browser.table_root,
                                self.browser.focus,
                                &self.browser.expanded,
                            )
                        });
                        let selected = self.browser.properties.clone();
                        if let Some(paths) = paths {
                            let view = paths.resolve(&result.report);
                            self.browser.table_root = view.table;
                            self.browser.focus = view.focus;
                            self.browser.expanded = view.expanded;
                        }
                        self.browser.root = result.report.folders[0].path.display().to_string();
                        self.scan.report = Some(result.report);
                        self.scan.index = result.index;
                        self.scan.report_revision = self.scan.report_revision.wrapping_add(1);
                        self.scan.snapshot_at = Some(std::time::Instant::now());
                        self.scan.tracker = None;
                        self.scan.restored_snapshot = false;
                        if result.saved.is_some() {
                            self.scan.saved_snapshot_at = result.saved;
                        }
                        self.collections.indexed = None;
                        self.browser.history.remap(|mut state| {
                            if let Some(id) = state.folder {
                                state.folder = Some(*result.id_map.get(id)?.as_ref()?);
                            }
                            Some(state)
                        });
                        self.files.adopt_scope(
                            self.scan.report_revision,
                            &result.roots,
                            result.shallow,
                        );
                        self.browser.tree_revision = self.browser.tree_revision.wrapping_add(1);
                        if let Some(p) = selected {
                            if self.editor.dirty() {
                                // A removed/replaced file must not turn a background
                                // update into a navigation prompt or discard a draft.
                                self.browser.properties = Some(p);
                            } else if p.path.exists() {
                                let bytes = self
                                    .scan
                                    .report
                                    .as_ref()
                                    .unwrap()
                                    .folders
                                    .iter()
                                    .find(|f| f.path == p.path)
                                    .map(|f| f.bytes)
                                    .unwrap_or(p.bytes);
                                self.browser.properties =
                                    Some(Properties::load(p.path.clone(), bytes, None, false));
                            } else {
                                self.browser.properties = None;
                            }
                        }
                        self.adopt_scope_refresh(ctx);
                        self.search.invalidate();
                        self.message =
                            crate::tr!("Поточний scope оновлено", "Current scope refreshed").into();
                    }
                    Ok(_) => {}
                    Err(error) => self.message = error,
                }
                ctx.request_repaint();
            }
        }
        let roots = self.scope_roots();
        let enabled = !self.demo && self.category_live() && self.browser.view != View::Trash;
        let scope_changed = self.live.configure_scope(
            if enabled {
                roots.clone()
            } else {
                Arc::default()
            },
            self.scan.report.clone(),
            self.scan.report_revision,
            if enabled { self.scope_filter() } else { None },
            ctx,
        );
        if scope_changed {
            self.scoped.dirty.clear();
            self.scoped.full = false;
            self.scoped.watch_error = None;
        }
        let events = self.live.poll();
        if let Some(error) = events.error {
            self.scoped.watch_error = Some(error.clone());
            self.message = crate::tr!(
                "Не вдалося відстежувати файли: {error}",
                "Could not watch files: {error}",
                error = error
            );
        }
        if enabled {
            for directory in events.directories {
                if roots.iter().any(|root| directory.starts_with(root)) {
                    self.files.invalidate(&directory);
                    self.scoped.mark_dirty(directory);
                }
            }
            let config = crate::settings::config_dir();
            let cache = crate::snapshot::file().parent().map(Path::to_path_buf);
            for path in events.paths {
                if self.editor.path.as_ref() == Some(&path) {
                    self.reload_changed_document(ctx);
                }
                if path.starts_with(&config)
                    || cache.as_ref().is_some_and(|root| path.starts_with(root))
                {
                    continue;
                }
                if let Some(parent) = path.parent() {
                    self.files.invalidate(parent);
                    if let Some(root) = roots.iter().find(|root| path.starts_with(root)) {
                        self.scoped.mark_dirty(if parent.starts_with(root) {
                            parent.to_path_buf()
                        } else {
                            root.clone()
                        });
                    }
                }
            }
            self.scoped.full |= events.overflow;
            if !self.scoped.busy()
                && !self.scan.busy
                && !self.preferences.open
                && self.new_file.is_none()
                && self.editor.saving.is_none()
                && self
                    .scoped
                    .last_auto
                    .is_none_or(|at| at.elapsed() >= std::time::Duration::from_millis(750))
            {
                if self.scoped.full {
                    self.scoped.full = false;
                    self.scoped.dirty.clear();
                    self.scoped.last_auto = Some(std::time::Instant::now());
                    self.begin_scope_refresh(roots.as_ref().clone(), false, ctx);
                } else if !self.scoped.dirty.is_empty() {
                    self.scoped.last_auto = Some(std::time::Instant::now());
                    let mut dirty: Vec<_> =
                        std::mem::take(&mut self.scoped.dirty).into_iter().collect();
                    dirty.sort();
                    dirty.dedup();
                    self.begin_scope_refresh(dirty, true, ctx);
                }
            }
        } else {
            self.scoped.watch_error = None;
            self.scoped.dirty.clear();
            self.scoped.full = false;
        }
        let home =
            if self.preferences.layout.search_home_fallback && !self.browser.filter.is_empty() {
                if self
                    .scoped
                    .home
                    .as_ref()
                    .is_none_or(|(path, _)| path != &self.preferences.layout.home_path)
                {
                    self.scoped.home = Some((
                        self.preferences.layout.home_path.clone(),
                        self.preferences.layout.home_directory(),
                    ));
                }
                self.scoped
                    .home
                    .as_ref()
                    .map(|(_, p)| p.clone())
                    .filter(|home| {
                        self.scope_filter().is_some()
                            || !roots.iter().any(|root| home.starts_with(root))
                    })
            } else {
                None
            };
        self.search.configure(
            crate::file_search::Spec {
                query: self.browser.filter.clone(),
                roots,
                home,
                category: self.scope_filter(),
                report_revision: self.scan.report_revision,
            },
            self.scan.report.clone(),
            ctx,
        );
        if self.scoped.busy() || self.scoped.full || !self.scoped.dirty.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    pub(in crate::app) fn scope_status_text(&self) -> Option<String> {
        if self.scoped.busy() {
            return Some(
                crate::tr!("Оновлення поточного scope…", "Refreshing current scope…").into(),
            );
        }
        if !self.browser.filter.is_empty() {
            if self.search.busy() {
                return Some(
                    crate::tr!("Пошук папок і файлів…", "Searching folders and files…").into(),
                );
            }
            if let Some(result) = self.search.for_query(&self.browser.filter) {
                let place = if result.home {
                    crate::tr!("Результати з Home", "Results from Home")
                } else {
                    crate::tr!("Результати в scope", "Scope results")
                };
                let suffix = if result.truncated {
                    crate::tr!(" · уточни запит", " · refine the query")
                } else {
                    ""
                };
                return Some(crate::tr!(
                    "{place}: {count}{suffix} · недоступно: {skipped}",
                    "{place}: {count}{suffix} · inaccessible: {skipped}",
                    place = place,
                    count = result.hits.len(),
                    suffix = suffix,
                    skipped = result.skipped
                ));
            }
        }
        if let Some(error) = &self.scoped.watch_error {
            return Some(crate::tr!(
                "Автооновлення неповне: {error}",
                "Live updates are incomplete: {error}",
                error = error
            ));
        }
        if self.category_read_only() {
            return Some(crate::tr!("Категорія: лише читання", "Category: read only").into());
        }
        None
    }
}
