//! One bounded background monitor for the active file, independent of scan scopes.
//! Metadata polling also covers atomic replacement, directory rename, missed native
//! notifications and files outside the snapshot. Idle documents never wake the UI.
use crate::text_editor::Stamp;
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant},
};
const INTERVAL: Duration = Duration::from_millis(200);
type Target = (u64, Option<PathBuf>);
#[derive(Default)]
pub(crate) struct Monitor {
    path: Option<PathBuf>,
    generation: u64,
    command: Arc<Mutex<Option<Target>>>,
    pending: Arc<Mutex<Option<u64>>>,
    ready: Arc<AtomicU64>,
    tx: Option<SyncSender<()>>,
}
impl Monitor {
    pub fn configure(&mut self, path: Option<PathBuf>, ctx: &egui::Context) {
        if self.path == path {
            return;
        }
        self.path = path.clone();
        self.generation = self.generation.wrapping_add(1);
        *self.pending.lock().unwrap() = None;
        if path.is_none() && self.tx.is_none() {
            return;
        }
        *self.command.lock().unwrap() = Some((self.generation, path));
        if self.tx.is_none() {
            let (tx, rx) = mpsc::sync_channel(1);
            self.tx = Some(tx);
            let command = self.command.clone();
            let pending = self.pending.clone();
            let ready = self.ready.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                crate::scan::background_priority();
                let mut target: Target = (0, None);
                let mut observed: Option<Option<Stamp>> = None;
                let mut published: Option<Option<Stamp>> = None;
                let mut since = Instant::now();
                let mut first: Option<Instant> = None;
                loop {
                    let signal = if target.1.is_some() {
                        rx.recv_timeout(INTERVAL)
                    } else {
                        rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                    };
                    if matches!(signal, Err(mpsc::RecvTimeoutError::Disconnected)) {
                        break;
                    }
                    if let Some(next) = command.lock().unwrap().take() {
                        target = next;
                        observed = None;
                        published = None;
                        first = None;
                    }
                    let Some(path) = &target.1 else { continue };
                    let stamp = std::fs::symlink_metadata(path).ok().as_ref().map(Stamp::of);
                    if observed != Some(stamp) {
                        observed = Some(stamp);
                        since = Instant::now();
                        first.get_or_insert(since);
                    }
                    // Wait for a stable sample to avoid reading the truncate/write
                    // gap, but do not starve the preview during continuous writes.
                    if published != Some(stamp)
                        && (since.elapsed() >= INTERVAL
                            || first.is_some_and(|at| at.elapsed() >= Duration::from_secs(1)))
                    {
                        *pending.lock().unwrap() = Some(target.0);
                        ready.store(target.0, Ordering::Relaxed);
                        published = Some(stamp);
                        first = None;
                        ctx.request_repaint();
                    }
                }
            });
        }
        let _ = self.tx.as_ref().unwrap().try_send(());
    }
    pub fn changed(&mut self) -> bool {
        self.pending
            .lock()
            .unwrap()
            .take()
            .is_some_and(|generation| generation == self.generation)
    }
    #[cfg(test)]
    pub fn ready(&self) -> bool {
        self.path.is_some() && self.ready.load(Ordering::Relaxed) == self.generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_latest_active_path_publishes_changes_and_none_disables_it() {
        let root =
            std::env::temp_dir().join(format!("disco-document-watch-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let a = root.join("a.txt");
        let b = root.join("b.txt");
        std::fs::write(&a, "before").unwrap();
        std::fs::write(&b, "second").unwrap();
        let ctx = egui::Context::default();
        let mut monitor = Monitor::default();
        monitor.configure(Some(a.clone()), &ctx);
        let end = Instant::now() + Duration::from_secs(3);
        while !monitor.ready() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(monitor.ready());
        monitor.configure(Some(b.clone()), &ctx);
        assert!(!monitor.changed(), "discard the old file's pending event");
        let end = Instant::now() + Duration::from_secs(3);
        while !monitor.ready() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(monitor.ready());
        assert!(monitor.changed());
        std::fs::write(&a, "old file changed").unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert!(!monitor.changed());
        let replacement = root.join("replacement");
        std::fs::write(&replacement, "new second").unwrap();
        std::fs::rename(replacement, &b).unwrap();
        let end = Instant::now() + Duration::from_secs(3);
        let mut changed = false;
        while Instant::now() < end {
            changed = monitor.changed();
            if changed {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(changed, "atomic replacement must publish a change");
        // A disabled monitor clears pending work and sleeps until reconfigured.
        monitor.configure(None, &ctx);
        assert!(!monitor.ready());
        std::fs::write(&b, "disabled").unwrap();
        std::thread::sleep(Duration::from_millis(450));
        assert!(!monitor.changed());
        std::fs::remove_dir_all(root).unwrap();
    }
}
