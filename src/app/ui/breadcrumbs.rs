//! Clickable path components, with ancestor overflow instead of broken names.
use crate::{
    app::{Disco, editor::Action, state::View},
    index::Category,
    settings::color,
};
use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Vec2};
use std::path::{Path, PathBuf};

pub(super) fn ancestors(path: &Path) -> Vec<PathBuf> {
    let mut all: Vec<_> = path.ancestors().map(Path::to_path_buf).collect();
    all.reverse();
    all
}
impl Disco {
    pub(in crate::app) fn breadcrumb_path(&mut self, ui: &mut egui::Ui, rect: Rect, path: &Path) {
        if self.address_bar(ui, rect) {
            return;
        }
        let rect = Rect::from_min_max(rect.min, rect.max - Vec2::new(30., 0.));
        let nodes = ancestors(path);
        if nodes.is_empty() || rect.width() < 20. {
            return;
        }
        let t = self.preferences.theme.clone();
        let font = FontId::proportional(14.);
        let labels: Vec<_> = nodes
            .iter()
            .map(|p| {
                p.file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "/".into())
            })
            .collect();
        let widths: Vec<_> = labels
            .iter()
            .map(|s| {
                ui.painter()
                    .layout_no_wrap(s.clone(), font.clone(), color(&t.text))
                    .size()
                    .x
                    + 12.
            })
            .collect();
        let mut first = 0;
        let mut total = widths.iter().sum::<f32>() + (nodes.len().saturating_sub(1) as f32) * 17.;
        while first + 1 < nodes.len() && total > rect.width() - if first > 0 { 42. } else { 0. } {
            total -= widths[first] + 17.;
            first += 1;
        }
        let mut x = rect.left();
        let mut selected = None;
        if first > 0 {
            let more = Rect::from_min_size(Pos2::new(x, rect.top() + 2.), Vec2::new(25., 30.));
            let response = ui
                .interact(
                    more,
                    egui::Id::unique("breadcrumb-ancestors"),
                    Sense::click(),
                )
                .on_hover_text(crate::tr!("Батьківські папки", "Parent folders"));
            if response.hovered() || response.has_focus() {
                ui.painter().rect_filled(more, t.radius, color(&t.raised));
            }
            ui.painter().text(
                more.center(),
                Align2::CENTER_CENTER,
                "…",
                font.clone(),
                color(&t.muted),
            );
            egui::Popup::menu(&response).show(|ui| {
                for node in nodes[..first].iter().rev() {
                    if ui.button(node.display().to_string()).clicked() {
                        selected = Some(node.clone());
                        ui.close();
                    }
                }
            });
            x += 34.;
        }
        for i in first..nodes.len() {
            let width = widths[i].min((rect.right() - x).max(0.));
            if width < 15. {
                break;
            }
            let button = Rect::from_min_size(Pos2::new(x, rect.top() + 2.), Vec2::new(width, 30.));
            let response = ui
                .interact(
                    button,
                    egui::Id::unique(("breadcrumb", &nodes[i])),
                    Sense::click(),
                )
                .on_hover_text(nodes[i].display().to_string());
            if response.hovered() || response.has_focus() {
                ui.painter().rect_filled(button, t.radius, color(&t.raised));
            }
            ui.painter()
                .with_clip_rect(button.shrink2(Vec2::new(3., 0.)))
                .text(
                    button.left_center() + Vec2::new(6., 0.),
                    Align2::LEFT_CENTER,
                    &labels[i],
                    font.clone(),
                    if response.hovered() || response.has_focus() {
                        color(&t.accent)
                    } else {
                        color(&t.text)
                    },
                );
            if response.clicked() {
                selected = Some(nodes[i].clone());
            }
            x += width + 17.;
            if i + 1 < nodes.len() {
                self.art.paint(
                    ui,
                    14,
                    Rect::from_center_size(Pos2::new(x - 8., rect.center().y), Vec2::splat(11.)),
                    color(&t.muted),
                );
            }
        }
        if let Some(path) = selected {
            self.editor_action(Action::Navigate(path), ui.ctx());
        }
    }
    pub(in crate::app) fn navigate_path(&mut self, path: PathBuf) {
        if self.scan.busy || self.deletion.rx.is_some() || self.recycle.busy {
            return;
        }
        let cached = self
            .scan
            .report
            .as_ref()
            .and_then(|r| r.folders.iter().position(|f| f.path == path));
        self.scoped.cancel();
        self.scoped = Default::default();
        self.leave_path_browser();
        self.browser.properties = None;
        self.browser.table_root = None;
        self.browser.selected.clear();
        self.browser.category = Category::All;
        self.browser.view = View::Folders;
        self.browser.filter.clear();
        if let Some(id) = cached {
            self.enter_folder(id);
        } else {
            self.browser.root = path.to_string_lossy().into_owned();
            self.paths.directory = Some(path);
            self.browser.rows_cache = None;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_keep_spaces_and_real_component_boundaries() {
        assert_eq!(
            ancestors(Path::new("/home/user/My Photos")),
            vec![
                PathBuf::from("/"),
                "/home".into(),
                "/home/user".into(),
                "/home/user/My Photos".into()
            ]
        );
    }
}
