//! Read-only sample data for previews and frame diagnostics.
use crate::scan::{BigFile, Folder, Report};
use std::{path::PathBuf, time::Duration};
pub(crate) fn report() -> Report {
    let base = PathBuf::from("/demo/home/alex");
    let mut folders = vec![Folder {
        path: base.clone(),
        parent: None,
        bytes: 0,
        files: 0,
    }];
    let gib = 1024u64.pow(3);
    for (n, b) in [
        ("Games", 355 * gib),
        ("workspace", 23 * gib),
        (".cache", 32 * gib),
        ("Documents", 8 * gib),
        ("other", 192 * gib),
    ] {
        folders.push(Folder {
            path: base.join(n),
            parent: Some(0),
            bytes: b,
            files: 1000,
        });
    }
    for (n, b) in [
        ("yay", 14.8),
        ("spotify", 4.3),
        ("pip", 2.1),
        ("thumbnails", 1.9),
        ("mesa_shader_cache", 1.6),
        ("google-chrome", 1.2),
        ("other", 6.1),
    ] {
        folders.push(Folder {
            path: base.join(".cache").join(n),
            parent: Some(3),
            bytes: (b * gib as f64) as u64,
            files: if n == "yay" { 12436 } else { 1000 },
        });
    }
    folders.push(Folder {
        path: base.join(".cache/yay/src"),
        parent: Some(6),
        bytes: 8 * gib,
        files: 8000,
    });
    folders[0].bytes = 610 * gib;
    Report {
        folders,
        largest: vec![BigFile {
            bytes: 5 * gib,
            path: base.join("other/model.gguf"),
        }],
        entries: 184229,
        elapsed: Duration::from_secs(18),
        complete: true,
        ..Default::default()
    }
}
