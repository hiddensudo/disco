use std::os::unix::fs::MetadataExt;
use std::{
    collections::{BinaryHeap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Folder {
    #[serde(with = "crate::snapshot::path_bytes")]
    pub path: PathBuf,
    pub parent: Option<usize>,
    pub bytes: u64,
    pub files: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct BigFile {
    pub bytes: u64,
    #[serde(with = "crate::snapshot::path_bytes")]
    pub path: PathBuf,
}
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Report {
    pub folders: Vec<Folder>,
    pub largest: Vec<BigFile>,
    pub entries: u64,
    pub skipped: u64,
    pub hardlinks: u64,
    pub elapsed: Duration,
    pub complete: bool,
}
#[derive(Clone, Debug)]
pub struct Progress {
    pub entries: u64,
    pub bytes: u64,
    pub current: PathBuf,
    pub skipped: u64,
}
#[derive(Debug)]
pub(crate) enum Event {
    Progress(Progress),
    Finished(
        Report,
        crate::index::Index,
        Option<ChangeTracker>,
        Result<Option<u64>, String>,
        Option<crate::index::ResolvedView>,
    ),
    Restored(Result<Option<(crate::snapshot::SavedScan, crate::index::Index)>, String>),
    Duplicates(Vec<Vec<BigFile>>, u64),
    Error(String),
}
#[derive(Default)]
pub struct Control {
    pub cancel: AtomicBool,
    pub pause: AtomicBool,
    pub gentle: AtomicBool,
}
impl Control {
    pub fn proceed(&self) -> bool {
        while self.pause.load(Ordering::Relaxed) && !self.cancel.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(80));
        }
        !self.cancel.load(Ordering::Relaxed)
    }
}
pub fn background_priority() {
    unsafe {
        libc::nice(10);
    }
}
pub fn allocated(m: &fs::Metadata) -> u64 {
    m.blocks().saturating_mul(512)
}
/// An in-process snapshot guard, never a permanent filesystem indexer.
/// If any directory cannot be watched, events overflow, or hardlinks are present,
/// caching is disabled instead of claiming that an old result is current.
#[derive(Debug)]
pub struct ChangeTracker {
    fd: std::os::fd::OwnedFd,
    count: usize,
    limit: usize,
    dirty: bool,
    mounts: Vec<u8>,
    sealed_at: Option<Instant>,
}
impl ChangeTracker {
    fn new() -> Option<Self> {
        use std::os::fd::FromRawFd;
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if fd < 0 {
            return None;
        }
        let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) };
        let max = fs::read_to_string("/proc/sys/fs/inotify/max_user_watches")
            .ok()?
            .trim()
            .parse::<usize>()
            .ok()?;
        Some(Self {
            fd,
            count: 0,
            limit: (max / 8).min(65_536),
            dirty: false,
            mounts: fs::read("/proc/self/mountinfo").ok()?,
            sealed_at: None,
        })
    }
    fn watch(&mut self, path: &Path) -> bool {
        use std::os::{fd::AsRawFd, unix::ffi::OsStrExt};
        if self.count >= self.limit {
            return false;
        }
        let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
            return false;
        };
        let mask = libc::IN_MODIFY
            | libc::IN_ATTRIB
            | libc::IN_CLOSE_WRITE
            | libc::IN_CREATE
            | libc::IN_DELETE
            | libc::IN_MOVED_FROM
            | libc::IN_MOVED_TO
            | libc::IN_DELETE_SELF
            | libc::IN_MOVE_SELF
            | libc::IN_UNMOUNT
            | libc::IN_ONLYDIR
            | libc::IN_DONT_FOLLOW;
        let id = unsafe { libc::inotify_add_watch(self.fd.as_raw_fd(), path.as_ptr(), mask) };
        if id < 0 {
            return false;
        }
        self.count += 1;
        true
    }
    pub fn is_clean(&mut self) -> bool {
        use std::os::fd::AsRawFd;
        if self
            .sealed_at
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(60))
        {
            self.dirty = true;
        }
        if self.dirty {
            return false;
        }
        let mut buf = [0u8; 4096];
        let n = unsafe { libc::read(self.fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
        // Every subscribed event invalidates the snapshot, including overflow and
        // watch removal. Reads by the scanner are not subscribed to.
        let untouched =
            n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::WouldBlock;
        self.dirty = !untouched
            || fs::read("/proc/self/mountinfo")
                .map(|m| m != self.mounts)
                .unwrap_or(true);
        !self.dirty
    }
}

pub fn scan(root: &Path, c: &Control, mut update: impl FnMut(Progress)) -> std::io::Result<Report> {
    scan_inner(root, c, &mut update, &mut None)
}
fn scan_inner(
    root: &Path,
    c: &Control,
    update: &mut impl FnMut(Progress),
    tracker: &mut Option<ChangeTracker>,
) -> std::io::Result<Report> {
    let start = Instant::now();
    let mut yielded = Instant::now();
    let meta = fs::symlink_metadata(root)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(std::io::Error::other(crate::tr!(
            "Вибери справжню папку, а не символічне посилання",
            "Choose a real folder, not a symbolic link"
        )));
    }
    let mut r = Report::default();
    r.folders.push(Folder {
        path: root.to_owned(),
        parent: None,
        bytes: allocated(&meta),
        files: 0,
    });
    let dev = meta.dev();
    let mut tasks = vec![0usize];
    let mut seen = HashSet::new();
    let mut biggest = BinaryHeap::<std::cmp::Reverse<BigFile>>::new();
    let mut total = allocated(&meta);
    let mut last = Instant::now();
    while let Some(id) = tasks.pop() {
        if !c.proceed() {
            break;
        }
        let dir = r.folders[id].path.clone();
        if let Some(t) = tracker {
            if !t.watch(&dir) {
                *tracker = None;
            }
        }
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => {
                *tracker = None;
                r.skipped += 1;
                continue;
            }
        };
        for ent in entries {
            if !c.proceed() {
                break;
            }
            let ent = match ent {
                Ok(e) => e,
                Err(_) => {
                    *tracker = None;
                    r.skipped += 1;
                    continue;
                }
            };
            r.entries += 1;
            let m = match ent.metadata() {
                Ok(m) => m,
                Err(_) => {
                    *tracker = None;
                    r.skipped += 1;
                    continue;
                }
            };
            if m.file_type().is_symlink() || m.dev() != dev {
                r.skipped += 1;
                continue;
            }
            if m.is_dir() {
                let child = r.folders.len();
                let bytes = allocated(&m);
                r.folders.push(Folder {
                    path: ent.path(),
                    parent: Some(id),
                    bytes,
                    files: 0,
                });
                tasks.push(child);
                total += bytes;
            } else if m.is_file() {
                // Changes made through a hardlink outside this root need not
                // generate directory events here, so do not cache such a tree.
                if m.nlink() > 1 {
                    *tracker = None;
                }
                if m.nlink() > 1 && !seen.insert((m.dev(), m.ino())) {
                    r.hardlinks += 1;
                    continue;
                }
                let bytes = allocated(&m);
                r.folders[id].bytes += bytes;
                r.folders[id].files += 1;
                total += bytes;
                if biggest.len() < 300
                    || biggest
                        .peek()
                        .is_some_and(|smallest| bytes > smallest.0.bytes)
                {
                    biggest.push(std::cmp::Reverse(BigFile {
                        bytes,
                        path: ent.path(),
                    }));
                    if biggest.len() > 300 {
                        biggest.pop();
                    }
                }
            }
            if last.elapsed() > Duration::from_millis(300) {
                update(Progress {
                    entries: r.entries,
                    bytes: total,
                    current: dir.clone(),
                    skipped: r.skipped,
                });
                last = Instant::now();
            }
        }
        if c.gentle.load(Ordering::Relaxed) && yielded.elapsed() >= Duration::from_millis(50) {
            std::thread::sleep(Duration::from_millis(1));
            yielded = Instant::now();
        }
    }
    for id in (1..r.folders.len()).rev() {
        if let Some(p) = r.folders[id].parent {
            let b = r.folders[id].bytes;
            let n = r.folders[id].files;
            r.folders[p].bytes += b;
            r.folders[p].files += n;
        }
    }
    r.largest = biggest.into_iter().map(|x| x.0).collect();
    r.largest.sort_by(|a, b| b.cmp(a));
    r.elapsed = start.elapsed();
    r.complete = !c.cancel.load(Ordering::Relaxed);
    Ok(r)
}
pub(crate) fn spawn_scan(
    root: PathBuf,
    c: Arc<Control>,
    tx: Sender<Event>,
    persist: bool,
    restore: Option<crate::index::RefreshPaths>,
) {
    std::thread::spawn(move || {
        background_priority();
        let mut tracker = ChangeTracker::new();
        match scan_inner(
            &root,
            &c,
            &mut |p| {
                let _ = tx.send(Event::Progress(p));
            },
            &mut tracker,
        ) {
            Ok(r) => {
                if !r.complete || tracker.as_mut().is_some_and(|t| !t.is_clean()) {
                    tracker = None;
                }
                if let Some(t) = tracker.as_mut() {
                    t.sealed_at = Some(Instant::now());
                }
                let saved = if r.complete && persist {
                    crate::snapshot::save(&r).map(Some)
                } else {
                    Ok(None)
                };
                let index = crate::index::Index::new(&r);
                let resolved = restore.map(|v| v.resolve(&r));
                let _ = tx.send(Event::Finished(r, index, tracker, saved, resolved));
            }
            Err(e) => {
                let _ = tx.send(Event::Error(e.to_string()));
            }
        }
    });
}
// Exact duplicate checking is opt-in: metadata -> equal lengths -> full BLAKE3.
// Hardlinks are excluded; unreadable files are reported, never treated as matches.
pub fn duplicates(
    root: &Path,
    c: &Control,
    mut progress: impl FnMut(Progress),
) -> std::io::Result<(Vec<Vec<BigFile>>, u64)> {
    use std::io::Read;
    let device = fs::metadata(root)?.dev();
    let mut sizes: HashMap<u64, Vec<(PathBuf, u64, u64, u64)>> = HashMap::new();
    let mut seen = HashSet::new();
    let mut skipped = 0;
    let mut entries = 0;
    let mut last = Instant::now();
    for e in walkdir::WalkDir::new(root)
        .follow_links(false)
        .same_file_system(true)
    {
        if !c.proceed() {
            return Err(std::io::Error::other(crate::tr!(
                "Пошук дублікатів скасовано",
                "Duplicate search cancelled"
            )));
        }
        let e = match e {
            Ok(e) => e,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if !e.file_type().is_file() {
            continue;
        }
        let m = match e.metadata() {
            Ok(m) => m,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if m.dev() != device || m.len() < 1024 * 1024 || !seen.insert((m.dev(), m.ino())) {
            continue;
        }
        sizes.entry(m.len()).or_default().push((
            e.path().to_owned(),
            m.ino(),
            m.mtime() as u64,
            m.mtime_nsec() as u64,
        ));
        entries += 1;
        if last.elapsed() > Duration::from_millis(300) {
            progress(Progress {
                entries,
                bytes: 0,
                current: e.path().to_owned(),
                skipped,
            });
            last = Instant::now();
        }
    }
    let mut hashes: HashMap<(u64, blake3::Hash), Vec<BigFile>> = HashMap::new();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut read_bytes = 0;
    for (size, files) in sizes.into_iter().filter(|(_, v)| v.len() > 1) {
        for (p, ino, mtime, nsec) in files {
            let result = (|| -> std::io::Result<blake3::Hash> {
                use std::os::unix::fs::OpenOptionsExt;
                let mut f = fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&p)?;
                let before = f.metadata()?;
                if before.len() != size
                    || before.ino() != ino
                    || before.mtime() as u64 != mtime
                    || before.mtime_nsec() as u64 != nsec
                {
                    return Err(std::io::Error::other(crate::tr!(
                        "Файл змінився",
                        "File changed"
                    )));
                }
                let mut h = blake3::Hasher::new();
                loop {
                    if !c.proceed() {
                        return Err(std::io::Error::other(crate::tr!("Скасовано", "Cancelled")));
                    }
                    let n = f.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    h.update(&buf[..n]);
                    read_bytes += n as u64;
                    if last.elapsed() > Duration::from_millis(300) {
                        progress(Progress {
                            entries,
                            bytes: read_bytes,
                            current: p.clone(),
                            skipped,
                        });
                        last = Instant::now();
                    }
                    if c.gentle.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                let after = f.metadata()?;
                if after.len() != size
                    || after.mtime() != before.mtime()
                    || after.mtime_nsec() != before.mtime_nsec()
                    || after.ctime() != before.ctime()
                    || after.ctime_nsec() != before.ctime_nsec()
                {
                    return Err(std::io::Error::other(crate::tr!(
                        "Файл змінився",
                        "File changed"
                    )));
                }
                Ok(h.finalize())
            })();
            match result {
                Ok(h) => hashes.entry((size, h)).or_default().push(BigFile {
                    bytes: size,
                    path: p,
                }),
                Err(_) => skipped += 1,
            }
        }
    }
    if c.cancel.load(Ordering::Relaxed) {
        return Err(std::io::Error::other(crate::tr!(
            "Пошук дублікатів скасовано",
            "Duplicate search cancelled"
        )));
    }
    let mut groups: Vec<_> = hashes.into_values().filter(|v| v.len() > 1).collect();
    groups.sort_by_key(|v| std::cmp::Reverse(v[0].bytes * (v.len() - 1) as u64));
    Ok((groups, skipped))
}
pub fn disk_space(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let p = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut s = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(p.as_ptr(), s.as_mut_ptr()) } != 0 {
        return None;
    }
    let s = unsafe { s.assume_init() };
    Some((s.f_blocks * s.f_frsize, s.f_bavail * s.f_frsize))
}
pub fn human(n: u64) -> String {
    let units = [
        crate::tr!("Б", "B"),
        crate::tr!("КіБ", "KiB"),
        crate::tr!("МіБ", "MiB"),
        crate::tr!("ГіБ", "GiB"),
        crate::tr!("ТіБ", "TiB"),
    ];
    let mut x = n as f64;
    let mut i = 0;
    while x >= 1024. && i < 4 {
        x /= 1024.;
        i += 1;
    }
    if i == 0 {
        crate::tr!("{n} Б", "{n} B", n = n)
    } else {
        format!("{x:.1} {}", units[i])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("disko-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn scan_avoids_symlinks_and_double_counted_hardlinks() {
        let p = fixture("scan");
        fs::create_dir(p.join("sub")).unwrap();
        fs::write(p.join("a"), vec![1u8; 8192]).unwrap();
        fs::hard_link(p.join("a"), p.join("sub/b")).unwrap();
        std::os::unix::fs::symlink(&p, p.join("loop")).unwrap();
        let r = scan(&p, &Control::default(), |_| {}).unwrap();
        assert!(r.complete);
        assert_eq!(r.hardlinks, 1);
        assert_eq!(r.folders[0].files, 1);
        assert_eq!(r.largest.len(), 1);
        assert_eq!(r.skipped, 1);
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn duplicate_content_not_only_size() {
        let p = fixture("dups");
        fs::write(p.join("a"), vec![1u8; 1024 * 1024]).unwrap();
        fs::copy(p.join("a"), p.join("b")).unwrap();
        fs::write(p.join("different"), vec![2u8; 1024 * 1024]).unwrap();
        fs::hard_link(p.join("a"), p.join("link")).unwrap();
        let (d, _) = duplicates(&p, &Control::default(), |_| {}).unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].len(), 2);
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn cancellation_is_partial() {
        let p = fixture("cancel");
        let c = Control::default();
        c.cancel.store(true, Ordering::Relaxed);
        let r = scan(&p, &c, |_| {}).unwrap();
        assert!(!r.complete);
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn sparse_file_uses_allocated_space() {
        let p = fixture("sparse");
        let f = fs::File::create(p.join("sparse.img")).unwrap();
        f.set_len(1024 * 1024 * 1024).unwrap();
        let r = scan(&p, &Control::default(), |_| {}).unwrap();
        assert_eq!(r.folders[0].files, 1);
        assert!(r.folders[0].bytes < 1024 * 1024);
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn snapshot_detects_content_changes_without_directory_mtime_change() {
        let p = fixture("snapshot");
        fs::create_dir(p.join("nested")).unwrap();
        let file = p.join("nested/file");
        fs::write(&file, vec![1u8; 8192]).unwrap();
        let mut tracker = ChangeTracker::new();
        assert!(tracker.is_some());
        let r = scan_inner(&p, &Control::default(), &mut |_| {}, &mut tracker).unwrap();
        assert!(r.complete);
        let t = tracker.as_mut().unwrap();
        assert!(t.is_clean());
        assert!(
            t.is_clean(),
            "checking an untouched snapshot must not dirty it"
        );
        let dir_time = fs::metadata(p.join("nested")).unwrap().modified().unwrap();
        fs::write(&file, vec![2u8; 32768]).unwrap();
        assert_eq!(
            dir_time,
            fs::metadata(p.join("nested")).unwrap().modified().unwrap()
        );
        assert!(
            !t.is_clean(),
            "file growth must invalidate even if directory mtime is unchanged"
        );
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn incomplete_watch_coverage_and_hardlinks_disable_snapshot_reuse() {
        let p = fixture("watch-limit");
        fs::create_dir(p.join("sub")).unwrap();
        let mut tracker = ChangeTracker::new();
        tracker.as_mut().unwrap().limit = 1;
        scan_inner(&p, &Control::default(), &mut |_| {}, &mut tracker).unwrap();
        assert!(tracker.is_none());
        fs::write(p.join("a"), b"test").unwrap();
        fs::hard_link(p.join("a"), p.join("sub/b")).unwrap();
        let mut tracker = ChangeTracker::new();
        let r = scan_inner(&p, &Control::default(), &mut |_| {}, &mut tracker).unwrap();
        assert_eq!(r.folders[0].files, 1);
        assert!(
            tracker.is_none(),
            "hardlink edits through an unwatched alias require full validation"
        );
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn snapshot_detects_new_deleted_and_moved_subtrees() {
        let p = fixture("watch-tree");
        fs::create_dir(p.join("sub")).unwrap();
        let mut tracker = ChangeTracker::new();
        scan_inner(&p, &Control::default(), &mut |_| {}, &mut tracker).unwrap();
        assert!(tracker.as_mut().unwrap().is_clean());
        fs::create_dir(p.join("sub/new")).unwrap();
        assert!(!tracker.as_mut().unwrap().is_clean());
        let mut tracker = ChangeTracker::new();
        scan_inner(&p, &Control::default(), &mut |_| {}, &mut tracker).unwrap();
        assert!(tracker.as_mut().unwrap().is_clean());
        fs::rename(p.join("sub"), p.join("renamed")).unwrap();
        assert!(!tracker.as_mut().unwrap().is_clean());
        let mut tracker = ChangeTracker::new();
        scan_inner(&p, &Control::default(), &mut |_| {}, &mut tracker).unwrap();
        assert!(tracker.as_mut().unwrap().is_clean());
        fs::remove_dir(p.join("renamed/new")).unwrap();
        assert!(!tracker.as_mut().unwrap().is_clean());
        let mut fresh = ChangeTracker::new().unwrap();
        assert!(fresh.watch(&p));
        fresh.sealed_at = Some(Instant::now() - Duration::from_secs(61));
        assert!(
            !fresh.is_clean(),
            "old snapshots require full validation even without events"
        );
        fs::remove_dir_all(p).unwrap();
    }
}
