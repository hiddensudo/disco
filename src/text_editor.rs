//! Complete text documents and optimistic, atomic saves. Preview fragments are
//! never writable documents, and an external replacement is never overwritten.
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Stamp {
    dev: u64,
    ino: u64,
    len: u64,
    mtime: i64,
    mn: i64,
    ctime: i64,
    cn: i64,
}
impl Stamp {
    pub fn of(m: &fs::Metadata) -> Self {
        Self {
            dev: m.dev(),
            ino: m.ino(),
            len: m.len(),
            mtime: m.mtime(),
            mn: m.mtime_nsec(),
            ctime: m.ctime(),
            cn: m.ctime_nsec(),
        }
    }
}
#[derive(Clone, Copy, Debug)]
enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}
#[derive(Clone, Debug)]
pub(crate) struct Document {
    pub text: String,
    baseline: String,
    original: Vec<u8>,
    stamp: Stamp,
    encoding: Encoding,
    pub crlf: bool,
}
impl Document {
    pub fn from_bytes(bytes: &[u8], m: &fs::Metadata) -> Option<Self> {
        if bytes.len() > 128 * 1024 {
            return None;
        }
        let (text, encoding) =
            if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
                if bytes.len() % 2 != 0 {
                    return None;
                }
                let le = bytes[0] == 0xff;
                let units = bytes[2..]
                    .chunks_exact(2)
                    .map(|p| {
                        if le {
                            u16::from_le_bytes([p[0], p[1]])
                        } else {
                            u16::from_be_bytes([p[0], p[1]])
                        }
                    })
                    .collect::<Vec<_>>();
                (
                    String::from_utf16(&units).ok()?,
                    if le {
                        Encoding::Utf16Le
                    } else {
                        Encoding::Utf16Be
                    },
                )
            } else if let Some(bytes) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
                (
                    std::str::from_utf8(bytes).ok()?.to_owned(),
                    Encoding::Utf8Bom,
                )
            } else {
                (std::str::from_utf8(bytes).ok()?.to_owned(), Encoding::Utf8)
            };
        let crlf = text
            .find('\n')
            .is_some_and(|i| i > 0 && text.as_bytes()[i - 1] == b'\r');
        let text = text.replace("\r\n", "\n");
        Some(Self {
            baseline: text.clone(),
            text,
            original: bytes.to_vec(),
            stamp: Stamp::of(m),
            encoding,
            crlf,
        })
    }
    pub fn dirty(&self) -> bool {
        self.text != self.baseline
    }
    pub fn buffer(&mut self) -> Buffer<'_> {
        Buffer {
            text: &mut self.text,
        }
    }
    pub fn encoded(&self) -> Vec<u8> {
        let text = if self.crlf {
            self.text.replace("\r\n", "\n").replace('\n', "\r\n")
        } else {
            self.text.clone()
        };
        match self.encoding {
            Encoding::Utf8 => text.as_bytes().to_vec(),
            Encoding::Utf8Bom => [&[0xef, 0xbb, 0xbf][..], text.as_bytes()].concat(),
            Encoding::Utf16Le | Encoding::Utf16Be => {
                let le = matches!(self.encoding, Encoding::Utf16Le);
                let mut out = if le {
                    vec![0xff, 0xfe]
                } else {
                    vec![0xfe, 0xff]
                };
                for u in text.encode_utf16() {
                    out.extend_from_slice(&if le { u.to_le_bytes() } else { u.to_be_bytes() });
                }
                out
            }
        }
    }
    #[cfg(test)]
    pub fn mark_saved(&mut self, stamp: Stamp) {
        self.original = self.encoded();
        self.baseline = self.text.clone();
        self.stamp = stamp;
    }
    /// The user can keep typing while the saved snapshot is written in the
    /// background. Only that snapshot becomes the baseline, never newer edits.
    pub fn accept_saved(&mut self, stamp: Stamp, saved: &Document) {
        self.original = saved.encoded();
        self.baseline = saved.text.clone();
        self.stamp = stamp;
    }
}
/// Preserve the existing line-ending convention for newly inserted text too.
pub(crate) struct Buffer<'a> {
    text: &'a mut String,
}
impl eframe::egui::TextBuffer for Buffer<'_> {
    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Buffer<'static>>()
    }

    fn is_mutable(&self) -> bool {
        true
    }
    fn as_str(&self) -> &str {
        self.text
    }
    fn insert_text(&mut self, text: &str, index: eframe::egui::text::CharIndex) -> usize {
        let normalized = text.replace("\r\n", "\n");
        if self.text.len() + normalized.len() > 256 * 1024 {
            return 0;
        }
        eframe::egui::TextBuffer::insert_text(self.text, &normalized, index)
    }
    fn delete_char_range(&mut self, range: std::ops::Range<eframe::egui::text::CharIndex>) {
        eframe::egui::TextBuffer::delete_char_range(self.text, range);
    }
}
fn read_regular(path: &Path) -> std::io::Result<File> {
    let f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !f.metadata()?.is_file() {
        return Err(std::io::Error::other("Not a regular file"));
    }
    Ok(f)
}
fn unchanged(path: &Path, doc: &Document) -> std::io::Result<File> {
    let mut f = read_regular(path)?;
    if Stamp::of(&f.metadata()?) != doc.stamp {
        return Err(conflict());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut f)
        .take((doc.original.len() + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes != doc.original {
        return Err(conflict());
    }
    Ok(f)
}
fn conflict() -> std::io::Error {
    std::io::Error::other(crate::tr!(
        "Файл змінився на диску. Зміни не записані; скопіюй свій текст або скасуй редагування й онови файл.",
        "The file changed on disk. Nothing was written; copy your text, or discard edits and reload the file."
    ))
}
fn copy_attributes(from: &File, to: &File) -> std::io::Result<()> {
    let fd = from.as_raw_fd();
    let n = unsafe { libc::flistxattr(fd, std::ptr::null_mut(), 0) };
    if n < 0 {
        let e = std::io::Error::last_os_error();
        return if e.raw_os_error() == Some(libc::ENOTSUP) {
            Ok(())
        } else {
            Err(e)
        };
    }
    if n > 65536 {
        return Err(std::io::Error::other(
            "Extended attributes exceed the save limit",
        ));
    }
    let mut names = vec![0u8; n as usize];
    let n = unsafe { libc::flistxattr(fd, names.as_mut_ptr().cast(), names.len()) };
    if n < 0 {
        return Err(std::io::Error::last_os_error());
    }
    for name in names[..n as usize]
        .split(|b| *b == 0)
        .filter(|n| !n.is_empty())
    {
        let name = std::ffi::CString::new(name).map_err(std::io::Error::other)?;
        let len = unsafe { libc::fgetxattr(fd, name.as_ptr(), std::ptr::null_mut(), 0) };
        if !(0..=65536).contains(&len) {
            return Err(std::io::Error::other(
                "Could not preserve extended attributes",
            ));
        }
        let mut value = vec![0u8; len as usize];
        let len =
            unsafe { libc::fgetxattr(fd, name.as_ptr(), value.as_mut_ptr().cast(), value.len()) };
        if len < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if unsafe {
            libc::fsetxattr(
                to.as_raw_fd(),
                name.as_ptr(),
                value.as_ptr().cast(),
                len as usize,
                0,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}
static TEMP: AtomicU64 = AtomicU64::new(0);
pub(crate) fn save(path: &Path, doc: &Document) -> Result<Stamp, String> {
    let mut temporary: Option<PathBuf> = None;
    let result = (|| -> std::io::Result<Stamp> {
        let source = unchanged(path, doc)?;
        let metadata = source.metadata()?;
        if metadata.nlink() != 1 {
            return Err(std::io::Error::other(crate::tr!(
                "Файл має hardlink. Відкрий його у зовнішньому редакторі.",
                "This file has hardlinks. Use an external editor."
            )));
        }
        // Opening without truncation respects the file's write permissions.
        let _permission = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?;
        let parent = path
            .parent()
            .ok_or_else(|| std::io::Error::other("No parent directory"))?;
        let tmp = parent.join(format!(
            ".disco-edit-{}-{}.tmp",
            std::process::id(),
            TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        temporary = Some(tmp.clone());
        output.write_all(&doc.encoded())?;
        let fresh = output.metadata()?;
        if (fresh.uid(), fresh.gid()) != (metadata.uid(), metadata.gid())
            && unsafe { libc::fchown(output.as_raw_fd(), metadata.uid(), metadata.gid()) } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        output.set_permissions(fs::Permissions::from_mode(metadata.mode() & 0o7777))?;
        copy_attributes(&source, &output)?;
        output.sync_all()?;
        let _ = unchanged(path, doc)?;
        fs::rename(&tmp, path)?;
        temporary = None;
        let _ = File::open(parent).and_then(|dir| dir.sync_all());
        Ok(Stamp::of(&output.metadata()?))
    })();
    if let Some(tmp) = temporary {
        let _ = fs::remove_file(tmp);
    }
    result.map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typing_during_save_stays_dirty_and_can_be_saved_again() {
        let path = std::env::temp_dir().join(format!("disco-save-snapshot-{}", std::process::id()));
        fs::write(&path, "initial\n").unwrap();
        let mut doc = Document::from_bytes(b"initial\n", &fs::metadata(&path).unwrap()).unwrap();
        doc.text.push_str("saved\n");
        let snapshot = doc.clone();
        let stamp = save(&path, &snapshot).unwrap();
        doc.text.push_str("typed while saving\n");
        doc.accept_saved(stamp, &snapshot);
        assert!(doc.dirty());
        assert_eq!(fs::read_to_string(&path).unwrap(), snapshot.text);
        let second = doc.clone();
        let stamp = save(&path, &second).unwrap();
        doc.accept_saved(stamp, &second);
        assert!(!doc.dirty());
        assert_eq!(fs::read_to_string(&path).unwrap(), doc.text);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn atomic_saves_preserve_encoding_line_endings_and_permissions_and_reject_conflicts() {
        let root = std::env::temp_dir().join(format!("disco-editor-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        for (n, bytes) in [
            b"abc\r\n".to_vec(),
            [vec![0xef, 0xbb, 0xbf], b"abc\n".to_vec()].concat(),
            vec![0xff, 0xfe, b'a', 0, b'b', 0],
        ]
        .into_iter()
        .enumerate()
        {
            let path = root.join(format!("{n}.txt"));
            fs::write(&path, &bytes).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
            let attr_name = std::ffi::CString::new("user.disco-test").unwrap();
            let file = File::open(&path).unwrap();
            let attr_supported = unsafe {
                libc::fsetxattr(
                    file.as_raw_fd(),
                    attr_name.as_ptr(),
                    b"fixture".as_ptr().cast(),
                    7,
                    0,
                )
            } == 0;
            let mut doc = Document::from_bytes(&bytes, &fs::metadata(&path).unwrap()).unwrap();
            use eframe::egui::TextBuffer;
            let len = doc.text.chars().count();
            doc.buffer()
                .insert_text("x\n", eframe::egui::text::CharIndex(len));
            let stamp = save(&path, &doc).unwrap();
            assert_eq!(fs::read(&path).unwrap(), doc.encoded());
            assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o640);
            if attr_supported {
                let file = File::open(&path).unwrap();
                let mut value = [0u8; 7];
                assert_eq!(
                    unsafe {
                        libc::fgetxattr(
                            file.as_raw_fd(),
                            attr_name.as_ptr(),
                            value.as_mut_ptr().cast(),
                            value.len(),
                        )
                    },
                    7
                );
                assert_eq!(&value, b"fixture");
            }
            doc.mark_saved(stamp);
            assert!(!doc.dirty());
            if n == 0 {
                assert!(fs::read(&path).unwrap().ends_with(b"x\r\n"));
            }
            fs::write(&path, b"external").unwrap();
            doc.text.push('!');
            assert!(save(&path, &doc).is_err());
            assert_eq!(fs::read(&path).unwrap(), b"external");
        }
        let p = root.join("source.txt");
        fs::write(&p, b"keep").unwrap();
        let mut doc = Document::from_bytes(b"keep", &fs::metadata(&p).unwrap()).unwrap();
        doc.text.push('!');
        let link = root.join("link.txt");
        std::os::unix::fs::symlink(&p, &link).unwrap();
        assert!(save(&link, &doc).is_err());
        assert!(fs::read_dir(&root).unwrap().all(|e| {
            !e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".disco-edit")
        }));
        fs::remove_dir_all(root).unwrap();
    }
}
