//! Paths are relative to the chosen folder, even with a leading slash.
//! Descriptor-relative traversal prevents nested symlinks from escaping it.
use std::{
    ffi::CString,
    fs::File,
    os::fd::{AsRawFd, FromRawFd},
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub(crate) struct Directory {
    pub path: PathBuf,
    pub bytes: u64,
    pub created: bool,
}
#[derive(Debug)]
pub(crate) struct Created {
    pub path: PathBuf,
    pub directories: Vec<Directory>,
}
fn parts(input: &str) -> Result<Vec<&str>, String> {
    let input = input.trim_start_matches('/');
    let parts: Vec<_> = input.split('/').collect();
    if input.is_empty()
        || input.chars().any(char::is_control)
        || parts.len() > 64
        || parts
            .iter()
            .any(|p| p.trim().is_empty() || matches!(*p, "." | "..") || p.len() > 255)
    {
        return Err(crate::tr!(
            "Вкажи назву або шлях у поточній папці, наприклад new_folder/file.txt. Порожні частини шляху, . та .. не дозволені.",
            "Enter a name or a path within the current folder, for example new_folder/file.txt. Empty path components, . and .. are not allowed."
        ).into());
    }
    Ok(parts)
}
fn open_directory(parent: &File, name: &CString) -> std::io::Result<File> {
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
fn create_inner(directory: &Path, names: &[&str]) -> std::io::Result<Created> {
    let mut parent = File::open(directory)?;
    let mut path = directory.to_path_buf();
    let mut directories = Vec::new();
    for part in &names[..names.len() - 1] {
        let name = CString::new(*part).expect("validated path");
        let mut created = false;
        let child = match open_directory(&parent, &name) {
            Ok(child) => child,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let result = unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o777) };
                if result < 0 {
                    let error = std::io::Error::last_os_error();
                    if error.kind() != std::io::ErrorKind::AlreadyExists {
                        return Err(error);
                    }
                } else {
                    created = true;
                }
                open_directory(&parent, &name)?
            }
            Err(error) => return Err(error),
        };
        path.push(part);
        directories.push(Directory {
            path: path.clone(),
            bytes: crate::scan::allocated(&child.metadata()?),
            created,
        });
        parent = child;
    }
    let name = CString::new(*names.last().unwrap()).expect("validated path");
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o666,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    drop(unsafe { File::from_raw_fd(fd) });
    path.push(names.last().unwrap());
    Ok(Created { path, directories })
}
pub(crate) fn create(directory: &Path, name: &str) -> Result<Created, String> {
    let names = parts(name)?;
    create_inner(directory, &names).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            crate::tr!(
                "Файл із такою назвою вже існує. Обери іншу назву.",
                "A file with this name already exists. Choose another name."
            )
            .into()
        } else {
            crate::tr!(
                "Не вдалося створити файл: {error}",
                "Could not create the file: {error}",
                error = e
            )
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_paths_stay_relative_preserve_existing_content_and_reject_invalid_input() {
        let root = std::env::temp_dir().join(format!("disco-create-{}", std::process::id()));
        std::fs::create_dir_all(root.join("base")).unwrap();
        let base = root.join("base");
        for name in [
            "notes.custom",
            "without-extension",
            ".config",
            "with spaces.test",
            "/new_folder/sub/file.txt",
        ] {
            let created = create(&base, name).unwrap();
            assert!(created.path.starts_with(&base));
            std::fs::write(&created.path, "keep this").unwrap();
            assert!(create(&base, name).is_err());
            assert_eq!(std::fs::read_to_string(&created.path).unwrap(), "keep this");
        }
        assert!(base.join("new_folder/sub/file.txt").is_file());
        for name in [
            "",
            "  ",
            ".",
            "..",
            "../outside",
            "folder/../outside",
            "folder/",
            "folder//file",
            "bad\0name",
            "bad\nname",
        ] {
            assert!(create(&base, name).is_err(), "{name:?}");
        }
        assert!(
            !base.join("folder").exists(),
            "validate the whole path before any mkdir"
        );
        std::fs::create_dir(root.join("outside")).unwrap();
        std::os::unix::fs::symlink(root.join("outside"), base.join("link")).unwrap();
        assert!(create(&base, "link/escaped.txt").is_err());
        assert!(!root.join("outside/escaped.txt").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
