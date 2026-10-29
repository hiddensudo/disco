//! View modules share only the application-owned state and small rendering helpers.
mod breadcrumbs;
mod chrome;
mod collections;
mod dialogs;
mod editor_dialogs;
mod file_modes;
mod file_preview;
mod files;
mod folders;
mod inspector;
mod inspector_layout;
mod keyboard;
mod navigation;
pub(in crate::app) mod preferences;
mod preview;
mod style;
mod tools;
mod trash;
mod widgets;
pub(in crate::app) use widgets::resize_edges;

#[cfg(test)]
mod interaction_tests;
