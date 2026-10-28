//! Resize the inspector directly, while keeping TOML preferences authoritative.
use crate::{
    app::{Disco, state::View},
    settings::{self, Layout, PreviewMode, color},
};
use eframe::egui::{self, Pos2, Rect, Sense, Stroke};

fn width_limit(available: f32, mode: PreviewMode, view: View) -> f32 {
    let remaining = if mode == PreviewMode::Focus && view != View::Trash {
        570.
    } else {
        360.
    };
    (available - remaining).clamp(Layout::MIN_PROPERTY_WIDTH, Layout::MAX_PROPERTY_WIDTH)
}
impl Disco {
    pub(in crate::app) fn show_inspector_panel(&mut self, ui: &mut egui::Ui) -> Rect {
        let area = ui.available_rect_before_wrap();
        let max = width_limit(
            area.width(),
            self.preferences.layout.preview_mode,
            self.browser.view,
        );
        let handle_id = ui.scope_id().with("inspector-divider");
        let enabled = !self.preferences.open
            && !self.deletion.confirm
            && self.purge_targets.is_none()
            && self.editor.pending.is_none()
            && !egui::Popup::is_any_open(ui.ctx());
        // Resolve the current drag before layout, using our own public widget ID
        // rather than relying on egui's private panel-state/resize identifiers.
        if enabled
            && ui
                .read_response(handle_id)
                .is_some_and(|r| r.dragged() || r.drag_stopped())
        {
            if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                self.preferences.layout.property_width = (area.right() - pointer.x)
                    .clamp(Layout::MIN_PROPERTY_WIDTH, max)
                    .round();
            }
        }
        let width = self
            .preferences
            .layout
            .property_width
            .clamp(Layout::MIN_PROPERTY_WIDTH, max);
        let t = self.preferences.theme.clone();
        let panel = egui::Panel::right("inspector")
            .exact_size(width)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(color(&t.panel))
                    .inner_margin(egui::Margin::symmetric(18, 12)),
            )
            .show(ui, |ui| self.inspector(ui));
        let rect = panel.response.rect;
        ui.interact(rect, egui::Id::unique("properties-panel"), Sense::hover());
        let handle = Rect::from_min_max(
            Pos2::new(rect.left() - 4., rect.top()),
            Pos2::new(rect.left() + 4., rect.bottom()),
        );
        let response = ui
            .interact(
                handle,
                handle_id,
                if enabled {
                    Sense::drag()
                } else {
                    Sense::hover()
                },
            )
            .on_hover_text(crate::tr!(
                "Перетягни, щоб змінити ширину панелі",
                "Drag to resize the panel"
            ));
        if enabled && (response.hovered() || response.dragged()) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            ui.painter().vline(
                rect.left(),
                rect.y_range(),
                Stroke::new(1.5, color(&t.accent)),
            );
        } else {
            ui.painter().vline(
                rect.left(),
                rect.y_range(),
                Stroke::new(1., color(&t.border)),
            );
        }
        // A small grip makes the divider discoverable without a permanent accent.
        for offset in [-4., 0., 4.] {
            let y = rect.center().y + offset;
            ui.painter().line_segment(
                [
                    Pos2::new(rect.left() - 2., y),
                    Pos2::new(rect.left() + 2., y),
                ],
                Stroke::new(1., color(&t.muted).gamma_multiply(0.65)),
            );
        }
        if enabled && response.drag_stopped() {
            if let Err(e) = settings::write(
                &settings::config_dir().join("layout.toml"),
                &self.preferences.layout,
            ) {
                self.message = crate::tr!(
                    "Не вдалося зберегти ширину панелі: {e}",
                    "Could not save the panel width: {e}",
                    e = e
                );
            }
        }
        rect
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Vec2;
    use std::{fs, path::PathBuf, process::Command};
    #[test]
    fn divider_drag_saves_on_release_and_respects_settings_and_window_limits() {
        let Some(root) = std::env::var_os("DISCO_RESIZE_FIXTURE").map(PathBuf::from) else {
            let root = std::env::temp_dir().join(format!("disco-resize-{}", std::process::id()));
            fs::create_dir_all(&root).unwrap();
            fs::write(
                root.join("layout.toml"),
                "save_scan=false\nauto_refresh=false\nproperty_width=300.0\n",
            )
            .unwrap();
            let output=Command::new(std::env::current_exe().unwrap()).args(["--exact","app::ui::inspector_layout::tests::divider_drag_saves_on_release_and_respects_settings_and_window_limits","--nocapture"])
                .env("DISCO_RESIZE_FIXTURE",&root).env("DISCO_CONFIG_DIR",&root).output().unwrap();
            fs::remove_dir_all(root).unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        };
        let ctx = egui::Context::default();
        let mut app = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let mut time = 0.;
        let mut frame = |app: &mut Disco, width: f32, events: Vec<egui::Event>| {
            time += 0.05;
            let mut rect = Rect::NOTHING;
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 700.))),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    rect = app.show_inspector_panel(ui);
                    egui::CentralPanel::default().show(ui, |_| {});
                },
            )
            .drop_without_applying_deltas();
            rect
        };
        let before = fs::read(root.join("layout.toml")).unwrap();
        let initial = frame(&mut app, 1360., vec![]);
        assert!((initial.width() - 300.).abs() < 2.);
        let from = Pos2::new(initial.left(), 200.);
        let to = from - Vec2::new(200., 0.);
        frame(&mut app, 1360., vec![egui::Event::PointerMoved(from)]);
        frame(
            &mut app,
            1360.,
            vec![egui::Event::PointerButton {
                pos: from,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        frame(&mut app, 1360., vec![egui::Event::PointerMoved(to)]);
        assert_eq!(
            fs::read(root.join("layout.toml")).unwrap(),
            before,
            "dragging must not write every frame"
        );
        frame(
            &mut app,
            1360.,
            vec![egui::Event::PointerButton {
                pos: to,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let rect = frame(&mut app, 1360., vec![]);
        assert!((rect.width() - 500.).abs() < 2., "{rect:?}");
        assert!(
            (settings::read_layout(&root.join("layout.toml"))
                .unwrap()
                .property_width
                - 500.)
                .abs()
                < 2.
        );
        app.preferences.layout.property_width = 620.;
        assert!((frame(&mut app, 1360., vec![]).width() - 620.).abs() < 2.);
        let narrow = frame(&mut app, 800., vec![]);
        assert!(narrow.width() <= 442.);
        assert_eq!(app.preferences.layout.property_width, 620.);
        assert!((frame(&mut app, 1360., vec![]).width() - 620.).abs() < 2.);
        assert!(
            (settings::read_layout(&root.join("layout.toml"))
                .unwrap()
                .property_width
                - 500.)
                .abs()
                < 2.,
            "temporary window clamping must not overwrite preferences"
        );
    }
}
