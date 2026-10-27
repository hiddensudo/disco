use crate::app::Disco;
use eframe::egui::{self, RichText};
impl Disco {
    pub(in crate::app) fn editor_confirmation(&mut self, ctx: &egui::Context) {
        if self.editor.pending.is_none() {
            return;
        }
        let mut save = false;
        let mut discard = false;
        let mut cancel = false;
        let fresh = self.editor.confirm_fresh;
        self.editor.confirm_fresh = false;
        egui::Modal::new(egui::Id::unique("unsaved-text")).show(ctx, |ui| {
            ui.set_width(460.);
            ui.heading(crate::tr!("Зберегти зміни?", "Save changes?"));
            ui.add_space(8.);
            if let Some(path) = &self.editor.path {
                ui.label(RichText::new(crate::format::label(path)).strong());
                ui.label(path.to_string_lossy());
            }
            ui.add_space(8.);
            ui.label(crate::tr!(
                "Є незбережений текст. Збережи його, відкинь зміни або повернися до редагування.",
                "This file has unsaved text. Save it, discard the changes, or return to editing."
            ));
            if !self.editor.error.is_empty() {
                ui.add_space(8.);
                ui.colored_label(
                    crate::settings::color(&self.preferences.theme.accent),
                    &self.editor.error,
                );
            }
            ui.add_space(12.);
            ui.horizontal(|ui| {
                let button = ui.button(crate::tr!("Скасувати", "Cancel"));
                if fresh {
                    button.request_focus();
                }
                cancel = button.clicked();
                discard = ui
                    .add_enabled(
                        self.editor.saving.is_none(),
                        egui::Button::new(crate::tr!("Не зберігати", "Discard")),
                    )
                    .clicked();
                save = ui
                    .add_enabled(
                        self.editor.saving.is_none(),
                        egui::Button::new(if self.editor.saving.is_some() {
                            crate::tr!("Збереження…", "Saving…")
                        } else {
                            crate::tr!("Зберегти", "Save")
                        }),
                    )
                    .clicked();
            });
        });
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            cancel = true;
        }
        if cancel {
            self.editor.pending = None;
        } else if discard {
            self.editor.document = None;
            self.editor.path = None;
            self.file_preview.reload();
            if let Some(action) = self.editor.pending.take() {
                self.apply_editor_action(action, ctx);
            }
        } else if save {
            self.save_editor(ctx);
        }
    }
}
