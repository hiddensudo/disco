use crate::app::Disco;
use crate::app::state::View;
use crate::app::ui::widgets::visual_button;
use crate::art;
use crate::filesystem::open;
use crate::format::group_digits;
use crate::format::{display_size, label, shorten};
use crate::motion;
use crate::settings::color;
use eframe::egui::{self, Align2, Color32 as C, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

impl Disco {
    pub(in crate::app) fn inspector(&mut self, ui: &mut egui::Ui) {
        if self.browser.view == View::Trash {
            self.recycle_inspector(ui);
            return;
        }
        let t = self.preferences.theme.clone();
        let enter = motion::enter(
            ui,
            "inspector-enter",
            (
                self.browser.properties.as_ref().map(|p| &p.path),
                self.scan.report_revision,
            ),
        );
        ui.multiply_opacity(0.4 + enter * 0.6);

        let action_properties = self.browser.properties.clone();
        let action_area = ui.max_rect();
        egui::ScrollArea::vertical()
            .id_salt("properties")
            .max_height(
                (ui.available_height() - if action_properties.is_some() { 98. } else { 0. })
                    .max(50.),
            )
            .show(ui, |ui| {
                ui.add_space(7.);
                if let Some(p) = self.browser.properties.clone() {
                    let (rect, _) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 88.), Sense::hover());
                    let tile = Rect::from_min_size(rect.min + Vec2::new(0., 5.), Vec2::splat(66.));
                    ui.painter().rect_filled(tile, t.radius, color(&t.raised));
                    self.art.paint(
                        ui,
                        0,
                        Rect::from_center_size(
                            tile.center(),
                            Vec2::splat(46. * self.preferences.layout.icon_scale),
                        ),
                        C::WHITE,
                    );
                    ui.painter().text(
                        rect.min + Vec2::new(84., 13.),
                        Align2::LEFT_TOP,
                        label(&p.path),
                        art::bold(22.),
                        color(&t.text),
                    );
                    let path = shorten(
                        &p.path.to_string_lossy(),
                        ((rect.width() - 86.) / 6.2) as usize,
                    );
                    ui.painter().text(
                        rect.min + Vec2::new(84., 44.),
                        Align2::LEFT_TOP,
                        path,
                        FontId::proportional(12.),
                        color(&t.muted),
                    );
                    ui.interact(rect, ui.scope_id().with("property_path"), Sense::hover())
                        .on_hover_text(p.path.to_string_lossy());
                    ui.add_space(3.);
                    ui.label(RichText::new(display_size(p.bytes)).font(art::bold(29.)));
                    ui.add_space(12.);
                    for (k, v) in [
                        (
                            crate::tr!("Тип", "Type"),
                            if p.is_dir {
                                crate::tr!("Папка", "Folder").into()
                            } else {
                                crate::tr!("Файл", "File").into()
                            },
                        ),
                        (
                            crate::tr!("Зайнято на диску", "Size on disk"),
                            display_size(p.bytes),
                        ),
                        (
                            crate::tr!("Елементів", "Items"),
                            p.files.map(group_digits).unwrap_or_else(|| "—".into()),
                        ),
                        (
                            crate::tr!("Змінено", "Modified"),
                            p.modified.split(',').next().unwrap_or("—").to_string(),
                        ),
                        (
                            crate::tr!("Створено", "Created"),
                            p.created.split(',').next().unwrap_or("—").to_string(),
                        ),
                        (crate::tr!("Власник", "Owner"), p.owner),
                        (crate::tr!("Доступ", "Permissions"), p.permissions),
                    ] {
                        let (r, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 25.),
                            Sense::hover(),
                        );
                        ui.painter().text(
                            r.left_center(),
                            Align2::LEFT_CENTER,
                            k,
                            FontId::proportional(14.),
                            color(&t.muted),
                        );
                        ui.painter().text(
                            r.right_center(),
                            Align2::RIGHT_CENTER,
                            v,
                            FontId::proportional(14.),
                            color(&t.text),
                        );
                        ui.painter().hline(
                            r.x_range(),
                            r.bottom(),
                            Stroke::new(0.4, color(&t.border).gamma_multiply(0.65)),
                        );
                    }
                    if let Some(e) = p.error {
                        ui.add(
                            egui::Label::new(RichText::new(e).size(12.).color(color(&t.accent)))
                                .wrap(),
                        );
                    }
                } else {
                    let (r, _) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 75.), Sense::hover());
                    self.art.paint(
                        ui,
                        0,
                        Rect::from_min_size(r.min + Vec2::new(2., 9.), Vec2::splat(48.)),
                        C::from_white_alpha(110),
                    );
                    ui.painter().text(
                        r.min + Vec2::new(66., 17.),
                        Align2::LEFT_TOP,
                        crate::tr!("Властивості", "Properties"),
                        art::bold(18.),
                        color(&t.text),
                    );
                    ui.add_space(8.);
                    ui.label(
                        RichText::new(crate::tr!(
                            "Вибери папку або файл у списку.",
                            "Select a folder or file in the list."
                        ))
                        .size(14.)
                        .color(color(&t.muted)),
                    );
                }
            });
        if let Some(p) = action_properties {
            let r = Rect::from_min_size(
                Pos2::new(action_area.left(), action_area.bottom() - 83.),
                Vec2::new(action_area.width(), 36.),
            );
            if visual_button(
                ui,
                &self.art,
                r,
                10,
                crate::tr!("Відкрити в папці", "Open in file manager"),
                !self.demo,
                &t,
            )
            .clicked()
            {
                open(if p.is_dir {
                    p.path.as_path()
                } else {
                    p.path.parent().unwrap_or(&p.path)
                });
            }
            if p.is_dir {
                let r = r.translate(Vec2::new(0., 47.));
                let mut danger = t.clone();
                danger.text = t.accent.clone();
                danger.border = t.accent.clone();
                if visual_button(
                    ui,
                    &self.art,
                    r,
                    23,
                    crate::tr!("Видалити папку", "Delete folder"),
                    !self.demo
                        && !self.scan.busy
                        && self.deletion.rx.is_none()
                        && !self.recycle.busy
                        && p.error.is_none(),
                    &danger,
                )
                .on_hover_text(crate::tr!(
                    "Перемістити папку з усім вмістом у кошик після підтвердження",
                    "Move this folder and all its contents to Trash after confirmation"
                ))
                .clicked()
                {
                    self.request_delete(vec![(p.path, p.bytes)]);
                }
            }
        }
    }
}
