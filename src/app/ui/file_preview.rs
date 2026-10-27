use crate::{
    app::{Disco, editor::Action, state::View},
    file_preview::Content,
    format::{label, shorten},
    settings::{self, PreviewMode, Theme, color},
};
use eframe::egui::{self, Align2, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

fn command(
    ui: &mut egui::Ui,
    art: &crate::art::Art,
    rect: Rect,
    icon: usize,
    text: &str,
    enabled: bool,
    active: bool,
    t: &Theme,
) -> egui::Response {
    let response = super::widgets::visual_button(ui, art, rect, icon, text, enabled, t);
    if active {
        ui.painter().rect_stroke(
            rect,
            t.radius,
            Stroke::new(1., color(&t.accent)),
            egui::StrokeKind::Inside,
        );
    }
    response
}
pub(super) fn fitted_image(original: [u32; 2], viewport: Vec2) -> Vec2 {
    let size = Vec2::new(original[0].max(1) as f32, original[1].max(1) as f32);
    size * (viewport.x / size.x).min(viewport.y / size.y)
}
impl Disco {
    pub(in crate::app) fn set_preview_mode(&mut self, mode: PreviewMode, ctx: &egui::Context) {
        if mode == PreviewMode::Off && (self.editor.dirty() || self.editor.saving.is_some()) {
            self.editor_action(Action::Mode(mode), ctx);
            return;
        }
        self.preferences.layout.preview_mode = mode;
        if !self.preferences.open {
            if let Err(e) = settings::write(
                &settings::config_dir().join("layout.toml"),
                &self.preferences.layout,
            ) {
                self.message = crate::tr!(
                    "Не вдалося зберегти режим прев’ю: {e}",
                    "Could not save the preview mode: {e}",
                    e = e
                );
            }
        }
        ctx.request_repaint();
    }
    pub(in crate::app) fn preview_mode_buttons(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let t = self.preferences.theme.clone();
        for (slot, mode, icon, help) in [
            (
                0,
                PreviewMode::Inspector,
                25,
                crate::tr!("Прев’ю праворуч", "Preview in the right panel"),
            ),
            (
                1,
                PreviewMode::Workspace,
                26,
                crate::tr!("Прев’ю поруч зі списком", "Preview beside the list"),
            ),
            (
                2,
                PreviewMode::Off,
                21,
                crate::tr!("Закрити прев’ю", "Close preview"),
            ),
        ] {
            let r = Rect::from_min_size(
                rect.min + Vec2::new(slot as f32 * 32., 0.),
                Vec2::new(28., 28.),
            );
            if command(
                ui,
                &self.art,
                r,
                icon,
                "",
                true,
                self.preferences.layout.preview_mode == mode && mode != PreviewMode::Off,
                &t,
            )
            .on_hover_text(help)
            .clicked()
            {
                self.set_preview_mode(mode, ui.ctx());
            }
        }
    }
    pub(in crate::app) fn browser_content(&mut self, ui: &mut egui::Ui) {
        if self.preferences.layout.preview_mode == PreviewMode::Workspace {
            let width = ui.available_width();
            egui::Panel::right("content-preview")
                .default_size((width * 0.52).max(260.))
                .min_size(240.)
                .max_size((width - 270.).max(240.))
                .resizable(true)
                .frame(
                    egui::Frame::new()
                        .fill(color(&self.preferences.theme.panel))
                        .inner_margin(12.),
                )
                .show(ui, |ui| {
                    ui.set_min_height(ui.available_height());
                    let t = self.preferences.theme.clone();
                    let (head, _) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 38.), Sense::hover());
                    let title = self
                        .file_preview
                        .path
                        .as_ref()
                        .map(|p| label(p))
                        .unwrap_or_else(|| crate::tr!("Прев’ю файлу", "File preview").into());
                    ui.painter()
                        .with_clip_rect(Rect::from_min_max(
                            head.min,
                            Pos2::new(head.right() - 100., head.bottom()),
                        ))
                        .text(
                            head.left_center(),
                            Align2::LEFT_CENTER,
                            shorten(&title, ((head.width() - 105.) / 8.).max(5.) as usize),
                            FontId::proportional(16.),
                            color(&t.text),
                        );
                    self.preview_mode_buttons(
                        ui,
                        Rect::from_min_size(
                            Pos2::new(head.right() - 92., head.top() + 4.),
                            Vec2::new(92., 28.),
                        ),
                    );
                    ui.add_space(6.);
                    let h = ui.available_height();
                    self.file_preview_ui(ui, false, h);
                });
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| self.file_list_content(ui));
        } else {
            self.file_list_content(ui);
        }
    }
    fn file_list_content(&mut self, ui: &mut egui::Ui) {
        if self.browser.view == View::Folders {
            self.folder_table(ui);
        } else {
            self.other_content(ui);
        }
    }
    pub(in crate::app) fn file_preview_ui(
        &mut self,
        ui: &mut egui::Ui,
        compact: bool,
        available_height: f32,
    ) {
        let t = self.preferences.theme.clone();
        let path = self.file_preview.path.clone();
        if path.is_none() {
            let (r, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), available_height.max(100.)),
                Sense::hover(),
            );
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                crate::tr!("Вибери файл для перегляду", "Select a file to preview"),
                FontId::proportional(14.),
                color(&t.muted),
            );
            return;
        }
        let width = ui.available_width();
        let (bar, _) = ui.allocate_exact_size(Vec2::new(width, 30.), Sense::hover());
        let refresh = command(
            ui,
            &self.art,
            Rect::from_min_size(Pos2::new(bar.right() - 30., bar.top()), Vec2::splat(30.)),
            11,
            "",
            self.editor.saving.is_none(),
            false,
            &t,
        )
        .on_hover_text(crate::tr!("Оновити прев’ю", "Reload preview"))
        .clicked();
        if matches!(self.file_preview.content, Some(Content::Image { .. })) {
            let mut x = bar.left();
            for (w, text, help, action) in [
                (
                    76.,
                    crate::tr!("Вписати", "Fit"),
                    crate::tr!("Показати картинку цілком", "Fit the entire image"),
                    0,
                ),
                (30., "−", crate::tr!("Зменшити", "Zoom out"), 1),
                (30., "+", crate::tr!("Збільшити", "Zoom in"), 2),
            ] {
                if command(
                    ui,
                    &self.art,
                    Rect::from_min_size(Pos2::new(x, bar.top()), Vec2::new(w, 30.)),
                    usize::MAX,
                    text,
                    true,
                    false,
                    &t,
                )
                .on_hover_text(help)
                .clicked()
                {
                    self.file_preview.zoom = match action {
                        0 => 1.,
                        1 => (self.file_preview.zoom / 1.25).max(1.),
                        _ => (self.file_preview.zoom * 1.25).min(8.),
                    };
                }
                x += w + 6.;
            }
        } else if matches!(self.file_preview.content, Some(Content::Text { .. })) {
            let labels = width >= 250.;
            let mut x = bar.left();
            let save_width = if labels { 96. } else { 30. };
            if self.editor.document.is_some() {
                if command(
                    ui,
                    &self.art,
                    Rect::from_min_size(Pos2::new(x, bar.top()), Vec2::new(save_width, 30.)),
                    28,
                    if labels {
                        crate::tr!("Зберегти", "Save")
                    } else {
                        ""
                    },
                    self.editor.dirty() && self.editor.saving.is_none() && !self.demo,
                    false,
                    &t,
                )
                .on_hover_text(crate::tr!(
                    "Зберегти зміни · Ctrl+S",
                    "Save changes · Ctrl+S"
                ))
                .clicked()
                {
                    self.save_editor(ui.ctx());
                }
                x += save_width + 6.;
            }
            let copy_width = if labels { 100. } else { 30. };
            if command(
                ui,
                &self.art,
                Rect::from_min_size(Pos2::new(x, bar.top()), Vec2::new(copy_width, 30.)),
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
            .on_hover_text(crate::tr!("Копіювати текст", "Copy text"))
            .clicked()
            {
                if let Some(doc) = &self.editor.document {
                    ui.ctx().copy_text(doc.text.clone());
                } else if let Some(Content::Text { lines, .. }) = &self.file_preview.content {
                    ui.ctx().copy_text(lines.join("\n"));
                }
            }
        }
        ui.add_space(8.);
        let error_height = if self.editor.error.is_empty() {
            0.
        } else {
            58.
        };
        let height = if compact {
            match &self.file_preview.content {
                Some(Content::Image { original, .. }) => {
                    let desired = width * original[1].max(1) as f32 / original[0].max(1) as f32;
                    desired.clamp(170., (available_height - 70.).max(220.))
                }
                _ => (available_height * 0.65).clamp(180., 340.),
            }
        } else {
            (available_height - 68. - error_height).max(140.)
        };
        egui::Frame::new()
            .fill(color(&t.background))
            .stroke(Stroke::new(1., color(&t.border)))
            .corner_radius(t.radius)
            .inner_margin(8.)
            .show(ui, |ui| {
                let inner = Vec2::new((width - 18.).max(30.), (height - 18.).max(60.));
                ui.set_min_size(inner);
                ui.set_max_width(inner.x);
                if let Some(doc) = &mut self.editor.document {
                    let busy = self.editor.saving.is_some() || self.scan.busy;
                    let font = FontId::monospace(13. * self.preferences.layout.text_scale);
                    egui::ScrollArea::both()
                        .id_salt(("editable-content", &path))
                        .auto_shrink([false, false])
                        .max_height(inner.y)
                        .show(ui, |ui| {
                            ui.set_min_size(inner);
                            ui.horizontal_top(|ui| {
                                ui.allocate_space(Vec2::new(30., 0.));
                                let mut buffer = doc.buffer();
                                let output = ui
                                    .add_enabled_ui(!busy, |ui| {
                                        egui::TextEdit::multiline(&mut buffer)
                                            .id(egui::Id::unique(("file-editor", &path)))
                                            .code_editor()
                                            .font(font.clone())
                                            .frame(egui::Frame::NONE)
                                            .margin(Vec2::new(2., 0.))
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(8)
                                            .char_limit(128 * 1024)
                                            .show(ui)
                                    })
                                    .inner;
                                self.editor.focused |= output.response.has_focus();
                                let clip = ui.clip_rect();
                                for (n, row) in output.galley.rows.iter().enumerate() {
                                    let y = output.galley_pos.y + row.pos.y;
                                    if y + row.size.y < clip.top() {
                                        continue;
                                    }
                                    if y > clip.bottom() {
                                        break;
                                    }
                                    ui.painter().text(
                                        Pos2::new(output.galley_pos.x - 12., y + row.size.y * 0.5),
                                        Align2::RIGHT_CENTER,
                                        (n + 1).to_string(),
                                        font.clone(),
                                        color(&t.muted),
                                    );
                                }
                            });
                        });
                } else {
                    match &self.file_preview.content {
                        Some(Content::Image { original, .. }) => {
                            if let Some(texture) = &self.file_preview.texture {
                                let size = fitted_image(*original, inner) * self.file_preview.zoom;
                                egui::ScrollArea::both()
                                    .id_salt(("image-content", &path, compact))
                                    .auto_shrink([false, false])
                                    .max_height(inner.y)
                                    .show(ui, |ui| {
                                        let (rect, _) =
                                            ui.allocate_exact_size(size.max(inner), Sense::hover());
                                        ui.painter().image(
                                            texture.id(),
                                            Rect::from_center_size(rect.center(), size),
                                            Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                                            egui::Color32::WHITE,
                                        );
                                    });
                            }
                        }
                        Some(Content::Text { lines, .. }) => {
                            let font = FontId::monospace(13. * self.preferences.layout.text_scale);
                            let row_height = font.size + 5.;
                            egui::ScrollArea::both()
                                .id_salt(("readonly-content", &path, compact))
                                .auto_shrink([false, false])
                                .max_height(inner.y)
                                .show_rows(ui, row_height, lines.len(), |ui, range| {
                                    ui.spacing_mut().item_spacing.y = 0.;
                                    for n in range {
                                        ui.horizontal(|ui| {
                                            ui.add_sized(
                                                [28., row_height],
                                                egui::Label::new(
                                                    RichText::new((n + 1).to_string())
                                                        .font(font.clone())
                                                        .color(color(&t.muted)),
                                                ),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&lines[n])
                                                        .font(font.clone())
                                                        .color(color(&t.text)),
                                                )
                                                .wrap_mode(egui::TextWrapMode::Extend),
                                            );
                                        });
                                    }
                                });
                        }
                        other => {
                            let text = match other {
                                None => crate::tr!("Завантаження прев’ю…", "Loading preview…")
                                    .to_string(),
                                Some(Content::TooLarge) => crate::tr!(
                                    "Файл завеликий для легкого прев’ю",
                                    "File too large for a lightweight preview"
                                )
                                .into(),
                                Some(Content::Error(e)) => e.clone(),
                                _ => crate::tr!(
                                    "Для цього формату прев’ю немає",
                                    "Preview is unavailable for this format"
                                )
                                .into(),
                            };
                            ui.centered_and_justified(|ui| {
                                ui.label(RichText::new(text).size(13.).color(color(&t.muted)));
                            });
                        }
                    }
                }
            });
        ui.add_space(6.);
        match &self.file_preview.content {
            Some(Content::Image { original, .. }) => {
                ui.label(
                    RichText::new(format!("{} × {} px", original[0], original[1]))
                        .size(11.)
                        .color(color(&t.muted)),
                );
            }
            Some(Content::Text {
                encoding,
                truncated,
                ..
            }) => {
                let status = if self.editor.saving.is_some() {
                    crate::tr!("Збереження…", "Saving…")
                } else if self.editor.dirty() {
                    crate::tr!("Незбережені зміни · Ctrl+S", "Unsaved changes · Ctrl+S")
                } else if self.editor.document.is_some() {
                    crate::tr!("Редагування", "Editable")
                } else {
                    crate::tr!("Лише перегляд", "Read only")
                };
                ui.label(
                    RichText::new(format!("{encoding} · {status}"))
                        .size(11.)
                        .color(color(if self.editor.dirty() {
                            &t.accent
                        } else {
                            &t.muted
                        })),
                );
                if *truncated {
                    ui.label(
                        RichText::new(crate::tr!(
                            "Показано початок файлу · редагування недоступне",
                            "Showing a partial file · editing is unavailable"
                        ))
                        .size(11.)
                        .color(color(&t.muted)),
                    );
                }
            }
            _ => {}
        }
        if !self.editor.error.is_empty() {
            ui.add(
                egui::Label::new(
                    RichText::new(&self.editor.error)
                        .size(12.)
                        .color(color(&t.accent)),
                )
                .wrap(),
            );
        }
        if refresh {
            self.editor_action(Action::Reload, ui.ctx());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portrait_and_landscape_fit_the_available_canvas_without_cropping() {
        let portrait = fitted_image([720, 1280], Vec2::new(270., 340.));
        assert!(portrait.y > 330. && portrait.x > 180.);
        assert!((portrait.x / portrait.y - 720. / 1280.).abs() < 0.001);
        let landscape = fitted_image([1920, 1080], Vec2::new(270., 340.));
        assert!((landscape.x - 270.).abs() < 0.01);
        assert!(landscape.y < 160.);
    }
}
