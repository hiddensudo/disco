use crate::app::Disco;
use crate::app::ui::widgets::{mix, visual_button};
use crate::art;
use crate::format::display_size;
use crate::motion;
use crate::recycle;
use crate::settings::{Theme, color};
use eframe::egui::{self, Align2, Color32 as C, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::collections::HashSet;

impl Disco {
    pub(crate) fn bin_actions_enabled(&self) -> bool {
        !self.demo && !self.scan.busy && self.deletion.rx.is_none() && !self.recycle.busy
    }
    pub(crate) fn bin_restore(&mut self, ctx: &egui::Context) {
        if self.bin_actions_enabled() {
            self.recycle
                .start(self.recycle.picked(), recycle::Action::Restore, ctx);
        }
    }
    fn bin_request_purge(&mut self, all: bool) {
        if !self.bin_actions_enabled() {
            return;
        }
        let targets = if all {
            self.recycle.items.clone()
        } else {
            self.recycle.picked()
        };
        if !targets.is_empty() {
            self.purge_targets = Some(targets);
            self.purge_fresh = true;
        }
    }
    pub(crate) fn bin_confirmation(&mut self, ctx: &egui::Context) {
        let Some(targets) = self.purge_targets.as_ref() else {
            return;
        };
        let modal = purge_dialog(
            ctx,
            targets,
            &self.preferences.theme,
            self.purge_fresh,
            self.bin_actions_enabled(),
        );
        self.purge_fresh = false;
        if modal.inner.0.clicked() || modal.should_close() {
            self.purge_targets = None;
        } else if modal.inner.1.clicked() && self.bin_actions_enabled() {
            let targets = self.purge_targets.take().unwrap_or_default();
            self.recycle.start(targets, recycle::Action::Purge, ctx);
        }
    }
    pub(crate) fn recycle_content(&mut self, ui: &mut egui::Ui) {
        let t = self.preferences.theme.clone();
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(32.), Sense::hover());
            if visual_button(
                ui,
                &self.art,
                r,
                13,
                "",
                self.browser.history.can_back() && !self.scan.busy && !self.recycle.busy,
                &t,
            )
            .on_hover_text(crate::tr!("Назад", "Back"))
            .clicked()
            {
                self.history_step(false);
            }
            ui.heading(crate::tr!("Кошик", "Trash"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.recycle.busy && !self.demo,
                        egui::Button::new(crate::tr!("Оновити", "Refresh")),
                    )
                    .clicked()
                {
                    self.refresh_scope(ui.ctx());
                }
            });
        });
        let (bytes, complete) = self.recycle.total();
        ui.label(
            RichText::new(crate::tr!(
                "{} елементів · {}{} у кошику",
                "{} items · {}{} in Trash",
                self.recycle.items.len(),
                if complete {
                    ""
                } else {
                    crate::tr!("щонайменше ", "at least ")
                },
                display_size(bytes)
            ))
            .size(13.)
            .color(color(&t.muted)),
        );
        ui.add_space(6.);
        ui.horizontal_wrapped(|ui| {
            let enabled = self.bin_actions_enabled();
            let picked = !self.recycle.selected.is_empty();
            if ui
                .add_enabled(
                    enabled && picked,
                    egui::Button::new(crate::tr!("Відновити вибране", "Restore selected")),
                )
                .clicked()
            {
                self.bin_restore(ui.ctx());
            }
            if ui
                .add_enabled(
                    enabled && picked,
                    egui::Button::new(
                        RichText::new(crate::tr!("Видалити вибране", "Delete selected"))
                            .color(color(&t.accent)),
                    ),
                )
                .clicked()
            {
                self.bin_request_purge(false);
            }
            if ui
                .add_enabled(
                    enabled && !self.recycle.items.is_empty(),
                    egui::Button::new(crate::tr!("Спорожнити кошик", "Empty Trash")),
                )
                .clicked()
            {
                self.bin_request_purge(true);
            }
        });
        if self.recycle.busy || (self.recycle.loading && !self.recycle.checking) {
            ui.horizontal(|ui| {
                motion::activity(ui, false, color(&t.accent));
                ui.label(
                    RichText::new(if self.recycle.busy {
                        self.recycle.notice.as_str()
                    } else if self.recycle.loaded {
                        crate::tr!("Обчислюємо розміри папок…", "Calculating folder sizes…")
                    } else {
                        crate::tr!("Читаємо кошик…", "Reading Trash…")
                    })
                    .size(12.)
                    .color(color(&t.muted)),
                );
            });
        } else if !self.recycle.notice.is_empty() {
            ui.label(
                RichText::new(&self.recycle.notice)
                    .size(12.)
                    .color(color(&t.muted)),
            );
        }
        if !self.recycle.error.is_empty() {
            ui.add(
                egui::Label::new(
                    RichText::new(&self.recycle.error)
                        .size(12.)
                        .color(color(&t.accent)),
                )
                .wrap(),
            );
        }
        ui.add_space(6.);
        self.browser.text_input_focused = ui
            .add(
                egui::TextEdit::singleline(&mut self.browser.filter)
                    .hint_text(crate::tr!(
                        "Пошук у кошику за назвою або початковим шляхом…",
                        "Search Trash by name or original path…"
                    ))
                    .desired_width(f32::INFINITY),
            )
            .has_focus();
        ui.add_space(6.);
        let width = ui.available_width();
        let (header, _) = ui.allocate_exact_size(Vec2::new(width, 30.), Sense::hover());
        ui.painter().rect_filled(header, t.radius, color(&t.raised));
        let mut all = !self.recycle.items.is_empty()
            && self.recycle.selected.len() == self.recycle.items.len();
        let checkbox = check_at(
            ui,
            Rect::from_min_size(header.min + Vec2::new(10., 4.), Vec2::splat(23.)),
            &mut all,
            !self.recycle.busy && !self.recycle.items.is_empty(),
        );
        if checkbox.changed() && !self.recycle.busy {
            self.recycle.selected = if all {
                self.recycle
                    .items
                    .iter()
                    .map(|i| i.entry.id.clone())
                    .collect()
            } else {
                HashSet::new()
            };
        }
        let size_x = header.right() - 150.;
        let date_x = header.right() - 12.;
        for (p, align, text) in [
            (
                Pos2::new(header.left() + 44., header.center().y),
                Align2::LEFT_CENTER,
                crate::tr!("Назва / початковий шлях", "Name / original path"),
            ),
            (
                Pos2::new(size_x, header.center().y),
                Align2::RIGHT_CENTER,
                crate::tr!("Розмір", "Size"),
            ),
            (
                Pos2::new(date_x, header.center().y),
                Align2::RIGHT_CENTER,
                crate::tr!("Видалено", "Deleted"),
            ),
        ] {
            ui.painter()
                .text(p, align, text, FontId::proportional(12.), color(&t.muted));
        }
        let filter = self.browser.filter.to_lowercase();
        let ids: Vec<_> = self
            .recycle
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| {
                filter.is_empty()
                    || i.entry
                        .original_path()
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&filter)
            })
            .map(|(n, _)| n)
            .collect();
        if ids.is_empty() {
            ui.add_space(35.);
            ui.label(if self.recycle.loading {
                crate::tr!("Оновлюємо кошик…", "Refreshing Trash…")
            } else if !self.recycle.listing_complete() {
                crate::tr!("Стан кошика недоступний", "Trash contents unavailable")
            } else if self.recycle.items.is_empty() {
                crate::tr!("Кошик порожній", "Trash is empty")
            } else {
                crate::tr!("Нічого не знайдено", "No results")
            });
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("recycle-list")
            .auto_shrink([false, false])
            .show_rows(ui, 64., ids.len(), |ui, range| {
                ui.spacing_mut().item_spacing.y = 0.;
                for n in range {
                    let item = self.recycle.items[ids[n]].clone();
                    let (r, response) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 64.), Sense::click());
                    let mut selected = self.recycle.selected.contains(&item.entry.id);
                    let hovered =
                        motion::hover(ui, ("trash-row", &item.entry.id), response.hovered());
                    if selected || hovered > 0. {
                        ui.painter().rect_filled(
                            r,
                            0,
                            mix(
                                color(&t.background),
                                if selected {
                                    mix(color(&t.background), color(&t.accent), 0.25)
                                } else {
                                    color(&t.raised)
                                },
                                if selected { 1. } else { hovered },
                            ),
                        );
                    }
                    let check = check_at(
                        ui,
                        Rect::from_center_size(
                            Pos2::new(r.left() + 22., r.center().y),
                            Vec2::splat(23.),
                        ),
                        &mut selected,
                        !self.recycle.busy,
                    );
                    if !self.recycle.busy
                        && (check.changed() || (response.clicked() && !check.contains_pointer()))
                    {
                        if !check.changed() {
                            selected = !selected;
                        }
                        if selected {
                            self.recycle.selected.insert(item.entry.id.clone());
                        } else {
                            self.recycle.selected.remove(&item.entry.id);
                        }
                    }
                    self.art.paint(
                        ui,
                        if item.directory { 0 } else { 4 },
                        Rect::from_center_size(
                            Pos2::new(r.left() + 53., r.center().y),
                            Vec2::splat(23.),
                        ),
                        color(&t.text),
                    );
                    let text_right = (r.right() - 252.).max(r.left() + 85.);
                    let text_rect = Rect::from_min_max(
                        Pos2::new(r.left() + 75., r.top() + 8.),
                        Pos2::new(text_right, r.bottom() - 6.),
                    );
                    let p = ui.painter().with_clip_rect(text_rect);
                    p.text(
                        text_rect.min,
                        Align2::LEFT_TOP,
                        item.entry.name.to_string_lossy(),
                        art::bold(14.),
                        color(&t.text),
                    );
                    p.text(
                        text_rect.min + Vec2::new(0., 24.),
                        Align2::LEFT_TOP,
                        item.entry.original_path().to_string_lossy(),
                        FontId::proportional(11.),
                        color(&t.muted),
                    );
                    ui.painter().text(
                        Pos2::new(r.right() - 150., r.center().y),
                        Align2::RIGHT_CENTER,
                        item_size(&item),
                        FontId::proportional(13.),
                        color(&t.text),
                    );
                    ui.painter().text(
                        Pos2::new(r.right() - 12., r.center().y),
                        Align2::RIGHT_CENTER,
                        deleted_date(item.entry.time_deleted),
                        FontId::proportional(11.),
                        color(&t.muted),
                    );
                    ui.painter()
                        .hline(r.x_range(), r.bottom(), Stroke::new(0.5, color(&t.border)));
                    response.on_hover_text(item.entry.original_path().to_string_lossy());
                }
            });
    }
    pub(crate) fn recycle_inspector(&mut self, ui: &mut egui::Ui) {
        let t = self.preferences.theme.clone();
        let picked = self.recycle.picked();
        let area = ui.max_rect();
        let enabled = self.bin_actions_enabled() && !picked.is_empty();
        egui::ScrollArea::vertical().id_salt("recycle-inspector").max_height((ui.available_height()-98.).max(50.)).show(ui,|ui|{
            let (r,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),62.),Sense::hover());
            self.art.paint(ui,23,Rect::from_min_size(r.min+Vec2::new(0.,8.),Vec2::splat(38.)),color(&t.text));
            ui.painter().text(r.min+Vec2::new(54.,15.),Align2::LEFT_TOP,crate::tr!("Кошик", "Trash"),art::bold(21.),color(&t.text));
            if picked.is_empty(){
                let (bytes,complete)=self.recycle.total();
                ui.label(RichText::new(format!("{}{}",if complete{""}else{"≥ "},display_size(bytes))).font(art::bold(28.)));
                ui.label(crate::tr!("Елементів: {}", "Items: {}",self.recycle.items.len()));ui.add_space(12.);
                ui.label(crate::tr!("Вибери елементи у списку, щоб повернути їх або видалити остаточно.", "Select items in the list to restore or delete them permanently."));
            }else{
                ui.label(RichText::new(crate::tr!("Вибрано: {}", "Selected: {}",picked.len())).strong());ui.add_space(8.);
                for item in picked.iter().take(1){
                    ui.label(RichText::new(item.entry.name.to_string_lossy()).font(art::bold(18.)));
                    ui.label(RichText::new(item_size(item)).font(art::bold(27.)));ui.add_space(8.);
                    ui.label(RichText::new(crate::tr!("Початкове розташування", "Original location")).size(12.).color(color(&t.muted)));
                    ui.add(egui::Label::new(item.entry.original_path().to_string_lossy()).wrap());
                    ui.add_space(8.);ui.label(crate::tr!("Видалено: {}", "Deleted: {}",deleted_date(item.entry.time_deleted)));
                    ui.label(if item.directory{crate::tr!("Тип: папка", "Type: folder")}else{crate::tr!("Тип: файл або посилання", "Type: file or link")});
                }
                if picked.len()>1 {ui.add_space(6.);ui.label(crate::tr!("І ще {} елементів", "And {} more items",picked.len()-1));}
            }
            ui.add_space(16.);ui.label(RichText::new(crate::tr!("Остаточне видалення звільняє місце на диску. Після нього повернути файли з кошика буде неможливо.", "Permanent deletion frees disk space. You will no longer be able to restore these files from Trash.")).size(12.).color(color(&t.muted)));
        });
        let r = Rect::from_min_size(
            Pos2::new(area.left(), area.bottom() - 83.),
            Vec2::new(area.width(), 36.),
        );
        if visual_button(
            ui,
            &self.art,
            r,
            16,
            crate::tr!("Відновити вибране", "Restore selected"),
            enabled,
            &t,
        )
        .clicked()
        {
            self.bin_restore(ui.ctx());
        }
        let mut danger = t.clone();
        danger.text = t.accent.clone();
        danger.border = t.accent.clone();
        if visual_button(
            ui,
            &self.art,
            r.translate(Vec2::new(0., 47.)),
            23,
            crate::tr!("Видалити остаточно", "Delete permanently"),
            enabled,
            &danger,
        )
        .clicked()
        {
            self.bin_request_purge(false);
        }
    }
}
fn check_at(ui: &mut egui::Ui, rect: Rect, selected: &mut bool, enabled: bool) -> egui::Response {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(
        egui::Layout::centered_and_justified(egui::Direction::TopDown),
    ));
    child.add_enabled(enabled, egui::Checkbox::without_text(selected))
}
fn item_size(i: &recycle::Item) -> String {
    match i.bytes {
        Some(b) => format!(
            "{}{}",
            if i.complete_size { "" } else { "≥ " },
            display_size(b)
        ),
        None => crate::tr!("Рахуємо…", "Calculating…").into(),
    }
}
fn deleted_date(t: i64) -> String {
    chrono::DateTime::from_timestamp(t, 0)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%d.%m.%Y %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "—".into())
}
fn purge_dialog(
    ctx: &egui::Context,
    targets: &[recycle::Item],
    t: &Theme,
    fresh: bool,
    enabled: bool,
) -> egui::ModalResponse<(egui::Response, egui::Response)> {
    egui::Modal::new(egui::Id::unique("purge-trash-confirmation")).show(ctx, |ui| {
        ui.set_width(500.);
        ui.heading(crate::tr!(
            "Видалити з кошика назавжди?",
            "Permanently delete from Trash?"
        ));
        ui.add_space(8.);
        ui.label(
            RichText::new(crate::tr!(
                "Повернути ці файли з кошика буде неможливо.",
                "You will not be able to restore these files from Trash."
            ))
            .color(color(&t.accent))
            .strong(),
        );
        ui.add_space(10.);
        egui::ScrollArea::vertical()
            .max_height(200.)
            .show(ui, |ui| {
                for item in targets {
                    ui.label(RichText::new(item.entry.name.to_string_lossy()).strong());
                    ui.add(egui::Label::new(item.entry.original_path().to_string_lossy()).wrap());
                    ui.add_space(6.);
                }
            });
        let bytes = targets
            .iter()
            .fold(0u64, |n, i| n.saturating_add(i.bytes.unwrap_or(0)));
        let complete = targets.iter().all(|i| i.bytes.is_some() && i.complete_size);
        ui.label(crate::tr!(
            "Елементів: {} · {}{}",
            "Items: {} · {}{}",
            targets.len(),
            if complete {
                ""
            } else {
                crate::tr!("щонайменше ", "at least ")
            },
            display_size(bytes)
        ));
        ui.add_space(16.);
        ui.horizontal(|ui| {
            let cancel = ui.button(crate::tr!("Скасувати", "Cancel"));
            if fresh {
                cancel.request_focus();
            }
            let yes = ui.add_enabled(
                enabled,
                egui::Button::new(
                    RichText::new(crate::tr!("Видалити остаточно", "Delete permanently"))
                        .color(C::WHITE),
                )
                .fill(color(&t.accent)),
            );
            (cancel, yes)
        })
        .inner
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn row_checkboxes_do_not_overlap_following_rows() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            let (first, _) = ui.allocate_exact_size(Vec2::new(600., 64.), Sense::click());
            let cursor = ui.cursor();
            let mut selected = false;
            check_at(
                ui,
                Rect::from_center_size(first.center(), Vec2::splat(23.)),
                &mut selected,
                true,
            );
            assert_eq!(ui.cursor(), cursor);
            let (second, _) = ui.allocate_exact_size(Vec2::new(600., 64.), Sense::click());
            assert!(second.top() >= first.bottom());
        });
        output.drop_without_applying_deltas();
    }
    #[test]
    fn permanent_delete_dialog_requires_confirmation_and_cancels_on_escape() {
        let ctx = egui::Context::default();
        art::fonts(&ctx);
        let bin = recycle::Bin::demo();
        let theme = Theme::default();
        let frame = |events: Vec<egui::Event>, fresh: bool| {
            let mut result = None;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    result = Some(purge_dialog(ui.ctx(), &bin.items, &theme, fresh, true));
                },
            );
            output.drop_without_applying_deltas();
            result.unwrap()
        };
        let initial = frame(vec![], true);
        assert!(initial.inner.0.has_focus());
        assert!(!initial.inner.1.clicked());
        let escape = frame(
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        assert!(escape.should_close());
        assert!(!escape.inner.1.clicked());
        let stable = frame(vec![], false);
        let pos = stable.inner.1.rect.center();
        let down = frame(
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            false,
        );
        assert!(!down.inner.1.clicked());
        let up = frame(
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        assert!(up.inner.1.clicked());
    }
}
