//! Native Markdown reader. Parsing is delegated to pulldown/CommonMark;
//! document modes keep the existing editor, undo state and save path intact.
mod details;
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
    depth: u8,
    details: Vec<details::Detail>,
    source: Option<String>,
    path: Option<PathBuf>,
    rendered: String,
    cache: Option<CommonMarkCache>,
    links: Vec<(String, Target)>,
    images: Vec<String>,
    font_size: Option<(u32, u32)>,
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
        let (display, details) = details::extract(text, self.depth);
        self.details = details;
        let text = display.as_str();
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
        for detail in &self.details {
            detail.reader.borrow_mut().reload(ctx);
        }
        for uri in &self.images {
            ctx.forget_image(uri);
        }
        self.source = None;
    }
    pub fn show(&mut self, ui: &mut egui::Ui, path: &Path, text: &str) -> Option<Target> {
        self.show_document(ui, path, text, true)
    }
    fn show_document(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        text: &str,
        scroll: bool,
    ) -> Option<Target> {
        self.prepare(path, text);
        let loader = install_loader(ui.ctx());
        // Global UI 100% uses 16 * 0.85 points; retain UI/file zoom while giving
        // the reader its own typography. Nested disclosures inherit these fonts.
        let scale = ui.style().text_styles[&egui::TextStyle::Body].size / 13.6;
        let px = ui.ctx().pixels_per_point();
        let crisp = |size: f32| (size * scale * px).round().max(1.) / px;
        if !ui
            .style()
            .text_styles
            .contains_key(&egui::TextStyle::Name("Markdown Body".into()))
        {
            let style = ui.style_mut();
            style.visuals.override_text_color = None;
            style.visuals.extreme_bg_color = style.visuals.widgets.noninteractive.bg_fill;
            style.visuals.code_bg_color = style.visuals.extreme_bg_color;
            style.spacing.item_spacing = egui::vec2(4., 4. * scale);
            style.spacing.button_padding = egui::vec2(6., 3.);
            style.text_styles.insert(
                egui::TextStyle::Body,
                egui::FontId::proportional(crisp(14.)),
            );
            style.text_styles.insert(
                egui::TextStyle::Monospace,
                egui::FontId::monospace(crisp(13.)),
            );
            style.text_styles.insert(
                egui::TextStyle::Name("Markdown Body".into()),
                egui::FontId::proportional(crisp(14.)),
            );
            style.text_styles.insert(
                egui::TextStyle::Name("Markdown Bold".into()),
                crate::art::bold(crisp(14.)),
            );
            for (i, size) in [28., 22., 19., 17., 15., 14.].into_iter().enumerate() {
                style.text_styles.insert(
                    egui::TextStyle::Name(format!("Markdown H{}", i + 1).into()),
                    crate::art::bold(crisp(size)),
                );
            }
        }
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::unique("disco-markdown-image-label"),
                crate::tr!("Зображення недоступне", "Image unavailable").to_string(),
            );
            d.insert_temp(
                egui::Id::unique("disco-markdown-copy-label"),
                crate::tr!("Копіювати код", "Copy code").to_string(),
            )
        });
        let font_size = (
            ui.style().text_styles[&egui::TextStyle::Body]
                .size
                .to_bits(),
            px.to_bits(),
        );
        if self.font_size != Some(font_size) {
            self.cache.as_mut().unwrap().clear_viewers();
            self.font_size = Some(font_size);
        }
        ui.style_mut().visuals.dark_mode = crate::branding::dark(ui.visuals().panel_fill);
        let nested_target = std::cell::RefCell::new(None);
        let disclosure_changed = std::cell::Cell::new(false);
        let details = &self.details;
        let html = |ui: &mut egui::Ui, raw: &str| {
            let index = raw
                .trim()
                .strip_prefix("<div data-disco-details=\"")
                .and_then(|s| s.split('"').next())
                .and_then(|s| s.parse::<usize>().ok());
            if let Some(detail) = index.and_then(|i| details.get(i)) {
                ui.end_row();
                let response = ui
                    .vertical(|ui| {
                        egui::CollapsingHeader::new(&detail.title)
                            .id_salt(("markdown-details", path, index))
                            .open(Some(detail.expanded.get()))
                            .show(ui, |ui| {
                                if let Some(target) = detail.reader.borrow_mut().show_document(
                                    ui,
                                    path,
                                    &detail.source,
                                    false,
                                ) {
                                    *nested_target.borrow_mut() = Some(target);
                                }
                            })
                    })
                    .inner;
                ui.end_row();
                ui.interact(
                    response.header_response.rect,
                    egui::Id::unique(("markdown-details-toggle", path, index.unwrap())),
                    egui::Sense::hover(),
                );
                if response.header_response.clicked() {
                    detail.expanded.set(!detail.expanded.get());
                    disclosure_changed.set(true);
                }
            } else {
                ui.label(raw);
            }
        };
        let viewer = CommonMarkViewer::new()
            .render_html_fn(Some(&html))
            .syntax_theme_light("InspiredGitHub")
            .enable_scroll_to_heading(true)
            .max_image_width(Some((ui.available_width() - 48. * scale).max(1.) as usize));
        let id = egui::Id::unique(("markdown-reader", path));
        if scroll {
            viewer.show_scrollable(
                id,
                ui,
                self.cache.as_mut().unwrap(),
                &CommonMarkScrollOptions::default()
                    .viewport_cache(true)
                    .horizontal_scrolling(false)
                    .content_margin(egui::Margin::symmetric(
                        if ui.available_width() < 400. { 12 } else { 24 },
                        18,
                    )),
                &self.rendered,
            );
        } else {
            viewer.show_with_id(id, ui, self.cache.as_mut().unwrap(), &self.rendered);
        }
        if disclosure_changed.get() {
            self.cache.as_mut().unwrap().clear_viewers();
            ui.ctx().request_repaint();
        }
        let evicted = std::mem::take(&mut *loader.evicted.lock().unwrap());
        for uri in evicted {
            ui.ctx().forget_image(&uri);
        }
        nested_target.into_inner().or_else(|| {
            self.links
                .iter()
                .find(|(uri, _)| self.cache.as_ref().unwrap().get_link_hook(uri) == Some(true))
                .map(|(_, target)| target.clone())
        })
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

#[cfg(test)]
mod code_widget_tests {
    use eframe::egui::{self, FontId, Pos2, Rect, Vec2};
    #[test]
    fn tall_code_uses_the_document_scroll_and_vector_copy_copies_all_lines() {
        let source = (0..140)
            .map(|i| format!("line {i}: hello world\n"))
            .collect::<String>();
        let ctx = egui::Context::default();
        let run = |events| {
            let out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640., 260.))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let scroll = egui::ScrollArea::vertical().show(ui, |ui| {
                        let mut layout =
                            |ui: &egui::Ui, text: &dyn egui::TextBuffer, width: f32| {
                                let mut job = egui::text::LayoutJob::simple(
                                    text.as_str().into(),
                                    FontId::monospace(13.),
                                    egui::Color32::WHITE,
                                    width,
                                );
                                for s in &mut job.sections {
                                    s.format.line_height = Some(20.);
                                }
                                ui.fonts_mut(|f| f.layout_job(job))
                            };
                        let (_, galley) = egui_commonmark_backend::elements::code_block(
                            ui,
                            600.,
                            &source,
                            &mut layout,
                            None,
                        );
                        assert_eq!(galley.rows.len(), 140);
                        assert!(galley.size().y > 2700., "code must retain its full height");
                    });
                    assert!(scroll.content_size.y > 2700.);
                },
            );
            out
        };
        let first = run(vec![]);
        let mut text_shapes = Vec::new();
        fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(t) => out.push(t.galley.text().into()),
                egui::epaint::Shape::Vec(v) => {
                    for s in v {
                        collect(s, out)
                    }
                }
                _ => {}
            }
        }
        for s in &first.shapes {
            collect(&s.shape, &mut text_shapes);
        }
        assert!(
            !text_shapes
                .iter()
                .any(|s| s.contains('🗐') || s.contains('✔'))
        );
        first.drop_without_applying_deltas();
        // The copy control stays at the code block's top-right, not at its far bottom.
        let pos = Pos2::new(600. - 18., 17.);
        run(vec![egui::Event::PointerMoved(pos)]).drop_without_applying_deltas();
        run(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }])
        .drop_without_applying_deltas();
        let out = run(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(out.platform_output.commands.iter().any(|cmd| matches!(cmd,egui::OutputCommand::CopyText(text) if text==source.trim_end_matches('\n'))));
        out.drop_without_applying_deltas();
    }
}
