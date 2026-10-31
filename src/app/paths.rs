use super::{Disco, editor::Action, state::View, workspace_keyboard::Area};
use crate::{
    index::Category,
    path_input::{Completion, Resolved, Resolver},
    settings::PreviewMode,
};
use eframe::egui::{self, Key, Modifiers};
use std::path::PathBuf;
#[derive(Default)]
pub(super) struct Paths {
    pub standalone: Option<PathBuf>,
    /// Shallow directory browsing outside the available scan snapshot.
    pub directory: Option<PathBuf>,
    pub editing: bool,
    pub input: String,
    pub focus: bool,
    pub select_all: bool,
    pub chosen: Option<usize>,
    pub completion: Completion,
    pub resolver: Resolver,
    pub error: String,
}
impl Disco {
    pub(in crate::app) fn leave_path_browser(&mut self) {
        self.paths.standalone = None;
        self.paths.directory = None;
        self.browser.rows_cache = None;
        self.browser.tree_revision = self.browser.tree_revision.wrapping_add(1);
    }

    pub(in crate::app) fn begin_path_edit(&mut self, ctx: &egui::Context) {
        self.paths.input = self
            .paths
            .standalone
            .clone()
            .unwrap_or_else(|| self.breadcrumb_directory())
            .display()
            .to_string();
        self.paths.editing = true;
        self.paths.focus = true;
        self.paths.select_all = true;
        self.paths.chosen = None;
        self.paths.error.clear();
        self.configure_path_completion();
        ctx.request_repaint();
    }
    pub(in crate::app) fn configure_path_completion(&mut self) {
        let base = self.breadcrumb_directory();
        match crate::path_input::expand(&self.paths.input, &base) {
            Ok(path) => self.paths.completion.configure(path),
            Err(e) => self.paths.error = e,
        }
    }
    pub(in crate::app) fn path_shortcuts(&mut self, ctx: &egui::Context) {
        if self.preferences.open
            || self.editor.pending.is_some()
            || self.save_as.is_some()
            || self.new_file.is_some()
            || self.deletion.confirm
            || self.terminal_focused(ctx)
        {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::L)) {
            self.begin_path_edit(ctx);
        }
        if self.paths.editing && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.paths.editing = false;
            self.paths.error.clear();
            self.focus_area(Area::Tree, ctx);
        }
    }
    pub(in crate::app) fn submit_path(&mut self, text: String, ctx: &egui::Context) {
        self.paths.input = text.clone();
        match crate::path_input::expand(&text, &self.breadcrumb_directory()) {
            Ok(path) => {
                self.paths.resolver.request(path);
                self.paths.editing = false;
                self.paths.error.clear();
            }
            Err(e) => self.paths.error = e,
        }
        ctx.request_repaint();
    }
    pub(in crate::app) fn poll_paths(&mut self, ctx: &egui::Context) {
        self.paths.completion.poll(ctx);
        if let Some(path) = self.paths.directory.clone() {
            self.files.request(path.clone(), ctx);
            if let Some(listing) = self.files.get(&path) {
                if let Some(error) = &listing.error {
                    self.message = error.clone();
                } else if listing.limited {
                    self.message =
                        crate::tr!("Показано частину папки", "Showing a partial directory").into();
                }
            }
        }
        if let Some(result) = self.paths.resolver.poll(ctx) {
            match result {
                Ok(path) => self.editor_action(Action::OpenPath(path), ctx),
                Err(e) => {
                    self.paths.error = e.clone();
                    self.message = e;
                    self.paths.editing = true;
                    self.paths.focus = true;
                }
            }
        }
    }
    pub(in crate::app) fn open_resolved_path(&mut self, resolved: Resolved, ctx: &egui::Context) {
        if self.scan.busy || self.deletion.rx.is_some() || self.recycle.busy {
            self.message = crate::tr!(
                "Дочекайся завершення поточної операції",
                "Wait for the current operation to finish"
            )
            .into();
            return;
        }
        self.scoped.cancel();
        self.scoped = Default::default();
        self.scan.report_revision = self.scan.report_revision.wrapping_add(1);
        self.browser.category = Category::All;
        self.browser.view = View::Folders;
        self.browser.filter.clear();
        self.browser.table_root = None;
        self.browser.expanded.clear();
        self.browser.selected.clear();
        self.browser.rows_cache = None;
        self.browser.tree_revision = self.browser.tree_revision.wrapping_add(1);
        self.paths.editing = false;
        self.paths.error.clear();
        self.demo = false;
        self.paths.directory = None;
        if resolved.directory {
            self.paths.standalone = None;
            self.browser.root = resolved.path.display().to_string();
            self.start_job(false, true, false);
        } else {
            self.paths.standalone = Some(resolved.path.clone());
            self.browser.root = resolved
                .path
                .parent()
                .unwrap_or(std::path::Path::new("/"))
                .display()
                .to_string();
            self.preferences.layout.preview_mode = PreviewMode::Focus;
            self.preferences.layout.properties = false;
            // Keep the snapshot available for Home/category navigation.
            self.scan.tracker = None;
            self.browser.properties = Some(crate::filesystem::Properties::load(
                resolved.path.clone(),
                resolved.bytes,
                None,
                false,
            ));
            self.tabs.preview_request = None;
            self.open_focus_document(resolved.path.clone(), ctx);
            self.keep_preview_tab(&resolved.path);
            self.editor.focus_on_load = true;
        }
        ctx.request_repaint();
    }
}
