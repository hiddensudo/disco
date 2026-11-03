use crate::scan::Report;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde::{Deserialize, Serialize};
use std::os::unix::fs::OpenOptionsExt;
use std::{
    fs,
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const VERSION: u32 = 2;
const MAGIC: &[u8; 8] = b"DISCO\0\x02\n";
const MAX_COMPRESSED: u64 = 64 * 1024 * 1024;
const MAX_UNPACKED: u64 = 512 * 1024 * 1024;

// Linux filenames are bytes, not necessarily UTF-8. Compression keeps the byte
// arrays small while preserving names losslessly through a restart.
pub mod path_bytes {
    use super::*;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    pub fn serialize<S: serde::Serializer>(path: &Path, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(path.as_os_str().as_bytes())
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<PathBuf, D::Error> {
        struct Bytes;
        impl<'de> serde::de::Visitor<'de> for Bytes {
            type Value = PathBuf;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("Linux path bytes")
            }
            fn visit_bytes<E: serde::de::Error>(self, b: &[u8]) -> Result<PathBuf, E> {
                Ok(std::ffi::OsString::from_vec(b.to_vec()).into())
            }
            fn visit_byte_buf<E: serde::de::Error>(self, b: Vec<u8>) -> Result<PathBuf, E> {
                Ok(std::ffi::OsString::from_vec(b).into())
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<PathBuf, A::Error> {
                let mut b = Vec::new();
                while let Some(v) = seq.next_element::<u8>()? {
                    b.push(v);
                }
                self.visit_byte_buf(b)
            }
        }
        d.deserialize_byte_buf(Bytes)
    }
}

#[derive(Debug, Deserialize)]
pub struct SavedScan {
    version: u32,
    pub saved_at: u64,
    pub report: Report,
    #[serde(default)]
    pub disk: Option<crate::refresh::DiskSample>,
}
#[derive(Serialize)]
struct Envelope<'a> {
    version: u32,
    saved_at: u64,
    report: &'a Report,
    disk: Option<crate::refresh::DiskSample>,
}

pub fn file() -> PathBuf {
    if let Some(p) = std::env::var_os("DISCO_CONFIG_DIR") {
        return PathBuf::from(p).join("cache/last-scan.bin.gz");
    }
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache")
        });
    cache.join("disco/last-scan.bin.gz")
}

fn validate(s: &SavedScan) -> Result<(), String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    if s.saved_at > now.saturating_add(86_400) {
        return Err(crate::tr!("Неправильна дата знімка", "Invalid snapshot date").into());
    }
    let r = &s.report;
    if !matches!(s.version, 1 | VERSION)
        || !r.complete
        || r.folders.is_empty()
        || r.folders.len() > 1_000_000
        || r.largest.len() > 300
    {
        return Err(crate::tr!(
            "Несумісний або неповний знімок сканування",
            "Incompatible or incomplete scan snapshot"
        )
        .into());
    }
    let root = &r.folders[0];
    if root.parent.is_some() || !root.path.is_absolute() {
        return Err(crate::tr!("Неправильний корінь знімка", "Invalid snapshot root").into());
    }
    let mut sums = vec![(0u64, 0u64); r.folders.len()];
    for (i, f) in r.folders.iter().enumerate() {
        if !f.path.starts_with(&root.path)
            || f.path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(
                crate::tr!("Неправильний шлях у знімку", "Invalid path in snapshot").into(),
            );
        }
        if i == 0 {
            continue;
        }
        let p = f.parent.filter(|p| *p < i).ok_or(crate::tr!(
            "Неправильне дерево знімка",
            "Invalid snapshot tree"
        ))?;
        if f.path.parent() != Some(r.folders[p].path.as_path()) {
            return Err(
                crate::tr!("Невідповідний батьківський шлях", "Mismatched parent path").into(),
            );
        }
        sums[p].0 = sums[p]
            .0
            .checked_add(f.bytes)
            .ok_or(crate::tr!("Переповнення розміру", "Size overflow"))?;
        sums[p].1 = sums[p]
            .1
            .checked_add(f.files)
            .ok_or(crate::tr!("Переповнення кількості", "Count overflow"))?;
    }
    if r.folders
        .iter()
        .zip(sums)
        .any(|(f, sum)| sum.0 > f.bytes || sum.1 > f.files)
    {
        return Err(crate::tr!("Неправильні підсумки знімка", "Invalid snapshot totals").into());
    }
    if r.largest.iter().any(|f| {
        !f.path.starts_with(&root.path)
            || f.bytes > root.bytes
            || f.path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
    }) {
        return Err(crate::tr!(
            "Неправильний список великих файлів",
            "Invalid large files list"
        )
        .into());
    }
    Ok(())
}

struct BoundedWriter<W> {
    inner: W,
    bytes: u64,
}
impl<W: Write> Write for BoundedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if (buf.len() as u64) > MAX_UNPACKED.saturating_sub(self.bytes) {
            return Err(std::io::Error::other(crate::tr!(
                "Знімок перевищує допустимий розмір",
                "Snapshot exceeds the size limit"
            )));
        }
        let n = self.inner.write(buf)?;
        self.bytes += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub fn load() -> Result<Option<SavedScan>, String> {
    load_cached(&file())
}
fn load_cached(path: &Path) -> Result<Option<SavedScan>, String> {
    let legacy = path.with_file_name("last-scan.json.gz");
    let legacy_newer = fs::metadata(&legacy)
        .and_then(|m| m.modified())
        .ok()
        .zip(fs::metadata(path).and_then(|m| m.modified()).ok())
        .is_some_and(|(old, new)| old > new);
    if path.exists() && !legacy_newer {
        return load_from(path);
    }
    let saved = load_from(&legacy)?;
    if let Some(s) = &saved {
        // Preserve the original scan date. A legacy snapshot has no disk baseline.
        let _ = write_to(&path, &s.report, s.saved_at, s.disk);
    }
    Ok(saved)
}
fn load_from(path: &Path) -> Result<Option<SavedScan>, String> {
    let f = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    if f.metadata().map_err(|e| e.to_string())?.len() > MAX_COMPRESSED {
        return Err(crate::tr!(
            "Знімок перевищує допустимий розмір",
            "Snapshot exceeds the size limit"
        )
        .into());
    }
    let mut reader = BufReader::new(GzDecoder::new(BufReader::new(f))).take(MAX_UNPACKED + 1);
    let mut prefix = [0; 8];
    reader.read_exact(&mut prefix).map_err(|e| e.to_string())?;
    let saved: SavedScan = if &prefix == MAGIC {
        let saved = bincode::serde::decode_from_std_read(
            &mut reader,
            bincode::config::standard().with_limit::<536870912>(),
        )
        .map_err(|e| e.to_string())?;
        let mut tail = Vec::new();
        reader.read_to_end(&mut tail).map_err(|e| e.to_string())?;
        if !tail.is_empty() {
            return Err(crate::tr!("Зайві дані у знімку", "Unexpected data in snapshot").into());
        }
        saved
    } else {
        serde_json::from_reader(prefix.as_slice().chain(reader)).map_err(|e| e.to_string())?
    };
    validate(&saved)?;
    Ok(Some(saved))
}

pub fn save(report: &Report) -> Result<u64, String> {
    save_profile(&file(), report)
}
#[derive(Serialize, Deserialize)]
struct RootRecord {
    #[serde(with = "path_bytes")]
    root: PathBuf,
    saved_at: u64,
}
fn archive_dir(last: &Path, root: &Path) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    last.parent()
        .unwrap()
        .join("scans")
        .join(blake3::hash(root.as_os_str().as_bytes()).to_hex().as_str())
}
fn archive(
    last: &Path,
    report: &Report,
    saved_at: u64,
    disk: Option<crate::refresh::DiskSample>,
) -> Result<(), String> {
    let root = &report.folders[0].path;
    let dir = archive_dir(last, root);
    write_to(&dir.join("snapshot.bin.gz"), report, saved_at, disk)?;
    let record = serde_json::to_vec(&RootRecord {
        root: root.clone(),
        saved_at,
    })
    .map_err(|e| e.to_string())?;
    // Writers are serialized for this profile; readers see only complete records.
    let temp = dir.join(".root.tmp");
    fs::write(&temp, record).map_err(|e| e.to_string())?;
    fs::rename(temp, dir.join("root.json")).map_err(|e| e.to_string())?;
    Ok(())
}
fn save_profile(last: &Path, report: &Report) -> Result<u64, String> {
    if !report.complete || report.folders.is_empty() {
        return Err(crate::tr!(
            "Частковий скан не замінює попередній знімок",
            "A partial scan cannot replace the previous snapshot"
        )
        .into());
    }
    let parent = last.parent().ok_or("Missing cache folder")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(parent.join(".snapshots.lock"))
        .map_err(|e| e.to_string())?;
    use std::os::fd::AsRawFd;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // Keep the old single-file snapshot before the first newer root replaces it.
    // A damaged legacy file must not prevent saving a valid new scan.
    if let Ok(Some(previous)) = load_cached(last) {
        let target = archive_dir(last, &previous.report.folders[0].path).join("snapshot.bin.gz");
        if !target.exists() {
            archive(last, &previous.report, previous.saved_at, previous.disk)?;
        }
    }
    let saved_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    archive(
        last,
        report,
        saved_at,
        crate::refresh::DiskSample::capture(&report.folders[0].path),
    )?;
    // Retain the ordinary last-workspace entry point and compatibility with old builds.
    save_to(last, report)
}

/// Choose one cached workspace for the requested paths, without scanning or
/// combining independent timestamps into an invented complete filesystem scan.
pub fn load_for(targets: &[PathBuf]) -> Result<Option<SavedScan>, String> {
    load_for_at(&file(), targets)
}
fn load_for_at(last: &Path, targets: &[PathBuf]) -> Result<Option<SavedScan>, String> {
    if targets.is_empty() {
        return load_cached(last);
    }
    let score = |root: &Path, date: u64| {
        let covered = targets.iter().filter(|p| p.starts_with(root)).count();
        let partial = targets.iter().filter(|p| root.starts_with(p)).count();
        (covered, partial, root.components().count(), date)
    };
    let mut candidates = Vec::new();
    if let Ok(entries) = fs::read_dir(last.parent().unwrap().join("scans")) {
        for entry in entries.take(4096).flatten() {
            let dir = entry.path();
            let meta = dir.join("root.json");
            if fs::metadata(&meta).is_ok_and(|m| m.len() <= 1_048_576) {
                if let Ok(bytes) = fs::read(&meta) {
                    if let Ok(record) = serde_json::from_slice::<RootRecord>(&bytes) {
                        if archive_dir(last, &record.root) == dir {
                            let rank = score(&record.root, record.saved_at);
                            if rank.0 + rank.1 > 0 {
                                candidates.push((rank, record.root, dir.join("snapshot.bin.gz")));
                            }
                        }
                    }
                }
            }
        }
    }
    let legacy = load_cached(last);
    if let Ok(Some(s)) = &legacy {
        let root = &s.report.folders[0].path;
        let rank = score(root, s.saved_at);
        if rank.0 + rank.1 > 0 {
            candidates.push((rank, root.clone(), last.to_path_buf()));
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));
    let mut error = None;
    for (_, root, path) in candidates {
        match load_from(&path) {
            Ok(Some(s)) if s.report.folders[0].path == root => return Ok(Some(s)),
            Err(e) => error = Some(e),
            _ => {}
        }
    }
    match error {
        Some(e) => Err(e),
        None => legacy.map(|_| None),
    }
}

fn save_to(path: &Path, report: &Report) -> Result<u64, String> {
    if !report.complete || report.folders.is_empty() || report.folders.len() > 1_000_000 {
        return Err(crate::tr!(
            "Частковий скан не замінює попередній знімок",
            "A partial scan cannot replace the previous snapshot"
        )
        .into());
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    write_to(
        path,
        report,
        now.as_secs(),
        crate::refresh::DiskSample::capture(&report.folders[0].path),
    )
}
fn write_to(
    path: &Path,
    report: &Report,
    saved_at: u64,
    disk: Option<crate::refresh::DiskSample>,
) -> Result<u64, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    let parent = path
        .parent()
        .ok_or(crate::tr!("Немає папки кешу", "Missing cache folder"))?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = parent.join(format!(
        ".scan-{}-{}.tmp",
        std::process::id(),
        now.as_nanos()
    ));
    let result = (|| {
        let f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        let mut gzip = GzEncoder::new(BufWriter::new(f), Compression::fast());
        gzip.write_all(MAGIC)?;
        bincode::serde::encode_into_std_write(
            &Envelope {
                version: VERSION,
                saved_at,
                report,
                disk,
            },
            &mut BoundedWriter {
                inner: &mut gzip,
                bytes: 0,
            },
            bincode::config::standard(),
        )
        .map_err(std::io::Error::other)?;
        let mut out = gzip.finish()?;
        out.flush()?;
        out.get_ref().sync_all()?;
        if out.get_ref().metadata()?.len() > MAX_COMPRESSED {
            return Err(std::io::Error::other(crate::tr!(
                "Знімок перевищує допустимий розмір",
                "Snapshot exceeds the size limit"
            )));
        }
        drop(out);
        fs::rename(&tmp, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok::<_, std::io::Error>(saved_at)
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{Control, scan};
    use std::os::unix::ffi::OsStringExt;
    #[test]
    fn per_root_archives_preserve_legacy_and_independent_concurrent_scans() {
        let dir = std::env::temp_dir().join(format!("disco-root-archive-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("tree/practice/sub")).unwrap();
        fs::create_dir_all(dir.join("other")).unwrap();
        let broad = scan(&dir.join("tree"), &Control::default(), |_| {}).unwrap();
        let narrow = scan(&dir.join("tree/practice"), &Control::default(), |_| {}).unwrap();
        let other = scan(&dir.join("other"), &Control::default(), |_| {}).unwrap();
        let last = dir.join("cache/last-scan.bin.gz");
        save_to(&last, &broad).unwrap(); // A pre-upgrade profile.
        let original = fs::read(&last).unwrap();
        let path = last.clone();
        let a = std::thread::spawn(move || save_profile(&path, &narrow).unwrap());
        let path = last.clone();
        let b = std::thread::spawn(move || save_profile(&path, &other).unwrap());
        a.join().unwrap();
        b.join().unwrap();
        let restored = load_for_at(&last, &[dir.join("tree")]).unwrap().unwrap();
        assert_eq!(restored.report.folders[0].path, dir.join("tree"));
        assert_eq!(
            fs::read(archive_dir(&last, &dir.join("tree")).join("snapshot.bin.gz")).unwrap(),
            original
        );
        for root in ["tree/practice", "other"] {
            assert_eq!(
                load_for_at(&last, &[dir.join(root)])
                    .unwrap()
                    .unwrap()
                    .report
                    .folders[0]
                    .path,
                dir.join(root)
            );
        }
        assert!(
            load_for_at(&last, &[dir.join("unrelated")])
                .unwrap()
                .is_none()
        );
        let mut cancelled = restored.report.clone();
        cancelled.complete = false;
        assert!(save_profile(&last, &cancelled).is_err());
        // A damaged narrow archive falls back to its intact broader workspace.
        fs::write(
            archive_dir(&last, &dir.join("tree/practice")).join("snapshot.bin.gz"),
            b"damaged",
        )
        .unwrap();
        // Remove only the compatibility last file so it cannot mask corruption.
        fs::remove_file(&last).unwrap();
        assert_eq!(
            load_for_at(&last, &[dir.join("tree/practice")])
                .unwrap()
                .unwrap()
                .report
                .folders[0]
                .path,
            dir.join("tree")
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn last_snapshot_is_one_root_not_a_union_or_archive() {
        let root =
            std::env::temp_dir().join(format!("disco-snapshot-scope-{}", std::process::id()));
        fs::create_dir_all(root.join("tree/practice/sub")).unwrap();
        fs::create_dir_all(root.join("tree/other")).unwrap();
        let broad = scan(&root.join("tree"), &Control::default(), |_| {}).unwrap();
        let narrow = scan(&root.join("tree/practice"), &Control::default(), |_| {}).unwrap();
        let path = root.join("cache/last-scan.bin.gz");
        save_to(&path, &broad).unwrap();
        let prior = fs::read(&path).unwrap();
        // Independent windows share this atomic destination. The last completed
        // writer wins; atomicity prevents corruption, not a narrower scope.
        let target = path.clone();
        std::thread::spawn(move || save_to(&target, &narrow).unwrap())
            .join()
            .unwrap();
        let saved = load_from(&path).unwrap().unwrap();
        assert_eq!(saved.report.folders[0].path, root.join("tree/practice"));
        assert!(saved.report.folders.len() < broad.folders.len());
        assert_ne!(fs::read(&path).unwrap(), prior);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn snapshot_survives_restart_and_preserves_non_utf8_paths() {
        let dir = std::env::temp_dir().join(format!("disco-snapshot-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("tree/sub")).unwrap();
        fs::write(
            dir.join("tree/sub")
                .join(std::ffi::OsString::from_vec(vec![b'f', 0xff])),
            vec![1u8; 8192],
        )
        .unwrap();
        let r = scan(&dir.join("tree"), &Control::default(), |_| {}).unwrap();
        let path = dir.join("cache/snapshot.gz");
        let date = save_to(&path, &r).unwrap();
        drop(r);
        let loaded = load_from(&path).unwrap().unwrap();
        assert_eq!(loaded.saved_at, date);
        assert_eq!(loaded.report.folders[0].files, 1);
        assert_eq!(loaded.report.largest.len(), 1);
        assert_eq!(
            loaded.report.largest[0].path.file_name().unwrap(),
            std::ffi::OsString::from_vec(vec![b'f', 0xff])
        );
        let previous = fs::read(&path).unwrap();
        let mut cancelled = loaded.report;
        cancelled.complete = false;
        assert!(save_to(&path, &cancelled).is_err());
        assert_eq!(fs::read(&path).unwrap(), previous);
        // An interrupted save leaves the old atomic snapshot readable.
        fs::write(dir.join("cache/.interrupted.tmp"), b"half a file").unwrap();
        assert!(load_from(&path).unwrap().is_some());
        let mut corrupted = fs::read(&path).unwrap();
        corrupted.truncate(corrupted.len() / 2);
        fs::write(&path, corrupted).unwrap();
        assert!(load_from(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn legacy_json_migrates_without_changing_scan_date_or_inventing_disk_sample() {
        let dir = std::env::temp_dir().join(format!("disco-legacy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("tree/sub")).unwrap();
        let report = scan(&dir.join("tree"), &Control::default(), |_| {}).unwrap();
        let legacy = dir.join("last-scan.json.gz");
        let mut value = serde_json::to_value(Envelope {
            version: 1,
            saved_at: 123,
            report: &report,
            disk: None,
        })
        .unwrap();
        value.as_object_mut().unwrap().remove("disk");
        let mut out = GzEncoder::new(fs::File::create(&legacy).unwrap(), Compression::fast());
        serde_json::to_writer(&mut out, &value).unwrap();
        out.finish().unwrap();
        let binary = dir.join("last-scan.bin.gz");
        let first = load_cached(&binary).unwrap().unwrap();
        assert_eq!(first.saved_at, 123);
        assert_eq!(first.disk, None);
        let migrated = load_from(&binary).unwrap().unwrap();
        assert_eq!(migrated.version, VERSION);
        assert_eq!(migrated.saved_at, 123);
        assert_eq!(migrated.disk, None);
        assert_eq!(migrated.report.folders.len(), 2);
        // New saves retain the volume identity and reject even a corrupt gzip trailer.
        save_to(&binary, &report).unwrap();
        assert!(load_from(&binary).unwrap().unwrap().disk.is_some());
        let mut bytes = fs::read(&binary).unwrap();
        let n = bytes.len();
        bytes[n - 5] ^= 1;
        fs::write(&binary, bytes).unwrap();
        assert!(load_from(&binary).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn invalid_parent_indices_and_old_formats_are_rejected() {
        let root = std::env::temp_dir().join(format!("disco-invalid-{}", std::process::id()));
        fs::create_dir_all(root.join("sub")).unwrap();
        let report = scan(&root, &Control::default(), |_| {}).unwrap();
        let mut saved = SavedScan {
            version: VERSION,
            saved_at: 0,
            report,
            disk: None,
        };
        assert!(validate(&saved).is_ok());
        saved.report.folders[1].parent = Some(1);
        assert!(validate(&saved).is_err());
        saved.report.folders[1].parent = Some(0);
        saved.version = 99;
        assert!(validate(&saved).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
