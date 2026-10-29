//! One lazily opened PTY session. Hiding preserves it; closing destroys its tree.
use crate::settings::Theme;
use eframe::egui;
use egui_term::{
    BackendCommand, BackendSettings, ColorPalette, PtyEvent, TerminalBackend, TerminalTheme,
};
use std::{
    collections::{HashMap, HashSet},
    io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
};

#[derive(Clone, Copy)]
struct Process {
    pid: u32,
    parent: u32,
    session: u32,
    start: u64,
}
fn process(pid: u32) -> Option<Process> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<_> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    Some(Process {
        pid,
        parent: fields.get(1)?.parse().ok()?,
        session: fields.get(3)?.parse().ok()?,
        start: fields.get(19)?.parse().ok()?,
    })
}
fn terminate_session(leader: Process) {
    if leader.pid <= 1 || leader.pid == std::process::id() {
        return;
    }
    if process(leader.pid).is_some_and(|now| now.start != leader.start || now.session != leader.pid)
    {
        return;
    }
    let mut all = HashMap::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            if let Some(p) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse().ok())
                .and_then(process)
            {
                all.insert(p.pid, p);
            }
        }
    }
    let mut ids = HashSet::from([leader.pid]);
    loop {
        let next: HashSet<_> = all
            .values()
            .filter(|p| p.session == leader.pid || ids.contains(&p.parent))
            .map(|p| p.pid)
            .collect();
        if next.is_subset(&ids) {
            break;
        }
        ids.extend(next);
    }
    // Freeze only this PTY's captured processes before killing them, so a job
    // cannot keep forking during teardown. Recheck creation time against reuse.
    for signal in [libc::SIGSTOP, libc::SIGKILL] {
        for p in all.values().filter(|p| ids.contains(&p.pid)) {
            if p.pid != std::process::id() && process(p.pid).is_some_and(|now| now.start == p.start)
            {
                unsafe {
                    libc::kill(p.pid as i32, signal);
                }
            }
        }
    }
}
struct Session {
    backend: Option<TerminalBackend>,
    leader: Process,
}
impl Drop for Session {
    fn drop(&mut self) {
        if let Some(backend) = self.backend.take() {
            let leader = self.leader;
            std::thread::spawn(move || {
                terminate_session(leader);
                drop(backend);
            });
        }
    }
}
#[derive(Default)]
pub(crate) struct Terminal {
    session: Option<Session>,
    rx: Option<Receiver<(u64, PtyEvent)>>,
    pub visible: bool,
    pub exited: bool,
    pub error: String,
    pub title: String,
    pub cwd: PathBuf,
    pub fonts_loaded: bool,
    palette: Option<(Theme, TerminalTheme)>,
}
impl Terminal {
    pub fn open(&mut self, ctx: &egui::Context, cwd: &Path) -> io::Result<()> {
        if self.session.is_some() {
            self.visible = true;
            return Ok(());
        }
        let (tx, rx) = mpsc::channel();
        let shell = system_shell();
        let backend = TerminalBackend::new(
            1,
            ctx.clone(),
            tx,
            BackendSettings {
                shell: shell.clone(),
                working_directory: Some(cwd.into()),
                ..Default::default()
            },
        )?;
        let leader = process(backend.pty_id())
            .ok_or_else(|| io::Error::other("Shell exited during startup"))?;
        self.session = Some(Session {
            backend: Some(backend),
            leader,
        });
        self.rx = Some(rx);
        self.cwd = cwd.into();
        self.title = Path::new(&shell)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into();
        self.visible = true;
        self.exited = false;
        self.error.clear();
        Ok(())
    }
    pub fn backend(&mut self) -> Option<&mut TerminalBackend> {
        self.session.as_mut()?.backend.as_mut()
    }
    pub fn exists(&self) -> bool {
        self.session.is_some()
    }
    pub fn hide(&mut self) {
        self.visible = false;
        if let Some(b) = self.backend() {
            b.set_repaint_enabled(false);
        }
    }
    pub fn close(&mut self) {
        self.visible = false;
        self.rx = None;
        self.session = None;
        self.exited = false;
        self.error.clear();
        self.title.clear();
    }
    pub fn shutdown(&mut self) {
        if let Some(mut session) = self.session.take() {
            terminate_session(session.leader);
            drop(session.backend.take());
        }
        self.visible = false;
        self.rx = None;
    }
    pub fn theme(&mut self, t: &Theme) -> TerminalTheme {
        if self.palette.as_ref().is_none_or(|(old, _)| old != t) {
            let palette = ColorPalette {
                background: t.background.clone(),
                foreground: t.text.clone(),
                red: t.accent.clone(),
                green: "#a7c894".into(),
                yellow: t.trash.clone(),
                blue: "#87b7cf".into(),
                magenta: "#cbb49a".into(),
                bright_magenta: "#e0c7aa".into(),
                dim_magenta: "#887564".into(),
                ..Default::default()
            };
            self.palette = Some((t.clone(), TerminalTheme::new(Box::new(palette))));
        }
        self.palette.as_ref().unwrap().1.clone()
    }
    pub fn poll(&mut self, t: &Theme, shown: bool) {
        if !self.exists() {
            return;
        }
        let theme = self.theme(t);
        if let Some(b) = self.backend() {
            b.set_repaint_enabled(shown);
        }
        while let Some((_, event)) = self.rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            match event {
                PtyEvent::Exit | PtyEvent::ChildExit(_) => self.exited = true,
                PtyEvent::Title(title) => self.title = title,
                PtyEvent::ColorRequest(index, format) => {
                    if let Some(b) = self.backend() {
                        b.process_command(BackendCommand::Write(
                            format(theme.query_color(index)).into_bytes(),
                        ));
                    }
                }
                PtyEvent::TextAreaSizeRequest(format) => {
                    if let Some(b) = self.backend() {
                        b.process_command(BackendCommand::Write(
                            format(b.last_content().terminal_size.into()).into_bytes(),
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    #[cfg(test)]
    pub fn pid(&self) -> Option<u32> {
        self.session.as_ref().map(|s| s.leader.pid)
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn system_shell() -> String {
    if let Ok(shell) = std::env::var("SHELL") {
        if Path::new(&shell).is_file() {
            return shell;
        }
    }
    unsafe {
        let mut pwd: libc::passwd = std::mem::zeroed();
        let mut buffer = vec![0; 16384];
        let mut out = std::ptr::null_mut();
        if libc::getpwuid_r(
            libc::geteuid(),
            &mut pwd,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut out,
        ) == 0
            && !out.is_null()
            && !pwd.pw_shell.is_null()
        {
            let shell = std::ffi::CStr::from_ptr(pwd.pw_shell)
                .to_string_lossy()
                .into_owned();
            if Path::new(&shell).is_file() {
                return shell;
            }
        }
    }
    "/bin/sh".into()
}
