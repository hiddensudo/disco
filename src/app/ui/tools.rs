use crate::app::Disco;
use crate::app::state::View;
use crate::filesystem::{Properties, open};
use crate::format::label;
use crate::scan::human;
use crate::settings::color;
use eframe::egui::{self, RichText};

impl Disco {
    pub(in crate::app) fn other_content(&mut self, ui: &mut egui::Ui) {
        let t = self.preferences.theme.clone();
        match self.browser.view {
            View::Files => {
                ui.heading(crate::tr!("Великі файли", "Large files"));
                ui.label(
                    RichText::new(crate::tr!(
                        "300 найбільших файлів у вибраній області",
                        "The 300 largest files in the scanned area"
                    ))
                    .color(color(&t.muted))
                    .size(12.),
                );
                let files = self
                    .scan
                    .report
                    .as_ref()
                    .map(|r| r.largest.clone())
                    .unwrap_or_default();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for f in files {
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(
                                    self.browser
                                        .properties
                                        .as_ref()
                                        .map(|p| p.path == f.path)
                                        .unwrap_or(false),
                                    label(&f.path),
                                )
                                .on_hover_text(f.path.to_string_lossy())
                                .clicked()
                            {
                                self.browser.properties = Some(Properties::load(
                                    f.path.clone(),
                                    f.bytes,
                                    None,
                                    self.demo,
                                ));
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(human(f.bytes));
                                },
                            );
                        });
                        ui.separator();
                    }
                });
            }
            View::Cleanup => {
                ui.heading(crate::tr!("Очищення", "Cleanup"));
                ui.label(
                    RichText::new(crate::tr!(
                        "Вибери відомі кеші. Закрий відповідну програму перед очищенням.",
                        "Select known caches. Close the corresponding app before cleanup."
                    ))
                    .color(color(&t.muted))
                    .size(12.),
                );
                ui.label(
                    RichText::new(crate::tr!(
                        "Кошик не звільняє місце до його спорожнення.",
                        "Trash uses disk space until it is emptied."
                    ))
                    .color(color(&t.muted))
                    .size(12.),
                );
                let candidates = self.candidates();
                if candidates.is_empty() {
                    ui.add_space(20.);
                    ui.label(crate::tr!(
                        "Кандидатів немає. Проскануй домашню папку або .cache.",
                        "No candidates. Scan your home folder or .cache."
                    ));
                }
                for f in candidates {
                    ui.horizontal(|ui| {
                        let mut selected = self.browser.selected.contains(&f.path);
                        if ui
                            .add_enabled(
                                !self.scan.busy
                                    && !self.demo
                                    && self.deletion.rx.is_none()
                                    && !self.recycle.busy,
                                egui::Checkbox::new(&mut selected, label(&f.path)),
                            )
                            .changed()
                        {
                            if selected {
                                self.browser.selected.insert(f.path.clone());
                            } else {
                                self.browser.selected.remove(&f.path);
                            }
                        }
                        ui.label(human(f.bytes));
                        if ui
                            .small_button(crate::tr!("Властивості", "Properties"))
                            .clicked()
                        {
                            self.browser.properties = Some(Properties::load(
                                f.path.clone(),
                                f.bytes,
                                Some(f.files),
                                self.demo,
                            ));
                        }
                    });
                    ui.label(
                        RichText::new(f.path.to_string_lossy())
                            .size(11.)
                            .color(color(&t.muted)),
                    );
                    ui.separator();
                }
            }
            View::Duplicates => {
                ui.heading(crate::tr!("Дублікати", "Duplicates"));
                ui.label(
                    RichText::new(crate::tr!(
                        "Повне порівняння вмісту від 1 МіБ. Окреме читання диска.",
                        "Full content comparison for files from 1 MiB. Reads the disk separately."
                    ))
                    .size(12.)
                    .color(color(&t.muted)),
                );
                if ui
                    .add_enabled(
                        !self.scan.busy && self.deletion.rx.is_none() && !self.recycle.busy,
                        egui::Button::new(crate::tr!("Знайти дублікати", "Find duplicates")),
                    )
                    .clicked()
                {
                    self.start(true, true);
                }
                ui.label(
                    RichText::new(crate::tr!(
                        "Тут лише перегляд. Hardlink не рахується дублікатом.",
                        "Preview only. Hardlinks are not counted as duplicates."
                    ))
                    .size(12.)
                    .color(color(&t.muted)),
                );
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for g in &self.scan.dups {
                        ui.add_space(10.);
                        ui.label(
                            RichText::new(crate::tr!(
                                "{} копій × {}",
                                "{} copies × {}",
                                g.len(),
                                human(g[0].bytes)
                            ))
                            .strong(),
                        );
                        for f in g {
                            if ui.link(f.path.to_string_lossy()).clicked() {
                                if let Some(p) = f.path.parent() {
                                    open(p);
                                }
                            }
                        }
                        ui.separator();
                    }
                });
            }
            _ => {}
        }
    }
}
