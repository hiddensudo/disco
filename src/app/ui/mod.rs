//! View modules share only the application-owned state and small rendering helpers.
mod chrome;
mod dialogs;
mod editor_dialogs;
mod file_preview;
mod files;
mod folders;
mod inspector;
mod keyboard;
mod navigation;
mod preferences;
mod preview;
mod style;
mod tools;
mod trash;
mod widgets;
pub(in crate::app) use widgets::resize_edges;
