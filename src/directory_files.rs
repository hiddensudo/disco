//! Lazy, bounded listings for expanded directories, separate from scan snapshots.
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
};
const DIRECTORY_LIMIT: usize = 24;
const CACHE_FILES: usize = 30_000;
const FILE_LIMIT: usize = 10_000;
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub path: PathBuf,
    pub bytes: u64,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Listing {
    pub files: Vec<Arc<Entry>>,
    pub limited: bool,
    pub error: Option<String>,
}
fn read(path: &Path) -> Listing {
    let mut listing = Listing::default();
    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(e) => {
            listing.error = Some(e.to_string());
            return listing;
        }
    };
    // Cap visited entries too: a huge directory must not monopolize a worker.
    for (visited, result) in entries.enumerate() {
        if visited >= 100_000 || listing.files.len() >= FILE_LIMIT {
            listing.limited = true;
            break;
        }
        let entry = match result {
            Ok(e) => e,
            Err(e) => {
                listing.error = Some(e.to_string());
                continue;
            }
        };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_file() {
            continue;
        }
        let Ok(m) = entry.metadata() else { continue };
        listing.files.push(Arc::new(Entry {
            path: entry.path(),
            bytes: crate::scan::allocated(&m),
        }));
    }
    listing
}
struct Cached {
    listing: Arc<Listing>,
    touched: u64,
}
type Finished = (u64, PathBuf, Listing);
pub(crate) struct Files {
    cache: HashMap<PathBuf, Cached>,
    pending: HashSet<PathBuf>,
    tx: Sender<Finished>,
    rx: Receiver<Finished>,
    epoch: u64,
    tick: u64,
    pub revision: u64,
    report: u64,
}
impl Default for Files {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            cache: HashMap::new(),
            pending: HashSet::new(),
            tx,
            rx,
            epoch: 0,
            tick: 0,
            revision: 0,
            report: 0,
        }
    }
}
impl Files {
    pub fn invalidate(&mut self, path: &Path) {
        self.cache.remove(path);
        // Discard reads that began before a save/create. Otherwise an old
        // in-flight listing can repopulate the invalidated directory.
        self.epoch = self.epoch.wrapping_add(1);
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn get(&self, path: &Path) -> Option<&Arc<Listing>> {
        self.cache.get(path).map(|c| &c.listing)
    }
    pub fn touch(&mut self, path: &Path) {
        self.tick = self.tick.wrapping_add(1);
        if let Some(c) = self.cache.get_mut(path) {
            c.touched = self.tick;
        }
    }
    pub fn request(&mut self, path: PathBuf, ctx: &eframe::egui::Context) {
        self.touch(&path);
        if self.cache.contains_key(&path) || self.pending.contains(&path) || self.pending.len() >= 2
        {
            return;
        }
        self.pending.insert(path.clone());
        let tx = self.tx.clone();
        let epoch = self.epoch;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            crate::scan::background_priority();
            let listing = read(&path);
            let _ = tx.send((epoch, path, listing));
            ctx.request_repaint();
        });
    }
    pub fn poll(&mut self, report: u64) {
        if self.report != report {
            self.report = report;
            self.epoch = self.epoch.wrapping_add(1);
            self.cache.clear();
            // Keep obsolete jobs counted until they finish: repeated refreshes
            // cannot create an unbounded number of directory-reading threads.
            self.revision = self.revision.wrapping_add(1);
        }
        while let Ok((epoch, path, listing)) = self.rx.try_recv() {
            self.pending.remove(&path);
            if epoch != self.epoch {
                continue;
            }
            self.tick = self.tick.wrapping_add(1);
            self.cache.insert(
                path,
                Cached {
                    listing: Arc::new(listing),
                    touched: self.tick,
                },
            );
            while self.cache.len() > DIRECTORY_LIMIT
                || self
                    .cache
                    .values()
                    .map(|c| c.listing.files.len())
                    .sum::<usize>()
                    > CACHE_FILES
            {
                let key = self
                    .cache
                    .iter()
                    .min_by_key(|(_, c)| c.touched)
                    .map(|(p, _)| p.clone());
                if let Some(key) = key {
                    self.cache.remove(&key);
                } else {
                    break;
                }
            }
            self.revision = self.revision.wrapping_add(1);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn listing_ignores_directories_and_links_and_reports_failures() {
        let root = std::env::temp_dir().join(format!("disco-files-{}", std::process::id()));
        fs::create_dir_all(root.join("folder")).unwrap();
        fs::write(root.join("notes.md"), "hello").unwrap();
        std::os::unix::fs::symlink(root.join("notes.md"), root.join("link")).unwrap();
        let files = read(&root);
        assert_eq!(files.files.len(), 1);
        assert!(files.files[0].path.ends_with("notes.md"));
        assert!(read(&root.join("absent")).error.is_some());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn outdated_listings_are_discarded_and_cache_is_bounded() {
        let mut files = Files::default();
        files.poll(1);
        files
            .tx
            .send((0, "/old".into(), Listing::default()))
            .unwrap();
        files.poll(1);
        assert!(files.get(Path::new("/old")).is_none());
        for i in 0..40 {
            files
                .tx
                .send((files.epoch, format!("/{i}").into(), Listing::default()))
                .unwrap();
        }
        files.poll(1);
        assert_eq!(files.cache.len(), DIRECTORY_LIMIT);
        assert!(files.get(Path::new("/39")).is_some());
        let old_epoch = files.epoch;
        files.invalidate(Path::new("/39"));
        files
            .tx
            .send((old_epoch, "/39".into(), Listing::default()))
            .unwrap();
        files.poll(1);
        assert!(
            files.get(Path::new("/39")).is_none(),
            "a read from before file creation is stale"
        );
    }
}
