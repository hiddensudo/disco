//! Merge scoped filesystem reads into a snapshot. Unrelated subtrees are never
//! visited. All cloning, traversal, aggregation and indexing happens off the UI.
use crate::scan::{self, BigFile, Control, Folder, Report};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
};

pub(crate) fn normalize_roots(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots.sort();
    let mut unique: Vec<PathBuf> = Vec::new();
    for path in roots {
        if unique.last().is_none_or(|root| !path.starts_with(root)) {
            unique.push(path);
        }
    }
    unique
}
#[derive(Clone)]
struct Direct {
    bytes: u64,
    files: u64,
}
fn nodes(report: &Report) -> HashMap<PathBuf, Direct> {
    let mut direct: Vec<_> = report
        .folders
        .iter()
        .map(|f| Direct {
            bytes: f.bytes,
            files: f.files,
        })
        .collect();
    for f in &report.folders {
        if let Some(parent) = f.parent {
            direct[parent].bytes = direct[parent].bytes.saturating_sub(f.bytes);
            direct[parent].files = direct[parent].files.saturating_sub(f.files);
        }
    }
    report
        .folders
        .iter()
        .zip(direct)
        .map(|(f, d)| (f.path.clone(), d))
        .collect()
}
#[cfg(test)]
pub(crate) fn refresh(
    base: &Report,
    roots: &[PathBuf],
    shallow: bool,
    control: &Control,
) -> Result<Report, String> {
    refresh_category(base, roots, shallow, control, None)
}
pub(crate) fn refresh_category(
    base: &Report,
    roots: &[PathBuf],
    shallow: bool,
    control: &Control,
    category: Option<crate::index::Category>,
) -> Result<Report, String> {
    let trash_roots = if category.is_some() {
        crate::recycle::scan_roots()
    } else {
        vec![]
    };
    let matches = |path: &std::path::Path| {
        category.is_none_or(|category| crate::index::classify(path, &trash_roots) == category)
    };
    if base.folders.is_empty() {
        return Err("Missing base snapshot".into());
    }
    let normalized = normalize_roots(roots.to_vec());
    if category.is_none()
        && !shallow
        && normalized.len() == 1
        && normalized[0] == base.folders[0].path
    {
        let report = scan::scan(&normalized[0], control, |_| {}).map_err(|e| e.to_string())?;
        if !report.complete || report.skipped > 0 {
            return Err("Incomplete scope refresh".into());
        }
        return Ok(report);
    }
    let started = std::time::Instant::now();
    let mut map = nodes(base);
    let mut largest = base.largest.clone();
    let mut complete = base.complete && base.hardlinks == 0;
    let mut skipped = 0;
    let reads = if shallow {
        let mut paths = roots.to_vec();
        paths.sort();
        paths.dedup();
        paths
    } else {
        normalize_roots(roots.to_vec())
    };
    for root in reads {
        if !control.proceed() {
            return Err(crate::tr!("Оновлення скасовано", "Refresh cancelled").into());
        }
        let metadata = match fs::symlink_metadata(&root) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                map.retain(|p, _| !p.starts_with(&root));
                largest.retain(|f| !f.path.starts_with(&root));
                continue;
            }
            Err(e) => return Err(format!("{}: {e}", root.display())),
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(format!("{}: expected a real directory", root.display()));
        }
        if !shallow {
            let report =
                scan::scan_category(&root, control, category).map_err(|e| e.to_string())?;
            if !report.complete || report.skipped > 0 {
                return Err(format!(
                    "{}: incomplete refresh ({} skipped)",
                    root.display(),
                    report.skipped
                ));
            }
            complete &= report.hardlinks == 0;
            let replacement = nodes(&report);
            let removed = normalize_roots(
                map.keys()
                    .filter(|p| p.starts_with(&root) && matches(p) && !replacement.contains_key(*p))
                    .cloned()
                    .collect(),
            );
            let deleted = |p: &std::path::Path| {
                let index = removed.partition_point(|root| root.as_path() <= p);
                index > 0 && p.starts_with(&removed[index - 1])
            };
            map.retain(|p, _| !deleted(p) && (!p.starts_with(&root) || !matches(p)));
            map.extend(replacement);
            largest.retain(|f| {
                !deleted(&f.path)
                    && (!f.path.starts_with(&root) || !f.path.parent().is_some_and(&matches))
            });
            largest.extend(report.largest);
            skipped += report.skipped;
        } else {
            let mut direct = Direct {
                bytes: scan::allocated(&metadata),
                files: 0,
            };
            let mut children = HashSet::new();
            largest.retain(|f| f.path.parent() != Some(root.as_path()));
            for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
                if !control.proceed() {
                    return Err("Refresh cancelled".into());
                }
                let entry = entry.map_err(|e| e.to_string())?;
                let m = match fs::symlink_metadata(entry.path()) {
                    Ok(m) => m,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(e.to_string()),
                };
                use std::os::unix::fs::MetadataExt;
                if m.file_type().is_symlink() || m.dev() != metadata.dev() {
                    continue;
                }
                if m.is_dir() {
                    let path = entry.path();
                    children.insert(path.clone());
                    if !matches(&path) {
                        continue;
                    }
                    if !map.contains_key(&path) {
                        let report = scan::scan_category(&path, control, category)
                            .map_err(|e| e.to_string())?;
                        if !report.complete || report.skipped > 0 {
                            return Err(format!("{}: incomplete refresh", path.display()));
                        }
                        complete &= report.hardlinks == 0;
                        map.extend(nodes(&report));
                        largest.extend(report.largest);
                    }
                } else if m.is_file() {
                    complete &= m.nlink() <= 1;
                    let bytes = scan::allocated(&m);
                    direct.bytes += bytes;
                    direct.files += 1;
                    largest.push(BigFile {
                        path: entry.path(),
                        bytes,
                    });
                }
            }
            let removed: Vec<_> = map
                .keys()
                .filter(|p| p.parent() == Some(root.as_path()) && !children.contains(*p))
                .cloned()
                .collect();
            for path in removed {
                map.retain(|p, _| !p.starts_with(&path));
                largest.retain(|f| !f.path.starts_with(&path));
            }
            map.insert(root, direct);
        }
    }
    if map.is_empty() {
        return Err(crate::tr!("Папка більше не існує", "The folder no longer exists").into());
    }
    let mut root = base.folders[0].path.clone();
    for path in map.keys() {
        while !path.starts_with(&root) {
            if !root.pop() {
                break;
            }
        }
    }
    let mut missing = Vec::new();
    for path in map.keys() {
        let mut at = path.parent();
        while let Some(parent) = at {
            if !parent.starts_with(&root) {
                break;
            }
            if !map.contains_key(parent) {
                missing.push(parent.to_path_buf());
            }
            at = parent.parent();
        }
    }
    if !missing.is_empty() || root != base.folders[0].path {
        complete = false;
    }
    for path in missing {
        map.entry(path).or_insert(Direct { bytes: 0, files: 0 });
    }
    let mut paths: Vec<_> = map.keys().cloned().collect();
    paths.sort_by(|a, b| {
        a.components()
            .count()
            .cmp(&b.components().count())
            .then(a.cmp(b))
    });
    let mut ids = HashMap::new();
    let mut folders: Vec<Folder> = Vec::with_capacity(paths.len());
    for path in paths {
        let parent = path.parent().and_then(|p| ids.get(p).copied());
        let direct = &map[&path];
        ids.insert(path.clone(), folders.len());
        folders.push(Folder {
            path,
            parent,
            bytes: direct.bytes,
            files: direct.files,
        });
    }
    for i in (0..folders.len()).rev() {
        if let Some(parent) = folders[i].parent {
            folders[parent].bytes += folders[i].bytes;
            folders[parent].files += folders[i].files;
        }
    }
    largest.retain(|f| f.path.parent().is_some_and(|p| ids.contains_key(p)));
    largest.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.path.cmp(&b.path)));
    let mut seen = HashSet::new();
    largest.retain(|f| seen.insert(f.path.clone()));
    largest.truncate(300);
    Ok(Report {
        entries: folders[0].files + folders.len() as u64 - 1,
        folders,
        largest,
        elapsed: started.elapsed(),
        skipped: base.skipped + skipped,
        hardlinks: base.hardlinks,
        complete,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_category_refresh_handles_mixed_parents_without_reading_other_categories() {
        let root = std::env::temp_dir().join(format!("disco-mixed-{}", std::process::id()));
        fs::create_dir_all(root.join("Games")).unwrap();
        fs::create_dir_all(root.join("old/Games")).unwrap();
        fs::write(root.join("needle.txt"), "before").unwrap();
        fs::write(root.join("Games/stay"), vec![1u8; 9000]).unwrap();
        let c = Control::default();
        let base = scan::scan(&root, &c, |_| {}).unwrap();
        let game = base
            .folders
            .iter()
            .find(|f| f.path == root.join("Games"))
            .unwrap()
            .bytes;
        fs::write(root.join("Games/stay"), vec![1u8; 50000]).unwrap();
        fs::write(root.join("needle.txt"), vec![1u8; 17000]).unwrap();
        fs::remove_dir_all(root.join("old")).unwrap();
        let refreshed = refresh_category(
            &base,
            &[root.clone()],
            false,
            &c,
            Some(crate::index::Category::Other),
        )
        .unwrap();
        assert_eq!(
            refreshed
                .folders
                .iter()
                .find(|f| f.path == root.join("Games"))
                .unwrap()
                .bytes,
            game
        );
        assert!(
            !refreshed
                .folders
                .iter()
                .any(|f| f.path.starts_with(root.join("old")))
        );
        assert!(refreshed.folders[0].bytes > base.folders[0].bytes);
        let index = crate::index::Index::new(&refreshed);
        assert_eq!(
            index.category_scope_roots(&refreshed, crate::index::Category::Other),
            vec![0]
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "manual performance sample"]
    fn benchmark_small_scope_inside_large_snapshot() {
        let root = std::env::temp_dir().join(format!("disco-scope-bench-{}", std::process::id()));
        fs::create_dir_all(root.join("active")).unwrap();
        fs::write(root.join("active/config.rs"), "fn main() {}\n").unwrap();
        let control = Control::default();
        let mut base = scan::scan(&root, &control, |_| {}).unwrap();
        let parent = base.folders.len();
        base.folders.push(Folder {
            path: root.join("untouched"),
            parent: Some(0),
            bytes: 0,
            files: 0,
        });
        for i in 0..50000 {
            base.folders.push(Folder {
                path: root.join("untouched").join(format!("folder-{i}")),
                parent: Some(parent),
                bytes: 0,
                files: 0,
            });
        }
        let start = std::time::Instant::now();
        let result = refresh(&base, &[root.join("active")], true, &control).unwrap();
        let merge = start.elapsed();
        let start = std::time::Instant::now();
        let index = crate::index::Index::new(&result);
        let indexing = start.elapsed();
        assert_eq!(index.folder_count(), base.folders.len());
        eprintln!(
            "50k unrelated folders, shallow read+merge {:.2} ms; background index {:.2} ms",
            merge.as_secs_f64() * 1000.,
            indexing.as_secs_f64() * 1000.
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn scoped_and_shallow_updates_leave_unrelated_data_untouched() {
        let root = std::env::temp_dir().join(format!("disco-scope-{}", std::process::id()));
        fs::create_dir_all(root.join("a/nested")).unwrap();
        fs::create_dir_all(root.join("b")).unwrap();
        fs::write(root.join("a/nested/file"), "before").unwrap();
        fs::write(root.join("b/stay"), vec![1u8; 9000]).unwrap();
        let c = Control::default();
        let base = scan::scan(&root, &c, |_| {}).unwrap();
        let b = base
            .folders
            .iter()
            .find(|f| f.path == root.join("b"))
            .unwrap()
            .bytes;
        fs::remove_dir_all(root.join("b")).unwrap(); // unrelated subtree must not be read
        fs::write(root.join("a/nested/file"), vec![2u8; 17000]).unwrap();
        fs::write(root.join("a/direct"), vec![3u8; 9000]).unwrap();
        let both = refresh(&base, &[root.join("a"), root.join("a/nested")], true, &c).unwrap();
        assert!(
            both.folders
                .iter()
                .find(|f| f.path == root.join("a/nested"))
                .unwrap()
                .bytes
                > base
                    .folders
                    .iter()
                    .find(|f| f.path == root.join("a/nested"))
                    .unwrap()
                    .bytes,
            "shallow reads cannot discard a nested dirty directory"
        );
        let refreshed = refresh(&base, &[root.join("a/nested")], true, &c).unwrap();
        assert_eq!(
            refreshed
                .folders
                .iter()
                .find(|f| f.path == root.join("b"))
                .unwrap()
                .bytes,
            b
        );
        assert!(refreshed.folders[0].bytes > base.folders[0].bytes);
        fs::remove_dir_all(root.join("a/nested")).unwrap();
        fs::create_dir_all(root.join("a/new/deep")).unwrap();
        fs::write(root.join("a/new/deep/test"), "new").unwrap();
        let next = refresh(&refreshed, &[root.join("a")], true, &c).unwrap();
        assert!(!next.folders.iter().any(|f| f.path == root.join("a/nested")));
        assert!(
            next.folders
                .iter()
                .any(|f| f.path == root.join("a/new/deep"))
        );
        let full = refresh(&next, &[root.join("a")], false, &c).unwrap();
        assert_eq!(
            full.folders
                .iter()
                .find(|f| f.path == root.join("b"))
                .unwrap()
                .bytes,
            b
        );
        let index = crate::index::Index::new(&full);
        assert_eq!(index.folder_count(), full.folders.len());
        fs::remove_dir_all(root).unwrap();
    }
}
