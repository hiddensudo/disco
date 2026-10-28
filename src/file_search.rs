//! Cancellable filename search. Includes files that have never been expanded.
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub(crate) struct Hit {
    pub path: PathBuf,
    pub directory: bool,
    pub bytes: u64,
    pub folder_id: Option<usize>,
}
#[derive(Clone, PartialEq)]
pub(crate) struct Spec {
    pub query: String,
    pub roots: Arc<Vec<PathBuf>>,
    pub home: Option<PathBuf>,
    pub category: Option<crate::index::Category>,
    pub report_revision: u64,
}
#[derive(Default)]
pub(crate) struct Search {
    spec: Option<Spec>,
    wait: Option<Instant>,
    job: Option<Receiver<(Spec, ResultData)>>,
    cancel: Arc<AtomicBool>,
    pub result: Option<ResultData>,
    pub revision: u64,
}
pub(crate) struct ResultData {
    pub hits: Vec<Hit>,
    pub home: bool,
    pub truncated: bool,
    pub skipped: usize,
}
#[cfg(test)]
fn collect(roots: &[PathBuf], query: &str, cancel: &AtomicBool) -> ResultData {
    collect_filtered(roots, query, cancel, None)
}
fn collect_filtered(
    roots: &[PathBuf],
    query: &str,
    cancel: &AtomicBool,
    category: Option<crate::index::Category>,
) -> ResultData {
    let trash_roots = if category.is_some() {
        crate::recycle::scan_roots()
    } else {
        vec![]
    };
    let mut result = ResultData {
        hits: vec![],
        home: false,
        truncated: false,
        skipped: 0,
    };
    let mut seen = std::collections::HashSet::new();
    for root in roots {
        for entry in walkdir::WalkDir::new(root)
            .follow_links(false)
            .same_file_system(true)
            .into_iter()
            .filter_entry(|entry| {
                !entry.file_type().is_dir()
                    || category.is_none_or(|category| {
                        crate::index::classify(entry.path(), &trash_roots) == category
                    })
            })
        {
            if cancel.load(Ordering::Relaxed) {
                return result;
            }
            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    result.skipped += 1;
                    continue;
                }
            };
            if entry.file_type().is_symlink()
                || !(entry.file_type().is_file() || entry.file_type().is_dir())
            {
                continue;
            }
            if entry
                .file_name()
                .to_string_lossy()
                .to_lowercase()
                .contains(query)
                && seen.insert(entry.path().to_path_buf())
            {
                let bytes = entry
                    .metadata()
                    .map(|m| crate::scan::allocated(&m))
                    .unwrap_or(0);
                let directory = entry.file_type().is_dir();
                result.hits.push(Hit {
                    path: entry.into_path(),
                    directory,
                    bytes,
                    folder_id: None,
                });
                if result.hits.len() >= 2000 {
                    result.truncated = true;
                    return result;
                }
            }
        }
    }
    result
}
impl Search {
    pub fn configure(
        &mut self,
        spec: Spec,
        report: Option<Arc<crate::scan::Report>>,
        ctx: &eframe::egui::Context,
    ) {
        if self.spec.as_ref() != Some(&spec) {
            self.cancel.store(true, Ordering::Relaxed);
            self.result = None;
            self.revision = self.revision.wrapping_add(1);
            self.wait = (!spec.query.is_empty()).then(Instant::now);
            self.spec = Some(spec);
        }
        if let Some(job) = &self.job {
            match job.try_recv() {
                Ok((spec, result)) => {
                    self.job = None;
                    if self.spec.as_ref() == Some(&spec) && !self.cancel.load(Ordering::Relaxed) {
                        self.result = Some(result);
                        self.revision = self.revision.wrapping_add(1);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => self.job = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.job.is_none()
            && self
                .wait
                .is_some_and(|at| at.elapsed() >= Duration::from_millis(180))
        {
            self.wait = None;
            let spec = self.spec.clone().unwrap();
            let cancel = Arc::new(AtomicBool::new(false));
            self.cancel = cancel.clone();
            let (tx, rx) = mpsc::channel();
            self.job = Some(rx);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                crate::scan::background_priority();
                let q = spec.query.to_lowercase();
                let mut result = collect_filtered(&spec.roots, &q, &cancel, spec.category);
                if result.hits.is_empty() && !cancel.load(Ordering::Relaxed) {
                    if let Some(home) = &spec.home {
                        result = collect_filtered(std::slice::from_ref(home), &q, &cancel, None);
                        result.home = true;
                    }
                }
                if let Some(report) = report {
                    if result.hits.iter().any(|hit| hit.directory) {
                        let ids: std::collections::HashMap<_, _> = report
                            .folders
                            .iter()
                            .enumerate()
                            .map(|(id, f)| (f.path.as_path(), id))
                            .collect();
                        for hit in &mut result.hits {
                            if hit.directory {
                                if let Some(id) = ids.get(hit.path.as_path()) {
                                    hit.folder_id = Some(*id);
                                    hit.bytes = report.folders[*id].bytes;
                                }
                            }
                        }
                    }
                }
                let _ = tx.send((spec, result));
                ctx.request_repaint();
            });
        }
        if self.wait.is_some() || self.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    pub fn invalidate(&mut self) {
        self.spec = None;
    }
    pub fn for_query(&self, query: &str) -> Option<&ResultData> {
        self.spec
            .as_ref()
            .filter(|s| s.query == query)
            .and(self.result.as_ref())
    }
    pub fn busy(&self) -> bool {
        self.wait.is_some() || self.job.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_categories_include_direct_home_files_and_prune_other_categories() {
        let root =
            std::env::temp_dir().join(format!("disco-search-category-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Games")).unwrap();
        std::fs::create_dir_all(root.join(".cache")).unwrap();
        for path in ["needle.txt", "Games/needle.rs", ".cache/needle.json"] {
            std::fs::write(root.join(path), "test").unwrap();
        }
        let result = collect_filtered(
            &[root.clone()],
            "needle",
            &AtomicBool::new(false),
            Some(crate::index::Category::Other),
        );
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].path, root.join("needle.txt"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn search_finds_unexpanded_files_and_respects_roots_and_cancellation() {
        let root = std::env::temp_dir().join(format!("disco-search-{}", std::process::id()));
        std::fs::create_dir_all(root.join("scope/deep")).unwrap();
        std::fs::create_dir_all(root.join("home")).unwrap();
        std::fs::write(root.join("scope/deep/needle.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("home/needle.txt"), "outside").unwrap();
        let result = collect(&[root.join("scope")], "needle", &AtomicBool::new(false));
        assert_eq!(result.hits.len(), 1);
        assert!(!result.hits[0].directory);
        assert!(result.hits[0].path.ends_with("needle.rs"));
        assert!(
            collect(&[root.clone()], "needle", &AtomicBool::new(true))
                .hits
                .is_empty()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn home_fallback_is_optional_and_only_used_after_an_empty_scope() {
        let root = std::env::temp_dir().join(format!("disco-search-home-{}", std::process::id()));
        std::fs::create_dir_all(root.join("scope")).unwrap();
        std::fs::create_dir_all(root.join("home")).unwrap();
        std::fs::write(root.join("home/unique.json"), "{}").unwrap();
        let ctx = eframe::egui::Context::default();
        let mut search = Search::default();
        for fallback in [false, true] {
            let spec = Spec {
                query: "unique".into(),
                roots: Arc::new(vec![root.join("scope")]),
                home: fallback.then(|| root.join("home")),
                category: None,
                report_revision: 0,
            };
            let start = Instant::now();
            while search.for_query("unique").is_none() && start.elapsed() < Duration::from_secs(3) {
                search.configure(spec.clone(), None, &ctx);
                std::thread::sleep(Duration::from_millis(10));
            }
            let result = search.for_query("unique").expect("finished");
            assert_eq!(result.home, fallback);
            assert_eq!(result.hits.len(), usize::from(fallback));
            search.invalidate();
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}

impl Drop for Search {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
