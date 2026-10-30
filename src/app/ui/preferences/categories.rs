use crate::{
    app::Disco,
    categories::{self, Collection},
    settings::color,
};
use eframe::egui::{self, Rect, Sense, Vec2};

impl Disco {
    pub(super) fn categories_preferences(&mut self, ui: &mut egui::Ui) {
        if self.collections.draft.is_none() {
            self.collections.draft = Some(self.collections.config.clone());
        }
        let mut config = self.collections.draft.take().unwrap();
        self.collections.editing = self
            .collections
            .editing
            .min(config.items.len().saturating_sub(1));
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("edit-category")
                .width(225.)
                .selected_text(
                    config
                        .items
                        .get(self.collections.editing)
                        .map(Collection::title)
                        .unwrap_or("—"),
                )
                .show_ui(ui, |ui| {
                    for (i, c) in config.items.iter().enumerate() {
                        ui.selectable_value(&mut self.collections.editing, i, c.title());
                    }
                });
            if ui
                .add_enabled(
                    config.items.len() < 64,
                    egui::Button::new(crate::tr!("Нова категорія", "New category")),
                )
                .clicked()
            {
                let id = (1..)
                    .map(|i| format!("collection-{i}"))
                    .find(|id| {
                        !config.items.iter().any(|c| &c.id == id)
                            && !crate::settings::config_dir()
                                .join("categories")
                                .join(format!("{id}.toml"))
                                .exists()
                    })
                    .unwrap();
                config.items.push(Collection {
                    auto_refresh: false,
                    auto_save: false,
                    read_only: false,
                    id,
                    name: crate::tr!("Нова категорія", "New category").into(),
                    icon: "folder".into(),
                    folders: vec![],
                });
                self.collections.editing = config.items.len() - 1;
            }
            let removable = config
                .items
                .get(self.collections.editing)
                .is_some_and(|c| !c.builtin());
            if ui
                .add_enabled(
                    removable,
                    egui::Button::new(crate::tr!("Прибрати категорію", "Remove category")),
                )
                .on_hover_text(crate::tr!(
                    "Прибирає лише добірку. Папки залишаються на диску.",
                    "Removes the collection only. Folders stay on disk."
                ))
                .clicked()
            {
                config.items.remove(self.collections.editing);
                self.collections.editing = 0;
            }
        });
        ui.add_space(8.);
        if let Some(c) = config.items.get_mut(self.collections.editing) {
            ui.columns(2, |cols| {
                {
                    let ui = &mut cols[0];
                    let fallback_name = c.category().name();
                    ui.horizontal(|ui| {
                        ui.label(crate::tr!("Назва", "Name"));
                        ui.label(
                            egui::RichText::new(format!("id: {}", c.id))
                                .small()
                                .color(color(&self.preferences.theme.muted)),
                        );
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.), Sense::hover());
                        self.robot_button(ui, rect, c);
                    });
                    ui.add(
                        egui::TextEdit::singleline(&mut c.name)
                            .char_limit(80)
                            .desired_width(ui.available_width() - 8.)
                            .hint_text(fallback_name),
                    );
                    ui.add_space(8.);
                    ui.checkbox(
                        &mut c.auto_refresh,
                        crate::tr!("Автооновлення файлів", "Live file updates"),
                    );
                    ui.add_enabled(
                        !c.read_only,
                        egui::Checkbox::new(
                            &mut c.auto_save,
                            crate::tr!(
                                "Автозбереження після 1 с без набору",
                                "Auto-save after 1 s idle"
                            ),
                        ),
                    )
                    .on_hover_text(crate::tr!(
                        "При конфлікті чернетка залишається; файл не перезаписується.",
                        "Conflicts keep your draft without overwriting the file."
                    ));
                    ui.checkbox(
                        &mut c.read_only,
                        crate::tr!("Лише читання в disco", "Read only in disco"),
                    );
                    ui.label(crate::tr!("Папки в категорії", "Folders in this category"));
                    if c.builtin() && c.folders.is_empty() {
                        ui.label(
                    egui::RichText::new(crate::tr!(
                        "Автоматичні правила. Додай папки, щоб замінити їх власною добіркою.",
                        "Automatic rules. Add folders to replace them with your own collection."
                    ))
                    .small(),
                );
                    }
                    let mut remove = None;
                    egui::ScrollArea::vertical()
                        .id_salt("collection-folders")
                        .max_height(55.)
                        .show(ui, |ui| {
                            for (i, path) in c.folders.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    if ui
                                        .small_button("×")
                                        .on_hover_text(crate::tr!(
                                            "Прибрати з категорії",
                                            "Remove from category"
                                        ))
                                        .clicked()
                                    {
                                        remove = Some(i);
                                    }
                                    ui.add(egui::Label::new(path.to_string_lossy()).truncate())
                                        .on_hover_text(path.to_string_lossy());
                                });
                            }
                        });
                    if let Some(i) = remove {
                        c.folders.remove(i);
                    }
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.collections.folder_input)
                                .desired_width((ui.available_width() - 8.).max(100.))
                                .hint_text("/home/user/Pictures"),
                        );
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                c.folders.len() < 128,
                                egui::Button::new(crate::tr!("Додати", "Add")),
                            )
                            .clicked()
                        {
                            let path =
                                std::path::PathBuf::from(self.collections.folder_input.trim());
                            if path.is_absolute()
                                && !path
                                    .components()
                                    .any(|p| matches!(p, std::path::Component::ParentDir))
                            {
                                if !c.folders.contains(&path) {
                                    c.folders.push(path);
                                }
                                self.collections.folder_input.clear();
                            } else {
                                self.preferences.error = crate::tr!(
                                    "Вкажи абсолютний шлях до папки без ..",
                                    "Enter an absolute folder path without .."
                                )
                                .into();
                            }
                        }
                        if ui
                            .add_enabled(
                                c.folders.len() < 128,
                                egui::Button::new(crate::tr!("Обрати…", "Browse…")),
                            )
                            .clicked()
                        {
                            if let Some(paths) = rfd::FileDialog::new().pick_folders() {
                                for path in paths {
                                    if c.folders.len() < 128 && !c.folders.contains(&path) {
                                        c.folders.push(path);
                                    }
                                }
                            }
                        }
                    });
                }
                {
                    let ui = &mut cols[1];
                    ui.horizontal(|ui| {
                        let icon = self.art.category_icon(&c.icon);
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.), Sense::hover());
                        self.art.paint(
                            ui,
                            icon,
                            rect.shrink(3.),
                            crate::art::category_tint(icon, color(&self.preferences.theme.text)),
                        );
                        ui.label(crate::tr!("Іконка", "Icon"));
                        if ui
                            .button(crate::tr!("Завантажити свою…", "Import your own…"))
                            .clicked()
                        {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Icons", &["svg", "png", "jpg", "jpeg", "webp"])
                                .pick_file()
                            {
                                match crate::art::import_icon(&path) {
                                    Ok(icon) => c.icon = icon,
                                    Err(error) => self.preferences.error = error,
                                }
                            }
                        }
                    });
                    if let Some(error) = self.art.custom_error(&c.icon) {
                        ui.colored_label(color(&self.preferences.theme.accent), error);
                    }
                    egui::CollapsingHeader::new(crate::tr!(
                        "Обрати з набору іконок",
                        "Choose a built-in icon"
                    ))
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("category-icons")
                            .min_col_width(28.)
                            .spacing(Vec2::splat(3.))
                            .show(ui, |ui| {
                                for (i, (name, icon)) in
                                    crate::art::category_icons().iter().enumerate()
                                {
                                    let (rect, response) =
                                        ui.allocate_exact_size(Vec2::splat(28.), Sense::click());
                                    if c.icon == *name || response.hovered() {
                                        ui.painter().rect_filled(
                                            rect,
                                            self.preferences.theme.radius,
                                            color(&self.preferences.theme.raised),
                                        );
                                    }
                                    if c.icon == *name {
                                        ui.painter().rect_stroke(
                                            rect,
                                            self.preferences.theme.radius,
                                            egui::Stroke::new(
                                                1.,
                                                color(&self.preferences.theme.accent),
                                            ),
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    self.art.paint(
                                        ui,
                                        *icon,
                                        Rect::from_center_size(rect.center(), Vec2::splat(23.)),
                                        color(&self.preferences.theme.text),
                                    );
                                    if response.on_hover_text(*name).clicked() {
                                        c.icon = name.to_string();
                                    }
                                    if i % 8 == 7 {
                                        ui.end_row();
                                    }
                                }
                            });
                    });
                }
            });
        }
        ui.add_space(10.);
        let category_file = config
            .items
            .get(self.collections.editing)
            .map(|c| self.collections.file_for(c));
        let shown_file = category_file.unwrap_or_else(categories::path);
        ui.add(
            egui::Label::new(
                egui::RichText::new(shown_file.display().to_string())
                    .small()
                    .color(color(&self.preferences.theme.muted)),
            )
            .truncate(),
        )
        .on_hover_text(shown_file.display().to_string());
        ui.horizontal(|ui| {
            if ui
                .button(crate::tr!("Копіювати шлях TOML", "Copy TOML path"))
                .clicked()
            {
                ui.ctx().copy_text(shown_file.display().to_string());
            }
            if ui
                .button(crate::tr!("Перезавантажити з диска", "Reload from disk"))
                .on_hover_text(crate::tr!(
                    "Замінює незбережені зміни категорій даними з файлів категорій",
                    "Replaces unsaved category edits with the category files"
                ))
                .clicked()
            {
                match self.collections.reload() {
                    Ok(()) => {
                        config = self.collections.config.clone();
                        self.preferences.error.clear();
                    }
                    Err(e) => self.preferences.error = e,
                }
            }
        });
        if !self.collections.error.is_empty() {
            ui.colored_label(
                color(&self.preferences.theme.accent),
                &self.collections.error,
            );
        }
        self.collections.draft = Some(config);
    }
}
