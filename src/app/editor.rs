//! Keep editor state separate from preview decoding and guard transitions that
//! would abandon a draft. Saving is explicit and performed off the UI thread.
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
    history: History<super::state::NavState>,
}
impl Bookmark {
    fn capture(app: &Disco) -> Self {
        Self {
            root: app.browser.root.clone(),
            table: app.browser.table_root,
            focus: app.browser.focus,
            category: app.browser.category,
            view: app.browser.view,
            filter: app.browser.filter.clone(),
            properties: app.browser.properties.clone(),
            mode: app.preferences.layout.preview_mode,
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
        app.browser.history = self.history;
        app.browser.nav_traversed = true;
        app.browser.scroll_to = app.browser.properties.as_ref().map(|p| p.path.clone());
    }
}
pub(super) enum Action {
    Transition(Bookmark),
    Mode(PreviewMode),
    Reload,
    Scan {
        root: String,
        dup: bool,
        force: bool,
        automatic: bool,
    },
    Delete(Vec<(PathBuf, u64)>),
    Close,
}
#[derive(Default)]
pub(super) struct Editor {
    pub path: Option<PathBuf>,
    pub document: Option<Document>,
    pub focused: bool,
    pub pending: Option<Action>,
    pub error: String,
    pub confirm_fresh: bool,
    pub saving: Option<Receiver<Result<Stamp, String>>>,
    bookmark: Option<Bookmark>,
    revision: u64,
}
impl Editor {
    pub fn dirty(&self) -> bool {
        self.document.as_ref().is_some_and(Document::dirty)
    }
}
impl Disco {
    pub(in crate::app) fn sync_file_preview(&mut self, ctx: &egui::Context) {
        self.poll_editor(ctx);
        let desired = if self.preferences.layout.preview_mode != PreviewMode::Off
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
        let protected = self.editor.dirty() || self.editor.saving.is_some();
        if protected && desired != self.editor.path {
            if self.editor.pending.is_none() {
                self.editor.pending = Some(Action::Transition(Bookmark::capture(self)));
                self.editor.confirm_fresh = true;
            }
            if let Some(bookmark) = self.editor.bookmark.clone() {
                bookmark.restore(self);
            }
            return;
        }
        if protected {
            return;
        }
        if self.editor.path != desired || self.editor.revision != self.scan.report_revision {
            self.editor.document = None;
            self.editor.error.clear();
            self.editor.bookmark = None;
            self.editor.path = desired.clone();
            self.editor.revision = self.scan.report_revision;
        }
        self.file_preview.select(desired, self.scan.report_revision);
        self.file_preview.poll(ctx);
        if self.editor.document.is_none() {
            if let Some(crate::file_preview::Content::Text { document, .. }) =
                &mut self.file_preview.content
            {
                if let Some(doc) = document.take() {
                    self.editor.document = Some(doc);
                    egui::TextEdit::store_state(
                        ctx,
                        egui::Id::unique(("file-editor", &self.editor.path)),
                        Default::default(),
                    );
                    self.editor.bookmark = Some(Bookmark::capture(self));
                }
            }
        }
        // Layout changes do not constitute leaving the current document.
        if let Some(bookmark) = &mut self.editor.bookmark {
            bookmark.mode = self.preferences.layout.preview_mode;
        }
    }
    pub(in crate::app) fn editor_action(&mut self, action: Action, ctx: &egui::Context) {
        if self.editor.dirty() || self.editor.saving.is_some() {
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
                bookmark.restore(self);
                self.editor.document = None;
                self.editor.path = None;
            }
            Action::Mode(mode) => {
                self.set_preview_mode(mode, ctx);
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
            Action::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
        ctx.request_repaint();
    }
    pub(in crate::app) fn save_editor(&mut self, ctx: &egui::Context) {
        if self.demo || self.editor.saving.is_some() || !self.editor.dirty() {
            return;
        }
        let Some(path) = self.editor.path.clone() else {
            return;
        };
        let Some(document) = self.editor.document.clone() else {
            return;
        };
        self.editor.error.clear();
        let (tx, rx) = mpsc::channel();
        self.editor.saving = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::text_editor::save(&path, &document);
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    fn poll_editor(&mut self, ctx: &egui::Context) {
        let result = self
            .editor
            .saving
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(crate::tr!(
                    "Не вдалося зберегти файл",
                    "Could not save the file"
                )
                .into())),
            });
        if let Some(result) = result {
            self.editor.saving = None;
            match result {
                Ok(stamp) => {
                    if let Some(doc) = &mut self.editor.document {
                        doc.mark_saved(stamp);
                    }
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
                    if let Some(action) = self.editor.pending.take() {
                        self.apply_editor_action(action, ctx);
                    }
                }
                Err(error) => self.editor.error = error,
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
        self.editor_action(Action::Close, ctx);
    }
    pub(in crate::app) fn guard_native_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested())
            && (self.editor.dirty() || self.editor.saving.is_some())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.editor_action(Action::Close, ctx);
        }
    }
}
