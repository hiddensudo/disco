use crate::app::state::View;
use crate::art;
use crate::filesystem::{Properties, drive_info};
use crate::frame_probe;
use crate::i18n;
use crate::index::{Category, Index};
use crate::motion;
use crate::navigation;
use crate::recycle;
use crate::scan::{Control, Event, Folder, background_priority};
use crate::settings;
use crate::settings::color;
use crate::snapshot;
use eframe::egui;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::{Duration, Instant};

mod editor;
mod jobs;
mod new_file;
mod scope;
mod state;
mod ui;
use self::{
    state::{BrowserState, DeletionState, DriveState, PreferencesState, ScanState},
    ui::resize_edges,
};
pub(crate) struct Disco {
    art: art::Art,
    collections: crate::categories::Manager,
    frame_probe: Option<frame_probe::FrameProbe>,
    scan: ScanState,
    browser: BrowserState,
    preferences: PreferencesState,
    drive: DriveState,
    deletion: DeletionState,
    recycle: recycle::Bin,
    files: crate::directory_files::Files,
    file_preview: crate::file_preview::Preview,
    editor: editor::Editor,
    scoped: scope::Scope,
    search: crate::file_search::Search,
    live: crate::live_files::LiveFiles,
    syntax: crate::syntax::Syntax,
    new_file: Option<new_file::Draft>,
    opening_file: Arc<std::sync::atomic::AtomicBool>,
    purge_targets: Option<Vec<recycle::Item>>,
    purge_fresh: bool,
    message: String,
    started: Instant,
    demo: bool,
    screenshot: Option<String>,
    qa_preview_file: Option<PathBuf>,
    shots: usize,
}
impl Disco {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        art::fonts(&cc.egui_ctx);
        let args: Vec<_> = std::env::args().collect();
        let demo = args.iter().any(|x| x == "--demo");
        let argument = |key: &str| {
            args.iter()
                .position(|x| x == key)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        // QA-only override: exercise fractional DPI in screenshots without
        // changing the user's desktop settings or stored application layout.
        if argument("--screenshot").is_some() {
            if let Ok(scale) = std::env::var("DISCO_QA_SCALE")
                .unwrap_or_default()
                .parse::<f32>()
            {
                if (0.75..=3.).contains(&scale) {
                    cc.egui_ctx.set_pixels_per_point(scale);
                }
            }
        }
        let (theme, mut layout, message) = settings::load();
        layout.normalize_file_mode();
        i18n::set_current(layout.language);
        let root = argument("--folder")
            .unwrap_or_else(|| std::env::var("HOME").unwrap_or_else(|_| "/home".into()));
        let (drive_name, drive_model) = if demo {
            (
                crate::tr!("Системний SSD", "System SSD").into(),
                crate::tr!("NVMe SSD · 1 ТБ", "NVMe SSD · 1 TB").into(),
            )
        } else {
            drive_info(Path::new(&root))
        };
        let report = demo.then(|| {
            let mut r = crate::demo::report();
            if args.iter().any(|a| a == "--frame-probe-large") {
                let base = r.folders[3].path.clone();
                for i in 0..100_000 {
                    r.folders.push(Folder {
                        path: base.join(format!("test-folder-{i:06}")),
                        parent: Some(3),
                        bytes: 0,
                        files: 0,
                    });
                }
            }
            r
        });
        let index = report.as_ref().map(Index::new).unwrap_or_default();
        let properties = demo.then(|| {
            Properties::load(
                PathBuf::from("/demo/home/alex/.cache/yay"),
                (14.8 * 1024f64.powi(3)) as u64,
                Some(12436),
                true,
            )
        });
        let settings_page = match argument("--settings").as_deref() {
            Some("layout") => 1,
            Some("categories") => 2,
            _ => 0,
        };
        let settings_open = argument("--settings").is_some();
        let mut s = Self {
            art: art::Art::load(&cc.egui_ctx),
            collections: crate::categories::Manager::load(),
            frame_probe: if demo {
                argument("--frame-probe")
                    .map(|p| frame_probe::FrameProbe::new(p.into(), index.folder_count()))
            } else {
                None
            },
            scan: ScanState {
                report: report.map(Arc::new),
                report_revision: 0,
                index,
                tracker: None,
                snapshot_at: None,
                saved_snapshot_at: None,
                restored_snapshot: false,
                restore_requested_root: None,
                automatic_job: false,
                rx: None,
                control: Arc::new(Control::default()),
                busy: false,
                duplicate_job: false,
                progress: None,
                dups: vec![],
            },
            browser: BrowserState {
                root,
                table_root: None,
                tree_revision: 0,
                rows_cache: None,
                history: navigation::History::default(),
                nav_traversed: false,
                focus: if demo { 6 } else { 0 },
                filter: String::new(),
                selected: HashSet::new(),
                expanded: HashSet::from([0, 3]),
                category: if demo { Category::Cache } else { Category::All },
                view: if args.iter().any(|a| a == "--trash") {
                    View::Trash
                } else {
                    View::Folders
                },
                properties,
                text_input_focused: false,
                scroll_to: None,
                scroll_offset: 0.,
                scroll_height: 300.,
                sort_size: true,
                descending: true,
            },
            preferences: PreferencesState {
                theme,
                layout,
                applied: None,
                open: settings_open,
                page: settings_page,
                snapshot: None,
                draft: None,
                error: String::new(),
            },
            drive: DriveState {
                name: drive_name,
                model: drive_model,
                space: None,
                device: None,
                checked_at: Instant::now() - Duration::from_secs(10),
            },
            deletion: DeletionState {
                confirm: false,
                targets: vec![],
                fresh: false,
                rx: None,
            },
            recycle: recycle::Bin::default(),
            files: crate::directory_files::Files::default(),
            file_preview: crate::file_preview::Preview::default(),
            new_file: None,
            scoped: scope::Scope::default(),
            search: crate::file_search::Search::default(),
            live: crate::live_files::LiveFiles::default(),
            syntax: crate::syntax::Syntax::default(),
            editor: editor::Editor::default(),
            opening_file: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            purge_targets: None,
            purge_fresh: false,
            message,
            started: Instant::now(),
            demo,
            screenshot: argument("--screenshot"),
            qa_preview_file: argument("--screenshot")
                .and_then(|_| argument("--preview-file"))
                .map(PathBuf::from),
            shots: 0,
        };
        if demo {
            s.recycle = recycle::Bin::demo();
            if s.browser.view == View::Trash {
                s.browser.properties = None;
            }
        }
        if s.preferences.open {
            s.preferences.snapshot =
                Some((s.preferences.theme.clone(), s.preferences.layout.clone()));
        }
        if !demo && s.preferences.layout.save_scan {
            let (tx, rx) = mpsc::channel();
            s.scan.rx = Some(rx);
            s.scan.restore_requested_root = Some(s.browser.root.clone());
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                background_priority();
                let result = snapshot::load().map(|saved| {
                    saved.map(|saved| {
                        let index = Index::new(&saved.report);
                        (saved, index)
                    })
                });
                let _ = tx.send(Event::Restored(result));
                ctx.request_repaint();
            });
        }
        s.apply_style(&cc.egui_ctx);
        s
    }
}
impl eframe::App for Disco {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.preferences.layout.normalize_file_mode();
        self.guard_native_close(ui.ctx());
        self.editor.focused = false;
        self.browser.text_input_focused = false;
        self.poll();
        self.poll_scope(ui.ctx());
        self.update_collections();
        if self.scan.restore_requested_root.is_none() && !self.scan.busy {
            if let Some(path) = self.qa_preview_file.take() {
                if let Some((report, parent)) = self.scan.report.as_ref().and_then(|r| {
                    r.folders
                        .iter()
                        .position(|f| Some(f.path.as_path()) == path.parent())
                        .map(|id| (r, id))
                }) {
                    self.browser.category = Category::All;
                    self.browser.focus = parent;
                    self.browser.table_root = Some(parent);
                    let mut at = Some(parent);
                    while let Some(id) = at {
                        self.browser.expanded.insert(id);
                        at = report.folders[id].parent;
                    }
                    self.browser.tree_revision = self.browser.tree_revision.wrapping_add(1);
                    if let Ok(m) = std::fs::symlink_metadata(&path) {
                        self.select_file(path, crate::scan::allocated(&m));
                    }
                }
            }
        }
        self.files.poll(self.scan.report_revision);
        self.poll_new_file(ui.ctx());
        self.sync_file_preview(ui.ctx());
        if let Some(result) = self.recycle.poll(ui.ctx()) {
            self.refresh_drive();
            if result.changed
                && self.scan.report.is_some()
                && !self.scan.busy
                && self.deletion.rx.is_none()
            {
                self.start_job(false, true, true);
            }
        }
        if !self.demo
            && self.deletion.rx.is_none()
            && !self.deletion.confirm
            && self.purge_targets.is_none()
            && (self.preferences.layout.preview_mode == settings::PreviewMode::Overview
                || self.browser.view == View::Trash)
        {
            self.recycle.maintain(ui.ctx());
        }
        let previous_location = self.nav_state();
        self.browser.nav_traversed = false;
        if !self.preferences.open
            && !self.deletion.confirm
            && self.purge_targets.is_none()
            && !self.scan.busy
            && !self.recycle.busy
            && self.editor.pending.is_none()
            && self.new_file.is_none()
            && self.editor.saving.is_none()
        {
            let (back, forward) = ui.input(|i| {
                (
                    i.pointer.button_pressed(egui::PointerButton::Extra1)
                        || (i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft)),
                    i.pointer.button_pressed(egui::PointerButton::Extra2)
                        || (i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight)),
                )
            });
            if back {
                self.history_step(false);
            } else if forward {
                self.history_step(true);
            }
        }

        self.apply_style(ui.ctx());
        let ctx = ui.ctx().clone();
        if self.scan.busy
            || self.deletion.rx.is_some()
            || self.scan.restore_requested_root.is_some()
            || self.recycle.loading
            || self.recycle.busy
            || self.editor.saving.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(200));
        } else {
            ctx.request_repaint_after(Duration::from_secs(3));
        }
        let t = self.preferences.theme.clone();
        egui::Panel::top("brand")
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.titlebar(ui));
        if self.preferences.layout.preview_mode == settings::PreviewMode::Focus
            && self.browser.view != View::Trash
        {
            egui::Panel::top("focus-navigation")
                .frame(
                    egui::Frame::new()
                        .fill(color(&t.panel))
                        .inner_margin(egui::Margin::symmetric(12, 8)),
                )
                .show(ui, |ui| self.scope_controls(ui));
        } else {
            egui::Panel::top("drive")
                .frame(egui::Frame::NONE)
                .show(ui, |ui| self.drive_header(ui));
        }
        egui::Panel::bottom("selection")
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.footer(ui));
        if self.preferences.layout.categories {
            egui::Panel::left("categories")
                .exact_size(if self.preferences.layout.category_labels {
                    self.preferences.layout.category_width
                } else {
                    52.
                })
                .resizable(false)
                .frame(egui::Frame::new().fill(color(&t.panel)).inner_margin(0.))
                .show(ui, |ui| self.sidebar(ui));
        }
        if self.preferences.layout.preview_mode == settings::PreviewMode::Overview
            || self.browser.view == View::Trash
        {
            self.show_inspector_panel(ui);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(color(&t.background)).inner_margin(
                if self.preferences.layout.preview_mode == settings::PreviewMode::Focus {
                    egui::Margin::ZERO
                } else {
                    egui::Margin::symmetric(12, 8)
                },
            ))
            .show(ui, |ui| {
                if self.browser.view == View::Trash {
                    self.recycle_content(ui);
                    return;
                }
                if !self.preferences.layout.categories {
                    ui.horizontal(|ui| {
                        for cat in [
                            Category::All,
                            Category::Games,
                            Category::Development,
                            Category::Cache,
                            Category::Documents,
                            Category::Other,
                        ] {
                            if ui
                                .selectable_label(self.browser.category == cat, cat.name())
                                .clicked()
                            {
                                self.browser.category = cat;
                                self.browser.view = View::Folders;
                                self.browser.table_root = None;
                            }
                        }
                    });
                }
                let enter = motion::enter(
                    ui,
                    "content-enter",
                    (
                        self.nav_state(),
                        self.browser.sort_size,
                        self.browser.descending,
                    ),
                );
                ui.scope(|ui| {
                    ui.multiply_opacity(0.55 + enter * 0.45);

                    self.browser_content(ui);
                });
            });
        if self.browser.view == View::Trash
            && previous_location.view != View::Trash
            && !self.demo
            && !self.recycle.loading
            && !self.recycle.busy
        {
            self.recycle.refresh(&ctx);
        }
        self.keyboard_navigation(ui);
        self.confirmation(&ctx);
        self.bin_confirmation(&ctx);
        self.settings_window(&ctx);
        self.editor_confirmation(&ctx);
        self.new_file_dialog(&ctx);
        if i18n::current() != self.preferences.layout.language {
            i18n::set_current(self.preferences.layout.language);
            self.message.clear();
            self.recycle.relocalize_notice();
            self.refresh_drive();
            ctx.request_repaint();
        }
        if self.browser.nav_traversed {
            ctx.request_repaint();
        }
        if self.scan.report.is_some()
            && !self.scan.busy
            && !self.browser.nav_traversed
            && self.nav_state() != previous_location
        {
            self.browser.history.visit(previous_location);
            ctx.request_repaint();
        }

        if let Some(probe) = &mut self.frame_probe {
            probe.tick(ui, frame.info().cpu_usage);
        }
        resize_edges(ui);
        if let Some(path) = self.screenshot.clone() {
            if self.shots == 0
                && self.scan.restore_requested_root.is_none()
                && self.started.elapsed() > Duration::from_millis(1500)
            {
                self.shots = 1;
                let c = ctx.clone();
                ctx.request_screenshot(move |image| {
                    let data: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Err(e) = image::save_buffer(
                        &path,
                        &data,
                        image.size[0] as u32,
                        image.size[1] as u32,
                        image::ColorType::Rgba8,
                    ) {
                        eprintln!("{e}");
                    }
                    c.send_viewport_cmd(egui::ViewportCommand::Close);
                });
            }
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.scan.control.cancel.store(true, Ordering::Relaxed);
    }
}
