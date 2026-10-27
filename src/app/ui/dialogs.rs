use crate::app::Disco;
use crate::deletion;
use crate::format::{display_size, label};
use crate::settings::{Theme, color};
use eframe::egui::{self, Color32 as C, RichText};

impl Disco {
    pub(in crate::app) fn confirmation(&mut self, ctx: &egui::Context) {
        if !self.deletion.confirm {
            return;
        }
        let modal = delete_dialog(
            ctx,
            &self.deletion.targets,
            &self.preferences.theme,
            self.deletion.fresh,
        );
        let cancel = modal.inner.0.clicked();
        let yes = modal.inner.1.clicked();
        self.deletion.fresh = false;
        if cancel || modal.should_close() {
            self.deletion.confirm = false;
            self.deletion.targets.clear();
        } else if yes {
            self.commit_delete(ctx);
        }
    }
}
pub(in crate::app) fn delete_dialog(
    ctx: &egui::Context,
    targets: &[deletion::Target],
    t: &Theme,
    fresh: bool,
) -> egui::ModalResponse<(egui::Response, egui::Response)> {
    egui::Modal::new(egui::Id::unique("delete-confirmation")).show(ctx, |ui| {
        ui.set_width(480.);
        ui.heading(crate::tr!("Перемістити у кошик?", "Move to Trash?"));
        ui.add_space(8.);
        ui.label(crate::tr!("Вибрані файли та папки будуть переміщені у кошик. Папки — разом з усім вмістом.", "Selected files and folders will be moved to Trash. Folders include all their contents."));
        ui.add_space(10.);
        egui::ScrollArea::vertical().max_height(180.).show(ui, |ui| {
            for target in targets {
                ui.label(RichText::new(label(&target.path)).strong());
                ui.add(egui::Label::new(target.path.to_string_lossy()).wrap());
                ui.add_space(6.);
            }
        });
        let total = targets.iter().fold(0u64, |sum,t| sum.saturating_add(t.bytes));
        ui.label(RichText::new(crate::tr!("За останнім сканом: {}", "According to the last scan: {}", display_size(total))).strong());
        ui.add_space(8.);
        ui.label(RichText::new(crate::tr!("Повернути файли можна з кошика у Dolphin / Files. Місце на SSD звільниться після спорожнення кошика.", "You can restore files from Trash in Dolphin / Files. Disk space is freed after emptying Trash.")).size(13.).color(color(&t.muted)));
        ui.add_space(16.);
        ui.horizontal(|ui| {
            let cancel = ui.button(crate::tr!("Скасувати", "Cancel"));
            if fresh { cancel.request_focus(); }
            let yes = ui.add(egui::Button::new(RichText::new(crate::tr!("Так, перемістити у кошик", "Yes, move to Trash")).color(C::WHITE)).fill(color(&t.accent)));
            (cancel, yes)
        }).inner
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art;
    use eframe::egui::{Pos2, Rect, Vec2};
    use std::fs;
    #[test]
    fn delete_modal_defaults_to_cancel_and_requires_explicit_confirmation() {
        let path = std::env::temp_dir().join(format!("disco-modal-{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        let target = deletion::Target::capture(&path, 1024).unwrap();
        let ctx = egui::Context::default();
        art::fonts(&ctx);
        let theme = Theme::default();
        let frame = |events: Vec<egui::Event>, fresh| {
            let mut responses = None;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    responses = Some(delete_dialog(
                        ui.ctx(),
                        std::slice::from_ref(&target),
                        &theme,
                        fresh,
                    ));
                },
            );
            output.drop_without_applying_deltas();
            responses.unwrap()
        };
        let opened = frame(vec![], true);
        assert!(!opened.inner.1.clicked());
        assert!(opened.inner.0.has_focus());
        let escape = frame(
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        assert!(escape.should_close());
        assert!(!escape.inner.1.clicked());
        let stable = frame(vec![], false);
        let pos = stable.inner.1.rect.center();
        let pressed = frame(
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            false,
        );
        assert!(!pressed.inner.1.clicked());
        let released = frame(
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        assert!(released.inner.1.clicked());
        assert!(path.exists()); // Drawing and interacting with a dialog alone cannot delete.
        fs::remove_dir_all(path).unwrap();
    }
}
