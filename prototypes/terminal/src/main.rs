use eframe::egui::{self, Color32, FontId, RichText, Vec2};
use egui_term::{
    BackendCommand, BackendSettings, ColorPalette, FontSettings, PtyEvent, TerminalBackend,
    TerminalFont, TerminalTheme, TerminalView,
};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

const BG: Color32 = Color32::from_rgb(15, 20, 23);
const PANEL: Color32 = Color32::from_rgb(22, 28, 31);
const ACCENT: Color32 = Color32::from_rgb(233, 86, 80);
fn tr(en: bool, uk: &'static str, english: &'static str) -> &'static str {
    if en { english } else { uk }
}
fn arg(key: &str) -> Option<String> {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|a| a[0] == key)
        .map(|a| a[1].clone())
}
fn system_shell() -> String {
    if let Ok(shell) = std::env::var("SHELL") {
        if Path::new(&shell).is_file() {
            return shell;
        }
    }
    // Reentrant account lookup, before spawning the PTY workers.
    unsafe {
        let mut account: libc::passwd = std::mem::zeroed();
        let mut buffer = vec![0; 16384];
        let mut result = std::ptr::null_mut();
        if libc::getpwuid_r(
            libc::geteuid(),
            &mut account,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut result,
        ) == 0
            && !result.is_null()
            && !account.pw_shell.is_null()
        {
            let shell = std::ffi::CStr::from_ptr(account.pw_shell)
                .to_string_lossy()
                .into_owned();
            if Path::new(&shell).is_file() {
                return shell;
            }
        }
    }
    "/bin/sh".into()
}
fn terminal_fonts(ctx: &egui::Context) {
    let mut definitions = egui::FontDefinitions::default();
    let mut preferred = Vec::new();
    // egui has no automatic system font fallback. Fontconfig resolves portable
    // family/coverage requests; each file is read once, before opening sessions.
    for (index, pattern) in ["monospace", ":charset=1fb00", "Symbols Nerd Font Mono"]
        .iter()
        .enumerate()
    {
        if let Ok(result) = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", pattern])
            .output()
        {
            if !result.status.success() {
                continue;
            }
            let path = String::from_utf8_lossy(&result.stdout);
            if let Ok(bytes) = std::fs::read(path.trim()) {
                let name = format!("terminal-system-{index}");
                definitions
                    .font_data
                    .insert(name.clone(), egui::FontData::from_owned(bytes).into());
                preferred.push(name);
            }
        }
    }
    let fallback = definitions
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default();
    preferred.append(fallback);
    *fallback = preferred;
    ctx.set_fonts(definitions);
}
fn palette() -> TerminalTheme {
    TerminalTheme::new(Box::new(ColorPalette {
        background: "#0f1417".into(),
        foreground: "#e9ecef".into(),
        black: "#171d20".into(),
        red: "#e95650".into(),
        green: "#a7c894".into(),
        yellow: "#e6b768".into(),
        blue: "#87b7cf".into(),
        magenta: "#cbb49a".into(),
        bright_magenta: "#e0c7aa".into(),
        dim_magenta: "#887564".into(),
        ..Default::default()
    }))
}
struct Session {
    backend: TerminalBackend,
    title: String,
    started: Instant,
    first_output: Option<Duration>,
    exited: bool,
}
impl Session {
    fn spawn(
        id: u64,
        ctx: &egui::Context,
        tx: Sender<(u64, PtyEvent)>,
        shell: &str,
        cwd: &Path,
    ) -> std::io::Result<Self> {
        let started = Instant::now();
        let backend = TerminalBackend::new(
            id,
            ctx.clone(),
            tx,
            BackendSettings {
                shell: shell.into(),
                working_directory: Some(cwd.into()),
                ..Default::default()
            },
        )?;
        Ok(Self {
            backend,
            title: Path::new(shell)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            started,
            first_output: None,
            exited: false,
        })
    }
}
struct Lab {
    en: bool,
    shell: String,
    cwd: PathBuf,
    tx: Sender<(u64, PtyEvent)>,
    rx: Receiver<(u64, PtyEvent)>,
    sessions: [Option<Session>; 2],
    visible: [bool; 2],
    active: Option<usize>,
    theme: TerminalTheme,
    font_size: f32,
    files: Vec<PathBuf>,
    selected: Option<PathBuf>,
    content: String,
    error: String,
    frames: VecDeque<f64>,
    frame_count: u64,
    last_stats: Instant,
    screenshot: Option<PathBuf>,
    capture_requested: bool,
    launched: Instant,
}
impl Lab {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = &cc.egui_ctx;
        terminal_fonts(ctx);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BG;
        visuals.window_fill = PANEL;
        visuals.extreme_bg_color = BG;
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::same(3);
        }
        visuals.selection.bg_fill = Color32::from_rgb(88, 43, 42);
        visuals.selection.stroke.color = ACCENT;
        ctx.set_visuals(visuals);
        ctx.style_mut_of(egui::Theme::Dark, |s| {
            s.spacing.button_padding = Vec2::new(10., 6.);
            s.spacing.item_spacing = Vec2::new(8., 8.);
        });
        let cwd = arg("--folder")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap())
            .canonicalize()
            .unwrap();
        let shell = arg("--shell").unwrap_or_else(system_shell);
        let files = directory(&cwd);
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            en: std::env::args().any(|a| a == "--en"),
            shell,
            cwd,
            tx,
            rx,
            sessions: [None, None],
            visible: [true, std::env::args().any(|a| a == "--both")],
            active: Some(0),
            theme: palette(),
            font_size: 14.,
            files,
            selected: None,
            content: String::new(),
            error: String::new(),
            frames: VecDeque::new(),
            frame_count: 0,
            last_stats: Instant::now(),
            screenshot: arg("--screenshot").map(PathBuf::from),
            capture_requested: false,
            launched: Instant::now(),
        };
        app.ensure(0, ctx);
        if let Some(command) = arg("--run") {
            if let Some(s) = &mut app.sessions[0] {
                s.backend
                    .process_command(BackendCommand::Write(format!("{command}\r").into_bytes()));
            }
        }
        if let Some(path) = arg("--file").map(PathBuf::from) {
            if std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= 128 * 1024) {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    app.selected = Some(path);
                    app.content = text;
                }
            }
        }
        app
    }
    fn ensure(&mut self, id: usize, ctx: &egui::Context) {
        if self.sessions[id].is_none() {
            match Session::spawn(id as u64, ctx, self.tx.clone(), &self.shell, &self.cwd) {
                Ok(s) => {
                    self.sessions[id] = Some(s);
                    self.error.clear();
                }
                Err(e) => self.error = e.to_string(),
            }
        }
    }
    fn terminal(&mut self, ui: &mut egui::Ui, id: usize) {
        self.ensure(id, ui.ctx());
        let en = self.en;
        let mut restart = false;
        ui.horizontal(|ui| {
            ui.strong(tr(
                en,
                if id == 0 {
                    "Бічна сесія"
                } else {
                    "Нижня сесія"
                },
                if id == 0 {
                    "Side session"
                } else {
                    "Bottom session"
                },
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr(en, "Сховати", "Hide")).clicked() {
                    self.visible[id] = false;
                    if self.active == Some(id) {
                        self.active = None;
                    }
                }
                restart = ui
                    .button(tr(en, "Нова", "New"))
                    .on_hover_text(tr(
                        en,
                        "Завершити цю сесію й запустити нову",
                        "End this session and start a new one",
                    ))
                    .clicked();
            });
        });
        ui.horizontal(|ui| {
            if ui.button(tr(en, "Вставити", "Paste")).clicked() {
                self.active = Some(id);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            if ui.button(tr(en, "Копіювати", "Copy")).clicked() {
                if let Some(s) = &self.sessions[id] {
                    ui.ctx().copy_text(s.backend.selectable_content());
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(session) = self.sessions[id].as_ref() {
                    ui.label(
                        RichText::new(format!(
                            "{} · PID {}",
                            session.title,
                            session.backend.pty_id()
                        ))
                        .small()
                        .weak(),
                    );
                }
            });
        });
        ui.separator();
        if restart {
            self.sessions[id] = None;
            self.ensure(id, ui.ctx());
            self.active = Some(id);
        }
        let rect = ui.available_rect_before_wrap();
        if ui.input(|i| {
            i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| rect.contains(p))
        }) {
            self.active = Some(id);
        }
        if let Some(s) = self.sessions[id].as_mut() {
            if s.exited {
                ui.label(tr(
                    en,
                    "Сесію завершено. Натисни «Нова», щоб відкрити оболонку.",
                    "Session ended. Choose New to start a shell.",
                ));
            } else {
                let focus = self.active == Some(id);
                let font = TerminalFont::new(FontSettings {
                    font_type: FontId::monospace(self.font_size),
                });
                let view = TerminalView::new(ui, &mut s.backend)
                    .set_theme(self.theme.clone())
                    .set_font(font)
                    .set_focus(focus)
                    .set_size(rect.size().max(Vec2::splat(20.)));
                ui.add(view);
                if s.first_output.is_none()
                    && s.backend
                        .last_content()
                        .grid
                        .display_iter()
                        .any(|c| c.c != ' ')
                {
                    s.first_output = Some(s.started.elapsed());
                }
            }
        } else {
            ui.colored_label(ACCENT, &self.error);
        }
    }
    fn workspace(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("files")
            .default_size(210.)
            .min_size(140.)
            .resizable(true)
            .show(ui, |ui| {
                ui.strong(tr(self.en, "Файли · лише перегляд", "Files · read only"));
                if ui
                    .button(tr(self.en, "На рівень вище", "Parent directory"))
                    .clicked()
                {
                    if let Some(parent) = self.cwd.parent() {
                        self.cwd = parent.into();
                        self.files = directory(&self.cwd);
                    }
                }
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for path in self.files.clone() {
                        let is_dir = path.is_dir();
                        let name = format!(
                            "{}{}",
                            path.file_name().unwrap_or_default().to_string_lossy(),
                            if is_dir { "/" } else { "" }
                        );
                        if ui
                            .selectable_label(self.selected.as_ref() == Some(&path), name)
                            .clicked()
                        {
                            self.active = None;
                            if is_dir {
                                self.cwd = path;
                                self.files = directory(&self.cwd);
                                break;
                            } else {
                                self.content = match std::fs::metadata(&path) {
                                    Ok(m) if m.len() <= 128 * 1024 => {
                                        std::fs::read_to_string(&path).unwrap_or_else(|_| {
                                            tr(
                                                self.en,
                                                "Двійковий файл — текстовий перегляд недоступний.",
                                                "Binary file — no text preview.",
                                            )
                                            .into()
                                        })
                                    }
                                    _ => tr(
                                        self.en,
                                        "Для прототипу перегляд обмежено файлами до 128 КіБ.",
                                        "Prototype previews are limited to files under 128 KiB.",
                                    )
                                    .into(),
                                };
                                self.selected = Some(path);
                            }
                        }
                    }
                });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(path) = &self.selected {
                ui.strong(path.file_name().unwrap_or_default().to_string_lossy());
                ui.separator();
                egui::ScrollArea::both().show(ui, |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&self.content).monospace())
                            .wrap_mode(egui::TextWrapMode::Extend)
                            .selectable(true),
                    );
                });
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.);
                    ui.heading(tr(
                        self.en,
                        "Файли поруч. Термінал під рукою.",
                        "Files nearby. Terminal at hand.",
                    ));
                    ui.add_space(12.);
                    ui.label(tr(
                        self.en,
                        "Обери файл ліворуч або працюй в оболонці.",
                        "Choose a file on the left, or use your shell.",
                    ));
                    ui.label(tr(
                        self.en,
                        "Це окремий прототип: файли тут лише для перегляду.",
                        "Separate prototype: file previews are read only.",
                    ));
                });
            }
        });
    }
}
fn directory(path: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .take(300)
        .collect();
    files.sort_by(|a, b| {
        b.is_dir()
            .cmp(&a.is_dir())
            .then_with(|| a.file_name().cmp(&b.file_name()))
    });
    files
}
impl eframe::App for Lab {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let started = Instant::now();
        while let Ok((id, event)) = self.rx.try_recv() {
            if let Some(s) = self.sessions.get_mut(id as usize).and_then(Option::as_mut) {
                match event {
                    PtyEvent::Exit => s.exited = true,
                    PtyEvent::Title(t) => s.title = t,
                    PtyEvent::ColorRequest(index, format) => s.backend.process_command(
                        BackendCommand::Write(format(self.theme.query_color(index)).into_bytes()),
                    ),
                    PtyEvent::TextAreaSizeRequest(format) => {
                        s.backend.process_command(BackendCommand::Write(
                            format(s.backend.last_content().terminal_size.into()).into_bytes(),
                        ))
                    }
                    _ => {}
                }
            }
        }
        if ui
            .ctx()
            .input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Backtick))
        {
            self.visible[1] = !self.visible[1];
            self.active = self.visible[1].then_some(1);
        }
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("disco").size(23.).strong());
                ui.label(
                    RichText::new(tr(self.en, "ТЕРМІНАЛ · ПРОТОТИП", "TERMINAL · PROTOTYPE"))
                        .small()
                        .color(ACCENT),
                );
                ui.separator();
                for (id, uk, en) in [(0, "Праворуч", "Right"), (1, "Знизу", "Bottom")]
                {
                    if ui
                        .selectable_label(self.visible[id], tr(self.en, uk, en))
                        .clicked()
                    {
                        self.visible[id] = !self.visible[id];
                        if self.visible[id] {
                            self.active = Some(id);
                        } else if self.active == Some(id) {
                            self.active = None;
                        }
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("A+").clicked() {
                        self.font_size = (self.font_size + 1.).min(24.);
                    }
                    if ui.small_button("A−").clicked() {
                        self.font_size = (self.font_size - 1.).max(10.);
                    }
                    ui.label(RichText::new(&self.shell).small().weak());
                });
            });
            ui.label(RichText::new(self.cwd.display().to_string()).small().weak());
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr(self.en, "Ctrl+Shift+C / V · Ctrl+C — перервати · Ctrl+` — нижня панель", "Ctrl+Shift+C / V · Ctrl+C interrupts · Ctrl+` bottom panel")).small().weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut samples: Vec<_> = self.frames.iter().copied().collect(); samples.sort_by(f64::total_cmp);
                    let p95 = samples.get(samples.len().saturating_sub(1) * 95 / 100).copied().unwrap_or_default();
                    ui.label(RichText::new(format!("UI p95 {p95:.2} ms")).small().weak()).on_hover_text(tr(self.en, "Час побудови UI останніх 120 кадрів; не включає GPU та затримку монітора.", "UI construction time over the last 120 frames; excludes GPU and display latency."));
                    if let Some(ms) = self.sessions.iter().flatten().filter_map(|s| s.first_output).min() { ui.label(RichText::new(format!("PTY {} ms", ms.as_millis())).small().weak()); }
                });
            });
        });
        if self.visible[1] {
            egui::Panel::bottom("bottom-terminal")
                .default_size(250.)
                .min_size(130.)
                .max_size(ui.available_height() * 0.65)
                .resizable(true)
                .show(ui, |ui| self.terminal(ui, 1));
        }
        if self.visible[0] {
            egui::Panel::right("right-terminal")
                .default_size(560.)
                .min_size(330.)
                .max_size(ui.available_width() * 0.75)
                .resizable(true)
                .show(ui, |ui| self.terminal(ui, 0));
        }
        self.workspace(ui);
        self.frame_count += 1;
        if self.frames.len() == 120 {
            self.frames.pop_front();
        }
        self.frames
            .push_back(started.elapsed().as_secs_f64() * 1000.);
        // No periodic animation/polling timer: PTY output and input wake the UI.
        if self.screenshot.is_some() && !self.capture_requested {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
            if self.launched.elapsed() > Duration::from_secs(2) {
                self.capture_requested = true;
                let path = self.screenshot.clone().unwrap();
                let ctx = ui.ctx().clone();
                ctx.clone().request_screenshot(move |img| {
                    let pixels: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
                    image::save_buffer(
                        path,
                        &pixels,
                        img.size[0] as u32,
                        img.size[1] as u32,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                });
            }
        }
        if self.last_stats.elapsed() >= Duration::from_secs(5) {
            eprintln!(
                "prototype frames={} live_sessions={} ui_last_ms={:.3}",
                self.frame_count,
                self.sessions.iter().flatten().filter(|s| !s.exited).count(),
                self.frames.back().unwrap_or(&0.)
            );
            self.last_stats = Instant::now();
        }
    }
}
fn main() -> eframe::Result {
    // Safe here: this is the process entry point, before any worker, PTY or UI
    // thread exists. Do not leak the launching agent's no-color mode into shells.
    unsafe {
        std::env::remove_var("NO_COLOR");
    }
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440., 880.])
            .with_min_inner_size([1000., 620.]),
        ..Default::default()
    };
    let mut opts = opts;
    opts.glow_options.vsync = true;
    let title = arg("--title").unwrap_or_else(|| "disco · Terminal prototype".into());
    eframe::run_native(&title, opts, Box::new(|cc| Ok(Box::new(Lab::new(cc)))))
}

#[cfg(test)]
mod tests;
