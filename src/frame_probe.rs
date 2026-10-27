//! Opt-in, demo-only measurement of UI callback cadence, not physical presentation.
use eframe::egui::{self, Color32, Pos2};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub struct FrameProbe {
    path: PathBuf,
    start: Instant,
    last_input_time: Option<f64>,
    frames: Vec<f64>,
    cpu_ms: Vec<f64>,
    idle_frames: usize,
    idle_input_events: usize,
    idle_causes: Vec<String>,
    folders: usize,
    target: f32,
    seeded: bool,
    done: bool,
}
impl FrameProbe {
    pub fn new(path: PathBuf, folders: usize) -> Self {
        Self {
            path,
            start: Instant::now(),
            last_input_time: None,
            frames: vec![],
            cpu_ms: vec![],
            idle_frames: 0,
            idle_input_events: 0,
            idle_causes: vec![],
            folders,
            target: 0.5,
            seeded: false,
            done: false,
        }
    }
    pub fn tick(&mut self, ui: &egui::Ui, cpu: Option<f32>) {
        if self.done {
            return;
        }
        let elapsed = self.start.elapsed().as_secs_f64();
        let input = ui.input(|i| i.time);
        if self.last_input_time != Some(input) {
            self.last_input_time = Some(input);
            if (1.0..5.0).contains(&elapsed) {
                self.frames.push(elapsed);
                if let Some(cpu) = cpu {
                    self.cpu_ms.push(cpu as f64 * 1000.);
                }
            } else if (5.5..9.0).contains(&elapsed) {
                self.idle_frames += 1;
                self.idle_input_events += ui.input(|i| i.raw.events.len());
                if self.idle_frames <= 5 {
                    self.idle_causes
                        .push(format!("{:?}", ui.ctx().repaint_causes()));
                }
            }
        }
        if elapsed >= 9. {
            let intervals: Vec<f64> = self
                .frames
                .windows(2)
                .map(|f| (f[1] - f[0]) * 1000.)
                .collect();
            let span = self
                .frames
                .last()
                .zip(self.frames.first())
                .map(|(b, a)| b - a)
                .unwrap_or(0.);
            let fps = if span > 0. {
                (self.frames.len() - 1) as f64 / span
            } else {
                0.
            };
            let result = serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"), "measurement": "UI callback cadence; not physical presentation timestamps",
                "folder_count": self.folders, "active_frames": self.frames.len(), "fps": fps,
                "median_frame_ms": percentile(&intervals, 0.5), "p95_frame_ms": percentile(&intervals, 0.95),
                "max_frame_ms": percentile(&intervals, 1.), "p95_cpu_ms": percentile(&self.cpu_ms, 0.95),
                "idle_frames_in_3_5s": self.idle_frames, "idle_input_events": self.idle_input_events, "idle_causes_first_frames": self.idle_causes, "intervals_ms": intervals });
            if let Some(parent) = self.path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&self.path, serde_json::to_vec_pretty(&result).unwrap())
            {
                eprintln!("Frame probe: {e}");
            }
            self.done = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if !self.seeded {
            self.seeded = true;
            ui.ctx().request_repaint();
        }
        if elapsed < 5. {
            self.target = 0.5 + (elapsed as f32 * 3.).sin() * 0.5;
        }
        // Critically, the probe does NOT force a repaint loop: the actual motion
        // helper drives frames while this target moves, then settles and stops.
        let x = crate::motion::value(ui, "diagnostic-motion", self.target, 1.);
        ui.painter().circle_filled(
            Pos2::new(190. + x * 70., 21.),
            3.,
            Color32::from_rgb(228, 90, 82),
        );
        ui.ctx()
            .request_repaint_after(Duration::from_secs_f64(9. - elapsed));
    }
}
fn percentile(values: &[f64], fraction: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    Some(values[((values.len() - 1) as f64 * fraction).round() as usize])
}
