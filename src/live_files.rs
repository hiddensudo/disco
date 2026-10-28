//! One bounded background watcher for the active scope. Watch the directories
//! already present in the snapshot non-recursively: registration never walks
//! another tree or crosses a mount. New directories arrive via scoped refresh.
use notify::{RecursiveMode, Watcher};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant},
};
#[derive(Default)]
struct Pending {
    paths: HashSet<PathBuf>,
    directories: HashSet<PathBuf>,
    overflow: bool,
    error: Option<String>,
    changed: Option<Instant>,
    first: Option<Instant>,
}
#[derive(Default)]
pub(crate) struct Events {
    pub paths: Vec<PathBuf>,
    pub directories: Vec<PathBuf>,
    pub overflow: bool,
    pub error: Option<String>,
}
type Command = (
    u64,
    Arc<Vec<PathBuf>>,
    Option<Arc<crate::scan::Report>>,
    bool,
    Option<crate::index::Category>,
);
#[derive(Default)]
pub(crate) struct LiveFiles {
    roots: Arc<Vec<PathBuf>>,
    category: Option<crate::index::Category>,
    revision: u64,
    commands: Arc<Mutex<Option<Command>>>,
    pending: Arc<Mutex<Pending>>,
    generation: Arc<AtomicU64>,
    tx: Option<SyncSender<()>>,
    ready: Arc<AtomicU64>,
}
impl LiveFiles {
    #[cfg(test)]
    pub fn configure(&mut self, roots: Arc<Vec<PathBuf>>, ctx: &eframe::egui::Context) -> bool {
        self.configure_scope(roots, None, 0, None, ctx)
    }
    pub fn configure_scope(
        &mut self,
        roots: Arc<Vec<PathBuf>>,
        report: Option<Arc<crate::scan::Report>>,
        revision: u64,
        category: Option<crate::index::Category>,
        ctx: &eframe::egui::Context,
    ) -> bool {
        let changed = roots != self.roots || category != self.category;
        self.category = category;
        if !changed && (revision == self.revision || roots.is_empty()) {
            return false;
        }
        self.roots = roots.clone();
        self.revision = revision;
        let generation = if changed {
            *self.pending.lock().unwrap() = Pending::default();
            self.generation.fetch_add(1, Ordering::Relaxed) + 1
        } else {
            self.generation.load(Ordering::Relaxed)
        };
        if roots.is_empty() && self.tx.is_none() {
            return changed;
        }
        let report = if roots.is_empty() { None } else { report };
        *self.commands.lock().unwrap() = Some((generation, roots, report, changed, category));
        if self.tx.is_none() {
            let (tx, rx) = mpsc::sync_channel(1);
            self.tx = Some(tx);
            let commands = self.commands.clone();
            let shared = self.pending.clone();
            let epoch = self.generation.clone();
            let ready = self.ready.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                crate::scan::background_priority();
                let mut watcher: Option<notify::RecommendedWatcher> = None;
                let mut active_generation = 0;
                let mut watched = HashSet::<PathBuf>::new();
                while rx.recv().is_ok() {
                    let Some((generation, roots, report, initial, category)) =
                        commands.lock().unwrap().take()
                    else {
                        continue;
                    };
                    let initial = initial || generation != active_generation;
                    if generation != active_generation {
                        drop(watcher.take());
                        watched.clear();
                        active_generation = generation;
                        if roots.is_empty() {
                            drop(report);
                            continue;
                        }
                        let events = shared.clone();
                        let events_epoch = epoch.clone();
                        let wake = ctx.clone();
                        match notify::recommended_watcher(
                            move |event: notify::Result<notify::Event>| {
                                if events_epoch.load(Ordering::Relaxed) != generation
                                    || event.as_ref().is_ok_and(|e| e.kind.is_access())
                                {
                                    return;
                                }
                                let mut pending = events.lock().unwrap();
                                match event {
                                    Ok(event) => {
                                        pending.overflow |= event.need_rescan();
                                        for path in event.paths {
                                            if pending.paths.len() < 2048 {
                                                pending.paths.insert(path);
                                            } else {
                                                pending.overflow = true;
                                            }
                                        }
                                    }
                                    Err(error) => {
                                        pending.error = Some(error.to_string());
                                        pending.overflow = true;
                                    }
                                }
                                pending.first.get_or_insert(Instant::now());
                                pending.changed = Some(Instant::now());
                                wake.request_repaint_after(Duration::from_millis(350));
                            },
                        ) {
                            Ok(new) => watcher = Some(new),
                            Err(error) => {
                                shared.lock().unwrap().error = Some(error.to_string());
                                ctx.request_repaint();
                                continue;
                            }
                        }
                    }
                    let mut desired: HashSet<PathBuf> = roots.iter().cloned().collect();
                    let trash_roots = if category.is_some() {
                        crate::recycle::scan_roots()
                    } else {
                        vec![]
                    };
                    if let Some(report) = report {
                        for folder in &report.folders {
                            let index = roots
                                .partition_point(|root| root.as_path() <= folder.path.as_path());
                            if index > 0
                                && folder.path.starts_with(&roots[index - 1])
                                && category.is_none_or(|category| {
                                    crate::index::classify(&folder.path, &trash_roots) == category
                                })
                            {
                                desired.insert(folder.path.clone());
                            }
                        }
                    }
                    let Some(watcher) = &mut watcher else {
                        continue;
                    };
                    for old in watched.difference(&desired) {
                        let _ = watcher.unwatch(old);
                    }
                    watched.retain(|p| desired.contains(p));
                    let mut added = Vec::new();
                    for path in desired.difference(&watched) {
                        if epoch.load(Ordering::Relaxed) != generation {
                            break;
                        }
                        match watcher.watch(path, RecursiveMode::NonRecursive) {
                            Ok(()) => added.push(path.clone()),
                            Err(error) => {
                                shared.lock().unwrap().error = Some(error.to_string());
                                ctx.request_repaint();
                                break;
                            }
                        }
                    }
                    watched.extend(added.iter().cloned());
                    if epoch.load(Ordering::Relaxed) == generation {
                        ready.store(generation, Ordering::Relaxed);
                        if initial || !added.is_empty() {
                            let mut pending = shared.lock().unwrap();
                            pending.overflow |= initial;
                            if !initial {
                                for path in added {
                                    if pending.directories.len() < 2048 {
                                        pending.directories.insert(path);
                                    } else {
                                        pending.overflow = true;
                                    }
                                }
                            }
                            pending.first.get_or_insert(Instant::now());
                            pending.changed = Some(Instant::now());
                            ctx.request_repaint_after(Duration::from_millis(350));
                        }
                    }
                }
            });
        }
        let _ = self.tx.as_ref().unwrap().try_send(());
        changed
    }
    #[cfg(test)]
    pub(crate) fn ready(&self) -> bool {
        !self.roots.is_empty()
            && self.ready.load(Ordering::Relaxed) == self.generation.load(Ordering::Relaxed)
    }
    pub fn poll(&mut self) -> Events {
        let mut pending = self.pending.lock().unwrap();
        let mut result = Events {
            error: pending.error.take(),
            ..Default::default()
        };
        if pending
            .changed
            .is_some_and(|at| at.elapsed() >= Duration::from_millis(300))
            || pending
                .first
                .is_some_and(|at| at.elapsed() >= Duration::from_secs(2))
        {
            result.paths = pending.paths.drain().collect();
            result.directories = pending.directories.drain().collect();
            result.overflow = std::mem::take(&mut pending.overflow);
            pending.changed = None;
            pending.first = None;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_scopes_do_not_start_a_worker_or_retain_a_snapshot() {
        let ctx = eframe::egui::Context::default();
        let report = Arc::new(crate::scan::Report::default());
        let mut live = LiveFiles::default();
        live.configure_scope(
            Arc::default(),
            Some(report.clone()),
            1,
            Some(crate::index::Category::Games),
            &ctx,
        );
        assert!(live.tx.is_none());
        assert_eq!(Arc::strong_count(&report), 1);
    }
    #[test]
    fn real_watcher_detects_existing_file_changes_and_stops_when_disabled() {
        let root = std::env::temp_dir().join(format!("disco-watch-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("config.json");
        std::fs::write(&file, "before").unwrap();
        let ctx = eframe::egui::Context::default();
        let mut live = LiveFiles::default();
        live.configure(Arc::new(vec![root.clone()]), &ctx);
        let start = Instant::now();
        while !live.ready() && start.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(live.ready());
        std::fs::write(&file, "after").unwrap();
        let start = Instant::now();
        let mut seen = false;
        while !seen && start.elapsed() < Duration::from_secs(3) {
            seen |= live.poll().paths.contains(&file);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            seen,
            "changes to a file need not change the directory mtime"
        );
        live.configure(Arc::default(), &ctx);
        std::fs::write(&file, "off").unwrap();
        std::thread::sleep(Duration::from_millis(400));
        assert!(live.poll().paths.is_empty());
        drop(live);
        std::fs::remove_dir_all(root).unwrap();
    }
}
