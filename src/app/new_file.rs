//! Explicit, non-overwriting file creation in the selected directory.
use super::{Disco, editor::Action, state::View};
use eframe::egui::{self, Key, Modifiers};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};

#[derive(Debug)]
pub(super) struct Draft {
    directory: PathBuf,
    name: String,
    error: String,
    fresh: bool,
    creating: Option<Receiver<Result<crate::file_creation::Created, String>>>,
}
impl Draft {
    #[cfg(test)]
    pub(in crate::app) fn has_error(&self) -> bool {
        self.creating.is_none() && !self.error.is_empty()
    }
    pub(super) fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            name: String::new(),
            error: String::new(),
            fresh: true,
            creating: None,
        }
    }
}
impl Disco {
    /// Extend the current snapshot with directories created by this operation.
    /// Folder IDs stay stable; no home-directory rescan replaces the user's view.
    pub(in crate::app) fn integrate_created_directories(
        &mut self,
        created: &crate::file_creation::Created,
    ) {
        let Some(report) = &mut self.scan.report else {
            return;
        };
        if report
            .folders
            .first()
            .is_none_or(|f| !created.path.starts_with(&f.path))
        {
            return;
        }
        let report = std::sync::Arc::make_mut(report);
        let mut added = false;
        for directory in &created.directories {
            if report.folders.iter().any(|f| f.path == directory.path) {
                continue;
            }
            let Some(parent) = report
                .folders
                .iter()
                .position(|f| Some(f.path.as_path()) == directory.path.parent())
            else {
                continue;
            };
            report.folders.push(crate::scan::Folder {
                path: directory.path.clone(),
                parent: Some(parent),
                bytes: directory.bytes,
                files: 0,
            });
            let mut at = Some(parent);
            while let Some(id) = at {
                report.folders[id].bytes = report.folders[id].bytes.saturating_add(directory.bytes);
                at = report.folders[id].parent;
            }
            // An existing directory absent from the saved scan may contain
            // more data; don't claim a complete snapshot of that directory.
            report.complete &= directory.created;
            report.entries = report.entries.saturating_add(1);
            added = true;
        }
        let mut at = report
            .folders
            .iter()
            .position(|f| Some(f.path.as_path()) == created.path.parent());
        let bytes = crate::scan::allocated(&created.metadata);
        while let Some(id) = at {
            report.folders[id].bytes = report.folders[id].bytes.saturating_add(bytes);
            report.folders[id].files = report.folders[id].files.saturating_add(1);
            at = report.folders[id].parent;
        }
        report.entries = report.entries.saturating_add(1);
        if added || bytes > 0 {
            self.scan.index = crate::index::Index::new(report);
            self.collections.indexed = None;
        }
        self.scan.tracker = None;
        self.scan.snapshot_at = None;
        self.browser.tree_revision = self.browser.tree_revision.wrapping_add(1);
    }
    pub(in crate::app) fn request_new_file(&mut self, ctx: &egui::Context) {
        if self.category_read_only() || self.scoped.busy() {
            return;
        }
        if self.demo
            || self.scan.busy
            || self.browser.view == View::Trash
            || self.new_file.is_some()
            || self.deletion.rx.is_some()
            || self.recycle.busy
            || self.preferences.open
            || self.editor.pending.is_some()
        {
            return;
        }
        self.editor_action(Action::NewFile(self.breadcrumb_directory()), ctx);
    }
    pub(in crate::app) fn poll_new_file(&mut self, ctx: &egui::Context) {
        let result = self
            .new_file
            .as_ref()
            .and_then(|draft| draft.creating.as_ref())
            .and_then(|rx| match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(crate::tr!(
                    "Не вдалося створити файл",
                    "Could not create the file"
                )
                .into())),
            });
        let Some(result) = result else {
            return;
        };
        match result {
            Ok(created) => {
                self.integrate_created_directories(&created);
                let path = created.path;
                self.new_file = None;
                if let Some(parent) = path.parent() {
                    self.files.invalidate(parent);
                    if let Some(id) = self
                        .scan
                        .report
                        .as_ref()
                        .and_then(|r| r.folders.iter().position(|f| f.path == parent))
                    {
                        self.enter_folder(id);
                    }
                    self.files.request(parent.to_path_buf(), ctx);
                }
                self.browser.filter.clear();
                self.select_file(path.clone(), 0);
                self.browser.scroll_to = Some(path);
                self.editor.focus_on_load = true;
                self.message = crate::tr!("Файл створено", "File created").into();
            }
            Err(error) => {
                let draft = self.new_file.as_mut().unwrap();
                draft.creating = None;
                draft.error = error;
                draft.fresh = true;
            }
        }
        ctx.request_repaint();
    }
    pub(in crate::app) fn new_file_dialog(&mut self, ctx: &egui::Context) {
        let read_only = self.category_read_only();
        let Some(draft) = &mut self.new_file else {
            return;
        };
        let busy = draft.creating.is_some();
        let mut submit = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("new-file-dialog")).show(ctx, |ui| {
            ui.set_width(440.);
            ui.heading(crate::tr!("Новий файл", "New file"));
            ui.add_space(8.);
            ui.label(draft.directory.display().to_string());
            ui.add_space(10.);
            ui.label(crate::tr!("Назва або шлях", "Name or path"));
            let response = ui.add_enabled(
                !busy,
                egui::TextEdit::singleline(&mut draft.name)
                    .id(egui::Id::unique("new-file-name"))
                    .desired_width(f32::INFINITY)
                    .hint_text("new_folder/file.txt"),
            );
            if draft.fresh {
                response.request_focus();
                draft.fresh = false;
            }
            submit |= !busy && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
            ui.label(crate::tr!(
                "Шлях усередині папки вище: /new_folder/file.txt створить і папку, і файл. Розширення довільне.",
                "Paths stay within the folder above: /new_folder/file.txt creates both the folder and the file. Use any extension."
            ));
            if !draft.error.is_empty() {
                ui.colored_label(
                    crate::settings::color(&self.preferences.theme.accent),
                    &draft.error,
                );
            }
            ui.add_space(12.);
            ui.horizontal(|ui| {
                cancel = ui
                    .add_enabled(!busy, egui::Button::new(crate::tr!("Скасувати", "Cancel")))
                    .clicked();
                submit |= ui
                    .add_enabled(
                        !busy && !draft.name.trim().is_empty(),
                        egui::Button::new(if busy {
                            crate::tr!("Створення…", "Creating…")
                        } else {
                            crate::tr!("Створити", "Create")
                        }),
                    )
                    .clicked();
            });
        });
        cancel |= !busy && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        if cancel {
            self.new_file = None;
        } else if submit && read_only {
            draft.error = crate::tr!(
                "Категорія доступна лише для читання",
                "This category is read only"
            )
            .into();
            draft.fresh = true;
        } else if submit && !busy {
            draft.error.clear();
            let directory = draft.directory.clone();
            let name = draft.name.clone();
            let (tx, rx) = mpsc::channel();
            draft.creating = Some(rx);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(crate::file_creation::create(&directory, &name));
                ctx.request_repaint();
            });
        }
    }
}
