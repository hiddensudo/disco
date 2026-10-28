//! Scan classification and read-only navigation index, shared by workers and UI.
use crate::{recycle, scan::Report};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Category {
    All,
    Games,
    Development,
    Cache,
    Documents,
    Other,
    Trash,
    Custom(u64),
}
impl Category {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::All => crate::tr!("Усі папки", "All folders"),
            Self::Games => crate::tr!("Ігри та моди", "Games & mods"),
            Self::Development => crate::tr!("Розробка", "Development"),
            Self::Cache => crate::tr!("Кеші", "Caches"),
            Self::Documents => crate::tr!("Документи", "Documents"),
            Self::Other => crate::tr!("Інше", "Other"),
            Self::Trash => crate::tr!("Кошик", "Trash"),
            Self::Custom(_) => crate::tr!("Власна категорія", "Custom category"),
        }
    }
}
pub(crate) fn classify(path: &Path, trash_roots: &[PathBuf]) -> Category {
    if trash_roots.iter().any(|root| path.starts_with(root)) {
        return Category::Trash;
    }
    let parts: Vec<_> = path
        .components()
        .map(|s| s.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    if parts.iter().any(|s| s == ".cache") {
        Category::Cache
    } else if parts
        .iter()
        .any(|s| matches!(s.as_str(), "games" | "steam" | "faugus" | "heroic"))
    {
        Category::Games
    } else if parts.iter().any(|s| {
        matches!(
            s.as_str(),
            "workspace" | "projects" | "node_modules" | ".cargo" | ".rustup"
        )
    }) {
        Category::Development
    } else if parts.iter().any(|s| {
        matches!(
            s.as_str(),
            "documents" | "downloads" | "pictures" | "music" | "videos" | "desktop"
        )
    }) {
        Category::Documents
    } else {
        Category::Other
    }
}
#[derive(Debug, Default)]
pub(crate) struct Index {
    children: Vec<Vec<usize>>,
    roots: HashMap<Category, Vec<usize>>,
    totals: HashMap<Category, u64>,
    cleanup_ids: Vec<usize>,
    classes: Vec<Category>,
    contains_trash: Vec<bool>,
    collections: HashMap<Category, Vec<u64>>,
    collection_roots: HashMap<Category, Vec<usize>>,
    collection_totals: HashMap<Category, u64>,
}
impl Index {
    pub(crate) fn new(r: &Report) -> Self {
        Self::with_trash_roots(r, &recycle::scan_roots())
    }
    pub(crate) fn matches_category(&self, id: usize, category: Category) -> bool {
        if let Some(members) = self.collections.get(&category) {
            return members
                .get(id / 64)
                .is_some_and(|bits| bits & (1 << (id % 64)) != 0);
        }
        category == Category::All
            || (self.classes[id] == category
                && !(category == Category::Other && self.contains_trash[id]))
    }
    pub(crate) fn apply_collections(
        &mut self,
        report: &Report,
        config: &crate::categories::Config,
    ) -> HashMap<Category, Vec<PathBuf>> {
        // Called once per report/config revision, never per row or frame.
        self.collections.clear();
        self.collection_roots.clear();
        self.collection_totals.clear();
        let wanted: HashSet<_> = config
            .items
            .iter()
            .flat_map(|c| c.folders.iter().map(PathBuf::as_path))
            .collect();
        // Index only requested roots. Ordinary built-in categories do no extra
        // scan-wide pass, and custom categories do not clone every report path.
        let paths: HashMap<_, _> = if wanted.is_empty() {
            HashMap::new()
        } else {
            report
                .folders
                .iter()
                .enumerate()
                .filter(|(_, f)| wanted.contains(f.path.as_path()))
                .map(|(i, f)| (f.path.as_path(), i))
                .collect()
        };
        let mut missing = HashMap::new();
        for collection in &config.items {
            if collection.builtin() && collection.folders.is_empty() {
                continue;
            }
            let category = collection.category();
            missing.insert(
                category,
                collection
                    .folders
                    .iter()
                    .filter(|p| !paths.contains_key(p.as_path()))
                    .cloned()
                    .collect(),
            );
            let mut roots: Vec<_> = collection
                .folders
                .iter()
                .filter_map(|p| paths.get(p.as_path()).copied())
                .collect();
            roots.sort_by_key(|i| report.folders[*i].path.components().count());
            let mut unique = Vec::new();
            for id in roots {
                if !unique.iter().any(|i: &usize| {
                    report.folders[id]
                        .path
                        .starts_with(&report.folders[*i].path)
                }) {
                    unique.push(id);
                }
            }
            let mut members = if unique.is_empty() {
                vec![]
            } else {
                vec![0u64; report.folders.len().div_ceil(64)]
            };
            let mut stack = unique.clone();
            while let Some(id) = stack.pop() {
                members[id / 64] |= 1 << (id % 64);
                stack.extend_from_slice(&self.children[id]);
            }
            self.collection_totals.insert(
                category,
                unique.iter().map(|i| report.folders[*i].bytes).sum(),
            );
            self.collection_roots.insert(category, unique);
            self.collections.insert(category, members);
        }
        missing
    }
    pub(crate) fn category_scope_roots(&self, report: &Report, category: Category) -> Vec<usize> {
        report
            .folders
            .iter()
            .enumerate()
            .filter(|(id, f)| {
                self.classes[*id] == category
                    && f.parent
                        .is_none_or(|parent| self.classes[parent] != category)
            })
            .map(|(id, _)| id)
            .collect()
    }
    pub(crate) fn folder_count(&self) -> usize {
        self.children.len()
    }
    pub(crate) fn children(&self, id: usize) -> &[usize] {
        &self.children[id]
    }
    pub(crate) fn roots(&self, category: Category) -> &[usize] {
        self.collection_roots
            .get(&category)
            .or_else(|| self.roots.get(&category))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    pub(crate) fn total(&self, category: Category) -> u64 {
        self.collection_totals
            .get(&category)
            .or_else(|| self.totals.get(&category))
            .copied()
            .unwrap_or(0)
    }
    pub(crate) fn cleanup_ids(&self) -> &[usize] {
        &self.cleanup_ids
    }
    fn with_trash_roots(r: &Report, trash_roots: &[PathBuf]) -> Self {
        let mut s = Self {
            children: vec![vec![]; r.folders.len()],
            ..Default::default()
        };
        let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
        let cleanup_roots = [
            home.join(".cache/yay"),
            home.join(".cache/spotify"),
            home.join(".cache/spotifast"),
            home.join(".cache/google-chrome"),
        ];
        for (i, f) in r.folders.iter().enumerate() {
            if r.complete && f.bytes > 4096 && cleanup_roots.contains(&f.path) {
                s.cleanup_ids.push(i);
            }
            if let Some(p) = f.parent {
                s.children[p].push(i);
            }
        }
        let mut classes = Vec::with_capacity(r.folders.len());
        for f in &r.folders {
            classes.push(if f.parent.is_some_and(|p| classes[p] == Category::Trash) {
                Category::Trash
            } else {
                classify(&f.path, trash_roots)
            });
        }
        let mut contains_trash: Vec<_> = classes.iter().map(|c| *c == Category::Trash).collect();
        let mut uniform: Vec<_> = classes.iter().copied().map(Some).collect();
        for i in (0..r.folders.len()).rev() {
            let child_has_trash = s.children[i].iter().any(|c| contains_trash[*c]);
            contains_trash[i] |= child_has_trash;
            if s.children[i]
                .iter()
                .any(|c| uniform[*c] != Some(classes[i]))
            {
                uniform[i] = None;
            }
        }
        for (i, f) in r.folders.iter().enumerate() {
            let child: u64 = s.children[i].iter().map(|j| r.folders[*j].bytes).sum();
            *s.totals.entry(classes[i]).or_default() += f.bytes.saturating_sub(child);
            if let Some(cat) = uniform[i] {
                if f.parent.map(|p| uniform[p] != Some(cat)).unwrap_or(true) {
                    s.roots.entry(cat).or_default().push(i);
                }
            }
        }
        s.classes = classes;
        s.contains_trash = contains_trash;
        s.roots.insert(
            Category::All,
            if r.folders.is_empty() {
                vec![]
            } else {
                vec![0]
            },
        );
        s
    }
}
#[derive(Debug, Default)]
pub(crate) struct RefreshPaths {
    table: Option<PathBuf>,
    focus: PathBuf,
    expanded: HashSet<PathBuf>,
}
#[derive(Debug)]
pub(crate) struct ResolvedView {
    pub(crate) table: Option<usize>,
    pub(crate) focus: usize,
    pub(crate) expanded: HashSet<usize>,
}
impl RefreshPaths {
    pub(crate) fn capture(
        r: &Report,
        table: Option<usize>,
        focus: usize,
        expanded: &HashSet<usize>,
    ) -> Self {
        Self {
            table: table.and_then(|i| r.folders.get(i)).map(|f| f.path.clone()),
            focus: r
                .folders
                .get(focus)
                .map(|f| f.path.clone())
                .unwrap_or_default(),
            expanded: expanded
                .iter()
                .filter_map(|i| r.folders.get(*i))
                .map(|f| f.path.clone())
                .collect(),
        }
    }

    pub(crate) fn resolve(self, r: &Report) -> ResolvedView {
        let mut v = ResolvedView {
            table: None,
            focus: 0,
            expanded: HashSet::from([0]),
        };
        for (i, f) in r.folders.iter().enumerate() {
            if self.table.as_ref() == Some(&f.path) {
                v.table = Some(i);
            }
            if self.focus == f.path {
                v.focus = i;
            }
            if self.expanded.contains(&f.path) {
                v.expanded.insert(i);
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::Folder;
    #[test]
    fn category_sizes_do_not_double_count() {
        let r = crate::demo::report();
        let index = Index::new(&r);
        let total: u64 = index.totals.values().sum();
        assert_eq!(total, r.folders[0].bytes);
        assert_eq!(
            *index.totals.get(&Category::Cache).unwrap(),
            32 * 1024u64.pow(3)
        );
    }
    #[test]
    fn other_category_excludes_trash_bytes_and_search_results() {
        let home = PathBuf::from("/example/home");
        let trash = home.join(".local/share/Trash");
        let nodes = [
            (home.clone(), None, 1000),
            (home.join(".local"), Some(0), 800),
            (home.join(".local/share"), Some(1), 800),
            (trash.clone(), Some(2), 600),
            (trash.join("files"), Some(3), 550),
            (trash.join("files/Games"), Some(4), 400),
            (trash.join("info"), Some(3), 50),
            (home.join(".local/share/tool"), Some(2), 200),
            (home.join("notes"), Some(0), 200),
        ];
        let report = Report {
            folders: nodes
                .into_iter()
                .map(|(path, parent, bytes)| Folder {
                    path,
                    parent,
                    bytes,
                    files: 0,
                })
                .collect(),
            complete: true,
            ..Default::default()
        };
        let index = Index::with_trash_roots(&report, &[trash.clone()]);
        assert_eq!(index.totals[&Category::Other], 400);
        assert_eq!(index.totals[&Category::Trash], 600);
        assert_eq!(index.totals.values().sum::<u64>(), report.folders[0].bytes);
        assert_eq!(index.classes[5], Category::Trash); // Ignore names inside Trash.
        assert_eq!(index.roots[&Category::Other], vec![7, 8]);
        for id in [0, 1, 2, 3, 4, 5, 6] {
            assert!(!index.matches_category(id, Category::Other));
        }
        for id in [7, 8] {
            assert!(index.matches_category(id, Category::Other));
        }
        assert!(index.matches_category(3, Category::All)); // The actual file tree is intact.
        assert_eq!(report.folders[0].bytes, 1000);
    }
    #[test]
    fn trash_detection_respects_configured_locations_and_path_boundaries() {
        let roots = [
            PathBuf::from("/data/custom/Trash"),
            PathBuf::from("/volume/.Trash-1000"),
        ];
        for path in [
            "/data/custom/Trash",
            "/data/custom/Trash/files/Documents",
            "/volume/.Trash-1000/files/Games",
        ] {
            assert_eq!(classify(Path::new(path), &roots), Category::Trash);
        }
        assert_eq!(
            classify(Path::new("/data/custom/Trash-old"), &roots),
            Category::Other
        );
        assert_eq!(
            classify(Path::new("/home/me/personal/Trash"), &roots),
            Category::Other
        );
        assert_eq!(
            classify(Path::new("/home/me/Documents/Trash"), &roots),
            Category::Documents
        );
    }
}

#[cfg(test)]
mod collection_tests {
    use super::*;
    #[test]
    fn collections_are_overlays_and_nested_roots_are_counted_once() {
        let report = crate::demo::report();
        let cache = report.folders[3].path.clone();
        let nested = report.folders[6].path.clone();
        let mut config = crate::categories::Config::default();
        let mut item = crate::categories::Collection {
            auto_refresh: false,
            auto_save: false,
            read_only: false,
            id: "my-cache".into(),
            name: "My cache".into(),
            icon: "folder".into(),
            folders: vec![nested, cache.clone(), cache, "/not-scanned".into()],
        };
        let category = item.category();
        config.items.push(item.clone());
        let mut index = Index::new(&report);
        let baseline = index.total(Category::Cache);
        index.apply_collections(&report, &config);
        assert_eq!(index.roots(category), &[3]);
        assert_eq!(index.total(category), report.folders[3].bytes);
        assert_eq!(index.total(Category::Cache), baseline);
        assert!(index.matches_category(6, category));
        assert!(!index.matches_category(1, category));
        // Unrelated or missing roots cannot leak other files into a collection.
        item.folders = vec!["/not-scanned".into()];
        config.items[5] = item;
        index.apply_collections(&report, &config);
        assert!(index.roots(category).is_empty());
        assert_eq!(index.total(category), 0);
        assert!(!index.matches_category(6, category));
        config.items[2].folders = vec![report.folders[6].path.clone()];
        index.apply_collections(&report, &config);
        assert_eq!(index.total(Category::Cache), report.folders[6].bytes);
        config.items[2].folders.clear();
        index.apply_collections(&report, &config);
        assert_eq!(index.total(Category::Cache), baseline);
        assert_eq!(index.roots(Category::Cache), &[3]);
    }
}
