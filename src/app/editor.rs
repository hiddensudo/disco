//! Keep editor state separate from preview decoding and guard transitions that
//! would abandon a draft. All writes run off the UI thread.
use super::{Disco, state::View};
use crate::{
    filesystem::Properties,
    index::Category,
    navigation::History,
    settings::PreviewMode,
    text_editor::{Document, Stamp},
};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};
#[derive(Clone)]
pub(super) struct Bookmark {
    root: String,
    table: Option<usize>,
    focus: usize,
    category: Category,
    view: View,
    filter: String,
    properties: Option<Properties>,
    mode: PreviewMode,
    show_properties: bool,
    history: History<super::state::NavState>,
}
impl Bookmark {
    pub(super) fn capture(app: &Disco) -> Self {
        Self {
            root: app.browser.root.clone(),
            table: app.browser.table_root,
            focus: app.browser.focus,
            category: app.browser.category,
            view: app.browser.view,
            filter: app.browser.filter.clone(),
            properties: app.browser.properties.clone(),
            mode: app.preferences.layout.preview_mode,
            show_properties: app.preferences.layout.properties,
            history: app.browser.history.clone(),
        }
    }
    fn restore(self, app: &mut Disco) {
        app.browser.root = self.root;
        app.browser.table_root = self.table;
        app.browser.focus = self.focus;
        app.browser.category = self.category;
        app.browser.view = self.view;
        app.browser.filter = self.filter;
        app.browser.properties = self.properties;
        app.preferences.layout.preview_mode = self.mode;
        app.preferences.layout.properties = self.show_properties;
        app.browser.history = self.history;
        app.browser.nav_traversed = true;
        app.browser.scroll_to = app.browser.properties.as_ref().map(|p| p.path.clone());
    }
}
pub(super) enum Action {
    Transition(Bookmark),
    Navigate(PathBuf),
    NewFile(PathBuf),
    NewTextTab(PathBuf),
    SelectFile {
        path: PathBuf,
        bytes: u64,
        focus_editor: bool,
    },
    Reload,
    Scan {
        root: String,
        dup: bool,
        force: bool,
        automatic: bool,
    },
    Delete(Vec<(PathBuf, u64)>),
    CloseTab(PathBuf),
    Close,
}
#[derive(Default)]
pub(super) struct Editor {
    pub path: Option<PathBuf>,
    pub untitled: Option<super::untitled::Untitled>,
    pub origin: Option<Category>,
    pub document: Option<Document>,
    pub focused: bool,
    pub focus_on_load: bool,
    pub(super) keep_cursor: bool,
    preview_disk: Option<Stamp>,
    pub pending: Option<Action>,
    pub error: String,
    pub confirm_fresh: bool,
    pub saving: Option<Receiver<Result<(Stamp, Document), String>>>,
    pub(super) bookmark: Option<Bookmark>,
    pub(super) revision: u64,
    pub auto_save_at: Option<f64>,
    pub last_saved_at: Option<f64>,
}
impl Editor {
    pub fn dirty(&self) -> bool {
        self.document.as_ref().is_some_and(Document::dirty)
    }
}
impl Editor {
    pub(super) fn save_in_background(&mut self, ctx: &egui::Context) {
        if self.untitled.is_some() || self.saving.is_some() || !self.dirty() {
            return;
        }
        let (Some(path), Some(document)) = (self.path.clone(), self.document.clone()) else {
            return;
        };
        self.error.clear();
        self.auto_save_at = None;
        let (tx, rx) = mpsc::channel();
        self.saving = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::text_editor::save(&path, &document).map(|stamp| (stamp, document));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub(super) fn poll_save(&mut self, ctx: &egui::Context) -> Option<bool> {
        let result = self.saving.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(crate::tr!(
                "Не вдалося зберегти файл",
                "Could not save the file"
            )
            .into())),
        })?;
        self.saving = None;
        match result {
            Ok((stamp, saved)) => {
                if let Some(doc) = &mut self.document {
                    doc.accept_saved(stamp, &saved);
                }
                self.last_saved_at = Some(ctx.input(|i| i.time));
                Some(true)
            }
            Err(error) => {
                self.error = error;
                Some(false)
            }
        }
    }
}

impl Disco {
    pub(in crate::app) fn auto_save_enabled(&self) -> bool {
        self.editor.untitled.is_none()
            && !self.editor_read_only()
            && self
                .collections
                .config
                .get(self.editor_category())
                .map(|c| c.auto_save)
                .unwrap_or(self.preferences.layout.all_files_auto_save)
    }
    pub(in crate::app) fn note_editor_change(&mut self, ctx: &egui::Context) {
        self.editor.auto_save_at = Some(ctx.input(|i| i.time) + 1.);
        self.editor.last_saved_at = None;
        if let Some(path) = self.editor.path.clone() {
            self.keep_preview_tab(&path);
        }
    }
    pub(in crate::app) fn auto_save_tick(&mut self, ctx: &egui::Context) {
        if !self.auto_save_enabled() || !self.editor.dirty() {
            self.editor.auto_save_at = None;
            return;
        }
        if self.demo
            || self.preferences.open
            || self.editor.pending.is_some()
            || self.editor.saving.is_some()
            || !self.editor.error.is_empty()
        {
            return;
        }
        let now = ctx.input(|i| i.time);
        let due = *self.editor.auto_save_at.get_or_insert(now + 1.);
        if now >= due {
            self.save_editor(ctx);
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(due - now));
        }
    }
    pub(in crate::app) fn adopt_scope_refresh(&mut self, ctx: &egui::Context) {
        self.editor.revision = self.scan.report_revision;
        self.file_preview.adopt_revision(self.scan.report_revision);
        if self.editor.document.is_some() {
            self.editor.bookmark = Some(Bookmark::capture(self));
        }
        self.reload_changed_document(ctx);
    }
    pub(in crate::app) fn reload_changed_document(&mut self, ctx: &egui::Context) {
        if self.editor.untitled.is_some() || self.editor.saving.is_some() {
            return;
        }
        let Some(path) = self.editor.path.clone() else {
            return;
        };
        let metadata = std::fs::symlink_metadata(&path).ok();
        let same = self
            .editor
            .document
            .as_ref()
            .map(|d| metadata.as_ref().is_some_and(|m| d.matches_disk(m)))
            .unwrap_or_else(|| metadata.as_ref().map(Stamp::of) == self.editor.preview_disk);
        if same {
            return;
        }
        if self.editor.dirty() {
            self.editor.error = crate::tr!("Файл змінився на диску. Незбережений текст залишено; запис потребує перевірки конфлікту.",
                "The file changed on disk. Your unsaved text was kept; saving requires a conflict check.").into();
        } else {
            self.editor.document = None;
            self.editor.keep_cursor = true;
            self.editor.preview_disk = metadata.as_ref().map(Stamp::of);
            self.file_preview.reload();
        }
        ctx.request_repaint();
    }
    pub(in crate::app) fn sync_file_preview(&mut self, ctx: &egui::Context) {
        self.poll_tabs(ctx);
        self.poll_editor(ctx);
        let focus_tabs = self.preferences.layout.preview_mode == PreviewMode::Focus;
        let mut desired = if self.preferences.layout.preview_mode != PreviewMode::Off
            && self.browser.view != View::Trash
        {
            self.browser
                .properties
                .as_ref()
                .filter(|p| !p.is_dir && p.error.is_none())
                .map(|p| p.path.clone())
        } else {
            None
        };
        let restore_cached = !self.editor.dirty()
            && self.editor.saving.is_none()
            && desired
                .as_ref()
                .is_some_and(|path| self.tabs.inactive.contains_key(path));
        if self.editor.untitled.is_some() && !focus_tabs && desired.is_none() {
            // Overview may hide an untitled document without abandoning its
            // buffer. Selecting another file stashes it as an ordinary tab.
            return;
        }
        if focus_tabs || restore_cached || self.editor.untitled.is_some() {
            if focus_tabs {
                desired = desired.or_else(|| self.editor.path.clone());
            }
            if let Some(path) = &desired {
                if !self.open_focus_document(path.clone(), ctx) {
                    return;
                }
            }
        }
        let protected = self.editor.dirty() || self.editor.saving.is_some();
        if !focus_tabs && protected && desired != self.editor.path {
            if self.editor.pending.is_none() {
                self.editor.pending = Some(Action::Transition(Bookmark::capture(self)));
                self.editor.confirm_fresh = true;
            }
            if let Some(bookmark) = self.editor.bookmark.clone() {
                bookmark.restore(self);
            }
            return;
        }
        if protected && !focus_tabs {
            if let Some(bookmark) = &mut self.editor.bookmark {
                bookmark.table = self.browser.table_root;
                bookmark.mode = self.preferences.layout.preview_mode;
                bookmark.show_properties = self.preferences.layout.properties;
            }
            return;
        }
        if !protected
            && (self.editor.path != desired
                || (!focus_tabs && self.editor.revision != self.scan.report_revision))
        {
            self.editor.auto_save_at = None;
            self.editor.last_saved_at = None;
            self.editor.document = None;
            self.editor.error.clear();
            self.editor.bookmark = None;
            self.editor.path = desired.clone();
            self.editor.origin = Some(self.browser.category);
            self.editor.preview_disk = desired
                .as_ref()
                .and_then(|path| std::fs::symlink_metadata(path).ok())
                .as_ref()
                .map(Stamp::of);
            self.editor.revision = self.scan.report_revision;
        }
        if self.editor.untitled.is_some() {
            if let Some(path) = self.editor.path.clone() {
                self.file_preview.restore_text(
                    path,
                    self.scan.report_revision,
                    super::untitled::content(),
                );
            }
            return;
        }
        self.file_preview.select(desired, self.scan.report_revision);
        self.file_preview.poll(ctx);
        if self.editor.document.is_none() {
            if let Some(crate::file_preview::Content::Text { document, .. }) =
                &mut self.file_preview.content
            {
                if let Some(doc) = document.take() {
                    self.editor.document = Some(doc);
                    if !self.editor.keep_cursor {
                        egui::TextEdit::store_state(
                            ctx,
                            egui::Id::unique(("file-editor", &self.editor.path)),
                            Default::default(),
                        );
                    }
                    self.editor.keep_cursor = false;
                    self.editor.bookmark = Some(Bookmark::capture(self));
                    if self.editor.focus_on_load {
                        ctx.memory_mut(|m| {
                            m.request_focus(egui::Id::unique(("file-editor", &self.editor.path)))
                        });
                        self.editor.focus_on_load = false;
                    }
                }
            }
        }
        // Layout changes do not constitute leaving the current document.
        if let Some(bookmark) = &mut self.editor.bookmark {
            bookmark.mode = self.preferences.layout.preview_mode;
            bookmark.show_properties = self.preferences.layout.properties;
        }
    }
    pub(in crate::app) fn editor_action(&mut self, action: Action, ctx: &egui::Context) {
        let browsing = matches!(action, Action::NewTextTab(_))
            || (self.preferences.layout.preview_mode == PreviewMode::Focus
                && matches!(
                    action,
                    Action::SelectFile { .. }
                        | Action::Navigate(_)
                        | Action::NewFile(_)
                        | Action::NewTextTab(_)
                ));
        if !browsing && (self.editor.dirty() || self.editor.saving.is_some()) {
            self.editor.pending = Some(action);
            self.editor.confirm_fresh = true;
            ctx.request_repaint();
        } else {
            self.apply_editor_action(action, ctx);
        }
    }
    pub(in crate::app) fn apply_editor_action(&mut self, action: Action, ctx: &egui::Context) {
        self.editor.error.clear();
        match action {
            Action::Transition(bookmark) => {
                self.editor.untitled = None;
                bookmark.restore(self);
                self.editor.document = None;
                self.editor.path = None;
            }
            Action::Navigate(path) => self.navigate_path(path),
            Action::NewTextTab(directory) => self.open_empty_tab(directory, ctx),
            Action::NewFile(directory) => {
                self.new_file = Some(super::new_file::Draft::new(directory));
            }
            Action::SelectFile {
                path,
                bytes,
                focus_editor,
            } => {
                self.preview_keyboard_file(path.clone(), bytes);
                self.browser.scroll_to = Some(path);
                self.editor.focus_on_load = focus_editor;
            }
            Action::Scan {
                root,
                dup,
                force,
                automatic,
            } => {
                self.browser.root = root;
                self.start_job(dup, force, automatic);
            }
            Action::Reload => {
                self.editor.document = None;
                self.file_preview.reload();
            }
            Action::Delete(targets) => {
                self.request_delete(targets);
            }
            Action::CloseTab(path) => self.close_tab_now(&path, ctx),
            Action::Close => self.request_close(ctx),
        }
        ctx.request_repaint();
    }
    pub(in crate::app) fn save_editor(&mut self, ctx: &egui::Context) {
        if self.editor_read_only() {
            self.editor.error = crate::tr!(
                "Редагування заборонене для цієї категорії",
                "Editing is disabled for this category"
            )
            .into();
            return;
        }
        if self.editor.untitled.is_some() {
            self.request_save_as(ctx);
            return;
        }
        if self.demo || self.editor.saving.is_some() || !self.editor.dirty() {
            return;
        }
        self.editor.save_in_background(ctx);
    }
    fn poll_editor(&mut self, ctx: &egui::Context) {
        if self.editor.poll_save(ctx) == Some(true) {
            if let Some(path) = self.editor.path.clone() {
                if let Ok(m) = std::fs::metadata(&path) {
                    if self
                        .browser
                        .properties
                        .as_ref()
                        .is_some_and(|p| p.path == path)
                    {
                        self.browser.properties = Some(Properties::load(
                            path.clone(),
                            crate::scan::allocated(&m),
                            None,
                            false,
                        ));
                    }
                }
                if let Some(parent) = path.parent() {
                    self.files.invalidate(parent);
                }
            }
            self.message = crate::tr!("Файл збережено", "File saved").into();
            if !self.editor.dirty() {
                if let Some(action) = self.editor.pending.take() {
                    self.apply_editor_action(action, ctx);
                }
            }
        }
    }
    pub(in crate::app) fn guard_editor_scan(
        &mut self,
        dup: bool,
        force: bool,
        automatic: bool,
    ) -> bool {
        if !self.editor.dirty() && self.editor.saving.is_none() {
            return false;
        }
        if !automatic {
            self.editor.pending = Some(Action::Scan {
                root: self.browser.root.clone(),
                dup,
                force,
                automatic,
            });
            self.editor.confirm_fresh = true;
            if let Some(bookmark) = self.editor.bookmark.clone() {
                bookmark.restore(self);
            }
        }
        true
    }
    pub(in crate::app) fn request_close(&mut self, ctx: &egui::Context) {
        if self.editor.dirty() || self.editor.saving.is_some() {
            self.editor_action(Action::Close, ctx);
        } else if let Some(path) = self
            .tabs
            .inactive
            .iter()
            .find(|(_, tab)| tab.editor.dirty() || tab.editor.saving.is_some())
            .map(|(path, _)| path.clone())
        {
            self.activate_tab(&path, ctx);
            self.editor_action(Action::Close, ctx);
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    pub(in crate::app) fn guard_native_close(&mut self, ctx: &egui::Context) {
        if self.save_as.is_some() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            return;
        }
        if ctx.input(|i| i.viewport().close_requested())
            && (self.editor.dirty() || self.editor.saving.is_some() || self.tabs.has_unsaved())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_close(ctx);
        }
    }
}
