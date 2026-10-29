//! Cross-panel keyboard focus, distinct from text cursor and file-list movement.
use super::{Disco, state::View};
use crate::settings::{PreviewMode, color};
use eframe::egui::{self, Id, Key, Modifiers, Rect, Stroke};
use std::path::PathBuf;
const TRASH: &str = "__trash";
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Area {
    Categories,
    #[default]
    Tree,
    Tabs,
    Editor,
    Search,
    Terminal,
}
#[derive(Default)]
pub(super) struct Keyboard {
    pub area: Area,
    pub visible: bool,
    pub category: Option<String>,
    pub tab: Option<PathBuf>,
    pub reveal_category: bool,
    pub reveal_tab: bool,
    pending_focus: Option<Id>,
}
impl Disco {
    fn area_rect(&self, ctx: &egui::Context, area: Area) -> Option<Rect> {
        let focus = self.preferences.layout.preview_mode == PreviewMode::Focus
            && self.browser.view != View::Trash;
        let name = match area {
            Area::Categories => "category-panel-bounds",
            Area::Tree => {
                if focus {
                    "focus-tree-panel"
                } else {
                    "browser-panel-bounds"
                }
            }
            Area::Tabs => "document-tabs-bounds",
            Area::Editor => {
                if focus {
                    "focus-document-panel"
                } else {
                    "properties-panel"
                }
            }
            Area::Search => "folder-search-bounds",
            Area::Terminal => "terminal-panel-bounds",
        };
        ctx.read_response(Id::unique(name)).map(|r| r.rect)
    }
    fn available_areas(&self) -> Vec<Area> {
        let mut areas = vec![];
        if self.preferences.layout.categories {
            areas.push(Area::Categories);
        }
        areas.push(Area::Tree);
        if self.browser.view != View::Trash {
            if self.preferences.layout.preview_mode == PreviewMode::Focus
                && !self.tabs.order.is_empty()
            {
                areas.push(Area::Tabs);
            }
            if self.preferences.layout.preview_mode != PreviewMode::Off {
                areas.push(Area::Editor);
            }
            areas.push(Area::Search);
        }
        if self.terminal_shown() {
            areas.push(Area::Terminal);
        }
        areas
    }
    fn infer_workspace_area(&mut self, ctx: &egui::Context) {
        let point = ctx.input(|i| {
            (i.pointer.primary_pressed() || i.pointer.primary_released())
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        // read_response re-evaluates clicked-elsewhere focus handling. Never
        // re-query the editor after an explicit pointer focus transfer.
        let pos = if let Some(point) = point {
            point
        } else {
            let Some(id) = ctx.memory(|m| m.focused()) else {
                return;
            };
            if id == Id::unique(("file-editor", &self.editor.path)) {
                self.keyboard.area = Area::Editor;
                return;
            }
            if id == Id::unique("terminal-view") && self.terminal_shown() {
                self.keyboard.area = Area::Terminal;
                return;
            }
            if id == Id::unique("folder-search") {
                self.keyboard.area = Area::Search;
                return;
            }
            let Some(response) = ctx.read_response(id) else {
                return;
            };
            response.rect.center()
        };
        for area in [
            Area::Terminal,
            Area::Search,
            Area::Tabs,
            Area::Categories,
            Area::Tree,
            Area::Editor,
        ] {
            if self.area_rect(ctx, area).is_some_and(|r| r.contains(pos)) {
                self.keyboard.area = area;
                if point.is_some() {
                    self.keyboard.visible = false;
                }
                break;
            }
        }
    }
    pub(in crate::app) fn focus_area(&mut self, mut area: Area, ctx: &egui::Context) {
        if !self.available_areas().contains(&area) {
            area = Area::Tree;
        }
        if area != Area::Editor {
            self.editor.focus_on_load = false;
        }
        self.keyboard.area = area;
        self.keyboard.visible = true;
        if let Some(id) = ctx.memory(|m| m.focused()) {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
        let id = match area {
            Area::Categories => {
                let id = self
                    .keyboard
                    .category
                    .clone()
                    .filter(|id| {
                        id == TRASH || self.collections.config.items.iter().any(|c| c.id == *id)
                    })
                    .or_else(|| {
                        if self.browser.view == View::Trash {
                            Some(TRASH.into())
                        } else {
                            self.collections
                                .config
                                .get(self.browser.category)
                                .map(|c| c.id.clone())
                        }
                    })
                    .or_else(|| self.collections.config.items.first().map(|c| c.id.clone()))
                    .unwrap_or_else(|| TRASH.into());
                let name = if id == TRASH {
                    crate::tr!("Кошик", "Trash")
                } else {
                    self.collections
                        .config
                        .items
                        .iter()
                        .find(|c| c.id == id)
                        .map(|c| c.title())
                        .unwrap_or("")
                };
                self.message = crate::tr!(
                    "Категорія: {name} · Enter — відкрити",
                    "Category: {name} · Enter to open",
                    name = name
                );
                self.keyboard.category = Some(id.clone());
                self.keyboard.reveal_category = true;
                if id == TRASH {
                    Id::unique("trash-shortcut")
                } else {
                    Id::unique(("category", id))
                }
            }
            Area::Tabs => {
                let path = self
                    .keyboard
                    .tab
                    .clone()
                    .filter(|p| self.tabs.order.contains(p))
                    .or(self.editor.path.clone())
                    .or_else(|| self.tabs.order.first().cloned());
                self.keyboard.tab = path.clone();
                self.keyboard.reveal_tab = true;
                Id::unique(("document-tab", path.unwrap_or_default()))
            }
            Area::Editor if self.editor.document.is_some() => {
                Id::unique(("file-editor", &self.editor.path))
            }
            Area::Editor => {
                if self.editor.path.is_some() {
                    self.editor.focus_on_load = true;
                }
                Id::unique("preview-viewport")
            }
            Area::Tree => Id::unique("tree-keyboard-focus"),
            Area::Search => Id::unique("folder-search"),
            Area::Terminal => Id::unique("terminal-view"),
        };
        ctx.memory_mut(|m| {
            m.move_focus(egui::FocusDirection::None);
            m.request_focus(id);
        });
        self.keyboard.pending_focus = Some(id);
        ctx.request_repaint();
    }
    pub(in crate::app) fn open_preferences_keyboard(&mut self, ctx: &egui::Context) {
        if !self.preferences.open {
            self.preferences.snapshot = Some((
                self.preferences.theme.clone(),
                self.preferences.layout.clone(),
            ));
            self.preferences.draft = None;
            self.preferences.error.clear();
            self.preferences.open = true;
        } else {
            ctx.send_viewport_cmd_to(
                Self::settings_viewport_id(),
                egui::ViewportCommand::Minimized(false),
            );
            ctx.send_viewport_cmd_to(Self::settings_viewport_id(), egui::ViewportCommand::Focus);
        }
        ctx.request_repaint();
    }
    pub(in crate::app) fn select_collection_keyboard(&mut self, id: &str) {
        if id == TRASH {
            self.browser.view = View::Trash;
            self.browser.properties = None;
            self.browser.filter.clear();
            return;
        }
        let Some(category) = self
            .collections
            .config
            .items
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.category())
        else {
            return;
        };
        self.browser.category = category;
        self.browser.view = View::Folders;
        self.browser.filter.clear();
        self.browser.table_root = None;
        self.browser.properties = None;
        if let Some(id) = self.scan.index.roots(category).first().copied() {
            self.expand_folder(id);
            self.select_folder(id);
        }
    }
    pub(in crate::app) fn workspace_shortcuts(&mut self, ctx: &egui::Context) {
        if (self.preferences.open && ctx.embed_viewports())
            || self.deletion.confirm
            || self.purge_targets.is_some()
            || self.editor.pending.is_some()
            || self.new_file.is_some()
            || egui::Popup::is_any_open(ctx)
        {
            return;
        }
        self.infer_workspace_area(ctx);
        if self.terminal_focused(ctx) {
            // Reserve every panel-navigation chord, including layout edges:
            // forwarding an unhandled chord sends an escape sequence to the PTY.
            for key in [
                Key::ArrowLeft,
                Key::ArrowRight,
                Key::ArrowUp,
                Key::ArrowDown,
            ] {
                if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::ALT, key)) {
                    let target = match (self.preferences.layout.preview_mode, key) {
                        (PreviewMode::Overview, Key::ArrowUp) => Some(Area::Tree),
                        (PreviewMode::Overview, Key::ArrowRight) => Some(Area::Editor),
                        (PreviewMode::Focus, Key::ArrowLeft) => Some(Area::Editor),
                        _ => None,
                    };
                    if let Some(target) = target {
                        self.focus_area(target, ctx);
                    }
                    return;
                }
            }
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Comma)) {
            self.open_preferences_keyboard(ctx);
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::L)) {
            self.focus_area(Area::Search, ctx);
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::Home)) {
            self.editor_action(
                super::editor::Action::Navigate(self.preferences.layout.home_directory()),
                ctx,
            );
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::W)) {
            if self.preferences.layout.preview_mode == PreviewMode::Focus {
                if let Some(path) = self.editor.path.clone() {
                    self.request_close_tab(&path, ctx);
                }
            }
            return;
        }
        let reverse = ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::F6));
        if reverse || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F6)) {
            let areas = self.available_areas();
            let index = areas
                .iter()
                .position(|a| *a == self.keyboard.area)
                .unwrap_or(0);
            let next = if reverse {
                (index + areas.len() - 1) % areas.len()
            } else {
                (index + 1) % areas.len()
            };
            self.focus_area(areas[next], ctx);
            return;
        }
        for key in [
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::ArrowUp,
            Key::ArrowDown,
        ] {
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::ALT, key)) {
                let target = match (self.keyboard.area, key) {
                    (Area::Categories | Area::Tree, Key::ArrowDown)
                        if self.terminal_shown()
                            && self.preferences.layout.preview_mode == PreviewMode::Overview =>
                    {
                        Area::Terminal
                    }
                    (Area::Terminal, Key::ArrowLeft) => Area::Editor,
                    (Area::Editor, Key::ArrowRight)
                        if self.terminal_shown()
                            && self.preferences.layout.preview_mode == PreviewMode::Focus =>
                    {
                        Area::Terminal
                    }
                    (Area::Editor, Key::ArrowLeft)
                        if self.terminal_shown()
                            && self.preferences.layout.preview_mode == PreviewMode::Overview =>
                    {
                        Area::Terminal
                    }
                    (Area::Editor | Area::Tabs, Key::ArrowLeft) => Area::Tree,
                    (_, Key::ArrowLeft) => Area::Categories,
                    (Area::Categories, Key::ArrowRight) => Area::Tree,
                    (_, Key::ArrowRight) => Area::Editor,
                    (Area::Editor, Key::ArrowUp) => Area::Tabs,
                    (_, Key::ArrowUp) => Area::Search,
                    (Area::Search, Key::ArrowDown) => Area::Tree,
                    (Area::Categories, Key::ArrowDown) => Area::Categories,
                    (_, Key::ArrowDown) => Area::Editor,
                    _ => Area::Tree,
                };
                self.focus_area(target, ctx);
                return;
            }
        }
        if self.keyboard.area == Area::Categories && self.preferences.layout.categories {
            let mut ids: Vec<_> = self
                .collections
                .config
                .items
                .iter()
                .map(|c| c.id.clone())
                .collect();
            ids.push(TRASH.into());
            let current = self
                .keyboard
                .category
                .as_ref()
                .and_then(|c| ids.iter().position(|id| id == c))
                .unwrap_or(0);
            for (key, delta) in [(Key::ArrowUp, -1isize), (Key::ArrowDown, 1)] {
                if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, key)) {
                    if self.preferences.layout.categories_locked {
                        self.message = crate::tr!(
                            "Порядок категорій закріплено в налаштуваннях",
                            "Category order is locked in settings"
                        )
                        .into();
                        return;
                    }
                    if ids[current] == TRASH || ids.len() < 2 {
                        return;
                    }
                    let target =
                        (current as isize + delta).clamp(0, ids.len() as isize - 2) as usize;
                    if ids[current] != TRASH && ids[target] != TRASH {
                        if self.demo {
                            if self.collections.config.reorder(
                                &ids[current],
                                &ids[target],
                                delta > 0,
                            ) {
                                self.collections.revision =
                                    self.collections.revision.wrapping_add(1);
                            }
                        } else if let Err(error) =
                            self.collections
                                .reorder(&ids[current], &ids[target], delta > 0)
                        {
                            self.message = error;
                        }
                        self.keyboard.reveal_category = true;
                    }
                    ctx.request_repaint();
                    return;
                }
                if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key)) {
                    let next = (current as isize + delta).clamp(0, ids.len() as isize - 1) as usize;
                    self.keyboard.category = Some(ids[next].clone());
                    self.focus_area(Area::Categories, ctx);
                    return;
                }
            }
            if ctx.input_mut(|i| {
                i.consume_key(Modifiers::NONE, Key::Enter)
                    || i.consume_key(Modifiers::NONE, Key::Space)
            }) {
                self.keyboard.category = Some(ids[current].clone());
                self.select_collection_keyboard(&ids[current]);
                ctx.request_repaint();
                return;
            }
        }
        if self.keyboard.area == Area::Tabs && !self.tabs.order.is_empty() {
            let current = self
                .keyboard
                .tab
                .as_ref()
                .and_then(|p| self.tabs.order.iter().position(|path| path == p))
                .or_else(|| {
                    self.editor
                        .path
                        .as_ref()
                        .and_then(|p| self.tabs.order.iter().position(|path| path == p))
                })
                .unwrap_or(0);
            for (key, delta) in [(Key::ArrowLeft, -1isize), (Key::ArrowRight, 1)] {
                if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key)) {
                    let next = (current as isize + delta)
                        .clamp(0, self.tabs.order.len() as isize - 1)
                        as usize;
                    self.keyboard.tab = Some(self.tabs.order[next].clone());
                    self.focus_area(Area::Tabs, ctx);
                    return;
                }
            }
            if ctx.input_mut(|i| {
                i.consume_key(Modifiers::NONE, Key::Enter)
                    || i.consume_key(Modifiers::NONE, Key::Space)
            }) {
                let path = self.tabs.order[current].clone();
                self.activate_tab(&path, ctx);
                self.focus_area(Area::Editor, ctx);
                return;
            }
        }
    }
    pub(in crate::app) fn workspace_focus_ui(&mut self, ui: &mut egui::Ui) {
        if self.preferences.open && ui.ctx().embed_viewports() {
            return;
        }
        // Non-interactive panels cannot retain keyboard focus in egui unless
        // screen-reader mode is enabled. A tiny focus anchor in their margin
        // keeps tree navigation stable even while a preview is loading.
        if let Some(rect) = self.area_rect(ui.ctx(), Area::Tree) {
            ui.interact(
                Rect::from_min_size(rect.min + egui::vec2(2., 2.), egui::vec2(1., 1.)),
                Id::unique("tree-keyboard-focus"),
                egui::Sense::click(),
            );
        }
        if let Some(id) = self.keyboard.pending_focus.take() {
            ui.ctx().memory_mut(|m| {
                m.move_focus(egui::FocusDirection::None);
                m.request_focus(id);
            });
        }
        self.infer_workspace_area(ui.ctx());
        if self.keyboard.visible && self.editor.pending.is_none() {
            if let Some(rect) = self.area_rect(ui.ctx(), self.keyboard.area) {
                ui.painter().rect_stroke(
                    rect.shrink(1.),
                    self.preferences.theme.radius,
                    Stroke::new(1., color(&self.preferences.theme.accent)),
                    egui::StrokeKind::Inside,
                );
            }
        }
        let id = Id::unique("tree-keyboard-focus");
        if ui.ctx().memory(|m| m.has_focus(id)) {
            ui.ctx().memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
        }
    }
}
