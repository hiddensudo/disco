//! Literal search belongs to the visible document, including unsaved text.
use super::{Disco, workspace_keyboard::Area};
use crate::{file_preview::Content, settings::PreviewMode};
use eframe::egui::{self, Id, Key, Modifiers, Pos2, Rect, Stroke};
use std::ops::Range;

#[derive(Default)]
pub(super) struct Find {
    pub open: bool,
    pub query: String,
    pub case_sensitive: bool,
    pub matches: Vec<Range<usize>>,
    pub current: usize,
    source: String,
    searched: String,
    case_searched: bool,
    focus: bool,
    reveal: bool,
}
impl Find {
    pub fn update(&mut self, source: &str) {
        if self.source == source
            && self.searched == self.query
            && self.case_searched == self.case_sensitive
        {
            return;
        }
        let anchor = self.matches.get(self.current).map(|r| r.start).unwrap_or(0);
        let changed_query =
            self.searched != self.query || self.case_searched != self.case_sensitive;
        self.source = source.into();
        self.searched = self.query.clone();
        self.case_searched = self.case_sensitive;
        self.matches.clear();
        if !self.query.is_empty() {
            // Map folded UTF-8 bytes back to original character positions,
            // including Unicode lowercase expansions such as İ -> i + dot.
            let mut folded = String::new();
            let mut indices = Vec::new();
            for (index, c) in source.chars().enumerate() {
                let text = if self.case_sensitive {
                    c.to_string()
                } else {
                    c.to_lowercase().collect()
                };
                indices.extend(std::iter::repeat_n(index, text.len()));
                folded.push_str(&text);
            }
            let needle = if self.case_sensitive {
                self.query.clone()
            } else {
                self.query.to_lowercase()
            };
            for (byte, _) in folded.match_indices(&needle) {
                let range = indices[byte]..indices[byte + needle.len() - 1] + 1;
                if self.matches.last() != Some(&range) {
                    self.matches.push(range);
                }
            }
        }
        self.current = if changed_query {
            0
        } else {
            self.matches
                .iter()
                .position(|r| r.start >= anchor)
                .unwrap_or(0)
        };
        // Live updates refresh highlights without scrolling the reader away.
        self.reveal |= changed_query;
    }
    fn advance(&mut self, backwards: bool) {
        if !self.matches.is_empty() {
            self.current = (self.current + if backwards { self.matches.len() - 1 } else { 1 })
                % self.matches.len();
            self.reveal = true;
        }
    }
    pub fn paint(
        &mut self,
        ui: &egui::Ui,
        galley: &egui::Galley,
        origin: Pos2,
        accent: egui::Color32,
    ) -> Option<egui::text::CCursorRange> {
        if !self.open {
            return None;
        }
        let selected = self
            .matches
            .get(self.current)
            .filter(|_| self.reveal)
            .map(|range| {
                let start = galley
                    .pos_from_cursor(egui::text::CCursor::new(range.start))
                    .translate(origin.to_vec2());
                let end = galley
                    .pos_from_cursor(egui::text::CCursor::new(range.end))
                    .translate(origin.to_vec2());
                ui.scroll_to_rect(start.union(end), Some(egui::Align::Center));
                egui::text::CCursorRange::two(
                    egui::text::CCursor::new(range.start),
                    egui::text::CCursor::new(range.end),
                )
            });
        let mut index = 0;
        for row in &galley.rows {
            let row_end = index + row.char_count_excluding_newline().0;
            let top = origin.y + row.pos.y;
            if top > ui.clip_rect().bottom() {
                break;
            }
            if top + row.size.y >= ui.clip_rect().top() {
                let first = self.matches.partition_point(|range| range.end <= index);
                for (n, range) in self
                    .matches
                    .iter()
                    .enumerate()
                    .skip(first)
                    .take_while(|(_, r)| r.start < row_end)
                {
                    let a = range.start.max(index);
                    let b = range.end.min(row_end);
                    if a < b {
                        let r = Rect::from_min_max(
                            egui::pos2(
                                origin.x
                                    + row.pos.x
                                    + row.x_offset(egui::text::CharIndex(a - index)),
                                top,
                            ),
                            egui::pos2(
                                origin.x
                                    + row.pos.x
                                    + row.x_offset(egui::text::CharIndex(b - index)),
                                top + row.size.y,
                            ),
                        );
                        ui.painter().rect_filled(
                            r,
                            2.,
                            accent.linear_multiply(if n == self.current { 0.35 } else { 0.18 }),
                        );
                        if n == self.current {
                            ui.painter().rect_stroke(
                                r,
                                2.,
                                Stroke::new(1., accent),
                                egui::StrokeKind::Inside,
                            );
                        }
                    }
                }
            }
            index += row.char_count_including_newline().0;
        }
        self.reveal = false;
        selected
    }
}
impl Disco {
    pub(in crate::app) fn open_document_find(&mut self, ctx: &egui::Context) {
        if !matches!(self.file_preview.content, Some(Content::Text { .. })) {
            return;
        }
        let id = Id::unique(("file-editor", &self.editor.path));
        if let (Some(doc), Some(range)) = (
            &self.editor.document,
            egui::TextEdit::load_state(ctx, id).and_then(|s| s.cursor.char_range()),
        ) {
            let a = range.primary.index.0.min(range.secondary.index.0);
            let b = range.primary.index.0.max(range.secondary.index.0);
            if a < b && b - a <= 512 {
                let query: String = doc.text.chars().skip(a).take(b - a).collect();
                if !query.contains('\n') {
                    self.editor.find.query = query;
                }
            }
        }
        self.editor.find.open = true;
        self.editor.find.focus = true;
        self.editor.find.reveal = true;
        // Markdown search uses the source, so code, links and hidden details
        // have the same searchable content in both Read and Edit modes.
        if self
            .editor
            .path
            .as_deref()
            .is_some_and(crate::markdown::supported)
        {
            self.editor.markdown.editing = true;
        }
        self.keyboard.area = Area::Editor;
        ctx.request_repaint();
    }
    pub(in crate::app) fn document_find_shortcuts(&mut self, ctx: &egui::Context) {
        if self.preferences.layout.preview_mode != PreviewMode::Focus || !self.editor.find.open {
            return;
        }
        let focus = ctx.memory(|m| m.focused()) == Some(Id::unique("document-find"));
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.editor.find.open = false;
            self.focus_area(Area::Editor, ctx);
        } else {
            let previous = ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::F3))
                || focus && ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::Enter));
            let next = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F3))
                || focus && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
            if previous || next {
                self.editor.find.advance(previous);
            }
        }
    }
    pub(in crate::app) fn document_find_ui(&mut self, ui: &mut egui::Ui) {
        if !self.editor.find.open || self.preferences.layout.preview_mode != PreviewMode::Focus {
            return;
        }
        let mut close = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.;
            ui.spacing_mut().button_padding = egui::vec2(7., 4.);
            let find = &mut self.editor.find;
            let response = ui.add_sized(
                [(ui.available_width() - 205.).max(60.), 26.],
                egui::TextEdit::singleline(&mut find.query)
                    .id(Id::unique("document-find"))
                    .hint_text(crate::tr!("Знайти у файлі…", "Find in file…")),
            );
            if find.focus {
                response.request_focus();
                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), response.id) {
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(find.query.chars().count()),
                        )));
                    state.store(ui.ctx(), response.id);
                }
                find.focus = false;
            }
            let source = self
                .editor
                .document
                .as_ref()
                .map(|d| d.text.as_str())
                .or_else(|| match &self.file_preview.content {
                    Some(Content::Text { display_source, .. }) => Some(display_source.as_str()),
                    _ => None,
                })
                .unwrap_or("");
            find.update(source);
            let count = if find.matches.is_empty() {
                "0 / 0".into()
            } else {
                format!("{} / {}", find.current + 1, find.matches.len())
            };
            ui.label(count).on_hover_text(crate::tr!(
                "Збіги у відкритому тексті",
                "Matches in the open text"
            ));
            if ui
                .selectable_label(find.case_sensitive, "Aa")
                .on_hover_text(crate::tr!("Враховувати регістр", "Match case"))
                .clicked()
            {
                find.case_sensitive = !find.case_sensitive;
                find.update(source);
            }
            if ui
                .add_enabled(!find.matches.is_empty(), egui::Button::new("↑"))
                .on_hover_text(crate::tr!(
                    "Попередній · Shift+Enter / Shift+F3",
                    "Previous · Shift+Enter / Shift+F3"
                ))
                .clicked()
            {
                find.advance(true);
            }
            if ui
                .add_enabled(!find.matches.is_empty(), egui::Button::new("↓"))
                .on_hover_text(crate::tr!("Наступний · Enter / F3", "Next · Enter / F3"))
                .clicked()
            {
                find.advance(false);
            }
            close = ui
                .button("×")
                .on_hover_text(crate::tr!("Закрити пошук · Esc", "Close find · Esc"))
                .clicked();
        });
        ui.add_space(6.);
        if close {
            self.editor.find.open = false;
            self.focus_area(Area::Editor, ui.ctx());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_unicode_case_navigation_and_live_edits() {
        let mut find = Find {
            query: "\u{43a}\u{456}\u{442}".into(),
            ..Default::default()
        };
        find.update("\u{41a}\u{456}\u{442} 🦀 \u{43a}\u{456}\u{442}");
        assert_eq!(find.matches, vec![0..3, 6..9]);
        find.advance(true);
        assert_eq!(find.current, 1);
        find.advance(false);
        assert_eq!(find.current, 0);
        find.case_sensitive = true;
        find.update("\u{41a}\u{456}\u{442} 🦀 \u{43a}\u{456}\u{442}");
        assert_eq!(find.matches, vec![6..9]);
        find.query = ".*".into();
        find.update("literal .* not regex");
        assert_eq!(find.matches, vec![8..10]);
        find.query = "i".into();
        find.case_sensitive = false;
        find.update("İ i");
        assert_eq!(find.matches, vec![0..1, 2..3]);
        find.update("removed");
        assert!(find.matches.is_empty());
        find.query.clear();
        find.update("anything");
        assert!(find.matches.is_empty());
    }
}
