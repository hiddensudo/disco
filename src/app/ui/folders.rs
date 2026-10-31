use crate::app::Disco;
use crate::app::state::{FolderRows, RowsKey, TreeRow};
use crate::app::ui::widgets::mix;
use crate::filesystem::Properties;
use crate::format::{display_size, group_digits, label};
use crate::index::Category;
use crate::motion;
use crate::settings::{Layout, PreviewMode, color};
use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use std::sync::Arc;

impl Disco {
    fn search_folder_row(
        &mut self,
        ui: &mut egui::Ui,
        file: &Arc<crate::directory_files::Entry>,
        depth: usize,
        height: f32,
        name_width: f32,
    ) {
        let t = self.preferences.theme.clone();
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(rect, t.radius, color(&t.raised));
        }
        let indent = Layout::tree_indent(depth, name_width);
        let icon_size = 18. * self.preferences.layout.tree_icon_scale();
        self.art.paint(
            ui,
            0,
            Rect::from_center_size(
                rect.left_center() + Vec2::new(28. + indent, 0.),
                Vec2::splat(icon_size),
            ),
            color(&t.text),
        );
        super::widgets::row_text(
            ui,
            Rect::from_min_max(
                Pos2::new(rect.left() + 42. + indent, rect.top()),
                Pos2::new(rect.left() + name_width - 6., rect.bottom()),
            ),
            label(&file.path),
            FontId::proportional((14. * self.preferences.layout.tree_text_scale()).max(10.5)),
            color(&t.text),
        );
        if self.paths.directory.is_some() {
            ui.painter().text(
                Pos2::new(rect.right() - 8., rect.center().y),
                Align2::RIGHT_CENTER,
                "—",
                FontId::proportional(11.),
                color(&t.muted),
            );
        }
        if response.clicked() {
            self.editor_action(
                crate::app::editor::Action::Navigate(file.path.clone()),
                ui.ctx(),
            );
        }
        response.on_hover_text(file.path.display().to_string());
    }
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
            search: self.search.revision,
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
        if let Some(path) = &self.paths.standalone {
            return vec![TreeRow::File(
                Arc::new(crate::directory_files::Entry {
                    path: path.clone(),
                    bytes: self.browser.properties.as_ref().map_or(0, |p| p.bytes),
                }),
                0,
            )];
        }
        if let Some(directory) = &self.paths.directory {
            let Some(listing) = self.files.get(directory) else {
                return vec![];
            };
            let mut rows: Vec<_> = listing
                .directories
                .iter()
                .cloned()
                .map(|f| TreeRow::PathFolder(f, 0))
                .chain(listing.files.iter().cloned().map(|f| TreeRow::File(f, 0)))
                .filter(|row| {
                    let path = match row {
                        TreeRow::File(f, _) | TreeRow::PathFolder(f, _) => &f.path,
                        _ => unreachable!(),
                    };
                    label(path)
                        .to_lowercase()
                        .contains(&self.browser.filter.to_lowercase())
                })
                .collect();
            rows.sort_by(|a, b| {
                let (ad, af) = match a {
                    TreeRow::File(f, _) => (false, f),
                    TreeRow::PathFolder(f, _) => (true, f),
                    _ => unreachable!(),
                };
                let (bd, bf) = match b {
                    TreeRow::File(f, _) => (false, f),
                    TreeRow::PathFolder(f, _) => (true, f),
                    _ => unreachable!(),
                };
                bd.cmp(&ad).then_with(|| {
                    let cmp = if self.browser.sort_size {
                        af.bytes.cmp(&bf.bytes)
                    } else {
                        af.path.file_name().cmp(&bf.path.file_name())
                    };
                    if self.browser.descending {
                        cmp.reverse()
                    } else {
                        cmp
                    }
                })
            });
            return rows;
        }
        let Some(r) = &self.scan.report else {
            return vec![];
        };
        if !self.browser.filter.is_empty() {
            if let Some(results) = self.search.for_query(&self.browser.filter) {
                let mut hits: Vec<_> = results.hits.iter().collect();
                hits.sort_by(|a, b| {
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
                return hits
                    .into_iter()
                    .map(|hit| {
                        if hit.directory {
                            if let Some(id) = hit
                                .folder_id
                                .filter(|id| r.folders.get(*id).is_some_and(|f| f.path == hit.path))
                            {
                                return TreeRow::Folder(id, 0);
                            }
                            TreeRow::PathFolder(
                                Arc::new(crate::directory_files::Entry {
                                    path: hit.path.clone(),
                                    bytes: hit.bytes,
                                }),
                                0,
                            )
                        } else {
                            TreeRow::File(
                                Arc::new(crate::directory_files::Entry {
                                    path: hit.path.clone(),
                                    bytes: hit.bytes,
                                }),
                                0,
                            )
                        }
                    })
                    .collect();
            }
            return vec![];
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
        let focus = self.preferences.layout.preview_mode == PreviewMode::Focus;
        let count_width =
            if self.preferences.layout.show_count && (!focus || ui.available_width() >= 360.) {
                if focus { 56. } else { 80. }
            } else {
                0.
            };
        let size_width = if focus {
            66.
        } else if ui.available_width() < 500. {
            112.
        } else {
            145.
        };
        let width = ui.available_width();
        let name_width = (width - size_width - count_width).max(80.);
        let (h, _) = ui.allocate_exact_size(
            Vec2::new(width, if focus { 30. } else { 35. }),
            Sense::hover(),
        );
        ui.painter()
            .rect_filled(h, t.radius, mix(color(&t.panel), color(&t.raised), 0.45));
        ui.painter().rect_stroke(
            h,
            t.radius,
            Stroke::new(1., color(&t.border)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            h.left_center() + Vec2::new(10., 0.),
            Align2::LEFT_CENTER,
            crate::tr!("Назва", "Name"),
            FontId::proportional(if focus { 11.5 } else { 14. }),
            color(&t.text),
        );
        sort_arrow(
            ui,
            Pos2::new(h.left() + name_width - 22., h.center().y),
            self.browser.descending,
            !self.browser.sort_size,
            &t,
        );
        let size_left = h.left() + name_width;
        ui.painter()
            .vline(size_left, h.y_range(), Stroke::new(1., color(&t.border)));
        ui.painter().text(
            Pos2::new(
                h.right() - count_width - if focus { 18. } else { 48. },
                h.center().y,
            ),
            Align2::RIGHT_CENTER,
            crate::tr!("Розмір", "Size"),
            FontId::proportional(if focus { 11.5 } else { 14. }),
            color(&t.text),
        );
        sort_arrow(
            ui,
            Pos2::new(
                h.right() - count_width - if focus { 8. } else { 26. },
                h.center().y,
            ),
            self.browser.descending,
            self.browser.sort_size,
            &t,
        );
        if ui
            .interact(
                Rect::from_min_max(h.min, Pos2::new(size_left, h.bottom())),
                egui::Id::unique("sort-name"),
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
                egui::Id::unique("sort-size"),
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
        if count_width > 0. {
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
        let rows = self.visible_rows();
        if self.paths.standalone.is_none()
            && self.paths.directory.is_none()
            && (self.scan.report.is_none() || rows.is_empty() && self.browser.filter.is_empty())
        {
            self.browser_empty_state(ui, outer);
            return;
        }
        if rows.is_empty()
            && let Some(path) = &self.paths.directory
        {
            let text = match self.files.get(path) {
                None => crate::tr!("Читаємо папку…", "Reading directory…").to_string(),
                Some(listing) if listing.error.is_some() => listing.error.clone().unwrap(),
                Some(_) => crate::tr!("Немає файлів для показу", "No files to show").to_string(),
            };
            if self.files.get(path).is_none() {
                let spinner =
                    Rect::from_center_size(outer.center() - Vec2::new(0., 24.), Vec2::splat(14.));
                ui.put(
                    spinner,
                    egui::Spinner::new().size(14.).color(color(&t.muted)),
                );
            }
            ui.painter().text(
                outer.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(13.),
                color(&t.muted),
            );
        }
        let height = self.preferences.layout.tree_row_height();

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
                TreeRow::File(f, _) | TreeRow::PathFolder(f, _) => f.path == path,
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
            } else if path
                .parent()
                .is_some_and(|parent| self.files.get(parent).is_none())
            {
                self.browser.scroll_to = Some(path);
            }
        }
        let scroll = area.show_rows(ui, height, rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.;
            for n in range {
                let (id, depth) = match &rows[n] {
                    TreeRow::Folder(id, depth) => (*id, *depth),
                    TreeRow::PathFolder(file, depth) => {
                        self.search_folder_row(ui, file, *depth, height, name_width);
                        continue;
                    }
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
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
                let response = ui.interact(
                    rect,
                    egui::Id::unique(("folder-row", &f.path)),
                    Sense::click(),
                );
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
                let mut x = rect.left() + 8. + Layout::tree_indent(depth, name_width);
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
                    x += 21.;
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
                        if self.browser.filter.is_empty() {
                            self.toggle_folder(id);
                        } else {
                            self.enter_folder(id);
                        }
                    }
                }
                x += 20.;
                let icon_size =
                    (if focus { 18. } else { 23. }) * self.preferences.layout.tree_icon_scale();
                self.art.paint(
                    ui,
                    0,
                    Rect::from_center_size(
                        Pos2::new(x + icon_size * 0.5, rect.center().y),
                        Vec2::splat(icon_size),
                    ),
                    color(&t.text),
                );
                x += icon_size + 6.;
                super::widgets::row_text(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(x, rect.top()),
                        Pos2::new(rect.left() + name_width - 6., rect.bottom()),
                    ),
                    label(&f.path),
                    FontId::proportional(
                        (15. * self.preferences.layout.tree_text_scale()).max(10.5),
                    ),
                    color(&t.text),
                );
                ui.painter()
                    .with_clip_rect(Rect::from_min_max(
                        Pos2::new(rect.left() + name_width, rect.top()),
                        rect.max,
                    ))
                    .text(
                        Pos2::new(
                            rect.right() - count_width - if focus { 8. } else { 12. },
                            rect.center().y,
                        ),
                        Align2::RIGHT_CENTER,
                        display_size(f.bytes),
                        FontId::proportional(if focus {
                            (14. * self.preferences.layout.tree_text_scale()).clamp(10., 13.)
                        } else {
                            15. * self.preferences.layout.tree_text_scale()
                        }),
                        color(&t.text),
                    );
                if count_width > 0. {
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
                    ui.ctx().request_repaint();
                    if !self.browser.filter.is_empty() {
                        // A search result is an entry point into a directory,
                        // not a transient selection in the global tree.
                        self.enter_folder(id);
                    } else {
                        self.select_folder(id);
                        if self.preferences.layout.single_click_expand
                            && !response.double_clicked()
                            && self.folder_can_expand(id)
                        {
                            self.toggle_folder(id);
                        }
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

fn sort_arrow(ui: &egui::Ui, c: Pos2, descending: bool, active: bool, t: &crate::settings::Theme) {
    let direction = if descending { 1. } else { -1. };
    ui.painter().add(egui::Shape::line(
        vec![
            c + Vec2::new(-4., -2. * direction),
            c + Vec2::new(0., 2. * direction),
            c + Vec2::new(4., -2. * direction),
        ],
        Stroke::new(1.5, color(if active { &t.accent } else { &t.muted })),
    ));
}
