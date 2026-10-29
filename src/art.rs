use eframe::egui::{self, Color32, Pos2, Rect, Stroke, TextureHandle, TextureOptions};
use std::{cell::RefCell, collections::HashMap};

const BUNDLED_ICONS: usize = 59;

// Parse the embedded vectors once. Raster textures are generated only for physical
// sizes actually used by the UI, and retained in a bounded cache (never per frame).
pub struct Art {
    icons: Vec<resvg::usvg::Tree>,
    custom: HashMap<String, Result<usize, String>>,
    bitmaps: HashMap<usize, image::RgbaImage>,
    cache: RefCell<HashMap<(usize, u32, u32), TextureHandle>>,
}
impl Art {
    pub fn load(_ctx: &egui::Context) -> Self {
        let sources: [&[u8]; BUNDLED_ICONS] = [
            include_bytes!("../assets/icons/folder.svg"),
            include_bytes!("../assets/icons/game.svg"),
            include_bytes!("../assets/icons/code.svg"),
            include_bytes!("../assets/icons/cache.svg"),
            include_bytes!("../assets/icons/document.svg"),
            include_bytes!("../assets/icons/other.svg"),
            include_bytes!("../assets/icons/monitor.svg"),
            include_bytes!("../assets/icons/gear.svg"),
            include_bytes!("../assets/icons/drive.svg"),
            include_bytes!("../assets/icons/search.svg"),
            include_bytes!("../assets/icons/open.svg"),
            include_bytes!("../assets/icons/refresh.svg"),
            include_bytes!("../assets/icons/home.svg"),
            include_bytes!("../assets/icons/back.svg"),
            include_bytes!("../assets/icons/chevron.svg"),
            include_bytes!("../assets/icons/upload.svg"),
            include_bytes!("../assets/icons/reset.svg"),
            include_bytes!("../assets/icons/info.svg"),
            include_bytes!("../assets/icons/count.svg"),
            include_bytes!("../assets/icons/minimize.svg"),
            include_bytes!("../assets/icons/maximize.svg"),
            include_bytes!("../assets/icons/close.svg"),
            include_bytes!("../assets/icons/forward.svg"),
            include_bytes!("../assets/icons/trash.svg"),
            include_bytes!("../assets/icons/image.svg"),
            include_bytes!("../assets/icons/preview-side.svg"),
            include_bytes!("../assets/icons/preview-split.svg"),
            include_bytes!("../assets/icons/copy.svg"),
            include_bytes!("../assets/icons/save.svg"),
            include_bytes!("../assets/icons/robot.svg"),
            include_bytes!("../assets/icons/plus.svg"),
            include_bytes!("../assets/icons/music.svg"),
            include_bytes!("../assets/icons/video.svg"),
            include_bytes!("../assets/icons/camera.svg"),
            include_bytes!("../assets/icons/archive.svg"),
            include_bytes!("../assets/icons/cloud.svg"),
            include_bytes!("../assets/icons/download.svg"),
            include_bytes!("../assets/icons/heart.svg"),
            include_bytes!("../assets/icons/star.svg"),
            include_bytes!("../assets/icons/briefcase.svg"),
            include_bytes!("../assets/icons/book.svg"),
            include_bytes!("../assets/icons/package.svg"),
            include_bytes!("../assets/icons/film.svg"),
            include_bytes!("../assets/icons/brush.svg"),
            include_bytes!("../assets/icons/palette.svg"),
            include_bytes!("../assets/icons/headphones.svg"),
            include_bytes!("../assets/icons/network.svg"),
            include_bytes!("../assets/icons/database.svg"),
            include_bytes!("../assets/icons/chip.svg"),
            include_bytes!("../assets/icons/terminal.svg"),
            include_bytes!("../assets/icons/git.svg"),
            include_bytes!("../assets/icons/lock.svg"),
            include_bytes!("../assets/icons/shield.svg"),
            include_bytes!("../assets/icons/folder-star.svg"),
            include_bytes!("../assets/icons/folder-code.svg"),
            include_bytes!("../assets/icons/folder-photo.svg"),
            include_bytes!("../assets/icons/coffee.svg"),
            include_bytes!("../assets/icons/bolt.svg"),
            include_bytes!("../assets/icons/globe.svg"),
        ];
        Self {
            icons: sources
                .into_iter()
                .map(|s| {
                    resvg::usvg::Tree::from_data(s, &resvg::usvg::Options::default())
                        .expect("bundled SVG")
                })
                .collect(),
            custom: HashMap::new(),
            bitmaps: HashMap::new(),
            cache: RefCell::new(HashMap::new()),
        }
    }
    pub fn category_icon(&mut self, name: &str) -> usize {
        if let Some((_, index)) = category_icons().iter().find(|(n, _)| *n == name) {
            return *index;
        }
        if let Some(result) = self.custom.get(name) {
            return result.clone().unwrap_or(0);
        }
        if self.custom.len() >= 128 {
            self.icons.truncate(BUNDLED_ICONS);
            self.custom.clear();
            self.bitmaps.clear();
            self.cache.borrow_mut().clear();
        }
        let result = self.load_custom(name);
        let index = result.clone().unwrap_or(0);
        self.custom.insert(name.into(), result);
        index
    }
    pub fn reload_custom_failures(&mut self) {
        self.custom.retain(|_, result| result.is_ok());
    }
    pub fn custom_error(&self, name: &str) -> Option<&str> {
        self.custom
            .get(name)
            .and_then(|r| r.as_ref().err())
            .map(String::as_str)
    }
    fn load_custom(&mut self, name: &str) -> Result<usize, String> {
        if !crate::categories::valid_icon_path(name) {
            return Err("Invalid icon path".into());
        }
        // Content-addressed assets are immutable; only the configuration is reloaded.
        let bytes = crate::categories::read_bounded(
            &crate::settings::config_dir().join(name),
            2 * 1024 * 1024,
        )?;
        let index = self.icons.len();
        if name.ends_with(".svg") {
            self.icons.push(parse_custom_svg(&bytes)?);
        } else {
            let bitmap = decode_bitmap(&bytes)?;
            self.icons.push(self.icons[0].clone());
            self.bitmaps.insert(index, bitmap);
        }
        Ok(index)
    }
    pub fn len(&self) -> usize {
        self.icons.len()
    }
    pub fn paint(&self, ui: &egui::Ui, index: usize, rect: Rect, tint: Color32) {
        let ppp = ui.ctx().pixels_per_point();
        if (19..=21).contains(&index) {
            // Window controls stay vector geometry, with a full physical pixel
            // stroke at 1x. No texture minification can erase the minimize line.
            let physical_stroke = ppp.round().max(1.);
            let snap =
                |v: f32| ((v * ppp - physical_stroke * 0.5).round() + physical_stroke * 0.5) / ppp;
            let center = Pos2::new(snap(rect.center().x), snap(rect.center().y));
            let half = ((rect.width() * 0.33 * ppp).round() / ppp).max(3.);
            let stroke = Stroke::new(physical_stroke / ppp, tint);
            let painter = ui.painter();
            match index {
                19 => {
                    painter.line_segment(
                        [center - egui::vec2(half, 0.), center + egui::vec2(half, 0.)],
                        stroke,
                    );
                }
                20 => {
                    painter.rect_stroke(
                        Rect::from_center_size(center, egui::Vec2::splat(half * 2.)),
                        0,
                        stroke,
                        egui::StrokeKind::Middle,
                    );
                }
                _ => {
                    painter.line_segment(
                        [
                            center - egui::vec2(half, half),
                            center + egui::vec2(half, half),
                        ],
                        stroke,
                    );
                    painter.line_segment(
                        [
                            center + egui::vec2(-half, half),
                            center + egui::vec2(half, -half),
                        ],
                        stroke,
                    );
                }
            }
            return;
        }
        let w = (rect.width() * ppp).round().clamp(1., 512.) as u32;
        let h = (rect.height() * ppp).round().clamp(1., 512.) as u32;
        let key = (index, w, h);
        let texture = {
            let mut cache = self.cache.borrow_mut();
            if !cache.contains_key(&key) {
                if cache.len() >= 96 {
                    cache.clear();
                }
                let pixels = if let Some(bitmap) = self.bitmaps.get(&index) {
                    let rgba = image::imageops::resize(
                        bitmap,
                        w,
                        h,
                        image::imageops::FilterType::Triangle,
                    );
                    egui::ColorImage::from_rgba_unmultiplied(
                        [w as usize, h as usize],
                        rgba.as_raw(),
                    )
                } else {
                    rasterize(&self.icons[index], w, h)
                };
                cache.insert(
                    key,
                    ui.ctx().load_texture(
                        format!("svg-{index}-{w}x{h}"),
                        pixels,
                        TextureOptions::LINEAR,
                    ),
                );
            }
            cache[&key].clone()
        };
        // Align the texture to device pixels; fractional origins otherwise blur
        // even correctly sized artwork on a screen with fractional scaling.
        let min = egui::pos2(
            (rect.min.x * ppp).round() / ppp,
            (rect.min.y * ppp).round() / ppp,
        );
        let rect = Rect::from_min_size(min, egui::vec2(w as f32 / ppp, h as f32 / ppp));
        ui.painter().image(
            texture.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
            tint,
        );
    }
}
fn rasterize(tree: &resvg::usvg::Tree, w: u32, h: u32) -> egui::ColorImage {
    // Supersampling averages vector coverage before downsampling. In particular,
    // subpixel strokes survive instead of disappearing between texture samples.
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w * 2, h * 2).expect("icon size");
    let size = tree.size();
    resvg::render(
        tree,
        resvg::tiny_skia::Transform::from_scale(
            w as f32 * 2. / size.width(),
            h as f32 * 2. / size.height(),
        ),
        &mut pixmap.as_mut(),
    );
    let rgba = image::RgbaImage::from_raw(w * 2, h * 2, pixmap.data().to_vec()).expect("RGBA");
    let reduced = image::imageops::resize(&rgba, w, h, image::imageops::FilterType::Triangle);
    egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], reduced.as_raw())
}

pub fn app_icon() -> egui::IconData {
    let tree = resvg::usvg::Tree::from_data(
        include_bytes!("../assets/disco.svg"),
        &resvg::usvg::Options::default(),
    )
    .expect("application SVG");
    let pixels = rasterize(&tree, 256, 256);
    egui::IconData {
        width: 256,
        height: 256,
        rgba: pixels
            .pixels
            .iter()
            .flat_map(Color32::to_srgba_unmultiplied)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_svg_renders_at_real_display_sizes() {
        let art = Art::load(&egui::Context::default());
        for icon in &art.icons {
            for size in [9, 15, 19, 24, 30, 38, 60, 112, 224] {
                let image = rasterize(icon, size, size);
                assert_eq!(image.size, [size as usize; 2]);
                assert!(
                    image.pixels.iter().any(|c| c.a() > 40),
                    "empty icon at {size}px"
                );
                assert!(
                    image.pixels.iter().any(|c| c.a() > 0 && c.a() < 255),
                    "missing antialiasing"
                );
                assert!(
                    image
                        .pixels
                        .iter()
                        .all(|c| c.r() <= c.a() && c.g() <= c.a() && c.b() <= c.a())
                );
            }
        }
    }
}
pub fn bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("Inter Bold".into()))
}
pub fn fonts(ctx: &egui::Context) {
    install_fonts(ctx, false);
}

pub fn terminal_fonts(ctx: &egui::Context) {
    install_fonts(ctx, true);
}

fn install_fonts(ctx: &egui::Context, terminal: bool) {
    let mut f = egui::FontDefinitions::default();
    f.font_data.insert(
        "Inter".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf")).into(),
    );
    f.font_data.insert(
        "Inter Bold".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Bold.ttf")).into(),
    );
    f.families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "Inter".into());
    f.families.insert(
        egui::FontFamily::Name("Inter Bold".into()),
        vec!["Inter Bold".into(), "Inter".into()],
    );
    if terminal {
        let mut names = Vec::new();
        for (index, pattern) in ["monospace", ":charset=1fb00", "Symbols Nerd Font Mono"]
            .iter()
            .enumerate()
        {
            if let Ok(out) = std::process::Command::new("fc-match")
                .args(["-f", "%{file}", pattern])
                .output()
            {
                if !out.status.success() {
                    continue;
                }
                if let Ok(bytes) = std::fs::read(String::from_utf8_lossy(&out.stdout).trim()) {
                    let name = format!("terminal-system-{index}");
                    f.font_data
                        .insert(name.clone(), egui::FontData::from_owned(bytes).into());
                    names.push(name);
                }
            }
        }
        names.extend(
            f.families
                .get(&egui::FontFamily::Monospace)
                .cloned()
                .unwrap_or_default(),
        );
        f.families
            .insert(egui::FontFamily::Name("Terminal".into()), names);
    }
    ctx.set_fonts(f);
}

/// Select artwork without reading or decoding the file on the UI thread.
pub(crate) fn file_icon(path: &std::path::Path) -> usize {
    match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff" | "gif" | "svg" | "ico" => 24,
        _ => 4,
    }
}

/// Stable names used in categories.toml; indices are internal to the renderer.
pub fn category_icons() -> &'static [(&'static str, usize)] {
    &[
        ("folder", 0),
        ("game", 1),
        ("code", 2),
        ("cache", 3),
        ("document", 4),
        ("other", 5),
        ("monitor", 6),
        ("drive", 8),
        ("home", 12),
        ("image", 24),
        ("music", 31),
        ("video", 32),
        ("camera", 33),
        ("archive", 34),
        ("cloud", 35),
        ("download", 36),
        ("heart", 37),
        ("star", 38),
        ("briefcase", 39),
        ("book", 40),
        ("package", 41),
        ("film", 42),
        ("brush", 43),
        ("palette", 44),
        ("headphones", 45),
        ("network", 46),
        ("database", 47),
        ("chip", 48),
        ("terminal", 49),
        ("git", 50),
        ("lock", 51),
        ("shield", 52),
        ("folder-star", 53),
        ("folder-code", 54),
        ("folder-photo", 55),
        ("coffee", 56),
        ("bolt", 57),
        ("globe", 58),
    ]
}

fn parse_custom_svg(bytes: &[u8]) -> Result<resvg::usvg::Tree, String> {
    let mut options = resvg::usvg::Options::default();
    // Imported icons cannot resolve files, URLs or embedded image resources.
    options.image_href_resolver.resolve_data = Box::new(|_, _, _| None);
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    let tree = resvg::usvg::Tree::from_data(bytes, &options).map_err(|e| e.to_string())?;
    if tree.size().width() > 4096. || tree.size().height() > 4096. {
        return Err("SVG dimensions exceed 4096 pixels".into());
    }
    Ok(tree)
}
fn decode_bitmap(bytes: &[u8]) -> Result<image::RgbaImage, String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| e.to_string())?
        .thumbnail(256, 256)
        .to_rgba8();
    let mut square = image::RgbaImage::new(256, 256);
    image::imageops::overlay(
        &mut square,
        &image,
        ((256 - image.width()) / 2) as i64,
        ((256 - image.height()) / 2) as i64,
    );
    Ok(square)
}
pub fn import_icon(path: &std::path::Path) -> Result<String, String> {
    let bytes = crate::categories::read_bounded(path, 2 * 1024 * 1024)?;
    let (bytes, extension) = if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
    {
        parse_custom_svg(&bytes)?;
        (bytes, "svg")
    } else {
        let image = decode_bitmap(&bytes)?;
        let mut out = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut out, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        (out.into_inner(), "png")
    };
    let filename = format!("icons/{}.{}", blake3::hash(&bytes).to_hex(), extension);
    let target = crate::settings::config_dir().join(&filename);
    std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
    use std::io::Write;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
    {
        Ok(mut file) => {
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if std::fs::symlink_metadata(&target)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
                || crate::categories::read_bounded(&target, 2 * 1024 * 1024)? != bytes
            {
                return Err("Icon asset conflicts with an existing file".into());
            }
        }
        Err(error) => return Err(error.to_string()),
    }
    Ok(filename)
}
