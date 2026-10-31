use crate::{app::Disco, settings::color};
use eframe::egui::{self, Key, Modifiers, Pos2, Rect, Sense, Stroke, Vec2};
impl Disco {
    pub(in crate::app) fn address_bar(&mut self, ui: &mut egui::Ui, rect: Rect) -> bool {
        let t = self.preferences.theme.clone();
        if !self.paths.editing {
            let edit = Rect::from_min_size(
                Pos2::new(rect.right() - 26., rect.top() + 3.),
                Vec2::splat(26.),
            );
            let response = ui
                .interact(edit, egui::Id::unique("edit-address"), Sense::click())
                .on_hover_text(crate::tr!("Змінити шлях · Ctrl+L", "Edit path · Ctrl+L"));
            if response.hovered() {
                ui.painter().rect_filled(edit, t.radius, color(&t.raised));
            }
            let c = edit.center();
            let stroke = Stroke::new(1.6, color(&t.text));
            ui.painter()
                .line_segment([c + Vec2::new(-5., 4.), c + Vec2::new(3., -4.)], stroke);
            ui.painter()
                .line_segment([c + Vec2::new(-3., 6.), c + Vec2::new(5., -2.)], stroke);
            ui.painter()
                .line_segment([c + Vec2::new(-5., 4.), c + Vec2::new(-6., 7.)], stroke);
            if response.clicked() {
                self.begin_path_edit(ui.ctx());
            }
            return false;
        }
        let mut submit = None;
        let mut complete = false;
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) {
            self.paths.chosen = (!self.paths.completion.hits.is_empty()).then(|| {
                self.paths
                    .chosen
                    .map_or(0, |n| (n + 1).min(self.paths.completion.hits.len() - 1))
            });
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
            self.paths.chosen = self.paths.chosen.map(|n| n.saturating_sub(1));
        }
        let enter = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
            complete = true;
        }
        let response = ui.put(
            rect,
            egui::TextEdit::singleline(&mut self.paths.input)
                .id(egui::Id::unique("address-path"))
                .hint_text("/")
                .margin(Vec2::new(8., 7.)),
        );
        if self.paths.focus {
            self.paths.focus = false;
            response.request_focus();
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), response.id) {
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(if self.paths.select_all {
                            0
                        } else {
                            self.paths.input.chars().count()
                        }),
                        egui::text::CCursor::new(self.paths.input.chars().count()),
                    )));
                state.store(ui.ctx(), response.id);
            }
        }
        if response.changed() {
            self.paths.chosen = None;
            self.paths.error.clear();
            self.configure_path_completion();
            ui.ctx().request_repaint();
        }
        let selected = self
            .paths
            .chosen
            .and_then(|n| self.paths.completion.hits.get(n))
            .cloned();
        if enter {
            submit = Some(selected.as_ref().map_or_else(
                || self.paths.input.clone(),
                |s| s.path.display().to_string(),
            ));
        }
        if complete
            && let Some(s) = selected.or_else(|| self.paths.completion.hits.first().cloned())
        {
            self.paths.input = s.path.display().to_string() + if s.directory { "/" } else { "" };
            self.paths.chosen = None;
            self.paths.focus = true;
            self.paths.select_all = false;
            self.configure_path_completion();
            ui.ctx().request_repaint();
        }
        let hits = self.paths.completion.hits.clone();
        let error = if self.paths.error.is_empty() {
            self.paths.completion.error.clone()
        } else {
            self.paths.error.clone()
        };
        if !hits.is_empty() || !error.is_empty() {
            egui::Area::new(egui::Id::unique("address-suggestions"))
                .order(egui::Order::Foreground)
                .fixed_pos(rect.left_bottom() + Vec2::new(0., 3.))
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_width(rect.width().max(180.));
                        if !error.is_empty() {
                            ui.colored_label(color(&t.text), &error);
                        }
                        egui::ScrollArea::vertical()
                            .max_height(240.)
                            .show(ui, |ui| {
                                for (n, s) in hits.iter().enumerate() {
                                    let row = ui.horizontal(|ui| {
                                        let (icon, _) = ui
                                            .allocate_exact_size(Vec2::splat(18.), Sense::hover());
                                        self.art.paint(
                                            ui,
                                            if s.directory {
                                                0
                                            } else {
                                                crate::art::file_icon(&s.path)
                                            },
                                            icon,
                                            color(&t.text),
                                        );
                                        let name = s
                                            .path
                                            .file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy();
                                        let response = ui.selectable_label(
                                            self.paths.chosen == Some(n),
                                            format!("{name}{}", if s.directory { "/" } else { "" }),
                                        );
                                        if response.clicked() {
                                            submit = Some(s.path.display().to_string());
                                        }
                                        if self.paths.chosen == Some(n) {
                                            response.scroll_to_me(Some(egui::Align::Center));
                                        }
                                    });
                                    ui.interact(
                                        row.response.rect,
                                        egui::Id::unique(("path-suggestion", &s.path)),
                                        Sense::hover(),
                                    );
                                }
                            });
                    });
                });
        }
        if let Some(text) = submit {
            self.submit_path(text, ui.ctx());
        }
        true
    }
}
