use crate::app::Disco;
use crate::settings::color;
use eframe::egui::{self, FontId, Stroke, Vec2};

impl Disco {
    pub(in crate::app) fn apply_style(&mut self, ctx: &egui::Context) {
        if self.preferences.applied.as_ref()
            == Some(&(
                self.preferences.theme.clone(),
                self.preferences.layout.clone(),
            ))
        {
            return;
        }
        let t = &self.preferences.theme;
        if !self.preferences.applied.as_ref().is_some_and(|(old, _)| {
            crate::branding::dark(color(&old.background))
                == crate::branding::dark(color(&t.background))
        }) {
            ctx.send_viewport_cmd_to(
                egui::ViewportId::ROOT,
                egui::ViewportCommand::Icon(Some(crate::art::app_icon(color(&t.background)))),
            );
        }
        let mut s = egui::Style::default();
        if self.screenshot.is_some() {
            s.interaction.tooltip_delay = 60.;
        }
        s.visuals = egui::Visuals::dark();
        s.animation_time = if self.preferences.layout.animations {
            0.16 / self.preferences.layout.motion_speed
        } else {
            0.
        };
        s.visuals.panel_fill = color(&t.background);
        s.visuals.window_fill = color(&t.panel);
        s.visuals.extreme_bg_color = color(&t.background);
        s.visuals.override_text_color = Some(color(&t.text));
        s.visuals.hyperlink_color = color(&t.accent);
        s.visuals.selection.bg_fill = color(&t.accent);
        s.visuals.selection.stroke = Stroke::new(1., color(&t.text));
        s.visuals.window_corner_radius = egui::CornerRadius::same(t.radius);
        s.visuals.window_stroke = Stroke::new(1., color(&t.border));
        s.visuals.window_shadow = egui::epaint::Shadow::NONE;
        s.visuals.popup_shadow = egui::epaint::Shadow::NONE;
        for w in [
            &mut s.visuals.widgets.inactive,
            &mut s.visuals.widgets.hovered,
            &mut s.visuals.widgets.active,
            &mut s.visuals.widgets.open,
            &mut s.visuals.widgets.noninteractive,
        ] {
            w.corner_radius = egui::CornerRadius::same(t.radius);
            w.bg_fill = color(&t.raised);
            w.weak_bg_fill = color(&t.panel);
            w.bg_stroke = Stroke::new(1., color(&t.border));
            w.fg_stroke = Stroke::new(1., color(&t.text));
        }
        s.visuals.widgets.hovered.bg_fill = color(&t.raised);
        s.visuals.widgets.hovered.weak_bg_fill = color(&t.raised);
        s.visuals.widgets.active.bg_stroke = Stroke::new(1., color(&t.accent));
        s.spacing.item_spacing = Vec2::new(9., 6.);
        s.spacing.button_padding = Vec2::new(12., 8.);
        let scale = self.preferences.layout.effective_text_scale();
        for (style, size) in [
            (egui::TextStyle::Body, 16.),
            (egui::TextStyle::Button, 15.),
            (egui::TextStyle::Small, 12.),
            (egui::TextStyle::Heading, 22.),
            (egui::TextStyle::Monospace, 13.),
        ] {
            s.text_styles
                .insert(style, FontId::proportional(size * scale));
        }
        if self
            .preferences
            .applied
            .as_ref()
            .is_some_and(|(_, old)| old.animations != self.preferences.layout.animations)
        {
            ctx.clear_animations();
        }
        ctx.set_theme(egui::Theme::Dark);
        ctx.set_style_of(egui::Theme::Dark, s);
        self.preferences.applied = Some((
            self.preferences.theme.clone(),
            self.preferences.layout.clone(),
        ));
    }
}
