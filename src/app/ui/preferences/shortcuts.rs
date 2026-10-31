use crate::{
    settings::Layout,
    shortcuts::{Binding, Command},
};
use eframe::egui::{self, Key, Modifiers};
#[derive(Default)]
pub(in crate::app) struct Recorder {
    pub command: Option<Command>,
    error: String,
    kde: Option<String>,
    held: Modifiers,
}
impl Recorder {
    pub fn cancel(&mut self) {
        self.command = None;
        self.error.clear();
        self.held = Modifiers::NONE;
    }
    pub fn capture(&mut self, ctx: &egui::Context, layout: &mut Layout) {
        let Some(command) = self.command else {
            return;
        };
        if !ctx.input(|i| i.focused) {
            self.cancel();
            return;
        }
        self.held = ctx.input(|i| i.modifiers);
        let key = ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    repeat: false,
                    ..
                } if !crate::shortcuts::modifier_key(*key) => Some((*key, *modifiers)),
                _ => None,
            })
        });
        // Suppress releases/repeats and text/clipboard events too, even while
        // only modifiers are held. Recording must never execute a UI command.
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
        if let Some((key, mods)) = key {
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
            let waiting = format!(
                "{}{}{}{}",
                if self.held.ctrl { "Ctrl+" } else { "" },
                if self.held.alt { "Alt+" } else { "" },
                if self.held.shift { "Shift+" } else { "" },
                crate::tr!("Натисни клавішу…", "Press a key…")
            );
            ui.horizontal(|ui| {
                ui.add_sized([180., 30.], egui::Label::new(command.label()));
                let text = if self.command == Some(command) {
                    &waiting
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

#[cfg(test)]
mod modifier_tests {
    use super::*;
    #[test]
    fn modifier_sequences_wait_for_a_main_key_and_focus_loss_cancels() {
        let ctx = egui::Context::default();
        let mut layout = Layout::default();
        let mut recorder = Recorder::default();
        let frame = |mut events: Vec<egui::Event>,
                     mods,
                     focused,
                     recorder: &mut Recorder,
                     layout: &mut Layout| {
            events.insert(0, egui::Event::ModifiersChanged(mods));
            ctx.run_ui(
                egui::RawInput {
                    events,
                    focused,
                    ..Default::default()
                },
                |ui| {
                    recorder.capture(ui.ctx(), layout);
                    assert!(!ui.input(|i| {
                        i.events
                            .iter()
                            .any(|e| matches!(e, egui::Event::Key { .. } | egui::Event::Text(_)))
                    }));
                },
            )
            .drop_without_applying_deltas();
        };
        let event = |key, mods, pressed| egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers: mods,
        };
        for mods in [
            Modifiers::CTRL | Modifiers::SHIFT,
            Modifiers::ALT | Modifiers::SHIFT,
            Modifiers::CTRL | Modifiers::ALT,
            Modifiers::CTRL | Modifiers::ALT | Modifiers::SHIFT,
        ] {
            for keys in [
                [Key::ControlLeft, Key::AltLeft, Key::ShiftLeft],
                [Key::ShiftRight, Key::AltRight, Key::ControlRight],
            ] {
                recorder.command = Some(Command::Focus);
                let before = layout.shortcuts.clone();
                for key in keys {
                    frame(
                        vec![event(key, mods, true)],
                        mods,
                        true,
                        &mut recorder,
                        &mut layout,
                    );
                    assert_eq!(recorder.command, Some(Command::Focus));
                    assert_eq!(layout.shortcuts, before);
                }
                frame(
                    vec![event(Key::H, mods, true)],
                    mods,
                    true,
                    &mut recorder,
                    &mut layout,
                );
                assert_eq!(recorder.command, None);
                assert_eq!(layout.shortcuts.focus, Binding::new(Key::H, mods));
                recorder.command = Some(Command::Focus);
                frame(
                    vec![
                        event(Key::H, mods, false),
                        event(Key::ControlRight, mods, false),
                    ],
                    mods,
                    true,
                    &mut recorder,
                    &mut layout,
                );
                assert!(recorder.command.is_some());
                frame(vec![], Modifiers::NONE, false, &mut recorder, &mut layout);
                assert!(recorder.command.is_none());
            }
        }
        recorder.command = Some(Command::Focus);
        frame(
            vec![event(Key::Num1, Modifiers::CTRL, true)],
            Modifiers::CTRL,
            true,
            &mut recorder,
            &mut layout,
        );
        assert_eq!(
            layout.shortcuts.focus,
            Binding::new(Key::Num1, Modifiers::CTRL)
        );
        let legacy: Layout = toml::from_str("compact=true\n[shortcuts.focus]\nkey='ControlLeft'\nctrl=true\nalt=true\nshift=false\n").unwrap();
        assert!(legacy.compact);
        legacy.validate().unwrap();
        assert!(!crate::shortcuts::modifier_key(
            Key::from_name(&legacy.shortcuts.focus.key).unwrap()
        ));
    }
}
