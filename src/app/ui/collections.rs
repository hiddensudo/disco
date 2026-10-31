//! Category rail and user collections.
use crate::{
    app::{Disco, state::View},
    categories::{self, Collection},
    index::Category,
    settings::color,
};
use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Vec2};

#[derive(Clone)]
struct CategoryDrag(String);

impl Disco {
    pub(in crate::app) fn update_collections(&mut self) {
        if !self.preferences.open {
            self.collections.maintain();
        }
        let revision = (self.scan.report_revision, self.collections.revision);
        if self.collections.indexed == Some(revision) {
            return;
        }
        self.art.reload_custom_failures();
        self.collections.missing.clear();
        if let Some(report) = &self.scan.report {
            self.collections.missing = self
                .scan
                .index
                .apply_collections(report, &self.collections.config);
        }
        if self.scan.report.is_none() {
            for c in &self.collections.config.items {
                self.collections
                    .missing
                    .insert(c.category(), c.folders.clone());
            }
        }
        self.collections.indexed = Some(revision);
        self.browser.tree_revision = self.browser.tree_revision.wrapping_add(1);
        if matches!(self.browser.category, Category::Custom(_))
            && self.collections.config.get(self.browser.category).is_none()
        {
            self.browser.category = Category::All;
            self.browser.table_root = None;
        }
    }
    pub(in crate::app) fn open_collection_settings(&mut self, category: Option<Category>) {
        if !self.preferences.open {
            self.preferences.snapshot = Some((
                self.preferences.theme.clone(),
                self.preferences.layout.clone(),
            ));
        }
        self.preferences.open = true;
        self.preferences.page = 2;
        self.collections
            .draft
            .get_or_insert_with(|| self.collections.config.clone());
        if let Some(cat) = category {
            if let Some(i) = self
                .collections
                .config
                .items
                .iter()
                .position(|c| c.category() == cat)
            {
                self.collections.editing = i;
            }
        }
    }
    pub(in crate::app) fn robot_button(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        collection: &Collection,
    ) {
        let response = ui.interact(rect, ui.scope_id().with(("category-robot", &collection.id)), Sense::click())
            .on_hover_text(crate::tr!(
                "Скопіювати промпт для агента\nВстав його в чат з агентом, який має доступ до твого ПК. Він зможе знайти папки для цієї категорії й заповнити окремий TOML-файл цієї категорії.",
                "Copy a prompt for your agent\nPaste it into a chat with an agent that has access to your PC. It can find folders for this category and fill in this category’s own TOML file."
            ));
        let copied = ui
            .ctx()
            .data(|data| {
                data.get_temp::<f64>(ui.scope_id().with(("prompt-copied", &collection.id)))
            })
            .is_some_and(|at| ui.input(|i| i.time) - at < 2.);
        let t = &self.preferences.theme;
        if copied {
            response
                .clone()
                .on_hover_text(crate::tr!("Промпт скопійовано", "Prompt copied"));
        }
        if response.hovered() {
            ui.painter().rect_filled(rect, t.radius, color(&t.raised));
        }
        let shape = crate::motion::jelly(
            ui,
            ("category-robot-jelly", &collection.id),
            response.hovered(),
        );
        let icon = Rect::from_center_size(rect.center(), rect.shrink(5.).size() * shape);
        self.art.paint(
            ui,
            29,
            icon,
            if response.hovered() {
                color(&t.text)
            } else {
                color(&t.muted)
            },
        );
        if response.clicked() {
            ui.ctx().copy_text(categories::prompt(
                collection,
                &self.collections.file_for(collection),
                self.preferences.layout.effective_prompt_language(),
            ));
            let now = ui.input(|i| i.time);
            ui.ctx().data_mut(|data| {
                data.insert_temp(ui.scope_id().with(("prompt-copied", &collection.id)), now)
            });
        }
    }
    pub(in crate::app) fn sidebar(&mut self, ui: &mut egui::Ui) {
        let t = self.preferences.theme.clone();
        let labels = self.preferences.layout.category_labels;
        let compact_rail = !labels
            && self.terminal_shown()
            && self.preferences.layout.preview_mode == crate::settings::PreviewMode::Overview;
        let reorder = !self.preferences.layout.categories_locked && !self.preferences.open;
        let mut dropped = None;
        egui::Panel::bottom("collection-tools")
            .exact_size(if labels {
                40.
            } else if compact_rail {
                64.
            } else {
                76.
            })
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                if compact_rail {
                    ui.spacing_mut().item_spacing.y = 2.;
                }
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), if compact_rail { 32. } else { 38. }),
                    Sense::hover(),
                );
                let trash_rect = if labels {
                    Rect::from_min_max(rect.min, rect.max - Vec2::new(28., 0.))
                } else {
                    rect
                };
                let trash = ui
                    .interact(
                        trash_rect,
                        egui::Id::unique("trash-shortcut"),
                        Sense::click(),
                    )
                    .on_hover_text(crate::tr!("Кошик", "Trash"));
                if self.keyboard.reveal_category
                    && self.keyboard.category.as_deref() == Some("__trash")
                {
                    trash.request_focus();
                    self.keyboard.reveal_category = false;
                }
                if trash.has_focus() {
                    self.keyboard.category = Some("__trash".into());
                    ui.ctx().memory_mut(|m| {
                        m.set_focus_lock_filter(
                            trash.id,
                            egui::EventFilter {
                                horizontal_arrows: true,
                                vertical_arrows: true,
                                ..Default::default()
                            },
                        )
                    });
                }
                if trash.hovered() || trash.has_focus() || self.browser.view == View::Trash {
                    ui.painter().rect_filled(trash_rect, 0, color(&t.raised));
                }
                self.art.paint(
                    ui,
                    23,
                    Rect::from_center_size(
                        Pos2::new(rect.left() + 25., rect.center().y),
                        Vec2::splat(24.),
                    ),
                    color(&t.text),
                );
                if labels {
                    ui.painter().text(
                        rect.min + Vec2::new(60., 19.),
                        Align2::LEFT_CENTER,
                        crate::tr!("Кошик", "Trash"),
                        FontId::proportional(13.),
                        color(&t.text),
                    );
                }
                if trash.clicked() {
                    self.browser.view = View::Trash;
                    self.browser.properties = None;
                    self.browser.filter.clear();
                }
                let plus = if labels {
                    Rect::from_center_size(
                        Pos2::new(rect.right() - 14., rect.center().y),
                        Vec2::splat(27.),
                    )
                } else {
                    let (next, _) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), if compact_rail { 28. } else { 34. }),
                        Sense::hover(),
                    );
                    Rect::from_center_size(next.center(), Vec2::splat(30.))
                };
                let response = ui
                    .interact(
                        plus,
                        ui.scope_id().with("manage-collections"),
                        Sense::click(),
                    )
                    .on_hover_text(if self.collections.error.is_empty() {
                        crate::tr!("Керувати категоріями", "Manage categories")
                    } else {
                        &self.collections.error
                    });
                if response.hovered() {
                    ui.painter().rect_filled(plus, t.radius, color(&t.raised));
                }
                self.art.paint(ui, 30, plus.shrink(6.), color(&t.muted));
                if response.clicked() {
                    self.open_collection_settings(None);
                }
            });

        egui::ScrollArea::vertical()
            .id_salt("category-list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if compact_rail {
                    ui.spacing_mut().item_spacing.y = 0.;
                }
                ui.add_space(6.);
                // Reach off-screen categories without releasing the drag.
                if reorder && egui::DragAndDrop::has_payload_of_type::<CategoryDrag>(ui.ctx()) {
                    if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                        let clip = ui.clip_rect();
                        if clip.contains(pos) {
                            let direction = if pos.y < clip.top() + 28. {
                                1.
                            } else if pos.y > clip.bottom() - 28. {
                                -1.
                            } else {
                                0.
                            };
                            if direction != 0. {
                                let dt = ui.input(|i| i.stable_dt).min(0.05);
                                ui.scroll_with_delta(Vec2::new(0., direction * 280. * dt));
                                ui.ctx().request_repaint();
                            }
                        }
                    }
                }
                for collection in self.collections.config.items.clone() {
                    let cat = collection.category();
                    let active = self.paths.standalone.is_none()
                        && self.browser.view == View::Folders
                        && self.browser.category == cat;
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(
                            ui.available_width(),
                            if labels {
                                66.
                            } else if compact_rail {
                                30.
                            } else {
                                54.
                            },
                        ),
                        Sense::hover(),
                    );
                    let click_rect = rect;
                    let response = ui.interact(
                        click_rect,
                        egui::Id::unique(("category", &collection.id)),
                        if reorder {
                            Sense::click_and_drag()
                        } else {
                            Sense::click()
                        },
                    );
                    if reorder {
                        response.dnd_set_drag_payload(CategoryDrag(collection.id.clone()));
                        if let Some(payload) = response.dnd_hover_payload::<CategoryDrag>() {
                            if payload.0 != collection.id {
                                let after = ui
                                    .input(|i| i.pointer.hover_pos())
                                    .is_some_and(|p| p.y >= rect.center().y);
                                let y = if after { rect.bottom() } else { rect.top() };
                                ui.painter().hline(
                                    rect.shrink(4.).x_range(),
                                    y,
                                    egui::Stroke::new(2., color(&t.accent)),
                                );
                                if let Some(payload) =
                                    response.dnd_release_payload::<CategoryDrag>()
                                {
                                    dropped =
                                        Some((payload.0.clone(), collection.id.clone(), after));
                                }
                            }
                        }
                    }
                    if self.keyboard.reveal_category
                        && self.keyboard.category.as_deref() == Some(&collection.id)
                    {
                        response.scroll_to_me(Some(egui::Align::Center));
                        response.request_focus();
                        if ui.clip_rect().contains(rect.center()) {
                            self.keyboard.reveal_category = false;
                        }
                    }
                    if response.has_focus() {
                        ui.ctx().memory_mut(|m| {
                            m.set_focus_lock_filter(
                                response.id,
                                egui::EventFilter {
                                    horizontal_arrows: true,
                                    vertical_arrows: true,
                                    ..Default::default()
                                },
                            )
                        });
                        self.keyboard.category = Some(collection.id.clone());
                        ui.painter().rect_stroke(
                            rect.shrink(2.),
                            t.radius,
                            egui::Stroke::new(1., color(&t.accent)),
                            egui::StrokeKind::Inside,
                        );
                    }
                    let total = self
                        .scan
                        .report
                        .as_ref()
                        .filter(|_| {
                            collection.folders.is_empty() || !self.scan.index.roots(cat).is_empty()
                        })
                        .map(|_| self.scan.index.total(cat));
                    let help = format!(
                        "{}\n{}\n{}",
                        collection.title(),
                        total
                            .map(crate::format::display_size)
                            .unwrap_or_else(|| "—".into()),
                        if self.preferences.layout.categories_locked {
                            crate::tr!(
                                "Порядок закріплено в налаштуваннях",
                                "Order is locked in settings"
                            )
                        } else {
                            crate::tr!("Перетягни, щоб змінити порядок", "Drag to reorder")
                        }
                    );
                    let response = response.on_hover_text(help);
                    let hover =
                        crate::motion::hover(ui, ("category-hover", cat), response.hovered());
                    if active || hover > 0. {
                        ui.painter().rect_filled(
                            rect,
                            0,
                            super::widgets::mix(
                                color(&t.panel),
                                color(&t.raised),
                                if active { 1. } else { hover * 0.65 },
                            ),
                        );
                    }
                    if active {
                        ui.painter().rect_filled(
                            Rect::from_min_size(rect.min, Vec2::new(3., rect.height())),
                            0,
                            color(&t.accent),
                        );
                    }
                    let icon = self.art.category_icon(&collection.icon);
                    self.art.paint(
                        ui,
                        icon,
                        Rect::from_center_size(
                            Pos2::new(
                                rect.left() + if labels { 31. } else { 26. },
                                rect.center().y,
                            ),
                            Vec2::splat(
                                (if compact_rail { 24. } else { 27. })
                                    * self.preferences.layout.icon_scale,
                            ),
                        ),
                        crate::art::category_tint(icon, color(&t.text)),
                    );
                    if labels {
                        let text = Rect::from_min_max(
                            rect.min + Vec2::new(60., 0.),
                            rect.max - Vec2::new(10., 0.),
                        );
                        let painter = ui.painter().with_clip_rect(text);
                        painter.text(
                            Pos2::new(
                                text.left(),
                                rect.top()
                                    + if self.preferences.layout.category_sizes {
                                        18.
                                    } else {
                                        26.
                                    },
                            ),
                            Align2::LEFT_TOP,
                            collection.title(),
                            crate::art::bold(14.),
                            color(&t.text),
                        );
                        if self.preferences.layout.category_sizes {
                            painter.text(
                                Pos2::new(text.left(), rect.top() + 38.),
                                Align2::LEFT_TOP,
                                total
                                    .map(crate::format::display_size)
                                    .unwrap_or_else(|| "—".into()),
                                FontId::proportional(13.),
                                color(&t.muted),
                            );
                        }
                    }
                    response.context_menu(|ui| {
                        if ui
                            .button(crate::tr!("Налаштувати категорію…", "Edit category…"))
                            .clicked()
                        {
                            self.open_collection_settings(Some(cat));
                            ui.close();
                        }
                        if self.browser.properties.as_ref().is_some_and(|p| p.is_dir)
                            || !self.browser.selected.is_empty()
                        {
                            if ui
                                .button(crate::tr!("Додати вибрані папки", "Add selected folders"))
                                .clicked()
                            {
                                self.open_collection_settings(Some(cat));
                                let mut paths: Vec<_> = self
                                    .browser
                                    .selected
                                    .iter()
                                    .filter(|p| p.is_dir())
                                    .cloned()
                                    .collect();
                                if paths.is_empty() {
                                    if let Some(p) =
                                        self.browser.properties.as_ref().filter(|p| p.is_dir)
                                    {
                                        paths.push(p.path.clone());
                                    }
                                }
                                if let Some(c) = self
                                    .collections
                                    .draft
                                    .as_mut()
                                    .and_then(|d| d.items.get_mut(self.collections.editing))
                                {
                                    for path in paths {
                                        if !c.folders.contains(&path) {
                                            c.folders.push(path);
                                        }
                                    }
                                }
                                ui.close();
                            }
                        }
                    });
                    if response.clicked() {
                        self.leave_path_browser();
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
            });
        if let Some((source, target, after)) = dropped {
            if self.demo {
                if self.collections.config.reorder(&source, &target, after) {
                    self.collections.revision = self.collections.revision.wrapping_add(1);
                }
            } else if let Err(error) = self.collections.reorder(&source, &target, after) {
                self.message = error;
            }
            ui.ctx().request_repaint();
        }
        if reorder {
            if let Some(payload) = egui::DragAndDrop::payload::<CategoryDrag>(ui.ctx()) {
                if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    if let Some(c) = self
                        .collections
                        .config
                        .items
                        .iter()
                        .find(|c| c.id == payload.0)
                    {
                        egui::Area::new(egui::Id::unique("dragged-category"))
                            .order(egui::Order::Tooltip)
                            .interactable(false)
                            .fixed_pos(pos + Vec2::new(14., 10.))
                            .show(ui.ctx(), |ui| {
                                egui::Frame::new()
                                    .fill(color(&t.raised))
                                    .stroke(egui::Stroke::new(1., color(&t.accent)))
                                    .inner_margin(8.)
                                    .corner_radius(t.radius)
                                    .show(ui, |ui| {
                                        ui.label(c.title());
                                    });
                            });
                    }
                }
            }
        }
    }
    pub(in crate::app) fn unindexed_collection_folders(&mut self, ui: &mut egui::Ui) {
        if self.browser.view != View::Folders || self.browser.table_root.is_some() {
            return;
        }
        let Some(collection) = self.collections.config.get(self.browser.category).cloned() else {
            return;
        };
        if !collection.builtin() && collection.folders.is_empty() {
            ui.label(crate::tr!(
                "Ця категорія ще не має папок.",
                "This category has no folders yet."
            ));
            if ui
                .button(crate::tr!("Додати папки…", "Add folders…"))
                .clicked()
            {
                self.open_collection_settings(Some(collection.category()));
            }
        }
        let missing = self
            .collections
            .missing
            .get(&collection.category())
            .cloned()
            .unwrap_or_default();
        if missing.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(crate::tr!(
            "Папки поза поточним знімком ({})",
            "Folders outside the current snapshot ({})",
            missing.len()
        ))
        .default_open(true)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("unindexed-collection")
                .max_height(130.)
                .show(ui, |ui| {
                    for path in missing {
                        ui.horizontal(|ui| {
                            let button = ui.add_enabled(
                                !self.scan.busy,
                                egui::Button::new(crate::tr!("Сканувати", "Scan")),
                            );
                            if button.clicked() {
                                self.browser.root = path.to_string_lossy().into_owned();
                                self.start_job(false, false, false);
                            }
                            ui.add(egui::Label::new(path.to_string_lossy()).truncate())
                                .on_hover_text(path.to_string_lossy());
                        });
                    }
                });
        });
    }
}
