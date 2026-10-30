use crate::{
    app::{Disco, editor::Action, state::View},
    file_preview::Content,
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
// Consume the remainder after the inner ScrollArea has handled its wheel event.
// At a document boundary egui normally passes this delta to its scrolling parent.
fn contain_preview_scroll(ui: &mut egui::Ui, rect: Rect) {
    if ui.rect_contains_pointer(rect) {
        ui.input_mut(|input| input.smooth_scroll_delta = Vec2::ZERO);
    }
}
impl Disco {
    pub(in crate::app) fn set_preview_mode(&mut self, mode: PreviewMode, ctx: &egui::Context) {
        let mode = if mode == PreviewMode::Off {
            PreviewMode::Overview
        } else {
            mode
        };
        let entering_focus =
            mode == PreviewMode::Focus && self.preferences.layout.preview_mode != mode;
        self.preferences.layout.preview_mode = mode;
        match mode {
            PreviewMode::Overview => self.preferences.layout.properties = true,
            PreviewMode::Focus => self.preferences.layout.properties = false,
            PreviewMode::Off => {}
        }
        if entering_focus {
            self.prepare_focus_tree();
        }
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
    pub(in crate::app) fn prepare_focus_tree(&mut self) {
        self.browser.table_root = None;
        let mut ancestors = Vec::new();
        if let (Some(report), Some(p)) = (&self.scan.report, &self.browser.properties) {
            let parent = if p.is_dir {
                Some(p.path.as_path())
            } else {
                p.path.parent()
            };
            let mut current =
                parent.and_then(|path| report.folders.iter().position(|f| f.path == path));
            while let Some(id) = current {
                ancestors.push(id);
                current = report.folders[id].parent;
            }
        }
        for id in ancestors {
            self.expand_folder(id);
        }
        self.browser.scroll_to = self.browser.properties.as_ref().map(|p| p.path.clone());
    }
    pub(in crate::app) fn browser_content(&mut self, ui: &mut egui::Ui) {
        if self.preferences.layout.preview_mode == PreviewMode::Focus {
            let t = self.preferences.theme.clone();
            let initial_width = if self.screenshot.is_some() {
                std::env::var("DISCO_QA_TREE_WIDTH")
                    .ok()
                    .and_then(|s| s.parse::<f32>().ok())
                    .filter(|w| w.is_finite())
                    .unwrap_or(300.)
                    .clamp(240., 480.)
            } else {
                300.
            };
            let tree = egui::Panel::left("focus-file-tree")
                .default_size(initial_width)
                .min_size(240.)
                .max_size(480.)
                .resizable(true)
                .frame(
                    egui::Frame::new()
                        .fill(color(&t.background))
                        .inner_margin(egui::Margin::symmetric(8, 8)),
                )
                .show(ui, |ui| {
                    self.file_list_content(ui);
                });
            ui.interact(
                tree.response.rect,
                egui::Id::unique("focus-tree-panel"),
                Sense::focusable_noninteractive(),
            );
            ui.painter().vline(
                tree.response.rect.right(),
                tree.response.rect.y_range(),
                Stroke::new(1., color(&t.border)),
            );
            let document = egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(color(&t.panel))
                        .inner_margin(egui::Margin::symmetric(12, 8)),
                )
                .show(ui, |ui| {
                    self.document_tabs_ui(ui);
                    ui.add_space(4.);
                    let height = ui.available_height();
                    self.file_preview_ui(ui, false, height);
                });
            ui.interact(
                document.response.rect,
                egui::Id::unique("focus-document-panel"),
                Sense::hover(),
            );
        } else {
            self.file_list_content(ui);
        }
    }
    fn file_list_content(&mut self, ui: &mut egui::Ui) {
        if self.preferences.layout.preview_mode != PreviewMode::Focus {
            self.scope_controls(ui);
        }
        ui.add_space(8.);
        self.unindexed_collection_folders(ui);
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
        let read_only = self.editor_read_only();
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
        let bottom = ui.cursor().min.y + available_height;
        let old_spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 0.;
        let width = ui.available_width();
        let (bar, _) = ui.allocate_exact_size(Vec2::new(width, 30.), Sense::hover());
        let refresh = command(
            ui,
            &self.art,
            Rect::from_min_size(Pos2::new(bar.right() - 30., bar.top()), Vec2::splat(30.)),
            11,
            "",
            self.editor.saving.is_none() && self.editor.untitled.is_none(),
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
        let has_footer = matches!(
            self.file_preview.content,
            Some(Content::Text { .. } | Content::Image { .. })
        );
        let footer_height = if has_footer {
            ui.painter()
                .layout_no_wrap("UTF-8".into(), FontId::proportional(11.), color(&t.muted))
                .size()
                .y
                + 6.
        } else {
            0.
        };
        let room = (bottom - ui.cursor().min.y - footer_height - error_height).max(80.);
        let height = if compact {
            match &self.file_preview.content {
                Some(Content::Image { original, .. }) => {
                    let desired = width * original[1].max(1) as f32 / original[0].max(1) as f32;
                    desired.max(170.).min(room)
                }
                _ => room.min(480.),
            }
        } else {
            room
        };
        let mut text_changed = false;
        let viewport = egui::Frame::new()
            .fill(color(&t.background))
            .stroke(Stroke::new(1., color(&t.border)))
            .corner_radius(t.radius)
            .inner_margin(8.)
            .show(ui, |ui| {
                let inner = Vec2::new((width - 18.).max(30.), (height - 18.).max(60.));
                ui.set_min_size(inner);
                ui.set_max_width(inner.x);
                if let Some(doc) = &mut self.editor.document {
                    // Saving snapshots the document; keep this widget enabled
                    // so focus, cursor, selection and further typing survive IO.
                    let busy = self.scan.busy;
                    let font =
                        FontId::monospace(13. * self.preferences.layout.effective_text_scale());
                    egui::ScrollArea::both()
                        .id_salt(("editable-content", &path))
                        .auto_shrink([false, false])
                        .max_height(inner.y)
                        .show(ui, |ui| {
                            ui.set_min_size(inner);
                            ui.horizontal_top(|ui| {
                                let gutter_left = ui.cursor().min.x;
                                ui.allocate_space(Vec2::new(30., 0.));
                                let mut buffer = doc.buffer_with_access(!read_only);
                                let syntax = &mut self.syntax;
                                let mut layouter =
                                    |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, _width: f32| {
                                        syntax.layout(
                                            ui,
                                            buffer.as_str(),
                                            font.clone(),
                                            path.as_deref(),
                                            color(&t.text),
                                        )
                                    };
                                let mut output = ui
                                    .add_enabled_ui(!busy, |ui| {
                                        egui::TextEdit::multiline(&mut buffer)
                                            .id(egui::Id::unique(("file-editor", &path)))
                                            .code_editor()
                                            .layouter(&mut layouter)
                                            .font(font.clone())
                                            .frame(egui::Frame::NONE)
                                            .margin(Vec2::new(2., 0.))
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(1)
                                            // Keep hit testing below short documents inside
                                            // TextEdit, so egui maps the click to EOF.
                                            .min_size(Vec2::new(0., inner.y))
                                            .char_limit(128 * 1024)
                                            .show(ui)
                                    })
                                    .inner;
                                text_changed = output.response.changed();
                                let blank_gutter = Rect::from_min_max(
                                    Pos2::new(
                                        gutter_left,
                                        output.galley_pos.y + output.galley.size().y,
                                    ),
                                    Pos2::new(
                                        output.response.rect.left(),
                                        output.response.rect.bottom(),
                                    ),
                                );
                                if !busy
                                    && blank_gutter.is_positive()
                                    && ui
                                        .interact(
                                            blank_gutter,
                                            output.response.id.with("blank-gutter"),
                                            Sense::click(),
                                        )
                                        .on_hover_cursor(egui::CursorIcon::Text)
                                        .clicked()
                                {
                                    output.response.request_focus();
                                    output.state.cursor.set_char_range(Some(
                                        egui::text::CCursorRange::one(egui::text::CCursor::new(
                                            doc.text.chars().count(),
                                        )),
                                    ));
                                    output.state.clone().store(ui.ctx(), output.response.id);
                                }
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
                        Some(Content::Text { display_source, .. }) => {
                            let font = FontId::monospace(
                                13. * self.preferences.layout.effective_text_scale(),
                            );
                            let galley = self.syntax.layout(
                                ui,
                                display_source,
                                font.clone(),
                                path.as_deref(),
                                color(&t.text),
                            );
                            egui::ScrollArea::both()
                                .id_salt(("readonly-content", &path, compact))
                                .auto_shrink([false, false])
                                .max_height(inner.y)
                                .show(ui, |ui| {
                                    ui.set_min_size(inner);
                                    ui.horizontal_top(|ui| {
                                        ui.allocate_space(Vec2::new(30., 0.));
                                        let response = ui.add(
                                            egui::Label::new(galley.clone())
                                                .selectable(true)
                                                .wrap_mode(egui::TextWrapMode::Extend),
                                        );
                                        for (n, row) in galley.rows.iter().enumerate() {
                                            let y = response.rect.top() + row.pos.y;
                                            if y + row.size.y < ui.clip_rect().top()
                                                || y > ui.clip_rect().bottom()
                                            {
                                                continue;
                                            }
                                            ui.painter().text(
                                                Pos2::new(
                                                    response.rect.left() - 12.,
                                                    y + row.size.y * 0.5,
                                                ),
                                                Align2::RIGHT_CENTER,
                                                (n + 1).to_string(),
                                                font.clone(),
                                                color(&t.muted),
                                            );
                                        }
                                    });
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
        if self.preferences.layout.preview_mode == PreviewMode::Focus
            && ui.rect_contains_pointer(viewport.response.rect)
            && ui.input(|i| i.pointer.primary_pressed())
            && let Some(path) = &path
        {
            self.keep_preview_tab(path);
        }
        if text_changed {
            self.note_editor_change(ui.ctx());
        }
        ui.interact(
            viewport.response.rect,
            egui::Id::unique("preview-viewport"),
            Sense::hover(),
        );
        contain_preview_scroll(ui, viewport.response.rect);
        ui.add_space(6.);
        let footer = match &self.file_preview.content {
            Some(Content::Image { original, .. }) => {
                Some(format!("{} × {} px", original[0], original[1]))
            }
            Some(Content::Text {
                encoding,
                truncated,
                ..
            }) => {
                let state = if self.editor.saving.is_some() {
                    crate::tr!("Збереження…", "Saving…")
                } else if self.editor.dirty() {
                    crate::tr!("Незбережені зміни · Ctrl+S", "Unsaved changes · Ctrl+S")
                } else if read_only {
                    crate::tr!("Категорія: лише читання", "Category: read only")
                } else if self.editor.last_saved_at.is_some() {
                    crate::tr!("Збережено", "Saved")
                } else if self.editor.document.is_some() {
                    crate::tr!("Редагування", "Editable")
                } else {
                    crate::tr!("Лише перегляд", "Read only")
                };
                let language = self.syntax.language().unwrap_or("Text");
                let mut text = format!("{encoding} · {language} · {state}");
                if *truncated {
                    text.push_str(crate::tr!(
                        " · Показано початок файлу",
                        " · Showing a partial file"
                    ));
                }
                Some(text)
            }
            _ => None,
        };
        if let Some(text) = footer {
            let response = ui
                .add(
                    egui::Label::new(RichText::new(&text).size(11.).color(color(
                        if self.editor.dirty() {
                            &t.accent
                        } else {
                            &t.muted
                        },
                    )))
                    .truncate(),
                )
                .on_hover_text(&text);
            ui.interact(
                response.rect,
                egui::Id::unique("preview-status"),
                Sense::hover(),
            );
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
        ui.spacing_mut().item_spacing.y = old_spacing;
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
    #[test]
    fn wheel_inside_a_finished_or_short_document_does_not_scroll_the_parent() {
        for content_height in [40., 1800.] {
            let ctx = egui::Context::default();
            let mut time = 0.;
            let mut render = |pointer: Option<Pos2>, wheel: bool| {
                time += 0.05;
                let mut viewport = Rect::NOTHING;
                let mut parent_offset = 0.;
                let mut events = vec![];
                if let Some(pos) = pointer {
                    events.push(egui::Event::PointerMoved(pos));
                }
                if wheel {
                    events.push(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        source: egui::MouseWheelSource::Wheel,
                        phase: egui::TouchPhase::Move,
                        delta: Vec2::new(0., -240.),
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600., 500.))),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        let outer = egui::ScrollArea::vertical()
                            .id_salt("outer-properties-test")
                            .max_height(400.)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let frame = egui::Frame::NONE.show(ui, |ui| {
                                    egui::ScrollArea::both()
                                        .id_salt("inner-document-test")
                                        .max_height(180.)
                                        .auto_shrink([false, false])
                                        .vertical_scroll_offset(10000.)
                                        .show(ui, |ui| {
                                            ui.allocate_exact_size(
                                                Vec2::new(400., content_height),
                                                Sense::hover(),
                                            );
                                        });
                                });
                                viewport = frame.response.rect;
                                contain_preview_scroll(ui, viewport);
                                ui.allocate_exact_size(Vec2::new(400., 1600.), Sense::hover());
                            });
                        parent_offset = outer.state.clamped_offset().y;
                    },
                )
                .drop_without_applying_deltas();
                (viewport, parent_offset)
            };
            let (viewport, _) = render(None, false);
            let inside = viewport.center();
            for _ in 0..8 {
                let (_, offset) = render(Some(inside), true);
                assert!(
                    offset.abs() < 0.5,
                    "document height {content_height}, parent scrolled to {offset}"
                );
            }
            let mut moved = false;
            for _ in 0..4 {
                let (_, offset) = render(Some(Pos2::new(100., 300.)), true);
                moved |= offset > 1.;
            }
            assert!(
                moved,
                "the properties panel must still scroll outside the preview"
            );
        }
    }
}
