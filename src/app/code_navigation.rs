//! Shared LSP/diagnostic navigation. Opening a target retains other tab drafts.
use super::{Disco, state::View};
use crate::{filesystem::Properties, lsp::protocol::Location, settings::PreviewMode};
use eframe::egui;
impl Disco {
    pub(in crate::app) fn open_code_location(&mut self, location: Location, ctx: &egui::Context) {
        if let Some(path) = self.editor.path.clone() {
            self.keep_preview_tab(&path);
        }
        self.preferences.layout.preview_mode = PreviewMode::Focus;
        self.preferences.layout.properties = false;
        self.browser.view = View::Folders;
        self.tabs.preview_request = None;
        if !self.open_focus_document(location.path.clone(), ctx) {
            return;
        }
        self.keep_preview_tab(&location.path);
        self.browser.properties = Some(Properties::load(location.path, 0, None, false));
        self.editor.markdown.editing = true;
        self.editor.jump = Some(location.range.start);
        self.editor.focus_on_load = true;
        ctx.request_repaint();
    }
}
