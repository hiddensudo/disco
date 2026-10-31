use crate::app::Disco;
use crate::app::state::{NavState, View};
use crate::app::ui::widgets::visual_button;
use crate::filesystem::date;
use crate::format::display_size;
use crate::index::Category;
use crate::motion;
use crate::settings::color;
use eframe::egui::{self, FontId, Pos2, Rect, RichText, Sense, Vec2};
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
            self.browser.category = if matches!(target.category, Category::Custom(_))
                && self.collections.config.get(target.category).is_none()
            {
                Category::All
            } else {
                target.category
            };
            self.browser.table_root = target.folder;
            self.browser.filter.clear();
            self.browser.properties = None;
            self.browser.nav_traversed = true;
            if let Some(id) = target.folder {
                self.select_folder(id);
            }
        }
    }
    pub(in crate::app) fn breadcrumb_directory(&self) -> PathBuf {
        if let Some(p) = &self.browser.properties {
            if p.is_dir {
                return p.path.clone();
            }
            if let Some(parent) = p.path.parent() {
                return parent.to_path_buf();
            }
        }
        if let Some(untitled) = &self.editor.untitled {
            return untitled.directory.clone();
        }
        self.browser
            .table_root
            .and_then(|id| {
                self.scan
                    .report
                    .as_ref()
                    .and_then(|r| r.folders.get(id))
                    .map(|f| f.path.clone())
            })
            .or_else(|| {
                self.scan
                    .index
                    .roots(self.browser.category)
                    .first()
                    .and_then(|id| self.scan.report.as_ref().and_then(|r| r.folders.get(*id)))
                    .map(|f| f.path.clone())
            })
            .unwrap_or_else(|| PathBuf::from(&self.browser.root))
    }
    pub(in crate::app) fn scope_controls(&mut self, ui: &mut egui::Ui) {
        let t = self.preferences.theme.clone();
        let width = ui.available_width();
        let narrow = width < 600.;
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(width, if narrow { 76. } else { 36. }),
            Sense::hover(),
        );
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
            .interact(home_rect, egui::Id::unique("folder-root"), Sense::click())
            .on_hover_text(crate::tr!(
                "Додому · шлях із налаштувань",
                "Home · configured path"
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
            self.editor_action(
                crate::app::editor::Action::Navigate(self.preferences.layout.home_directory()),
                ui.ctx(),
            );
        }
        let path = self.breadcrumb_directory();
        let scan_width = if narrow { 34. } else { 145. };
        let scan = Rect::from_min_size(
            Pos2::new(
                rect.right() - scan_width,
                rect.top() + if narrow { 40. } else { 0. },
            ),
            Vec2::new(scan_width, 34.),
        );
        let search = if narrow {
            Rect::from_min_max(
                Pos2::new(rect.left(), rect.top() + 40.),
                Pos2::new(scan.left() - 50., rect.top() + 74.),
            )
        } else {
            Rect::from_min_max(
                Pos2::new(scan.left() - 249., rect.top()),
                Pos2::new(scan.left() - 50., rect.top() + 34.),
            )
        };
        let crumb_rect = Rect::from_min_max(
            rect.min + Vec2::new(106., 0.),
            Pos2::new(
                if narrow {
                    rect.right()
                } else {
                    search.left() - 8.
                },
                rect.top() + 36.,
            ),
        );
        self.breadcrumb_path(ui, crumb_rect, &path);
        // Semantic bounds keep narrow-panel spacing testable without pixel guesses.
        ui.interact(rect, egui::Id::unique("folder-toolbar"), Sense::hover());
        ui.interact(
            search,
            egui::Id::unique("folder-search-bounds"),
            Sense::hover(),
        );
        ui.interact(
            scan,
            egui::Id::unique("folder-refresh-bounds"),
            Sense::hover(),
        );
        let was_searching = !self.browser.filter.is_empty();
        let search_response = ui.put(
            search,
            egui::TextEdit::singleline(&mut self.browser.filter)
                .id(egui::Id::unique("folder-search"))
                .hint_text(crate::tr!("Пошук у папці…", "Search in folder…"))
                .font(FontId::proportional(14.))
                .margin(egui::Margin {
                    left: 29,
                    right: 6,
                    top: 8,
                    bottom: 6,
                }),
        );
        self.browser.text_input_focused = search_response.has_focus();
        if search_response.changed() {
            ui.ctx().request_repaint();
        }
        let scope = self
            .browser
            .table_root
            .and_then(|id| self.scan.report.as_ref().and_then(|r| r.folders.get(id)))
            .map(|f| f.path.display().to_string())
            .unwrap_or_else(|| self.browser.root.clone());
        search_response.on_hover_text(crate::tr!(
            "Пошук у: {scope}\nНатисни знайдену папку, щоб зайти в неї та шукати всередині.",
            "Search in: {scope}\nClick a matching folder to enter it and search inside.",
            scope = scope
        ));
        if was_searching && self.browser.filter.is_empty() {
            self.reveal_browser_selection();
        }
        self.art.paint(
            ui,
            9,
            Rect::from_min_size(search.min + Vec2::new(9., 9.), Vec2::splat(15.)),
            color(&t.muted),
        );
        let new_rect = Rect::from_min_size(
            Pos2::new(scan.left() - 42., scan.top()),
            Vec2::new(34., 34.),
        );
        if visual_button(
            ui,
            &self.art,
            new_rect,
            30,
            "",
            !self.demo
                && !self.scan.busy
                && self.browser.view != View::Trash
                && self.deletion.rx.is_none()
                && !self.recycle.busy
                && !self.preferences.open
                && !self.category_read_only()
                && !self.scoped.busy(),
            &t,
        )
        .on_hover_text(crate::tr!("Новий файл · Ctrl+N", "New file · Ctrl+N"))
        .clicked()
        {
            self.request_new_file(ui.ctx());
        }
        if visual_button(
            ui,
            &self.art,
            scan,
            11,
            if narrow {
                ""
            } else if self.scan.report.is_some() || self.paths.standalone.is_some() {
                crate::tr!("Оновити", "Refresh")
            } else {
                crate::tr!("Сканувати", "Scan")
            },
            !self.scan.busy
                && !self.scoped.busy()
                && self.deletion.rx.is_none()
                && !self.recycle.busy
                && self.scan.restore_requested_root.is_none(),
            &t,
        )
        .on_hover_text(if self.paths.standalone.is_some() {
            crate::tr!("Перечитати відкритий файл", "Reload the open file")
        } else {
            crate::tr!(
                "Оновити лише поточну папку або корені категорії",
                "Refresh only the current folder or category roots"
            )
        })
        .clicked()
        {
            if self.scoped.busy() {
                self.scoped.cancel();
            } else if self.scan.busy {
                self.scan.control.cancel.store(true, Ordering::Relaxed);
            } else {
                self.refresh_scope(ui.ctx());
            }
        }
        if self.preferences.layout.preview_mode == crate::settings::PreviewMode::Focus {
            return;
        }
        if self.scan.restored_snapshot {
            if let Some(at) = self.scan.saved_snapshot_at {
                ui.add(
                    egui::Label::new(
                        RichText::new(crate::tr!(
                            "Збережений скан від {} · «Оновити» перечитує поточний scope.",
                            "Saved scan from {} · Refresh rereads the current scope.",
                            date(std::time::UNIX_EPOCH + Duration::from_secs(at))
                        ))
                        .size(12.)
                        .color(color(&t.muted)),
                    )
                    .truncate(),
                );
            }
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
}
