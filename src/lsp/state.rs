//! UI-side generations and small overlays. The client owns all server IO.
use super::{
    client::{Client, Doc, Event, Info, Kind, Sync},
    manager::Manager,
    protocol::{self, Completion, Diagnostic},
};
use eframe::egui::{
    self,
    text::{CCursor, CCursorRange},
    text_edit::TextEditOutput,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
pub struct State {
    pub manager: Manager,
    pub client: Option<Client>,
    pub sessions: Vec<Info>,
    pub notice: String,
    path: Option<PathBuf>,
    text: Arc<str>,
    version: u64,
    revision: u64,
    open: Vec<PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
    items: Vec<Completion>,
    selected: usize,
    completion: u64,
    hover: u64,
    hover_text: String,
    hover_at: Option<(usize, Instant)>,
    hover_index: Option<usize>,
    due: Option<Instant>,
    cursor: usize,
    anchor: Option<egui::Pos2>,
    accept: bool,
    pending_cursor: Option<usize>,
    pub compile_config: bool,
}
impl State {
    pub fn new(config: PathBuf) -> Self {
        Self {
            manager: Manager::load(config),
            client: None,
            sessions: vec![],
            notice: String::new(),
            path: None,
            text: Arc::from(""),
            version: 0,
            revision: u64::MAX,
            open: vec![],
            diagnostics: vec![],
            items: vec![],
            selected: 0,
            completion: 0,
            hover: 0,
            hover_text: String::new(),
            hover_at: None,
            hover_index: None,
            due: None,
            cursor: 0,
            anchor: None,
            accept: false,
            pending_cursor: None,
            compile_config: true,
        }
    }
    pub fn tick(
        &mut self,
        ctx: &egui::Context,
        path: Option<&Path>,
        text: Option<&str>,
        mut open: Vec<PathBuf>,
    ) {
        if let Some((id, restart)) = self.manager.poll() {
            if restart && let Some(client) = &self.client {
                client.restart(&id);
            }
        }
        let launches = self.manager.launches();
        let usable = path.is_some_and(|p| {
            p.is_absolute()
                && launches.iter().any(|s| {
                    p.extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| s.extensions.iter().any(|x| x == e))
                })
        });
        let path = path.filter(|_| usable);
        let text = text.unwrap_or_default();
        open.sort();
        open.dedup();
        let path_changed = self.path.as_deref() != path;
        if path_changed {
            self.due = None;
        }
        let changed = path_changed || self.text.as_ref() != text;
        let sync = changed || self.revision != self.manager.revision || self.open != open;
        if changed {
            self.version += 1;
            self.path = path.map(Path::to_path_buf);
            self.text = Arc::from(text);
            self.items.clear();
            self.diagnostics.clear();
            self.hover_text.clear();
            self.hover_index = None;
            self.hover_at = None;
            self.completion = 0;
            self.hover = 0;
        }
        if self.client.is_none() && usable {
            self.client = Some(Client::new(ctx));
        }
        if sync {
            self.open = open;
            self.revision = self.manager.revision;
            if let Some(client) = &self.client {
                client.sync(Sync {
                    launches,
                    active: self.path.as_ref().map(|p| Doc {
                        path: p.clone(),
                        text: self.text.clone(),
                        version: self.version,
                    }),
                    open: self.open.iter().cloned().collect(),
                });
            }
        }
        if let Some(client) = &self.client {
            for e in client.poll() {
                match e {
                    Event::Completion {
                        path,
                        version,
                        request,
                        items,
                    } if self.path.as_ref() == Some(&path)
                        && version == self.version
                        && request == self.completion =>
                    {
                        if items.is_empty() {
                            self.completion = 0;
                        }
                        self.items = items;
                        self.selected = 0;
                    }
                    Event::Hover {
                        path,
                        version,
                        request,
                        text,
                    } if self.path.as_ref() == Some(&path)
                        && version == self.version
                        && request == self.hover =>
                    {
                        self.hover_text = text
                    }
                    Event::Diagnostics {
                        path,
                        version,
                        items,
                    } if self.path.as_ref() == Some(&path) && version == self.version => {
                        self.diagnostics = items
                    }
                    Event::Sessions(s) => {
                        self.compile_config = s
                            .iter()
                            .find(|s| self.path.as_ref().is_some_and(|p| p.starts_with(&s.root)))
                            .is_none_or(|s| s.compile_config);
                        self.sessions = s;
                    }
                    Event::Error { server, message } => {
                        self.notice = format!("{server}: {message}")
                    }
                    _ => {}
                }
            }
        }
        if let Some(at) = self.due {
            if at <= Instant::now() {
                self.due = None;
                self.request(Kind::Completion, self.cursor);
            } else {
                ctx.request_repaint_after(at.saturating_duration_since(Instant::now()));
            }
        }
    }
    fn request(&mut self, kind: Kind, index: usize) {
        if let (Some(c), Some(p)) = (&mut self.client, &self.path) {
            let id = c.request(p.clone(), self.version, index, kind);
            match kind {
                Kind::Completion => self.completion = id,
                Kind::Hover => self.hover = id,
            }
        }
    }
    pub fn before(&mut self, ctx: &egui::Context, id: egui::Id, text: &str, read_only: bool) {
        if read_only {
            self.items.clear();
        }
        if !ctx.memory(|m| m.has_focus(id)) || self.path.is_none() {
            return;
        }
        if ctx.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::Text(_)
                        | egui::Event::Paste(_)
                        | egui::Event::Cut
                        | egui::Event::Ime(_)
                        | egui::Event::Key {
                            key: egui::Key::Backspace | egui::Key::Delete,
                            pressed: true,
                            ..
                        }
                )
            })
        }) {
            self.items.clear();
            self.accept = false;
        }
        if (!self.items.is_empty() || self.completion != 0 || self.due.is_some())
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.items.clear();
            self.completion = 0;
            self.due = None;
            self.accept = false;
        }
        let state = egui::text_edit::TextEditState::load(ctx, id);
        let index = state
            .as_ref()
            .and_then(|s| s.cursor.char_range())
            .map(|r| usize::from(r.primary.index))
            .unwrap_or(0);
        let manual = ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Space));
        if manual && !read_only {
            self.cursor = index;
            self.request(Kind::Completion, index);
        }
        if !self.items.is_empty() {
            let mut move_by = 0;
            ctx.input_mut(|i| {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    move_by = 1;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    move_by = -1;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    || i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
                {
                    self.accept = true;
                }
            });
            if !self.items.is_empty() {
                self.selected = (self.selected as isize + move_by)
                    .rem_euclid(self.items.len() as isize) as usize;
            }
        }
        if self.accept {
            self.accept = false;
            if !read_only && let Some(item) = self.items.get(self.selected) {
                match protocol::apply_completion(text, index, item) {
                    Ok((new, cursor)) => {
                        if let Some(mut state) = state {
                            state.cursor.set_char_range(Some(CCursorRange::two(
                                CCursor::new(0),
                                CCursor::new(text.chars().count()),
                            )));
                            state.store(ctx, id);
                            ctx.input_mut(|i| i.events.insert(0, egui::Event::Paste(new)));
                            self.pending_cursor = Some(cursor);
                        }
                    }
                    Err(e) => self.notice = e,
                }
            }
            self.items.clear();
        }
    }
    pub fn after(&mut self, ui: &egui::Ui, out: &mut TextEditOutput, text: &str, read_only: bool) {
        if let Some(index) = self.pending_cursor.take() {
            out.state
                .cursor
                .set_char_range(Some(CCursorRange::one(CCursor::new(index))));
            out.state.clone().store(ui.ctx(), out.response.id);
        }
        let index = out
            .state
            .cursor
            .char_range()
            .map(|r| usize::from(r.primary.index))
            .unwrap_or(0);
        if index != self.cursor {
            self.completion = 0;
            self.due = None;
            self.items.clear();
            self.cursor = index;
        }
        let typed = ui.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, egui::Event::Text(s) | egui::Event::Ime(egui::ImeEvent::Commit(s)) if !s.is_empty()))
        });
        if out.response.changed() && out.response.has_focus() && !read_only && typed {
            let prefix: String = text.chars().take(index).collect();
            let identifier = prefix
                .chars()
                .rev()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .count();
            if identifier >= 2
                || prefix.ends_with('.')
                || prefix.ends_with("::")
                || prefix.ends_with("->")
            {
                self.due = Some(Instant::now() + Duration::from_millis(180));
                ui.ctx().request_repaint_after(Duration::from_millis(180));
            }
        }
        self.anchor = Some(
            out.galley_pos
                + out
                    .galley
                    .pos_from_cursor(CCursor::new(index))
                    .max
                    .to_vec2(),
        );
        if self.path.is_none() {
            return;
        }
        if let Some(pointer) = ui
            .ctx()
            .pointer_hover_pos()
            .filter(|p| out.response.rect.contains(*p) && ui.clip_rect().contains(*p))
        {
            let cursor = usize::from(out.galley.cursor_from_pos(pointer - out.galley_pos).index);
            if self.hover_at.is_none_or(|(i, _)| i != cursor) {
                self.hover_at = Some((cursor, Instant::now()));
                self.hover_text.clear();
                self.hover = 0;
                self.hover_index = None;
            }
            if self
                .hover_at
                .is_some_and(|(_, at)| at.elapsed() >= Duration::from_millis(450))
                && self.hover_index != Some(cursor)
            {
                self.hover_index = Some(cursor);
                self.request(Kind::Hover, cursor);
            } else {
                ui.ctx().request_repaint_after(Duration::from_millis(450));
            }
            let diagnostic = self.diagnostics.iter().find(|d| {
                let p = protocol::position(text, cursor);
                (p.line, p.character) >= (d.range.start.line, d.range.start.character)
                    && (p.line, p.character) <= (d.range.end.line, d.range.end.character)
            });
            let message = diagnostic
                .map(|d| d.message.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(&self.hover_text);
            if !message.is_empty() {
                egui::Response::clone(&out.response).on_hover_ui(|ui| {
                    ui.set_max_width(520.);
                    ui.label(message);
                });
            }
        } else {
            self.hover_at = None;
            self.hover = 0;
            self.hover_text.clear();
            self.hover_index = None;
        }
        for d in self.diagnostics.iter().take(200) {
            let Some(start) = protocol::byte_offset(text, d.range.start) else {
                continue;
            };
            let n = text[..start].chars().count();
            let r = out
                .galley
                .pos_from_cursor(CCursor::new(n))
                .translate(out.galley_pos.to_vec2());
            if !ui.clip_rect().intersects(r) {
                continue;
            }
            let color = if d.severity == 1 {
                egui::Color32::from_rgb(235, 95, 90)
            } else {
                egui::Color32::from_rgb(220, 175, 65)
            };
            ui.painter()
                .circle_filled(egui::pos2(out.galley_pos.x - 6., r.center().y), 2.5, color);
            ui.painter().line_segment(
                [r.left_bottom(), r.left_bottom() + egui::vec2(12., 0.)],
                egui::Stroke::new(1., color),
            );
        }
    }
    pub fn overlay(&mut self, ctx: &egui::Context) {
        // egui handles Escape focus navigation before the next UI pass.
        // Reserve it while completion is visible/pending so it dismisses the
        // popup before any panel navigation can surrender editor focus.
        if (!self.items.is_empty() || self.completion != 0 || self.due.is_some())
            && let Some(path) = &self.path
        {
            let id = egui::Id::unique(("file-editor", &Some(path.clone())));
            if ctx.memory(|m| m.has_focus(id)) {
                ctx.memory_mut(|m| {
                    m.set_focus_lock_filter(
                        id,
                        egui::EventFilter {
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            tab: true,
                            escape: true,
                            ..Default::default()
                        },
                    )
                });
            }
        }
        if self.items.is_empty() {
            return;
        }
        if let Some(path) = &self.path {
            if !ctx.memory(|m| m.has_focus(egui::Id::unique(("file-editor", &Some(path.clone())))))
            {
                return;
            }
        }
        let Some(at) = self.anchor else { return };
        egui::Area::new(egui::Id::unique("lsp-completion"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ctx, |ui| {
                let popup = egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(380.);
                    egui::ScrollArea::vertical()
                        .max_height(220.)
                        .show(ui, |ui| {
                            for (n, item) in self.items.iter().enumerate() {
                                let r = ui
                                    .selectable_label(n == self.selected, &item.label)
                                    .on_hover_text(&item.detail);
                                if n == self.selected {
                                    r.scroll_to_me(Some(egui::Align::Center));
                                }
                                if r.clicked() {
                                    self.selected = n;
                                    self.accept = true;
                                    if let Some(path) = &self.path {
                                        ctx.memory_mut(|m| {
                                            m.request_focus(egui::Id::unique((
                                                "file-editor",
                                                &Some(path.clone()),
                                            )))
                                        });
                                    }
                                }
                            }
                        });
                });
                ui.interact(
                    popup.response.rect,
                    egui::Id::unique("lsp-completion-bounds"),
                    egui::Sense::hover(),
                );
            });
    }
    #[cfg(test)]
    pub fn completion_count(&self) -> usize {
        self.items.len()
    }
    pub fn shutdown(&mut self) {
        self.manager.shutdown();
        if let Some(mut c) = self.client.take() {
            c.shutdown();
        }
    }
}
