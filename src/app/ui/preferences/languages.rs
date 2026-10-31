use crate::{app::Disco, lsp::manager::Action};
use eframe::egui;
impl Disco {
    pub(super) fn language_preferences(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 8.;
        ui.label(crate::tr!("Підказки: Ctrl+Space · прийняти: Enter/Tab · закрити: Esc. Наведи курсор на код для довідки.","Completion: Ctrl+Space · accept: Enter/Tab · dismiss: Esc. Hover over code for documentation."));
        ui.small(crate::tr!("Сервер запускається лише для ввімкненої мови у відкритому проєкті. Без відкритих документів завершується через 30 с.","Servers start only for an enabled language in an open project and exit after 30 s without documents."));
        ui.horizontal_wrapped(|ui| {
            ui.label("lsp.toml");
            ui.monospace(self.languages.manager.path().display().to_string());
        });
        if self.languages.manager.busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(&self.languages.manager.operation);
                if ui
                    .button(crate::tr!("Скасувати операцію", "Cancel operation"))
                    .clicked()
                {
                    self.languages.manager.cancel_job();
                }
            });
        }
        if !self.languages.manager.error.is_empty() {
            ui.colored_label(
                ui.visuals().error_fg_color,
                crate::tr!(
                    "Помилка LSP: {e}",
                    "LSP error: {e}",
                    e = self.languages.manager.error
                ),
            );
        }
        if !self.languages.notice.is_empty() {
            ui.label(&self.languages.notice);
        }
        let servers = self
            .languages
            .manager
            .draft
            .as_ref()
            .unwrap_or(&self.languages.manager.catalog)
            .servers
            .clone();
        let mut action = None;
        let mut control = None;
        for (id, server) in servers {
            ui.separator();
            ui.horizontal(|ui| {
                let mut enabled = server.enabled;
                if ui.checkbox(&mut enabled, &server.name).changed() {
                    self.languages
                        .manager
                        .draft
                        .as_mut()
                        .unwrap()
                        .servers
                        .get_mut(&id)
                        .unwrap()
                        .enabled = enabled;
                }
                ui.small(server.extensions.join(" · "));
            });
            ui.small(format!(
                "{} · {} {}",
                server.provider, server.repository, server.package
            ));
            if server.provider == "npm" {
                ui.small(crate::tr!(
                    "Потрібні Node.js і npm; розмір включає залежності.",
                    "Requires Node.js and npm; size includes dependencies."
                ));
            }
            if server.provider == "system" || server.provider == "rustup" {
                ui.small(crate::tr!(
                    "Зовнішня установка: керування через системний менеджер пакетів / rustup.",
                    "External installation: manage through your system package manager / rustup."
                ));
            }
            ui.label(self.languages.manager.installed(&id));
            if let Some(update) = self.languages.manager.updates.get(&id) {
                ui.label(crate::tr!(
                    "Доступна версія: {v}",
                    "Available version: {v}",
                    v = update.version
                ));
            }
            if let Some(message) = self.languages.manager.messages.get(&id) {
                ui.small(message);
            }
            ui.add_enabled_ui(!self.languages.manager.busy(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (label, a) in [
                        (
                            crate::tr!("Перевірити установку", "Check installation"),
                            Action::Probe,
                        ),
                        (
                            crate::tr!("Перевірити оновлення", "Check updates"),
                            Action::Check,
                        ),
                        (
                            if self.languages.manager.updates.contains_key(&id) {
                                crate::tr!("Оновити", "Update")
                            } else {
                                crate::tr!("Встановити", "Install")
                            },
                            Action::Install,
                        ),
                        (crate::tr!("Відкотити", "Roll back"), Action::Rollback),
                        (crate::tr!("Видалити", "Remove"), Action::Remove),
                    ] {
                        let external = matches!(server.provider.as_str(), "system" | "rustup");
                        if ui
                            .add_enabled(
                                (!external || matches!(a, Action::Probe))
                                    && match a {
                                        Action::Install => self.languages.manager.can_install(&id),
                                        Action::Rollback => {
                                            self.languages.manager.can_rollback(&id)
                                        }
                                        Action::Remove => {
                                            self.languages.manager.owned_installation(&id)
                                        }
                                        _ => true,
                                    },
                                egui::Button::new(label),
                            )
                            .clicked()
                        {
                            action = Some((id.clone(), a));
                        }
                    }
                });
            });
            let sessions: Vec<_> = self
                .languages
                .sessions
                .iter()
                .filter(|s| s.server == id)
                .cloned()
                .collect();
            for session in sessions {
                ui.label(format!(
                    "{} · PID {} · {:.1} MiB · {} {}",
                    session.root.display(),
                    session.pid,
                    session.rss as f64 / 1048576.,
                    session.documents,
                    crate::tr!("док.", "docs")
                ));
                ui.small(format!(
                    "CPU {:.2} s",
                    session.cpu_ticks as f64
                        / unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64
                ));
                ui.small(if session.ready {
                    crate::tr!("Запущено", "Running")
                } else {
                    crate::tr!("Ініціалізація…", "Initializing…")
                });
                if !session.compile_config {
                    ui.colored_label(ui.visuals().warn_fg_color,crate::tr!("Немає compile_commands.json / compile_flags.txt: clangd використовує стандартні прапорці.","No compile_commands.json / compile_flags.txt: clangd uses fallback flags."));
                }
            }
            ui.add_enabled_ui(!self.languages.manager.busy(), |ui| {
                ui.horizontal(|ui| {
                    if ui.button(crate::tr!("Зупинити", "Stop")).clicked() {
                        control = Some((id.clone(), false));
                    }
                    if ui.button(crate::tr!("Перезапустити", "Restart")).clicked() {
                        control = Some((id.clone(), true));
                    }
                });
            });
        }
        if let Some((id, restart)) = control {
            if let Some(c) = &self.languages.client {
                if restart {
                    c.restart(&id);
                } else {
                    let _ = c.stop(&id);
                }
            }
        }
        if let Some((id, a)) = action {
            let ack = if matches!(a, Action::Remove | Action::Rollback) {
                self.languages.client.as_ref().map(|c| c.stop(&id))
            } else {
                None
            };
            self.languages.manager.start(&id, a, ack, ui.ctx());
        }
    }
}
