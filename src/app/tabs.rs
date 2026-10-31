//! Focus documents own their drafts and save jobs; only the visible image owns
//! decoded pixels/textures. egui keeps cursor and scroll state keyed by path.
use super::{
    Disco,
    editor::{Action, Editor},
    state::View,
};
use crate::{
    file_preview::Content,
    filesystem::Properties,
    index::Category,
    settings::{PreviewMode, color},
};
use eframe::egui::{self, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub(super) struct Tab {
    pub editor: Editor,
    preview: Option<Content>,
}
#[derive(Default)]
pub(super) struct Tabs {
    pub order: Vec<PathBuf>,
    pub next_untitled: u64,
    pub inactive: HashMap<PathBuf, Tab>,
    /// At most one replaceable tab, shared by mouse and keyboard file browsing.
    pub preview: Option<PathBuf>,
    pub preview_request: Option<PathBuf>,
    reveal: bool,
}
impl Tabs {
    pub fn has_unsaved(&self) -> bool {
        self.inactive
            .values()
            .any(|t| t.editor.dirty() || t.editor.saving.is_some())
    }
}
impl Disco {
    pub(in crate::app) fn editor_category(&self) -> Category {
        if self.preferences.layout.preview_mode == PreviewMode::Focus {
            self.editor.origin.unwrap_or(self.browser.category)
        } else {
            self.browser.category
        }
    }
    pub(in crate::app) fn editor_read_only(&self) -> bool {
        self.collections
            .config
            .get(self.editor_category())
            .map(|c| c.read_only)
            .unwrap_or(self.preferences.layout.all_files_read_only)
    }
    pub(in crate::app) fn keep_preview_tab(&mut self, path: &Path) -> bool {
        if self.tabs.preview.as_deref() != Some(path) {
            return false;
        }
        self.tabs.preview = None;
        self.message = crate::tr!("Вкладку закріплено", "Tab kept open").into();
        true
    }
    pub(in crate::app) fn open_focus_document(
        &mut self,
        path: PathBuf,
        ctx: &egui::Context,
    ) -> bool {
        if self.paths.standalone.is_some() && path.is_absolute() {
            self.paths.standalone = Some(path.clone());
        }
        let temporary = self.tabs.preview_request.as_ref() == Some(&path);
        if temporary {
            self.tabs.preview_request = None;
        }
        let focus_on_load = self.editor.focus_on_load;
        if !self.tabs.order.contains(&path) {
            let mut slot = None;
            if temporary && let Some(old) = self.tabs.preview.clone() {
                let protected = if self.editor.path.as_ref() == Some(&old) {
                    self.editor.dirty() || self.editor.saving.is_some()
                } else {
                    self.tabs
                        .inactive
                        .get(&old)
                        .is_some_and(|tab| tab.editor.dirty() || tab.editor.saving.is_some())
                };
                // Drafts/save jobs become retained documents; only a clean
                // preview may be discarded as the file selection moves.
                self.tabs.preview = None;
                if !protected {
                    slot = self.tabs.order.iter().position(|p| p == &old);
                    self.tabs.order.retain(|p| p != &old);
                    self.tabs.inactive.remove(&old);
                    if self.editor.path.as_ref() == Some(&old) {
                        self.editor = Editor::default();
                    }
                    ctx.data_mut(|data| {
                        data.remove::<egui::text_edit::TextEditState>(egui::Id::unique((
                            "file-editor",
                            Some(old),
                        )))
                    });
                }
            }
            if self.tabs.order.len() >= 64 {
                self.message = crate::tr!(
                    "Відкрито 64 вкладки. Закрий одну перед відкриттям іншого файла.",
                    "64 tabs are open. Close one before opening another file."
                )
                .into();
                self.browser.properties = self
                    .editor
                    .path
                    .clone()
                    .map(|p| Properties::load(p, 0, None, self.demo));
                return false;
            }
            if let Some(slot) = slot {
                self.tabs
                    .order
                    .insert(slot.min(self.tabs.order.len()), path.clone());
            } else {
                self.tabs.order.push(path.clone());
            }
            if temporary {
                self.tabs.preview = Some(path.clone());
            }
        }
        if self.editor.path.as_ref() == Some(&path) {
            return true;
        }
        self.editor.focus_on_load = false;
        // Preview owns the active worker; an inactive tab must not retain its
        // in-flight flag after that worker is superseded by another path.
        self.editor.refreshing = false;
        if let Some(old_path) = self.editor.path.clone() {
            if !self.tabs.order.contains(&old_path) {
                self.tabs.order.push(old_path.clone());
            }
            let preview = if matches!(self.file_preview.content, Some(Content::Text { .. })) {
                self.file_preview.content.take()
            } else {
                None
            };
            self.tabs.inactive.insert(
                old_path,
                Tab {
                    editor: std::mem::take(&mut self.editor),
                    preview,
                },
            );
        }
        let restored = self.tabs.inactive.remove(&path);
        let was_restored = restored.is_some();
        let cached = if let Some(tab) = restored {
            self.editor = tab.editor;
            tab.preview
        } else {
            self.editor = Editor::default();
            None
        };
        self.editor.path = Some(path.clone());
        self.editor.origin.get_or_insert(self.browser.category);
        self.editor.revision = self.scan.report_revision;
        self.editor.keep_cursor = was_restored;
        self.editor.focus_on_load = focus_on_load;
        self.file_preview
            .select(Some(path.clone()), self.scan.report_revision);
        if let Some(content) = cached {
            self.file_preview
                .restore_text(path, self.scan.report_revision, content);
        }
        if was_restored {
            self.reload_changed_document(ctx);
        }
        if focus_on_load && self.editor.document.is_some() {
            ctx.memory_mut(|m| {
                m.request_focus(egui::Id::unique(("file-editor", &self.editor.path)))
            });
            self.editor.focus_on_load = false;
        }
        self.tabs.reveal = true;
        true
    }
    pub(in crate::app) fn activate_tab(&mut self, path: &Path, ctx: &egui::Context) {
        let origin = self
            .tabs
            .inactive
            .get(path)
            .and_then(|t| t.editor.origin)
            .or(self.editor.origin)
            .unwrap_or(Category::All);
        self.browser.category = if self.collections.config.get(origin).is_some() {
            origin
        } else {
            Category::All
        };
        self.browser.view = View::Folders;
        self.browser.table_root = None;
        self.browser.filter.clear();
        self.browser.properties = if path.is_absolute() {
            Some(Properties::load(path.to_path_buf(), 0, None, self.demo))
        } else {
            None
        };
        if self.open_focus_document(path.to_path_buf(), ctx) {
            self.reveal_browser_selection();
            if self.editor.document.is_some() {
                ctx.memory_mut(|m| {
                    m.request_focus(egui::Id::unique(("file-editor", &self.editor.path)))
                });
            } else {
                self.editor.focus_on_load = true;
            }
        }
        ctx.request_repaint();
    }
    pub(in crate::app) fn request_close_tab(&mut self, path: &Path, ctx: &egui::Context) {
        if self.editor.path.as_deref() != Some(path)
            && self
                .tabs
                .inactive
                .get(path)
                .is_some_and(|tab| tab.editor.dirty() || tab.editor.saving.is_some())
        {
            self.activate_tab(path, ctx);
        }
        if self.editor.path.as_deref() == Some(path) {
            self.editor_action(Action::CloseTab(path.to_path_buf()), ctx);
        } else {
            self.close_tab_now(path, ctx);
        }
    }
    pub(in crate::app) fn close_tab_now(&mut self, path: &Path, ctx: &egui::Context) {
        let Some(index) = self.tabs.order.iter().position(|p| p == path) else {
            return;
        };
        self.tabs.order.remove(index);
        self.tabs.inactive.remove(path);
        if self.tabs.preview.as_deref() == Some(path) {
            self.tabs.preview = None;
        }
        if self.tabs.preview_request.as_deref() == Some(path) {
            self.tabs.preview_request = None;
        }
        // Discard clears editor.path before applying its pending close action.
        if self.editor.path.as_deref() == Some(path) || self.editor.path.is_none() {
            self.editor = Editor::default();
            self.file_preview.select(None, self.scan.report_revision);
            self.browser.properties = None;
            if let Some(next) = self
                .tabs
                .order
                .get(index.min(self.tabs.order.len().saturating_sub(1)))
                .cloned()
            {
                self.activate_tab(&next, ctx);
            }
        }
        ctx.request_repaint();
    }
    pub(in crate::app) fn poll_tabs(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        for tab in self.tabs.inactive.values_mut() {
            let editor = &mut tab.editor;
            editor.poll_save(ctx);
            if editor.saving.is_some() {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
                continue;
            }
            let category = editor.origin.unwrap_or(Category::All);
            let collection = self.collections.config.get(category);
            let read_only = collection
                .map(|c| c.read_only)
                .unwrap_or(self.preferences.layout.all_files_read_only);
            let auto = collection
                .map(|c| c.auto_save)
                .unwrap_or(self.preferences.layout.all_files_auto_save);
            if editor.untitled.is_none()
                && !read_only
                && auto
                && editor.dirty()
                && editor.error.is_empty()
                && !self.demo
                && !self.preferences.open
            {
                let due = *editor.auto_save_at.get_or_insert(now + 1.);
                if now >= due {
                    editor.save_in_background(ctx);
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_secs_f64(due - now));
                }
            }
        }
    }
    pub(in crate::app) fn document_tabs_ui(&mut self, ui: &mut egui::Ui) {
        let t = self.preferences.theme.clone();
        let mut activate = None;
        let mut close = None;
        if self.tabs.order.is_empty() {
            ui.allocate_space(Vec2::new(ui.available_width(), 34.));
            return;
        }
        let tab_strip = egui::ScrollArea::horizontal()
            .id_salt("document-tabs")
            .auto_shrink([false, true])
            .max_height(36.)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.;
                    for path in &self.tabs.order {
                        let active = self.editor.path.as_ref() == Some(path);
                        let editor = if active {
                            Some(&self.editor)
                        } else {
                            self.tabs.inactive.get(path).map(|tab| &tab.editor)
                        };
                        let dirty = editor.is_some_and(Editor::dirty);
                        let error = editor.is_some_and(|e| !e.error.is_empty());
                        let name = editor
                            .and_then(|e| e.untitled.as_ref())
                            .map(|u| u.label())
                            .unwrap_or_else(|| crate::format::label(path));
                        let width = (name.chars().count() as f32 * 7.5 + 58.).clamp(115., 230.);
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(width, 33.), Sense::hover());
                        ui.painter().rect_filled(
                            rect,
                            t.radius,
                            color(if active { &t.raised } else { &t.panel }),
                        );
                        if active {
                            ui.painter().hline(
                                rect.x_range(),
                                rect.bottom(),
                                Stroke::new(2., color(&t.accent)),
                            );
                        }
                        let close_rect = Rect::from_center_size(
                            Pos2::new(rect.right() - 15., rect.center().y),
                            Vec2::splat(25.),
                        );
                        let hit = Rect::from_min_max(
                            rect.min,
                            Pos2::new(close_rect.left() - 2., rect.bottom()),
                        );
                        let response = ui.interact(
                            hit,
                            egui::Id::unique(("document-tab", path)),
                            Sense::click(),
                        );
                        let label = if error {
                            format!("! {name}")
                        } else if dirty {
                            format!("• {name}")
                        } else {
                            name
                        };
                        let temporary = self.tabs.preview.as_ref() == Some(path);
                        let mut text = egui::text::LayoutJob::simple(
                            label,
                            FontId::proportional(13.),
                            color(&t.text),
                            f32::INFINITY,
                        );
                        for section in &mut text.sections {
                            section.format.italics = temporary;
                        }
                        let galley = ui.fonts_mut(|fonts| fonts.layout_job(text));
                        ui.painter().with_clip_rect(hit.shrink(6.)).galley(
                            Pos2::new(rect.left() + 9., rect.center().y - galley.size().y / 2.),
                            galley,
                            color(&t.text),
                        );
                        if response.has_focus() {
                            self.keyboard.tab = Some(path.clone());
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
                            ui.painter().rect_stroke(
                                hit.shrink(1.),
                                t.radius,
                                Stroke::new(1., color(&t.accent)),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if self.keyboard.reveal_tab && self.keyboard.tab.as_ref() == Some(path) {
                            response.scroll_to_me(Some(egui::Align::Center));
                            response.request_focus();
                            if ui.clip_rect().contains(rect.center()) {
                                self.keyboard.reveal_tab = false;
                            }
                        }
                        if response.clicked() {
                            activate = Some(path.clone());
                        }
                        if response.clicked_by(egui::PointerButton::Middle) {
                            close = Some(path.clone());
                        }
                        if active && self.tabs.reveal {
                            response.scroll_to_me(Some(egui::Align::Center));
                        }
                        response.on_hover_text(format!(
                            "{}{}{}",
                            editor
                                .and_then(|e| e.untitled.as_ref())
                                .map(|u| crate::tr!(
                                    "Не збережено · {directory}",
                                    "Not saved · {directory}",
                                    directory = u.directory.display()
                                ))
                                .unwrap_or_else(|| path.display().to_string()),
                            if temporary {
                                crate::tr!(
                                    "\nТимчасовий перегляд · Enter або → — закріпити",
                                    "\nTemporary preview · Enter or Right arrow to keep open"
                                )
                            } else {
                                ""
                            },
                            editor
                                .filter(|e| !e.error.is_empty())
                                .map(|e| format!("\n{}", e.error))
                                .unwrap_or_default()
                        ));
                        let button = ui
                            .interact(
                                close_rect,
                                egui::Id::unique(("close-document-tab", path)),
                                Sense::click(),
                            )
                            .on_hover_text(crate::tr!("Закрити вкладку", "Close tab"));
                        self.art
                            .paint(ui, 21, close_rect.shrink(7.), color(&t.muted));
                        if button.clicked() {
                            close = Some(path.clone());
                        }
                    }
                });
            });
        ui.interact(
            tab_strip.inner_rect,
            egui::Id::unique("document-tabs-bounds"),
            Sense::hover(),
        );
        self.tabs.reveal = false;
        if let Some(path) = activate {
            self.activate_tab(&path, ui.ctx());
        }
        if let Some(path) = close {
            self.request_close_tab(&path, ui.ctx());
        }
    }
}
