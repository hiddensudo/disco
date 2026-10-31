//! Paths are data. Completion and resolution run on bounded background workers.
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

pub fn expand(text: &str, base: &Path) -> Result<PathBuf, String> {
    if text.is_empty() {
        return Err(crate::tr!("Введи шлях", "Enter a path").into());
    }
    let path = if text == "~" || text.starts_with("~/") {
        let home = std::env::var_os("HOME").ok_or_else(|| {
            crate::tr!("Домашній шлях недоступний", "Home directory is unavailable").to_string()
        })?;
        PathBuf::from(home).join(text.strip_prefix("~/").unwrap_or(""))
    } else {
        PathBuf::from(text)
    };
    Ok(if path.is_absolute() {
        path
    } else {
        base.join(path)
    })
}
pub fn startup(args: &[String]) -> Result<Option<PathBuf>, String> {
    let mut path = None;
    let mut i = 1;
    let mut literal = false;
    while i < args.len() {
        let arg = &args[i];
        if !literal && arg == "--" {
            literal = true;
            i += 1;
            continue;
        }
        if !literal
            && [
                "--folder",
                "--preview-file",
                "--screenshot",
                "--settings",
                "--frame-probe",
                "--frame-probe-x",
            ]
            .contains(&arg.as_str())
        {
            i += 2;
            continue;
        }
        if !literal && arg.starts_with('-') {
            i += 1;
            continue;
        }
        if path.is_some() {
            return Err(crate::tr!(
                "Передай один файл або одну папку",
                "Pass one file or one folder"
            )
            .into());
        }
        path = Some(expand(
            arg,
            &std::env::current_dir().map_err(|e| e.to_string())?,
        )?);
        i += 1;
    }
    Ok(path)
}
#[derive(Clone, Debug)]
pub struct Resolved {
    pub path: PathBuf,
    pub directory: bool,
    pub bytes: u64,
}
pub fn resolve(path: &Path) -> Result<Resolved, String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if !meta.is_file() && !meta.is_dir() {
        return Err(crate::tr!(
            "Потрібен звичайний файл або папка",
            "Choose a regular file or directory"
        )
        .into());
    }
    Ok(Resolved {
        path,
        directory: meta.is_dir(),
        bytes: crate::scan::allocated(&meta),
    })
}
#[derive(Clone, Debug)]
pub struct Suggestion {
    pub path: PathBuf,
    pub directory: bool,
}
fn complete(path: &Path, cancel: &AtomicBool) -> Result<Vec<Suggestion>, String> {
    let (directory, prefix) = if path.is_dir() {
        (path.to_path_buf(), String::new())
    } else {
        (
            path.parent().unwrap_or(Path::new("/")).into(),
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase(),
        )
    };
    let entries = std::fs::read_dir(directory).map_err(|e| e.to_string())?;
    let mut hits = Vec::new();
    for entry in entries.take(5000) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(entry) = entry else {
            continue;
        };
        if !entry
            .file_name()
            .to_string_lossy()
            .to_lowercase()
            .starts_with(&prefix)
        {
            continue;
        }
        let Ok(meta) = std::fs::metadata(entry.path()) else {
            continue;
        };
        if meta.is_file() || meta.is_dir() {
            hits.push(Suggestion {
                path: entry.path(),
                directory: meta.is_dir(),
            });
        }
    }
    hits.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.path.cmp(&b.path))
    });
    hits.truncate(50);
    Ok(hits)
}
#[derive(Default)]
pub struct Completion {
    query: Option<PathBuf>,
    generation: u64,
    waiting: Option<Instant>,
    job: Option<Receiver<(u64, Result<Vec<Suggestion>, String>)>>,
    cancel: Arc<AtomicBool>,
    pub hits: Vec<Suggestion>,
    pub error: String,
}
impl Completion {
    pub fn configure(&mut self, path: PathBuf) {
        if self.query.as_ref() == Some(&path) {
            return;
        }
        self.query = Some(path);
        self.generation += 1;
        self.cancel.store(true, Ordering::Relaxed);
        self.waiting = Some(Instant::now());
        self.hits.clear();
        self.error.clear();
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok((generation, result)) => {
                    self.job = None;
                    if generation == self.generation {
                        match result {
                            Ok(hits) => self.hits = hits,
                            Err(e) => self.error = e,
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => self.job = None,
                _ => {}
            }
        }
        if self.job.is_none()
            && self
                .waiting
                .is_some_and(|at| at.elapsed() >= Duration::from_millis(120))
        {
            self.waiting = None;
            let path = self.query.clone().unwrap();
            let generation = self.generation;
            let cancel = Arc::new(AtomicBool::new(false));
            self.cancel = cancel.clone();
            let (tx, rx) = mpsc::channel();
            self.job = Some(rx);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                crate::scan::background_priority();
                let result = complete(&path, &cancel);
                let _ = tx.send((generation, result));
                ctx.request_repaint();
            });
        }
        if self.waiting.is_some() || self.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }
}
#[derive(Default)]
pub struct Resolver {
    desired: Option<PathBuf>,
    job: Option<Receiver<(PathBuf, Result<Resolved, String>)>>,
}
impl Resolver {
    pub fn request(&mut self, path: PathBuf) {
        self.desired = Some(path);
    }
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<Result<Resolved, String>> {
        let mut finished = None;
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok((path, result)) => {
                    self.job = None;
                    if self.desired.as_ref() == Some(&path) {
                        self.desired = None;
                        finished = Some(result);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.job = None;
                    self.desired = None;
                    finished = Some(Err(crate::tr!(
                        "Не вдалося відкрити шлях",
                        "Could not open path"
                    )
                    .into()));
                }
                _ => {}
            }
        }
        if self.job.is_none()
            && let Some(path) = self.desired.clone()
        {
            let (tx, rx) = mpsc::channel();
            self.job = Some(rx);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                crate::scan::background_priority();
                let result = resolve(&path);
                let _ = tx.send((path, result));
                ctx.request_repaint();
            });
        }
        if self.desired.is_some() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
        finished
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cli_paths_completion_unicode_and_symlinks() {
        let root = std::env::temp_dir().join(format!("disco-paths-{}", std::process::id()));
        std::fs::create_dir_all(root.join("A folder")).unwrap();
        std::fs::write(root.join("notes.txt"), "hello").unwrap();
        std::fs::write(root.join(".hidden"), "hello").unwrap();
        let hits = complete(&root, &AtomicBool::new(false)).unwrap();
        assert!(hits[0].directory);
        assert_eq!(hits.len(), 3);
        let path = expand("A folder", &root).unwrap();
        assert!(resolve(&path).unwrap().directory);
        std::os::unix::fs::symlink(root.join("notes.txt"), root.join("link.txt")).unwrap();
        assert_eq!(
            resolve(&root.join("link.txt")).unwrap().path,
            root.join("notes.txt")
        );
        assert!(resolve(&root.join("missing")).is_err());
        assert_eq!(
            startup(&[
                "disco".into(),
                "--folder".into(),
                "/tmp".into(),
                "--".into(),
                "hello world.txt".into()
            ])
            .unwrap(),
            Some(std::env::current_dir().unwrap().join("hello world.txt"))
        );
        assert!(startup(&["disco".into(), "one".into(), "two".into()]).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
