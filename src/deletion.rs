//! Capture the exact file or directory the user is about to confirm, then revalidate it
//! immediately before handing it to the desktop trash implementation.
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug)]
pub struct Target {
    pub path: PathBuf,
    pub bytes: u64,
    device: u64,
    inode: u64,
    pub is_dir: bool,
}
impl Target {
    pub fn capture(path: &Path, bytes: u64) -> Result<Self, String> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        Self::capture_with_home(path, bytes, home.as_deref())
    }
    fn capture_with_home(path: &Path, bytes: u64, home: Option<&Path>) -> Result<Self, String> {
        if !path.is_absolute()
            || path.parent().is_none()
            || home.is_some_and(|home| home.starts_with(path))
            || [
                "/bin", "/boot", "/dev", "/etc", "/home", "/lib", "/lib64", "/proc", "/root",
                "/run", "/sbin", "/sys", "/usr", "/var", "/mnt", "/media",
            ]
            .iter()
            .any(|p| path == Path::new(p))
        {
            return Err(crate::tr!(
                "Цю системну або домашню папку не можна видалити з disco.",
                "This system or home folder cannot be deleted from disco."
            )
            .into());
        }
        let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !(meta.is_dir() || meta.is_file())
            || meta.file_type().is_symlink()
            || path.canonicalize().map_err(|e| e.to_string())? != path
        {
            return Err(crate::tr!(
                "Шлях має вказувати безпосередньо на файл або папку, без символічних посилань.",
                "The path must point directly to a file or folder, without symbolic links."
            )
            .into());
        }
        if path
            .parent()
            .and_then(|p| fs::metadata(p).ok())
            .is_none_or(|p| p.dev() != meta.dev())
        {
            return Err(crate::tr!(
                "Корінь підключеного диска не можна перемістити у кошик.",
                "The root of a mounted drive cannot be moved to Trash."
            )
            .into());
        }
        Ok(Self {
            path: path.to_owned(),
            bytes,
            device: meta.dev(),
            inode: meta.ino(),
            is_dir: meta.is_dir(),
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        let fresh = Self::capture(&self.path, self.bytes)?;
        if fresh.device != self.device || fresh.inode != self.inode || fresh.is_dir != self.is_dir {
            return Err(crate::tr!(
                "Файл або папка змінилися після відкриття підтвердження: {}",
                "File or folder changed after opening confirmation: {}",
                self.path.display()
            ));
        }
        Ok(())
    }
}
#[derive(Default, Debug)]
pub struct Outcome {
    pub done: Vec<PathBuf>,
    pub error: Option<String>,
}
pub fn move_to_trash(targets: Vec<Target>) -> Outcome {
    execute(targets, |p| trash::delete(p).map_err(|e| e.to_string()))
}
fn execute(targets: Vec<Target>, mut remove: impl FnMut(&Path) -> Result<(), String>) -> Outcome {
    let mut result = Outcome::default();
    // Validate the entire confirmed batch before making its first change.
    if let Some(e) = targets.iter().find_map(|t| t.validate().err()) {
        result.error = Some(e);
        return result;
    }
    for target in targets {
        if let Err(e) = target.validate().and_then(|()| remove(&target.path)) {
            result.error = Some(e);
            break;
        }
        result.done.push(target.path);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("disco-delete-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn desktop_trash_roundtrip_of_owned_fixture() {
        if let Some(root) = std::env::var_os("DISCO_TRASH_TEST_FIXTURE") {
            let root = PathBuf::from(root);
            assert!(root.starts_with(std::env::temp_dir()));
            assert!(
                root.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("disco-delete-roundtrip-")
            );
            let folder = root.join("folder");
            assert_eq!(
                fs::read(folder.join("payload.txt")).unwrap(),
                b"owned fixture"
            );
            let result = move_to_trash(vec![Target::capture(&folder, 13).unwrap()]);
            assert!(result.error.is_none(), "{:?}", result.error);
            assert_eq!(result.done, vec![folder.clone()]);
            assert!(!folder.exists());
            let trashed = root.join("data/Trash/files/folder");
            assert_eq!(
                fs::read(trashed.join("payload.txt")).unwrap(),
                b"owned fixture"
            );
            assert!(root.join("data/Trash/info/folder.trashinfo").is_file());
            fs::rename(trashed, &folder).unwrap();
            assert!(folder.join("payload.txt").is_file());
            let file = root.join("file.txt");
            fs::write(&file, b"plain file").unwrap();
            let target = Target::capture(&file, 10).unwrap();
            assert!(!target.is_dir);
            let result = move_to_trash(vec![target]);
            assert!(result.error.is_none());
            assert!(!file.exists());
            let trashed = root.join("data/Trash/files/file.txt");
            assert_eq!(fs::read(&trashed).unwrap(), b"plain file");
            assert!(root.join("data/Trash/info/file.txt.trashinfo").is_file());
            fs::rename(trashed, &file).unwrap();
            return;
        }
        let root = fixture("roundtrip");
        fs::create_dir(root.join("folder")).unwrap();
        fs::write(root.join("folder/payload.txt"), b"owned fixture").unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "deletion::tests::desktop_trash_roundtrip_of_owned_fixture",
                "--nocapture",
            ])
            .env("DISCO_TRASH_TEST_FIXTURE", &root)
            .env("XDG_DATA_HOME", root.join("data"))
            .status()
            .unwrap();
        assert!(status.success());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn preparing_and_cancelling_request_changes_nothing() {
        let p = fixture("cancel");
        fs::write(p.join("keep.txt"), b"keep").unwrap();
        let request = Target::capture(&p, 4).unwrap();
        drop(request);
        assert_eq!(fs::read(p.join("keep.txt")).unwrap(), b"keep");
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn rejects_symlinks_home_and_system_roots() {
        let p = fixture("guard");
        let home = p.join("home");
        fs::create_dir(&home).unwrap();
        assert!(Target::capture_with_home(&home, 0, Some(&home)).is_err());
        assert!(Target::capture_with_home(&p, 0, Some(&home)).is_err());
        assert!(Target::capture(Path::new("/"), 0).is_err());
        assert!(Target::capture(Path::new("/usr"), 0).is_err());
        std::os::unix::fs::symlink(&home, p.join("link")).unwrap();
        assert!(Target::capture(&p.join("link"), 0).is_err());
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn replaced_directory_is_not_deleted_and_partial_failures_are_reported() {
        let p = fixture("replace");
        let a = p.join("a");
        let b = p.join("b");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        let first = Target::capture(&a, 0).unwrap();
        let second = Target::capture(&b, 0).unwrap();
        fs::rename(&b, p.join("old-b")).unwrap();
        fs::create_dir(&b).unwrap();
        let mut called = false;
        let out = execute(vec![first.clone(), second], |_| {
            called = true;
            Ok(())
        });
        assert!(!called);
        assert!(out.done.is_empty() && out.error.is_some());
        let second = Target::capture(&b, 0).unwrap();
        let out = execute(vec![first, second], |path| {
            if path == a {
                Ok(())
            } else {
                Err("trash unavailable".into())
            }
        });
        assert_eq!(out.done, vec![a]);
        assert!(out.error.is_some());
        assert!(b.exists());
        fs::remove_dir_all(p).unwrap();
    }
}
