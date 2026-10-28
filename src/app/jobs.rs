use crate::app::Disco;
use crate::app::state::View;
use crate::deletion;
use crate::filesystem::drive_info;
use crate::index::{Category, Index, RefreshPaths};
use crate::scan::{
    ChangeTracker, Control, Event, Folder, background_priority, duplicates, spawn_scan,
};
use eframe::egui;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::{Duration, Instant};

impl Disco {
    pub(in crate::app) fn start(&mut self, dup: bool, force: bool) {
        self.start_job(dup, force, false);
    }
    pub(in crate::app) fn start_job(&mut self, dup: bool, force: bool, automatic: bool) {
        if self.scoped.busy() || self.scan.busy || self.deletion.rx.is_some() || self.recycle.busy {
            return;
        }
        if self.guard_editor_scan(dup, force, automatic) {
            return;
        }
        self.scan.restore_requested_root = None;
        let path = PathBuf::from(&self.browser.root);
        if !path.is_absolute() {
            self.message = crate::tr!(
                "Вкажи абсолютний шлях до папки",
                "Enter an absolute folder path"
            )
            .into();
            return;
        }
        let path = match path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                self.message = e.to_string();
                return;
            }
        };
        let same_snapshot = self
            .scan
            .report
            .as_ref()
            .is_some_and(|r| r.complete && r.folders.first().is_some_and(|f| f.path == path));
        if !dup
            && !force
            && !self.demo
            && self.preferences.layout.reuse_scan
            && same_snapshot
            && self
                .scan
                .tracker
                .as_mut()
                .is_some_and(ChangeTracker::is_clean)
        {
            self.message = crate::tr!(
                "Подій змін немає · попередній результат ({} с тому)",
                "No changes detected · previous result ({} s ago)",
                self.scan
                    .snapshot_at
                    .map(|t| t.elapsed().as_secs())
                    .unwrap_or(0)
            );
            self.refresh_drive();
            return;
        }
        let restore_view = if automatic {
            self.scan.report.as_ref().map(|r| {
                RefreshPaths::capture(
                    r,
                    self.browser.table_root,
                    self.browser.focus,
                    &self.browser.expanded,
                )
            })
        } else {
            None
        };
        self.scan.automatic_job = automatic;
        self.scan.restored_snapshot = false;
        self.browser.root = path.to_string_lossy().into_owned();
        if !automatic {
            self.browser.table_root = None;
        }
        self.refresh_drive();
        self.scan.control = Arc::new(Control::default());
        self.scan.control.gentle.store(
            automatic || self.preferences.layout.gentle_scan,
            Ordering::Relaxed,
        );
        self.scan.busy = true;
        self.scan.duplicate_job = dup;
        self.scan.progress = None;
        self.message.clear();
        self.started = Instant::now();
        self.browser.selected.clear();
        self.demo = false;
        let (tx, rx) = mpsc::channel();
        self.scan.rx = Some(rx);
        if dup {
            self.scan.dups.clear();
            let c = self.scan.control.clone();
            std::thread::spawn(move || {
                background_priority();
                let event = match duplicates(&path, &c, |p| {
                    let _ = tx.send(Event::Progress(p));
                }) {
                    Ok((d, n)) => Event::Duplicates(d, n),
                    Err(e) => Event::Error(e.to_string()),
                };
                let _ = tx.send(event);
            });
        } else {
            self.scan.tracker = None;
            self.browser.history.clear();
            if !same_snapshot {
                self.scan.report_revision = self.scan.report_revision.wrapping_add(1);
                self.scan.report = None;
                self.scan.index = Index::default();
                self.browser.properties = None;
            }
            self.scan.dups.clear();
            if !automatic {
                self.browser.focus = 0;
            }
            spawn_scan(
                path,
                self.scan.control.clone(),
                tx,
                self.preferences.layout.save_scan,
                restore_view,
            );
        }
    }
    pub(in crate::app) fn poll(&mut self) {
        let events: Vec<_> = self
            .scan
            .rx
            .as_ref()
            .map(|r| r.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Progress(p) => self.scan.progress = Some(p),
                Event::Restored(result) => {
                    let expected = self.scan.restore_requested_root.take();
                    if expected.as_deref() != Some(&self.browser.root)
                        || self.scan.busy
                        || self.scan.report.is_some()
                    {
                        continue;
                    }
                    match result {
                        Ok(Some((saved, index))) => {
                            let matches_requested = std::env::args().any(|a| a == "--folder");
                            if matches_requested
                                && saved.report.folders[0].path != PathBuf::from(&self.browser.root)
                            {
                                continue;
                            }
                            self.browser.root =
                                saved.report.folders[0].path.to_string_lossy().into_owned();
                            self.scan.index = index;
                            self.scan.report_revision = self.scan.report_revision.wrapping_add(1);
                            self.scan.report = Some(Arc::new(saved.report));
                            self.scan.saved_snapshot_at = Some(saved.saved_at);
                            self.scan.restored_snapshot = true;
                            self.scan.tracker = None;
                            self.browser.history.clear();
                            self.browser.table_root = None;
                            self.browser.expanded = HashSet::from([0]);
                            self.browser.category = Category::All;
                            if self.browser.view != View::Trash {
                                self.browser.view = View::Folders;
                            }
                            self.message = crate::tr!(
                                "Відновлено останній знімок сканування.",
                                "Restored the last scan snapshot."
                            )
                            .into();
                            self.refresh_drive();
                        }
                        Ok(None) => {}
                        Err(e) => {
                            self.message = crate::tr!(
                                "Не вдалося відновити скан: {e}",
                                "Could not restore the scan: {e}",
                                e = e
                            )
                        }
                    }
                }
                Event::Finished(r, index, tracker, saved, resolved) => {
                    if !r.complete && self.scan.report.as_ref().is_some_and(|old| old.complete) {
                        self.scan.busy = false;
                        self.scan.automatic_job = false;
                        self.scan.tracker = None;
                        self.scan.restored_snapshot = true;
                        self.message = crate::tr!(
                            "Оновлення скасовано. Залишено попередній повний скан.",
                            "Refresh cancelled. Kept the previous complete scan."
                        )
                        .into();
                        continue;
                    }
                    self.scan.automatic_job = false;
                    self.scan.saved_snapshot_at = saved.as_ref().ok().copied().flatten();
                    self.scan.restored_snapshot = false;
                    self.scan.tracker = tracker;
                    self.scan.snapshot_at = Some(Instant::now());
                    self.browser.history.clear();
                    self.scan.busy = false;
                    self.message = if r.complete {
                        crate::tr!(
                            "Завершено за {:.1} с · пропущено: {} · повторних hardlink: {}",
                            "Finished in {:.1} s · skipped: {} · duplicate hardlinks: {}",
                            r.elapsed.as_secs_f64(),
                            r.skipped,
                            r.hardlinks
                        ) + if self.scan.tracker.is_some() {
                            crate::tr!(" · перевірка змін активна", " · change monitoring active")
                        } else {
                            crate::tr!(" · повторний обхід повністю", " · full rescan required")
                        }
                    } else {
                        crate::tr!(
                            "Сканування скасовано. Показано частковий результат.",
                            "Scan cancelled. Showing partial results."
                        )
                        .into()
                    };
                    if let Err(error) = saved {
                        self.message.push_str(&crate::tr!(
                            " · не збережено: {error}",
                            " · not saved: {error}",
                            error = error
                        ));
                    }
                    self.scan.report_revision = self.scan.report_revision.wrapping_add(1);
                    self.scan.index = index;
                    if let Some(v) = resolved {
                        self.browser.table_root = v.table;
                        self.browser.focus = v.focus;
                        self.browser.expanded = v.expanded;
                    } else {
                        self.browser.expanded = HashSet::from([0]);
                    }
                    self.browser.tree_revision = self.browser.tree_revision.wrapping_add(1);
                    self.browser.properties = None;
                    self.scan.report = Some(Arc::new(r));
                }
                Event::Duplicates(d, n) => {
                    self.scan.busy = false;
                    self.message = crate::tr!(
                        "Знайдено {} груп · пропущено або змінено: {n}",
                        "Found {} groups · skipped or changed: {n}",
                        d.len(),
                        n = n
                    );
                    self.scan.dups = d;
                }
                Event::Error(e) => {
                    self.scan.automatic_job = false;
                    self.scan.restored_snapshot = self.scan.report.is_some();
                    self.scan.busy = false;
                    self.message = e;
                }
            }
        }
        if let Some(rx) = &self.deletion.rx {
            if let Ok(result) = rx.try_recv() {
                self.deletion.rx = None;
                if !result.done.is_empty() {
                    self.recycle.invalidate();
                    self.browser.selected.clear();
                    self.browser.properties = None;
                    self.scan.tracker = None;
                    // A selected folder can also be the current scan root.
                    let mut root = PathBuf::from(&self.browser.root);
                    while !root.is_dir() {
                        if !root.pop() {
                            break;
                        }
                    }
                    self.browser.root = root.to_string_lossy().into_owned();
                    self.start_job(false, true, true);
                    self.message = crate::tr!(
                        "У кошику: {} елементів. Оновлюємо список. Місце звільниться після спорожнення кошика.",
                        "Moved {} items to Trash. Refreshing the list. Empty Trash to free disk space.",
                        result.done.len()
                    );
                }
                if let Some(error) = result.error {
                    self.message = crate::tr!(
                        "У кошику: {} елементів. Не вдалося завершити: {error}",
                        "Moved {} items to Trash. Could not finish: {error}",
                        result.done.len(),
                        error = error
                    );
                }
            }
        }
    }
    pub(in crate::app) fn request_delete(&mut self, folders: Vec<(PathBuf, u64)>) {
        if self.category_read_only() || self.scoped.busy() {
            self.message = crate::tr!(
                "Категорія доступна лише для читання або оновлюється",
                "The category is read only or refreshing"
            )
            .into();
            return;
        }
        if self.demo
            || self.scan.busy
            || self.deletion.rx.is_some()
            || self.recycle.busy
            || folders.is_empty()
        {
            return;
        }
        if self.editor.dirty()
            && self
                .editor
                .path
                .as_ref()
                .is_some_and(|edited| folders.iter().any(|(path, _)| edited.starts_with(path)))
        {
            self.editor.pending = Some(super::editor::Action::Delete(folders));
            self.editor.confirm_fresh = true;
            return;
        }
        let targets: Result<Vec<_>, _> = folders
            .into_iter()
            .map(|(p, b)| deletion::Target::capture(&p, b))
            .collect();
        match targets {
            Ok(targets) => {
                self.deletion.targets = targets;
                self.deletion.fresh = true;
                self.deletion.confirm = true;
            }
            Err(e) => {
                self.message = crate::tr!(
                    "Не можна видалити елемент: {e}",
                    "Cannot delete item: {e}",
                    e = e
                )
            }
        }
    }
    pub(in crate::app) fn commit_delete(&mut self, ctx: &egui::Context) {
        if self.category_read_only() {
            self.deletion.confirm = false;
            return;
        }
        self.deletion.confirm = false;
        let targets = std::mem::take(&mut self.deletion.targets);
        let (tx, rx) = mpsc::channel();
        self.deletion.rx = Some(rx);
        self.message = crate::tr!("Переміщуємо у кошик…", "Moving to Trash…").into();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(deletion::move_to_trash(targets));
            ctx.request_repaint();
        });
    }
    pub(in crate::app) fn candidates(&self) -> Vec<Folder> {
        self.scan
            .report
            .as_ref()
            .filter(|r| r.complete)
            .map(|r| {
                self.scan
                    .index
                    .cleanup_ids()
                    .iter()
                    .filter_map(|i| r.folders.get(*i).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }
    pub(in crate::app) fn refresh_drive(&mut self) {
        let (n, m) = if self.demo {
            (
                crate::tr!("Системний SSD", "System SSD").into(),
                crate::tr!("NVMe SSD · 1 ТБ", "NVMe SSD · 1 TB").into(),
            )
        } else {
            drive_info(Path::new(&self.browser.root))
        };
        self.drive.name = n;
        self.drive.model = m;
        self.drive.checked_at = Instant::now() - Duration::from_secs(10);
    }
}
