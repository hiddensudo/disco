use crate::{
    deletion,
    filesystem::Properties,
    index::{Category, Index},
    navigation,
    scan::{BigFile, ChangeTracker, Control, Event, Progress, Report},
    settings::{Layout, Theme},
};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, mpsc::Receiver},
    time::Instant,
};
#[derive(Clone, Copy, PartialEq, Hash)]
pub(super) enum View {
    Folders,
    Files,
    Cleanup,
    Duplicates,
    Trash,
}
#[derive(Clone, Copy, PartialEq, Hash)]
pub(super) struct NavState {
    pub(super) view: View,
    pub(super) category: Category,
    pub(super) folder: Option<usize>,
}
#[derive(PartialEq)]
pub(super) struct RowsKey {
    pub(super) location: NavState,
    pub(super) filter: String,
    pub(super) sort_size: bool,
    pub(super) descending: bool,
    pub(super) report: u64,
    pub(super) tree: u64,
    pub(super) files: u64,
    pub(super) search: u64,
}
#[derive(Clone)]
pub(super) enum TreeRow {
    Folder(usize, usize),
    File(Arc<crate::directory_files::Entry>, usize),
    PathFolder(Arc<crate::directory_files::Entry>, usize),
    Pending(usize, usize),
    Notice(usize, usize),
}
pub(super) type FolderRows = Arc<Vec<TreeRow>>;

/// State owned by the scan part of the application.
pub(super) struct ScanState {
    pub(super) report: Option<Arc<Report>>,
    pub(super) report_revision: u64,
    pub(super) index: Index,
    pub(super) tracker: Option<ChangeTracker>,
    pub(super) snapshot_at: Option<Instant>,
    pub(super) saved_snapshot_at: Option<u64>,
    pub(super) restored_snapshot: bool,
    pub(super) restore_requested_root: Option<String>,
    pub(super) automatic_job: bool,
    pub(super) rx: Option<Receiver<Event>>,
    pub(super) control: Arc<Control>,
    pub(super) busy: bool,
    pub(super) duplicate_job: bool,
    pub(super) progress: Option<Progress>,
    pub(super) dups: Vec<Vec<BigFile>>,
}

/// State owned by the browser part of the application.
pub(super) struct BrowserState {
    pub(super) root: String,
    pub(super) table_root: Option<usize>,
    pub(super) tree_revision: u64,
    pub(super) rows_cache: Option<(RowsKey, FolderRows)>,
    pub(super) history: navigation::History<NavState>,
    pub(super) nav_traversed: bool,
    pub(super) focus: usize,
    pub(super) filter: String,
    pub(super) selected: HashSet<PathBuf>,
    pub(super) expanded: HashSet<usize>,
    pub(super) category: Category,
    pub(super) view: View,
    pub(super) properties: Option<Properties>,
    pub(super) text_input_focused: bool,
    pub(super) scroll_to: Option<PathBuf>,
    pub(super) scroll_offset: f32,
    pub(super) scroll_height: f32,
    pub(super) sort_size: bool,
    pub(super) descending: bool,
}

/// State owned by the preferences part of the application.
pub(super) struct PreferencesState {
    pub(super) recorder: super::ui::preferences::shortcuts::Recorder,
    pub(super) theme: Theme,
    pub(super) layout: Layout,
    pub(super) applied: Option<(Theme, Layout)>,
    pub(super) open: bool,
    pub(super) page: usize,
    pub(super) snapshot: Option<(Theme, Layout)>,
    pub(super) draft: Option<Theme>,
    pub(super) error: String,
}

/// State owned by the drive part of the application.
pub(super) struct DriveState {
    pub(super) name: String,
    pub(super) model: String,
    pub(super) space: Option<(u64, u64)>,
    pub(super) device: Option<u64>,
    pub(super) checked_at: Instant,
}

/// State owned by the deletion part of the application.
pub(super) struct DeletionState {
    pub(super) confirm: bool,
    pub(super) targets: Vec<deletion::Target>,
    pub(super) fresh: bool,
    pub(super) rx: Option<Receiver<deletion::Outcome>>,
}
