use crate::app::Disco;
use crate::app::state::View;
use crate::app::ui::widgets::{chevron, mix, paint_logo, visual_button};
use crate::art;
use crate::format::{display_size, group_digits, plural, shorten};
use crate::index::Category;
use crate::motion;
use crate::refresh;
use crate::settings::color;
use eframe::egui::{self, Align2, Color32 as C, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use std::path::Path;
use std::time::{Duration, Instant};

impl Disco {
    pub(in crate::app) fn titlebar(&mut self, ui: &mut egui::Ui) {
        let t = &self.preferences.theme;
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 42.),
            Sense::click_and_drag(),
        );
        ui.painter()
            .rect_filled(rect, 0, mix(color(&t.panel), color(&t.raised), 0.35));
        ui.painter().hline(
            rect.x_range(),
            rect.bottom(),
            Stroke::new(1., color(&t.border)),
        );
        let logo_rect = Rect::from_min_size(rect.min + Vec2::new(17., 9.), Vec2::splat(25.));
        paint_logo(ui, logo_rect, color(&t.accent));
        ui.painter().text(
            rect.min + Vec2::new(52., 21.),
            Align2::LEFT_CENTER,
            "disco",
            art::bold(23.),
            color(&t.text),
        );
        ui.painter().text(
            rect.min + Vec2::new(129., 23.),
            Align2::LEFT_CENTER,
            env!("CARGO_PKG_VERSION"),
            FontId::proportional(11.),
            color(&t.muted),
        );
        for (slot, icon, help) in [
            (0, 21, crate::tr!("Закрити", "Close")),
            (1, 20, crate::tr!("Розгорнути", "Maximize")),
            (2, 19, crate::tr!("Згорнути", "Minimize")),
            (3, 7, crate::tr!("Налаштування", "Settings")),
        ] {
            let r = Rect::from_min_size(
                Pos2::new(rect.right() - 44. * (slot as f32 + 1.), rect.top() + 3.),
                Vec2::new(42., 36.),
            );
            let b = ui
                .interact(r, ui.scope_id().with(("chrome", slot)), Sense::click())
                .on_hover_text(help);
            let hover = motion::hover(ui, ("chrome-hover", slot), b.hovered());
            if hover > 0. {
                ui.painter().rect_filled(
                    r,
                    t.radius,
                    mix(
                        mix(color(&t.panel), color(&t.raised), 0.35),
                        color(&t.raised),
                        hover,
                    ),
                );
            }
            self.art.paint(
                ui,
                icon,
                Rect::from_center_size(r.center(), Vec2::splat(if slot == 3 { 19. } else { 15. })),
                color(&t.text),
            );
            if b.clicked() {
                match slot {
                    0 => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                    1 => {
                        let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                    }
                    2 => ui
                        .ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                    _ => {
                        self.preferences.snapshot = Some((
                            self.preferences.theme.clone(),
                            self.preferences.layout.clone(),
                        ));
                        self.preferences.draft = None;
                        self.preferences.error.clear();
                        self.preferences.open = true;
                    }
                }
            }
        }
        if response.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if response.double_clicked() {
            let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
        }
    }
    pub(in crate::app) fn drive_header(&mut self, ui: &mut egui::Ui) {
        if self.drive.checked_at.elapsed() > Duration::from_secs(3) {
            self.drive.device = None;
            self.drive.space = if self.demo {
                self.drive.device = Some(0);
                Some((915 * 1024u64.pow(3), 305 * 1024u64.pow(3)))
            } else {
                refresh::DiskSample::capture(Path::new(&self.browser.root)).map(|sample| {
                    self.drive.device = Some(sample.device);
                    if !self.scan.busy {
                        self.scan.auto_refresh.observe(sample, Instant::now());
                    }
                    (sample.total, sample.total.saturating_sub(sample.used))
                })
            };
            self.drive.checked_at = Instant::now();
        }
        let t = self.preferences.theme.clone();
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 84.), Sense::hover());
        ui.painter()
            .rect_filled(rect, 0, mix(color(&t.panel), color(&t.raised), 0.12));
        ui.painter().hline(
            rect.x_range(),
            rect.bottom(),
            Stroke::new(1., color(&t.border)),
        );
        self.art.paint(
            ui,
            8,
            Rect::from_min_size(rect.min + Vec2::new(24., 10.), Vec2::splat(56.)),
            C::WHITE,
        );
        let name = if self.preferences.layout.drive_name.trim().is_empty() {
            &self.drive.name
        } else {
            &self.preferences.layout.drive_name
        };
        ui.painter().text(
            rect.min
                + Vec2::new(
                    88.,
                    if self.preferences.layout.drive_model {
                        18.
                    } else {
                        30.
                    },
                ),
            Align2::LEFT_TOP,
            name.chars().take(23).collect::<String>(),
            art::bold(19.),
            color(&t.text),
        );
        if self.preferences.layout.drive_model {
            ui.painter().text(
                rect.min + Vec2::new(88., 47.),
                Align2::LEFT_TOP,
                &self.drive.model,
                FontId::proportional(14.),
                color(&t.muted),
            );
        }
        chevron(ui, rect.min + Vec2::new(299., 30.), true, color(&t.muted));
        if ui
            .interact(
                Rect::from_min_size(rect.min, Vec2::new(320., 84.)),
                ui.scope_id().with("choose_drive"),
                Sense::click(),
            )
            .on_hover_text(crate::tr!(
                "Обрати диск або папку",
                "Choose a drive or folder"
            ))
            .clicked()
        {
            if let Some(p) = rfd::FileDialog::new()
                .set_directory(&self.browser.root)
                .pick_folder()
            {
                self.browser.root = p.to_string_lossy().into_owned();
                self.refresh_drive();
            }
        }
        if let Some((total, free)) = self.drive.space {
            let sum = self
                .drive
                .device
                .map(|device| self.recycle.summary_for(device));
            let parts = storage_parts(total, free, sum.map(|s| s.bytes).unwrap_or(0));
            let used = parts.used;
            let left = rect.left() + 337.;
            let right = rect.right() - 252.;
            let line = Rect::from_min_size(
                Pos2::new(left, rect.top() + 23.),
                Vec2::new((right - left).max(30.), 12.),
            );
            ui.painter().rect_filled(line, 3, color(&t.border));
            let target = (line.width() * used as f32 / total.max(1) as f32).round();
            let intro = motion::enter(ui, "disk-appear", &self.browser.root);
            let fill_width =
                motion::value(ui, "disk-fill", target, 2.5).clamp(0., line.width()) * intro;

            ui.painter().rect_filled(
                Rect::from_min_size(line.min, Vec2::new(fill_width, line.height())),
                3,
                color(&t.accent),
            );
            let trash_target = line.width() * parts.trash as f32 / total.max(1) as f32;
            let trash_width =
                (motion::value(ui, ("disk-trash", &self.browser.root), trash_target, 2.5) * intro)
                    .clamp(0., fill_width);
            if trash_width > 0. {
                let amber = Rect::from_min_max(
                    Pos2::new(line.left() + fill_width - trash_width, line.top()),
                    Pos2::new(line.left() + fill_width, line.bottom()),
                );
                let radius = egui::CornerRadius {
                    nw: if trash_width >= fill_width { 3 } else { 0 },
                    sw: if trash_width >= fill_width { 3 } else { 0 },
                    ne: 3,
                    se: 3,
                };
                ui.painter().rect_filled(amber, radius, color(&t.trash));
            }
            ui.painter().text(
                Pos2::new(left, rect.top() + 46.),
                Align2::LEFT_TOP,
                crate::tr!("{} зайнято", "{} used", display_size(used)),
                FontId::proportional(14.),
                color(&t.text),
            );
            ui.painter().text(
                Pos2::new(right, rect.top() + 46.),
                Align2::RIGHT_TOP,
                crate::tr!("{} загалом", "{} total", display_size(total)),
                FontId::proportional(14.),
                color(&t.muted),
            );
            let measured = sum.filter(|_| self.recycle.loaded);
            let unavailable = !self.recycle.error.is_empty() && self.recycle.items.is_empty();
            let caption = if unavailable {
                crate::tr!("Кошик: недоступно", "Trash: unavailable").to_string()
            } else if let Some(s) = measured {
                if s.entries > 0 && s.bytes == 0 && !s.complete {
                    crate::tr!("У кошику: рахуємо…", "In Trash: calculating…").to_string()
                } else {
                    crate::tr!(
                        "У кошику: {}{}",
                        "In Trash: {}{}",
                        if s.complete { "" } else { "≈ " },
                        display_size(s.bytes)
                    )
                }
            } else {
                crate::tr!("Кошик: перевіряємо…", "Trash: checking…").to_string()
            };
            ui.interact(
                line.expand(3.),
                ui.scope_id().with("storage-breakdown"),
                Sense::hover(),
            )
            .on_hover_text(crate::tr!(
                "Інше зайняте місце: {}\nКошик: {}{}\nВільно: {}",
                "Other used space: {}\nTrash: {}{}\nFree: {}",
                display_size(parts.other),
                if measured.is_some_and(|s| s.complete) {
                    ""
                } else {
                    "≈ "
                },
                display_size(parts.trash),
                display_size(free)
            ));
            let font = FontId::proportional(if line.width() < 600. { 12. } else { 14. });
            let galley = ui.painter().layout_no_wrap(caption, font, color(&t.trash));
            let legend_width = galley.size().x + 15.;
            let legend = Rect::from_min_size(
                Pos2::new(line.center().x - legend_width * 0.5, rect.top() + 46.),
                Vec2::new(legend_width, 21.),
            );
            let response = ui.interact(
                legend.expand(4.),
                ui.scope_id().with("trash-usage"),
                Sense::click(),
            );
            ui.painter().circle_filled(
                Pos2::new(legend.left() + 4., legend.top() + 8.),
                3.,
                color(&t.trash),
            );
            ui.painter()
                .galley(legend.min + Vec2::new(15., 0.), galley, color(&t.trash));
            if response.hovered() {
                ui.painter().hline(
                    legend.x_range(),
                    legend.bottom(),
                    Stroke::new(1., color(&t.trash).gamma_multiply(0.5)),
                );
            }
            let tooltip = if unavailable {
                self.recycle.error.clone()
            } else if let Some(s) = measured {
                crate::tr!(
                    "{} {} · {} {}\nПоказано кошик на цьому диску. Він уже входить у зайняте місце.{}\nНатисни, щоб відкрити кошик.",
                    "{} {} · {} {}\nShowing Trash on this drive. It is already included in used space.{}\nClick to open Trash.",
                    group_digits(s.files),
                    plural(
                        s.files,
                        crate::tr!("файл", "file"),
                        crate::tr!("файли", "files"),
                        crate::tr!("файлів", "files")
                    ),
                    group_digits(s.folders),
                    plural(
                        s.folders,
                        crate::tr!("папка", "folder"),
                        crate::tr!("папки", "folders"),
                        crate::tr!("папок", "folders")
                    ),
                    if s.complete {
                        ""
                    } else {
                        crate::tr!(
                            " Підрахунок ще триває або частина даних недоступна.",
                            " Still calculating, or some data is unavailable."
                        )
                    }
                )
            } else {
                crate::tr!(
                    "Підрахунок кошика у фоні. Натисни, щоб відкрити його.",
                    "Calculating Trash usage in the background. Click to open it."
                )
                .into()
            };
            if response.on_hover_text(tooltip).clicked() {
                self.browser.view = View::Trash;
                self.browser.filter.clear();
                self.browser.properties = None;
            }
            if let Some(s) = measured.filter(|s| s.files > 0 || s.complete) {
                ui.painter().text(
                    Pos2::new(rect.right() - 22., rect.top() + 61.),
                    Align2::RIGHT_CENTER,
                    crate::tr!(
                        "{}{} {} у кошику",
                        "{}{} {} in Trash",
                        if s.complete { "" } else { "≈ " },
                        group_digits(s.files),
                        plural(
                            s.files,
                            crate::tr!("файл", "file"),
                            crate::tr!("файли", "files"),
                            crate::tr!("файлів", "files")
                        )
                    ),
                    FontId::proportional(12.),
                    color(&t.trash),
                );
            }
            let word = ui.painter().layout_no_wrap(
                crate::tr!("вільно", "free").into(),
                FontId::proportional(16.),
                color(&t.text),
            );
            let x = rect.right() - 22.;
            ui.painter().text(
                Pos2::new(x, rect.top() + 37.),
                Align2::RIGHT_CENTER,
                crate::tr!("вільно", "free"),
                FontId::proportional(16.),
                color(&t.text),
            );
            ui.painter().text(
                Pos2::new(x - word.size().x - 8., rect.top() + 36.),
                Align2::RIGHT_CENTER,
                display_size(free),
                art::bold(28.),
                color(&t.text),
            );
        }
    }
    pub(in crate::app) fn footer(&mut self, ui: &mut egui::Ui) {
        let t = self.preferences.theme.clone();
        let total: u64 = if self.browser.view == View::Trash {
            self.recycle
                .items
                .iter()
                .filter(|i| self.recycle.selected.contains(&i.entry.id))
                .map(|i| i.bytes.unwrap_or(0))
                .sum()
        } else {
            self.candidates()
                .iter()
                .filter(|f| self.browser.selected.contains(&f.path))
                .map(|f| f.bytes)
                .sum()
        };
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.), Sense::hover());
        ui.painter()
            .rect_filled(r, 0, mix(color(&t.panel), color(&t.raised), 0.25));
        ui.painter()
            .hline(r.x_range(), r.top(), Stroke::new(1., color(&t.border)));
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(Rect::from_min_size(
                r.min + Vec2::new(12., 5.),
                Vec2::new(38., 32.),
            )),
            |ui| {
                ui.menu_button("···", |ui| {
                    for (view, name) in [
                        (View::Folders, crate::tr!("Усі папки", "All folders")),
                        (View::Files, crate::tr!("Великі файли", "Large files")),
                        (View::Cleanup, crate::tr!("Очищення", "Cleanup")),
                        (View::Duplicates, crate::tr!("Дублікати", "Duplicates")),
                        (View::Trash, crate::tr!("Кошик", "Trash")),
                    ] {
                        if ui.button(name).clicked() {
                            self.browser.view = view;
                            if view == View::Trash {
                                self.browser.filter.clear();
                                self.browser.properties = None;
                            }
                            if view == View::Folders {
                                self.browser.category = Category::All;
                                self.browser.table_root = None;
                            }
                            ui.close();
                        }
                    }
                });
            },
        );
        ui.painter().text(
            r.min + Vec2::new(58., 21.),
            Align2::LEFT_CENTER,
            crate::tr!(
                "Вибрано: {} ({} елементів)",
                "Selected: {} ({} items)",
                display_size(total),
                if self.browser.view == View::Trash {
                    self.recycle.selected.len()
                } else {
                    self.browser.selected.len()
                }
            ),
            FontId::proportional(14.),
            color(&t.muted),
        );
        if self.demo {
            ui.painter().text(
                r.min + Vec2::new(390., 21.),
                Align2::LEFT_CENTER,
                crate::tr!("ДЕМО", "DEMO"),
                FontId::proportional(11.),
                color(&t.accent),
            );
        }
        let button = Rect::from_min_size(
            Pos2::new(r.right() - 238., r.top() + 5.),
            Vec2::new(224., 32.),
        );
        if visual_button(
            ui,
            &self.art,
            button,
            10,
            if self.browser.view == View::Trash {
                crate::tr!("Відновити вибране", "Restore selected")
            } else {
                crate::tr!("Переглянути вибране", "Review selected")
            },
            if self.browser.view == View::Trash {
                !self.recycle.selected.is_empty() && self.bin_actions_enabled()
            } else {
                !self.browser.selected.is_empty()
                    && !self.scan.busy
                    && !self.demo
                    && self.deletion.rx.is_none()
                    && !self.recycle.busy
            },
            &t,
        )
        .clicked()
        {
            if self.browser.view == View::Trash {
                self.bin_restore(ui.ctx());
            } else {
                let folders = self
                    .candidates()
                    .into_iter()
                    .filter(|f| self.browser.selected.contains(&f.path))
                    .map(|f| (f.path, f.bytes))
                    .collect();
                self.request_delete(folders);
            }
        }
        if !self.message.is_empty() {
            let hint = ui.interact(
                Rect::from_min_max(
                    Pos2::new(r.left() + 340., r.top()),
                    Pos2::new(button.left() - 8., r.bottom()),
                ),
                ui.scope_id().with("scan_status"),
                Sense::hover(),
            );
            hint.on_hover_text(&self.message);
            if !self.demo {
                ui.painter().text(
                    Pos2::new(button.left() - 15., r.center().y),
                    Align2::RIGHT_CENTER,
                    shorten(&self.message, 43),
                    FontId::proportional(11.),
                    color(&t.muted),
                );
            }
        }
    }
}
#[derive(Debug, PartialEq)]
struct StorageParts {
    used: u64,
    trash: u64,
    other: u64,
}
fn storage_parts(total: u64, free: u64, trash: u64) -> StorageParts {
    let used = total.saturating_sub(free.min(total));
    let trash = trash.min(used);
    StorageParts {
        used,
        trash,
        other: used - trash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn storage_bar_partitions_used_space_without_double_counting() {
        let p = storage_parts(1000, 200, 300);
        assert_eq!(
            p,
            StorageParts {
                used: 800,
                trash: 300,
                other: 500
            }
        );
        assert_eq!(p.other + p.trash + 200, 1000);
        assert_eq!(storage_parts(100, 40, 999).trash, 60);
        assert_eq!(
            storage_parts(0, 0, 9),
            StorageParts {
                used: 0,
                trash: 0,
                other: 0
            }
        );
        assert_eq!(storage_parts(10, 20, 9).used, 0);
        let p = storage_parts(u64::MAX, 1, u64::MAX);
        assert_eq!(p.other, 0);
        assert_eq!(p.trash, u64::MAX - 1);
        assert_eq!(
            plural(
                1,
                crate::tr!("файл", "file"),
                crate::tr!("файли", "files"),
                crate::tr!("файлів", "files")
            ),
            crate::tr!("файл", "file")
        );
        assert_eq!(
            plural(
                22,
                crate::tr!("файл", "file"),
                crate::tr!("файли", "files"),
                crate::tr!("файлів", "files")
            ),
            crate::tr!("файли", "files")
        );
        assert_eq!(
            plural(
                111,
                crate::tr!("файл", "file"),
                crate::tr!("файли", "files"),
                crate::tr!("файлів", "files")
            ),
            crate::tr!("файлів", "files")
        );
    }
}
