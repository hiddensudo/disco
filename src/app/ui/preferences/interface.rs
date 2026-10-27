use crate::app::ui::preview;
use crate::app::ui::widgets::check_row;
use crate::art;
use crate::settings::{Layout, PreviewMode, Theme, color};
use eframe::egui::{self, RichText};

pub(super) fn show(
    cols: &mut [egui::Ui],
    art: &art::Art,
    theme: &Theme,
    layout: &mut Layout,
    t: &Theme,
    drive_name: &str,
) {
    cols[0].label(RichText::new(crate::tr!("Щільність рядків", "Row density")).size(13.));
    cols[0].horizontal(|ui| {
        ui.selectable_value(
            &mut layout.compact,
            false,
            crate::tr!("Звичайна", "Comfortable"),
        );
        ui.selectable_value(
            &mut layout.compact,
            true,
            crate::tr!("Компактна", "Compact"),
        );
    });
    cols[0].add_space(4.);
    cols[0].label(RichText::new(crate::tr!("Масштаб тексту", "Text size")).size(13.));
    let mut pct = layout.text_scale * 100.;
    cols[0].add(
        egui::Slider::new(&mut pct, 85.0..=140.0)
            .suffix("%")
            .fixed_decimals(0),
    );
    layout.text_scale = pct / 100.;
    cols[0].add_space(4.);
    check_row(
        &mut cols[0],
        &mut layout.categories,
        crate::tr!("Показувати категорії", "Show categories"),
        true,
        t,
    );
    check_row(
        &mut cols[0],
        &mut layout.properties,
        crate::tr!("Показувати властивості", "Show properties"),
        true,
        t,
    );
    cols[0].add_space(7.);
    cols[0].label(RichText::new(crate::tr!("Прев’ю файлів", "File previews")).size(13.));
    egui::ComboBox::from_id_salt("preview-mode-settings")
        .selected_text(match layout.preview_mode {
            PreviewMode::Off => crate::tr!("Вимкнено", "Off"),
            PreviewMode::Inspector => crate::tr!("Праворуч", "Right panel"),
            PreviewMode::Workspace => crate::tr!("Поруч зі списком", "Beside list"),
        })
        .show_ui(&mut cols[0], |ui| {
            for (mode, text) in [
                (PreviewMode::Off, crate::tr!("Вимкнено", "Off")),
                (
                    PreviewMode::Inspector,
                    crate::tr!("Праворуч", "Right panel"),
                ),
                (
                    PreviewMode::Workspace,
                    crate::tr!("Поруч зі списком", "Beside list"),
                ),
            ] {
                ui.selectable_value(&mut layout.preview_mode, mode, text);
            }
        });
    cols[0]
        .add(egui::Checkbox::new(
            &mut layout.open_files_on_double_click,
            RichText::new(crate::tr!(
                "Даблклік відкриває файл у типовій програмі",
                "Double click opens files in the default app"
            ))
            .size(13.),
        ))
        .on_hover_text(crate::tr!(
            "Без призначеної програми нічого не відкривається",
            "Nothing opens if no default app is assigned"
        ));
    cols[0].add_space(5.);
    cols[0].label(RichText::new(crate::tr!("Колонки у списку", "List columns")).size(13.));
    let mut always = true;
    check_row(
        &mut cols[0],
        &mut always,
        crate::tr!("Назва", "Name"),
        false,
        t,
    );
    check_row(
        &mut cols[0],
        &mut always,
        crate::tr!("Розмір", "Size"),
        false,
        t,
    );
    check_row(
        &mut cols[0],
        &mut layout.show_count,
        crate::tr!("Кількість файлів", "File count"),
        true,
        t,
    );
    egui::CollapsingHeader::new(
        RichText::new(crate::tr!("Рух і відгук", "Motion & feedback"))
            .size(12.)
            .color(color(&t.muted)),
    )
    .show(&mut cols[0], |ui| {
        check_row(
            ui,
            &mut layout.animations,
            crate::tr!("Анімації інтерфейсу", "Interface animations"),
            true,
            t,
        );
        ui.add_enabled(
            layout.animations,
            egui::Slider::new(&mut layout.motion_speed, 0.5..=2.)
                .text(crate::tr!("Швидкість", "Speed"))
                .suffix("×")
                .fixed_decimals(2),
        );
        ui.label(
            RichText::new(crate::tr!(
                "Вимкни анімації для зменшеного руху. У спокої вони не працюють.",
                "Disable animations for reduced motion. They stop when idle."
            ))
            .size(12.)
            .color(color(&t.muted)),
        );
    });
    cols[1].label(
        RichText::new(crate::tr!("Попередній перегляд", "Preview"))
            .size(12.)
            .color(color(&t.muted)),
    );
    preview::show(&mut cols[1], art, theme, layout);
    egui::CollapsingHeader::new(RichText::new(crate::tr!("Навігація та сканування", "Navigation & scanning")).size(12.).color(color(&t.muted)))
    .show(&mut cols[1], |ui| {
        ui.label(RichText::new(crate::tr!("Клік по папці", "Folder clicks")).size(13.));
        ui.radio_value(&mut layout.single_click_expand, true, crate::tr!("Один клік — розгорнути дерево", "Single click expands the tree"));
        ui.radio_value(&mut layout.single_click_expand, false, crate::tr!("Подвійний клік — відкрити папку", "Double click opens a folder"));
        check_row(ui, &mut layout.save_scan, crate::tr!("Зберігати й відновлювати останній скан", "Save and restore the last scan"), true, t);
        check_row(ui, &mut layout.auto_refresh, crate::tr!("Автооновлення при зміні SSD", "Refresh when disk usage changes"), true, t);
        ui.label(RichText::new(crate::tr!("Зміна від 64 МіБ → оновлення через 8 с, не частіше разу на хвилину. Однаковий обсяг не гарантує відсутності змін — доступне ручне оновлення.", "Changes of 64 MiB or more trigger a refresh after 8 s, at most once a minute. Unchanged usage does not guarantee unchanged files; you can refresh manually.")).size(12.).color(color(&t.muted)));
        check_row(ui, &mut layout.reuse_scan, crate::tr!("Перевіряти зміни перед обходом", "Check for changes before rescanning"), true, t);
        check_row(ui, &mut layout.gentle_scan, crate::tr!("Бережний режим сканування", "Gentle scanning mode"), true, t);
        ui.label(RichText::new(crate::tr!("Бічні кнопки миші: назад / уперед. Shift + Оновити: повний обхід.", "Mouse side buttons: back / forward. Shift + Refresh: full scan.")).size(12.).color(color(&t.muted)));
    });
    egui::CollapsingHeader::new(
        RichText::new(crate::tr!("Панелі та диск", "Panels & drive"))
            .size(12.)
            .color(color(&t.muted)),
    )
    .show(&mut cols[1], |ui| {
        ui.add(
            egui::Slider::new(&mut layout.category_width, 155.0..=280.0)
                .text(crate::tr!("Категорії, px", "Categories, px")),
        );
        ui.add(
            egui::Slider::new(&mut layout.property_width, 240.0..=380.0)
                .text(crate::tr!("Властивості, px", "Properties, px")),
        );
        ui.label(crate::tr!("Назва диска", "Drive name"));
        ui.add(
            egui::TextEdit::singleline(&mut layout.drive_name)
                .hint_text(drive_name)
                .char_limit(64),
        );
    });
    egui::CollapsingHeader::new(
        RichText::new(crate::tr!("Деталі вигляду", "Display details"))
            .size(12.)
            .color(color(&t.muted)),
    )
    .show(&mut cols[1], |ui| {
        ui.label(
            RichText::new(crate::tr!(
                "Розмір іконок у панелях і дереві",
                "Icon size in panels and tree"
            ))
            .size(12.),
        );
        let mut pct = layout.icon_scale * 100.;
        ui.add(
            egui::Slider::new(&mut pct, 80.0..=140.0)
                .suffix("%")
                .fixed_decimals(0),
        );
        layout.icon_scale = pct / 100.;
        check_row(
            ui,
            &mut layout.category_sizes,
            crate::tr!("Розміри під категоріями", "Sizes below categories"),
            true,
            t,
        );
        check_row(
            ui,
            &mut layout.row_separators,
            crate::tr!("Розділювачі рядків", "Row separators"),
            true,
            t,
        );
        check_row(
            ui,
            &mut layout.striped_rows,
            crate::tr!("Чергування фону рядків", "Alternating row backgrounds"),
            true,
            t,
        );
        check_row(
            ui,
            &mut layout.drive_model,
            crate::tr!("Модель диска в шапці", "Drive model in header"),
            true,
            t,
        );
    });
}
