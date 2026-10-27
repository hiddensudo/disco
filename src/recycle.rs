//! Freedesktop Trash, using IDs supplied by the system trash API. Permanent
//! actions receive an explicit captured set, never a late "delete everything".
use eframe::egui;
use std::{
    collections::HashSet,
    ffi::OsString,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub struct Item {
    pub entry: trash::TrashItem,
    pub stored: PathBuf,
    pub bytes: Option<u64>,
    pub complete_size: bool,
    pub directory: bool,
    info_identity: (u64, u64, i64, i64, u64),
    identity: (u64, u64),
}
impl Item {
    fn capture(entry: trash::TrashItem) -> Result<Self, String> {
        let info = Path::new(&entry.id);
        if !info.is_absolute()
            || info.extension().is_none_or(|e| e != "trashinfo")
            || info
                .parent()
                .and_then(Path::file_name)
                .is_none_or(|n| n != "info")
        {
            return Err("Неправильний ідентифікатор елемента кошика".into());
        }
        let original = entry.original_path();
        if !original.is_absolute()
            || original
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err("Неправильний початковий шлях у кошику".into());
        }
        let meta = fs::symlink_metadata(info).map_err(|e| e.to_string())?;
        if !meta.is_file() || info.canonicalize().map_err(|e| e.to_string())? != info {
            return Err("Метадані кошика мають бути звичайним файлом".into());
        }
        let stem = info.file_stem().ok_or("Немає назви елемента")?;
        if Path::new(stem).components().count() != 1
            || !matches!(
                Path::new(stem).components().next(),
                Some(std::path::Component::Normal(_))
            )
        {
            return Err("Неправильна назва елемента кошика".into());
        }
        let stored = info
            .parent()
            .and_then(Path::parent)
            .ok_or("Немає папки кошика")?
            .join("files")
            .join(stem);
        let parent = stored.parent().ok_or("Немає папки files")?;
        if parent.canonicalize().map_err(|e| e.to_string())? != parent {
            return Err("Папка files у кошику не може бути посиланням".into());
        }
        let m = fs::symlink_metadata(&stored).map_err(|e| e.to_string())?;
        Ok(Self {
            entry,
            stored,
            bytes: (!m.is_dir()).then_some(m.blocks().saturating_mul(512)),
            complete_size: !m.is_dir(),
            directory: m.is_dir(),
            info_identity: (
                meta.dev(),
                meta.ino(),
                meta.mtime(),
                meta.mtime_nsec(),
                meta.len(),
            ),
            identity: (m.dev(), m.ino()),
        })
    }
    fn matches(&self, other: &Self) -> bool {
        self.entry.id == other.entry.id
            && self.entry.name == other.entry.name
            && self.entry.original_parent == other.entry.original_parent
            && self.entry.time_deleted == other.entry.time_deleted
            && self.info_identity == other.info_identity
            && self.identity == other.identity
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Restore,
    Purge,
}
#[derive(Debug)]
pub struct Finished {
    pub action: Action,
    pub done: usize,
    pub error: Option<String>,
    pub changed: bool,
}
fn list() -> Result<Vec<trash::TrashItem>, String> {
    std::panic::catch_unwind(trash::os_limited::list)
        .map_err(|_| "Не вдалося прочитати метадані кошика".to_string())?
        .map_err(|e| e.to_string())
}
fn execute(items: &[Item], action: Action) -> Finished {
    let mut result = Finished {
        action,
        done: 0,
        error: None,
        changed: false,
    };
    let current = match list() {
        Ok(v) => v,
        Err(e) => {
            result.error = Some(e.to_string());
            return result;
        }
    };
    // Revalidate the whole confirmed set before the first mutation.
    for item in items {
        let fresh = current
            .iter()
            .find(|e| e.id == item.entry.id)
            .cloned()
            .and_then(|e| Item::capture(e).ok());
        if fresh.as_ref().is_none_or(|f| !item.matches(f)) {
            result.error = Some(format!(
                "Елемент кошика змінився: {}. Онови список.",
                item.entry.name.to_string_lossy()
            ));
            return result;
        }
        if action == Action::Restore && fs::symlink_metadata(item.entry.original_path()).is_ok() {
            result.error = Some(format!(
                "Уже існує: {}. Відновлення не перезаписує наявні файли.",
                item.entry.original_path().display()
            ));
            return result;
        }
    }
    for item in items {
        if Item::capture(item.entry.clone())
            .map(|fresh| !item.matches(&fresh))
            .unwrap_or(true)
        {
            result.error = Some("Елемент кошика змінився під час операції".into());
            break;
        }
        result.changed = true;
        let op = std::panic::catch_unwind(|| match action {
            Action::Restore => trash::os_limited::restore_all([item.entry.clone()]),
            Action::Purge => trash::os_limited::purge_all([&item.entry]),
        })
        .map_err(|_| "Операцію кошика перервано. Перевір оновлений список.".to_string())
        .and_then(|result| result.map_err(|e| e.to_string()));
        match op {
            Ok(()) => result.done += 1,
            Err(e) => {
                result.error = Some(e.to_string());
                break;
            }
        }
    }
    result
}
fn size(path: &Path, cancel: &AtomicBool) -> Option<(u64, bool)> {
    let mut bytes = 0u64;
    let mut complete = true;
    let mut links = HashSet::new();
    let mut yielded = Instant::now();
    for e in walkdir::WalkDir::new(path)
        .follow_links(false)
        .same_file_system(true)
    {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let meta = match e.and_then(|e| e.metadata()) {
            Ok(m) => m,
            Err(_) => {
                complete = false;
                continue;
            }
        };
        if meta.is_file() && meta.nlink() > 1 && !links.insert((meta.dev(), meta.ino())) {
            continue;
        }
        bytes = bytes.saturating_add(meta.blocks().saturating_mul(512));
        if yielded.elapsed() > Duration::from_millis(50) {
            std::thread::sleep(Duration::from_millis(1));
            yielded = Instant::now();
        }
    }
    Some((bytes, complete))
}
enum Event {
    Loaded(Vec<Item>, Option<String>),
    Size(OsString, u64, bool),
    Ready,
    Error(String),
    Finished(Finished),
}
#[derive(Default)]
pub struct Bin {
    pub items: Vec<Item>,
    pub selected: HashSet<OsString>,
    pub loading: bool,
    pub loaded: bool,
    pub busy: bool,
    pub notice: String,
    pub error: String,
    rx: Option<Receiver<Event>>,
    cancel: Arc<AtomicBool>,
}
impl Bin {
    pub fn demo() -> Self {
        let mut s = Self::default();
        s.loaded = true;
        for (name, original, bytes, directory) in [
            (
                "Modlist_Downloads",
                "/home/alex/Games",
                2197u64 * 1024u64.pow(3) / 10,
                true,
            ),
            ("old-build", "/home/alex/workspace", 12884901888, true),
            ("screenshot.png", "/home/alex/Pictures", 1835008, false),
        ] {
            let entry = trash::TrashItem {
                id: OsString::from(format!("/demo/trash/info/{name}.trashinfo")),
                name: name.into(),
                original_parent: original.into(),
                time_deleted: 1793111115,
            };
            s.items.push(Item {
                entry,
                stored: PathBuf::from(format!("/demo/trash/files/{name}")),
                bytes: Some(bytes),
                complete_size: true,
                directory,
                info_identity: (0, 0, 0, 0, 0),
                identity: (0, 0),
            });
        }
        s
    }
    pub fn invalidate(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.loaded = false;
        self.loading = false;
        self.rx = None;
    }
    pub fn picked(&self) -> Vec<Item> {
        self.items
            .iter()
            .filter(|i| self.selected.contains(&i.entry.id))
            .cloned()
            .collect()
    }
    pub fn refresh(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.loading = true;
        self.error.clear();
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        std::thread::spawn(move || {
            crate::scan::background_priority();
            let entries = match list() {
                Ok(v) => v,
                Err(e) => {
                    let _ = tx.send(Event::Error(e.to_string()));
                    ctx.request_repaint();
                    return;
                }
            };
            let mut items = Vec::new();
            let mut skipped = 0;
            for entry in entries {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                match Item::capture(entry) {
                    Ok(i) => items.push(i),
                    Err(_) => skipped += 1,
                }
            }
            items.sort_by(|a, b| {
                b.entry
                    .time_deleted
                    .cmp(&a.entry.time_deleted)
                    .then_with(|| a.entry.name.cmp(&b.entry.name))
            });
            let directories: Vec<_> = items
                .iter()
                .filter(|i| i.directory)
                .map(|i| (i.entry.id.clone(), i.stored.clone()))
                .collect();
            let warning =
                (skipped > 0).then(|| format!("Не вдалося прочитати {skipped} елементів кошика."));
            if tx.send(Event::Loaded(items, warning)).is_err() {
                return;
            }
            ctx.request_repaint();
            for (id, path) in directories {
                let Some((bytes, complete)) = size(&path, &cancel) else {
                    return;
                };
                if tx.send(Event::Size(id, bytes, complete)).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
            let _ = tx.send(Event::Ready);
            ctx.request_repaint();
        });
    }
    pub fn start(&mut self, items: Vec<Item>, action: Action, ctx: &egui::Context) {
        if self.busy || items.is_empty() {
            return;
        }
        self.cancel.store(true, Ordering::Relaxed);
        self.loading = false;
        self.busy = true;
        self.error.clear();
        self.notice = match action {
            Action::Restore => "Відновлюємо вибране…",
            Action::Purge => "Видаляємо остаточно…",
        }
        .into();
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            crate::scan::background_priority();
            let _ = tx.send(Event::Finished(execute(&items, action)));
            ctx.request_repaint();
        });
    }
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<Finished> {
        let events: Vec<_> = self
            .rx
            .as_ref()
            .map(|r| r.try_iter().collect())
            .unwrap_or_default();
        let mut finished = None;
        for event in events {
            match event {
                Event::Loaded(items, warning) => {
                    self.items = items;
                    self.loaded = true;
                    self.selected
                        .retain(|id| self.items.iter().any(|i| &i.entry.id == id));
                    if let Some(w) = warning {
                        if !self.error.is_empty() {
                            self.error.push_str(" · ");
                        }
                        self.error.push_str(&w);
                    };
                }
                Event::Size(id, bytes, complete) => {
                    if let Some(i) = self.items.iter_mut().find(|i| i.entry.id == id) {
                        i.bytes = Some(bytes);
                        i.complete_size = complete;
                    }
                }
                Event::Ready => {
                    self.loading = false;
                    self.rx = None;
                }
                Event::Error(e) => {
                    self.loaded = true;
                    self.loading = false;
                    self.rx = None;
                    self.error = e;
                }
                Event::Finished(result) => {
                    self.busy = false;
                    self.selected.clear();
                    self.notice = format!(
                        "{}: {}",
                        if result.action == Action::Restore {
                            "Відновлено"
                        } else {
                            "Видалено остаточно"
                        },
                        result.done
                    );
                    let error = result.error.clone();
                    self.refresh(ctx);
                    if let Some(e) = error {
                        self.error = e;
                    }
                    finished = Some(result);
                }
            }
        }
        finished
    }
    pub fn total(&self) -> (u64, bool) {
        self.items.iter().fold((0, true), |(n, complete), i| {
            (
                n.saturating_add(i.bytes.unwrap_or(0)),
                complete && i.bytes.is_some() && i.complete_size,
            )
        })
    }
}
impl Drop for Bin {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("disco-recycle-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }
    #[test]
    fn folder_sizes_count_allocated_blocks_once_and_do_not_follow_links() {
        let root = fixture("size");
        let folder = root.join("folder");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("file"), vec![1u8; 8192]).unwrap();
        fs::hard_link(folder.join("file"), folder.join("hardlink")).unwrap();
        fs::write(root.join("outside"), vec![1u8; 65536]).unwrap();
        std::os::unix::fs::symlink(root.join("outside"), folder.join("link")).unwrap();
        let expected = [folder.clone(), folder.join("file"), folder.join("link")]
            .iter()
            .map(|p| fs::symlink_metadata(p).unwrap().blocks() * 512)
            .sum::<u64>();
        assert_eq!(
            size(&folder, &AtomicBool::new(false)),
            Some((expected, true))
        );
        assert_eq!(size(&folder, &AtomicBool::new(true)), None);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn refuses_escaping_ids_and_symlinked_trash_files_folder() {
        let root = fixture("guard");
        fs::create_dir_all(root.join("trash/info")).unwrap();
        fs::create_dir_all(root.join("outside")).unwrap();
        let info = root.join("trash/info/item.trashinfo");
        fs::write(&info, b"metadata").unwrap();
        fs::write(root.join("outside/item"), b"do not touch").unwrap();
        std::os::unix::fs::symlink(root.join("outside"), root.join("trash/files")).unwrap();
        let mut entry = trash::TrashItem {
            id: info.into_os_string(),
            name: "item".into(),
            original_parent: root.clone(),
            time_deleted: 0,
        };
        assert!(Item::capture(entry.clone()).is_err());
        entry.id = root.join("trash/info/...trashinfo").into_os_string();
        fs::write(Path::new(&entry.id), b"metadata").unwrap();
        assert!(Item::capture(entry).is_err());
        assert!(root.join("outside/item").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn isolated_bin_restore_collision_and_confirmed_purge() {
        if let Some(root) = std::env::var_os("DISCO_RECYCLE_TEST_FIXTURE") {
            let root = PathBuf::from(root);
            assert!(root.starts_with(std::env::temp_dir()));
            assert!(
                root.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("disco-recycle-integration-")
            );
            let owned = || {
                trash::os_limited::list()
                    .unwrap()
                    .into_iter()
                    .filter(|e| Path::new(&e.id).starts_with(root.join("data/Trash/info")))
                    .map(|e| Item::capture(e).unwrap())
                    .collect::<Vec<_>>()
            };
            for name in ["restore", "collision", "purge"] {
                trash::delete(root.join(name)).unwrap();
            }
            let initial = owned();
            assert_eq!(initial.len(), 3);
            let restore = initial.iter().find(|i| i.entry.name == "restore").unwrap();
            let out = execute(std::slice::from_ref(restore), Action::Restore);
            assert_eq!(out.done, 1);
            assert!(out.error.is_none());
            assert_eq!(
                fs::read(root.join("restore/payload")).unwrap(),
                b"restore me"
            );
            let collision = initial
                .iter()
                .find(|i| i.entry.name == "collision")
                .unwrap();
            fs::write(root.join("collision"), b"keep existing").unwrap();
            let out = execute(std::slice::from_ref(collision), Action::Restore);
            assert_eq!(out.done, 0);
            assert!(!out.changed);
            assert!(out.error.is_some());
            assert_eq!(fs::read(root.join("collision")).unwrap(), b"keep existing");
            assert!(collision.stored.exists());
            // An item arriving after confirmation must not be purged.
            fs::write(root.join("late"), b"new arrival").unwrap();
            trash::delete(root.join("late")).unwrap();
            let purge = initial.iter().find(|i| i.entry.name == "purge").unwrap();
            let out = execute(std::slice::from_ref(purge), Action::Purge);
            assert_eq!(out.done, 1);
            assert!(out.error.is_none());
            assert!(!purge.stored.exists());
            assert!(!Path::new(&purge.entry.id).exists());
            let current = owned();
            assert!(current.iter().any(|i| i.entry.name == "late"));
            // A changed metadata record invalidates a previously captured target.
            let late = current.iter().find(|i| i.entry.name == "late").unwrap();
            use std::io::Write;
            let mut f = fs::OpenOptions::new()
                .append(true)
                .open(Path::new(&late.entry.id))
                .unwrap();
            f.write_all(b"\n").unwrap();
            drop(f);
            let out = execute(std::slice::from_ref(late), Action::Purge);
            assert_eq!(out.done, 0);
            assert!(out.error.is_some());
            assert!(late.stored.exists());
            let ctx = egui::Context::default();
            let mut bin = Bin::default();
            bin.refresh(&ctx);
            let deadline = Instant::now() + Duration::from_secs(3);
            while bin.loading && Instant::now() < deadline {
                bin.poll(&ctx);
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(!bin.loading);
            assert!(bin.loaded);
            assert!(bin.items.iter().any(|i| i.entry.name == "late"));
            return;
        }
        let root = fixture("integration");
        fs::create_dir(root.join("restore")).unwrap();
        fs::write(root.join("restore/payload"), b"restore me").unwrap();
        fs::write(root.join("collision"), b"trashed copy").unwrap();
        fs::create_dir(root.join("purge")).unwrap();
        fs::write(root.join("purge/payload"), b"purge fixture").unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "recycle::tests::isolated_bin_restore_collision_and_confirmed_purge",
                "--nocapture",
            ])
            .env("DISCO_RECYCLE_TEST_FIXTURE", &root)
            .env("XDG_DATA_HOME", root.join("data"))
            .status()
            .unwrap();
        assert!(status.success());
        fs::remove_dir_all(root).unwrap();
    }
}
