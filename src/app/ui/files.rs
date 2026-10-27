use crate::{
    app::Disco,
    directory_files::Entry,
    filesystem::Properties,
    format::{display_size, label, shorten},
    settings::{Theme, color},
};
use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
};

impl Disco {
    pub(in crate::app) fn folder_can_expand(&self, id: usize) -> bool {
        if !self.scan.index.children(id).is_empty() {
            return true;
        }
        if self.demo {
            return false;
        }
        let Some(report) = &self.scan.report else {
            return false;
        };
        self.files
            .get(&report.folders[id].path)
            .is_none_or(|l| !l.files.is_empty() || l.limited || l.error.is_some())
    }
    pub(in crate::app) fn select_file(&mut self, path: PathBuf, bytes: u64) {
        self.browser.properties = Some(Properties::load(path, bytes, None, self.demo));
    }
    pub(in crate::app) fn open_file(&self, path: PathBuf) {
        if self.demo
            || !self.preferences.layout.open_files_on_double_click
            || self.opening_file.swap(true, Ordering::Relaxed)
        {
            return;
        }
        let busy = self.opening_file.clone();
        std::thread::spawn(move || {
            crate::associations::open(&path);
            busy.store(false, Ordering::Relaxed);
        });
    }
    pub(in crate::app) fn file_row(
        &mut self,
        ui: &mut egui::Ui,
        file: Arc<Entry>,
        depth: usize,
        height: f32,
        name_width: f32,
        count_width: f32,
        t: &Theme,
    ) {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
        let active = self
            .browser
            .properties
            .as_ref()
            .is_some_and(|p| !p.is_dir && p.path == file.path);
        if active || response.hovered() {
            ui.painter().rect_filled(
                rect,
                t.radius,
                if active {
                    super::widgets::mix(color(&t.panel), color(&t.accent), 0.31)
                } else {
                    color(&t.raised)
                },
            );
            if active {
                ui.painter().rect_filled(
                    Rect::from_min_size(rect.min, Vec2::new(4., height)),
                    0,
                    color(&t.accent),
                );
            }
        }
        let x = rect.left() + 40. + (depth as f32 * 19.).min(name_width * 0.25);
        self.art.paint(
            ui,
            crate::art::file_icon(&file.path),
            Rect::from_center_size(
                Pos2::new(x + 10., rect.center().y),
                Vec2::splat(21. * self.preferences.layout.icon_scale),
            ),
            color(&t.text),
        );
        let text_x = x + 28.;
        let shown = shorten(
            &label(&file.path),
            ((name_width - (text_x - rect.left()) - 7.) / 7.2).max(3.) as usize,
        );
        ui.painter()
            .with_clip_rect(Rect::from_min_max(
                rect.min,
                Pos2::new(rect.left() + name_width, rect.bottom()),
            ))
            .text(
                Pos2::new(text_x, rect.center().y),
                Align2::LEFT_CENTER,
                shown,
                FontId::proportional(14. * self.preferences.layout.text_scale),
                color(&t.text),
            );
        ui.painter().text(
            Pos2::new(rect.right() - count_width - 12., rect.center().y),
            Align2::RIGHT_CENTER,
            display_size(file.bytes),
            FontId::proportional(14. * self.preferences.layout.text_scale),
            color(&t.text),
        );
        if self.preferences.layout.row_separators {
            ui.painter().hline(
                rect.x_range(),
                rect.bottom(),
                Stroke::new(0.5, color(&t.border)),
            );
        }
        if response.clicked() {
            self.select_file(file.path.clone(), file.bytes);
        }
        if response.double_clicked() {
            self.open_file(file.path.clone());
        }
        response.on_hover_text(file.path.to_string_lossy());
    }
    pub(in crate::app) fn file_listing_row(
        &mut self,
        ui: &mut egui::Ui,
        id: usize,
        depth: usize,
        height: f32,
        pending: bool,
        t: &Theme,
    ) {
        let Some(folder) = self.scan.report.as_ref().and_then(|r| r.folders.get(id)) else {
            return;
        };
        let path = folder.path.clone();
        let text = if pending {
            self.files.request(path, ui.ctx());
            crate::tr!("Підвантаження файлів…", "Loading files…").to_string()
        } else {
            self.files
                .get(&path)
                .map(|l| {
                    l.error.clone().unwrap_or_else(|| {
                        crate::tr!(
                            "Список скорочено · решта у файловому менеджері",
                            "Listing shortened · use the file manager for the rest"
                        )
                        .into()
                    })
                })
                .unwrap_or_default()
        };
        let (r, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
        ui.painter().with_clip_rect(r).text(
            r.left_center() + Vec2::new(16. + depth as f32 * 14., 0.),
            Align2::LEFT_CENTER,
            text,
            FontId::proportional(11.),
            color(&t.muted),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        process::Command,
        time::{Duration, Instant},
    };
    #[test]
    fn file_gestures_select_once_and_only_launch_an_enabled_existing_association() {
        let Some(root) = std::env::var_os("DISCO_GESTURE_TEST_DIR").map(PathBuf::from) else {
            let root =
                std::env::temp_dir().join(format!("disco-file-gesture-{}", std::process::id()));
            fs::create_dir_all(root.join("config")).unwrap();
            fs::create_dir_all(root.join("bin")).unwrap();
            fs::create_dir_all(root.join("data/applications")).unwrap();
            fs::write(
                root.join("config/layout.toml"),
                "save_scan=false\nauto_refresh=false\n",
            )
            .unwrap();
            fs::write(root.join("notes.txt"), "only a fixture").unwrap();
            for (name, script) in [
                (
                    "xdg-mime",
                    "#!/bin/sh\nif [ \"$2\" = filetype ]; then printf 'text/plain\\n'; else printf 'viewer.desktop\\n'; fi\n",
                ),
                (
                    "gio",
                    "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$DISCO_OPEN_LOG\"\n",
                ),
            ] {
                let p = root.join("bin").join(name);
                fs::write(&p, script).unwrap();
                fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
            }
            let result=Command::new(std::env::current_exe().unwrap())
                .args(["--exact","app::ui::files::tests::file_gestures_select_once_and_only_launch_an_enabled_existing_association","--nocapture"])
                .env("DISCO_GESTURE_TEST_DIR",&root).env("DISCO_CONFIG_DIR",root.join("config"))
                .env("XDG_DATA_HOME",root.join("data")).env("XDG_DATA_DIRS",root.join("absent"))
                .env("DISCO_OPEN_LOG",root.join("opened.log"))
                .env("PATH",format!("{}:/usr/bin:/bin",root.join("bin").display()))
                .output().unwrap();
            fs::remove_dir_all(root).unwrap();
            assert!(
                result.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        };
        let desktop = root.join("data/applications/viewer.desktop");
        let log = root.join("opened.log");
        for (enabled, associated) in [(true, true), (false, true), (true, false)] {
            let _ = fs::remove_file(&log);
            if associated {
                fs::write(&desktop, "[Desktop Entry]\nType=Application\nExec=false\n").unwrap();
            } else {
                fs::remove_file(&desktop).unwrap();
            }
            let ctx = egui::Context::default();
            let cc = eframe::CreationContext::_new_kittest(ctx.clone());
            let mut app = Disco::new(&cc);
            app.preferences.layout.open_files_on_double_click = enabled;
            let file = Arc::new(Entry {
                path: root.join("notes.txt"),
                bytes: 4096,
            });
            let theme = app.preferences.theme.clone();
            let pos = egui::pos2(100., 15.);
            let mut frame = |time: f64, pressed: Option<bool>| {
                let mut events = vec![egui::Event::PointerMoved(pos)];
                if let Some(pressed) = pressed {
                    events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(600., 300.),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        app.file_row(ui, file.clone(), 0, 30., 400., 0., &theme);
                    },
                )
                .drop_without_applying_deltas();
            };
            frame(0., None);
            frame(0.1, Some(true));
            frame(0.15, Some(false));
            assert!(
                !log.exists(),
                "a single click must never launch an application"
            );
            frame(0.2, Some(true));
            frame(0.25, Some(false));
            assert_eq!(
                app.browser.properties.as_ref().map(|p| &p.path),
                Some(&file.path)
            );
            let start = Instant::now();
            while app.opening_file.load(Ordering::Relaxed)
                && start.elapsed() < Duration::from_secs(3)
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(!app.opening_file.load(Ordering::Relaxed));
            assert_eq!(log.exists(), enabled && associated);
            if log.exists() {
                let lines = fs::read_to_string(&log).unwrap();
                assert_eq!(
                    lines.lines().collect::<Vec<_>>(),
                    vec![
                        "launch",
                        desktop.to_str().unwrap(),
                        file.path.to_str().unwrap()
                    ]
                );
            }
        }
    }
}
