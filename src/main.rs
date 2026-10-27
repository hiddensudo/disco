mod art;
mod deletion;
mod frame_probe;
mod i18n;
mod motion;
mod navigation;
mod recycle;
mod recycle_ui;
mod refresh;
mod scan;
mod settings;
mod snapshot;
use eframe::egui::{self, Align2, Color32 as C, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use scan::*;
use settings::{Layout, Theme, color};
use std::os::unix::fs::MetadataExt;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::Ordering,
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};
#[derive(Clone, Copy, PartialEq, Hash)]
enum View {
    Folders,
    Files,
    Cleanup,
    Duplicates,
    Trash,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Category {
    All,
    Games,
    Development,
    Cache,
    Documents,
    Other,
}
impl Category {
    fn name(self) -> &'static str {
        match self {
            Self::All => crate::tr!("Усі папки", "All folders"),
            Self::Games => crate::tr!("Ігри та моди", "Games & mods"),
            Self::Development => crate::tr!("Розробка", "Development"),
            Self::Cache => crate::tr!("Кеші", "Caches"),
            Self::Documents => crate::tr!("Документи", "Documents"),
            Self::Other => crate::tr!("Інше", "Other"),
        }
    }
}
fn classify(path: &Path) -> Category {
    let parts: Vec<_> = path
        .components()
        .map(|s| s.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    if parts.iter().any(|s| s == ".cache") {
        Category::Cache
    } else if parts
        .iter()
        .any(|s| matches!(s.as_str(), "games" | "steam" | "faugus" | "heroic"))
    {
        Category::Games
    } else if parts.iter().any(|s| {
        matches!(
            s.as_str(),
            "workspace" | "projects" | "node_modules" | ".cargo" | ".rustup"
        )
    }) {
        Category::Development
    } else if parts.iter().any(|s| {
        matches!(
            s.as_str(),
            "documents" | "downloads" | "pictures" | "music" | "videos" | "desktop"
        )
    }) {
        Category::Documents
    } else {
        Category::Other
    }
}
#[derive(Debug, Default)]
pub(crate) struct Index {
    children: Vec<Vec<usize>>,
    roots: HashMap<Category, Vec<usize>>,
    totals: HashMap<Category, u64>,
    cleanup_ids: Vec<usize>,
    classes: Vec<Category>,
}
impl Index {
    fn new(r: &Report) -> Self {
        let mut s = Self {
            children: vec![vec![]; r.folders.len()],
            ..Default::default()
        };
        let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
        let cleanup_roots = [
            home.join(".cache/yay"),
            home.join(".cache/spotify"),
            home.join(".cache/spotifast"),
            home.join(".cache/google-chrome"),
        ];
        for (i, f) in r.folders.iter().enumerate() {
            if r.complete && f.bytes > 4096 && cleanup_roots.contains(&f.path) {
                s.cleanup_ids.push(i);
            }
            if let Some(p) = f.parent {
                s.children[p].push(i);
            }
        }
        let classes: Vec<_> = r.folders.iter().map(|f| classify(&f.path)).collect();
        let mut uniform: Vec<_> = classes.iter().copied().map(Some).collect();
        for i in (0..r.folders.len()).rev() {
            if s.children[i]
                .iter()
                .any(|c| uniform[*c] != Some(classes[i]))
            {
                uniform[i] = None;
            }
        }
        for (i, f) in r.folders.iter().enumerate() {
            let child: u64 = s.children[i].iter().map(|j| r.folders[*j].bytes).sum();
            *s.totals.entry(classes[i]).or_default() += f.bytes.saturating_sub(child);
            if let Some(cat) = uniform[i] {
                if f.parent.map(|p| uniform[p] != Some(cat)).unwrap_or(true) {
                    s.roots.entry(cat).or_default().push(i);
                }
            }
        }
        s.classes = classes;
        s.roots.insert(
            Category::All,
            if r.folders.is_empty() {
                vec![]
            } else {
                vec![0]
            },
        );
        s
    }
}
#[derive(Debug, Default)]
pub(crate) struct RefreshPaths {
    table: Option<PathBuf>,
    focus: PathBuf,
    expanded: HashSet<PathBuf>,
}
#[derive(Debug)]
pub(crate) struct ResolvedView {
    table: Option<usize>,
    focus: usize,
    expanded: HashSet<usize>,
}
impl RefreshPaths {
    pub(crate) fn resolve(self, r: &Report) -> ResolvedView {
        let mut v = ResolvedView {
            table: None,
            focus: 0,
            expanded: HashSet::from([0]),
        };
        for (i, f) in r.folders.iter().enumerate() {
            if self.table.as_ref() == Some(&f.path) {
                v.table = Some(i);
            }
            if self.focus == f.path {
                v.focus = i;
            }
            if self.expanded.contains(&f.path) {
                v.expanded.insert(i);
            }
        }
        v
    }
}
#[derive(Clone)]
struct Properties {
    path: PathBuf,
    bytes: u64,
    files: Option<u64>,
    modified: String,
    created: String,
    owner: String,
    permissions: String,
    is_dir: bool,
    error: Option<String>,
}
impl Properties {
    fn load(path: PathBuf, bytes: u64, files: Option<u64>, demo: bool) -> Self {
        let mut p = Self {
            path,
            bytes,
            files,
            modified: "—".into(),
            created: "—".into(),
            owner: "—".into(),
            permissions: "—".into(),
            is_dir: true,
            error: None,
        };
        if demo {
            p.modified = "12.10.2026, 14:27".into();
            p.owner = "alex".into();
            p.permissions = "rwxr-xr-x".into();
            return p;
        }
        match fs::symlink_metadata(&p.path) {
            Ok(m) => {
                p.is_dir = m.is_dir();
                p.modified = m.modified().map(date).unwrap_or_else(|_| "—".into());
                p.created = m.created().map(date).unwrap_or_else(|_| "—".into());
                p.owner = if m.uid() == unsafe { libc::geteuid() } {
                    std::env::var("USER").unwrap_or_else(|_| m.uid().to_string())
                } else {
                    m.uid().to_string()
                };
                p.permissions = permissions(m.mode());
            }
            Err(e) => p.error = Some(e.to_string()),
        }
        p
    }
}
fn permissions(mode: u32) -> String {
    (0..9)
        .map(|i| {
            if mode & (1 << (8 - i)) != 0 {
                ['r', 'w', 'x'][i % 3]
            } else {
                '-'
            }
        })
        .collect()
}
fn date(t: std::time::SystemTime) -> String {
    let d: chrono::DateTime<chrono::Local> = t.into();
    d.format("%d.%m.%Y, %H:%M").to_string()
}
fn label(p: &Path) -> String {
    p.file_name()
        .unwrap_or(p.as_os_str())
        .to_string_lossy()
        .into_owned()
}
fn drive_info(path: &Path) -> (String, String) {
    let Some(m) = fs::metadata(path).ok() else {
        return (crate::tr!("Диск", "Drive").into(), String::new());
    };
    let dev = m.dev();
    let sys = PathBuf::from(format!(
        "/sys/dev/block/{}:{}",
        libc::major(dev),
        libc::minor(dev)
    ));
    let Ok(sys) = sys.canonicalize() else {
        return (crate::tr!("Диск", "Drive").into(), String::new());
    };
    let mut model = String::new();
    let mut ssd = false;
    for p in [
        Some(sys.as_path()),
        sys.parent(),
        sys.parent().and_then(Path::parent),
    ]
    .into_iter()
    .flatten()
    {
        if model.is_empty() {
            model = fs::read_to_string(p.join("device/model"))
                .unwrap_or_default()
                .trim()
                .to_string();
        }
        if fs::read_to_string(p.join("queue/rotational"))
            .map(|s| s.trim() == "0")
            .unwrap_or(false)
        {
            ssd = true;
        }
    }
    if model == "CT1000P3SSD8" {
        model = "Crucial P3 · 1 TB".into();
    }
    (
        if ssd {
            crate::tr!("Системний SSD", "System SSD")
        } else {
            crate::tr!("Системний диск", "System drive")
        }
        .into(),
        model,
    )
}
#[derive(Clone, Copy, PartialEq, Hash)]
struct NavState {
    view: View,
    category: Category,
    folder: Option<usize>,
}
type FolderRows = Arc<Vec<(usize, usize)>>;
#[derive(PartialEq)]
struct RowsKey {
    location: NavState,
    filter: String,
    sort_size: bool,
    descending: bool,
    report: u64,
    tree: u64,
}
struct Disco {
    art: art::Art,
    table_root: Option<usize>,
    root: String,
    report: Option<Report>,
    report_revision: u64,
    frame_probe: Option<frame_probe::FrameProbe>,
    tree_revision: u64,
    rows_cache: Option<(RowsKey, FolderRows)>,
    tracker: Option<ChangeTracker>,
    snapshot_at: Option<Instant>,
    saved_snapshot_at: Option<u64>,
    restored_snapshot: bool,
    auto_refresh: refresh::AutoRefresh,
    automatic_job: bool,
    restore_requested_root: Option<String>,
    history: navigation::History<NavState>,
    nav_traversed: bool,
    rx: Option<Receiver<Event>>,
    control: Arc<Control>,
    busy: bool,
    duplicate_job: bool,
    progress: Option<Progress>,
    focus: usize,
    filter: String,
    selected: HashSet<PathBuf>,
    confirm: bool,
    delete_targets: Vec<deletion::Target>,
    confirm_fresh: bool,
    recycle: recycle::Bin,
    purge_targets: Option<Vec<recycle::Item>>,
    purge_fresh: bool,
    message: String,
    dups: Vec<Vec<BigFile>>,
    started: Instant,
    demo: bool,
    screenshot: Option<String>,
    shots: usize,
    trash_rx: Option<Receiver<deletion::Outcome>>,
    theme: Theme,
    layout: Layout,
    applied: Option<(Theme, Layout)>,
    settings_open: bool,
    settings_page: usize,
    settings_snapshot: Option<(Theme, Layout)>,
    theme_draft: Option<Theme>,
    settings_error: String,
    index: Index,
    expanded: HashSet<usize>,
    category: Category,
    view: View,
    properties: Option<Properties>,
    sort_size: bool,
    descending: bool,
    drive_name: String,
    drive_model: String,
    space: Option<(u64, u64)>,
    space_device: Option<u64>,
    space_at: Instant,
}
fn demo_report() -> Report {
    let base = PathBuf::from("/demo/home/alex");
    let mut folders = vec![Folder {
        path: base.clone(),
        parent: None,
        bytes: 0,
        files: 0,
    }];
    let gib = 1024u64.pow(3);
    for (n, b) in [
        ("Games", 355 * gib),
        ("workspace", 23 * gib),
        (".cache", 32 * gib),
        ("Documents", 8 * gib),
        ("other", 192 * gib),
    ] {
        folders.push(Folder {
            path: base.join(n),
            parent: Some(0),
            bytes: b,
            files: 1000,
        });
    }
    for (n, b) in [
        ("yay", 14.8),
        ("spotify", 4.3),
        ("pip", 2.1),
        ("thumbnails", 1.9),
        ("mesa_shader_cache", 1.6),
        ("google-chrome", 1.2),
        ("other", 6.1),
    ] {
        folders.push(Folder {
            path: base.join(".cache").join(n),
            parent: Some(3),
            bytes: (b * gib as f64) as u64,
            files: if n == "yay" { 12436 } else { 1000 },
        });
    }
    folders.push(Folder {
        path: base.join(".cache/yay/src"),
        parent: Some(6),
        bytes: 8 * gib,
        files: 8000,
    });
    folders[0].bytes = 610 * gib;
    Report {
        folders,
        largest: vec![BigFile {
            bytes: 5 * gib,
            path: base.join("other/model.gguf"),
        }],
        entries: 184229,
        elapsed: Duration::from_secs(18),
        complete: true,
        ..Default::default()
    }
}
impl Disco {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
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
        let (theme, layout, message) = settings::load();
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
            let mut r = demo_report();
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
        let settings_page = if argument("--settings").as_deref() == Some("layout") {
            1
        } else {
            0
        };
        let settings_open = argument("--settings").is_some();
        let mut s = Self {
            art: art::Art::load(&cc.egui_ctx),
            table_root: None,
            root,
            report,
            report_revision: 0,
            frame_probe: if demo {
                argument("--frame-probe")
                    .map(|p| frame_probe::FrameProbe::new(p.into(), index.children.len()))
            } else {
                None
            },
            tree_revision: 0,
            rows_cache: None,
            tracker: None,
            snapshot_at: None,
            saved_snapshot_at: None,
            restored_snapshot: false,
            auto_refresh: refresh::AutoRefresh::default(),
            automatic_job: false,
            restore_requested_root: None,
            history: navigation::History::default(),
            nav_traversed: false,
            rx: None,
            control: Arc::new(Control::default()),
            busy: false,
            duplicate_job: false,
            progress: None,
            focus: if demo { 6 } else { 0 },
            filter: String::new(),
            selected: HashSet::new(),
            confirm: false,
            delete_targets: vec![],
            confirm_fresh: false,
            recycle: recycle::Bin::default(),
            purge_targets: None,
            purge_fresh: false,
            message,
            dups: vec![],
            started: Instant::now(),
            demo,
            screenshot: argument("--screenshot"),
            shots: 0,
            trash_rx: None,
            theme,
            layout,
            applied: None,
            settings_open,
            settings_page,
            settings_snapshot: None,
            theme_draft: None,
            settings_error: String::new(),
            index,
            expanded: HashSet::from([0, 3]),
            category: if demo { Category::Cache } else { Category::All },
            view: if args.iter().any(|a| a == "--trash") {
                View::Trash
            } else {
                View::Folders
            },
            properties,
            sort_size: true,
            descending: true,
            drive_name,
            drive_model,
            space: None,
            space_device: None,
            space_at: Instant::now() - Duration::from_secs(10),
        };
        if demo {
            s.recycle = recycle::Bin::demo();
            if s.view == View::Trash {
                s.properties = None;
            }
        }
        if s.settings_open {
            s.settings_snapshot = Some((s.theme.clone(), s.layout.clone()));
        }
        if !demo && s.layout.save_scan {
            let (tx, rx) = mpsc::channel();
            s.rx = Some(rx);
            s.restore_requested_root = Some(s.root.clone());
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
    fn apply_style(&mut self, ctx: &egui::Context) {
        if self.applied.as_ref() == Some(&(self.theme.clone(), self.layout.clone())) {
            return;
        }
        let t = &self.theme;
        let mut s = egui::Style::default();
        s.visuals = egui::Visuals::dark();
        s.animation_time = if self.layout.animations {
            0.16 / self.layout.motion_speed
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
        let scale = self.layout.text_scale;
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
            .applied
            .as_ref()
            .is_some_and(|(_, old)| old.animations != self.layout.animations)
        {
            ctx.clear_animations();
        }
        ctx.set_theme(egui::Theme::Dark);
        ctx.set_style_of(egui::Theme::Dark, s);
        self.applied = Some((self.theme.clone(), self.layout.clone()));
    }
    fn start(&mut self, dup: bool, force: bool) {
        self.start_job(dup, force, false);
    }
    fn start_job(&mut self, dup: bool, force: bool, automatic: bool) {
        if self.busy || self.trash_rx.is_some() || self.recycle.busy {
            return;
        }
        self.restore_requested_root = None;
        let path = PathBuf::from(&self.root);
        if !path.is_absolute() {
            self.message = crate::tr!(
                "Вкажи абсолютний шлях до папки",
                "Enter an absolute folder path"
            )
            .into();
            return;
        }
        let path = match path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                if automatic {
                    self.auto_refresh.failed(Instant::now());
                }
                self.message = e.to_string();
                return;
            }
        };
        let same_snapshot = self
            .report
            .as_ref()
            .is_some_and(|r| r.complete && r.folders.first().is_some_and(|f| f.path == path));
        if !dup
            && !force
            && !self.demo
            && self.layout.reuse_scan
            && same_snapshot
            && self.tracker.as_mut().is_some_and(ChangeTracker::is_clean)
        {
            self.message = crate::tr!(
                "Подій змін немає · попередній результат ({} с тому)",
                "No changes detected · previous result ({} s ago)",
                self.snapshot_at.map(|t| t.elapsed().as_secs()).unwrap_or(0)
            );
            self.refresh_drive();
            return;
        }
        let restore_view = if automatic {
            self.report.as_ref().map(|r| RefreshPaths {
                table: self
                    .table_root
                    .and_then(|i| r.folders.get(i))
                    .map(|f| f.path.clone()),
                focus: r
                    .folders
                    .get(self.focus)
                    .map(|f| f.path.clone())
                    .unwrap_or_default(),
                expanded: self
                    .expanded
                    .iter()
                    .filter_map(|i| r.folders.get(*i))
                    .map(|f| f.path.clone())
                    .collect(),
            })
        } else {
            None
        };
        self.automatic_job = automatic;
        self.restored_snapshot = false;
        self.root = path.to_string_lossy().into_owned();
        if !automatic {
            self.table_root = None;
        }
        self.refresh_drive();
        self.control = Arc::new(Control::default());
        self.control
            .gentle
            .store(automatic || self.layout.gentle_scan, Ordering::Relaxed);
        self.busy = true;
        self.duplicate_job = dup;
        self.progress = None;
        self.message.clear();
        self.started = Instant::now();
        self.selected.clear();
        self.demo = false;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        if dup {
            self.dups.clear();
            let c = self.control.clone();
            std::thread::spawn(move || {
                background_priority();
                let event = match duplicates(&path, &c, |p| {
                    let _ = tx.send(Event::Progress(p));
                }) {
                    Ok((d, n)) => Event::Duplicates(d, n),
                    Err(e) => Event::Error(e.to_string()),
                };
                let _ = tx.send(event);
            });
        } else {
            self.tracker = None;
            self.history.clear();
            if !same_snapshot {
                self.report = None;
                self.index = Index::default();
            }
            self.dups.clear();
            if !automatic {
                self.focus = 0;
            }
            spawn_scan(
                path,
                self.control.clone(),
                tx,
                self.layout.save_scan,
                restore_view,
            );
        }
    }
    fn poll(&mut self) {
        let events: Vec<_> = self
            .rx
            .as_ref()
            .map(|r| r.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Progress(p) => self.progress = Some(p),
                Event::Restored(result) => {
                    let expected = self.restore_requested_root.take();
                    if expected.as_deref() != Some(&self.root) || self.busy || self.report.is_some()
                    {
                        continue;
                    }
                    match result {
                        Ok(Some((saved, index))) => {
                            let matches_requested = std::env::args().any(|a| a == "--folder");
                            if matches_requested
                                && saved.report.folders[0].path != PathBuf::from(&self.root)
                            {
                                continue;
                            }
                            self.root = saved.report.folders[0].path.to_string_lossy().into_owned();
                            self.index = index;
                            self.auto_refresh.restore(saved.disk);
                            self.report_revision = self.report_revision.wrapping_add(1);
                            self.report = Some(saved.report);
                            self.saved_snapshot_at = Some(saved.saved_at);
                            self.restored_snapshot = true;
                            self.tracker = None;
                            self.history.clear();
                            self.table_root = None;
                            self.expanded = HashSet::from([0]);
                            self.category = Category::All;
                            if self.view != View::Trash {
                                self.view = View::Folders;
                            }
                            self.message = crate::tr!(
                                "Відновлено останній знімок сканування.",
                                "Restored the last scan snapshot."
                            )
                            .into();
                            self.refresh_drive();
                        }
                        Ok(None) => {}
                        Err(e) => {
                            self.message = crate::tr!(
                                "Не вдалося відновити скан: {e}",
                                "Could not restore the scan: {e}",
                                e = e
                            )
                        }
                    }
                }
                Event::Finished(r, index, tracker, saved, resolved) => {
                    if !r.complete && self.report.as_ref().is_some_and(|old| old.complete) {
                        self.busy = false;
                        self.auto_refresh.failed(Instant::now());
                        self.automatic_job = false;
                        self.tracker = None;
                        self.restored_snapshot = true;
                        self.message = crate::tr!(
                            "Оновлення скасовано. Залишено попередній повний скан.",
                            "Refresh cancelled. Kept the previous complete scan."
                        )
                        .into();
                        continue;
                    }
                    if r.complete {
                        self.auto_refresh.completed(
                            refresh::DiskSample::capture(Path::new(&self.root)),
                            Instant::now(),
                        );
                    } else {
                        self.auto_refresh.failed(Instant::now());
                    }
                    self.automatic_job = false;
                    self.saved_snapshot_at = saved.as_ref().ok().copied().flatten();
                    self.restored_snapshot = false;
                    self.tracker = tracker;
                    self.snapshot_at = Some(Instant::now());
                    self.history.clear();
                    self.busy = false;
                    self.message = if r.complete {
                        crate::tr!(
                            "Завершено за {:.1} с · пропущено: {} · повторних hardlink: {}",
                            "Finished in {:.1} s · skipped: {} · duplicate hardlinks: {}",
                            r.elapsed.as_secs_f64(),
                            r.skipped,
                            r.hardlinks
                        ) + if self.tracker.is_some() {
                            crate::tr!(" · перевірка змін активна", " · change monitoring active")
                        } else {
                            crate::tr!(" · повторний обхід повністю", " · full rescan required")
                        }
                    } else {
                        crate::tr!(
                            "Сканування скасовано. Показано частковий результат.",
                            "Scan cancelled. Showing partial results."
                        )
                        .into()
                    };
                    if let Err(error) = saved {
                        self.message.push_str(&crate::tr!(
                            " · не збережено: {error}",
                            " · not saved: {error}",
                            error = error
                        ));
                    }
                    self.report_revision = self.report_revision.wrapping_add(1);
                    self.index = index;
                    if let Some(v) = resolved {
                        self.table_root = v.table;
                        self.focus = v.focus;
                        self.expanded = v.expanded;
                    } else {
                        self.expanded = HashSet::from([0]);
                    }
                    self.tree_revision = self.tree_revision.wrapping_add(1);
                    self.properties = None;
                    self.report = Some(r);
                }
                Event::Duplicates(d, n) => {
                    self.busy = false;
                    self.message = crate::tr!(
                        "Знайдено {} груп · пропущено або змінено: {n}",
                        "Found {} groups · skipped or changed: {n}",
                        d.len(),
                        n = n
                    );
                    self.dups = d;
                }
                Event::Error(e) => {
                    self.auto_refresh.failed(Instant::now());
                    self.automatic_job = false;
                    self.restored_snapshot = self.report.is_some();
                    self.busy = false;
                    self.message = e;
                }
            }
        }
        if let Some(rx) = &self.trash_rx {
            if let Ok(result) = rx.try_recv() {
                self.trash_rx = None;
                if !result.done.is_empty() {
                    self.recycle.invalidate();
                    self.selected.clear();
                    self.properties = None;
                    self.tracker = None;
                    // A selected folder can also be the current scan root.
                    let mut root = PathBuf::from(&self.root);
                    while !root.is_dir() {
                        if !root.pop() {
                            break;
                        }
                    }
                    self.root = root.to_string_lossy().into_owned();
                    self.start_job(false, true, true);
                    self.message = crate::tr!(
                        "У кошику: {} папок. Оновлюємо список. Місце звільниться після спорожнення кошика.",
                        "Moved {} folders to Trash. Refreshing the list. Empty Trash to free disk space.",
                        result.done.len()
                    );
                }
                if let Some(error) = result.error {
                    self.message = crate::tr!(
                        "У кошику: {} папок. Не вдалося завершити: {error}",
                        "Moved {} folders to Trash. Could not finish: {error}",
                        result.done.len(),
                        error = error
                    );
                }
            }
        }
    }
    fn request_delete(&mut self, folders: Vec<(PathBuf, u64)>) {
        if self.demo
            || self.busy
            || self.trash_rx.is_some()
            || self.recycle.busy
            || folders.is_empty()
        {
            return;
        }
        let targets: Result<Vec<_>, _> = folders
            .into_iter()
            .map(|(p, b)| deletion::Target::capture(&p, b))
            .collect();
        match targets {
            Ok(targets) => {
                self.delete_targets = targets;
                self.confirm_fresh = true;
                self.confirm = true;
            }
            Err(e) => {
                self.message = crate::tr!(
                    "Не можна видалити папку: {e}",
                    "Cannot delete folder: {e}",
                    e = e
                )
            }
        }
    }
    fn confirmation(&mut self, ctx: &egui::Context) {
        if !self.confirm {
            return;
        }
        let modal = delete_dialog(ctx, &self.delete_targets, &self.theme, self.confirm_fresh);
        let cancel = modal.inner.0.clicked();
        let yes = modal.inner.1.clicked();
        self.confirm_fresh = false;
        if cancel || modal.should_close() {
            self.confirm = false;
            self.delete_targets.clear();
        } else if yes {
            self.confirm = false;
            let targets = std::mem::take(&mut self.delete_targets);
            let (tx, rx) = mpsc::channel();
            self.trash_rx = Some(rx);
            self.message =
                crate::tr!("Переміщуємо папку у кошик…", "Moving folder to Trash…").into();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(deletion::move_to_trash(targets));
                ctx.request_repaint();
            });
        }
    }
    fn candidates(&self) -> Vec<Folder> {
        self.report
            .as_ref()
            .filter(|r| r.complete)
            .map(|r| {
                self.index
                    .cleanup_ids
                    .iter()
                    .filter_map(|i| r.folders.get(*i).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }
    fn select_folder(&mut self, id: usize) {
        self.focus = id;
        if let Some(r) = &self.report {
            let f = &r.folders[id];
            self.properties = Some(Properties::load(
                f.path.clone(),
                f.bytes,
                r.complete.then_some(f.files),
                self.demo,
            ));
        }
    }
    fn titlebar(&mut self, ui: &mut egui::Ui) {
        let t = &self.theme;
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 42.),
            Sense::click_and_drag(),
        );
        ui.painter()
            .rect_filled(rect, 0, mix(color(&t.panel), color(&t.raised), 0.35));
        ui.painter().hline(
            rect.x_range(),
            rect.bottom(),
            Stroke::new(1., color(&t.border)),
        );
        let logo_rect = Rect::from_min_size(rect.min + Vec2::new(17., 9.), Vec2::splat(25.));
        paint_logo(ui, logo_rect, color(&t.accent));
        ui.painter().text(
            rect.min + Vec2::new(52., 21.),
            Align2::LEFT_CENTER,
            "disco",
            art::bold(23.),
            color(&t.text),
        );
        ui.painter().text(
            rect.min + Vec2::new(129., 23.),
            Align2::LEFT_CENTER,
            env!("CARGO_PKG_VERSION"),
            FontId::proportional(11.),
            color(&t.muted),
        );
        for (slot, icon, help) in [
            (0, 21, crate::tr!("Закрити", "Close")),
            (1, 20, crate::tr!("Розгорнути", "Maximize")),
            (2, 19, crate::tr!("Згорнути", "Minimize")),
            (3, 7, crate::tr!("Налаштування", "Settings")),
        ] {
            let r = Rect::from_min_size(
                Pos2::new(rect.right() - 44. * (slot as f32 + 1.), rect.top() + 3.),
                Vec2::new(42., 36.),
            );
            let b = ui
                .interact(r, ui.scope_id().with(("chrome", slot)), Sense::click())
                .on_hover_text(help);
            let hover = motion::hover(ui, ("chrome-hover", slot), b.hovered());
            if hover > 0. {
                ui.painter().rect_filled(
                    r,
                    t.radius,
                    mix(
                        mix(color(&t.panel), color(&t.raised), 0.35),
                        color(&t.raised),
                        hover,
                    ),
                );
            }
            self.art.paint(
                ui,
                icon,
                Rect::from_center_size(r.center(), Vec2::splat(if slot == 3 { 19. } else { 15. })),
                color(&t.text),
            );
            if b.clicked() {
                match slot {
                    0 => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                    1 => {
                        let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                    }
                    2 => ui
                        .ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                    _ => {
                        self.settings_snapshot = Some((self.theme.clone(), self.layout.clone()));
                        self.theme_draft = None;
                        self.settings_error.clear();
                        self.settings_open = true;
                    }
                }
            }
        }
        if response.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if response.double_clicked() {
            let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
        }
    }
    fn drive_header(&mut self, ui: &mut egui::Ui) {
        if self.space_at.elapsed() > Duration::from_secs(3) {
            self.space_device = None;
            self.space = if self.demo {
                self.space_device = Some(0);
                Some((915 * 1024u64.pow(3), 305 * 1024u64.pow(3)))
            } else {
                refresh::DiskSample::capture(Path::new(&self.root)).map(|sample| {
                    self.space_device = Some(sample.device);
                    if !self.busy {
                        self.auto_refresh.observe(sample, Instant::now());
                    }
                    (sample.total, sample.total.saturating_sub(sample.used))
                })
            };
            self.space_at = Instant::now();
        }
        let t = self.theme.clone();
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 84.), Sense::hover());
        ui.painter()
            .rect_filled(rect, 0, mix(color(&t.panel), color(&t.raised), 0.12));
        ui.painter().hline(
            rect.x_range(),
            rect.bottom(),
            Stroke::new(1., color(&t.border)),
        );
        self.art.paint(
            ui,
            8,
            Rect::from_min_size(rect.min + Vec2::new(24., 10.), Vec2::splat(56.)),
            C::WHITE,
        );
        let name = if self.layout.drive_name.trim().is_empty() {
            &self.drive_name
        } else {
            &self.layout.drive_name
        };
        ui.painter().text(
            rect.min + Vec2::new(88., if self.layout.drive_model { 18. } else { 30. }),
            Align2::LEFT_TOP,
            name.chars().take(23).collect::<String>(),
            art::bold(19.),
            color(&t.text),
        );
        if self.layout.drive_model {
            ui.painter().text(
                rect.min + Vec2::new(88., 47.),
                Align2::LEFT_TOP,
                &self.drive_model,
                FontId::proportional(14.),
                color(&t.muted),
            );
        }
        chevron(ui, rect.min + Vec2::new(299., 30.), true, color(&t.muted));
        if ui
            .interact(
                Rect::from_min_size(rect.min, Vec2::new(320., 84.)),
                ui.scope_id().with("choose_drive"),
                Sense::click(),
            )
            .on_hover_text(crate::tr!(
                "Обрати диск або папку",
                "Choose a drive or folder"
            ))
            .clicked()
        {
            if let Some(p) = rfd::FileDialog::new()
                .set_directory(&self.root)
                .pick_folder()
            {
                self.root = p.to_string_lossy().into_owned();
                self.refresh_drive();
            }
        }
        if let Some((total, free)) = self.space {
            let sum = self
                .space_device
                .map(|device| self.recycle.summary_for(device));
            let parts = storage_parts(total, free, sum.map(|s| s.bytes).unwrap_or(0));
            let used = parts.used;
            let left = rect.left() + 337.;
            let right = rect.right() - 252.;
            let line = Rect::from_min_size(
                Pos2::new(left, rect.top() + 23.),
                Vec2::new((right - left).max(30.), 12.),
            );
            ui.painter().rect_filled(line, 3, color(&t.border));
            let target = (line.width() * used as f32 / total.max(1) as f32).round();
            let intro = motion::enter(ui, "disk-appear", &self.root);
            let fill_width =
                motion::value(ui, "disk-fill", target, 2.5).clamp(0., line.width()) * intro;

            ui.painter().rect_filled(
                Rect::from_min_size(line.min, Vec2::new(fill_width, line.height())),
                3,
                color(&t.accent),
            );
            let trash_target = line.width() * parts.trash as f32 / total.max(1) as f32;
            let trash_width = (motion::value(ui, ("disk-trash", &self.root), trash_target, 2.5)
                * intro)
                .clamp(0., fill_width);
            if trash_width > 0. {
                let amber = Rect::from_min_max(
                    Pos2::new(line.left() + fill_width - trash_width, line.top()),
                    Pos2::new(line.left() + fill_width, line.bottom()),
                );
                let radius = egui::CornerRadius {
                    nw: if trash_width >= fill_width { 3 } else { 0 },
                    sw: if trash_width >= fill_width { 3 } else { 0 },
                    ne: 3,
                    se: 3,
                };
                ui.painter().rect_filled(amber, radius, color(&t.trash));
            }
            ui.painter().text(
                Pos2::new(left, rect.top() + 46.),
                Align2::LEFT_TOP,
                crate::tr!("{} зайнято", "{} used", display_size(used)),
                FontId::proportional(14.),
                color(&t.text),
            );
            ui.painter().text(
                Pos2::new(right, rect.top() + 46.),
                Align2::RIGHT_TOP,
                crate::tr!("{} загалом", "{} total", display_size(total)),
                FontId::proportional(14.),
                color(&t.muted),
            );
            let measured = sum.filter(|_| self.recycle.loaded);
            let unavailable = !self.recycle.error.is_empty() && self.recycle.items.is_empty();
            let caption = if unavailable {
                crate::tr!("Кошик: недоступно", "Trash: unavailable").to_string()
            } else if let Some(s) = measured {
                if s.entries > 0 && s.bytes == 0 && !s.complete {
                    crate::tr!("У кошику: рахуємо…", "In Trash: calculating…").to_string()
                } else {
                    crate::tr!(
                        "У кошику: {}{}",
                        "In Trash: {}{}",
                        if s.complete { "" } else { "≈ " },
                        display_size(s.bytes)
                    )
                }
            } else {
                crate::tr!("Кошик: перевіряємо…", "Trash: checking…").to_string()
            };
            ui.interact(
                line.expand(3.),
                ui.scope_id().with("storage-breakdown"),
                Sense::hover(),
            )
            .on_hover_text(crate::tr!(
                "Інше зайняте місце: {}\nКошик: {}{}\nВільно: {}",
                "Other used space: {}\nTrash: {}{}\nFree: {}",
                display_size(parts.other),
                if measured.is_some_and(|s| s.complete) {
                    ""
                } else {
                    "≈ "
                },
                display_size(parts.trash),
                display_size(free)
            ));
            let font = FontId::proportional(if line.width() < 600. { 12. } else { 14. });
            let galley = ui.painter().layout_no_wrap(caption, font, color(&t.trash));
            let legend_width = galley.size().x + 15.;
            let legend = Rect::from_min_size(
                Pos2::new(line.center().x - legend_width * 0.5, rect.top() + 46.),
                Vec2::new(legend_width, 21.),
            );
            let response = ui.interact(
                legend.expand(4.),
                ui.scope_id().with("trash-usage"),
                Sense::click(),
            );
            ui.painter().circle_filled(
                Pos2::new(legend.left() + 4., legend.top() + 8.),
                3.,
                color(&t.trash),
            );
            ui.painter()
                .galley(legend.min + Vec2::new(15., 0.), galley, color(&t.trash));
            if response.hovered() {
                ui.painter().hline(
                    legend.x_range(),
                    legend.bottom(),
                    Stroke::new(1., color(&t.trash).gamma_multiply(0.5)),
                );
            }
            let tooltip = if unavailable {
                self.recycle.error.clone()
            } else if let Some(s) = measured {
                crate::tr!(
                    "{} {} · {} {}\nПоказано кошик на цьому диску. Він уже входить у зайняте місце.{}\nНатисни, щоб відкрити кошик.",
                    "{} {} · {} {}\nShowing Trash on this drive. It is already included in used space.{}\nClick to open Trash.",
                    group_digits(s.files),
                    plural(
                        s.files,
                        crate::tr!("файл", "file"),
                        crate::tr!("файли", "files"),
                        crate::tr!("файлів", "files")
                    ),
                    group_digits(s.folders),
                    plural(
                        s.folders,
                        crate::tr!("папка", "folder"),
                        crate::tr!("папки", "folders"),
                        crate::tr!("папок", "folders")
                    ),
                    if s.complete {
                        ""
                    } else {
                        crate::tr!(
                            " Підрахунок ще триває або частина даних недоступна.",
                            " Still calculating, or some data is unavailable."
                        )
                    }
                )
            } else {
                crate::tr!(
                    "Підрахунок кошика у фоні. Натисни, щоб відкрити його.",
                    "Calculating Trash usage in the background. Click to open it."
                )
                .into()
            };
            if response.on_hover_text(tooltip).clicked() {
                self.view = View::Trash;
                self.filter.clear();
                self.properties = None;
            }
            if let Some(s) = measured.filter(|s| s.files > 0 || s.complete) {
                ui.painter().text(
                    Pos2::new(rect.right() - 22., rect.top() + 61.),
                    Align2::RIGHT_CENTER,
                    crate::tr!(
                        "{}{} {} у кошику",
                        "{}{} {} in Trash",
                        if s.complete { "" } else { "≈ " },
                        group_digits(s.files),
                        plural(
                            s.files,
                            crate::tr!("файл", "file"),
                            crate::tr!("файли", "files"),
                            crate::tr!("файлів", "files")
                        )
                    ),
                    FontId::proportional(12.),
                    color(&t.trash),
                );
            }
            let word = ui.painter().layout_no_wrap(
                crate::tr!("вільно", "free").into(),
                FontId::proportional(16.),
                color(&t.text),
            );
            let x = rect.right() - 22.;
            ui.painter().text(
                Pos2::new(x, rect.top() + 37.),
                Align2::RIGHT_CENTER,
                crate::tr!("вільно", "free"),
                FontId::proportional(16.),
                color(&t.text),
            );
            ui.painter().text(
                Pos2::new(x - word.size().x - 8., rect.top() + 36.),
                Align2::RIGHT_CENTER,
                display_size(free),
                art::bold(28.),
                color(&t.text),
            );
        }
    }
    fn nav_state(&self) -> NavState {
        NavState {
            view: self.view,
            category: self.category,
            folder: self.table_root,
        }
    }
    fn history_step(&mut self, forward: bool) {
        let current = self.nav_state();
        let target = if forward {
            self.history.forward(current)
        } else {
            self.history.back(current)
        };
        if let Some(target) = target {
            self.view = target.view;
            self.category = target.category;
            self.table_root = target.folder;
            self.filter.clear();
            self.properties = None;
            self.nav_traversed = true;
            if let Some(id) = target.folder {
                self.select_folder(id);
            }
        }
    }
    fn scope_controls(&mut self, ui: &mut egui::Ui) {
        let t = self.theme.clone();
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 36.), Sense::hover());
        for (offset, icon, forward, enabled, help) in [
            (
                0.,
                13,
                false,
                self.history.can_back(),
                crate::tr!(
                    "Назад · бічна кнопка миші / Alt+←",
                    "Back · mouse side button / Alt+←"
                ),
            ),
            (
                34.,
                22,
                true,
                self.history.can_forward(),
                crate::tr!(
                    "Уперед · бічна кнопка миші / Alt+→",
                    "Forward · mouse side button / Alt+→"
                ),
            ),
        ] {
            let r = Rect::from_min_size(rect.min + Vec2::new(offset, 0.), Vec2::new(28., 34.));
            let response = ui
                .interact(
                    r,
                    ui.scope_id().with(("navigation", forward)),
                    if enabled {
                        Sense::click()
                    } else {
                        Sense::hover()
                    },
                )
                .on_hover_text(help);
            if enabled && response.hovered() {
                ui.painter().rect_filled(r, t.radius, color(&t.raised));
            }
            self.art.paint(
                ui,
                icon,
                Rect::from_center_size(r.center(), Vec2::splat(19.)),
                color(&t.muted).gamma_multiply(if enabled { 1. } else { 0.4 }),
            );
            if enabled && response.clicked() {
                self.history_step(forward);
            }
        }
        let home_rect = Rect::from_min_size(rect.min + Vec2::new(68., 0.), Vec2::new(28., 34.));
        let home = ui
            .interact(home_rect, ui.scope_id().with("root_folder"), Sense::click())
            .on_hover_text(crate::tr!(
                "Усі папки · корінь сканування",
                "All folders · scan root"
            ));
        if home.hovered() {
            ui.painter()
                .rect_filled(home_rect, t.radius, color(&t.raised));
        }
        self.art.paint(
            ui,
            12,
            Rect::from_center_size(home_rect.center(), Vec2::splat(19.)),
            color(&t.text),
        );
        if home.clicked() {
            self.category = Category::All;
            self.table_root = None;
            self.view = View::Folders;
            self.expand_folder(0);
            self.filter.clear();
        }
        let path = self
            .table_root
            .and_then(|i| self.report.as_ref().map(|r| r.folders[i].path.clone()))
            .or_else(|| {
                if self.category != Category::All {
                    self.index
                        .roots
                        .get(&self.category)
                        .and_then(|v| v.first())
                        .and_then(|i| self.report.as_ref().map(|r| r.folders[*i].path.clone()))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| PathBuf::from(&self.root));
        let crumb_width = (width - 461.).max(65.);
        let crumb_rect =
            Rect::from_min_size(rect.min + Vec2::new(106., 0.), Vec2::new(crumb_width, 36.));
        let mut path_text = path
            .to_string_lossy()
            .replace('/', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("   ›   ");
        if path_text.is_empty() {
            path_text = "/".into();
        }
        let max = ((crumb_width - 10.) / 7.).max(8.) as usize;
        if path_text.chars().count() > max {
            path_text = format!(
                "…{}",
                path_text
                    .chars()
                    .rev()
                    .take(max - 1)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<String>()
            );
        }
        ui.painter().with_clip_rect(crumb_rect).text(
            crumb_rect.left_center(),
            Align2::LEFT_CENTER,
            path_text,
            FontId::proportional(14.),
            color(&t.text),
        );
        let scan = Rect::from_min_size(
            Pos2::new(rect.right() - 145., rect.top()),
            Vec2::new(145., 34.),
        );
        let search = Rect::from_min_max(
            Pos2::new(scan.left() - 207., rect.top()),
            Pos2::new(scan.left() - 8., rect.bottom() - 2.),
        );
        ui.put(
            search,
            egui::TextEdit::singleline(&mut self.filter)
                .hint_text(crate::tr!("Пошук у цій папці…", "Search in this folder…"))
                .font(FontId::proportional(14.))
                .margin(egui::Margin {
                    left: 29,
                    right: 6,
                    top: 8,
                    bottom: 6,
                }),
        );
        self.art.paint(
            ui,
            9,
            Rect::from_min_size(search.min + Vec2::new(9., 9.), Vec2::splat(15.)),
            color(&t.muted),
        );
        if visual_button(
            ui,
            &self.art,
            scan,
            11,
            if self.report.is_some() {
                crate::tr!("Оновити", "Refresh")
            } else {
                crate::tr!("Сканувати", "Scan")
            },
            !self.busy && self.trash_rx.is_none() && !self.recycle.busy && self.restore_requested_root.is_none(),
            &t,
        )
        .on_hover_text(
            crate::tr!("Перевірити події змін; знімок до 60 с можна використати повторно. Shift + клік — завжди повний обхід.", "Check for changes; reuse a snapshot up to 60 s old. Shift + click always runs a full scan."),
        )
        .clicked()
        {
            self.start(false, ui.input(|i| i.modifiers.shift));
            self.view = View::Folders;
            self.table_root = None;
        }
        if self.restored_snapshot
            || self.auto_refresh.pending()
            || (self.busy && !self.duplicate_job && self.report.is_some())
        {
            ui.add_space(4.);
            let caption = if self.busy {
                crate::tr!(
                    "Оновлюємо дані — поки показано попередній скан.",
                    "Refreshing — showing the previous scan for now."
                )
                .to_string()
            } else if self.layout.auto_refresh && self.auto_refresh.pending() {
                crate::tr!(
                    "На SSD є зміни — заплановано фонове оновлення.",
                    "SSD usage changed — background refresh scheduled."
                )
                .to_string()
            } else if let Some(at) = self.saved_snapshot_at {
                crate::tr!(
                    "Збережений скан від {}. {}",
                    "Saved scan from {}. {}",
                    date(std::time::UNIX_EPOCH + Duration::from_secs(at)),
                    if self.layout.auto_refresh {
                        crate::tr!(
                            "Зміни зайнятого місця відстежуються автоматично.",
                            "Disk usage changes are monitored automatically."
                        )
                    } else {
                        crate::tr!(
                            "Для перевірки змін натисни «Оновити».",
                            "Click Refresh to check for changes."
                        )
                    }
                )
            } else {
                crate::tr!(
                    "Попередній результат — натисни «Оновити» для перевірки змін.",
                    "Previous result — click Refresh to check for changes."
                )
                .to_string()
            };
            ui.add(
                egui::Label::new(RichText::new(caption).size(12.).color(color(&t.muted))).wrap(),
            );
        }
        if self.busy {
            ui.add_space(5.);
            ui.horizontal(|ui| {
                motion::activity(
                    ui,
                    self.control.pause.load(Ordering::Relaxed),
                    color(&t.accent),
                );
                ui.label(if self.duplicate_job {
                    crate::tr!("Пошук дублікатів", "Finding duplicates")
                } else {
                    if self.automatic_job {
                        crate::tr!("Автооновлення", "Auto refresh")
                    } else {
                        crate::tr!("Сканування", "Scanning")
                    }
                });
                let paused = self.control.pause.load(Ordering::Relaxed);
                if ui
                    .button(if paused {
                        crate::tr!("Продовжити", "Resume")
                    } else {
                        crate::tr!("Пауза", "Pause")
                    })
                    .clicked()
                {
                    self.control.pause.store(!paused, Ordering::Relaxed);
                }
                if ui.button(crate::tr!("Скасувати", "Cancel")).clicked() {
                    self.control.cancel.store(true, Ordering::Relaxed);
                }
                let mut gentle = self.control.gentle.load(Ordering::Relaxed);
                if ui
                    .checkbox(&mut gentle, crate::tr!("Бережний режим", "Gentle mode"))
                    .changed()
                {
                    self.layout.gentle_scan = gentle;
                    self.control.gentle.store(gentle, Ordering::Relaxed);
                }
                if let Some(p) = &self.progress {
                    ui.label(
                        RichText::new(crate::tr!(
                            "{} · {} записів",
                            "{} · {} entries",
                            display_size(p.bytes),
                            p.entries
                        ))
                        .size(12.)
                        .color(color(&t.muted)),
                    )
                    .on_hover_text(crate::tr!(
                        "{}\nПропущено: {}",
                        "{}\nSkipped: {}",
                        p.current.display(),
                        p.skipped
                    ));
                }
            });
        }
    }
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let t = self.theme.clone();
        let row_height = ((ui.available_height() - 20.) / 5.).clamp(60., 72.);
        egui::ScrollArea::vertical()
            .id_salt("category-list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.;
                ui.add_space(9.);
                for (cat, kind) in [
                    (Category::Games, 1),
                    (Category::Development, 2),
                    (Category::Cache, 3),
                    (Category::Documents, 4),
                    (Category::Other, 5),
                ] {
                    let active = self.view == View::Folders && self.category == cat;
                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), row_height),
                        Sense::click(),
                    );
                    let hover = motion::hover(ui, ("category-hover", kind), response.hovered());
                    let selected = motion::value(
                        ui,
                        ("category-selected", kind),
                        if active { 1. } else { 0. },
                        1.,
                    );
                    let strength = selected.max(hover * 0.65);
                    if strength > 0. {
                        ui.painter().rect_filled(
                            rect,
                            0,
                            mix(color(&t.panel), color(&t.raised), strength),
                        );
                    }
                    if active {
                        let y = motion::value(ui, "category-rail-y", rect.top(), 1.15);
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                Pos2::new(rect.left(), y),
                                Vec2::new(4., rect.height()),
                            ),
                            0,
                            color(&t.accent),
                        );
                    }
                    self.art.paint(
                        ui,
                        kind,
                        Rect::from_center_size(
                            rect.min + Vec2::new(35., 34.),
                            Vec2::splat(30. * self.layout.icon_scale),
                        ),
                        C::WHITE,
                    );
                    ui.painter().text(
                        rect.min
                            + Vec2::new(64., if self.layout.category_sizes { 20. } else { 28. }),
                        Align2::LEFT_TOP,
                        cat.name(),
                        art::bold(15. * self.layout.text_scale),
                        color(&t.text),
                    );
                    if self.layout.category_sizes {
                        let bytes = self
                            .report
                            .as_ref()
                            .map(|_| *self.index.totals.get(&cat).unwrap_or(&0));
                        ui.painter().text(
                            rect.min + Vec2::new(64., 42.),
                            Align2::LEFT_TOP,
                            bytes.map(display_size).unwrap_or_else(|| "—".into()),
                            FontId::proportional(14.),
                            color(&t.muted),
                        );
                    }
                    if response.clicked() {
                        self.category = cat;
                        self.view = View::Folders;
                        self.filter.clear();
                        self.table_root = None;
                        self.properties = None;
                        if let Some(id) =
                            self.index.roots.get(&cat).and_then(|v| v.first()).copied()
                        {
                            self.expand_folder(id);
                            self.select_folder(id);
                        }
                    }
                }
                ui.add_space(8.);
                ui.separator();
                let active = self.view == View::Trash;
                let (r, response) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 54.), Sense::click());
                let hover = motion::hover(ui, "trash-navigation", response.hovered());
                ui.painter().rect_filled(
                    r,
                    0,
                    mix(
                        color(&t.panel),
                        color(&t.raised),
                        if active { 1. } else { hover * 0.65 },
                    ),
                );
                if active {
                    let y = motion::value(ui, "category-rail-y", r.top(), 1.15);
                    ui.painter().rect_filled(
                        Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(4., r.height())),
                        0,
                        color(&t.accent),
                    );
                }
                self.art.paint(
                    ui,
                    23,
                    Rect::from_center_size(
                        r.min + Vec2::new(35., 27.),
                        Vec2::splat(27. * self.layout.icon_scale),
                    ),
                    color(&t.text),
                );
                ui.painter().text(
                    r.min + Vec2::new(64., 18.),
                    Align2::LEFT_TOP,
                    crate::tr!("Кошик", "Trash"),
                    art::bold(15. * self.layout.text_scale),
                    color(&t.text),
                );
                if response.clicked() {
                    self.view = View::Trash;
                    self.filter.clear();
                    self.properties = None;
                }
            });
    }
    fn inspector(&mut self, ui: &mut egui::Ui) {
        if self.view == View::Trash {
            self.recycle_inspector(ui);
            return;
        }
        let t = self.theme.clone();
        let enter = motion::enter(
            ui,
            "inspector-enter",
            (
                self.properties.as_ref().map(|p| &p.path),
                self.report_revision,
            ),
        );
        ui.multiply_opacity(0.4 + enter * 0.6);

        let action_properties = self.properties.clone();
        let action_area = ui.max_rect();
        egui::ScrollArea::vertical()
            .id_salt("properties")
            .max_height(
                (ui.available_height() - if action_properties.is_some() { 98. } else { 0. })
                    .max(50.),
            )
            .show(ui, |ui| {
                ui.add_space(7.);
                if let Some(p) = self.properties.clone() {
                    let (rect, _) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 88.), Sense::hover());
                    let tile = Rect::from_min_size(rect.min + Vec2::new(0., 5.), Vec2::splat(66.));
                    ui.painter().rect_filled(tile, t.radius, color(&t.raised));
                    self.art.paint(
                        ui,
                        0,
                        Rect::from_center_size(
                            tile.center(),
                            Vec2::splat(46. * self.layout.icon_scale),
                        ),
                        C::WHITE,
                    );
                    ui.painter().text(
                        rect.min + Vec2::new(84., 13.),
                        Align2::LEFT_TOP,
                        label(&p.path),
                        art::bold(22.),
                        color(&t.text),
                    );
                    let path = shorten(
                        &p.path.to_string_lossy(),
                        ((rect.width() - 86.) / 6.2) as usize,
                    );
                    ui.painter().text(
                        rect.min + Vec2::new(84., 44.),
                        Align2::LEFT_TOP,
                        path,
                        FontId::proportional(12.),
                        color(&t.muted),
                    );
                    ui.interact(rect, ui.scope_id().with("property_path"), Sense::hover())
                        .on_hover_text(p.path.to_string_lossy());
                    ui.add_space(3.);
                    ui.label(RichText::new(display_size(p.bytes)).font(art::bold(29.)));
                    ui.add_space(12.);
                    for (k, v) in [
                        (
                            crate::tr!("Тип", "Type"),
                            if p.is_dir {
                                crate::tr!("Папка", "Folder").into()
                            } else {
                                crate::tr!("Файл", "File").into()
                            },
                        ),
                        (
                            crate::tr!("Зайнято на диску", "Size on disk"),
                            display_size(p.bytes),
                        ),
                        (
                            crate::tr!("Елементів", "Items"),
                            p.files.map(group_digits).unwrap_or_else(|| "—".into()),
                        ),
                        (
                            crate::tr!("Змінено", "Modified"),
                            p.modified.split(',').next().unwrap_or("—").to_string(),
                        ),
                        (
                            crate::tr!("Створено", "Created"),
                            p.created.split(',').next().unwrap_or("—").to_string(),
                        ),
                        (crate::tr!("Власник", "Owner"), p.owner),
                        (crate::tr!("Доступ", "Permissions"), p.permissions),
                    ] {
                        let (r, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 25.),
                            Sense::hover(),
                        );
                        ui.painter().text(
                            r.left_center(),
                            Align2::LEFT_CENTER,
                            k,
                            FontId::proportional(14.),
                            color(&t.muted),
                        );
                        ui.painter().text(
                            r.right_center(),
                            Align2::RIGHT_CENTER,
                            v,
                            FontId::proportional(14.),
                            color(&t.text),
                        );
                        ui.painter().hline(
                            r.x_range(),
                            r.bottom(),
                            Stroke::new(0.4, color(&t.border).gamma_multiply(0.65)),
                        );
                    }
                    if let Some(e) = p.error {
                        ui.add(
                            egui::Label::new(RichText::new(e).size(12.).color(color(&t.accent)))
                                .wrap(),
                        );
                    }
                } else {
                    let (r, _) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 75.), Sense::hover());
                    self.art.paint(
                        ui,
                        0,
                        Rect::from_min_size(r.min + Vec2::new(2., 9.), Vec2::splat(48.)),
                        C::from_white_alpha(110),
                    );
                    ui.painter().text(
                        r.min + Vec2::new(66., 17.),
                        Align2::LEFT_TOP,
                        crate::tr!("Властивості", "Properties"),
                        art::bold(18.),
                        color(&t.text),
                    );
                    ui.add_space(8.);
                    ui.label(
                        RichText::new(crate::tr!(
                            "Вибери папку або файл у списку.",
                            "Select a folder or file in the list."
                        ))
                        .size(14.)
                        .color(color(&t.muted)),
                    );
                }
            });
        if let Some(p) = action_properties {
            let r = Rect::from_min_size(
                Pos2::new(action_area.left(), action_area.bottom() - 83.),
                Vec2::new(action_area.width(), 36.),
            );
            if visual_button(
                ui,
                &self.art,
                r,
                10,
                crate::tr!("Відкрити в папці", "Open in file manager"),
                !self.demo,
                &t,
            )
            .clicked()
            {
                open(if p.is_dir {
                    p.path.as_path()
                } else {
                    p.path.parent().unwrap_or(&p.path)
                });
            }
            if p.is_dir {
                let r = r.translate(Vec2::new(0., 47.));
                let mut danger = t.clone();
                danger.text = t.accent.clone();
                danger.border = t.accent.clone();
                if visual_button(
                    ui,
                    &self.art,
                    r,
                    23,
                    crate::tr!("Видалити папку", "Delete folder"),
                    !self.demo
                        && !self.busy
                        && self.trash_rx.is_none()
                        && !self.recycle.busy
                        && p.error.is_none(),
                    &danger,
                )
                .on_hover_text(crate::tr!(
                    "Перемістити папку з усім вмістом у кошик після підтвердження",
                    "Move this folder and all its contents to Trash after confirmation"
                ))
                .clicked()
                {
                    self.request_delete(vec![(p.path, p.bytes)]);
                }
            }
        }
    }
    fn refresh_drive(&mut self) {
        let (n, m) = if self.demo {
            (
                crate::tr!("Системний SSD", "System SSD").into(),
                crate::tr!("NVMe SSD · 1 ТБ", "NVMe SSD · 1 TB").into(),
            )
        } else {
            drive_info(Path::new(&self.root))
        };
        self.drive_name = n;
        self.drive_model = m;
        self.space_at = Instant::now() - Duration::from_secs(10);
    }
    fn expand_folder(&mut self, id: usize) {
        if self.expanded.insert(id) {
            self.tree_revision = self.tree_revision.wrapping_add(1);
        }
    }
    fn toggle_folder(&mut self, id: usize) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
        self.tree_revision = self.tree_revision.wrapping_add(1);
    }
    fn visible_rows(&mut self) -> FolderRows {
        let key = RowsKey {
            location: self.nav_state(),
            filter: self.filter.clone(),
            sort_size: self.sort_size,
            descending: self.descending,
            report: self.report_revision,
            tree: self.tree_revision,
        };
        if let Some((old, rows)) = &self.rows_cache {
            if *old == key {
                return rows.clone();
            }
        }
        let rows = Arc::new(self.build_visible_rows());
        self.rows_cache = Some((key, rows.clone()));
        rows
    }
    fn build_visible_rows(&self) -> Vec<(usize, usize)> {
        let Some(r) = &self.report else {
            return vec![];
        };
        if !self.filter.is_empty() {
            let q = self.filter.to_lowercase();
            let mut ids: Vec<_> = r
                .folders
                .iter()
                .enumerate()
                .filter(|(id, f)| {
                    (self.category == Category::All || self.index.classes[*id] == self.category)
                        && f.path.to_string_lossy().to_lowercase().contains(&q)
                })
                .map(|(i, _)| i)
                .collect();
            self.sort_ids(&mut ids);
            return ids.into_iter().map(|i| (i, 0)).collect();
        }
        let roots = if let Some(id) = self.table_root {
            vec![id]
        } else {
            self.index
                .roots
                .get(&self.category)
                .cloned()
                .unwrap_or_default()
        };
        let mut roots = roots;
        self.sort_ids(&mut roots);
        let mut stack: Vec<_> = roots.into_iter().rev().map(|i| (i, 0)).collect();
        let mut rows = vec![];
        while let Some((i, d)) = stack.pop() {
            rows.push((i, d));
            if self.expanded.contains(&i) {
                let mut children = self.index.children[i].clone();
                if self.category != Category::All {
                    children.retain(|j| self.index.classes[*j] == self.category);
                }
                self.sort_ids(&mut children);
                stack.extend(children.into_iter().rev().map(|j| (j, d + 1)));
            }
        }
        rows
    }
    fn sort_ids(&self, ids: &mut [usize]) {
        if let Some(r) = &self.report {
            ids.sort_by(|a, b| {
                let o = if self.sort_size {
                    r.folders[*a].bytes.cmp(&r.folders[*b].bytes)
                } else {
                    r.folders[*a]
                        .path
                        .file_name()
                        .cmp(&r.folders[*b].path.file_name())
                };
                if self.descending { o.reverse() } else { o }
            });
        }
    }
    fn folder_table(&mut self, ui: &mut egui::Ui) {
        let t = self.theme.clone();
        let count_width = if self.layout.show_count { 80. } else { 0. };
        let size_width = 145.;
        let width = ui.available_width();
        let name_width = (width - size_width - count_width).max(150.);
        let (h, _) = ui.allocate_exact_size(Vec2::new(width, 35.), Sense::hover());
        ui.painter()
            .rect_filled(h, t.radius, mix(color(&t.panel), color(&t.raised), 0.45));
        ui.painter().rect_stroke(
            h,
            t.radius,
            Stroke::new(1., color(&t.border)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            h.min + Vec2::new(15., 17.),
            Align2::LEFT_CENTER,
            crate::tr!("Назва", "Name"),
            FontId::proportional(14.),
            color(&t.text),
        );
        let size_left = h.left() + name_width;
        ui.painter()
            .vline(size_left, h.y_range(), Stroke::new(1., color(&t.border)));
        ui.painter().text(
            Pos2::new(h.right() - count_width - 48., h.center().y),
            Align2::RIGHT_CENTER,
            crate::tr!("Розмір", "Size"),
            FontId::proportional(14.),
            color(&t.text),
        );
        chevron(
            ui,
            Pos2::new(h.right() - count_width - 26., h.center().y),
            self.descending,
            if self.sort_size {
                color(&t.accent)
            } else {
                color(&t.muted)
            },
        );
        if ui
            .interact(
                Rect::from_min_max(h.min, Pos2::new(size_left, h.bottom())),
                ui.scope_id().with("sort_name"),
                Sense::click(),
            )
            .clicked()
        {
            self.descending = if self.sort_size {
                false
            } else {
                !self.descending
            };
            self.sort_size = false;
        }
        if ui
            .interact(
                Rect::from_min_max(Pos2::new(size_left, h.top()), h.max),
                ui.scope_id().with("sort_size"),
                Sense::click(),
            )
            .clicked()
        {
            self.descending = if self.sort_size {
                !self.descending
            } else {
                true
            };
            self.sort_size = true;
        }
        if self.layout.show_count {
            ui.painter().text(
                Pos2::new(h.right() - 9., h.center().y),
                Align2::RIGHT_CENTER,
                crate::tr!("Файлів", "Files"),
                FontId::proportional(12.),
                color(&t.muted),
            );
        }
        ui.spacing_mut().item_spacing.y = 0.;
        let outer = Rect::from_min_size(
            ui.cursor().min,
            Vec2::new(width, ui.available_height().max(100.)),
        );
        ui.painter().rect_stroke(
            outer,
            t.radius,
            Stroke::new(1., color(&t.border)),
            egui::StrokeKind::Inside,
        );
        if self.report.is_none() {
            let center = outer.center();
            self.art.paint(
                ui,
                0,
                Rect::from_center_size(center - Vec2::new(0., 45.), Vec2::splat(42.)),
                C::from_white_alpha(90),
            );
            ui.painter().text(
                center + Vec2::new(0., 0.),
                Align2::CENTER_CENTER,
                if self.restore_requested_root.is_some() {
                    crate::tr!(
                        "Відновлюємо останній знімок…",
                        "Restoring the last snapshot…"
                    )
                } else if self.busy {
                    crate::tr!("Сканування папок…", "Scanning folders…")
                } else {
                    crate::tr!(
                        "Обери папку та почни сканування",
                        "Choose a folder and start scanning"
                    )
                },
                FontId::proportional(17.),
                color(&t.muted),
            );
            ui.painter().text(
                center + Vec2::new(0., 28.),
                Align2::CENTER_CENTER,
                if self.restore_requested_root.is_some() {
                    crate::tr!(
                        "Читаємо збережене дерево у фоні, без нового сканування.",
                        "Loading the saved tree in the background, without a new scan."
                    )
                } else if self.busy {
                    crate::tr!(
                        "Збираємо розміри. Прогрес і керування — вгорі.",
                        "Collecting sizes. Progress and controls are above."
                    )
                } else {
                    crate::tr!(
                        "Назва диска зліва зверху відкриває вибір папки",
                        "Click the drive name at the top left to choose a folder"
                    )
                },
                FontId::proportional(12.),
                color(&t.muted),
            );
            return;
        }
        let rows = self.visible_rows();
        let height = ((if self.layout.compact { 28. } else { 34. }) * self.layout.text_scale)
            .max(23. * self.layout.icon_scale + 6.);

        egui::ScrollArea::vertical()
            .id_salt("folder-table")
            .auto_shrink([false, false])
            .show_rows(ui, height, rows.len(), |ui, range| {
                ui.spacing_mut().item_spacing.y = 0.;
                for n in range {
                    let (id, depth) = rows[n];
                    let f = self.report.as_ref().unwrap().folders[id].clone();
                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), height),
                        Sense::click(),
                    );
                    if self.layout.striped_rows && n % 2 == 1 {
                        ui.painter().rect_filled(
                            rect,
                            0,
                            mix(color(&t.background), color(&t.raised), 0.3),
                        );
                    }
                    let selected = motion::value(
                        ui,
                        ("row-selected", self.report_revision, id),
                        if self.focus == id { 1. } else { 0. },
                        1.,
                    );
                    let hover = motion::hover(
                        ui,
                        ("row-hover", self.report_revision, id),
                        response.hovered(),
                    );
                    if hover > 0. && selected < 1. {
                        ui.painter().rect_filled(
                            rect,
                            t.radius,
                            color(&t.raised).gamma_multiply(hover * (1. - selected)),
                        );
                    }
                    if selected > 0. {
                        ui.painter().rect_filled(
                            rect,
                            t.radius,
                            mix(color(&t.panel), color(&t.accent), 0.31).gamma_multiply(selected),
                        );
                        ui.painter().rect_stroke(
                            rect,
                            t.radius,
                            Stroke::new(0.6, color(&t.accent).gamma_multiply(0.45 * selected)),
                            egui::StrokeKind::Inside,
                        );
                        ui.painter().rect_filled(
                            Rect::from_min_size(rect.min, Vec2::new(4., rect.height())),
                            0,
                            color(&t.accent).gamma_multiply(selected),
                        );
                    }
                    let mut child_clicked = false;
                    let can_select =
                        self.index.cleanup_ids.contains(&id) && !self.busy && !self.demo;
                    let mut x = rect.left() + 11. + (depth as f32 * 19.).min(name_width * 0.35);
                    if can_select {
                        let cbrect = Rect::from_min_size(
                            Pos2::new(x, rect.center().y - 9.),
                            Vec2::splat(18.),
                        );
                        let mut checked = self.selected.contains(&f.path);
                        let ir = ui.scope_builder(egui::UiBuilder::new().max_rect(cbrect), |ui| {
                            ui.checkbox(&mut checked, "")
                        });
                        if ir.inner.changed() {
                            child_clicked = true;
                            if checked {
                                self.selected.insert(f.path.clone());
                            } else {
                                self.selected.remove(&f.path);
                            }
                        }
                        x += 24.;
                    }
                    let arrow = Rect::from_center_size(
                        Pos2::new(x + 9., rect.center().y),
                        Vec2::new(19., height),
                    );
                    if !self.index.children[id].is_empty() {
                        let openness = motion::value(
                            ui,
                            ("folder-disclosure", self.report_revision, id),
                            if self.expanded.contains(&id) { 1. } else { 0. },
                            1.,
                        );
                        motion::disclosure(ui, arrow.center(), openness, color(&t.muted));
                        if ui
                            .interact(arrow, ui.scope_id().with(("expand", id)), Sense::click())
                            .clicked()
                        {
                            child_clicked = true;
                            ui.ctx().request_repaint();
                            self.toggle_folder(id);
                        }
                    }
                    x += 29.;
                    self.art.paint(
                        ui,
                        0,
                        Rect::from_center_size(
                            Pos2::new(x + 11., rect.center().y),
                            Vec2::splat(23. * self.layout.icon_scale),
                        ),
                        C::WHITE,
                    );
                    x += 32. + (23. * (self.layout.icon_scale - 1.)).max(0.) * 0.5;
                    let shown = shorten(
                        &label(&f.path),
                        ((name_width - (x - rect.left()) - 8.) / 7.2).max(5.) as usize,
                    );
                    ui.painter().text(
                        Pos2::new(x, rect.center().y),
                        Align2::LEFT_CENTER,
                        shown,
                        FontId::proportional(15. * self.layout.text_scale),
                        color(&t.text),
                    );
                    ui.painter().text(
                        Pos2::new(rect.right() - count_width - 12., rect.center().y),
                        Align2::RIGHT_CENTER,
                        display_size(f.bytes),
                        FontId::proportional(15. * self.layout.text_scale),
                        color(&t.text),
                    );
                    if self.layout.show_count {
                        ui.painter().text(
                            Pos2::new(rect.right() - 9., rect.center().y),
                            Align2::RIGHT_CENTER,
                            group_digits(f.files),
                            FontId::proportional(12.),
                            color(&t.muted),
                        );
                    }
                    if self.layout.row_separators {
                        ui.painter().hline(
                            rect.x_range(),
                            rect.bottom(),
                            Stroke::new(0.5, color(&t.border)),
                        );
                    }
                    if response.clicked() && !child_clicked {
                        self.select_folder(id);
                        if self.layout.single_click_expand
                            && !response.double_clicked()
                            && !self.index.children[id].is_empty()
                        {
                            ui.ctx().request_repaint();
                            self.toggle_folder(id);
                        }
                    }
                    if response.double_clicked()
                        && !child_clicked
                        && !self.layout.single_click_expand
                    {
                        self.table_root = Some(id);
                        self.expand_folder(id);
                        self.filter.clear();
                    }
                    response.on_hover_text(f.path.to_string_lossy());
                }
            });
    }
    fn other_content(&mut self, ui: &mut egui::Ui) {
        let t = self.theme.clone();
        match self.view {
            View::Files => {
                ui.heading(crate::tr!("Великі файли", "Large files"));
                ui.label(
                    RichText::new(crate::tr!(
                        "300 найбільших файлів у вибраній області",
                        "The 300 largest files in the scanned area"
                    ))
                    .color(color(&t.muted))
                    .size(12.),
                );
                let files = self
                    .report
                    .as_ref()
                    .map(|r| r.largest.clone())
                    .unwrap_or_default();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for f in files {
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(
                                    self.properties
                                        .as_ref()
                                        .map(|p| p.path == f.path)
                                        .unwrap_or(false),
                                    label(&f.path),
                                )
                                .on_hover_text(f.path.to_string_lossy())
                                .clicked()
                            {
                                self.properties = Some(Properties::load(
                                    f.path.clone(),
                                    f.bytes,
                                    None,
                                    self.demo,
                                ));
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(human(f.bytes));
                                },
                            );
                        });
                        ui.separator();
                    }
                });
            }
            View::Cleanup => {
                ui.heading(crate::tr!("Очищення", "Cleanup"));
                ui.label(
                    RichText::new(crate::tr!(
                        "Вибери відомі кеші. Закрий відповідну програму перед очищенням.",
                        "Select known caches. Close the corresponding app before cleanup."
                    ))
                    .color(color(&t.muted))
                    .size(12.),
                );
                ui.label(
                    RichText::new(crate::tr!(
                        "Кошик не звільняє місце до його спорожнення.",
                        "Trash uses disk space until it is emptied."
                    ))
                    .color(color(&t.muted))
                    .size(12.),
                );
                let candidates = self.candidates();
                if candidates.is_empty() {
                    ui.add_space(20.);
                    ui.label(crate::tr!(
                        "Кандидатів немає. Проскануй домашню папку або .cache.",
                        "No candidates. Scan your home folder or .cache."
                    ));
                }
                for f in candidates {
                    ui.horizontal(|ui| {
                        let mut selected = self.selected.contains(&f.path);
                        if ui
                            .add_enabled(
                                !self.busy
                                    && !self.demo
                                    && self.trash_rx.is_none()
                                    && !self.recycle.busy,
                                egui::Checkbox::new(&mut selected, label(&f.path)),
                            )
                            .changed()
                        {
                            if selected {
                                self.selected.insert(f.path.clone());
                            } else {
                                self.selected.remove(&f.path);
                            }
                        }
                        ui.label(human(f.bytes));
                        if ui
                            .small_button(crate::tr!("Властивості", "Properties"))
                            .clicked()
                        {
                            self.properties = Some(Properties::load(
                                f.path.clone(),
                                f.bytes,
                                Some(f.files),
                                self.demo,
                            ));
                        }
                    });
                    ui.label(
                        RichText::new(f.path.to_string_lossy())
                            .size(11.)
                            .color(color(&t.muted)),
                    );
                    ui.separator();
                }
            }
            View::Duplicates => {
                ui.heading(crate::tr!("Дублікати", "Duplicates"));
                ui.label(
                    RichText::new(crate::tr!(
                        "Повне порівняння вмісту від 1 МіБ. Окреме читання диска.",
                        "Full content comparison for files from 1 MiB. Reads the disk separately."
                    ))
                    .size(12.)
                    .color(color(&t.muted)),
                );
                if ui
                    .add_enabled(
                        !self.busy && self.trash_rx.is_none() && !self.recycle.busy,
                        egui::Button::new(crate::tr!("Знайти дублікати", "Find duplicates")),
                    )
                    .clicked()
                {
                    self.start(true, true);
                }
                ui.label(
                    RichText::new(crate::tr!(
                        "Тут лише перегляд. Hardlink не рахується дублікатом.",
                        "Preview only. Hardlinks are not counted as duplicates."
                    ))
                    .size(12.)
                    .color(color(&t.muted)),
                );
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for g in &self.dups {
                        ui.add_space(10.);
                        ui.label(
                            RichText::new(crate::tr!(
                                "{} копій × {}",
                                "{} copies × {}",
                                g.len(),
                                human(g[0].bytes)
                            ))
                            .strong(),
                        );
                        for f in g {
                            if ui.link(f.path.to_string_lossy()).clicked() {
                                if let Some(p) = f.path.parent() {
                                    open(p);
                                }
                            }
                        }
                        ui.separator();
                    }
                });
            }
            _ => {}
        }
    }
    fn footer(&mut self, ui: &mut egui::Ui) {
        let t = self.theme.clone();
        let total: u64 = if self.view == View::Trash {
            self.recycle
                .items
                .iter()
                .filter(|i| self.recycle.selected.contains(&i.entry.id))
                .map(|i| i.bytes.unwrap_or(0))
                .sum()
        } else {
            self.candidates()
                .iter()
                .filter(|f| self.selected.contains(&f.path))
                .map(|f| f.bytes)
                .sum()
        };
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.), Sense::hover());
        ui.painter()
            .rect_filled(r, 0, mix(color(&t.panel), color(&t.raised), 0.25));
        ui.painter()
            .hline(r.x_range(), r.top(), Stroke::new(1., color(&t.border)));
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(Rect::from_min_size(
                r.min + Vec2::new(12., 5.),
                Vec2::new(38., 32.),
            )),
            |ui| {
                ui.menu_button("···", |ui| {
                    for (view, name) in [
                        (View::Folders, crate::tr!("Усі папки", "All folders")),
                        (View::Files, crate::tr!("Великі файли", "Large files")),
                        (View::Cleanup, crate::tr!("Очищення", "Cleanup")),
                        (View::Duplicates, crate::tr!("Дублікати", "Duplicates")),
                        (View::Trash, crate::tr!("Кошик", "Trash")),
                    ] {
                        if ui.button(name).clicked() {
                            self.view = view;
                            if view == View::Trash {
                                self.filter.clear();
                                self.properties = None;
                            }
                            if view == View::Folders {
                                self.category = Category::All;
                                self.table_root = None;
                            }
                            ui.close();
                        }
                    }
                });
            },
        );
        ui.painter().text(
            r.min + Vec2::new(58., 21.),
            Align2::LEFT_CENTER,
            crate::tr!(
                "Вибрано: {} ({} елементів)",
                "Selected: {} ({} items)",
                display_size(total),
                if self.view == View::Trash {
                    self.recycle.selected.len()
                } else {
                    self.selected.len()
                }
            ),
            FontId::proportional(14.),
            color(&t.muted),
        );
        if self.demo {
            ui.painter().text(
                r.min + Vec2::new(390., 21.),
                Align2::LEFT_CENTER,
                crate::tr!("ДЕМО", "DEMO"),
                FontId::proportional(11.),
                color(&t.accent),
            );
        }
        let button = Rect::from_min_size(
            Pos2::new(r.right() - 238., r.top() + 5.),
            Vec2::new(224., 32.),
        );
        if visual_button(
            ui,
            &self.art,
            button,
            10,
            if self.view == View::Trash {
                crate::tr!("Відновити вибране", "Restore selected")
            } else {
                crate::tr!("Переглянути вибране", "Review selected")
            },
            if self.view == View::Trash {
                !self.recycle.selected.is_empty() && self.bin_actions_enabled()
            } else {
                !self.selected.is_empty()
                    && !self.busy
                    && !self.demo
                    && self.trash_rx.is_none()
                    && !self.recycle.busy
            },
            &t,
        )
        .clicked()
        {
            if self.view == View::Trash {
                self.bin_restore(ui.ctx());
            } else {
                let folders = self
                    .candidates()
                    .into_iter()
                    .filter(|f| self.selected.contains(&f.path))
                    .map(|f| (f.path, f.bytes))
                    .collect();
                self.request_delete(folders);
            }
        }
        if !self.message.is_empty() {
            let hint = ui.interact(
                Rect::from_min_max(
                    Pos2::new(r.left() + 340., r.top()),
                    Pos2::new(button.left() - 8., r.bottom()),
                ),
                ui.scope_id().with("scan_status"),
                Sense::hover(),
            );
            hint.on_hover_text(&self.message);
            if !self.demo {
                ui.painter().text(
                    Pos2::new(button.left() - 15., r.center().y),
                    Align2::RIGHT_CENTER,
                    shorten(&self.message, 43),
                    FontId::proportional(11.),
                    color(&t.muted),
                );
            }
        }
    }
    fn preview(&self, ui: &mut egui::Ui, theme: &Theme, layout: &Layout) {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().min(300.), 208.),
            Sense::hover(),
        );
        let p = ui.painter().with_clip_rect(rect);
        let fg = color(&theme.text);
        let muted = color(&theme.muted);
        let border = Stroke::new(1., color(&theme.border));
        p.rect_filled(rect, theme.radius, color(&theme.background));
        let top = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 25.));
        p.rect_filled(top, theme.radius, color(&theme.raised));
        paint_logo(
            ui,
            Rect::from_min_size(rect.min + Vec2::new(8., 6.), Vec2::splat(13.)),
            color(&theme.accent),
        );
        p.text(
            rect.min + Vec2::new(28., 12.),
            Align2::LEFT_CENTER,
            "disco",
            art::bold(10.),
            fg,
        );
        for (i, icon) in [19, 20, 21].into_iter().enumerate() {
            self.art.paint(
                ui,
                icon,
                Rect::from_center_size(
                    Pos2::new(rect.right() - 45. + i as f32 * 17., rect.top() + 12.),
                    Vec2::splat(9.),
                ),
                muted,
            );
        }
        let side = if layout.categories { 32. } else { 0. };
        let prop = if layout.properties { 66. } else { 0. };
        let body = Rect::from_min_max(rect.min + Vec2::new(0., 25.), rect.max - Vec2::new(0., 12.));
        if layout.categories {
            let nav = Rect::from_min_size(body.min, Vec2::new(side, body.height()));
            p.rect_filled(nav, 0, color(&theme.panel));
            p.vline(nav.right(), nav.y_range(), border);
            for (i, icon) in [1, 2, 3, 4, 5].into_iter().enumerate() {
                let y = nav.top() + 21. + i as f32 * 29.;
                if i == 2 {
                    p.rect_filled(
                        Rect::from_min_size(Pos2::new(nav.left(), y - 13.), Vec2::new(side, 26.)),
                        0,
                        color(&theme.raised),
                    );
                    p.rect_filled(
                        Rect::from_min_size(Pos2::new(nav.left(), y - 13.), Vec2::new(2., 26.)),
                        0,
                        color(&theme.accent),
                    );
                }
                self.art.paint(
                    ui,
                    icon,
                    Rect::from_center_size(
                        Pos2::new(nav.center().x, y),
                        Vec2::splat(16. * layout.icon_scale),
                    ),
                    fg,
                );
            }
        }
        let table = Rect::from_min_max(
            body.min + Vec2::new(side + 7., 7.),
            body.max - Vec2::new(prop + 7., 7.),
        );
        p.text(
            table.min + Vec2::new(2., 4.),
            Align2::LEFT_TOP,
            "‹   /   home   ›",
            FontId::proportional(8.),
            muted,
        );
        p.hline(table.x_range(), table.top() + 21., border);
        let row_h = if layout.compact { 19. } else { 23. };
        for i in 0..5 {
            let row = Rect::from_min_size(
                table.min + Vec2::new(0., 27. + i as f32 * row_h),
                Vec2::new(table.width(), row_h),
            );
            if layout.striped_rows && i % 2 == 1 {
                p.rect_filled(
                    row,
                    0,
                    mix(color(&theme.background), color(&theme.raised), 0.3),
                );
            }
            if i == 1 {
                p.rect_filled(
                    row,
                    theme.radius,
                    mix(color(&theme.panel), color(&theme.accent), 0.31),
                );
                p.rect_filled(
                    Rect::from_min_size(row.min, Vec2::new(2., row_h)),
                    0,
                    color(&theme.accent),
                );
            }
            let indent = if i == 0 { 0. } else { 7. };
            p.text(
                row.min + Vec2::new(4. + indent, row_h * 0.5),
                Align2::LEFT_CENTER,
                "›",
                FontId::proportional(10.),
                muted,
            );
            self.art.paint(
                ui,
                0,
                Rect::from_center_size(
                    row.min + Vec2::new(20. + indent, row_h * 0.5),
                    Vec2::splat(13. * layout.icon_scale),
                ),
                C::WHITE,
            );
            p.line_segment(
                [
                    row.min + Vec2::new(32. + indent, row_h * 0.5),
                    row.min + Vec2::new(65. + (i % 3) as f32 * 8., row_h * 0.5),
                ],
                Stroke::new(3., muted),
            );
            p.line_segment(
                [
                    Pos2::new(row.right() - 22., row.center().y),
                    Pos2::new(row.right() - 5., row.center().y),
                ],
                Stroke::new(2., muted),
            );
            if layout.row_separators {
                p.hline(
                    row.x_range(),
                    row.bottom(),
                    Stroke::new(0.5, color(&theme.border)),
                );
            }
        }
        if layout.properties {
            let detail = Rect::from_min_max(Pos2::new(body.right() - prop, body.top()), body.max);
            p.rect_filled(detail, 0, color(&theme.panel));
            p.vline(detail.left(), detail.y_range(), border);
            let tile = Rect::from_min_size(detail.min + Vec2::new(10., 14.), Vec2::splat(44.));
            p.rect_filled(tile, theme.radius, color(&theme.raised));
            self.art.paint(ui, 0, tile.shrink(7.), C::WHITE);
            for i in 0..5 {
                let y = detail.top() + 77. + i as f32 * 14.;
                p.line_segment(
                    [
                        Pos2::new(detail.left() + 9., y),
                        Pos2::new(detail.right() - 9., y),
                    ],
                    Stroke::new(if i == 0 { 4. } else { 2. }, muted),
                );
            }
        }
        p.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left(), rect.bottom() - 12.),
                Vec2::new(rect.width(), 12.),
            ),
            0,
            color(&theme.raised),
        );
        p.rect_stroke(rect, theme.radius, border, egui::StrokeKind::Inside);
    }
    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut opened = true;
        let mut save = false;
        let mut cancel = false;
        let mut theme = self
            .theme_draft
            .clone()
            .unwrap_or_else(|| self.theme.clone());
        let mut layout = self.layout.clone();
        let mut action: Option<(&str, bool)> = None;
        let t = self.theme.clone();
        egui::Window::new("disco-preferences")
            .fade_in(self.layout.animations)
            .fade_out(self.layout.animations)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .fixed_size([850., 545.])
            .frame(
                egui::Frame::new()
                    .fill(color(&t.panel))
                    .stroke(Stroke::new(1., color(&t.border)))
                    .corner_radius(t.radius)
                    .inner_margin(0.),
            )
            .anchor(Align2::CENTER_CENTER, [0., 0.])
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                let (window_rect, _) =
                    ui.allocate_exact_size(Vec2::new(850., 545.), Sense::hover());
                let head = Rect::from_min_size(window_rect.min, Vec2::new(850., 40.));
                ui.painter().rect_filled(
                    head,
                    t.radius,
                    mix(color(&t.panel), color(&t.raised), 0.3),
                );
                paint_logo(
                    ui,
                    Rect::from_min_size(head.min + Vec2::new(14., 10.), Vec2::splat(20.)),
                    color(&t.accent),
                );
                ui.painter().text(
                    head.min + Vec2::new(44., 20.),
                    Align2::LEFT_CENTER,
                    format!(
                        "disco — {}",
                        if self.settings_page == 0 {
                            crate::tr!("Оформлення", "Appearance")
                        } else if self.settings_page == 1 {
                            crate::tr!("Інтерфейс", "Interface")
                        } else {
                            crate::tr!("Про програму", "About")
                        }
                    ),
                    art::bold(15.),
                    color(&t.text),
                );
                ui.painter().hline(
                    head.x_range(),
                    head.bottom(),
                    Stroke::new(1., color(&t.border)),
                );
                ui.scope_builder(
                    egui::UiBuilder::new().max_rect(Rect::from_min_size(Pos2::new(head.right()-300.,head.top()+5.), Vec2::new(245.,30.))),
                    |ui| {
                        ui.horizontal(|ui| {
                            ui.label(crate::tr!("Мова", "Language"));
                            language_picker(ui, &mut layout.language);
                        });
                    },
                );
                let close = Rect::from_center_size(
                    Pos2::new(head.right() - 22., head.center().y),
                    Vec2::splat(30.),
                );
                self.art.paint(ui, 21, close.shrink(8.), color(&t.muted));
                if ui
                    .interact(
                        close,
                        ui.scope_id().with("close_preferences"),
                        Sense::click(),
                    )
                    .clicked()
                {
                    opened = false;
                }
                let body = Rect::from_min_size(
                    window_rect.min + Vec2::new(0., 40.),
                    Vec2::new(850., 386.),
                );
                let nav = Rect::from_min_size(body.min, Vec2::new(153., body.height()));
                ui.painter().vline(
                    nav.right(),
                    nav.y_range(),
                    Stroke::new(1., color(&t.border)),
                );
                for (i, name, icon) in [
                    (0, crate::tr!("Оформлення", "Appearance"), 6),
                    (1, crate::tr!("Інтерфейс", "Interface"), 7),
                    (2, crate::tr!("Про програму", "About"), 17),
                ] {
                    let r = Rect::from_min_size(
                        nav.min + Vec2::new(0., 13. + i as f32 * 45.),
                        Vec2::new(153., 41.),
                    );
                    let b = ui.interact(r, ui.scope_id().with(("settings_nav", i)), Sense::click());
                    let active = self.settings_page == i;
                    if active || b.hovered() {
                        ui.painter().rect_filled(r, 0, color(&t.raised));
                    }
                    if active {
                        ui.painter().rect_filled(
                            Rect::from_min_size(r.min, Vec2::new(3., r.height())),
                            0,
                            color(&t.accent),
                        );
                    }
                    self.art.paint(
                        ui,
                        icon,
                        Rect::from_min_size(r.min + Vec2::new(15., 10.), Vec2::splat(20.)),
                        color(&t.text),
                    );
                    ui.painter().text(
                        r.min + Vec2::new(45., 20.),
                        Align2::LEFT_CENTER,
                        name,
                        FontId::proportional(13.),
                        color(&t.text),
                    );
                    if b.clicked() {
                        self.settings_page = i;
                    }
                }
                let content = Rect::from_min_max(
                    body.min + Vec2::new(174., 15.),
                    body.max - Vec2::new(20., 5.),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(content)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                    |ui| {
                        ui.style_mut()
                            .text_styles
                            .insert(egui::TextStyle::Body, FontId::proportional(14.));
                        ui.style_mut()
                            .text_styles
                            .insert(egui::TextStyle::Button, FontId::proportional(13.));
                        ui.label(
                            RichText::new(if self.settings_page == 0 {
                                crate::tr!("Оформлення", "Appearance")
                            } else if self.settings_page == 1 {
                                crate::tr!("Інтерфейс", "Interface")
                            } else {
                                crate::tr!("Про програму", "About")
                            })
                            .font(art::bold(19.)),
                        );
                        ui.add_space(15.);
                        if self.settings_page < 2 {
                            egui::ScrollArea::vertical()
                                .id_salt("preferences_body")
                                .max_height(317.)
                                .show(ui, |ui| {
                                    ui.columns(2, |cols| {
                                        cols[0].spacing_mut().item_spacing.y =
                                            if self.settings_page == 0 { 10. } else { 4. };
                                        if self.settings_page == 0 {
                                            let previous = theme.name.clone();
                                            cols[0].horizontal(|ui| {
                                                ui.add_sized(
                                                    [67., 30.],
                                                    egui::Label::new(
                                                        RichText::new(crate::tr!("Тема", "Theme")).size(13.),
                                                    ),
                                                );
                                                egui::ComboBox::from_id_salt("theme_select")
                                                    .width(200.)
                                                    .selected_text(&theme.name)
                                                    .show_ui(ui, |ui| {
                                                        for name in ["Carbon", "Ember", "Noir"] {
                                                            ui.selectable_value(
                                                                &mut theme.name,
                                                                name.into(),
                                                                name,
                                                            );
                                                        }
                                                    });
                                            });
                                            if previous != theme.name {
                                                theme = Theme::preset(&theme.name);
                                            }
                                            cols[0].add_space(10.);
                                            for (name, value) in [
                                                (crate::tr!("Акцент", "Accent"), &mut theme.accent),
                                                (crate::tr!("Фон", "Background"), &mut theme.background),
                                                (crate::tr!("Панелі", "Panels"), &mut theme.panel),
                                                (crate::tr!("Текст", "Text"), &mut theme.text),
                                            ] {
                                                theme_color_row(&mut cols[0], name, value, &t);
                                            }
                                            cols[0].add_space(4.);
                                            egui::CollapsingHeader::new(
                                                RichText::new(crate::tr!("Додаткові кольори", "Additional colors"))
                                                    .size(12.)
                                                    .color(color(&t.muted)),
                                            )
                                            .show(
                                                &mut cols[0],
                                                |ui| {
                                                    for (name, value) in [
                                                        (crate::tr!("Кошик", "Trash"), &mut theme.trash),
                                                        (crate::tr!("Підписи", "Labels"), &mut theme.muted),
                                                        (crate::tr!("Межі", "Borders"), &mut theme.border),
                                                        (crate::tr!("Підняття", "Raised"), &mut theme.raised),
                                                    ] {
                                                        theme_color_row(ui, name, value, &t);
                                                    }
                                                    ui.add(
                                                        egui::Slider::new(
                                                            &mut theme.radius,
                                                            0..=12,
                                                        )
                                                        .text(crate::tr!("Радіус, px", "Corner radius, px")),
                                                    );
                                                },
                                            );
                                            cols[1].label(
                                                RichText::new(crate::tr!("Попередній перегляд", "Preview"))
                                                    .size(12.)
                                                    .color(color(&t.muted)),
                                            );
                                            self.preview(&mut cols[1], &theme, &layout);
                                            cols[1].add_space(9.);
                                            cols[1].label(
                                                RichText::new(crate::tr!("Контраст тексту: {:.1}:1", "Text contrast: {:.1}:1",
                                                    settings::contrast(
                                                        color(&theme.text),
                                                        color(&theme.background)
                                                    )
                                                ))
                                                .size(12.)
                                                .color(color(&t.muted)),
                                            );
                                        } else {
                                            cols[0]
                                                .label(RichText::new(crate::tr!("Щільність рядків", "Row density")).size(13.));
                                            cols[0].horizontal(|ui| {
                                                ui.selectable_value(
                                                    &mut layout.compact,
                                                    false,
                                                    crate::tr!("Звичайна", "Comfortable"),
                                                );
                                                ui.selectable_value(
                                                    &mut layout.compact,
                                                    true,
                                                    crate::tr!("Компактна", "Compact"),
                                                );
                                            });
                                            cols[0].add_space(4.);
                                            cols[0]
                                                .label(RichText::new(crate::tr!("Масштаб тексту", "Text size")).size(13.));
                                            let mut pct = layout.text_scale * 100.;
                                            cols[0].add(
                                                egui::Slider::new(&mut pct, 85.0..=140.0)
                                                    .suffix("%")
                                                    .fixed_decimals(0),
                                            );
                                            layout.text_scale = pct / 100.;
                                            cols[0].add_space(4.);
                                            check_row(
                                                &mut cols[0],
                                                &mut layout.categories,
                                                crate::tr!("Показувати категорії", "Show categories"),
                                                true,
                                                &t,
                                            );
                                            check_row(
                                                &mut cols[0],
                                                &mut layout.properties,
                                                crate::tr!("Показувати властивості", "Show properties"),
                                                true,
                                                &t,
                                            );
                                            cols[0].add_space(5.);
                                            cols[0]
                                                .label(RichText::new(crate::tr!("Колонки у списку", "List columns")).size(13.));
                                            let mut always = true;
                                            check_row(
                                                &mut cols[0],
                                                &mut always,
                                                crate::tr!("Назва", "Name"),
                                                false,
                                                &t,
                                            );
                                            check_row(
                                                &mut cols[0],
                                                &mut always,
                                                crate::tr!("Розмір", "Size"),
                                                false,
                                                &t,
                                            );
                                            check_row(
                                                &mut cols[0],
                                                &mut layout.show_count,
                                                crate::tr!("Кількість файлів", "File count"),
                                                true,
                                                &t,
                                            );
                                            egui::CollapsingHeader::new(RichText::new(crate::tr!("Рух і відгук", "Motion & feedback")).size(12.).color(color(&t.muted)))
                                                .show(&mut cols[0], |ui| {
                                                    check_row(ui, &mut layout.animations, crate::tr!("Анімації інтерфейсу", "Interface animations"), true, &t);
                                                    ui.add_enabled(layout.animations, egui::Slider::new(&mut layout.motion_speed, 0.5..=2.).text(crate::tr!("Швидкість", "Speed")).suffix("×").fixed_decimals(2));
                                                    ui.label(RichText::new(crate::tr!("Вимкни анімації для зменшеного руху. У спокої вони не працюють.", "Disable animations for reduced motion. They stop when idle.")).size(12.).color(color(&t.muted)));
                                                });
                                            cols[1].label(
                                                RichText::new(crate::tr!("Попередній перегляд", "Preview"))
                                                    .size(12.)
                                                    .color(color(&t.muted)),
                                            );
                                            self.preview(&mut cols[1], &theme, &layout);
                                            egui::CollapsingHeader::new(RichText::new(crate::tr!("Навігація та сканування", "Navigation & scanning")).size(12.).color(color(&t.muted)))
                                                .show(&mut cols[1], |ui| {
                                                    ui.label(RichText::new(crate::tr!("Клік по папці", "Folder clicks")).size(13.));
                                                    ui.radio_value(&mut layout.single_click_expand, true, crate::tr!("Один клік — розгорнути дерево", "Single click expands the tree"));
                                                    ui.radio_value(&mut layout.single_click_expand, false, crate::tr!("Подвійний клік — відкрити папку", "Double click opens a folder"));
                                                    check_row(ui, &mut layout.save_scan, crate::tr!("Зберігати й відновлювати останній скан", "Save and restore the last scan"), true, &t);
                                                    check_row(ui, &mut layout.auto_refresh, crate::tr!("Автооновлення при зміні SSD", "Refresh when disk usage changes"), true, &t);
                                                    ui.label(RichText::new(crate::tr!("Зміна від 64 МіБ → оновлення через 8 с, не частіше разу на хвилину. Однаковий обсяг не гарантує відсутності змін — доступне ручне оновлення.", "Changes of 64 MiB or more trigger a refresh after 8 s, at most once a minute. Unchanged usage does not guarantee unchanged files; you can refresh manually.")).size(12.).color(color(&t.muted)));
                                                    check_row(ui, &mut layout.reuse_scan, crate::tr!("Перевіряти зміни перед обходом", "Check for changes before rescanning"), true, &t);
                                                    check_row(ui, &mut layout.gentle_scan, crate::tr!("Бережний режим сканування", "Gentle scanning mode"), true, &t);
                                                    ui.label(RichText::new(crate::tr!("Бічні кнопки миші: назад / уперед. Shift + Оновити: повний обхід.", "Mouse side buttons: back / forward. Shift + Refresh: full scan.")).size(12.).color(color(&t.muted)));
                                                });
                                            egui::CollapsingHeader::new(
                                                RichText::new(crate::tr!("Панелі та диск", "Panels & drive"))
                                                    .size(12.)
                                                    .color(color(&t.muted)),
                                            )
                                            .show(
                                                &mut cols[1],
                                                |ui| {
                                                    ui.add(
                                                        egui::Slider::new(
                                                            &mut layout.category_width,
                                                            155.0..=280.0,
                                                        )
                                                        .text(crate::tr!("Категорії, px", "Categories, px")),
                                                    );
                                                    ui.add(
                                                        egui::Slider::new(
                                                            &mut layout.property_width,
                                                            240.0..=380.0,
                                                        )
                                                        .text(crate::tr!("Властивості, px", "Properties, px")),
                                                    );
                                                    ui.label(crate::tr!("Назва диска", "Drive name"));
                                                    ui.add(
                                                        egui::TextEdit::singleline(
                                                            &mut layout.drive_name,
                                                        )
                                                        .hint_text(&self.drive_name)
                                                        .char_limit(64),
                                                    );
                                                },
                                            );
                                            egui::CollapsingHeader::new(
                                                RichText::new(crate::tr!("Деталі вигляду", "Display details"))
                                                    .size(12.)
                                                    .color(color(&t.muted)),
                                            )
                                            .show(
                                                &mut cols[1],
                                                |ui| {
                                                    ui.label(
                                                        RichText::new(
                                                            crate::tr!("Розмір іконок у панелях і дереві", "Icon size in panels and tree"),
                                                        )
                                                        .size(12.),
                                                    );
                                                    let mut pct = layout.icon_scale * 100.;
                                                    ui.add(
                                                        egui::Slider::new(&mut pct, 80.0..=140.0)
                                                            .suffix("%")
                                                            .fixed_decimals(0),
                                                    );
                                                    layout.icon_scale = pct / 100.;
                                                    check_row(
                                                        ui,
                                                        &mut layout.category_sizes,
                                                        crate::tr!("Розміри під категоріями", "Sizes below categories"),
                                                        true,
                                                        &t,
                                                    );
                                                    check_row(
                                                        ui,
                                                        &mut layout.row_separators,
                                                        crate::tr!("Розділювачі рядків", "Row separators"),
                                                        true,
                                                        &t,
                                                    );
                                                    check_row(
                                                        ui,
                                                        &mut layout.striped_rows,
                                                        crate::tr!("Чергування фону рядків", "Alternating row backgrounds"),
                                                        true,
                                                        &t,
                                                    );
                                                    check_row(
                                                        ui,
                                                        &mut layout.drive_model,
                                                        crate::tr!("Модель диска в шапці", "Drive model in header"),
                                                        true,
                                                        &t,
                                                    );
                                                },
                                            );
                                        }
                                    });
                                });
                        } else {
                            ui.add_space(10.);
                            ui.horizontal(|ui| {
                                logo(ui, 45., color(&t.accent));
                                ui.label(RichText::new(format!("disco {}", env!("CARGO_PKG_VERSION"))).font(art::bold(25.)));
                            });
                            ui.add_space(18.);
                            ui.label(crate::tr!("Аналізатор дискового простору", "Disk space analyzer"));
                            ui.label(
                                RichText::new(crate::tr!("Папки, великі файли та контрольоване очищення.", "Folders, large files, and cleanup with confirmation."))
                                    .color(color(&t.muted))
                                    .size(14.),
                            );
                        }
                    },
                );
                let foot = Rect::from_min_size(
                    window_rect.min + Vec2::new(0., 426.),
                    Vec2::new(850., 119.),
                );
                ui.painter().hline(
                    foot.x_range(),
                    foot.top(),
                    Stroke::new(1., color(&t.border)),
                );
                if self.settings_page < 2 {
                    let name = if self.settings_page == 0 {
                        "theme.toml"
                    } else {
                        "layout.toml"
                    };
                    for (x, w, icon, text, which) in [
                        (
                            174.,
                            150.,
                            15,
                            if self.settings_page == 0 {
                                crate::tr!("Імпортувати тему", "Import theme")
                            } else {
                                crate::tr!("Імпортувати", "Import")
                            }
                            .to_string(),
                            0,
                        ),
                        (333., 258., 15, crate::tr!("Експортувати {name}", "Export {name}", name = name), 1),
                        (600., 225., 16, crate::tr!("Скинути", "Reset").to_string(), 2),
                    ] {
                        let r =
                            Rect::from_min_size(foot.min + Vec2::new(x, 12.), Vec2::new(w, 34.));
                        if visual_button(ui, &self.art, r, icon, &text, true, &t).clicked() {
                            match which {
                                0 => action = Some((name, false)),
                                1 => action = Some((name, true)),
                                _ => {
                                    if self.settings_page == 0 {
                                        theme = Theme::default();
                                    } else {
                                        layout = Layout { language: layout.language, ..Layout::default() };
                                    }
                                }
                            }
                        }
                    }
                }
                let caption = if self.settings_error.is_empty() {
                    crate::tr!("Збереження — кнопкою «Зберегти»", "Click Save to keep your changes")
                } else {
                    &self.settings_error
                };
                ui.painter()
                    .with_clip_rect(Rect::from_min_size(
                        foot.min + Vec2::new(174., 58.),
                        Vec2::new(360., 42.),
                    ))
                    .text(
                        foot.min + Vec2::new(174., 77.),
                        Align2::LEFT_CENTER,
                        caption,
                        FontId::proportional(11.),
                        color(&t.muted),
                    );
                let back =
                    Rect::from_min_size(foot.min + Vec2::new(600., 60.), Vec2::new(100., 32.));
                let accept =
                    Rect::from_min_size(foot.min + Vec2::new(710., 60.), Vec2::new(115., 32.));
                cancel =
                    visual_button(ui, &self.art, back, usize::MAX, crate::tr!("Скасувати", "Cancel"), true, &t).clicked();
                save = visual_button(ui, &self.art, accept, usize::MAX, crate::tr!("Зберегти", "Save"), true, &t)
                    .clicked();
            });
        self.theme_draft = Some(theme.clone());
        if theme != self.theme || layout != self.layout {
            match theme.validate().and_then(|_| layout.validate()) {
                Ok(()) => {
                    self.theme = theme;
                    self.layout = layout;
                    self.settings_error.clear();
                }
                Err(e) => self.settings_error = e,
            }
        }
        if let Some((name, export)) = action {
            let path = if export {
                rfd::FileDialog::new()
                    .add_filter("TOML", &["toml"])
                    .set_file_name(name)
                    .save_file()
            } else {
                rfd::FileDialog::new()
                    .add_filter("TOML", &["toml"])
                    .pick_file()
            };
            if let Some(p) = path {
                let result = if export {
                    if name == "theme.toml" {
                        settings::write(&p, &self.theme)
                    } else {
                        settings::write(&p, &self.layout)
                    }
                } else if name == "theme.toml" {
                    settings::read_theme(&p).map(|t| {
                        self.theme = t.clone();
                        self.theme_draft = Some(t);
                    })
                } else {
                    settings::read_layout(&p).map(|l| self.layout = l)
                };
                self.settings_error = match result {
                    Ok(()) => {
                        if export {
                            crate::tr!("Експортовано", "Exported").into()
                        } else {
                            crate::tr!(
                                "Імпортовано. Натисни «Зберегти» для постійного застосування.",
                                "Imported. Click Save to apply permanently."
                            )
                            .into()
                        }
                    }
                    Err(e) => e,
                };
            }
        }
        if save {
            let p = settings::config_dir();
            match self
                .theme_draft
                .as_ref()
                .unwrap_or(&self.theme)
                .validate()
                .and_then(|_| self.layout.validate())
                .and_then(|_| settings::write(&p.join("theme.toml"), &self.theme))
                .and_then(|_| settings::write(&p.join("layout.toml"), &self.layout))
            {
                Ok(()) => {
                    self.settings_open = false;
                    self.settings_snapshot = None;
                    self.theme_draft = None;
                    self.message = crate::tr!("Налаштування збережено", "Settings saved").into();
                }
                Err(e) => self.settings_error = e,
            }
        }
        if cancel || !opened {
            if let Some((t, l)) = self.settings_snapshot.take() {
                self.theme = t;
                self.layout = l;
            }
            self.settings_open = false;
            self.theme_draft = None;
        }
    }
}
fn open(p: &Path) {
    let _ = std::process::Command::new("xdg-open").arg(p).spawn();
}
fn chevron(ui: &egui::Ui, c: Pos2, down: bool, color: C) {
    let points = if down {
        vec![
            c + Vec2::new(-4., -2.),
            c + Vec2::new(0., 2.),
            c + Vec2::new(4., -2.),
        ]
    } else {
        vec![
            c + Vec2::new(-2., -4.),
            c + Vec2::new(2., 0.),
            c + Vec2::new(-2., 4.),
        ]
    };
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(1.5, color)));
}
fn logo(ui: &mut egui::Ui, size: f32, c: C) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = ui.painter();
    let center = r.center();
    for i in 0..3 {
        let start = i as f32 * std::f32::consts::TAU / 3. + 0.08;
        let end = (i + 1) as f32 * std::f32::consts::TAU / 3. - 0.08;
        let points = (0..20)
            .map(|j| {
                let a = start + (end - start) * j as f32 / 19.;
                center + Vec2::angled(a) * size * 0.35
            })
            .collect();
        p.add(egui::Shape::line(points, Stroke::new(size * 0.2, c)));
    }
}
impl eframe::App for Disco {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.poll();
        if let Some(result) = self.recycle.poll(ui.ctx()) {
            self.refresh_drive();
            if result.changed && self.report.is_some() && !self.busy && self.trash_rx.is_none() {
                self.start_job(false, true, true);
            }
        }
        if !self.demo && self.trash_rx.is_none() && !self.confirm && self.purge_targets.is_none() {
            self.recycle.maintain(ui.ctx());
        }
        let previous_location = self.nav_state();
        self.nav_traversed = false;
        if !self.settings_open
            && !self.confirm
            && self.purge_targets.is_none()
            && !self.busy
            && !self.recycle.busy
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
        if self.busy
            || self.trash_rx.is_some()
            || self.restore_requested_root.is_some()
            || self.recycle.loading
            || self.recycle.busy
        {
            ctx.request_repaint_after(Duration::from_millis(200));
        } else {
            ctx.request_repaint_after(Duration::from_secs(3));
        }
        let t = self.theme.clone();
        egui::Panel::top("brand")
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.titlebar(ui));
        egui::Panel::top("drive")
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.drive_header(ui));
        egui::Panel::bottom("selection")
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.footer(ui));
        if self.layout.categories {
            egui::Panel::left("categories")
                .exact_size(self.layout.category_width)
                .resizable(false)
                .frame(egui::Frame::new().fill(color(&t.panel)).inner_margin(0.))
                .show(ui, |ui| self.sidebar(ui));
        }
        if self.layout.properties {
            egui::Panel::right("inspector")
                .exact_size(self.layout.property_width)
                .resizable(false)
                .frame(
                    egui::Frame::new()
                        .fill(color(&t.panel))
                        .inner_margin(egui::Margin::symmetric(18, 12)),
                )
                .show(ui, |ui| self.inspector(ui));
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(color(&t.background))
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(ui, |ui| {
                if self.view == View::Trash {
                    self.recycle_content(ui);
                    return;
                }
                self.scope_controls(ui);
                if !self.layout.categories {
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
                                .selectable_label(self.category == cat, cat.name())
                                .clicked()
                            {
                                self.category = cat;
                                self.view = View::Folders;
                                self.table_root = None;
                            }
                        }
                    });
                }
                ui.add_space(8.);
                let enter = motion::enter(
                    ui,
                    "content-enter",
                    (
                        self.nav_state(),
                        self.report_revision,
                        self.sort_size,
                        self.descending,
                    ),
                );
                ui.scope(|ui| {
                    ui.multiply_opacity(0.55 + enter * 0.45);

                    if self.view == View::Folders {
                        self.folder_table(ui);
                    } else {
                        self.other_content(ui);
                    }
                });
            });
        if self.view == View::Trash
            && previous_location.view != View::Trash
            && !self.demo
            && !self.recycle.loading
            && !self.recycle.busy
        {
            self.recycle.refresh(&ctx);
        }
        self.confirmation(&ctx);
        self.bin_confirmation(&ctx);
        self.settings_window(&ctx);
        if i18n::current() != self.layout.language {
            i18n::set_current(self.layout.language);
            self.message.clear();
            self.recycle.relocalize_notice();
            self.refresh_drive();
            ctx.request_repaint();
        }
        if !self.demo
            && self.layout.auto_refresh
            && !self.busy
            && self.restore_requested_root.is_none()
            && self.trash_rx.is_none()
            && !self.confirm
            && self.purge_targets.is_none()
            && !self.recycle.busy
            && self.view != View::Trash
            && !self.settings_open
            && self.selected.is_empty()
            && self.view != View::Duplicates
            && self.report.as_ref().is_some_and(|r| {
                r.complete
                    && r.folders
                        .first()
                        .is_some_and(|f| f.path == Path::new(&self.root))
            })
            && self.auto_refresh.due(Instant::now())
        {
            self.start_job(false, true, true);
            ctx.request_repaint();
        }
        if self.nav_traversed {
            ctx.request_repaint();
        }
        if self.report.is_some()
            && !self.busy
            && !self.nav_traversed
            && self.nav_state() != previous_location
        {
            self.history.visit(previous_location);
            ctx.request_repaint();
        }

        if let Some(probe) = &mut self.frame_probe {
            probe.tick(ui, frame.info().cpu_usage);
        }
        resize_edges(ui);
        if let Some(path) = self.screenshot.clone() {
            if self.shots == 0
                && self.restore_requested_root.is_none()
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
        self.control.cancel.store(true, Ordering::Relaxed);
    }
}
fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("disco {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--snapshot-info") {
        i18n::set_current(settings::load().1.language);
        let started = Instant::now();
        match snapshot::load() {
            Ok(Some(saved)) => {
                let loaded = started.elapsed();
                let index = Index::new(&saved.report);
                println!(
                    "folders={} saved_at={} disk_baseline={} load_ms={:.1} ready_ms={:.1} indexed={}",
                    saved.report.folders.len(),
                    saved.saved_at,
                    saved.disk.is_some(),
                    loaded.as_secs_f64() * 1000.,
                    started.elapsed().as_secs_f64() * 1000.,
                    index.children.len()
                );
            }
            Ok(None) => println!("No saved snapshot"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    if let Some(i) = args.iter().position(|x| x == "--scan") {
        i18n::set_current(settings::load().1.language);
        if let Some(p) = args.get(i + 1) {
            let c = Control::default();
            c.gentle.store(true, Ordering::Relaxed);
            background_priority();
            match scan(Path::new(p), &c, |_| {}) {
                Ok(r) => {
                    println!(
                        "{} entries | {} | {:.3}s | skipped {} | hardlinks {}",
                        r.entries,
                        human(r.folders[0].bytes),
                        r.elapsed.as_secs_f64(),
                        r.skipped,
                        r.hardlinks
                    );
                    if args.iter().any(|a| a == "--save-scan") {
                        if let Err(e) = snapshot::save(&r) {
                            eprintln!("{e}");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        } else {
            eprintln!("Usage: disco --scan /absolute/path");
            std::process::exit(2);
        }
        return Ok(());
    }
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1360., 660.])
            .with_min_inner_size([1140., 620.])
            .with_decorations(false)
            .with_app_id("ua.disco")
            .with_title(format!("disco {}", env!("CARGO_PKG_VERSION")))
            .with_icon(art::app_icon()),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    if args.iter().any(|a| a == "--demo") && args.iter().any(|a| a == "--frame-probe") {
        let x = args
            .iter()
            .position(|a| a == "--frame-probe-x")
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(80.);
        options.viewport = options
            .viewport
            .with_position([x, 80.])
            .with_window_level(egui::WindowLevel::AlwaysOnTop);
    }
    options.glow_options.vsync = true;
    eframe::run_native(
        "disco",
        options,
        Box::new(|cc| Ok(Box::new(Disco::new(cc)))),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn category_sizes_do_not_double_count() {
        let r = demo_report();
        let index = Index::new(&r);
        let total: u64 = index.totals.values().sum();
        assert_eq!(total, r.folders[0].bytes);
        assert_eq!(
            *index.totals.get(&Category::Cache).unwrap(),
            32 * 1024u64.pow(3)
        );
    }
    #[test]
    fn delete_modal_defaults_to_cancel_and_requires_explicit_confirmation() {
        let path = std::env::temp_dir().join(format!("disco-modal-{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        let target = deletion::Target::capture(&path, 1024).unwrap();
        let ctx = egui::Context::default();
        art::fonts(&ctx);
        let theme = Theme::default();
        let frame = |events: Vec<egui::Event>, fresh| {
            let mut responses = None;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    responses = Some(delete_dialog(
                        ui.ctx(),
                        std::slice::from_ref(&target),
                        &theme,
                        fresh,
                    ));
                },
            );
            output.drop_without_applying_deltas();
            responses.unwrap()
        };
        let opened = frame(vec![], true);
        assert!(!opened.inner.1.clicked());
        assert!(opened.inner.0.has_focus());
        let escape = frame(
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        assert!(escape.should_close());
        assert!(!escape.inner.1.clicked());
        let stable = frame(vec![], false);
        let pos = stable.inner.1.rect.center();
        let pressed = frame(
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            false,
        );
        assert!(!pressed.inner.1.clicked());
        let released = frame(
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        assert!(released.inner.1.clicked());
        assert!(path.exists()); // Drawing and interacting with a dialog alone cannot delete.
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn storage_bar_partitions_used_space_without_double_counting() {
        let p = storage_parts(1000, 200, 300);
        assert_eq!(
            p,
            StorageParts {
                used: 800,
                trash: 300,
                other: 500
            }
        );
        assert_eq!(p.other + p.trash + 200, 1000);
        assert_eq!(storage_parts(100, 40, 999).trash, 60);
        assert_eq!(
            storage_parts(0, 0, 9),
            StorageParts {
                used: 0,
                trash: 0,
                other: 0
            }
        );
        assert_eq!(storage_parts(10, 20, 9).used, 0);
        let p = storage_parts(u64::MAX, 1, u64::MAX);
        assert_eq!(p.other, 0);
        assert_eq!(p.trash, u64::MAX - 1);
        assert_eq!(
            plural(
                1,
                crate::tr!("файл", "file"),
                crate::tr!("файли", "files"),
                crate::tr!("файлів", "files")
            ),
            crate::tr!("файл", "file")
        );
        assert_eq!(
            plural(
                22,
                crate::tr!("файл", "file"),
                crate::tr!("файли", "files"),
                crate::tr!("файлів", "files")
            ),
            crate::tr!("файли", "files")
        );
        assert_eq!(
            plural(
                111,
                crate::tr!("файл", "file"),
                crate::tr!("файли", "files"),
                crate::tr!("файлів", "files")
            ),
            crate::tr!("файлів", "files")
        );
    }
    #[test]
    fn permission_display() {
        assert_eq!(permissions(0o755), "rwxr-xr-x");
        assert_eq!(permissions(0o600), "rw-------");
    }
}
fn delete_dialog(
    ctx: &egui::Context,
    targets: &[deletion::Target],
    t: &Theme,
    fresh: bool,
) -> egui::ModalResponse<(egui::Response, egui::Response)> {
    egui::Modal::new(egui::Id::unique("delete-confirmation")).show(ctx, |ui| {
        ui.set_width(480.);
        ui.heading(if targets.len() == 1 { crate::tr!("Видалити папку?", "Delete folder?") } else { crate::tr!("Видалити вибрані папки?", "Delete selected folders?") });
        ui.add_space(8.);
        ui.label(crate::tr!("Папка та весь її вміст будуть переміщені у кошик.", "The folder and all its contents will be moved to Trash."));
        ui.add_space(10.);
        egui::ScrollArea::vertical().max_height(180.).show(ui, |ui| {
            for target in targets {
                ui.label(RichText::new(label(&target.path)).strong());
                ui.add(egui::Label::new(target.path.to_string_lossy()).wrap());
                ui.add_space(6.);
            }
        });
        let total = targets.iter().fold(0u64, |sum,t| sum.saturating_add(t.bytes));
        ui.label(RichText::new(crate::tr!("За останнім сканом: {}", "According to the last scan: {}", display_size(total))).strong());
        ui.add_space(8.);
        ui.label(RichText::new(crate::tr!("Повернути файли можна з кошика у Dolphin / Files. Місце на SSD звільниться після спорожнення кошика.", "You can restore files from Trash in Dolphin / Files. Disk space is freed after emptying Trash.")).size(13.).color(color(&t.muted)));
        ui.add_space(16.);
        ui.horizontal(|ui| {
            let cancel = ui.button(crate::tr!("Скасувати", "Cancel"));
            if fresh { cancel.request_focus(); }
            let yes = ui.add(egui::Button::new(RichText::new(crate::tr!("Так, перемістити у кошик", "Yes, move to Trash")).color(C::WHITE)).fill(color(&t.accent)));
            (cancel, yes)
        }).inner
    })
}
#[derive(Debug, PartialEq)]
struct StorageParts {
    used: u64,
    trash: u64,
    other: u64,
}
fn storage_parts(total: u64, free: u64, trash: u64) -> StorageParts {
    let used = total.saturating_sub(free.min(total));
    let trash = trash.min(used);
    StorageParts {
        used,
        trash,
        other: used - trash,
    }
}
fn plural<'a>(n: u64, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    if i18n::current() == i18n::Language::En {
        return if n == 1 { one } else { many };
    }
    if (11..=14).contains(&(n % 100)) {
        many
    } else {
        match n % 10 {
            1 => one,
            2..=4 => few,
            _ => many,
        }
    }
}
fn language_picker(ui: &mut egui::Ui, language: &mut i18n::Language) {
    egui::ComboBox::from_id_salt("interface-language")
        .width(135.)
        .selected_text(language.name())
        .show_ui(ui, |ui| {
            for choice in [i18n::Language::Uk, i18n::Language::En] {
                ui.selectable_value(language, choice, choice.name());
            }
        });
}
fn display_size(n: u64) -> String {
    human(n).replace(".0 ", " ")
}
fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}
fn shorten(s: &str, limit: usize) -> String {
    if s.chars().count() <= limit {
        s.to_string()
    } else {
        format!(
            "{}…",
            s.chars().take(limit.saturating_sub(1)).collect::<String>()
        )
    }
}
fn mix(a: C, b: C, t: f32) -> C {
    let lerp = |x: u8, y: u8| ((x as f32) * (1. - t) + (y as f32) * t).round() as u8;
    C::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}
fn paint_logo(ui: &egui::Ui, r: Rect, c: C) {
    for i in 0..3 {
        let start = i as f32 * std::f32::consts::TAU / 3. + 0.08;
        let end = (i + 1) as f32 * std::f32::consts::TAU / 3. - 0.08;
        let points = (0..22)
            .map(|j| {
                r.center() + Vec2::angled(start + (end - start) * j as f32 / 21.) * r.width() * 0.35
            })
            .collect();
        ui.painter()
            .add(egui::Shape::line(points, Stroke::new(r.width() * 0.22, c)));
    }
}
fn visual_button(
    ui: &mut egui::Ui,
    art: &art::Art,
    r: Rect,
    icon: usize,
    text: &str,
    enabled: bool,
    t: &Theme,
) -> egui::Response {
    let response = ui.interact(
        r,
        ui.scope_id()
            .with(("visual", text, r.min.x.to_bits(), r.min.y.to_bits())),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hover = motion::hover(
        ui,
        ("button-hover", text, r.min.x.to_bits(), r.min.y.to_bits()),
        enabled && response.hovered(),
    );
    let press = motion::value(
        ui,
        ("button-press", text, r.min.x.to_bits(), r.min.y.to_bits()),
        if enabled && response.is_pointer_button_down_on() {
            1.
        } else {
            0.
        },
        0.5,
    );
    let fill = mix(
        mix(color(&t.panel), color(&t.raised), 0.22),
        color(&t.raised),
        hover,
    );
    let fill = mix(fill, color(&t.background), press * 0.25);
    ui.painter().rect_filled(r, t.radius, fill);
    ui.painter().rect_stroke(
        r,
        t.radius,
        Stroke::new(
            1.,
            if enabled {
                color(&t.border)
            } else {
                color(&t.border).gamma_multiply(0.65)
            },
        ),
        egui::StrokeKind::Inside,
    );
    let fg = if enabled {
        color(&t.text)
    } else {
        color(&t.muted).gamma_multiply(0.52)
    };
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(13.), fg);
    let has_icon = icon < art.len();
    let total = galley.size().x + if has_icon { 26. } else { 0. };
    let left = r.center().x - total / 2.;
    if has_icon {
        art.paint(
            ui,
            icon,
            Rect::from_center_size(Pos2::new(left + 8., r.center().y), Vec2::splat(17.)),
            fg,
        );
    }
    ui.painter().galley(
        Pos2::new(
            left + if has_icon { 26. } else { 0. },
            r.center().y - galley.size().y / 2.,
        ),
        galley,
        fg,
    );
    response
}
fn theme_color_row(ui: &mut egui::Ui, name: &str, value: &mut String, t: &Theme) {
    ui.horizontal(|ui| {
        let (label, _) = ui.allocate_exact_size(Vec2::new(74., 34.), Sense::hover());
        ui.painter().text(
            label.left_center(),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(13.),
            color(&t.text),
        );
        let mut c = color(value);
        let old = c;
        let response = ui.add_sized(
            [32., 32.],
            egui::Button::new("")
                .fill(c)
                .stroke(Stroke::new(1., color(&t.border))),
        );
        egui::Popup::from_toggle_button_response(&response).show(|ui| {
            egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque);
        });
        if c != old {
            *value = format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
        }
        ui.add_sized(
            [100., 32.],
            egui::TextEdit::singleline(value).font(FontId::proportional(13.)),
        );
    });
    ui.add_space(3.);
}
fn check_row(ui: &mut egui::Ui, value: &mut bool, text: &str, enabled: bool, t: &Theme) {
    let (r, b) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 24.),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let c = Rect::from_center_size(Pos2::new(r.left() + 9., r.center().y), Vec2::splat(17.));
    ui.painter().rect_filled(
        c,
        2,
        if *value {
            color(&t.accent).gamma_multiply(if enabled { 1. } else { 0.65 })
        } else {
            color(&t.panel)
        },
    );
    ui.painter().rect_stroke(
        c,
        2,
        Stroke::new(
            1.,
            if *value {
                color(&t.accent)
            } else {
                color(&t.border)
            },
        ),
        egui::StrokeKind::Inside,
    );
    if *value {
        ui.painter().add(egui::Shape::line(
            vec![
                c.center() + Vec2::new(-4., 0.),
                c.center() + Vec2::new(-1., 3.),
                c.center() + Vec2::new(5., -4.),
            ],
            Stroke::new(1.8, C::WHITE),
        ));
    }
    ui.painter().text(
        Pos2::new(r.left() + 28., r.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(14.),
        if enabled {
            color(&t.text)
        } else {
            color(&t.muted)
        },
    );
    if enabled && b.clicked() {
        ui.ctx().request_repaint();
        *value = !*value;
    }
}
fn resize_edges(ui: &mut egui::Ui) {
    if ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let r = ui.max_rect();
    let edges = [
        (
            Rect::from_min_size(r.min, Vec2::new(7., r.height())),
            egui::ResizeDirection::West,
            egui::CursorIcon::ResizeHorizontal,
        ),
        (
            Rect::from_min_size(
                Pos2::new(r.right() - 7., r.top()),
                Vec2::new(7., r.height()),
            ),
            egui::ResizeDirection::East,
            egui::CursorIcon::ResizeHorizontal,
        ),
        (
            Rect::from_min_size(r.min, Vec2::new(r.width(), 5.)),
            egui::ResizeDirection::North,
            egui::CursorIcon::ResizeVertical,
        ),
        (
            Rect::from_min_size(
                Pos2::new(r.left(), r.bottom() - 5.),
                Vec2::new(r.width(), 5.),
            ),
            egui::ResizeDirection::South,
            egui::CursorIcon::ResizeVertical,
        ),
    ];
    for (i, (rect, dir, cursor)) in edges.into_iter().enumerate() {
        let response = ui
            .interact(rect, ui.scope_id().with(("resize", i)), Sense::drag())
            .on_hover_cursor(cursor);
        if response.drag_started() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }
}
