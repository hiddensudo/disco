//! A single in-context action, instead of scan buttons above an empty file tree.
use crate::{app::Disco, index::Category, settings::color};
use eframe::egui::{self, Align, FontId, Rect, RichText, Sense, Vec2};
impl Disco {
    pub(in crate::app) fn browser_empty_state(&mut self, ui: &mut egui::Ui, outer: Rect) {
        let t = self.preferences.theme.clone();
        let restoring = self.scan.restore_requested_root.is_some() || self.paths.snapshot.loading();
        let scanning = self.scan.busy || self.scoped.busy();
        let collection = self.collections.config.get(self.browser.category).cloned();
        let needs_folders = collection.as_ref().is_some_and(|c| {
            c.folders.is_empty()
                && (!c.builtin() || self.scan.report.is_none() && c.category() != Category::All)
        });
        let can_scan = self.scan.report.is_none()
            || collection.as_ref().is_some_and(|c| !c.folders.is_empty());
        let title = if restoring {
            crate::tr!("Підвантажуємо папки…", "Loading folders…")
        } else if scanning {
            crate::tr!("Скануємо папки…", "Scanning folders…")
        } else if needs_folders {
            crate::tr!("Додай папки до категорії", "Add folders to this category")
        } else if can_scan {
            crate::tr!(
                "Папки ще не проскановано",
                "Folders have not been scanned yet"
            )
        } else {
            crate::tr!(
                "У цій категорії поки немає папок",
                "No folders in this category yet"
            )
        };
        let detail = if restoring {
            crate::tr!(
                "Відновлюємо збережений список. Це не нове сканування.",
                "Restoring the saved list, without running a new scan."
            )
        } else if scanning {
            crate::tr!(
                "Підраховуємо розміри. Документ залишається відкритим.",
                "Calculating sizes. Your document stays open."
            )
        } else if needs_folders {
            crate::tr!(
                "Вибери папки, які мають тут відображатися.",
                "Choose the folders you want to see here."
            )
        } else if can_scan {
            crate::tr!(
                "Сканування покаже папки та їхні розміри.",
                "Scan to see folders and their sizes."
            )
        } else {
            crate::tr!(
                "Вибери іншу категорію або додай папки в налаштуваннях.",
                "Choose another category or add folders in Settings."
            )
        };
        let width = (outer.width() - 32.).max(80.);
        let height = 240_f32.min((outer.height() - 20.).max(100.));
        let rect = Rect::from_center_size(outer.center(), Vec2::new(width, height));
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(Align::Center)),
            |ui| {
                ui.set_width(width);
                let (icon, _) = ui.allocate_exact_size(Vec2::splat(38.), Sense::hover());
                self.art.paint(ui, 0, icon, color(&t.muted));
                ui.add_space(10.);
                if restoring || scanning {
                    let spinner = ui.add(egui::Spinner::new().size(14.).color(color(&t.muted)));
                    ui.interact(
                        spinner.rect,
                        egui::Id::unique("folder-loading-indicator"),
                        Sense::hover(),
                    );
                    ui.add_space(5.);
                }
                ui.add(
                    egui::Label::new(
                        RichText::new(title)
                            .font(FontId::proportional(15.))
                            .color(color(&t.text)),
                    )
                    .wrap(),
                );
                ui.add_space(6.);
                let response = ui.add(
                    egui::Label::new(RichText::new(detail).size(12.).color(color(&t.muted))).wrap(),
                );
                ui.interact(
                    response.rect,
                    egui::Id::unique("folder-empty-description"),
                    Sense::hover(),
                );
                if !restoring && !scanning && (can_scan || needs_folders) {
                    let path = collection
                        .as_ref()
                        .and_then(|c| {
                            (c.folders.len() == 1).then(|| c.folders[0].display().to_string())
                        })
                        .unwrap_or_else(|| {
                            if self.browser.category == Category::All {
                                self.browser.root.clone()
                            } else {
                                collection
                                    .as_ref()
                                    .map(|c| c.title().to_string())
                                    .unwrap_or_default()
                            }
                        });
                    if !path.is_empty() {
                        ui.add_space(6.);
                        let caption = if std::path::Path::new(&path).is_absolute() {
                            std::path::Path::new(&path)
                                .file_name()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path.clone())
                        } else {
                            path.clone()
                        };
                        ui.add(
                            egui::Label::new(
                                RichText::new(&caption).size(11.).color(color(&t.muted)),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&path);
                    }
                    ui.add_space(12.);
                    let (button, _) =
                        ui.allocate_exact_size(Vec2::new(width.min(170.), 34.), Sense::hover());
                    let response = super::widgets::visual_button(
                        ui,
                        &self.art,
                        button,
                        if needs_folders { 30 } else { 11 },
                        if needs_folders {
                            crate::tr!("Додати папки", "Add folders")
                        } else {
                            crate::tr!("Сканувати", "Scan")
                        },
                        self.deletion.rx.is_none() && !self.recycle.busy,
                        &t,
                    );
                    ui.interact(
                        response.rect,
                        egui::Id::unique("folder-empty-action"),
                        Sense::hover(),
                    );
                    if response.clicked() {
                        if needs_folders {
                            self.open_collection_settings(Some(self.browser.category));
                        } else {
                            self.refresh_scope(ui.ctx());
                        }
                    }
                }
            },
        );
    }
}
