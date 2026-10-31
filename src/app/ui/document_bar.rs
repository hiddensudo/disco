//! One row of document actions; zoom belongs to the bottom status bar.
use super::file_preview::command;
use crate::{app::Disco, file_preview::Content, settings::color};
use eframe::egui::{self, Align2, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
impl Disco {
    pub(super) fn document_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        width: f32,
        markdown: bool,
        read_only: bool,
    ) -> bool {
        let t = self.preferences.theme.clone();
        let (bar, _) = ui.allocate_exact_size(Vec2::new(width, 30.), Sense::hover());
        ui.interact(bar, egui::Id::unique("document-toolbar"), Sense::hover());
        let refresh_rect =
            Rect::from_min_size(Pos2::new(bar.right() - 30., bar.top()), Vec2::splat(30.));
        let reload = command(
            ui,
            &self.art,
            refresh_rect,
            11,
            "",
            self.editor.saving.is_none() && self.editor.untitled.is_none(),
            false,
            &t,
        )
        .on_hover_text(crate::tr!("Оновити прев’ю", "Reload preview"));
        ui.interact(
            reload.rect,
            egui::Id::unique("document-reload"),
            Sense::hover(),
        );
        if matches!(self.file_preview.content, Some(Content::Text { .. })) {
            let labels = width >= 480.;
            let mut x = bar.left();
            if self.editor.document.is_some() {
                let w = if labels { 96. } else { 30. };
                let save = command(
                    ui,
                    &self.art,
                    Rect::from_min_size(Pos2::new(x, bar.top()), Vec2::new(w, 30.)),
                    28,
                    if labels {
                        crate::tr!("Зберегти", "Save")
                    } else {
                        ""
                    },
                    (self.editor.dirty() || self.editor.untitled.is_some())
                        && self.editor.saving.is_none()
                        && !self.demo
                        && !read_only,
                    false,
                    &t,
                )
                .on_hover_text(crate::tr!(
                    "Зберегти зміни · Ctrl+S",
                    "Save changes · Ctrl+S"
                ));
                ui.interact(save.rect, egui::Id::unique("document-save"), Sense::hover());
                if save.clicked() {
                    self.save_editor(ui.ctx());
                }
                x += w + 6.;
            }
            let copy = command(
                ui,
                &self.art,
                Rect::from_min_size(
                    Pos2::new(x, bar.top()),
                    Vec2::new(if labels { 100. } else { 30. }, 30.),
                ),
                27,
                if labels {
                    crate::tr!("Копіювати", "Copy")
                } else {
                    ""
                },
                true,
                false,
                &t,
            )
            .on_hover_text(crate::tr!("Копіювати текст", "Copy text"));
            ui.interact(copy.rect, egui::Id::unique("document-copy"), Sense::hover());
            if copy.clicked() {
                if let Some(doc) = &self.editor.document {
                    ui.ctx().copy_text(doc.text.clone());
                } else if let Some(Content::Text { lines, .. }) = &self.file_preview.content {
                    ui.ctx().copy_text(lines.join("\n"));
                }
            }
            if markdown {
                let mode_width = if labels { 224. } else { 100. };
                let group = Rect::from_min_size(
                    Pos2::new(refresh_rect.left() - mode_width - 12., bar.top()),
                    Vec2::new(mode_width, 30.),
                );
                ui.painter().rect_filled(group, t.radius, color(&t.panel));
                ui.painter().rect_stroke(
                    group,
                    t.radius,
                    Stroke::new(1., color(&t.border)),
                    egui::StrokeKind::Inside,
                );
                for (n, editing, label, help, id) in [
                    (
                        0,
                        false,
                        if labels {
                            crate::tr!("Читання", "Read")
                        } else {
                            crate::tr!("Чит.", "Read")
                        },
                        crate::tr!("Читання Markdown", "Read Markdown"),
                        "markdown-read",
                    ),
                    (
                        1,
                        true,
                        if labels {
                            crate::tr!("Редагування", "Edit")
                        } else {
                            crate::tr!("Ред.", "Edit")
                        },
                        crate::tr!("Редагування Markdown", "Edit Markdown"),
                        "markdown-edit",
                    ),
                ] {
                    let w = (mode_width - 4.) / 2.;
                    let rect = Rect::from_min_size(
                        group.min + Vec2::new(2. + n as f32 * w, 2.),
                        Vec2::new(w, 26.),
                    );
                    let selected = self.editor.markdown.editing == editing;
                    if selected {
                        ui.painter().rect_filled(rect, t.radius, color(&t.raised));
                    }
                    let response = ui
                        .interact(rect, egui::Id::unique(id), Sense::click())
                        .on_hover_text(help);
                    if response.hovered() || response.has_focus() {
                        ui.painter().rect_stroke(
                            rect,
                            t.radius,
                            Stroke::new(1., color(&t.accent)),
                            egui::StrokeKind::Inside,
                        );
                    }
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        label,
                        FontId::proportional(13.),
                        color(if selected { &t.text } else { &t.muted }),
                    );
                    if response.clicked() {
                        self.editor.markdown.editing = editing;
                    }
                }
            }
        } else if matches!(self.file_preview.content, Some(Content::Image { .. })) {
            ui.painter().text(
                bar.left_center(),
                Align2::LEFT_CENTER,
                crate::tr!("Зображення", "Image"),
                FontId::proportional(12.),
                color(&t.muted),
            );
        }
        reload.clicked()
    }
    pub(super) fn document_status(&mut self, ui: &mut egui::Ui, text: &str) {
        let t = self.preferences.theme.clone();
        let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.), Sense::hover());
        ui.interact(bar, egui::Id::unique("document-status-bar"), Sense::hover());
        ui.painter()
            .hline(bar.x_range(), bar.top(), Stroke::new(1., color(&t.border)));
        let image = matches!(self.file_preview.content, Some(Content::Image { .. }));
        let left = bar.right() - 132.;
        let status = Rect::from_min_max(
            bar.min + Vec2::new(0., 3.),
            Pos2::new((left - 8.).max(bar.left()), bar.bottom()),
        );
        super::widgets::row_text(
            ui,
            status,
            text.to_string(),
            FontId::proportional(11.),
            color(if self.editor.dirty() {
                &t.accent
            } else {
                &t.muted
            }),
        );
        ui.interact(status, egui::Id::unique("preview-status"), Sense::hover())
            .on_hover_text(text);
        for (offset, w, label, id, step, enabled) in [
            (
                0.,
                26.,
                "−".to_string(),
                "document-zoom-out",
                -1.,
                self.editor.zoom.factor > if image { 0.25 } else { 0.5 },
            ),
            (
                30.,
                68.,
                format!("{:.0}%", self.editor.zoom.factor * 100.),
                "document-zoom-reset",
                0.,
                true,
            ),
            (
                102.,
                26.,
                "+".to_string(),
                "document-zoom-in",
                1.,
                self.editor.zoom.factor < if image { 8. } else { 3. },
            ),
        ] {
            let rect =
                Rect::from_min_size(Pos2::new(left + offset, bar.top() + 2.), Vec2::new(w, 24.));
            let help = if step == 0. {
                format!(
                    "{} · {}",
                    crate::tr!(
                        "Скинути до 100% (зображення: вписати)",
                        "Reset to 100% (images: Fit)"
                    ),
                    self.preferences.layout.shortcuts.reset_zoom.label()
                )
            } else if step < 0. {
                crate::tr!("Зменшити · Ctrl+Minus", "Zoom out · Ctrl+Minus").into()
            } else {
                crate::tr!("Збільшити · Ctrl+Plus", "Zoom in · Ctrl+Plus").into()
            };
            let help = if step == 0. {
                help
            } else {
                format!(
                    "{help} · {} {}",
                    self.preferences.layout.file_zoom_step,
                    crate::tr!("п.п.", "pp")
                )
            };
            let response = ui
                .put(
                    rect,
                    egui::Button::new(RichText::new(label).size(11.).color(color(if enabled {
                        &t.text
                    } else {
                        &t.muted
                    })))
                    .frame(false)
                    .sense(if enabled {
                        Sense::click()
                    } else {
                        Sense::hover()
                    }),
                )
                .on_hover_text(help);
            ui.interact(response.rect, egui::Id::unique(id), Sense::hover());
            if enabled && response.clicked() {
                self.focus_area(crate::app::workspace_keyboard::Area::Editor, ui.ctx());
                if step == 0. {
                    self.editor.zoom.factor = 1.;
                } else {
                    self.editor
                        .zoom
                        .change(step, self.preferences.layout.file_zoom_step, image);
                }
            }
        }
    }
}
