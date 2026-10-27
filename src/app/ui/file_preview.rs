use crate::{
    app::{Disco, state::View},
    file_preview::Content,
    format::label,
    settings::{self, PreviewMode, color},
};
use eframe::egui::{self, Align2, FontId, Rect, RichText, Sense, Vec2};

impl Disco {
    pub(in crate::app) fn sync_file_preview(&mut self, ctx: &egui::Context) {
        let path = if self.preferences.layout.preview_mode != PreviewMode::Off
            && self.browser.view != View::Trash
        {
            self.browser
                .properties
                .as_ref()
                .filter(|p| !p.is_dir && p.error.is_none())
                .map(|p| p.path.clone())
        } else {
            None
        };
        self.file_preview.select(path, self.scan.report_revision);
        self.file_preview.poll(ctx);
    }
    pub(in crate::app) fn preview_controls(&mut self, ui: &mut egui::Ui) {
        let previous = self.preferences.layout.preview_mode;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(crate::tr!("Прев’ю", "Preview"))
                    .size(12.)
                    .color(color(&self.preferences.theme.muted)),
            );
            for (mode, text) in [
                (PreviewMode::Off, crate::tr!("Вимкнено", "Off")),
                (
                    PreviewMode::Inspector,
                    crate::tr!("Праворуч", "Right panel"),
                ),
                (
                    PreviewMode::Workspace,
                    crate::tr!("Поруч зі списком", "Beside list"),
                ),
            ] {
                ui.selectable_value(&mut self.preferences.layout.preview_mode, mode, text);
            }
        });
        if previous != self.preferences.layout.preview_mode {
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
            ui.ctx().request_repaint();
        }
    }
    pub(in crate::app) fn browser_content(&mut self, ui: &mut egui::Ui) {
        let workspace = self.preferences.layout.preview_mode == PreviewMode::Workspace;
        if workspace {
            let width = ui.available_width();
            egui::Panel::right("content-preview")
                .default_size((width * 0.52).max(260.))
                .min_size(240.)
                .max_size((width - 270.).max(240.))
                .resizable(true)
                .frame(
                    egui::Frame::new()
                        .fill(color(&self.preferences.theme.panel))
                        .inner_margin(10.),
                )
                .show(ui, |ui| {
                    ui.set_min_height(ui.available_height());
                    if let Some(path) = self.file_preview.path.clone() {
                        ui.label(RichText::new(label(&path)).strong().size(17.));
                    } else {
                        ui.label(
                            RichText::new(crate::tr!("Прев’ю файлу", "File preview"))
                                .strong()
                                .size(17.),
                        );
                    }
                    ui.add_space(8.);
                    self.file_preview_ui(ui, false);
                });
            egui::CentralPanel::default()
                .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 0)))
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
    pub(in crate::app) fn file_preview_ui(&mut self, ui: &mut egui::Ui, compact: bool) {
        let t = self.preferences.theme.clone();
        let path = self.file_preview.path.clone();
        if path.is_none() {
            let (rect, _) = ui.allocate_exact_size(
                Vec2::new(
                    ui.available_width(),
                    if compact {
                        120.
                    } else {
                        ui.available_height().max(120.)
                    },
                ),
                Sense::hover(),
            );
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                crate::tr!("Вибери файл для перегляду", "Select a file to preview"),
                FontId::proportional(14.),
                color(&t.muted),
            );
            return;
        }
        let mut refresh = false;
        ui.horizontal(|ui| {
            if matches!(self.file_preview.content, Some(Content::Image { .. })) {
                if ui.small_button(crate::tr!("Вписати", "Fit")).clicked() {
                    self.file_preview.zoom = 1.;
                }
                if ui
                    .small_button("−")
                    .on_hover_text(crate::tr!("Зменшити", "Zoom out"))
                    .clicked()
                {
                    self.file_preview.zoom = (self.file_preview.zoom / 1.25).max(1.);
                }
                if ui
                    .small_button("+")
                    .on_hover_text(crate::tr!("Збільшити", "Zoom in"))
                    .clicked()
                {
                    self.file_preview.zoom = (self.file_preview.zoom * 1.25).min(8.);
                }
            }
            if let Some(Content::Text { lines, .. }) = &self.file_preview.content {
                if ui
                    .small_button(crate::tr!("Копіювати", "Copy"))
                    .on_hover_text(crate::tr!(
                        "Копіювати показаний текст",
                        "Copy the displayed text"
                    ))
                    .clicked()
                {
                    ui.ctx().copy_text(lines.join("\n"));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(26., 24.), Sense::hover());
                refresh = super::widgets::visual_button(ui, &self.art, rect, 11, "", true, &t)
                    .on_hover_text(crate::tr!("Оновити прев’ю", "Refresh preview"))
                    .clicked();
                if compact {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(26., 24.), Sense::hover());
                    if super::widgets::visual_button(ui, &self.art, rect, 20, "", true, &t)
                        .on_hover_text(crate::tr!(
                            "Велике прев’ю поруч зі списком",
                            "Large preview beside the list"
                        ))
                        .clicked()
                    {
                        self.preferences.layout.preview_mode = PreviewMode::Workspace;
                        if !self.preferences.open {
                            if let Err(e) = settings::write(
                                &settings::config_dir().join("layout.toml"),
                                &self.preferences.layout,
                            ) {
                                self.message = e;
                            }
                        }
                        ui.ctx().request_repaint();
                    }
                }
            });
        });
        ui.add_space(5.);
        let height = if compact {
            // Reserve the title, factual properties, and action button. Taller
            // windows can spend the additional space on the image or text.
            (ui.ctx().viewport_rect().height() - 560.).clamp(90., 260.)
        } else {
            (ui.available_height() - 38.).max(100.)
        };
        let width = ui.available_width();
        egui::Frame::new()
            .fill(color(&t.background))
            .stroke(egui::Stroke::new(1., color(&t.border)))
            .corner_radius(t.radius)
            .show(ui, |ui| {
                ui.set_width(width - 2.);
                ui.set_height(height);
                match &self.file_preview.content {
                    Some(Content::Image { size, original, .. }) => {
                        if let Some(texture) = &self.file_preview.texture {
                            let aspect = original[0] as f32 / original[1].max(1) as f32;
                            let fitted = if aspect > width / height {
                                Vec2::new(width, width / aspect)
                            } else {
                                Vec2::new(height * aspect, height)
                            };
                            let size = fitted * self.file_preview.zoom;
                            egui::ScrollArea::both()
                                .id_salt(("image-content", &path, compact))
                                .auto_shrink([false, false])
                                .max_height(height)
                                .show(ui, |ui| {
                                    let (rect, _) = ui.allocate_exact_size(
                                        size.max(Vec2::new(width - 2., height)),
                                        Sense::hover(),
                                    );
                                    let image = Rect::from_center_size(rect.center(), size);
                                    ui.painter().image(
                                        texture.id(),
                                        image,
                                        Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                                        egui::Color32::WHITE,
                                    );
                                });
                        } else {
                            let _ = size;
                        }
                    }
                    Some(Content::Text { lines, .. }) if lines.is_empty() => {
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                RichText::new(crate::tr!("Порожній файл", "Empty file"))
                                    .color(color(&t.muted)),
                            );
                        });
                    }
                    Some(Content::Text { lines, .. }) => {
                        let font = FontId::monospace(
                            if compact { 12. } else { 14. } * self.preferences.layout.text_scale,
                        );
                        let line_height = font.size + 6.;
                        egui::ScrollArea::both()
                            .id_salt(("text-content", &path, compact))
                            .max_height(height)
                            .auto_shrink([false, false])
                            .show_rows(ui, line_height, lines.len(), |ui, range| {
                                for n in range {
                                    ui.horizontal(|ui| {
                                        ui.add_sized(
                                            [35., line_height],
                                            egui::Label::new(
                                                RichText::new((n + 1).to_string())
                                                    .font(font.clone())
                                                    .color(color(&t.muted)),
                                            )
                                            .selectable(false),
                                        );
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&lines[n]).font(font.clone()).color(
                                                    color(if lines[n].starts_with('#') {
                                                        &t.accent
                                                    } else {
                                                        &t.text
                                                    }),
                                                ),
                                            )
                                            .wrap_mode(egui::TextWrapMode::Extend),
                                        );
                                    });
                                }
                            });
                    }
                    content => {
                        let text = match content {
                            None => {
                                crate::tr!("Завантаження прев’ю…", "Loading preview…").to_string()
                            }
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
                            ui.add(
                                egui::Label::new(
                                    RichText::new(text).size(13.).color(color(&t.muted)),
                                )
                                .wrap(),
                            );
                        });
                    }
                }
            });
        ui.add_space(4.);
        match &self.file_preview.content {
            Some(Content::Image { original, size, .. }) => {
                ui.label(
                    RichText::new(format!("{} × {} px", original[0], original[1]))
                        .size(11.)
                        .color(color(&t.muted)),
                );
                if !compact && (original[0] > size[0] as u32 || original[1] > size[1] as u32) {
                    ui.label(
                        RichText::new(crate::tr!("Зменшене прев’ю", "Downscaled preview"))
                            .size(11.)
                            .color(color(&t.muted)),
                    );
                }
            }
            Some(Content::Text {
                encoding,
                truncated,
                ..
            }) => {
                ui.label(
                    RichText::new(format!(
                        "{} · {}",
                        encoding,
                        crate::tr!("Лише перегляд", "Read only")
                    ))
                    .size(11.)
                    .color(color(&t.muted)),
                );
                if *truncated {
                    ui.label(
                        RichText::new(crate::tr!(
                            "Показано початок файлу",
                            "Showing the beginning of the file"
                        ))
                        .size(11.)
                        .color(color(&t.accent)),
                    );
                }
            }
            _ => {}
        }
        if refresh {
            self.file_preview.reload();
            ui.ctx().request_repaint();
        }
    }
}
