//! Launch an existing desktop association; never fall back to an app chooser.
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
fn query(args: &[&OsStr]) -> Option<String> {
    let output = Command::new("xdg-mime")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.len() > 8192 {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_owned())
}
fn application_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home.map(|p| p.join(".local/share")));
    let mut dirs = data.into_iter().collect::<Vec<_>>();
    dirs.extend(
        std::env::split_paths(
            &std::env::var_os("XDG_DATA_DIRS")
                .unwrap_or_else(|| "/usr/local/share:/usr/share".into()),
        )
        .filter(|p| p.is_absolute()),
    );
    dirs.into_iter().map(|p| p.join("applications")).collect()
}
fn desktop_file(id: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    if !id.ends_with(".desktop")
        || id.contains('/')
        || id.contains('\\')
        || id.chars().any(char::is_control)
    {
        return None;
    }
    for dir in dirs {
        let direct = dir.join(id);
        if direct.is_file() {
            return Some(direct);
        }
        // Desktop IDs may encode subdirectories with '-'. Respect XDG directory
        // priority; never interpret an Exec field ourselves or invoke a shell.
        for entry in walkdir::WalkDir::new(dir)
            .max_depth(5)
            .into_iter()
            .filter_map(Result::ok)
            .take(10_000)
        {
            if entry.file_type().is_file()
                && entry.path().extension() == Some(OsStr::new("desktop"))
            {
                let rel = entry.path().strip_prefix(dir).ok()?;
                if rel
                    .components()
                    .map(|p| p.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("-")
                    == id
                {
                    return Some(entry.into_path());
                }
            }
        }
    }
    None
}
fn resolve(
    path: &Path,
    dirs: &[PathBuf],
    mut query: impl FnMut(&[&OsStr]) -> Option<String>,
) -> Option<PathBuf> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !path.is_absolute() || !metadata.is_file() {
        return None;
    }
    let mime = query(&[
        OsStr::new("query"),
        OsStr::new("filetype"),
        path.as_os_str(),
    ])?;
    if mime.is_empty() || !mime.contains('/') || mime.chars().any(char::is_whitespace) {
        return None;
    }
    let id = query(&[
        OsStr::new("query"),
        OsStr::new("default"),
        OsStr::new(&mime),
    ])?;
    desktop_file(&id, dirs)
}
pub(crate) fn open(path: &Path) {
    if let Some(desktop) = resolve(path, &application_dirs(), query) {
        let _ = Command::new("gio")
            .arg("launch")
            .arg(desktop)
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_installed_associations_resolve_and_paths_remain_separate_arguments() {
        let root = std::env::temp_dir().join(format!("disco-association-{}", std::process::id()));
        std::fs::create_dir_all(root.join("applications")).unwrap();
        let file = root.join("strange ' $(name).txt");
        std::fs::write(&file, "hello").unwrap();
        let dirs = vec![root.join("applications")];
        let desktop = dirs[0].join("viewer.desktop");
        std::fs::write(&desktop, "[Desktop Entry]\nType=Application\nExec=false\n").unwrap();
        let mut calls = 0;
        let resolved = resolve(&file, &dirs, |args| {
            calls += 1;
            if calls == 1 {
                assert_eq!(args[2], file.as_os_str());
                Some("text/plain".into())
            } else {
                Some("viewer.desktop".into())
            }
        });
        assert_eq!(resolved, Some(desktop));
        for id in [
            "",
            "missing.desktop",
            "../viewer.desktop",
            "/tmp/viewer.desktop",
        ] {
            assert!(desktop_file(id, &dirs).is_none());
        }
        assert!(resolve(&file, &dirs, |_| None).is_none());
        assert!(resolve(&root, &dirs, |_| panic!("directories must not be launched")).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
