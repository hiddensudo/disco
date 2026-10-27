use eframe::egui::{self, Color32, Pos2, Rect, Stroke, TextureHandle, TextureOptions};
use std::{cell::RefCell, collections::HashMap};

// Parse the embedded vectors once. Raster textures are generated only for physical
// sizes actually used by the UI, and retained in a bounded cache (never per frame).
pub struct Art {
    icons: Vec<resvg::usvg::Tree>,
    cache: RefCell<HashMap<(usize, u32, u32), TextureHandle>>,
}
impl Art {
    pub fn load(_ctx: &egui::Context) -> Self {
        let sources: [&[u8]; 25] = [
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
        ];
        Self {
            icons: sources
                .into_iter()
                .map(|s| {
                    resvg::usvg::Tree::from_data(s, &resvg::usvg::Options::default())
                        .expect("bundled SVG")
                })
                .collect(),
            cache: RefCell::new(HashMap::new()),
        }
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
                let pixels = rasterize(&self.icons[index], w, h);
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
