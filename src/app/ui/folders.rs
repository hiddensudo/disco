use crate::app::Disco;
use crate::app::state::{FolderRows, RowsKey, TreeRow};
use crate::app::ui::widgets::{chevron, mix};
use crate::filesystem::Properties;
use crate::format::{display_size, group_digits, label, shorten};
use crate::index::Category;
use crate::motion;
use crate::settings::color;
use eframe::egui::{self, Align2, Color32 as C, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use std::sync::Arc;

impl Disco {
    pub(in crate::app) fn select_folder(&mut self, id: usize) {
        self.browser.focus = id;
        if let Some(r) = &self.scan.report {
            let f = &r.folders[id];
            self.browser.properties = Some(Properties::load(
                f.path.clone(),
                f.bytes,
                r.complete.then_some(f.files),
                self.demo,
            ));
        }
    }
    pub(in crate::app) fn expand_folder(&mut self, id: usize) {
        if self.browser.expanded.insert(id) {
            self.browser.tree_revision = self.browser.tree_revision.wrapping_add(1);
        }
    }
    pub(in crate::app) fn toggle_folder(&mut self, id: usize) {
        if !self.browser.expanded.remove(&id) {
            self.browser.expanded.insert(id);
        }
        self.browser.tree_revision = self.browser.tree_revision.wrapping_add(1);
    }
    pub(in crate::app) fn visible_rows(&mut self) -> FolderRows {
        let key = RowsKey {
            location: self.nav_state(),
            filter: self.browser.filter.clone(),
            sort_size: self.browser.sort_size,
            descending: self.browser.descending,
            report: self.scan.report_revision,
            tree: self.browser.tree_revision,
            files: self.files.revision,
        };
        if let Some((old, rows)) = &self.browser.rows_cache {
            if *old == key {
                return rows.clone();
            }
        }
        let rows = Arc::new(self.build_visible_rows());
        self.browser.rows_cache = Some((key, rows.clone()));
        rows
    }
    pub(in crate::app) fn build_visible_rows(&self) -> Vec<TreeRow> {
        let Some(r) = &self.scan.report else {
            return vec![];
        };
        if !self.browser.filter.is_empty() {
            let q = self.browser.filter.to_lowercase();
            let mut ids: Vec<_> = r
                .folders
                .iter()
                .enumerate()
                .filter(|(id, f)| {
                    self.scan.index.matches_category(*id, self.browser.category)
                        && f.path.to_string_lossy().to_lowercase().contains(&q)
                })
                .map(|(i, _)| i)
                .collect();
            self.sort_ids(&mut ids);
            let mut rows: Vec<_> = ids.into_iter().map(|i| TreeRow::Folder(i, 0)).collect();
            for (id, folder) in r.folders.iter().enumerate() {
                if !self.scan.index.matches_category(id, self.browser.category) {
                    continue;
                }
                if let Some(listing) = self.files.get(&folder.path) {
                    for file in &listing.files {
                        if file.path.to_string_lossy().to_lowercase().contains(&q) {
                            rows.push(TreeRow::File(file.clone(), 0));
                        }
                    }
                }
            }
            return rows;
        }
        let roots = if let Some(id) = self.browser.table_root {
            vec![id]
        } else {
            self.scan.index.roots(self.browser.category).to_vec()
        };
        let mut roots = roots;
        self.sort_ids(&mut roots);
        let mut stack: Vec<_> = roots
            .into_iter()
            .rev()
            .map(|i| TreeRow::Folder(i, 0))
            .collect();
        let mut rows = vec![];
        while let Some(row) = stack.pop() {
            let TreeRow::Folder(i, d) = row else {
                rows.push(row);
                continue;
            };
            rows.push(TreeRow::Folder(i, d));
            if self.browser.expanded.contains(&i) {
                if let Some(listing) = self.files.get(&r.folders[i].path) {
                    if listing.limited || listing.error.is_some() {
                        stack.push(TreeRow::Notice(i, d + 1));
                    }
                    let mut files = listing.files.clone();
                    files.sort_by(|a, b| {
                        let cmp = if self.browser.sort_size {
                            a.bytes.cmp(&b.bytes)
                        } else {
                            a.path.file_name().cmp(&b.path.file_name())
                        };
                        if self.browser.descending {
                            cmp.reverse()
                        } else {
                            cmp
                        }
                    });
                    stack.extend(files.into_iter().rev().map(|f| TreeRow::File(f, d + 1)));
                } else if !self.demo {
                    stack.push(TreeRow::Pending(i, d + 1));
                }
                let mut children = self.scan.index.children(i).to_vec();
                if self.browser.category != Category::All {
                    children
                        .retain(|j| self.scan.index.matches_category(*j, self.browser.category));
                }
                self.sort_ids(&mut children);
                stack.extend(
                    children
                        .into_iter()
                        .rev()
                        .map(|j| TreeRow::Folder(j, d + 1)),
                );
            }
        }
        rows
    }
    pub(in crate::app) fn sort_ids(&self, ids: &mut [usize]) {
        if let Some(r) = &self.scan.report {
            ids.sort_by(|a, b| {
                let o = if self.browser.sort_size {
                    r.folders[*a].bytes.cmp(&r.folders[*b].bytes)
                } else {
                    r.folders[*a]
                        .path
                        .file_name()
                        .cmp(&r.folders[*b].path.file_name())
                };
                if self.browser.descending {
                    o.reverse()
                } else {
                    o
                }
            });
        }
    }
    pub(in crate::app) fn folder_table(&mut self, ui: &mut egui::Ui) {
        let t = self.preferences.theme.clone();
        let count_width = if self.preferences.layout.show_count {
            80.
        } else {
            0.
        };
        let size_width = if ui.available_width() < 500. {
            95.
        } else {
            145.
        };
        let width = ui.available_width();
        let name_width = (width - size_width - count_width).max(80.);
        let (h, _) = ui.allocate_exact_size(Vec2::new(width, 35.), Sense::hover());
        ui.painter()
            .rect_filled(h, t.radius, mix(color(&t.panel), color(&t.raised), 0.45));
        ui.painter().rect_stroke(
            h,
            t.radius,
            Stroke::new(1., color(&t.border)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            h.min + Vec2::new(15., 17.),
            Align2::LEFT_CENTER,
            crate::tr!("Назва", "Name"),
            FontId::proportional(14.),
            color(&t.text),
        );
        let size_left = h.left() + name_width;
        ui.painter()
            .vline(size_left, h.y_range(), Stroke::new(1., color(&t.border)));
        ui.painter().text(
            Pos2::new(h.right() - count_width - 48., h.center().y),
            Align2::RIGHT_CENTER,
            crate::tr!("Розмір", "Size"),
            FontId::proportional(14.),
            color(&t.text),
        );
        chevron(
            ui,
            Pos2::new(h.right() - count_width - 26., h.center().y),
            self.browser.descending,
            if self.browser.sort_size {
                color(&t.accent)
            } else {
                color(&t.muted)
            },
        );
        if ui
            .interact(
                Rect::from_min_max(h.min, Pos2::new(size_left, h.bottom())),
                ui.scope_id().with("sort_name"),
                Sense::click(),
            )
            .clicked()
        {
            self.browser.descending = if self.browser.sort_size {
                false
            } else {
                !self.browser.descending
            };
            self.browser.sort_size = false;
        }
        if ui
            .interact(
                Rect::from_min_max(Pos2::new(size_left, h.top()), h.max),
                ui.scope_id().with("sort_size"),
                Sense::click(),
            )
            .clicked()
        {
            self.browser.descending = if self.browser.sort_size {
                !self.browser.descending
            } else {
                true
            };
            self.browser.sort_size = true;
        }
        if self.preferences.layout.show_count {
            ui.painter().text(
                Pos2::new(h.right() - 9., h.center().y),
                Align2::RIGHT_CENTER,
                crate::tr!("Файлів", "Files"),
                FontId::proportional(12.),
                color(&t.muted),
            );
        }
        ui.spacing_mut().item_spacing.y = 0.;
        let outer = Rect::from_min_size(
            ui.cursor().min,
            Vec2::new(width, ui.available_height().max(100.)),
        );
        ui.painter().rect_stroke(
            outer,
            t.radius,
            Stroke::new(1., color(&t.border)),
            egui::StrokeKind::Inside,
        );
        if self.scan.report.is_none() {
            let center = outer.center();
            self.art.paint(
                ui,
                0,
                Rect::from_center_size(center - Vec2::new(0., 45.), Vec2::splat(42.)),
                C::from_white_alpha(90),
            );
            ui.painter().text(
                center + Vec2::new(0., 0.),
                Align2::CENTER_CENTER,
                if self.scan.restore_requested_root.is_some() {
                    crate::tr!(
                        "Відновлюємо останній знімок…",
                        "Restoring the last snapshot…"
                    )
                } else if self.scan.busy {
                    crate::tr!("Сканування папок…", "Scanning folders…")
                } else {
                    crate::tr!(
                        "Обери папку та почни сканування",
                        "Choose a folder and start scanning"
                    )
                },
                FontId::proportional(17.),
                color(&t.muted),
            );
            ui.painter().text(
                center + Vec2::new(0., 28.),
                Align2::CENTER_CENTER,
                if self.scan.restore_requested_root.is_some() {
                    crate::tr!(
                        "Читаємо збережене дерево у фоні, без нового сканування.",
                        "Loading the saved tree in the background, without a new scan."
                    )
                } else if self.scan.busy {
                    crate::tr!(
                        "Збираємо розміри. Прогрес і керування — вгорі.",
                        "Collecting sizes. Progress and controls are above."
                    )
                } else {
                    crate::tr!(
                        "Назва диска зліва зверху відкриває вибір папки",
                        "Click the drive name at the top left to choose a folder"
                    )
                },
                FontId::proportional(12.),
                color(&t.muted),
            );
            return;
        }
        let rows = self.visible_rows();
        let height = ((if self.preferences.layout.compact {
            28.
        } else {
            34.
        }) * self.preferences.layout.text_scale)
            .max(23. * self.preferences.layout.icon_scale + 6.);

        let mut area = egui::ScrollArea::vertical()
            .id_salt("folder-table")
            .auto_shrink([false, false]);
        if let Some(path) = self.browser.scroll_to.take() {
            if let Some(n) = rows.iter().position(|row| match row {
                TreeRow::Folder(id, _) => self
                    .scan
                    .report
                    .as_ref()
                    .is_some_and(|r| r.folders[*id].path == path),
                TreeRow::File(f, _) => f.path == path,
                _ => false,
            }) {
                let y = n as f32 * height;
                let current = self.browser.scroll_offset;
                let view = self.browser.scroll_height;
                let offset = if y < current {
                    y
                } else if y + height > current + view {
                    y + height - view
                } else {
                    current
                };
                area = area.vertical_scroll_offset(offset.max(0.));
            }
        }
        let scroll = area.show_rows(ui, height, rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.;
            for n in range {
                let (id, depth) = match &rows[n] {
                    TreeRow::Folder(id, depth) => (*id, *depth),
                    TreeRow::File(file, depth) => {
                        self.file_row(
                            ui,
                            file.clone(),
                            *depth,
                            height,
                            name_width,
                            count_width,
                            &t,
                        );
                        continue;
                    }
                    TreeRow::Pending(id, depth) | TreeRow::Notice(id, depth) => {
                        self.file_listing_row(
                            ui,
                            *id,
                            *depth,
                            height,
                            matches!(&rows[n], TreeRow::Pending(..)),
                            &t,
                        );
                        continue;
                    }
                };
                let f = self.scan.report.as_ref().unwrap().folders[id].clone();
                self.files.touch(&f.path);
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
                if self.preferences.layout.striped_rows && n % 2 == 1 {
                    ui.painter().rect_filled(
                        rect,
                        0,
                        mix(color(&t.background), color(&t.raised), 0.3),
                    );
                }
                let selected = motion::value(
                    ui,
                    ("row-selected", self.scan.report_revision, id),
                    if self.browser.focus == id
                        && self.browser.properties.as_ref().is_none_or(|p| p.is_dir)
                    {
                        1.
                    } else {
                        0.
                    },
                    1.,
                );
                let hover = motion::hover(
                    ui,
                    ("row-hover", self.scan.report_revision, id),
                    response.hovered(),
                );
                if hover > 0. && selected < 1. {
                    ui.painter().rect_filled(
                        rect,
                        t.radius,
                        color(&t.raised).gamma_multiply(hover * (1. - selected)),
                    );
                }
                if selected > 0. {
                    ui.painter().rect_filled(
                        rect,
                        t.radius,
                        mix(color(&t.panel), color(&t.accent), 0.31).gamma_multiply(selected),
                    );
                    ui.painter().rect_stroke(
                        rect,
                        t.radius,
                        Stroke::new(0.6, color(&t.accent).gamma_multiply(0.45 * selected)),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().rect_filled(
                        Rect::from_min_size(rect.min, Vec2::new(4., rect.height())),
                        0,
                        color(&t.accent).gamma_multiply(selected),
                    );
                }
                let mut child_clicked = false;
                let can_select =
                    self.scan.index.cleanup_ids().contains(&id) && !self.scan.busy && !self.demo;
                let mut x = rect.left() + 11. + (depth as f32 * 19.).min(name_width * 0.35);
                if can_select {
                    let cbrect =
                        Rect::from_min_size(Pos2::new(x, rect.center().y - 9.), Vec2::splat(18.));
                    let mut checked = self.browser.selected.contains(&f.path);
                    let ir = ui.scope_builder(egui::UiBuilder::new().max_rect(cbrect), |ui| {
                        ui.checkbox(&mut checked, "")
                    });
                    if ir.inner.changed() {
                        child_clicked = true;
                        if checked {
                            self.browser.selected.insert(f.path.clone());
                        } else {
                            self.browser.selected.remove(&f.path);
                        }
                    }
                    x += 24.;
                }
                let arrow = Rect::from_center_size(
                    Pos2::new(x + 9., rect.center().y),
                    Vec2::new(19., height),
                );
                if self.folder_can_expand(id) {
                    let openness = motion::value(
                        ui,
                        ("folder-disclosure", self.scan.report_revision, id),
                        if self.browser.expanded.contains(&id) {
                            1.
                        } else {
                            0.
                        },
                        1.,
                    );
                    motion::disclosure(ui, arrow.center(), openness, color(&t.muted));
                    if ui
                        .interact(arrow, ui.scope_id().with(("expand", id)), Sense::click())
                        .clicked()
                    {
                        child_clicked = true;
                        ui.ctx().request_repaint();
                        self.toggle_folder(id);
                    }
                }
                x += 29.;
                self.art.paint(
                    ui,
                    0,
                    Rect::from_center_size(
                        Pos2::new(x + 11., rect.center().y),
                        Vec2::splat(23. * self.preferences.layout.icon_scale),
                    ),
                    C::WHITE,
                );
                x += 32. + (23. * (self.preferences.layout.icon_scale - 1.)).max(0.) * 0.5;
                let shown = shorten(
                    &label(&f.path),
                    ((name_width - (x - rect.left()) - 8.) / 7.2).max(5.) as usize,
                );
                ui.painter().text(
                    Pos2::new(x, rect.center().y),
                    Align2::LEFT_CENTER,
                    shown,
                    FontId::proportional(15. * self.preferences.layout.text_scale),
                    color(&t.text),
                );
                ui.painter().text(
                    Pos2::new(rect.right() - count_width - 12., rect.center().y),
                    Align2::RIGHT_CENTER,
                    display_size(f.bytes),
                    FontId::proportional(15. * self.preferences.layout.text_scale),
                    color(&t.text),
                );
                if self.preferences.layout.show_count {
                    ui.painter().text(
                        Pos2::new(rect.right() - 9., rect.center().y),
                        Align2::RIGHT_CENTER,
                        group_digits(f.files),
                        FontId::proportional(12.),
                        color(&t.muted),
                    );
                }
                if self.preferences.layout.row_separators {
                    ui.painter().hline(
                        rect.x_range(),
                        rect.bottom(),
                        Stroke::new(0.5, color(&t.border)),
                    );
                }
                if response.clicked() && !child_clicked {
                    self.select_folder(id);
                    if self.preferences.layout.single_click_expand
                        && !response.double_clicked()
                        && self.folder_can_expand(id)
                    {
                        ui.ctx().request_repaint();
                        self.toggle_folder(id);
                    }
                }
                if response.double_clicked()
                    && !child_clicked
                    && !self.preferences.layout.single_click_expand
                {
                    self.browser.table_root = Some(id);
                    self.expand_folder(id);
                    self.browser.filter.clear();
                }
                response.on_hover_text(f.path.to_string_lossy());
            }
        });
        self.browser.scroll_offset = scroll.state.clamped_offset().y;
        self.browser.scroll_height = scroll.inner_rect.height();
    }
}
