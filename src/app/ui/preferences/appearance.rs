use crate::app::ui::preview;
use crate::app::ui::widgets::theme_color_row;
use crate::settings::{Layout, Theme, color};
use crate::{art, settings, themes};
use eframe::egui::{self, Rect, RichText, Sense, Vec2};

fn swatch(ui: &egui::Ui, rect: Rect, accent: egui::Color32) {
    let square = Rect::from_center_size(
        egui::pos2(rect.left() + 14., rect.center().y),
        Vec2::splat(12.),
    );
    ui.painter().rect_filled(square, 3, accent);
}

pub(super) fn show(
    cols: &mut [egui::Ui],
    art: &art::Art,
    theme: &mut Theme,
    layout: &Layout,
    t: &Theme,
    store: &mut themes::Manager,
    qa: bool,
) {
    let mut select = None;
    cols[0].horizontal(|ui| {
        ui.add_sized(
            [67., 30.],
            egui::Label::new(RichText::new(crate::tr!("Тема", "Theme")).size(13.)),
        );
        if qa && std::env::var("DISCO_QA_THEME_MENU").is_ok_and(|s| s == "1") {
            let id = ui.make_persistent_id(egui::IdSalt::new("theme_select"));
            egui::Popup::open_id(ui.ctx(), id.with("popup"));
        }
        let picker = egui::ComboBox::from_id_salt("theme_select")
            .width(210.)
            .height(232.)
            .truncate()
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .selected_text(format!("      {}", theme.name))
            .show_ui(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                ui.spacing_mut().button_padding.y = 5.;
                let list = egui::ScrollArea::vertical()
                    .id_salt("theme-list-scroll")
                    .max_height(150.)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for entry in store.entries() {
                            let button = ui.add_enabled(
                                entry.theme.is_some(),
                                egui::Button::selectable(
                                    store.selected() == entry.id,
                                    format!("      {}", entry.name),
                                )
                                .min_size(Vec2::new(ui.available_width(), 30.))
                                .wrap_mode(egui::TextWrapMode::Truncate),
                            );
                            swatch(
                                ui,
                                button.rect,
                                entry
                                    .theme
                                    .as_ref()
                                    .map(|t| color(&t.accent))
                                    .unwrap_or(color(&t.muted)),
                            );
                            ui.interact(
                                button.rect,
                                egui::Id::unique(("theme-choice", &entry.id)),
                                Sense::hover(),
                            );
                            if let Some(error) = &entry.error {
                                button.clone().on_disabled_hover_text(error);
                            }
                            if button.clicked() {
                                select = Some(entry.id.clone());
                                ui.close();
                            }
                        }
                    });
                ui.interact(
                    list.inner_rect,
                    egui::Id::unique("theme-list-visible"),
                    Sense::hover(),
                );
                ui.add_space(4.);
                ui.separator();
                let create = ui.add(
                    egui::Button::new(crate::tr!("+ Створити нову…", "+ Create new…"))
                        .min_size(Vec2::new(ui.available_width(), 30.)),
                );
                ui.interact(
                    create.rect,
                    egui::Id::unique("theme-create-fixed"),
                    Sense::hover(),
                );
                if create.clicked() {
                    store.creating = true;
                    store.new_name.clear();
                    store.error.clear();
                    ui.close();
                }
            });
        swatch(ui, picker.response.rect, color(&theme.accent));
        ui.interact(
            picker.response.rect,
            egui::Id::unique("theme-picker"),
            Sense::hover(),
        )
        .on_hover_text(&theme.name);
    });
    if let Some(id) = select {
        match store.select(&id, theme) {
            Ok(next) => *theme = next,
            Err(error) => store.error = error,
        }
    }
    if store.creating {
        cols[0].group(|ui| {
            ui.label(crate::tr!("Назва нової теми", "New theme name"));
            ui.add(
                egui::TextEdit::singleline(&mut store.new_name)
                    .id(egui::Id::unique("theme-new-name"))
                    .char_limit(80)
                    .desired_width(ui.available_width()),
            );
            ui.horizontal(|ui| {
                let create = ui.button(crate::tr!("Створити", "Create"));
                ui.interact(
                    create.rect,
                    egui::Id::unique("theme-create-confirm"),
                    Sense::hover(),
                );
                if create.clicked() {
                    let name = store.new_name.clone();
                    store.remember(theme);
                    match store.create(&name, theme) {
                        Ok(next) => *theme = next,
                        Err(error) => store.error = error,
                    }
                }
                if ui.button(crate::tr!("Скасувати", "Cancel")).clicked() {
                    store.creating = false;
                    store.new_name.clear();
                    store.error.clear();
                }
            });
            ui.label(
                RichText::new(crate::tr!(
                    "Файли буде створено після «Зберегти».",
                    "Files are created when you click Save."
                ))
                .small(),
            );
        });
        if cols[0].input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            store.creating = false;
        }
    }
    cols[0].horizontal(|ui| {
        ui.label(RichText::new(crate::tr!("Кольори", "Colors")).size(13.));
        let (robot, click) = ui.allocate_exact_size(Vec2::splat(28.), Sense::click());
        art.paint(ui, 29, robot.shrink(4.), color(&t.text));
        ui.interact(
            robot,
            egui::Id::unique("theme-prompt-button"),
            Sense::hover(),
        );
        if click
            .on_hover_text(crate::tr!(
                "Створити тему з AI: формат, шляхи та промпт",
                "Create a theme with AI: format, paths and prompt"
            ))
            .clicked()
        {
            store.prompt = Some((
                layout.effective_prompt_language(),
                store.generation_prompt(layout.effective_prompt_language(), theme),
            ));
        }
        if ui
            .small_button(crate::tr!("Оновити теми", "Reload themes"))
            .on_hover_text(crate::tr!(
                "Завантажити зовнішні зміни й відкинути незбережені зміни тем",
                "Load external changes and discard unsaved theme changes"
            ))
            .clicked()
        {
            match store.reload() {
                Ok(next) => *theme = next,
                Err(error) => store.error = error,
            }
        }
    });
    if !store.error.is_empty() {
        cols[0].label(RichText::new(&store.error).small().color(color(&t.accent)));
    }
    let unavailable = store.entries().iter().filter(|e| e.error.is_some()).count();
    if unavailable > 0 {
        cols[0].label(
            RichText::new(crate::tr!(
                "Недоступні теми: {n}. Причина — при наведенні у списку.",
                "Unavailable themes: {n}. Hover over an entry for details.",
                n = unavailable
            ))
            .small()
            .color(color(&t.muted)),
        );
    }
    cols[0].add_space(5.);
    for (name, value) in [
        (crate::tr!("Акцент", "Accent"), &mut theme.accent),
        (crate::tr!("Фон", "Background"), &mut theme.background),
        (crate::tr!("Панелі", "Panels"), &mut theme.panel),
        (crate::tr!("Текст", "Text"), &mut theme.text),
    ] {
        theme_color_row(&mut cols[0], name, value, t);
    }
    cols[0].add_space(4.);
    egui::CollapsingHeader::new(
        RichText::new(crate::tr!("Додаткові кольори", "Additional colors"))
            .size(12.)
            .color(color(&t.muted)),
    )
    .show(&mut cols[0], |ui| {
        for (name, value) in [
            (crate::tr!("Кошик", "Trash"), &mut theme.trash),
            (crate::tr!("Підписи", "Labels"), &mut theme.muted),
            (crate::tr!("Межі", "Borders"), &mut theme.border),
            (crate::tr!("Підняття", "Raised"), &mut theme.raised),
        ] {
            theme_color_row(ui, name, value, t);
        }
        ui.add(
            egui::Slider::new(&mut theme.radius, 0..=12)
                .text(crate::tr!("Радіус, px", "Corner radius, px")),
        );
    });
    cols[1].label(
        RichText::new(crate::tr!("Попередній перегляд", "Preview"))
            .size(12.)
            .color(color(&t.muted)),
    );
    preview::show(&mut cols[1], art, theme, layout);
    cols[1].add_space(9.);
    cols[1].label(
        RichText::new(crate::tr!(
            "Контраст тексту: {:.1}:1",
            "Text contrast: {:.1}:1",
            settings::contrast(color(&theme.text), color(&theme.background))
        ))
        .size(12.)
        .color(color(&t.muted)),
    );
    if qa
        && store.prompt.is_none()
        && std::env::var("DISCO_QA_THEME_PROMPT").is_ok_and(|s| s == "1")
    {
        store.prompt = Some((
            layout.effective_prompt_language(),
            store.generation_prompt(layout.effective_prompt_language(), theme),
        ));
    }
    if store
        .prompt
        .as_ref()
        .is_some_and(|(language, _)| *language != layout.effective_prompt_language())
    {
        store.prompt = Some((
            layout.effective_prompt_language(),
            store.generation_prompt(layout.effective_prompt_language(), theme),
        ));
    }
    let mut prompt_open = store.prompt.is_some();
    if let Some((_, prompt)) = store.prompt.as_mut() {
        egui::Window::new(crate::tr!("Промпт генерації теми", "Theme generation prompt"))
            .id(egui::Id::unique("theme-prompt-window")).open(&mut prompt_open)
            .default_width(640.).max_height((cols[0].ctx().content_rect().height()-70.).max(200.)).vscroll(true)
            .show(cols[0].ctx(),|ui| {
                ui.label(crate::tr!("Скопіюй промпт у чат асистента й допиши бажану тему наприкінці. Він містить TOML-схему та точні шляхи.", "Copy the prompt into your assistant chat and append the theme you want at the end. It includes the TOML schema and exact paths."));
                if ui.button(crate::tr!("Копіювати промпт", "Copy prompt")).clicked(){ui.ctx().copy_text(prompt.clone());}
                ui.add(egui::TextEdit::multiline(prompt).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).desired_rows(18));
            });
        if cols[0].input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            prompt_open = false;
        }
    }
    if !prompt_open {
        store.prompt = None;
    }
}
