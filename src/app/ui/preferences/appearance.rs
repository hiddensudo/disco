use crate::app::ui::preview;
use crate::app::ui::widgets::theme_color_row;
use crate::art;
use crate::settings;
use crate::settings::{Layout, Theme, color};
use eframe::egui::{self, RichText};

pub(super) fn show(
    cols: &mut [egui::Ui],
    art: &art::Art,
    theme: &mut Theme,
    layout: &Layout,
    t: &Theme,
) {
    let previous = theme.name.clone();
    cols[0].horizontal(|ui| {
        ui.add_sized(
            [67., 30.],
            egui::Label::new(RichText::new(crate::tr!("Тема", "Theme")).size(13.)),
        );
        egui::ComboBox::from_id_salt("theme_select")
            .width(200.)
            .selected_text(&theme.name)
            .show_ui(ui, |ui| {
                for name in ["Carbon", "Ember", "Noir"] {
                    ui.selectable_value(&mut theme.name, name.into(), name);
                }
            });
    });
    if previous != theme.name {
        *theme = Theme::preset(&theme.name);
    }
    cols[0].add_space(10.);
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
}
