//! Small, bounded, event-driven tweens. No timers or background animation thread.
use eframe::egui::{self, Color32, Id, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

const LIMIT: usize = 384;

#[derive(Clone)]
struct Tween {
    from: f32,
    to: f32,
    start: f64,
    seconds: f32,
    token: Option<u64>,
    touched: u64,
}
impl Tween {
    fn sample(&self, now: f64) -> (f32, bool) {
        if self.seconds <= 0. || self.from == self.to {
            return (self.to, false);
        }
        let t = ((now - self.start) as f32 / self.seconds).clamp(0., 1.);
        if t >= 1. {
            return (self.to, false);
        }
        let eased = 1. - (1. - t).powi(3);
        (self.from + (self.to - self.from) * eased, true)
    }
}
#[derive(Clone, Default)]
struct Bank {
    entries: HashMap<Id, Tween>,
    tick: u64,
}
impl Bank {
    fn sample(
        &mut self,
        id: Id,
        target: f32,
        token: Option<u64>,
        now: f64,
        seconds: f32,
    ) -> (f32, bool) {
        self.tick = self.tick.wrapping_add(1);
        if !self.entries.contains_key(&id) {
            if self.entries.len() >= LIMIT {
                if let Some(oldest) = self
                    .entries
                    .iter()
                    .min_by_key(|(_, t)| t.touched)
                    .map(|(k, _)| *k)
                {
                    self.entries.remove(&oldest);
                }
            }
            let from = if token.is_some() && seconds > 0. {
                0.
            } else {
                target
            };
            self.entries.insert(
                id,
                Tween {
                    from,
                    to: target,
                    start: now,
                    seconds,
                    token,
                    touched: self.tick,
                },
            );
        }
        let tween = self.entries.get_mut(&id).unwrap();
        let current = tween.sample(now).0;
        if seconds <= 0. {
            tween.from = target;
            tween.to = target;
            tween.seconds = 0.;
            tween.token = token;
        } else if tween.token != token || tween.to != target {
            tween.from = if token.is_some() { 0. } else { current };
            tween.to = target;
            tween.start = now;
            tween.seconds = seconds;
            tween.token = token;
        }
        tween.touched = self.tick;
        tween.sample(now)
    }
}
pub fn key(value: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}
fn sample(ui: &Ui, id: Id, target: f32, token: Option<u64>, multiplier: f32) -> f32 {
    let seconds = ui.style().animation_time * multiplier;
    let now = ui.input(|i| i.time);
    let (value, active) = ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_default::<Bank>(Id::unique("disco-motion-bank"))
            .sample(id, target, token, now, seconds)
    });
    if active {
        ui.ctx().request_repaint();
    }
    value
}
pub fn value(ui: &Ui, id: impl Hash, target: f32, duration: f32) -> f32 {
    sample(ui, Id::unique(key(id)), target, None, duration)
}
pub fn hover(ui: &Ui, id: impl Hash, on: bool) -> f32 {
    value(ui, id, if on { 1. } else { 0. }, 0.8)
}
pub fn enter(ui: &Ui, id: impl Hash, token: impl Hash) -> f32 {
    sample(ui, Id::unique(key(id)), 1., Some(key(token)), 1.25)
}
pub fn disclosure(ui: &Ui, center: Pos2, openness: f32, color: Color32) {
    let angle = openness * std::f32::consts::FRAC_PI_2;
    let (sin, cos) = angle.sin_cos();
    let points = [Vec2::new(-2., -4.), Vec2::new(2., 0.), Vec2::new(-2., 4.)]
        .map(|p| center + Vec2::new(p.x * cos - p.y * sin, p.x * sin + p.y * cos));
    ui.painter()
        .add(egui::Shape::line(points.to_vec(), Stroke::new(1.5, color)));
}
pub fn activity(ui: &mut Ui, paused: bool, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.), Sense::hover());
    if paused {
        for x in [-3., 3.] {
            ui.painter().rect_filled(
                Rect::from_center_size(rect.center() + Vec2::new(x, 0.), Vec2::new(2., 10.)),
                1,
                color,
            );
        }
        return;
    }
    let animated = ui.style().animation_time > 0.;
    let angle = if animated {
        ui.input(|i| (i.time * 3.5).rem_euclid(std::f64::consts::TAU) as f32)
    } else {
        -1.5
    };
    let points = (0..24)
        .map(|i| rect.center() + Vec2::angled(angle + i as f32 / 23. * 4.6) * 6.5)
        .collect();
    ui.painter().circle_stroke(
        rect.center(),
        6.5,
        Stroke::new(1.5, color.gamma_multiply(0.15)),
    );
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(1.8, color)));
    if animated {
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tweens_settle_and_interrupt_without_jumping() {
        let mut b = Bank::default();
        let id = Id::unique("hover");
        assert_eq!(b.sample(id, 0., None, 0., 0.2), (0., false));
        assert_eq!(b.sample(id, 1., None, 1., 0.2), (0., true));
        let mid = b.sample(id, 1., None, 1.1, 0.2).0;
        assert!(mid > 0. && mid < 1.);
        assert_eq!(b.sample(id, 0., None, 1.1, 0.2).0, mid);
        assert_eq!(b.sample(id, 0., None, 1.4, 0.2), (0., false));
        assert_eq!(b.sample(id, 0., None, 50., 0.2), (0., false));
    }
    #[test]
    fn reduced_motion_snaps_and_enter_restarts_only_on_new_content() {
        let mut b = Bank::default();
        let id = Id::unique("panel");
        assert_eq!(b.sample(id, 1., Some(1), 0., 0.2), (0., true));
        assert_eq!(b.sample(id, 1., Some(1), 1., 0.2), (1., false));
        assert_eq!(b.sample(id, 1., Some(2), 2., 0.2), (0., true));
        assert_eq!(b.sample(id, 1., Some(2), 2.01, 0.), (1., false));
        assert_eq!(b.sample(id, 1., Some(2), 3., 0.2), (1., false));
    }
    #[test]
    fn scrolling_through_many_folders_does_not_grow_animation_memory() {
        let mut b = Bank::default();
        for i in 0..10_000 {
            b.sample(Id::unique(i), 0., None, i as f64, 0.2);
        }
        assert_eq!(b.entries.len(), LIMIT);
    }
    #[test]
    fn animation_progresses_across_actual_egui_frames() {
        let ctx = egui::Context::default();
        let frame = |time: f64, on: bool, seconds: f32| {
            let mut result = -1.;
            ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |ui| {
                    ui.style_mut().animation_time = seconds;
                    result = hover(ui, "frame-hover", on);
                },
            )
            .drop_without_applying_deltas();
            result
        };
        assert_eq!(frame(0., false, 0.16), 0.);
        assert_eq!(frame(1., true, 0.16), 0.);
        let mid = frame(1.05, true, 0.16);
        assert!(
            mid > 0. && mid < 1.,
            "egui must retain the tween and interpolate between frames"
        );
        assert_eq!(frame(2., true, 0.16), 1.);
        assert_eq!(frame(3., false, 0.), 0.);
    }
    #[test]
    fn active_animation_requests_next_frame_without_a_30fps_timer() {
        let ctx = egui::Context::default();
        let frame = |time: f64, on: bool| {
            let out = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |ui| {
                    ui.style_mut().animation_time = 0.16;
                    hover(ui, "pacing-test", on);
                },
            );
            let delay = out.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
            out.drop_without_applying_deltas();
            delay
        };
        for i in 0..5 {
            frame(i as f64 * 0.01, false);
        }
        assert_eq!(frame(1., true), std::time::Duration::ZERO);
        let mut settled = std::time::Duration::ZERO;
        for i in 0..6 {
            settled = frame(2. + i as f64 * 0.1, true);
        }
        assert_eq!(
            settled,
            std::time::Duration::MAX,
            "settled effects must not keep requesting frames"
        );
    }
}

/// One damped hover response; no continuous motion while the pointer stays put.
pub fn jelly(ui: &Ui, id: impl Hash, hovered: bool) -> Vec2 {
    #[derive(Clone, Default)]
    struct Jelly {
        hovered: bool,
        started: Option<f64>,
    }
    let now = ui.input(|i| i.time);
    let enabled = ui.style().animation_time > 0.;
    let started = ui.ctx().data_mut(|data| {
        let state = data.get_temp_mut_or_default::<Jelly>(Id::unique(("jelly", key(id))));
        if hovered && !state.hovered {
            state.started = Some(now);
        }
        state.hovered = hovered;
        if !enabled {
            state.started = None;
        }
        state.started
    });
    let Some(started) = started else {
        return Vec2::splat(1.);
    };
    let t = ((now - started) / 0.55).clamp(0., 1.) as f32;
    if t >= 1. {
        return Vec2::splat(1.);
    }
    ui.ctx().request_repaint();
    let wobble = (t * std::f32::consts::TAU * 2.5).sin() * (-5. * t).exp() * 0.22;
    Vec2::new(1. + wobble, 1. - wobble * 0.8)
}
