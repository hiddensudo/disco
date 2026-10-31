//! File zoom belongs to a document/tab, never to egui's whole-window scale.
use super::{Disco, workspace_keyboard::Area};
use eframe::egui::{self, Event, Key, Rect, Vec2};

#[derive(Clone, Copy)]
pub(super) struct Zoom {
    pub factor: f32,
    drawn: f32,
}
impl Default for Zoom {
    fn default() -> Self {
        Self {
            factor: 1.,
            drawn: 1.,
        }
    }
}
impl Zoom {
    pub fn change(&mut self, steps: f32, image: bool) {
        self.factor = (self.factor * 1.1_f32.powf(steps.clamp(-20., 20.)))
            .clamp(if image { 0.25 } else { 0.5 }, if image { 8. } else { 3. });
        if (self.factor - 1.).abs() < 0.0001 {
            self.factor = 1.;
        }
    }
    pub fn ratio(&self) -> f32 {
        self.factor / self.drawn
    }
    pub fn drawn(&mut self) {
        self.drawn = self.factor;
    }
    pub fn scroll(
        &self,
        area: egui::ScrollArea,
        ui: &egui::Ui,
        salt: impl std::hash::Hash + std::fmt::Debug,
        anchor: Vec2,
    ) -> egui::ScrollArea {
        if (self.ratio() - 1.).abs() < 0.0001 {
            return area;
        }
        let id = ui.make_persistent_id(salt);
        if let Some(state) = egui::scroll_area::State::load(ui.ctx(), id) {
            area.scroll_offset(
                ((state.clamped_offset() + anchor) * self.ratio() - anchor).max(Vec2::ZERO),
            )
        } else {
            area
        }
    }
}
fn zoom_key(key: Key, physical: Option<Key>) -> Option<f32> {
    match key {
        Key::Plus | Key::Equals => Some(1.),
        Key::Minus => Some(-1.),
        _ => match physical {
            Some(Key::Plus | Key::Equals) => Some(1.),
            Some(Key::Minus) => Some(-1.),
            _ => None,
        },
    }
}
impl Disco {
    fn zoom_blocked(&self) -> bool {
        self.preferences.open
            || self.deletion.confirm
            || self.purge_targets.is_some()
            || self.editor.pending.is_some()
            || self.save_as.is_some()
            || self.new_file.is_some()
            || !matches!(
                self.file_preview.content,
                Some(
                    crate::file_preview::Content::Text { .. }
                        | crate::file_preview::Content::Image { .. }
                )
            )
    }
    pub(in crate::app) fn document_zoom_shortcuts(&mut self, ctx: &egui::Context) {
        if self.zoom_blocked() || self.terminal_focused(ctx) || egui::Popup::is_any_open(ctx) {
            return;
        }
        let focus = ctx.memory(|m| m.focused());
        let editor = egui::Id::unique(("file-editor", &self.editor.path));
        if focus != Some(editor)
            && !(self.keyboard.area == Area::Editor
                && (focus.is_none() || focus == Some(egui::Id::unique("preview-viewport"))))
        {
            return;
        }
        let steps = ctx.input_mut(|i| {
            let mut steps = 0.;
            let mut consumed = false;
            i.events.retain(|event| {
                if let Event::Key {
                    key,
                    physical_key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                    && modifiers.ctrl
                    && !modifiers.alt
                    && !modifiers.mac_cmd
                    && let Some(step) = zoom_key(*key, *physical_key)
                {
                    steps += step;
                    consumed = true;
                    return false;
                }
                true
            });
            if consumed {
                i.events.retain(
                    |event| !matches!(event,Event::Text(s) if ["+","=","-"].contains(&s.as_str())),
                );
            }
            steps
        });
        self.editor.zoom.change(
            steps,
            matches!(
                self.file_preview.content,
                Some(crate::file_preview::Content::Image { .. })
            ),
        );
    }
    pub(in crate::app) fn document_zoom_wheel(&mut self, ui: &egui::Ui, rect: Rect) {
        if self.zoom_blocked()
            || !ui.rect_contains_pointer(rect)
            || egui::Popup::is_any_open(ui.ctx())
        {
            return;
        }
        let steps = ui.input_mut(|i| {
            let mut steps = 0.;
            let mut consumed = false;
            i.events.retain(|event| {
                if let Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } = event
                    && modifiers.ctrl
                    && !modifiers.alt
                    && !modifiers.mac_cmd
                {
                    steps += delta.y
                        * match unit {
                            egui::MouseWheelUnit::Line => 1.,
                            egui::MouseWheelUnit::Point => 1. / 50.,
                            egui::MouseWheelUnit::Page => 3.,
                        };
                    consumed = true;
                    return false;
                }
                true
            });
            if consumed {
                i.smooth_scroll_delta = Vec2::ZERO;
            }
            steps.clamp(-4., 4.)
        });
        self.editor.zoom.change(
            steps,
            matches!(
                self.file_preview.content,
                Some(crate::file_preview::Content::Image { .. })
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zoom_is_bounded_and_physical_plus_supports_alternate_layouts() {
        let mut zoom = Zoom::default();
        for _ in 0..100 {
            zoom.change(1., false);
        }
        assert_eq!(zoom.factor, 3.);
        for _ in 0..100 {
            zoom.change(-1., false);
        }
        assert_eq!(zoom.factor, 0.5);
        for _ in 0..100 {
            zoom.change(-1., true);
        }
        assert_eq!(zoom.factor, 0.25);
        assert_eq!(zoom_key(Key::Plus, None), Some(1.));
        assert_eq!(zoom_key(Key::Equals, None), Some(1.));
        assert_eq!(zoom_key(Key::A, Some(Key::Equals)), Some(1.));
        assert_eq!(zoom_key(Key::Minus, None), Some(-1.));
    }
}
