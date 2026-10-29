//! Paths are relative to the chosen folder, even with a leading slash.
//! Descriptor-relative traversal prevents nested symlinks from escaping it.
use std::{
    ffi::CString,
    fs::File,
    io::Write,
    os::fd::{AsRawFd, FromRawFd},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
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
    pub metadata: std::fs::Metadata,
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
fn create_inner(directory: &Path, names: &[&str], contents: &[u8]) -> std::io::Result<Created> {
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
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temporary = CString::new(format!(
        ".disco-new-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
    .unwrap();
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            temporary.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o666,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut output = unsafe { File::from_raw_fd(fd) };
    let result = (|| {
        output.write_all(contents)?;
        output.sync_all()?;
        // Publish only a complete file, and never replace an existing path.
        let result = unsafe {
            libc::renameat2(
                parent.as_raw_fd(),
                temporary.as_ptr(),
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if result < 0 {
            return Err(std::io::Error::last_os_error());
        }
        path.push(names.last().unwrap());
        let metadata = output.metadata()?;
        let _ = parent.sync_all();
        Ok(Created {
            path,
            directories,
            metadata,
        })
    })();
    if result.is_err() {
        unsafe {
            libc::unlinkat(parent.as_raw_fd(), temporary.as_ptr(), 0);
        }
    }
    result
}

pub(crate) fn create(directory: &Path, name: &str) -> Result<Created, String> {
    let names = parts(name)?;
    create_inner(directory, &names, &[]).map_err(|e| {
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
/// Save-As paths use ordinary absolute/relative semantics, unlike legacy Ctrl+N.
pub(crate) fn create_document(
    base: &Path,
    input: &str,
    contents: &[u8],
) -> Result<Created, String> {
    use std::path::Component;
    let input = input.trim();
    if input.is_empty()
        || input.len() > 4096
        || input.chars().any(char::is_control)
        || input.ends_with('/')
        || matches!(input.rsplit('/').next(), Some("." | ".."))
        || !matches!(
            Path::new(input).components().next_back(),
            Some(Component::Normal(_))
        )
    {
        return Err(crate::tr!(
            "Вкажи шлях до файла, наприклад notes.txt або /home/user/notes.txt",
            "Enter a file path, such as notes.txt or /home/user/notes.txt"
        )
        .into());
    }
    let joined = if Path::new(input).is_absolute() {
        PathBuf::from(input)
    } else {
        base.join(input)
    };
    let mut target = PathBuf::new();
    for part in joined.components() {
        match part {
            Component::ParentDir => {
                target.pop();
            }
            Component::CurDir => {}
            _ => target.push(part.as_os_str()),
        }
    }
    let path = target.to_str().ok_or_else(|| {
        crate::tr!(
            "Шлях має бути коректним UTF-8",
            "The path must be valid UTF-8"
        )
        .to_string()
    })?;
    if !target.is_absolute() {
        return Err(crate::tr!(
            "Базова папка має бути абсолютною",
            "The base directory must be absolute"
        )
        .into());
    }
    let names = parts(path)?;
    create_inner(Path::new("/"), &names, contents).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            crate::tr!("Файл уже існує. Обери інший шлях; наявні файли не перезаписуються.", "The file already exists. Choose another path; existing files are never overwritten.").into()
        } else { crate::tr!("Не вдалося зберегти файл: {error}", "Could not save the file: {error}", error = e) }
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

#[cfg(test)]
mod save_as_tests {
    use super::*;
    #[test]
    fn first_save_resolves_relative_absolute_paths_and_publishes_complete_content() {
        let root = std::env::temp_dir().join(format!("disco-save-as-{}", std::process::id()));
        let base = root.join("workspace/dotfiles");
        std::fs::create_dir_all(&base).unwrap();
        let data = "Hello 🦀\n世界\n".as_bytes();
        let relative = create_document(&base, "nested/file.txt", data).unwrap();
        assert_eq!(relative.path, base.join("nested/file.txt"));
        assert_eq!(std::fs::read(&relative.path).unwrap(), data);
        assert_eq!(relative.metadata.len(), data.len() as u64);
        assert!(create_document(&base, "nested/file.txt", b"do not overwrite").is_err());
        assert_eq!(std::fs::read(&relative.path).unwrap(), data);
        let absolute = root.join("outside/new.txt");
        create_document(&base, absolute.to_str().unwrap(), b"absolute").unwrap();
        assert_eq!(std::fs::read(absolute).unwrap(), b"absolute");
        let parent = create_document(&base, "../parent.txt", b"relative parent").unwrap();
        assert_eq!(parent.path, root.join("workspace/parent.txt"));
        for name in ["", "/", "../", "bad\nname", "folder/"] {
            assert!(create_document(&base, name, b"invalid").is_err());
        }
        std::os::unix::fs::symlink(root.join("outside"), base.join("link")).unwrap();
        assert!(create_document(&base, "link/escape.txt", b"no").is_err());
        assert!(!root.join("outside/escape.txt").exists());
        assert!(
            !walkdir::WalkDir::new(&root)
                .into_iter()
                .filter_map(Result::ok)
                .any(|e| e.file_name().to_string_lossy().starts_with(".disco-new-"))
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
