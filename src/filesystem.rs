//! Metadata and file-manager integration, independent of application state.
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
#[derive(Clone)]
pub(crate) struct Properties {
    pub(crate) path: PathBuf,
    pub(crate) bytes: u64,
    pub(crate) files: Option<u64>,
    pub(crate) modified: String,
    pub(crate) created: String,
    pub(crate) owner: String,
    pub(crate) permissions: String,
    pub(crate) is_dir: bool,
    pub(crate) error: Option<String>,
}
impl Properties {
    pub(crate) fn load(path: PathBuf, bytes: u64, files: Option<u64>, demo: bool) -> Self {
        let mut p = Self {
            path,
            bytes,
            files,
            modified: "—".into(),
            created: "—".into(),
            owner: "—".into(),
            permissions: "—".into(),
            is_dir: true,
            error: None,
        };
        if demo {
            p.modified = "12.10.2026, 14:27".into();
            p.owner = "alex".into();
            p.permissions = "rwxr-xr-x".into();
            return p;
        }
        match fs::symlink_metadata(&p.path) {
            Ok(m) => {
                p.is_dir = m.is_dir();
                p.modified = m.modified().map(date).unwrap_or_else(|_| "—".into());
                p.created = m.created().map(date).unwrap_or_else(|_| "—".into());
                p.owner = if m.uid() == unsafe { libc::geteuid() } {
                    std::env::var("USER").unwrap_or_else(|_| m.uid().to_string())
                } else {
                    m.uid().to_string()
                };
                p.permissions = permissions(m.mode());
            }
            Err(e) => p.error = Some(e.to_string()),
        }
        p
    }
}
pub(crate) fn permissions(mode: u32) -> String {
    (0..9)
        .map(|i| {
            if mode & (1 << (8 - i)) != 0 {
                ['r', 'w', 'x'][i % 3]
            } else {
                '-'
            }
        })
        .collect()
}
pub(crate) fn date(t: std::time::SystemTime) -> String {
    let d: chrono::DateTime<chrono::Local> = t.into();
    d.format("%d.%m.%Y, %H:%M").to_string()
}
pub(crate) fn drive_info(path: &Path) -> (String, String) {
    let Some(m) = fs::metadata(path).ok() else {
        return (crate::tr!("Диск", "Drive").into(), String::new());
    };
    let dev = m.dev();
    let sys = PathBuf::from(format!(
        "/sys/dev/block/{}:{}",
        libc::major(dev),
        libc::minor(dev)
    ));
    let Ok(sys) = sys.canonicalize() else {
        return (crate::tr!("Диск", "Drive").into(), String::new());
    };
    let mut model = String::new();
    let mut ssd = false;
    for p in [
        Some(sys.as_path()),
        sys.parent(),
        sys.parent().and_then(Path::parent),
    ]
    .into_iter()
    .flatten()
    {
        if model.is_empty() {
            model = fs::read_to_string(p.join("device/model"))
                .unwrap_or_default()
                .trim()
                .to_string();
        }
        if fs::read_to_string(p.join("queue/rotational"))
            .map(|s| s.trim() == "0")
            .unwrap_or(false)
        {
            ssd = true;
        }
    }
    if model == "CT1000P3SSD8" {
        model = "Crucial P3 · 1 TB".into();
    }
    (
        if ssd {
            crate::tr!("Системний SSD", "System SSD")
        } else {
            crate::tr!("Системний диск", "System drive")
        }
        .into(),
        model,
    )
}
pub(crate) fn open(p: &Path) {
    let _ = std::process::Command::new("xdg-open").arg(p).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permission_display() {
        assert_eq!(permissions(0o755), "rwxr-xr-x");
        assert_eq!(permissions(0o600), "rw-------");
    }
}
