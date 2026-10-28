//! The two file layouts have one fixed, labeled switch in the title bar.
use crate::{
    app::{Disco, state::View},
    settings::{PreviewMode, color},
};
use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Stroke, Vec2};
impl Disco {
    pub(in crate::app) fn file_mode_buttons(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let t = self.preferences.theme.clone();
        for (offset, width, mode, icon, label, help, id) in [
            (
                0.,
                108.,
                PreviewMode::Overview,
                25,
                crate::tr!("Огляд", "Overview"),
                crate::tr!(
                    "Огляд: список, прев’ю та властивості",
                    "Overview: list, preview and properties"
                ),
                "file-mode-overview",
            ),
            (
                112.,
                78.,
                PreviewMode::Focus,
                26,
                crate::tr!("Фокус", "Focus"),
                crate::tr!(
                    "Фокус: дерево папок ліворуч, документ праворуч",
                    "Focus: folder tree on the left, document on the right"
                ),
                "file-mode-focus",
            ),
        ] {
            let r = Rect::from_min_size(rect.min + Vec2::new(offset, 0.), Vec2::new(width, 30.));
            let response = ui
                .interact(r, egui::Id::unique(id), Sense::click())
                .on_hover_text(help);
            let active =
                self.browser.view != View::Trash && self.preferences.layout.preview_mode == mode;
            if active || response.hovered() {
                ui.painter().rect_filled(r, t.radius, color(&t.raised));
            }
            ui.painter().rect_stroke(
                r,
                t.radius,
                Stroke::new(1., color(if active { &t.accent } else { &t.border })),
                egui::StrokeKind::Inside,
            );
            self.art.paint(
                ui,
                icon,
                Rect::from_center_size(Pos2::new(r.left() + 17., r.center().y), Vec2::splat(18.)),
                color(if active { &t.text } else { &t.muted }),
            );
            ui.painter().text(
                Pos2::new(r.left() + 33., r.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(13.),
                color(&t.text),
            );
            if response.clicked() {
                if self.browser.view == View::Trash {
                    self.browser.view = View::Folders;
                    self.browser.properties = None;
                }
                self.set_preview_mode(mode, ui.ctx());
            }
        }
    }
}
