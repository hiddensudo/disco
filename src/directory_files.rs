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
    pub directories: Vec<Arc<Entry>>,
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
        if visited >= 100_000 || listing.files.len() + listing.directories.len() >= FILE_LIMIT {
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
        if kind.is_dir() {
            listing.directories.push(Arc::new(Entry {
                path: entry.path(),
                bytes: 0,
            }));
            continue;
        }
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
    pub fn adopt_scope(&mut self, revision: u64, roots: &[PathBuf], shallow: bool) {
        self.report = revision;
        self.epoch = self.epoch.wrapping_add(1);
        self.cache.retain(|path, _| {
            !roots.iter().any(|root| {
                if shallow {
                    path == root
                } else {
                    path.starts_with(root)
                }
            })
        });
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn invalidate(&mut self, path: &Path) {
        self.cache.remove(path);
        // Discard reads that began before a save/create. Otherwise an old
        // in-flight listing can repopulate the invalidated directory.
        self.epoch = self.epoch.wrapping_add(1);
        self.revision = self.revision.wrapping_add(1);
    }

    /// A content edit changes one row, not the directory's identity. Preserve
    /// the visible listing instead of replacing it with a loading frame.
    pub fn update_file(&mut self, path: &Path, bytes: Option<u64>) {
        let Some(parent) = path.parent() else { return };
        let Some(cached) = self.cache.get_mut(parent) else {
            return;
        };
        let current = cached.listing.files.iter().find(|entry| entry.path == path);
        if current.map(|entry| entry.bytes) == bytes {
            return;
        }
        let listing = Arc::make_mut(&mut cached.listing);
        listing.files.retain(|entry| entry.path != path);
        if let Some(bytes) = bytes {
            listing.files.push(Arc::new(Entry {
                path: path.into(),
                bytes,
            }));
        }
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
                    .map(|c| c.listing.files.len() + c.listing.directories.len())
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
    fn content_edits_update_one_row_without_dropping_the_visible_listing() {
        let mut files = Files::default();
        let path = Path::new("/workspace/main.rs");
        let other = Arc::new(Entry {
            path: "/workspace/other.rs".into(),
            bytes: 10,
        });
        files.cache.insert(
            "/workspace".into(),
            Cached {
                listing: Arc::new(Listing {
                    files: vec![
                        Arc::new(Entry {
                            path: path.into(),
                            bytes: 1,
                        }),
                        other.clone(),
                    ],
                    ..Default::default()
                }),
                touched: 0,
            },
        );
        files.update_file(path, Some(2));
        let visible = files.get(path.parent().unwrap()).unwrap();
        assert_eq!(visible.files.len(), 2);
        assert!(visible.files.iter().any(|f| f.path == path && f.bytes == 2));
        assert!(visible.files.iter().any(|f| Arc::ptr_eq(f, &other)));
        files.update_file(path, None);
        assert_eq!(files.get(path.parent().unwrap()).unwrap().files.len(), 1);
        files.update_file(path, Some(3));
        assert_eq!(files.get(path.parent().unwrap()).unwrap().files.len(), 2);
        assert!(
            files.pending.is_empty(),
            "content edits do not rescan a directory"
        );
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
