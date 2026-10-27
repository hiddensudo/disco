use crate::app::Disco;
use crate::app::ui::widgets::{logo, mix, paint_logo, visual_button};
use crate::art;
use crate::i18n;
use crate::settings;
use crate::settings::{Layout, Theme, color};
use eframe::egui::{self, Align2, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
mod appearance;
mod interface;

impl Disco {
    pub(in crate::app) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.preferences.open {
            return;
        }
        let mut opened = true;
        let mut save = false;
        let mut cancel = false;
        let mut theme = self
            .preferences
            .draft
            .clone()
            .unwrap_or_else(|| self.preferences.theme.clone());
        let mut layout = self.preferences.layout.clone();
        let mut action: Option<(&str, bool)> = None;
        let t = self.preferences.theme.clone();
        egui::Window::new("disco-preferences")
            .fade_in(self.preferences.layout.animations)
            .fade_out(self.preferences.layout.animations)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .fixed_size([850., 545.])
            .frame(
                egui::Frame::new()
                    .fill(color(&t.panel))
                    .stroke(Stroke::new(1., color(&t.border)))
                    .corner_radius(t.radius)
                    .inner_margin(0.),
            )
            .anchor(Align2::CENTER_CENTER, [0., 0.])
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                let (window_rect, _) =
                    ui.allocate_exact_size(Vec2::new(850., 545.), Sense::hover());
                let head = Rect::from_min_size(window_rect.min, Vec2::new(850., 40.));
                ui.painter().rect_filled(
                    head,
                    t.radius,
                    mix(color(&t.panel), color(&t.raised), 0.3),
                );
                paint_logo(
                    ui,
                    Rect::from_min_size(head.min + Vec2::new(14., 10.), Vec2::splat(20.)),
                    color(&t.accent),
                );
                ui.painter().text(
                    head.min + Vec2::new(44., 20.),
                    Align2::LEFT_CENTER,
                    format!(
                        "disco — {}",
                        if self.preferences.page == 0 {
                            crate::tr!("Оформлення", "Appearance")
                        } else if self.preferences.page == 1 {
                            crate::tr!("Інтерфейс", "Interface")
                        } else {
                            crate::tr!("Про програму", "About")
                        }
                    ),
                    art::bold(15.),
                    color(&t.text),
                );
                ui.painter().hline(
                    head.x_range(),
                    head.bottom(),
                    Stroke::new(1., color(&t.border)),
                );
                ui.scope_builder(
                    egui::UiBuilder::new().max_rect(Rect::from_min_size(
                        Pos2::new(head.right() - 300., head.top() + 5.),
                        Vec2::new(245., 30.),
                    )),
                    |ui| {
                        ui.horizontal(|ui| {
                            ui.label(crate::tr!("Мова", "Language"));
                            language_picker(ui, &mut layout.language);
                        });
                    },
                );
                let close = Rect::from_center_size(
                    Pos2::new(head.right() - 22., head.center().y),
                    Vec2::splat(30.),
                );
                self.art.paint(ui, 21, close.shrink(8.), color(&t.muted));
                if ui
                    .interact(
                        close,
                        ui.scope_id().with("close_preferences"),
                        Sense::click(),
                    )
                    .clicked()
                {
                    opened = false;
                }
                let body = Rect::from_min_size(
                    window_rect.min + Vec2::new(0., 40.),
                    Vec2::new(850., 386.),
                );
                let nav = Rect::from_min_size(body.min, Vec2::new(153., body.height()));
                ui.painter().vline(
                    nav.right(),
                    nav.y_range(),
                    Stroke::new(1., color(&t.border)),
                );
                for (i, name, icon) in [
                    (0, crate::tr!("Оформлення", "Appearance"), 6),
                    (1, crate::tr!("Інтерфейс", "Interface"), 7),
                    (2, crate::tr!("Про програму", "About"), 17),
                ] {
                    let r = Rect::from_min_size(
                        nav.min + Vec2::new(0., 13. + i as f32 * 45.),
                        Vec2::new(153., 41.),
                    );
                    let b = ui.interact(r, ui.scope_id().with(("settings_nav", i)), Sense::click());
                    let active = self.preferences.page == i;
                    if active || b.hovered() {
                        ui.painter().rect_filled(r, 0, color(&t.raised));
                    }
                    if active {
                        ui.painter().rect_filled(
                            Rect::from_min_size(r.min, Vec2::new(3., r.height())),
                            0,
                            color(&t.accent),
                        );
                    }
                    self.art.paint(
                        ui,
                        icon,
                        Rect::from_min_size(r.min + Vec2::new(15., 10.), Vec2::splat(20.)),
                        color(&t.text),
                    );
                    ui.painter().text(
                        r.min + Vec2::new(45., 20.),
                        Align2::LEFT_CENTER,
                        name,
                        FontId::proportional(13.),
                        color(&t.text),
                    );
                    if b.clicked() {
                        self.preferences.page = i;
                    }
                }
                let content = Rect::from_min_max(
                    body.min + Vec2::new(174., 15.),
                    body.max - Vec2::new(20., 5.),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(content)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                    |ui| {
                        ui.style_mut()
                            .text_styles
                            .insert(egui::TextStyle::Body, FontId::proportional(14.));
                        ui.style_mut()
                            .text_styles
                            .insert(egui::TextStyle::Button, FontId::proportional(13.));
                        ui.label(
                            RichText::new(if self.preferences.page == 0 {
                                crate::tr!("Оформлення", "Appearance")
                            } else if self.preferences.page == 1 {
                                crate::tr!("Інтерфейс", "Interface")
                            } else {
                                crate::tr!("Про програму", "About")
                            })
                            .font(art::bold(19.)),
                        );
                        ui.add_space(15.);
                        if self.preferences.page < 2 {
                            egui::ScrollArea::vertical()
                                .id_salt("preferences_body")
                                .max_height(317.)
                                .show(ui, |ui| {
                                    ui.columns(2, |cols| {
                                        cols[0].spacing_mut().item_spacing.y =
                                            if self.preferences.page == 0 { 10. } else { 4. };
                                        if self.preferences.page == 0 {
                                            appearance::show(
                                                cols, &self.art, &mut theme, &layout, &t,
                                            );
                                        } else {
                                            interface::show(
                                                cols,
                                                &self.art,
                                                &theme,
                                                &mut layout,
                                                &t,
                                                &self.drive.name,
                                            );
                                        }
                                    });
                                });
                        } else {
                            ui.add_space(10.);
                            ui.horizontal(|ui| {
                                logo(ui, 45., color(&t.accent));
                                ui.label(
                                    RichText::new(format!("disco {}", env!("CARGO_PKG_VERSION")))
                                        .font(art::bold(25.)),
                                );
                            });
                            ui.add_space(18.);
                            ui.label(crate::tr!(
                                "Аналізатор дискового простору",
                                "Disk space analyzer"
                            ));
                            ui.label(
                                RichText::new(crate::tr!(
                                    "Папки, великі файли та контрольоване очищення.",
                                    "Folders, large files, and cleanup with confirmation."
                                ))
                                .color(color(&t.muted))
                                .size(14.),
                            );
                        }
                    },
                );
                let foot = Rect::from_min_size(
                    window_rect.min + Vec2::new(0., 426.),
                    Vec2::new(850., 119.),
                );
                ui.painter().hline(
                    foot.x_range(),
                    foot.top(),
                    Stroke::new(1., color(&t.border)),
                );
                if self.preferences.page < 2 {
                    let name = if self.preferences.page == 0 {
                        "theme.toml"
                    } else {
                        "layout.toml"
                    };
                    for (x, w, icon, text, which) in [
                        (
                            174.,
                            150.,
                            15,
                            if self.preferences.page == 0 {
                                crate::tr!("Імпортувати тему", "Import theme")
                            } else {
                                crate::tr!("Імпортувати", "Import")
                            }
                            .to_string(),
                            0,
                        ),
                        (
                            333.,
                            258.,
                            15,
                            crate::tr!("Експортувати {name}", "Export {name}", name = name),
                            1,
                        ),
                        (
                            600.,
                            225.,
                            16,
                            crate::tr!("Скинути", "Reset").to_string(),
                            2,
                        ),
                    ] {
                        let r =
                            Rect::from_min_size(foot.min + Vec2::new(x, 12.), Vec2::new(w, 34.));
                        if visual_button(ui, &self.art, r, icon, &text, true, &t).clicked() {
                            match which {
                                0 => action = Some((name, false)),
                                1 => action = Some((name, true)),
                                _ => {
                                    if self.preferences.page == 0 {
                                        theme = Theme::default();
                                    } else {
                                        layout = Layout {
                                            language: layout.language,
                                            ..Layout::default()
                                        };
                                    }
                                }
                            }
                        }
                    }
                }
                let caption = if self.preferences.error.is_empty() {
                    crate::tr!(
                        "Збереження — кнопкою «Зберегти»",
                        "Click Save to keep your changes"
                    )
                } else {
                    &self.preferences.error
                };
                ui.painter()
                    .with_clip_rect(Rect::from_min_size(
                        foot.min + Vec2::new(174., 58.),
                        Vec2::new(360., 42.),
                    ))
                    .text(
                        foot.min + Vec2::new(174., 77.),
                        Align2::LEFT_CENTER,
                        caption,
                        FontId::proportional(11.),
                        color(&t.muted),
                    );
                let back =
                    Rect::from_min_size(foot.min + Vec2::new(600., 60.), Vec2::new(100., 32.));
                let accept =
                    Rect::from_min_size(foot.min + Vec2::new(710., 60.), Vec2::new(115., 32.));
                cancel = visual_button(
                    ui,
                    &self.art,
                    back,
                    usize::MAX,
                    crate::tr!("Скасувати", "Cancel"),
                    true,
                    &t,
                )
                .clicked();
                save = visual_button(
                    ui,
                    &self.art,
                    accept,
                    usize::MAX,
                    crate::tr!("Зберегти", "Save"),
                    true,
                    &t,
                )
                .clicked();
            });
        self.preferences.draft = Some(theme.clone());
        if theme != self.preferences.theme || layout != self.preferences.layout {
            match theme.validate().and_then(|_| layout.validate()) {
                Ok(()) => {
                    self.preferences.theme = theme;
                    self.preferences.layout = layout;
                    self.preferences.error.clear();
                }
                Err(e) => self.preferences.error = e,
            }
        }
        if let Some((name, export)) = action {
            let path = if export {
                rfd::FileDialog::new()
                    .add_filter("TOML", &["toml"])
                    .set_file_name(name)
                    .save_file()
            } else {
                rfd::FileDialog::new()
                    .add_filter("TOML", &["toml"])
                    .pick_file()
            };
            if let Some(p) = path {
                let result = if export {
                    if name == "theme.toml" {
                        settings::write(&p, &self.preferences.theme)
                    } else {
                        settings::write(&p, &self.preferences.layout)
                    }
                } else if name == "theme.toml" {
                    settings::read_theme(&p).map(|t| {
                        self.preferences.theme = t.clone();
                        self.preferences.draft = Some(t);
                    })
                } else {
                    settings::read_layout(&p).map(|l| self.preferences.layout = l)
                };
                self.preferences.error = match result {
                    Ok(()) => {
                        if export {
                            crate::tr!("Експортовано", "Exported").into()
                        } else {
                            crate::tr!(
                                "Імпортовано. Натисни «Зберегти» для постійного застосування.",
                                "Imported. Click Save to apply permanently."
                            )
                            .into()
                        }
                    }
                    Err(e) => e,
                };
            }
        }
        if save {
            let p = settings::config_dir();
            match self
                .preferences
                .draft
                .as_ref()
                .unwrap_or(&self.preferences.theme)
                .validate()
                .and_then(|_| self.preferences.layout.validate())
                .and_then(|_| settings::write(&p.join("theme.toml"), &self.preferences.theme))
                .and_then(|_| settings::write(&p.join("layout.toml"), &self.preferences.layout))
            {
                Ok(()) => {
                    self.preferences.open = false;
                    self.preferences.snapshot = None;
                    self.preferences.draft = None;
                    self.message = crate::tr!("Налаштування збережено", "Settings saved").into();
                }
                Err(e) => self.preferences.error = e,
            }
        }
        if cancel || !opened {
            if let Some((t, l)) = self.preferences.snapshot.take() {
                self.preferences.theme = t;
                self.preferences.layout = l;
            }
            self.preferences.open = false;
            self.preferences.draft = None;
        }
    }
}
fn language_picker(ui: &mut egui::Ui, language: &mut i18n::Language) {
    egui::ComboBox::from_id_salt("interface-language")
        .width(135.)
        .selected_text(language.name())
        .show_ui(ui, |ui| {
            for choice in [i18n::Language::Uk, i18n::Language::En] {
                ui.selectable_value(language, choice, choice.name());
            }
        });
}
