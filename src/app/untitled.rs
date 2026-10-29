//! In-memory text tabs and explicit, non-overwriting first saves.
use super::{
    Disco,
    editor::{Action, Bookmark},
    state::View,
};
use crate::{
    file_creation::Created,
    file_preview::Content,
    settings::PreviewMode,
    text_editor::{Document, Stamp},
};
use eframe::egui::{self, Id, Key, Modifiers};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};
#[derive(Clone)]
pub(super) struct Untitled {
    pub directory: PathBuf,
    number: u64,
}
impl Untitled {
    pub fn label(&self) -> String {
        crate::tr!(
            "Без назви {number}",
            "Untitled {number}",
            number = self.number
        )
    }
}
pub(super) struct SaveAs {
    pub directory: PathBuf,
    pub input: String,
    pub error: String,
    pub fresh: bool,
    identity: PathBuf,
    job: Option<Receiver<Result<(Created, Document), String>>>,
}
pub(super) fn content() -> Content {
    Content::Text {
        lines: vec![String::new()],
        display_source: String::new(),
        encoding: "UTF-8",
        truncated: false,
        document: None,
    }
}
impl Disco {
    pub(in crate::app) fn request_empty_tab(&mut self, ctx: &egui::Context) {
        if self.demo
            || self.preferences.open
            || self.save_as.is_some()
            || self.new_file.is_some()
            || self.editor.pending.is_some()
            || self.browser.view == View::Trash
        {
            return;
        }
        if self.category_read_only() {
            self.message = crate::tr!(
                "Категорія доступна лише для читання",
                "This category is read only"
            )
            .into();
            return;
        }
        let base = self.breadcrumb_directory();
        let base = if base.is_dir() {
            base
        } else {
            PathBuf::from(&self.browser.root)
        };
        self.editor_action(Action::NewTextTab(base), ctx);
    }
    pub(in crate::app) fn open_empty_tab(&mut self, directory: PathBuf, ctx: &egui::Context) {
        if self.tabs.order.len() >= 64 {
            self.message = crate::tr!(
                "Закрий одну з 64 вкладок перед створенням нової",
                "Close one of the 64 tabs before creating another"
            )
            .into();
            return;
        }
        self.set_preview_mode(PreviewMode::Focus, ctx);
        self.tabs.next_untitled += 1;
        let number = self.tabs.next_untitled;
        let identity = PathBuf::from(format!("untitled:{number}"));
        self.tabs.preview_request = None;
        if !self.open_focus_document(identity.clone(), ctx) {
            return;
        }
        self.editor.untitled = Some(Untitled { directory, number });
        self.editor.document = Some(Document::empty());
        self.editor.focus_on_load = false;
        self.editor.keep_cursor = false;
        self.browser.properties = None;
        self.editor.bookmark = Some(Bookmark::capture(self));
        self.file_preview
            .restore_text(identity, self.scan.report_revision, content());
        self.focus_area(super::workspace_keyboard::Area::Editor, ctx);
    }
    pub(in crate::app) fn request_save_as(&mut self, ctx: &egui::Context) {
        if self.save_as.is_some() {
            return;
        }
        let (Some(untitled), Some(identity)) = (&self.editor.untitled, &self.editor.path) else {
            return;
        };
        self.save_as = Some(SaveAs {
            directory: untitled.directory.clone(),
            input: String::new(),
            error: String::new(),
            fresh: true,
            identity: identity.clone(),
            job: None,
        });
        ctx.request_repaint();
    }
    pub(in crate::app) fn submit_save_as(&mut self, ctx: &egui::Context) {
        if self.editor_read_only() {
            if let Some(save) = &mut self.save_as {
                save.error = crate::tr!(
                    "Категорія доступна лише для читання",
                    "This category is read only"
                )
                .into();
                save.fresh = true;
            }
            return;
        }
        let Some(save) = &mut self.save_as else {
            return;
        };
        if save.job.is_some() {
            return;
        }
        let Some(document) = self.editor.document.clone() else {
            return;
        };
        if self.editor.path.as_ref() != Some(&save.identity) {
            return;
        }
        let base = save.directory.clone();
        let input = save.input.clone();
        let (tx, rx) = mpsc::channel();
        save.job = Some(rx);
        save.error.clear();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::file_creation::create_document(&base, &input, &document.encoded())
                .map(|created| (created, document));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub(in crate::app) fn poll_save_as(&mut self, ctx: &egui::Context) {
        let result = self
            .save_as
            .as_ref()
            .and_then(|s| s.job.as_ref())
            .and_then(|rx| match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(crate::tr!(
                    "Збереження перервано; чернетку залишено",
                    "Saving was interrupted; the draft was kept"
                )
                .into())),
            });
        let Some(result) = result else {
            return;
        };
        match result {
            Err(error) => {
                let save = self.save_as.as_mut().unwrap();
                save.error = error;
                save.job = None;
                save.fresh = true;
            }
            Ok((created, saved)) => {
                let save = self.save_as.take().unwrap();
                let old = save.identity;
                let path = created.path.clone();
                // Preserve cursor/undo and the active draft while rekeying its tab.
                let old_id = Id::unique(("file-editor", Some(old.clone())));
                let new_id = Id::unique(("file-editor", Some(path.clone())));
                if let Some(state) = egui::TextEdit::load_state(ctx, old_id) {
                    egui::TextEdit::store_state(ctx, new_id, state);
                }
                for key in &mut self.tabs.order {
                    if *key == old {
                        *key = path.clone();
                    }
                }
                self.tabs.preview = self.tabs.preview.take().filter(|p| *p != old);
                self.tabs.preview_request = None;
                self.editor.path = Some(path.clone());
                self.editor.untitled = None;
                if let Some(doc) = &mut self.editor.document {
                    doc.accept_saved(Stamp::of(&created.metadata), &saved);
                }
                self.editor.last_saved_at = Some(ctx.input(|i| i.time));
                self.editor.auto_save_at = None;
                self.editor.error.clear();
                self.integrate_created_directories(&created);
                self.file_preview
                    .restore_text(path.clone(), self.scan.report_revision, content());
                self.browser.properties = Some(crate::filesystem::Properties::load(
                    path.clone(),
                    crate::scan::allocated(&created.metadata),
                    None,
                    false,
                ));
                if let Some(parent) = path.parent() {
                    self.files.invalidate(parent);
                    self.files.request(parent.into(), ctx);
                }
                if matches!(self.editor.pending, Some(Action::CloseTab(ref p)) if *p == old) {
                    self.editor.pending = Some(Action::CloseTab(path.clone()));
                }
                self.editor.bookmark = Some(Bookmark::capture(self));
                self.focus_area(super::workspace_keyboard::Area::Editor, ctx);
                self.message = crate::tr!(
                    "Файл збережено: {path}",
                    "File saved: {path}",
                    path = path.display()
                );
                if !self.editor.dirty()
                    && let Some(action) = self.editor.pending.take()
                {
                    self.apply_editor_action(action, ctx);
                }
            }
        }
        ctx.request_repaint();
    }
    pub(in crate::app) fn save_as_dialog(&mut self, ctx: &egui::Context) {
        let Some(save) = &mut self.save_as else {
            return;
        };
        let busy = save.job.is_some();
        // Capture Enter before single-line TextEdit handles it as focus loss.
        let mut submit = !busy && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        let mut cancel = false;
        egui::Modal::new(Id::unique("save-as-dialog")).show(ctx, |ui| {
            ui.set_width(500.);
            ui.heading(crate::tr!("Зберегти новий файл", "Save new file"));
            ui.add_space(10.);
            ui.label(crate::tr!("Базова папка для відносних шляхів", "Base directory for relative paths"));
            ui.horizontal(|ui| {
                ui.label(save.directory.display().to_string());
                if ui.add_enabled(!busy, egui::Button::new(crate::tr!("Обрати…", "Choose…"))).clicked()
                    && let Some(path) = rfd::FileDialog::new().set_directory(&save.directory).pick_folder() { save.directory = path; save.fresh = true; }
            });
            ui.add_space(10.);
            let response = ui.add_enabled(!busy, egui::TextEdit::singleline(&mut save.input)
                .id(Id::unique("save-as-path")).desired_width(f32::INFINITY).hint_text("notes.txt / folder/notes.txt / /home/user/notes.txt"));
            if save.fresh { response.request_focus(); save.fresh = false; }
            ui.label(crate::tr!("Відносний шлях починається з папки вище. Абсолютний шлях починається з /. Вкладені папки створяться автоматично.", "Relative paths start in the folder above. Absolute paths start with /. Missing parent folders are created automatically."));
            if !save.error.is_empty() { ui.colored_label(crate::settings::color(&self.preferences.theme.accent), &save.error); }
            ui.add_space(12.);
            ui.horizontal(|ui| {
                cancel = ui.add_enabled(!busy, egui::Button::new(crate::tr!("Скасувати", "Cancel"))).clicked();
                submit |= ui.add_enabled(!busy && !save.input.trim().is_empty(), egui::Button::new(if busy { crate::tr!("Збереження…", "Saving…") } else { crate::tr!("Зберегти", "Save") })).clicked();
            });
        });
        cancel |= !busy && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        submit |= !busy && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        if cancel {
            self.save_as = None;
            self.focus_area(super::workspace_keyboard::Area::Editor, ctx);
        } else if submit {
            self.submit_save_as(ctx);
        }
    }
}
