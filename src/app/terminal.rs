use super::{Disco, state::View, workspace_keyboard::Area};
use crate::{
    art,
    settings::{PreviewMode, color},
};
use eframe::egui::{self, FontFamily, FontId, Id, Rect, Sense, Stroke, Vec2};
use egui_term::{FontSettings, TerminalFont, TerminalView};
use std::path::PathBuf;
impl Disco {
    pub(in crate::app) fn terminal_shown(&self) -> bool {
        self.terminal.visible
            && self.preferences.layout.preview_mode == PreviewMode::Focus
            && self.browser.view != View::Trash
    }
    pub(in crate::app) fn terminal_focused(&self, ctx: &egui::Context) -> bool {
        self.terminal_shown() && ctx.memory(|m| m.focused()) == Some(Id::unique("terminal-view"))
    }
    pub(in crate::app) fn toggle_terminal(&mut self, ctx: &egui::Context) {
        if self.terminal_shown() {
            self.terminal.hide();
            self.focus_area(Area::Editor, ctx);
        } else {
            if !self.terminal.fonts_loaded {
                art::terminal_fonts(ctx);
                self.terminal.fonts_loaded = true;
            }
            let path = self
                .browser
                .properties
                .as_ref()
                .map(|p| {
                    if p.is_dir {
                        p.path.clone()
                    } else {
                        p.path.parent().unwrap_or(&p.path).into()
                    }
                })
                .or_else(|| {
                    self.scan
                        .report
                        .as_ref()
                        .and_then(|r| r.folders.get(self.browser.focus))
                        .map(|f| f.path.clone())
                })
                .unwrap_or_else(|| PathBuf::from(&self.browser.root));
            let path = if path.is_dir() {
                path
            } else {
                self.preferences.layout.home_directory()
            };
            if let Err(e) = self.terminal.open(ctx, &path) {
                self.terminal.error = crate::tr!(
                    "Не вдалося відкрити термінал: {e}",
                    "Could not open terminal: {e}",
                    e = e
                );
                self.terminal.visible = true;
            }
            self.focus_area(Area::Terminal, ctx);
        }
        ctx.request_repaint();
    }
    pub(in crate::app) fn terminal_button(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let t = self.preferences.theme.clone();
        let enabled = self.preferences.layout.preview_mode == PreviewMode::Focus
            && self.browser.view != View::Trash;
        let response = ui.interact(
            rect,
            Id::unique("toggle-terminal"),
            if enabled {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        ui.scope(|ui| {
            if !enabled {
                ui.multiply_opacity(0.4);
            }
            ui.painter().rect_filled(rect, t.radius, color(&t.raised));
            ui.painter().rect_stroke(
                rect,
                t.radius,
                Stroke::new(
                    1.,
                    color(if self.terminal_shown() {
                        &t.accent
                    } else {
                        &t.border
                    }),
                ),
                egui::StrokeKind::Inside,
            );
            // Vector terminal glyph; no font glyph dependencies.
            let x = rect.left() + 12.;
            let y = rect.center().y;
            ui.painter().line_segment(
                [egui::pos2(x, y - 4.), egui::pos2(x + 4., y)],
                Stroke::new(1.4, color(&t.text)),
            );
            ui.painter().line_segment(
                [egui::pos2(x + 4., y), egui::pos2(x, y + 4.)],
                Stroke::new(1.4, color(&t.text)),
            );
            ui.painter()
                .hline(x + 7. ..=x + 12., y + 4., Stroke::new(1.4, color(&t.text)));
            ui.painter().text(
                egui::pos2(rect.left() + 33., y),
                egui::Align2::LEFT_CENTER,
                crate::tr!("Термінал", "Terminal"),
                FontId::proportional(13.),
                color(&t.text),
            );
        });
        if enabled && response.clicked() {
            self.toggle_terminal(ui.ctx());
        }
        response.on_hover_text(if enabled {
            crate::tr!(
                "Відкрити або сховати термінал · сесія зберігається",
                "Open or hide terminal · session stays alive"
            )
        } else {
            crate::tr!(
                "Термінал наразі доступний у режимі Фокус",
                "Terminal is currently available in Focus"
            )
        });
    }
    pub(in crate::app) fn terminal_panel(&mut self, ui: &mut egui::Ui) {
        if !self.terminal_shown() {
            return;
        }
        let t = self.preferences.theme.clone();
        let mut close = false;
        let mut hide = false;
        let max = (ui.available_width() - 520.).max(300.);
        let panel = egui::Panel::right("terminal-panel")
            .default_size(460.)
            .min_size(300.)
            .max_size(max)
            .resizable(true)
            .frame(
                egui::Frame::new()
                    .fill(color(&t.background))
                    .inner_margin(egui::Margin::symmetric(10, 8)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(crate::tr!("Термінал", "Terminal"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.), Sense::hover());
                        let r = ui.interact(rect, Id::unique("close-terminal"), Sense::click());
                        self.art.paint(ui, 21, rect.shrink(6.), color(&t.text));
                        close = r
                            .on_hover_text(crate::tr!(
                                "Завершити сесію та всі її процеси",
                                "End the session and all its processes"
                            ))
                            .clicked();
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.), Sense::hover());
                        let r = ui.interact(rect, Id::unique("hide-terminal"), Sense::click());
                        self.art.paint(ui, 19, rect.shrink(6.), color(&t.muted));
                        hide = r
                            .on_hover_text(crate::tr!(
                                "Сховати, зберігши сесію",
                                "Hide and keep the session alive"
                            ))
                            .clicked();
                    });
                });
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(self.terminal.cwd.display().to_string())
                            .small()
                            .color(color(&t.muted)),
                    )
                    .truncate(),
                )
                .on_hover_text(self.terminal.cwd.display().to_string());
                ui.separator();
                if !self.terminal.error.is_empty() {
                    ui.colored_label(color(&t.accent), &self.terminal.error);
                } else if self.terminal.exited {
                    ui.label(crate::tr!("Сесію завершено", "Session ended"));
                    if ui.button(crate::tr!("Нова сесія", "New session")).clicked() {
                        self.terminal.close();
                        self.toggle_terminal(ui.ctx());
                    }
                } else {
                    let rect = ui.available_rect_before_wrap();
                    if ui.input(|i| {
                        i.pointer.primary_pressed()
                            && i.pointer.interact_pos().is_some_and(|p| rect.contains(p))
                    }) {
                        self.focus_area(Area::Terminal, ui.ctx());
                    }
                    let theme = self.terminal.theme(&t);
                    let focused = self.keyboard.area == Area::Terminal
                        && !(self.preferences.open
                            || self.editor.pending.is_some()
                            || self.deletion.confirm
                            || self.new_file.is_some()
                            || egui::Popup::is_any_open(ui.ctx()));
                    let family = FontFamily::Name("Terminal".into());
                    let family = if ui.fonts_mut(|f| f.definitions().families.contains_key(&family))
                    {
                        family
                    } else {
                        FontFamily::Monospace
                    };
                    if let Some(backend) = self.terminal.backend() {
                        let view = TerminalView::new(ui, backend)
                            .id(Id::unique("terminal-view"))
                            .set_focus(focused)
                            .set_theme(theme)
                            .set_font(TerminalFont::new(FontSettings {
                                font_type: FontId::new(
                                    14. * self.preferences.layout.text_scale,
                                    family,
                                ),
                            }))
                            .set_size(rect.size());
                        ui.add(view);
                    }
                }
            });
        ui.interact(
            panel.response.rect,
            Id::unique("terminal-panel-bounds"),
            Sense::hover(),
        );
        ui.painter().vline(
            panel.response.rect.left(),
            panel.response.rect.y_range(),
            Stroke::new(1., color(&t.border)),
        );
        if close {
            self.terminal.close();
            self.focus_area(Area::Editor, ui.ctx());
            ui.ctx().request_repaint();
        } else if hide {
            self.terminal.hide();
            self.focus_area(Area::Editor, ui.ctx());
            ui.ctx().request_repaint();
        }
    }
}
