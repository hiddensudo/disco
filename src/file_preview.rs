//! Bounded file previews; content I/O and image decoding run on a worker.
use image::{ImageDecoder, ImageReader};
use std::{
    fs::{File, OpenOptions},
    io::{BufReader, Read},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

const TEXT_BYTES: usize = 128 * 1024;
const TEXT_LINES: usize = 2000;
const LINE_CHARS: usize = 2048;
const IMAGE_BYTES: u64 = 32 * 1024 * 1024;
const IMAGE_PIXELS: u64 = 12_000_000;
const THUMBNAIL_EDGE: u32 = 2048;

pub(crate) enum Content {
    Image {
        rgba: Vec<u8>,
        size: [usize; 2],
        original: [u32; 2],
    },
    Text {
        lines: Vec<String>,
        display_source: String,
        encoding: &'static str,
        truncated: bool,
        document: Option<crate::text_editor::Document>,
    },
    Unsupported,
    TooLarge,
    Error(String),
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp(u64, u64, u64, i64, i64, i64, i64);
fn stamp(m: &std::fs::Metadata) -> Stamp {
    Stamp(
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
fn regular_file(path: &Path) -> std::io::Result<File> {
    let f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !f.metadata()?.is_file() {
        return Err(std::io::Error::other("Not a regular file"));
    }
    Ok(f)
}
fn read_text(bytes: &[u8], clipped: bool) -> Option<Content> {
    let (text, encoding) = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if (bytes.len() - 2) % 2 != 0 && !clipped {
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
        let units = if clipped && units.last().is_some_and(|u| (0xd800..=0xdbff).contains(u)) {
            &units[..units.len() - 1]
        } else {
            &units[..]
        };
        (
            String::from_utf16(units).ok()?,
            if le { "UTF-16 LE" } else { "UTF-16 BE" },
        )
    } else {
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        let text = match std::str::from_utf8(bytes) {
            Ok(s) => s,
            Err(e) if clipped && e.error_len().is_none() => {
                std::str::from_utf8(&bytes[..e.valid_up_to()]).ok()?
            }
            Err(_) => return None,
        };
        (text.to_owned(), "UTF-8")
    };
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return None;
    }
    let mut truncated = clipped;
    let mut lines = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if n >= TEXT_LINES {
            truncated = true;
            break;
        }
        let mut chars = line.chars();
        let mut line: String = chars.by_ref().take(LINE_CHARS).collect();
        if chars.next().is_some() {
            line.push('…');
            truncated = true;
        }
        // Tabs are displayed, never interpreted as terminal escape sequences.
        lines.push(line.replace('\t', "    "));
    }
    Some(Content::Text {
        display_source: lines.join("\n"),
        lines,
        encoding,
        truncated,
        document: None,
    })
}
fn load_inner(path: &Path) -> Result<Content, Box<dyn std::error::Error>> {
    let file = regular_file(path)?;
    let before = file.metadata()?;
    let mut reader = ImageReader::new(BufReader::new(file)).with_guessed_format()?;
    let content = if reader.format().is_some() {
        if before.len() > IMAGE_BYTES {
            return Ok(Content::TooLarge);
        }
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(20000);
        limits.max_image_height = Some(20000);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        let mut decoder = reader.into_decoder()?;
        let (w, h) = decoder.dimensions();
        if u64::from(w) * u64::from(h) > IMAGE_PIXELS {
            return Ok(Content::TooLarge);
        }
        let orientation = decoder
            .orientation()
            .unwrap_or(image::metadata::Orientation::NoTransforms);
        let mut image = image::DynamicImage::from_decoder(decoder)?;
        image.apply_orientation(orientation);
        let original = [image.width(), image.height()];
        let image = image.thumbnail(THUMBNAIL_EDGE, THUMBNAIL_EDGE).into_rgba8();
        Content::Image {
            size: [image.width() as usize, image.height() as usize],
            rgba: image.into_raw(),
            original,
        }
    } else {
        let mut bytes = Vec::new();
        reader
            .into_inner()
            .take((TEXT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        let clipped = bytes.len() > TEXT_BYTES;
        bytes.truncate(TEXT_BYTES);
        let mut text = read_text(&bytes, clipped).unwrap_or(Content::Unsupported);
        if let Content::Text {
            truncated: false,
            document,
            ..
        } = &mut text
        {
            *document = crate::text_editor::Document::from_bytes(&bytes, &before);
        }
        text
    };
    // Reject a replaced or concurrently modified file instead of displaying a
    // mixture of versions. Reopening also rechecks the no-symlink policy.
    if stamp(&before) != stamp(&regular_file(path)?.metadata()?) {
        return Err(std::io::Error::other(crate::tr!(
            "Файл змінився під час читання. Онови прев’ю.",
            "The file changed while reading. Refresh the preview."
        ))
        .into());
    }
    Ok(content)
}
pub(crate) fn load(path: &Path) -> Content {
    match load_inner(path) {
        Ok(c) => c,
        Err(e) => Content::Error(e.to_string()),
    }
}

/// One active decoder and one latest selection, never a thread per hover/click.
#[derive(Default)]
pub(crate) struct Preview {
    pub path: Option<PathBuf>,
    pub content: Option<Content>,
    pub texture: Option<eframe::egui::TextureHandle>,
    pub zoom: f32,
    generation: u64,
    revision: u64,
    waiting_since: Option<Instant>,
    job: Option<Receiver<(u64, Content)>>,
}
impl Preview {
    pub fn select(&mut self, path: Option<PathBuf>, revision: u64) {
        if self.path == path && self.revision == revision {
            return;
        }
        self.path = path;
        self.revision = revision;
        self.reload();
    }
    pub fn adopt_revision(&mut self, revision: u64) {
        self.revision = revision;
    }
    pub fn reload(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.content = None;
        self.texture = None;
        self.zoom = 1.;
        self.waiting_since = self.path.as_ref().map(|_| Instant::now());
    }
    pub fn poll(&mut self, ctx: &eframe::egui::Context) {
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok((generation, mut content)) => {
                    self.job = None;
                    if generation == self.generation {
                        if let Content::Image { rgba, size, .. } = &mut content {
                            let pixels =
                                eframe::egui::ColorImage::from_rgba_unmultiplied(*size, rgba);
                            self.texture = Some(ctx.load_texture(
                                "file-preview",
                                pixels,
                                eframe::egui::TextureOptions::LINEAR,
                            ));
                            rgba.clear();
                            rgba.shrink_to_fit();
                        }
                        self.content = Some(content);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.job = None;
                    if self.waiting_since.is_none() && self.path.is_some() {
                        self.content = Some(Content::Error(
                            crate::tr!("Не вдалося прочитати файл", "Could not read the file")
                                .into(),
                        ));
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(since) = self.waiting_since {
            if self.job.is_none() && since.elapsed() >= Duration::from_millis(100) {
                if let Some(path) = self.path.clone() {
                    let generation = self.generation;
                    let (tx, rx) = mpsc::channel();
                    let ctx = ctx.clone();
                    self.job = Some(rx);
                    self.waiting_since = None;
                    std::thread::spawn(move || {
                        crate::scan::background_priority();
                        let content =
                            std::panic::catch_unwind(|| load(&path)).unwrap_or_else(|_| {
                                Content::Error(
                                    crate::tr!("Помилка декодування", "Decoding failed").into(),
                                )
                            });
                        let _ = tx.send((generation, content));
                        ctx.request_repaint();
                    });
                }
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_handles_boms_binary_limits_and_partial_unicode() {
        assert!(matches!(read_text(b"a\0b", false), None));
        assert!(matches!(
            read_text(&[0xff, 0xfe, b'a', 0, b'b', 0], false),
            Some(Content::Text {
                encoding: "UTF-16 LE",
                ..
            })
        ));
        let mut data = b"abc".to_vec();
        data.extend_from_slice(&[0xe2, 0x82]);
        assert!(matches!(
            read_text(&data, true),
            Some(Content::Text {
                truncated: true,
                ..
            })
        ));
        assert!(read_text(&data, false).is_none());
        let long = "x".repeat(LINE_CHARS + 1);
        let Some(Content::Text {
            lines, truncated, ..
        }) = read_text(long.as_bytes(), false)
        else {
            panic!()
        };
        assert!(truncated);
        assert_eq!(lines[0].chars().count(), LINE_CHARS + 1);
    }
    #[test]
    fn images_are_bounded_and_non_regular_files_are_rejected() {
        let base = std::env::temp_dir().join(format!("disco-preview-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let path = base.join("test.png");
        image::RgbaImage::from_pixel(2200, 10, image::Rgba([200, 40, 30, 255]))
            .save(&path)
            .unwrap();
        let Content::Image { size, original, .. } = load(&path) else {
            panic!()
        };
        assert_eq!(original, [2200, 10]);
        assert_eq!(size[0], 2048);
        for format in [
            image::ImageFormat::Jpeg,
            image::ImageFormat::WebP,
            image::ImageFormat::Bmp,
            image::ImageFormat::Tiff,
        ] {
            let path = base.join(format!("image.{format:?}"));
            image::RgbImage::from_pixel(20, 10, image::Rgb([200, 40, 30]))
                .save_with_format(&path, format)
                .unwrap();
            assert!(
                matches!(
                    load(&path),
                    Content::Image {
                        original: [20, 10],
                        ..
                    }
                ),
                "{format:?}"
            );
        }
        let link = base.join("link.png");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(matches!(load(&link), Content::Error(_)));
        assert!(matches!(load(&base), Content::Error(_)));
        let fifo = base.join("pipe");
        let c = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        assert!(matches!(load(&fifo), Content::Error(_)));
        std::fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn stale_selection_never_replaces_the_current_preview() {
        let mut p = Preview::default();
        p.select(Some("/a".into()), 0);
        let old = p.generation;
        let (tx, rx) = mpsc::channel();
        p.job = Some(rx);
        p.select(Some("/b".into()), 0);
        tx.send((old, Content::Unsupported)).unwrap();
        p.poll(&eframe::egui::Context::default());
        assert!(p.content.is_none());
        assert_eq!(p.path, Some("/b".into()));
    }
}
