//! Bottom-docked diagnostics for currently open LSP documents.
use crate::{app::Disco, lsp::protocol::Location, settings::color};
use eframe::egui::{self, Align2, FontId, Id, Pos2, Rect, Sense, Stroke, Vec2};

fn triangle(ui: &egui::Ui, center: Pos2, ink: egui::Color32) {
    ui.painter().add(egui::Shape::closed_line(
        vec![
            center + Vec2::new(0., -7.),
            center + Vec2::new(8., 7.),
            center + Vec2::new(-8., 7.),
        ],
        Stroke::new(1.3, ink),
    ));
    ui.painter().line_segment(
        [center + Vec2::new(0., -2.5), center + Vec2::new(0., 2.)],
        Stroke::new(1.5, ink),
    );
    ui.painter()
        .circle_filled(center + Vec2::new(0., 4.5), 0.9, ink);
}
impl Disco {
    pub(super) fn problems_button(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let t = &self.preferences.theme;
        let count: usize = self.languages.problems.values().map(Vec::len).sum();
        let response = ui.interact(rect, Id::unique("toggle-problems"), Sense::click());

        if response.hovered() || self.languages.problems_open {
            ui.painter().rect_filled(rect, t.radius, color(&t.raised));
        }
        let ink = color(if count > 0 { &t.accent } else { &t.muted });
        triangle(ui, egui::pos2(rect.left() + 13., rect.center().y), ink);
        ui.painter().text(
            rect.left_center() + Vec2::new(27., 0.),
            Align2::LEFT_CENTER,
            if count > 999 {
                "999+".into()
            } else {
                count.to_string()
            },
            FontId::proportional(11.),
            ink,
        );
        if self.languages.problems_open {
            ui.painter().hline(
                rect.x_range(),
                rect.bottom(),
                Stroke::new(1., color(&t.accent)),
            );
        }
        if response
            .on_hover_text(crate::tr!(
                "Проблеми мовних серверів · {count}",
                "Language server problems · {count}",
                count = count
            ))
            .clicked()
        {
            self.languages.problems_open = !self.languages.problems_open;
            ui.ctx().request_repaint();
        }
    }
    pub(in crate::app) fn problems_panel(&mut self, ui: &mut egui::Ui) {
        if !self.languages.problems_open {
            return;
        }
        let t = self.preferences.theme.clone();
        let mut close = false;
        let mut target = None;
        let rows: Vec<_> = self
            .languages
            .problems
            .iter()
            .flat_map(|(path, items)| items.iter().map(move |item| (path, item)))
            .collect();
        let count = rows.len();
        let errors = rows.iter().filter(|(_, d)| d.severity == 1).count();
        let warnings = rows.iter().filter(|(_, d)| d.severity == 2).count();
        let panel = egui::Panel::bottom("problems-panel")
            .default_size(210.)
            .min_size(110.)
            .max_size((ui.available_height() * 0.5).max(110.))
            .resizable(true)
            .frame(
                egui::Frame::new()
                    .fill(color(&t.panel))
                    .inner_margin(egui::Margin::symmetric(12, 6)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(crate::tr!("Проблеми", "Problems"));
                    ui.label(egui::RichText::new(count.to_string()).small().weak());
                    ui.label(
                        egui::RichText::new(crate::tr!(
                            "Помилки: {errors} · попередження: {warnings}",
                            "Errors: {errors} · warnings: {warnings}",
                            errors = errors,
                            warnings = warnings
                        ))
                        .small()
                        .weak(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(24.), Sense::hover());
                        let response =
                            ui.interact(rect, Id::unique("close-problems"), Sense::click());
                        self.art.paint(ui, 21, rect.shrink(5.), color(&t.muted));
                        close = response
                            .on_hover_text(crate::tr!(
                                "Сховати панель проблем",
                                "Hide problems panel"
                            ))
                            .clicked();
                        ui.small(crate::tr!("Відкриті файли", "Open files"));
                    });
                });
                ui.separator();
                if rows.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(12.);
                        ui.label(crate::tr!(
                            "Немає діагностик для відкритих файлів.",
                            "No diagnostics for open files."
                        ));
                        if self.languages.sessions.is_empty() {
                            ui.small(crate::tr!(
                                "Увімкни мовний сервер у налаштуваннях і відкрий файл коду.",
                                "Enable a language server in Settings and open a source file."
                            ));
                        }
                    });
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt("problems-list")
                    .auto_shrink([false, false])
                    .show_rows(ui, 26., rows.len(), |ui, visible| {
                        for n in visible {
                            let (path, diagnostic) = rows[n];
                            let (rect, _) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 26.),
                                Sense::hover(),
                            );
                            let response =
                                ui.interact(rect, Id::unique(("problem-row", n)), Sense::click());
                            if response.hovered() || response.has_focus() {
                                ui.painter().rect_filled(rect, t.radius, color(&t.raised));
                            }
                            let ink = if diagnostic.severity == 1 {
                                color(&t.accent)
                            } else {
                                color(&t.muted)
                            };
                            if diagnostic.severity == 1 {
                                let at = rect.left_center() + Vec2::new(11., 0.);
                                ui.painter().circle_stroke(at, 6., Stroke::new(1.3, ink));
                                for flip in [-1., 1.] {
                                    ui.painter().line_segment(
                                        [
                                            at + Vec2::new(-2.5, flip * 2.5),
                                            at + Vec2::new(2.5, -flip * 2.5),
                                        ],
                                        Stroke::new(1.2, ink),
                                    );
                                }
                            } else {
                                triangle(ui, rect.left_center() + Vec2::new(11., 0.), ink);
                            }
                            let file_width = (rect.width() * 0.28).clamp(120., 280.);
                            let file = Rect::from_min_max(
                                Pos2::new(rect.right() - file_width, rect.top()),
                                rect.max,
                            );
                            super::widgets::row_text(
                                ui,
                                Rect::from_min_max(
                                    rect.min + Vec2::new(28., 0.),
                                    Pos2::new(file.left() - 12., rect.bottom()),
                                ),
                                diagnostic
                                    .message
                                    .lines()
                                    .next()
                                    .unwrap_or_default()
                                    .to_owned(),
                                FontId::proportional(12.),
                                color(&t.text),
                            );
                            super::widgets::row_text(
                                ui,
                                file,
                                format!(
                                    "{}:{}:{}",
                                    path.file_name().unwrap_or_default().to_string_lossy(),
                                    diagnostic.range.start.line.saturating_add(1),
                                    diagnostic.range.start.character.saturating_add(1)
                                ),
                                FontId::monospace(11.),
                                color(&t.muted),
                            );
                            let activate = response.clicked()
                                || response.has_focus()
                                    && ui.input_mut(|i| {
                                        i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                                    });
                            response.on_hover_text(format!(
                                "{}\n{}",
                                path.display(),
                                diagnostic.message
                            ));
                            if activate {
                                target = Some(Location {
                                    path: path.clone(),
                                    range: diagnostic.range,
                                });
                            }
                        }
                    });
            });
        ui.interact(
            panel.response.rect,
            Id::unique("problems-panel-bounds"),
            Sense::hover(),
        );
        if close {
            self.languages.problems_open = false;
            ui.ctx().request_repaint();
        }
        if let Some(target) = target {
            self.open_code_location(target, ui.ctx());
        }
    }
}
