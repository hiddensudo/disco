use crate::{
    settings::Layout,
    shortcuts::{Binding, Command},
};
use eframe::egui::{self, Key};
#[derive(Default)]
pub(in crate::app) struct Recorder {
    pub command: Option<Command>,
    error: String,
    kde: Option<String>,
}
impl Recorder {
    pub fn cancel(&mut self) {
        self.command = None;
        self.error.clear();
    }
    pub fn capture(&mut self, ctx: &egui::Context, layout: &mut Layout) {
        let Some(command) = self.command else {
            return;
        };
        let key = ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    repeat: false,
                    ..
                } => Some((*key, *modifiers)),
                _ => None,
            })
        });
        if let Some((key, mods)) = key {
            ctx.input_mut(|i| {
                i.events.retain(|event| {
                    !matches!(
                        event,
                        egui::Event::Key { .. }
                            | egui::Event::Text(_)
                            | egui::Event::Paste(_)
                            | egui::Event::Copy
                            | egui::Event::Cut
                    )
                })
            });
            if key == Key::Escape {
                self.command = None;
                self.error.clear();
                return;
            }
            match layout.shortcuts.assign(command, Binding::new(key, mods)) {
                Ok(()) => {
                    self.command = None;
                    self.error.clear();
                }
                Err(error) => self.error = error,
            }
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui, layout: &mut Layout) {
        self.capture(ui.ctx(), layout);
        let kde = self.kde.get_or_insert_with(|| {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                        .join(".config")
                });
            std::fs::read_to_string(config.join("kglobalshortcutsrc")).unwrap_or_default()
        });
        ui.label(crate::tr!(
            "Натисни комбінацію праворуч, потім введи нову. Escape скасовує запис.",
            "Click a shortcut, then press the new combination. Escape cancels recording."
        ));
        ui.add_space(16.);
        for command in Command::ALL {
            let binding = layout.shortcuts.get(command);
            let label = binding.label();
            ui.horizontal(|ui| {
                ui.add_sized([180., 30.], egui::Label::new(command.label()));
                let text = if self.command == Some(command) {
                    crate::tr!("Натисни комбінацію…", "Press a shortcut…")
                } else {
                    &label
                };
                if ui
                    .push_id(("record-shortcut", command.label()), |ui| {
                        ui.add_sized([240., 32.], egui::Button::new(text))
                    })
                    .inner
                    .clicked()
                {
                    self.command = Some(command);
                    self.error.clear();
                }
            });
            if kde
                .lines()
                .filter_map(|line| line.split_once('=').map(|(_, value)| value))
                .any(|value| value.split(',').next() == Some(label.as_str()))
            {
                ui.colored_label(
                    egui::Color32::from_rgb(227, 179, 104),
                    crate::tr!(
                        "Ця комбінація також призначена глобально в KDE.",
                        "This shortcut is also assigned globally in KDE."
                    ),
                );
            }
            ui.add_space(10.);
        }
        if !self.error.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(228, 90, 82), &self.error);
        }
        ui.add_space(10.);
        if ui
            .button(crate::tr!("Скинути гарячі клавіші", "Reset shortcuts"))
            .clicked()
        {
            layout.shortcuts = Default::default();
            self.command = None;
            self.error.clear();
        }
        ui.add_space(16.);
        ui.label(crate::tr!("Команди локальні для вікна disco. Якщо система перехоплює комбінацію (наприклад, Ctrl+Alt+T відкриває Kitty), обери іншу тут або зміни її в системних налаштуваннях. disco не змінює глобальні хоткеї.", "Commands are local to the disco window. If the system intercepts a shortcut (for example Ctrl+Alt+T opens Kitty), choose another here or change the system binding. disco does not modify global shortcuts."));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Modifiers;
    #[test]
    fn recorder_consumes_keys_rejects_conflicts_and_escape_cancels_only_capture() {
        let ctx = egui::Context::default();
        let mut layout = Layout::default();
        let mut recorder = Recorder::default();
        let capture = |key, mods, recorder: &mut Recorder, layout: &mut Layout| {
            ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: mods,
                    }],
                    ..Default::default()
                },
                |ui| {
                    recorder.capture(ui.ctx(), layout);
                    assert!(ui.input(|i| i.events.is_empty()));
                },
            )
            .drop_without_applying_deltas();
        };
        recorder.command = Some(Command::Focus);
        capture(
            Key::T,
            Modifiers::CTRL | Modifiers::ALT,
            &mut recorder,
            &mut layout,
        );
        assert!(recorder.command.is_some());
        assert!(!recorder.error.is_empty());
        capture(Key::F9, Modifiers::NONE, &mut recorder, &mut layout);
        assert!(recorder.command.is_none());
        assert_eq!(layout.shortcuts.focus.key, "F9");
        recorder.command = Some(Command::Terminal);
        capture(Key::Escape, Modifiers::NONE, &mut recorder, &mut layout);
        assert!(recorder.command.is_none());
        assert_eq!(
            layout.shortcuts.terminal,
            crate::shortcuts::Shortcuts::default().terminal
        );
    }
}
