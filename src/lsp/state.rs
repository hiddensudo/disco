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
    revealed_selection: Option<usize>,
    completion: u64,
    signature: Option<protocol::SignatureHelp>,
    signature_selected: usize,
    signature_request: u64,
    signature_due: Option<Instant>,
    docs_request: u64,
    docs_due: Option<Instant>,
    hover: u64,
    hover_text: String,
    hover_at: Option<(usize, Instant)>,
    hover_index: Option<usize>,
    due: Option<Instant>,
    cursor: usize,
    anchor: Option<egui::Pos2>,
    completion_rect: Option<egui::Rect>,
    signature_rect: Option<egui::Rect>,
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
            revealed_selection: None,
            completion: 0,
            signature: None,
            signature_selected: 0,
            signature_request: 0,
            signature_due: None,
            docs_request: 0,
            docs_due: None,
            hover: 0,
            hover_text: String::new(),
            hover_at: None,
            hover_index: None,
            due: None,
            cursor: 0,
            anchor: None,
            completion_rect: None,
            signature_rect: None,
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
            self.signature = None;
            self.signature_due = None;
            self.due = None;
        }
        let changed = path_changed || self.text.as_ref() != text;
        let sync = changed || self.revision != self.manager.revision || self.open != open;
        if changed {
            super::trace(format_args!(
                "sync next_version={} chars={} path_changed={path_changed}",
                self.version + 1,
                text.chars().count()
            ));
            self.version += 1;
            self.path = path.map(Path::to_path_buf);
            self.text = Arc::from(text);
            self.items.clear();
            self.diagnostics.clear();
            self.hover_text.clear();
            self.hover_index = None;
            self.hover_at = None;
            self.completion = 0;
            self.signature_request = 0;
            self.docs_request = 0;
            self.docs_due = None;
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
                if let Event::Completion {
                    version,
                    request,
                    items,
                    path,
                } = &e
                {
                    super::trace(format_args!(
                        "completion response version={version}/{} request={request}/{} items={} same_path={}",
                        self.version,
                        self.completion,
                        items.len(),
                        self.path.as_ref() == Some(path)
                    ));
                }
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
                        self.revealed_selection = None;
                        self.items = items;
                        self.selected = 0;
                        self.docs_due = Some(Instant::now() + Duration::from_millis(120));
                        ctx.request_repaint_after(Duration::from_millis(120));
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
                    Event::Signature {
                        path,
                        version,
                        request,
                        help,
                    } if self.path.as_ref() == Some(&path)
                        && version == self.version
                        && request == self.signature_request =>
                    {
                        self.signature_selected = help
                            .active_signature
                            .min(help.signatures.len().saturating_sub(1));
                        self.signature = (!help.signatures.is_empty()).then_some(help);
                    }
                    Event::Resolved {
                        path,
                        version,
                        request,
                        item,
                    } if self.path.as_ref() == Some(&path)
                        && version == self.version
                        && request == self.docs_request =>
                    {
                        self.docs_request = 0;
                        if let Some(selected) = self.items.get_mut(self.selected) {
                            if item.get("documentation").is_some() {
                                selected.raw["documentation"] = item["documentation"].clone();
                            }
                            if let Some(detail) = item["detail"].as_str() {
                                selected.detail = detail.chars().take(512).collect();
                            }
                        }
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
        if let Some(at) = self.signature_due {
            if at <= Instant::now() {
                self.signature_due = None;
                self.request(Kind::Signature, self.cursor);
            } else {
                ctx.request_repaint_after(at.saturating_duration_since(Instant::now()));
            }
        }
        if let Some(at) = self.docs_due {
            if at <= Instant::now() {
                self.docs_due = None;
                if let (Some(client), Some(path), Some(item)) =
                    (&mut self.client, &self.path, self.items.get(self.selected))
                {
                    if protocol::documentation(&item.raw["documentation"]).is_empty() {
                        self.docs_request =
                            client.resolve(path.clone(), self.version, item.raw.clone());
                    }
                }
            } else {
                ctx.request_repaint_after(at.saturating_duration_since(Instant::now()));
            }
        }
    }
    fn request(&mut self, kind: Kind, index: usize) {
        if let (Some(c), Some(p)) = (&mut self.client, &self.path) {
            let id = c.request(p.clone(), self.version, index, kind);
            super::trace(format_args!(
                "request {kind:?} id={id} version={} index={index}",
                self.version
            ));
            match kind {
                Kind::Completion => self.completion = id,
                Kind::Hover => self.hover = id,
                Kind::Signature => self.signature_request = id,
                Kind::Resolve => {}
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
            i.events.iter().any(|e| match e {
                egui::Event::Text(s) | egui::Event::Paste(s) => !s.is_empty(),
                // Wayland may emit empty preedit/IME lifecycle events without
                // changing the buffer. They must not erase a just-arrived list.
                egui::Event::Ime(
                    egui::ImeEvent::Commit(s) | egui::ImeEvent::Preedit { text: s, .. },
                ) => !s.is_empty(),
                egui::Event::Cut
                | egui::Event::Key {
                    key: egui::Key::Backspace | egui::Key::Delete,
                    pressed: true,
                    ..
                } => true,
                _ => false,
            })
        }) {
            self.items.clear();
            self.accept = false;
        }
        if (!self.items.is_empty()
            || self.completion != 0
            || self.due.is_some()
            || self.signature.is_some()
            || self.signature_due.is_some())
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.items.clear();
            self.completion = 0;
            self.due = None;
            self.accept = false;
            self.signature = None;
            self.signature_request = 0;
            self.signature_due = None;
            self.docs_request = 0;
            self.docs_due = None;
        }
        let state = egui::text_edit::TextEditState::load(ctx, id);
        let index = state
            .as_ref()
            .and_then(|s| s.cursor.char_range())
            .map(|r| usize::from(r.primary.index))
            .unwrap_or(0);
        let manual_signature = ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
                egui::Key::Space,
            )
        });
        let manual = !manual_signature
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Space));
        if manual_signature {
            self.cursor = index;
            self.request(Kind::Signature, index);
        }
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
            if move_by != 0 {
                self.docs_request = 0;
                self.docs_due = Some(Instant::now() + Duration::from_millis(120));
                ctx.request_repaint_after(Duration::from_millis(120));
            }
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
        if out.response.changed() && text != self.text.as_ref() {
            self.items.clear();
            self.completion = 0;
            self.due = None;
        }
        if (out.response.changed() && text != self.text.as_ref() || index != self.cursor)
            && self.signature.is_some()
        {
            self.signature_due = Some(Instant::now() + Duration::from_millis(60));
            ui.ctx().request_repaint_after(Duration::from_millis(60));
        }
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
        if out.response.changed() {
            super::trace(format_args!(
                "edit index={index} changed=true focused={} readonly={read_only} typed={typed} text_event={} ime_commit={} ime_preedit={}",
                out.response.has_focus(),
                ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_)))),
                ui.input(|i| i
                    .events
                    .iter()
                    .any(|e| matches!(e, egui::Event::Ime(egui::ImeEvent::Commit(_))))),
                ui.input(|i| i
                    .events
                    .iter()
                    .any(|e| matches!(e, egui::Event::Ime(egui::ImeEvent::Preedit { .. }))))
            ));
        }
        if out.response.changed() && out.response.has_focus() && !read_only && typed {
            let prefix: String = text.chars().take(index).collect();
            let identifier = prefix
                .chars()
                .rev()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .count();
            if prefix.ends_with('(') || prefix.ends_with(',') {
                self.signature_due = Some(Instant::now() + Duration::from_millis(60));
                ui.ctx().request_repaint_after(Duration::from_millis(60));
            }
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
        if (!self.items.is_empty()
            || self.completion != 0
            || self.due.is_some()
            || self.signature.is_some()
            || self.signature_due.is_some())
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
            self.revealed_selection = None;
            self.signature_overlay(ctx);
            return;
        }
        super::trace(format_args!(
            "overlay items={} focus={:?} anchor={:?}",
            self.items.len(),
            ctx.memory(|m| m.focused()),
            self.anchor
        ));
        if let Some(path) = &self.path {
            let editor = egui::Id::unique(("file-editor", &Some(path.clone())));
            if !ctx.memory(|m| m.has_focus(editor)) {
                let clicking = self.completion_rect.is_some_and(|r| {
                    ctx.input(|i| {
                        i.pointer.interact_pos().is_some_and(|p| r.contains(p))
                            && (i.pointer.any_down()
                                || i.pointer.any_pressed()
                                || i.pointer.any_released())
                    })
                });
                if clicking {
                    ctx.memory_mut(|m| m.request_focus(editor));
                } else {
                    return;
                }
            }
        }
        let Some(at) = self.anchor else { return };
        egui::Area::new(egui::Id::unique("lsp-completion"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ctx, |ui| {
                let popup = egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(420.);
                    let selected_fill = ui.visuals().selection.bg_fill.gamma_multiply(0.35);
                    ui.visuals_mut().selection.bg_fill = selected_fill;
                    ui.spacing_mut().item_spacing.y = 0.;
                    ui.spacing_mut().button_padding = egui::vec2(6., 2.);
                    ui.spacing_mut().interact_size.y = 22.;
                    egui::ScrollArea::vertical()
                        .id_salt("completion-list")
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .animated(false)
                        .max_height(198.)
                        .show(ui, |ui| {
                            for (n, item) in self.items.iter().enumerate() {
                                let r = ui.add(
                                    egui::Button::selectable(
                                        n == self.selected,
                                        egui::RichText::new(item.label.trim_start_matches('•'))
                                            .monospace()
                                            .size(12.),
                                    )
                                    .truncate()
                                    .min_size(egui::vec2(ui.available_width(), 24.))
                                    .corner_radius(2),
                                );
                                if r.hovered()
                                    && self.selected != n
                                    && ui.input(|i| {
                                        i.pointer.delta() != egui::Vec2::ZERO
                                            || i.pointer.any_pressed()
                                    })
                                {
                                    self.selected = n;
                                    self.docs_request = 0;
                                    self.docs_due =
                                        Some(Instant::now() + Duration::from_millis(120));
                                    ctx.request_repaint_after(Duration::from_millis(120));
                                }
                                // Reveal only a new keyboard selection. Re-centering every frame
                                // fights wheel scrolling and makes clipped boundary rows oscillate.
                                if n == self.selected && self.revealed_selection != Some(n) {
                                    r.scroll_to_me(None);
                                    self.revealed_selection = Some(n);
                                }
                                ui.interact(
                                    r.rect,
                                    egui::Id::unique(("lsp-completion-row", n)),
                                    egui::Sense::hover(),
                                );
                                if r.is_pointer_button_down_on() {
                                    if let Some(path) = &self.path {
                                        ctx.memory_mut(|m| {
                                            m.request_focus(egui::Id::unique((
                                                "file-editor",
                                                &Some(path.clone()),
                                            )))
                                        });
                                    }
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
                    if let Some(item) = self.items.get(self.selected) {
                        let description = protocol::documentation(&item.raw["documentation"]);
                        if !item.detail.is_empty() || !description.is_empty() {
                            ui.separator();
                            egui::ScrollArea::vertical()
                                .id_salt("completion-documentation")
                                .auto_shrink([false, false])
                                .max_height(128.)
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 5.;
                                    if let Some(count) = overload_count(&item.detail) {
                                        ui.label(
                                            egui::RichText::new(crate::tr!(
                                                "{count} варіантів виклику",
                                                "{count} overloads",
                                                count = count
                                            ))
                                            .small()
                                            .weak(),
                                        );
                                    } else if !item.detail.is_empty() {
                                        ui.label(
                                            egui::RichText::new(&item.detail)
                                                .monospace()
                                                .small()
                                                .weak(),
                                        );
                                    }
                                    render_documentation(ui, &description);
                                });
                        }
                    }
                });
                self.completion_rect = Some(popup.response.rect);
                ui.interact(
                    popup.response.rect,
                    egui::Id::unique("lsp-completion-bounds"),
                    egui::Sense::hover(),
                );
            });
    }
    pub fn signature_shortcuts(&mut self, ctx: &egui::Context) {
        if self.signature.is_none() || !self.items.is_empty() {
            return;
        }
        let Some(path) = &self.path else { return };
        let editor = egui::Id::unique(("file-editor", &Some(path.clone())));
        if !ctx.memory(|m| m.has_focus(editor)) {
            return;
        }
        let previous = ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft));
        let next = ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowRight));
        if previous || next {
            self.cycle_signature(if previous { -1 } else { 1 });
            ctx.request_repaint();
        }
    }
    fn cycle_signature(&mut self, step: isize) {
        if let Some(help) = &self.signature {
            let n = help.signatures.len();
            if n > 0 {
                self.signature_selected =
                    (self.signature_selected as isize + step).rem_euclid(n as isize) as usize;
            }
        }
    }
    fn signature_overlay(&mut self, ctx: &egui::Context) {
        let (Some(help), Some(at), Some(path)) = (&self.signature, self.anchor, &self.path) else {
            return;
        };
        let editor = egui::Id::unique(("file-editor", &Some(path.clone())));
        if !ctx.memory(|m| m.has_focus(editor)) {
            let clicking = self.signature_rect.is_some_and(|r| {
                ctx.input(|i| {
                    i.pointer.interact_pos().is_some_and(|p| r.contains(p))
                        && (i.pointer.any_down()
                            || i.pointer.any_pressed()
                            || i.pointer.any_released())
                })
            });
            if clicking {
                ctx.memory_mut(|m| m.request_focus(editor));
            } else {
                return;
            }
        }
        let Some(signature) = help.signatures.get(self.signature_selected) else {
            return;
        };
        let active = signature
            .active_parameter
            .or(help.active_parameter)
            .and_then(|n| signature.parameters.get(n));
        let mut step = 0;
        egui::Area::new(egui::Id::unique("lsp-signature"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ctx, |ui| {
                let frame = egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(560.);
                    ui.spacing_mut().button_padding = egui::vec2(5., 2.);
                    ui.spacing_mut().interact_size.y = 22.;
                    ui.horizontal(|ui| {
                        let previous = ui
                            .add_enabled(help.signatures.len() > 1, egui::Button::new("‹"))
                            .on_hover_text(crate::tr!(
                                "Попереднє перевантаження · Alt+←",
                                "Previous overload · Alt+←"
                            ));
                        ui.interact(
                            previous.rect,
                            egui::Id::unique("lsp-signature-previous"),
                            egui::Sense::hover(),
                        );
                        if previous.is_pointer_button_down_on() {
                            ctx.memory_mut(|m| m.request_focus(editor));
                        }
                        if previous.clicked() {
                            step = -1;
                        }
                        ui.small(format!(
                            "{} / {}",
                            self.signature_selected + 1,
                            help.signatures.len()
                        ));
                        let next = ui
                            .add_enabled(help.signatures.len() > 1, egui::Button::new("›"))
                            .on_hover_text(crate::tr!(
                                "Наступне перевантаження · Alt+→",
                                "Next overload · Alt+→"
                            ));
                        ui.interact(
                            next.rect,
                            egui::Id::unique("lsp-signature-next"),
                            egui::Sense::hover(),
                        );
                        if next.is_pointer_button_down_on() {
                            ctx.memory_mut(|m| m.request_focus(editor));
                        }
                        if next.clicked() {
                            step = 1;
                        }
                    });
                    let font = egui::FontId::monospace(12.);
                    let normal = egui::TextFormat::simple(font.clone(), ui.visuals().text_color());
                    let mut job = egui::text::LayoutJob::default();
                    if let Some(range) = active.and_then(|p| p.range.clone()) {
                        job.append(&signature.label[..range.start], 0., normal.clone());
                        let mut selected = normal.clone();
                        selected.color = ui.visuals().selection.stroke.color;
                        selected.underline = egui::Stroke::new(1., selected.color);
                        job.append(&signature.label[range.clone()], 0., selected);
                        job.append(&signature.label[range.end..], 0., normal);
                    } else {
                        job.append(&signature.label, 0., normal);
                    }
                    ui.label(job);
                    egui::ScrollArea::vertical()
                        .id_salt("signature-docs")
                        .max_height(160.)
                        .show(ui, |ui| {
                            if !signature.documentation.is_empty() {
                                render_documentation(ui, &signature.documentation);
                            }
                            if let Some(p) = active {
                                if !p.documentation.is_empty() {
                                    ui.separator();
                                    render_documentation(ui, &p.documentation);
                                }
                            }
                        });
                });
                self.signature_rect = Some(frame.response.rect);
                ui.interact(
                    frame.response.rect,
                    egui::Id::unique("lsp-signature-bounds"),
                    egui::Sense::hover(),
                );
            });
        if step != 0 {
            self.cycle_signature(step);
            ctx.memory_mut(|m| m.request_focus(editor));
            ctx.request_repaint();
        }
    }
    #[cfg(test)]
    pub fn completion_count(&self) -> usize {
        self.items.len()
    }
    #[cfg(test)]
    pub fn signature_summary(&self) -> (usize, usize, Option<usize>) {
        self.signature
            .as_ref()
            .map(|h| {
                (
                    h.signatures.len(),
                    self.signature_selected,
                    h.signatures
                        .get(self.signature_selected)
                        .and_then(|s| s.active_parameter)
                        .or(h.active_parameter),
                )
            })
            .unwrap_or_default()
    }
    #[cfg(test)]
    pub fn completion_documentation(&self) -> String {
        self.items
            .get(self.selected)
            .map(|i| protocol::documentation(&i.raw["documentation"]))
            .unwrap_or_default()
    }

    pub fn shutdown(&mut self) {
        self.manager.shutdown();
        if let Some(mut c) = self.client.take() {
            c.shutdown();
        }
    }
}

// clangd's bundled-completion detail is metadata, not a function signature.
fn overload_count(detail: &str) -> Option<usize> {
    detail
        .strip_prefix('[')?
        .strip_suffix(" overloads]")?
        .parse()
        .ok()
}

/// Present common Doxygen commands without exposing markup in plain-text LSP docs.
/// Unknown commands and ordinary prose are preserved.
fn documentation_lines(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(|line| {
            let line = line.trim();
            let command = line.strip_prefix('@').or_else(|| line.strip_prefix('\\'));
            if let Some(command) = command {
                if let Some(rest) = command.strip_prefix("brief ") {
                    return (String::new(), clean_doc_inline(rest.trim()));
                }
                if let Some(rest) = command.strip_prefix("param ") {
                    let rest = rest.trim();
                    let rest = if rest.starts_with('[') {
                        rest.split_once(']')
                            .map_or(rest, |(_, rest)| rest.trim_start())
                    } else {
                        rest
                    };
                    let (name, description) =
                        rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                    return (name.to_owned(), clean_doc_inline(description.trim()));
                }
                if let Some(rest) = command
                    .strip_prefix("throw ")
                    .or_else(|| command.strip_prefix("throws "))
                    .or_else(|| command.strip_prefix("exception "))
                {
                    return (
                        crate::tr!("Винятки", "Throws").to_owned(),
                        clean_doc_inline(rest.trim()),
                    );
                }
                if let Some(rest) = command
                    .strip_prefix("return ")
                    .or_else(|| command.strip_prefix("returns "))
                {
                    return (
                        crate::tr!("Результат", "Returns").to_owned(),
                        clean_doc_inline(rest.trim()),
                    );
                }
            }
            (String::new(), clean_doc_inline(line))
        })
        .collect()
}

fn clean_doc_inline(text: &str) -> String {
    text.split_inclusive(char::is_whitespace)
        .filter_map(|word| {
            let token = word.trim_end();
            if matches!(
                token,
                "@a" | "@c" | "@p" | "@ref" | "\\a" | "\\c" | "\\p" | "\\ref"
            ) {
                None
            } else if word
                .strip_prefix('%')
                .is_some_and(|tail| tail.starts_with(char::is_alphabetic))
            {
                Some(&word[1..])
            } else {
                Some(word)
            }
        })
        .collect()
}

fn render_documentation(ui: &mut egui::Ui, text: &str) {
    for (name, description) in documentation_lines(text) {
        if name.is_empty() {
            if description.is_empty() {
                ui.add_space(3.);
            } else {
                ui.label(egui::RichText::new(description).size(12.));
            }
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(name).monospace().size(12.).weak());
                ui.label(egui::RichText::new(description).size(12.));
            });
        }
    }
}

#[cfg(test)]
mod presentation_tests {
    use super::*;

    #[test]
    fn doxygen_is_readable_and_unrecognized_prose_is_preserved() {
        crate::i18n::with_language(crate::i18n::Language::En, || {
            assert_eq!(
                documentation_lines(
                    "@brief Replace characters.\n@param __pos Index to start.\n@param [in,out] value Input value.\n\\return New text.\n@note Keep this.\nnormal prose"
                ),
                vec![
                    ("".into(), "Replace characters.".into()),
                    ("__pos".into(), "Index to start.".into()),
                    ("value".into(), "Input value.".into()),
                    ("Returns".into(), "New text.".into()),
                    ("".into(), "@note Keep this.".into()),
                    ("".into(), "normal prose".into()),
                ]
            );
        });
        crate::i18n::with_language(crate::i18n::Language::Uk, || {
            assert_eq!(
                documentation_lines("@return value")[0].0,
                "\u{420}\u{435}\u{437}\u{443}\u{43b}\u{44c}\u{442}\u{430}\u{442}"
            );
        });
        assert_eq!(
            clean_doc_inline("Index @a pos in %string; @c size() at 50%."),
            "Index pos in string; size() at 50%."
        );
        assert_eq!(overload_count("[18 overloads]"), Some(18));
        assert_eq!(overload_count("int replace()"), None);
    }

    #[test]
    fn last_completion_stays_still_and_wheel_scroll_is_not_undone() {
        let ctx = egui::Context::default();
        let mut state = State::new(std::env::temp_dir().join("disco-popup-layout-unused"));
        state.anchor = Some(egui::pos2(100., 100.));
        state.items = (0..30).map(|n| Completion {
            label: format!("function_{n}(…)"), detail: "[18 overloads]".into(),
            raw: serde_json::json!({"documentation":"@brief Replace characters.\n@param __pos Index of first character."}),
        }).collect();
        state.selected = 29;
        let mut time = 0.;
        let mut frame = |state: &mut State, events| {
            time += 0.016;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 800.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| state.overlay(ui.ctx()),
            );
            output.drop_without_applying_deltas();
            (
                state.completion_rect.unwrap(),
                ctx.read_response(egui::Id::unique(("lsp-completion-row", 29usize)))
                    .unwrap()
                    .rect,
            )
        };
        for _ in 0..5 {
            frame(&mut state, vec![]);
        }
        let stable = frame(&mut state, vec![]);
        assert!(
            stable.0.contains_rect(stable.1),
            "last item should be fully visible: {stable:?}"
        );
        for _ in 0..60 {
            assert_eq!(frame(&mut state, vec![]), stable);
        }
        frame(
            &mut state,
            vec![egui::Event::PointerMoved(egui::pos2(200., 110.))],
        );
        let selected = state.selected;
        frame(
            &mut state,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
                delta: egui::vec2(0., 96.),
                modifiers: egui::Modifiers::NONE,
            }],
        );
        for _ in 0..60 {
            frame(&mut state, vec![]);
        }
        let scrolled = frame(&mut state, vec![]);
        assert!(
            scrolled.1.top() > stable.1.top() + 30.,
            "wheel scrolling must not snap back"
        );
        assert_eq!(state.selected, selected);
        for _ in 0..30 {
            assert_eq!(frame(&mut state, vec![]), scrolled);
        }
        state.items[0].raw["documentation"] =
            serde_json::Value::String("Long documentation.\n".repeat(50));
        state.selected = 0;
        for _ in 0..5 {
            frame(&mut state, vec![]);
        }
        assert_eq!(
            state.completion_rect.unwrap().size(),
            stable.0.size(),
            "switching to long docs must not resize the popup or list"
        );
        let first = ctx
            .read_response(egui::Id::unique(("lsp-completion-row", 0usize)))
            .unwrap()
            .rect;
        assert!(state.completion_rect.unwrap().contains_rect(first));
    }
}
