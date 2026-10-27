use crate::app::Disco;
use crate::app::state::{NavState, View};
use crate::app::ui::widgets::{mix, visual_button};
use crate::art;
use crate::filesystem::date;
use crate::format::display_size;
use crate::index::Category;
use crate::motion;
use crate::settings::color;
use eframe::egui::{self, Align2, Color32 as C, FontId, Pos2, Rect, RichText, Sense, Vec2};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

impl Disco {
    pub(in crate::app) fn nav_state(&self) -> NavState {
        NavState {
            view: self.browser.view,
            category: self.browser.category,
            folder: self.browser.table_root,
        }
    }
    pub(in crate::app) fn history_step(&mut self, forward: bool) {
        let current = self.nav_state();
        let target = if forward {
            self.browser.history.forward(current)
        } else {
            self.browser.history.back(current)
        };
        if let Some(target) = target {
            self.browser.view = target.view;
            self.browser.category = target.category;
            self.browser.table_root = target.folder;
            self.browser.filter.clear();
            self.browser.properties = None;
            self.browser.nav_traversed = true;
            if let Some(id) = target.folder {
                self.select_folder(id);
            }
        }
    }
    pub(in crate::app) fn scope_controls(&mut self, ui: &mut egui::Ui) {
        let t = self.preferences.theme.clone();
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 36.), Sense::hover());
        for (offset, icon, forward, enabled, help) in [
            (
                0.,
                13,
                false,
                self.browser.history.can_back(),
                crate::tr!(
                    "Назад · бічна кнопка миші / Alt+←",
                    "Back · mouse side button / Alt+←"
                ),
            ),
            (
                34.,
                22,
                true,
                self.browser.history.can_forward(),
                crate::tr!(
                    "Уперед · бічна кнопка миші / Alt+→",
                    "Forward · mouse side button / Alt+→"
                ),
            ),
        ] {
            let r = Rect::from_min_size(rect.min + Vec2::new(offset, 0.), Vec2::new(28., 34.));
            let response = ui
                .interact(
                    r,
                    ui.scope_id().with(("navigation", forward)),
                    if enabled {
                        Sense::click()
                    } else {
                        Sense::hover()
                    },
                )
                .on_hover_text(help);
            if enabled && response.hovered() {
                ui.painter().rect_filled(r, t.radius, color(&t.raised));
            }
            self.art.paint(
                ui,
                icon,
                Rect::from_center_size(r.center(), Vec2::splat(19.)),
                color(&t.muted).gamma_multiply(if enabled { 1. } else { 0.4 }),
            );
            if enabled && response.clicked() {
                self.history_step(forward);
            }
        }
        let home_rect = Rect::from_min_size(rect.min + Vec2::new(68., 0.), Vec2::new(28., 34.));
        let home = ui
            .interact(home_rect, ui.scope_id().with("root_folder"), Sense::click())
            .on_hover_text(crate::tr!(
                "Усі папки · корінь сканування",
                "All folders · scan root"
            ));
        if home.hovered() {
            ui.painter()
                .rect_filled(home_rect, t.radius, color(&t.raised));
        }
        self.art.paint(
            ui,
            12,
            Rect::from_center_size(home_rect.center(), Vec2::splat(19.)),
            color(&t.text),
        );
        if home.clicked() {
            self.browser.category = Category::All;
            self.browser.table_root = None;
            self.browser.view = View::Folders;
            self.expand_folder(0);
            self.browser.filter.clear();
        }
        let path = self
            .browser
            .table_root
            .and_then(|i| self.scan.report.as_ref().map(|r| r.folders[i].path.clone()))
            .or_else(|| {
                if self.browser.category != Category::All {
                    self.scan
                        .index
                        .roots(self.browser.category)
                        .first()
                        .and_then(|i| {
                            self.scan
                                .report
                                .as_ref()
                                .map(|r| r.folders[*i].path.clone())
                        })
                } else {
                    None
                }
            })
            .unwrap_or_else(|| PathBuf::from(&self.browser.root));
        let crumb_width = (width - 461.).max(65.);
        let crumb_rect =
            Rect::from_min_size(rect.min + Vec2::new(106., 0.), Vec2::new(crumb_width, 36.));
        let mut path_text = path
            .to_string_lossy()
            .replace('/', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("   ›   ");
        if path_text.is_empty() {
            path_text = "/".into();
        }
        let max = ((crumb_width - 10.) / 7.).max(8.) as usize;
        if path_text.chars().count() > max {
            path_text = format!(
                "…{}",
                path_text
                    .chars()
                    .rev()
                    .take(max - 1)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<String>()
            );
        }
        ui.painter().with_clip_rect(crumb_rect).text(
            crumb_rect.left_center(),
            Align2::LEFT_CENTER,
            path_text,
            FontId::proportional(14.),
            color(&t.text),
        );
        let scan = Rect::from_min_size(
            Pos2::new(rect.right() - 145., rect.top()),
            Vec2::new(145., 34.),
        );
        let search = Rect::from_min_max(
            Pos2::new(scan.left() - 207., rect.top()),
            Pos2::new(scan.left() - 8., rect.bottom() - 2.),
        );
        ui.put(
            search,
            egui::TextEdit::singleline(&mut self.browser.filter)
                .hint_text(crate::tr!("Пошук у цій папці…", "Search in this folder…"))
                .font(FontId::proportional(14.))
                .margin(egui::Margin {
                    left: 29,
                    right: 6,
                    top: 8,
                    bottom: 6,
                }),
        );
        self.art.paint(
            ui,
            9,
            Rect::from_min_size(search.min + Vec2::new(9., 9.), Vec2::splat(15.)),
            color(&t.muted),
        );
        if visual_button(
            ui,
            &self.art,
            scan,
            11,
            if self.scan.report.is_some() {
                crate::tr!("Оновити", "Refresh")
            } else {
                crate::tr!("Сканувати", "Scan")
            },
            !self.scan.busy && self.deletion.rx.is_none() && !self.recycle.busy && self.scan.restore_requested_root.is_none(),
            &t,
        )
        .on_hover_text(
            crate::tr!("Перевірити події змін; знімок до 60 с можна використати повторно. Shift + клік — завжди повний обхід.", "Check for changes; reuse a snapshot up to 60 s old. Shift + click always runs a full scan."),
        )
        .clicked()
        {
            self.start(false, ui.input(|i| i.modifiers.shift));
            self.browser.view = View::Folders;
            self.browser.table_root = None;
        }
        if self.scan.restored_snapshot
            || self.scan.auto_refresh.pending()
            || (self.scan.busy && !self.scan.duplicate_job && self.scan.report.is_some())
        {
            ui.add_space(4.);
            let caption = if self.scan.busy {
                crate::tr!(
                    "Оновлюємо дані — поки показано попередній скан.",
                    "Refreshing — showing the previous scan for now."
                )
                .to_string()
            } else if self.preferences.layout.auto_refresh && self.scan.auto_refresh.pending() {
                crate::tr!(
                    "На SSD є зміни — заплановано фонове оновлення.",
                    "SSD usage changed — background refresh scheduled."
                )
                .to_string()
            } else if let Some(at) = self.scan.saved_snapshot_at {
                crate::tr!(
                    "Збережений скан від {}. {}",
                    "Saved scan from {}. {}",
                    date(std::time::UNIX_EPOCH + Duration::from_secs(at)),
                    if self.preferences.layout.auto_refresh {
                        crate::tr!(
                            "Зміни зайнятого місця відстежуються автоматично.",
                            "Disk usage changes are monitored automatically."
                        )
                    } else {
                        crate::tr!(
                            "Для перевірки змін натисни «Оновити».",
                            "Click Refresh to check for changes."
                        )
                    }
                )
            } else {
                crate::tr!(
                    "Попередній результат — натисни «Оновити» для перевірки змін.",
                    "Previous result — click Refresh to check for changes."
                )
                .to_string()
            };
            ui.add(
                egui::Label::new(RichText::new(caption).size(12.).color(color(&t.muted))).wrap(),
            );
        }
        if self.scan.busy {
            ui.add_space(5.);
            ui.horizontal(|ui| {
                motion::activity(
                    ui,
                    self.scan.control.pause.load(Ordering::Relaxed),
                    color(&t.accent),
                );
                ui.label(if self.scan.duplicate_job {
                    crate::tr!("Пошук дублікатів", "Finding duplicates")
                } else {
                    if self.scan.automatic_job {
                        crate::tr!("Автооновлення", "Auto refresh")
                    } else {
                        crate::tr!("Сканування", "Scanning")
                    }
                });
                let paused = self.scan.control.pause.load(Ordering::Relaxed);
                if ui
                    .button(if paused {
                        crate::tr!("Продовжити", "Resume")
                    } else {
                        crate::tr!("Пауза", "Pause")
                    })
                    .clicked()
                {
                    self.scan.control.pause.store(!paused, Ordering::Relaxed);
                }
                if ui.button(crate::tr!("Скасувати", "Cancel")).clicked() {
                    self.scan.control.cancel.store(true, Ordering::Relaxed);
                }
                let mut gentle = self.scan.control.gentle.load(Ordering::Relaxed);
                if ui
                    .checkbox(&mut gentle, crate::tr!("Бережний режим", "Gentle mode"))
                    .changed()
                {
                    self.preferences.layout.gentle_scan = gentle;
                    self.scan.control.gentle.store(gentle, Ordering::Relaxed);
                }
                if let Some(p) = &self.scan.progress {
                    ui.label(
                        RichText::new(crate::tr!(
                            "{} · {} записів",
                            "{} · {} entries",
                            display_size(p.bytes),
                            p.entries
                        ))
                        .size(12.)
                        .color(color(&t.muted)),
                    )
                    .on_hover_text(crate::tr!(
                        "{}\nПропущено: {}",
                        "{}\nSkipped: {}",
                        p.current.display(),
                        p.skipped
                    ));
                }
            });
        }
    }
    pub(in crate::app) fn sidebar(&mut self, ui: &mut egui::Ui) {
        let t = self.preferences.theme.clone();
        let row_height = ((ui.available_height() - 20.) / 5.).clamp(60., 72.);
        egui::ScrollArea::vertical()
            .id_salt("category-list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.;
                ui.add_space(9.);
                for (cat, kind) in [
                    (Category::Games, 1),
                    (Category::Development, 2),
                    (Category::Cache, 3),
                    (Category::Documents, 4),
                    (Category::Other, 5),
                ] {
                    let active = self.browser.view == View::Folders && self.browser.category == cat;
                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), row_height),
                        Sense::click(),
                    );
                    let hover = motion::hover(ui, ("category-hover", kind), response.hovered());
                    let selected = motion::value(
                        ui,
                        ("category-selected", kind),
                        if active { 1. } else { 0. },
                        1.,
                    );
                    let strength = selected.max(hover * 0.65);
                    if strength > 0. {
                        ui.painter().rect_filled(
                            rect,
                            0,
                            mix(color(&t.panel), color(&t.raised), strength),
                        );
                    }
                    if active {
                        let y = motion::value(ui, "category-rail-y", rect.top(), 1.15);
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                Pos2::new(rect.left(), y),
                                Vec2::new(4., rect.height()),
                            ),
                            0,
                            color(&t.accent),
                        );
                    }
                    self.art.paint(
                        ui,
                        kind,
                        Rect::from_center_size(
                            rect.min + Vec2::new(35., 34.),
                            Vec2::splat(30. * self.preferences.layout.icon_scale),
                        ),
                        C::WHITE,
                    );
                    ui.painter().text(
                        rect.min
                            + Vec2::new(
                                64.,
                                if self.preferences.layout.category_sizes {
                                    20.
                                } else {
                                    28.
                                },
                            ),
                        Align2::LEFT_TOP,
                        cat.name(),
                        art::bold(15. * self.preferences.layout.text_scale),
                        color(&t.text),
                    );
                    if self.preferences.layout.category_sizes {
                        let bytes = self
                            .scan
                            .report
                            .as_ref()
                            .map(|_| self.scan.index.total(cat));
                        ui.painter().text(
                            rect.min + Vec2::new(64., 42.),
                            Align2::LEFT_TOP,
                            bytes.map(display_size).unwrap_or_else(|| "—".into()),
                            FontId::proportional(14.),
                            color(&t.muted),
                        );
                    }
                    if response.clicked() {
                        self.browser.category = cat;
                        self.browser.view = View::Folders;
                        self.browser.filter.clear();
                        self.browser.table_root = None;
                        self.browser.properties = None;
                        if let Some(id) = self.scan.index.roots(cat).first().copied() {
                            self.expand_folder(id);
                            self.select_folder(id);
                        }
                    }
                }
                ui.add_space(8.);
                ui.separator();
                let active = self.browser.view == View::Trash;
                let (r, response) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 54.), Sense::click());
                let hover = motion::hover(ui, "trash-navigation", response.hovered());
                ui.painter().rect_filled(
                    r,
                    0,
                    mix(
                        color(&t.panel),
                        color(&t.raised),
                        if active { 1. } else { hover * 0.65 },
                    ),
                );
                if active {
                    let y = motion::value(ui, "category-rail-y", r.top(), 1.15);
                    ui.painter().rect_filled(
                        Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(4., r.height())),
                        0,
                        color(&t.accent),
                    );
                }
                self.art.paint(
                    ui,
                    23,
                    Rect::from_center_size(
                        r.min + Vec2::new(35., 27.),
                        Vec2::splat(27. * self.preferences.layout.icon_scale),
                    ),
                    color(&t.text),
                );
                ui.painter().text(
                    r.min + Vec2::new(64., 18.),
                    Align2::LEFT_TOP,
                    crate::tr!("Кошик", "Trash"),
                    art::bold(15. * self.preferences.layout.text_scale),
                    color(&t.text),
                );
                if response.clicked() {
                    self.browser.view = View::Trash;
                    self.browser.filter.clear();
                    self.browser.properties = None;
                }
            });
    }
}
