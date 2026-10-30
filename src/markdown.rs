//! Native Markdown reader. Parsing is delegated to pulldown/CommonMark;
//! document modes keep the existing editor, undo state and save path intact.
use eframe::egui::{
    self,
    load::{ImageLoadResult, ImageLoader, ImagePoll, LoadError, SizeHint},
};
use egui_commonmark::{CommonMarkCache, CommonMarkScrollOptions, CommonMarkViewer};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub fn supported(path: &Path) -> bool {
    path.extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("md") || s.eq_ignore_ascii_case("markdown"))
}

#[derive(Default)]
pub struct Reader {
    pub editing: bool,
    source: Option<String>,
    path: Option<PathBuf>,
    rendered: String,
    cache: Option<CommonMarkCache>,
    links: Vec<(String, Target)>,
    images: Vec<String>,
    font_size: Option<u32>,
    #[cfg(test)]
    rebuilds: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Local(PathBuf),
    External(String),
    Blocked,
}

fn local_path(base: &Path, uri: &str) -> Option<PathBuf> {
    let uri = uri.strip_prefix("file://").unwrap_or(uri);
    if uri.contains("://") || uri.starts_with("//") || uri.contains('\0') {
        return None;
    }
    // Decode standard percent escapes in relative links (spaces, UTF-8, etc.).
    let mut bytes = Vec::new();
    let input = uri.as_bytes();
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' && i + 2 < input.len() {
            let hex = std::str::from_utf8(&input[i + 1..i + 3]).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            bytes.push(input[i]);
            i += 1;
        }
    }
    let decoded = String::from_utf8(bytes).ok()?;
    // Other schemes (javascript:, data:, mailto:, etc.) are never local files.
    if decoded.contains(':') || decoded.contains('\0') {
        return None;
    }
    Some(base.join(decoded))
}
fn target(base: &Path, uri: &str) -> Target {
    if uri.starts_with("https://") || uri.starts_with("http://") || uri.starts_with("mailto:") {
        Target::External(uri.into())
    } else {
        local_path(base, uri.split('#').next().unwrap_or(uri))
            .map(Target::Local)
            .unwrap_or(Target::Blocked)
    }
}
fn slug(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                Some(c)
            } else if c.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}
impl Reader {
    fn prepare(&mut self, path: &Path, text: &str) {
        if self.source.as_deref() == Some(text) && self.path.as_deref() == Some(path) {
            return;
        }
        self.source = Some(text.into());
        self.path = Some(path.into());
        let cache = self.cache.get_or_insert_with(CommonMarkCache::default);
        cache.clear_viewers();
        cache.link_hooks_clear();
        self.links.clear();
        self.images.clear();
        #[cfg(test)]
        {
            self.rebuilds += 1;
        }
        let base = path.parent().unwrap_or(Path::new("/"));
        let mut replacements = Vec::new();
        let mut heading: Option<(std::ops::Range<usize>, String, bool)> = None;
        let mut counts = HashMap::<String, usize>::new();
        let mut image: Option<(std::ops::Range<usize>, String, String)> = None;
        for (event, range) in Parser::new_ext(
            text,
            Options::ENABLE_TABLES
                | Options::ENABLE_TASKLISTS
                | Options::ENABLE_STRIKETHROUGH
                | Options::ENABLE_HEADING_ATTRIBUTES,
        )
        .into_offset_iter()
        {
            match event {
                Event::Start(Tag::Heading { id, .. }) => {
                    heading = Some((range, String::new(), id.is_some()))
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Some((range, label, explicit)) = heading.take() {
                        if !explicit {
                            let name = slug(&label);
                            let count = counts.entry(name.clone()).or_default();
                            let id = if *count == 0 {
                                name
                            } else {
                                format!("{name}-{count}")
                            };
                            *count += 1;
                            let block = &text[range.clone()];
                            let line = block.lines().next().unwrap_or(block);
                            let end = range.start + line.trim_end().len();
                            replacements.push((end..end, format!(" {{#{id}}}")));
                        }
                    }
                }
                Event::Start(Tag::Image { dest_url, .. }) => {
                    image = Some((range, dest_url.into_string(), String::new()))
                }
                Event::End(TagEnd::Image) => {
                    if let Some((range, uri, alt)) = image.take() {
                        let replacement = if let Some(local) = local_path(base, &uri) {
                            // Custom loader uses the existing bounded, asynchronous decoder.
                            let uri = format!("disco-image://{}", local.display());
                            self.images.push(uri.clone());
                            format!("![{}](<{uri}>)", alt.replace(']', ""))
                        } else {
                            format!(
                                "[{}: {}]",
                                crate::tr!(
                                    "Зовнішнє зображення не завантажується",
                                    "Remote image is not loaded"
                                ),
                                alt.replace(['[', ']'], "")
                            )
                        };
                        replacements.push((range, replacement));
                    }
                }
                Event::Start(Tag::Link { dest_url, .. }) if !dest_url.starts_with('#') => {
                    let uri = dest_url.into_string();
                    if !self.links.iter().any(|(key, _)| key == &uri) {
                        self.cache.as_mut().unwrap().add_link_hook(&uri);
                        self.links.push((uri.clone(), target(base, &uri)));
                    }
                }
                Event::Text(label) | Event::Code(label) => {
                    if let Some((_, text, _)) = &mut heading {
                        text.push_str(&label);
                    }
                    if let Some((_, _, alt)) = &mut image {
                        alt.push_str(&label);
                    }
                }
                _ => {}
            }
        }
        self.rendered = text.into();
        // Source offsets remain valid when applied from the end.
        replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        for (range, replacement) in replacements {
            self.rendered.replace_range(range, &replacement);
        }
    }
    pub fn reload(&mut self, ctx: &egui::Context) {
        for uri in &self.images {
            ctx.forget_image(uri);
        }
        self.source = None;
    }
    pub fn show(&mut self, ui: &mut egui::Ui, path: &Path, text: &str) -> Option<Target> {
        self.prepare(path, text);
        let loader = install_loader(ui.ctx());
        let font_size = ui.style().text_styles[&egui::TextStyle::Body]
            .size
            .to_bits();
        if self.font_size != Some(font_size) {
            self.cache.as_mut().unwrap().clear_viewers();
            self.font_size = Some(font_size);
        }
        ui.style_mut().visuals.dark_mode = crate::branding::dark(ui.visuals().panel_fill);
        CommonMarkViewer::new()
            .enable_scroll_to_heading(true)
            .max_image_width(Some(ui.available_width().max(1.) as usize))
            .show_scrollable(
                egui::Id::unique(("markdown-reader", path)),
                ui,
                self.cache.as_mut().unwrap(),
                &CommonMarkScrollOptions::default()
                    .viewport_cache(true)
                    .horizontal_scrolling(true),
                &self.rendered,
            );
        let evicted = std::mem::take(&mut *loader.evicted.lock().unwrap());
        for uri in evicted {
            ui.ctx().forget_image(&uri);
        }
        self.links
            .iter()
            .find(|(uri, _)| self.cache.as_ref().unwrap().get_link_hook(uri) == Some(true))
            .map(|(_, target)| target.clone())
    }
}

const LOADER: &str = "disco::markdown::BoundedLocalImages";
#[derive(Default)]
struct LocalImages {
    images: Mutex<HashMap<String, ImageEntry>>,
    evicted: Mutex<Vec<String>>,
}
struct ImageEntry {
    serial: u64,
    job: std::sync::mpsc::Receiver<crate::file_preview::Content>,
    result: Option<ImageLoadResult>,
}
impl ImageLoader for LocalImages {
    fn id(&self) -> &str {
        LOADER
    }
    fn load(&self, ctx: &egui::Context, uri: &str, _: SizeHint) -> ImageLoadResult {
        let Some(path) = uri.strip_prefix("disco-image://") else {
            return Err(LoadError::NotSupported);
        };
        let mut images = self.images.lock().unwrap();
        // Bound both work and retained pixels. A refresh/context reset releases the cache.
        if images.len() >= 16 && !images.contains_key(uri) {
            let oldest = images
                .iter()
                .filter(|(_, entry)| entry.result.is_some())
                .min_by_key(|(_, entry)| entry.serial)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                images.remove(&oldest);
                self.evicted.lock().unwrap().push(oldest);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(25));
                return Ok(ImagePoll::Pending { size: None });
            }
        }
        let serial = ctx.cumulative_pass_nr();
        let entry = images.entry(uri.into()).or_insert_with(|| {
            let (tx, job) = std::sync::mpsc::channel();
            let path = PathBuf::from(path);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                crate::scan::background_priority();
                let content = crate::file_preview::load(&path);
                let _ = tx.send(content);
                ctx.request_repaint();
            });
            ImageEntry {
                serial,
                job,
                result: None,
            }
        });
        if let Some(result) = &entry.result {
            return result.clone();
        }
        if let Ok(content) = entry.job.try_recv() {
            let result = match content {
                crate::file_preview::Content::Image { rgba, size, .. } => Ok(ImagePoll::Ready {
                    image: Arc::new(egui::ColorImage::from_rgba_unmultiplied(size, &rgba)),
                }),
                crate::file_preview::Content::Error(e) => Err(LoadError::Loading(e)),
                _ => Err(LoadError::Loading(
                    crate::tr!(
                        "Зображення відсутнє або завелике",
                        "Image unavailable or too large"
                    )
                    .into(),
                )),
            };
            entry.result = Some(result.clone());
            return result;
        }
        Ok(ImagePoll::Pending { size: None })
    }
    fn forget(&self, uri: &str) {
        self.images.lock().unwrap().remove(uri);
    }
    fn forget_all(&self) {
        self.images.lock().unwrap().clear();
    }
    fn byte_size(&self) -> usize {
        self.images
            .lock()
            .unwrap()
            .values()
            .map(|e| match &e.result {
                Some(Ok(ImagePoll::Ready { image })) => image.pixels.len() * 4,
                _ => 0,
            })
            .sum()
    }
    fn has_pending(&self) -> bool {
        self.images
            .lock()
            .unwrap()
            .values()
            .any(|e| e.result.is_none())
    }
}
fn install_loader(ctx: &egui::Context) -> Arc<LocalImages> {
    let id = egui::Id::unique(LOADER);
    if let Some(loader) = ctx.data(|d| d.get_temp::<Arc<LocalImages>>(id)) {
        return loader;
    }
    let loader = Arc::new(LocalImages::default());
    ctx.add_image_loader(loader.clone());
    ctx.data_mut(|d| d.insert_temp(id, loader.clone()));
    loader
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_images_decode_off_thread_and_never_fetch_remote_uris() {
        let root = std::env::temp_dir().join(format!("disco-md-images-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("sample.png");
        image::RgbImage::from_pixel(30, 20, image::Rgb([15, 50, 80]))
            .save(&path)
            .unwrap();
        let ctx = egui::Context::default();
        let loader = LocalImages::default();
        let uri = format!("disco-image://{}", path.display());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            match loader.load(&ctx, &uri, SizeHint::default()).unwrap() {
                ImagePoll::Ready { image } => {
                    assert_eq!(image.size, [30, 20]);
                    break;
                }
                ImagePoll::Pending { .. } => {
                    assert!(std::time::Instant::now() < deadline);
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        }
        assert!(matches!(
            loader.load(&ctx, "https://example.com/a.png", SizeHint::default()),
            Err(LoadError::NotSupported)
        ));
        assert!(loader.byte_size() > 0);
        loader.forget(&uri);
        assert_eq!(loader.byte_size(), 0);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn large_reader_reuses_prepared_source_and_survives_width_changes() {
        let source=(0..600).map(|i|format!("## Section {i}\n\nA paragraph with **emphasis**, a [link](#section-0), and words that wrap.\n\n")).collect::<String>();
        let mut reader = Reader::default();
        let ctx = egui::Context::default();
        for width in [800., 800., 800., 400., 400., 800.] {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 500.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    reader.show(ui, Path::new("/tmp/large.md"), &source);
                },
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(reader.rebuilds, 1);
    }

    #[test]
    fn resolves_relative_resources_and_builds_anchors_without_reparsing() {
        let path = Path::new("/tmp/docs/README.md");
        let text = "# Hello world\n\n[local](other%20file.md) [web](https://example.com) [bad](javascript:bad)\n\n![picture](images/photo.png) ![external](https://example.com/a.png)\n\n## Hello world\n\n[anchor](#hello-world-1)\n";
        let mut reader = Reader::default();
        reader.prepare(path, text);
        assert!(reader.rendered.contains("# Hello world {#hello-world}"));
        assert!(reader.rendered.contains("## Hello world {#hello-world-1}"));
        assert!(
            reader
                .rendered
                .contains("disco-image:///tmp/docs/images/photo.png")
        );
        assert!(!reader.rendered.contains("![external]"));
        assert!(reader.links.contains(&(
            "other%20file.md".into(),
            Target::Local(PathBuf::from("/tmp/docs/other file.md"))
        )));
        assert!(
            reader
                .links
                .contains(&("javascript:bad".into(), Target::Blocked))
        );
        reader.prepare(path, text);
        assert_eq!(reader.rebuilds, 1);
        reader.prepare(path, "# Changed");
        assert_eq!(reader.rebuilds, 2);
    }
}
