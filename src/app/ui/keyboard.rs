use crate::app::{
    Disco,
    state::{TreeRow, View},
};
use eframe::egui::{self, Key, Modifiers};
use std::path::PathBuf;
#[derive(Clone)]
struct Item {
    path: PathBuf,
    bytes: u64,
    folder: Option<usize>,
}
impl Disco {
    pub(in crate::app) fn keyboard_navigation(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx();
        if self.preferences.open
            || self.deletion.confirm
            || self.purge_targets.is_some()
            || self.editor.pending.is_some()
            || self.new_file.is_some()
            || egui::Popup::is_any_open(ctx)
        {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::N)) {
            self.request_new_file(ctx);
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::S)) {
            self.save_editor(ctx);
            return;
        }
        if self.editor.focused {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                if let Some(id) = ctx.memory(|m| m.focused()) {
                    ctx.memory_mut(|m| m.surrender_focus(id));
                }
            }
            return;
        }
        if self.browser.text_input_focused {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                self.browser.filter.clear();
                self.reveal_browser_selection();
                ctx.memory_mut(|m| m.surrender_focus(egui::Id::unique("folder-search")));
                ctx.request_repaint();
            }
            return;
        }
        if self.scan.busy
            || self.deletion.rx.is_some()
            || self.recycle.busy
            || self.editor.saving.is_some()
        {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            if !self.browser.filter.is_empty() {
                self.browser.filter.clear();
                self.reveal_browser_selection();
            }
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Backspace)) {
            self.keyboard_back();
            ctx.request_repaint();
            return;
        }
        if self.browser.view == View::Trash {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
            self.start(false, true);
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)) {
            if let Some(p) = &self.browser.properties {
                self.request_delete(vec![(p.path.clone(), p.bytes)]);
                ctx.request_repaint();
            }
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
            if let Some(p) = self.browser.properties.clone() {
                if p.is_dir {
                    if let Some(id) = self
                        .scan
                        .report
                        .as_ref()
                        .and_then(|r| r.folders.iter().position(|f| f.path == p.path))
                    {
                        self.enter_folder(id);
                    }
                } else {
                    self.open_default_file(p.path);
                }
                ctx.request_repaint();
            }
            return;
        }
        if self.browser.view == View::Folders {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowRight)) {
                if self.browser.properties.as_ref().is_some_and(|p| p.is_dir) {
                    let id = self.browser.focus;
                    if !self.browser.expanded.contains(&id) {
                        self.expand_folder(id);
                    } else {
                        self.move_keyboard_selection(1, false);
                    }
                    ctx.request_repaint();
                }
                return;
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowLeft)) {
                if self.browser.properties.as_ref().is_some_and(|p| p.is_dir)
                    && self.browser.expanded.contains(&self.browser.focus)
                {
                    self.toggle_folder(self.browser.focus);
                } else {
                    self.select_parent();
                }
                ctx.request_repaint();
                return;
            }
        }
        for (key, delta, edge) in [
            (Key::ArrowUp, -1, false),
            (Key::ArrowDown, 1, false),
            (Key::PageUp, -10, false),
            (Key::PageDown, 10, false),
            (Key::Home, -1, true),
            (Key::End, 1, true),
        ] {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key)) {
                self.move_keyboard_selection(delta, edge);
                ctx.request_repaint();
                break;
            }
        }
    }
    fn keyboard_items(&mut self) -> Vec<Item> {
        match self.browser.view {
            View::Folders => {
                let rows = self.visible_rows();
                let Some(r) = &self.scan.report else {
                    return vec![];
                };
                rows.iter()
                    .filter_map(|row| match row {
                        TreeRow::Folder(id, _) => {
                            let f = &r.folders[*id];
                            Some(Item {
                                path: f.path.clone(),
                                bytes: f.bytes,
                                folder: Some(*id),
                            })
                        }
                        TreeRow::File(f, _) => Some(Item {
                            path: f.path.clone(),
                            bytes: f.bytes,
                            folder: None,
                        }),
                        _ => None,
                    })
                    .collect()
            }
            View::Files => self
                .scan
                .report
                .as_ref()
                .map(|r| {
                    r.largest
                        .iter()
                        .map(|f| Item {
                            path: f.path.clone(),
                            bytes: f.bytes,
                            folder: None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            View::Duplicates => self
                .scan
                .dups
                .iter()
                .flatten()
                .map(|f| Item {
                    path: f.path.clone(),
                    bytes: f.bytes,
                    folder: None,
                })
                .collect(),
            View::Cleanup => self
                .candidates()
                .into_iter()
                .map(|f| {
                    let id = self
                        .scan
                        .report
                        .as_ref()
                        .and_then(|r| r.folders.iter().position(|i| i.path == f.path));
                    Item {
                        path: f.path,
                        bytes: f.bytes,
                        folder: id,
                    }
                })
                .collect(),
            _ => vec![],
        }
    }
    fn move_keyboard_selection(&mut self, delta: isize, edge: bool) {
        let items = self.keyboard_items();
        if items.is_empty() {
            return;
        }
        let current = self
            .browser
            .properties
            .as_ref()
            .and_then(|p| items.iter().position(|i| i.path == p.path));
        let next = if edge {
            if delta < 0 { 0 } else { items.len() - 1 }
        } else {
            match current {
                Some(i) => (i as isize + delta).clamp(0, items.len() as isize - 1) as usize,
                None => {
                    if delta < 0 {
                        items.len() - 1
                    } else {
                        0
                    }
                }
            }
        };
        let item = &items[next];
        if let Some(id) = item.folder {
            self.select_folder(id);
        } else {
            self.select_file(item.path.clone(), item.bytes);
        }
        self.browser.scroll_to = Some(item.path.clone());
    }
    pub(in crate::app) fn enter_folder(&mut self, id: usize) {
        self.browser.table_root = Some(id);
        self.browser.view = View::Folders;
        self.expand_folder(id);
        self.browser.filter.clear();
        self.select_folder(id);
        self.browser.scroll_to = self.browser.properties.as_ref().map(|p| p.path.clone());
    }
    /// Clearing a query must reveal the selected result in the same scope,
    /// including ancestors that were collapsed before the search.
    pub(in crate::app) fn reveal_browser_selection(&mut self) {
        let Some(p) = &self.browser.properties else {
            return;
        };
        let path = p.path.clone();
        let folder = if p.is_dir {
            Some(path.as_path())
        } else {
            path.parent()
        };
        let mut current = folder.and_then(|path| {
            self.scan
                .report
                .as_ref()
                .and_then(|r| r.folders.iter().position(|f| f.path == path))
        });
        while let Some(id) = current {
            current = self.scan.report.as_ref().unwrap().folders[id].parent;
            self.expand_folder(id);
        }
        self.browser.scroll_to = Some(path);
    }
    fn select_parent(&mut self) {
        let parent = self
            .browser
            .properties
            .as_ref()
            .and_then(|p| p.path.parent())
            .map(PathBuf::from);
        if let Some(id) = parent.and_then(|path| {
            self.scan
                .report
                .as_ref()
                .and_then(|r| r.folders.iter().position(|f| f.path == path))
        }) {
            if self.browser.table_root.is_some_and(|root| {
                self.scan.report.as_ref().unwrap().folders[root].path
                    == self.browser.properties.as_ref().unwrap().path
            }) {
                self.browser.table_root = Some(id);
            }
            self.select_folder(id);
            self.browser.scroll_to = self.browser.properties.as_ref().map(|p| p.path.clone());
        }
    }
    fn keyboard_back(&mut self) {
        if self.browser.history.can_back() {
            self.history_step(false);
            return;
        }
        if let Some(root) = self.browser.table_root {
            if let Some(parent) = self
                .scan
                .report
                .as_ref()
                .and_then(|r| r.folders[root].parent)
            {
                self.browser.table_root = Some(parent);
                self.select_folder(parent);
                self.expand_folder(parent);
                return;
            }
        }
        self.select_parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{editor::Action, state::View};
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        process::Command,
        time::{Duration, Instant},
    };
    fn event(key: Key, modifiers: Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }
    fn frame(
        app: &mut Disco,
        ctx: &egui::Context,
        time: &mut f64,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        *time += 0.05;
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1360., 800.),
                )),
                time: Some(*time),
                events,
                ..Default::default()
            },
            |ui| {
                eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest());
            },
        )
    }
    fn press(app: &mut Disco, ctx: &egui::Context, time: &mut f64, key: Key, modifiers: Modifiers) {
        frame(app, ctx, time, vec![event(key, modifiers)]).drop_without_applying_deltas();
        frame(
            app,
            ctx,
            time,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers,
            }],
        )
        .drop_without_applying_deltas();
    }
    fn settle(
        app: &mut Disco,
        ctx: &egui::Context,
        time: &mut f64,
        ready: impl Fn(&Disco) -> bool,
    ) {
        let start = Instant::now();
        while !ready(app) && start.elapsed() < Duration::from_secs(4) {
            frame(app, ctx, time, vec![]).drop_without_applying_deltas();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            ready(app),
            "path={:?}, document={}, new_file={:?}, pending={}, focus={:?}",
            app.editor.path,
            app.editor.document.is_some(),
            app.new_file,
            app.editor.pending.is_some(),
            ctx.memory(|m| m.focused())
        );
    }
    #[test]
    fn keyboard_editor_and_unsaved_transitions_work_without_touching_real_files() {
        let Some(root) = std::env::var_os("DISCO_EDITOR_UI_FIXTURE").map(PathBuf::from) else {
            let root = std::env::temp_dir().join(format!("disco-editor-ui-{}", std::process::id()));
            for dir in ["config", "bin", "data/applications", "files/child"] {
                fs::create_dir_all(root.join(dir)).unwrap();
            }
            fs::write(root.join("config/layout.toml"),"save_scan=false\nauto_refresh=false\npreview_mode='workspace'\nopen_files_on_double_click=false\n").unwrap();
            fs::write(root.join("files/a.txt"), "original\n").unwrap();
            fs::write(root.join("files/b.txt"), "second\n").unwrap();
            fs::write(
                root.join("data/applications/viewer.desktop"),
                "[Desktop Entry]\nType=Application\nExec=false\n",
            )
            .unwrap();
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
            let out=Command::new(std::env::current_exe().unwrap()).args(["--exact","app::ui::keyboard::tests::keyboard_editor_and_unsaved_transitions_work_without_touching_real_files","--nocapture"])
                .env("DISCO_EDITOR_UI_FIXTURE",&root).env("DISCO_CONFIG_DIR",root.join("config"))
                .env("XDG_DATA_HOME",root.join("data")).env("XDG_DATA_DIRS",root.join("absent"))
                .env("DISCO_OPEN_LOG",root.join("opened.log")).env("PATH",format!("{}:/usr/bin:/bin",root.join("bin").display())).output().unwrap();
            fs::remove_dir_all(root).unwrap();
            assert!(
                out.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            return;
        };
        let ctx = egui::Context::default();
        let mut app = Disco::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let mut time = 0.;
        let folder = root.join("files");
        let a = folder.join("a.txt");
        let b = folder.join("b.txt");
        let report = crate::scan::scan(&folder, &crate::scan::Control::default(), |_| {}).unwrap();
        app.scan.index = crate::index::Index::new(&report);
        app.scan.report = Some(report);
        app.browser.root = folder.to_string_lossy().into();
        app.browser.category = crate::index::Category::All;
        app.browser.expanded = std::collections::HashSet::from([0]);
        app.select_file(a.clone(), 4096);
        settle(&mut app, &ctx, &mut time, |a| a.editor.document.is_some());
        // Real text events edit the draft, never the file until Ctrl+S.
        let editor_id = egui::Id::unique(("file-editor", Some(a.clone())));
        ctx.memory_mut(|m| m.request_focus(editor_id));
        frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::Paste("edited".into())],
        )
        .drop_without_applying_deltas();
        assert!(app.editor.focused && app.editor.dirty());
        assert_eq!(fs::read_to_string(&a).unwrap(), "original\n");
        press(&mut app, &ctx, &mut time, Key::Delete, Modifiers::NONE);
        assert!(!app.deletion.confirm);
        assert!(app.editor.pending.is_none());
        press(&mut app, &ctx, &mut time, Key::ArrowDown, Modifiers::NONE);
        assert_eq!(app.browser.properties.as_ref().unwrap().path, a);
        press(&mut app, &ctx, &mut time, Key::Enter, Modifiers::NONE);
        assert!(!root.join("opened.log").exists());
        press(&mut app, &ctx, &mut time, Key::S, Modifiers::CTRL);
        settle(&mut app, &ctx, &mut time, |a| {
            a.editor.saving.is_none() && !a.editor.dirty()
        });
        assert_eq!(
            fs::read(&a).unwrap(),
            app.editor.document.as_ref().unwrap().encoded()
        );
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(editor_id),
            "Ctrl+S must preserve editor focus"
        );
        frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::Text("more".into())],
        )
        .drop_without_applying_deltas();
        assert!(
            app.editor.focused && app.editor.dirty(),
            "typing continues after saving"
        );
        press(&mut app, &ctx, &mut time, Key::S, Modifiers::CTRL);
        settle(&mut app, &ctx, &mut time, |a| {
            a.editor.saving.is_none() && !a.editor.dirty()
        });
        press(&mut app, &ctx, &mut time, Key::Escape, Modifiers::NONE);
        settle(&mut app, &ctx, &mut time, |a| {
            a.files.get(&folder).is_some()
        });
        press(&mut app, &ctx, &mut time, Key::Home, Modifiers::NONE);
        assert!(app.browser.properties.as_ref().unwrap().is_dir);
        press(&mut app, &ctx, &mut time, Key::ArrowDown, Modifiers::NONE);
        assert!(
            app.browser
                .properties
                .as_ref()
                .unwrap()
                .path
                .ends_with("child")
        );
        press(&mut app, &ctx, &mut time, Key::Enter, Modifiers::NONE);
        assert!(app.browser.table_root.is_some());
        press(&mut app, &ctx, &mut time, Key::Backspace, Modifiers::NONE);
        assert!(app.browser.table_root.is_none());
        // Enter is an explicit open even when double-click opening is disabled.
        app.select_file(a.clone(), 4096);
        frame(&mut app, &ctx, &mut time, vec![]).drop_without_applying_deltas();
        press(&mut app, &ctx, &mut time, Key::Enter, Modifiers::NONE);
        let start = Instant::now();
        while app.opening_file.load(std::sync::atomic::Ordering::Relaxed)
            && start.elapsed() < Duration::from_secs(3)
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(root.join("opened.log").exists());
        settle(&mut app, &ctx, &mut time, |a| a.editor.document.is_some());
        ctx.memory_mut(|m| m.request_focus(egui::Id::unique("folder-search")));
        app.browser.filter = "abc".into();
        press(&mut app, &ctx, &mut time, Key::Backspace, Modifiers::NONE);
        assert_eq!(app.browser.filter, "ab");
        assert!(app.browser.view == View::Folders);
        ctx.memory_mut(|m| m.surrender_focus(egui::Id::unique("folder-search")));
        app.browser.filter.clear();
        app.editor.document.as_mut().unwrap().text.push_str("draft");
        app.start(false, true);
        assert!(!app.scan.busy);
        assert!(matches!(app.editor.pending, Some(Action::Scan { .. })));
        press(&mut app, &ctx, &mut time, Key::Escape, Modifiers::NONE);
        assert!(app.editor.pending.is_none());
        app.select_file(b.clone(), 4096);
        frame(&mut app, &ctx, &mut time, vec![]).drop_without_applying_deltas();
        assert!(app.editor.pending.is_some());
        assert_eq!(app.browser.properties.as_ref().unwrap().path, a);
        press(&mut app, &ctx, &mut time, Key::Escape, Modifiers::NONE);
        assert!(app.editor.pending.is_none() && app.editor.dirty());
        // Save and continue applies the queued selection only after successful IO.
        app.select_file(b.clone(), 4096);
        frame(&mut app, &ctx, &mut time, vec![]).drop_without_applying_deltas();
        app.save_editor(&ctx);
        settle(&mut app, &ctx, &mut time, |a| {
            a.editor.pending.is_none()
                && a.browser.properties.as_ref().is_some_and(|p| p.path == b)
                && a.editor.document.is_some()
        });
        assert!(fs::read_to_string(&a).unwrap().ends_with("draft"));
        // A failed save retains the draft and the pending transition.
        app.editor.document.as_mut().unwrap().text.push_str("local");
        fs::write(&b, "external").unwrap();
        app.request_close(&ctx);
        app.save_editor(&ctx);
        settle(&mut app, &ctx, &mut time, |a| a.editor.saving.is_none());
        assert!(app.editor.dirty() && !app.editor.error.is_empty());
        assert!(matches!(app.editor.pending, Some(Action::Close)));
        assert_eq!(fs::read_to_string(&b).unwrap(), "external");
        app.editor.pending = None;
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1360., 800.),
            )),
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let output = ctx.run_ui(input, |ui| {
            eframe::App::ui(&mut app, ui, &mut eframe::Frame::_new_kittest())
        });
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::CancelClose)
        );
        output.drop_without_applying_deltas();
        assert!(matches!(app.editor.pending, Some(Action::Close)));
        app.editor.pending = None;
        app.editor.document = None;
        app.editor.focused = false;
        app.browser.view = View::Folders;
        press(&mut app, &ctx, &mut time, Key::Delete, Modifiers::NONE);
        assert!(app.deletion.confirm);
        assert!(b.exists());
        // Create from the real Ctrl+N modal, paste a custom extension and
        // immediately type into the newly focused empty editor.
        app.deletion.confirm = false;
        app.enter_folder(0);
        press(&mut app, &ctx, &mut time, Key::N, Modifiers::CTRL);
        assert!(app.new_file.is_some());
        frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::Paste("../outside.txt".into())],
        )
        .drop_without_applying_deltas();
        press(&mut app, &ctx, &mut time, Key::Enter, Modifiers::NONE);
        settle(&mut app, &ctx, &mut time, |a| {
            a.new_file.as_ref().is_some_and(|draft| draft.has_error())
        });
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(egui::Id::unique("new-file-name")),
            "invalid input keeps the dialog and input focused"
        );
        assert!(!root.join("outside.txt").exists());
        press(
            &mut app,
            &ctx,
            &mut time,
            Key::A,
            Modifiers {
                ctrl: true,
                command: true,
                ..Modifiers::NONE
            },
        );
        frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::Paste("/new_folder/sub/fresh.custom".into())],
        )
        .drop_without_applying_deltas();
        press(&mut app, &ctx, &mut time, Key::Enter, Modifiers::NONE);
        let fresh = folder.join("new_folder/sub/fresh.custom");
        settle(&mut app, &ctx, &mut time, |a| {
            a.new_file.is_none()
                && a.editor.path.as_ref() == Some(&fresh)
                && a.editor.document.is_some()
        });
        assert!(fresh.is_file());
        assert_eq!(
            app.browser.root,
            folder.to_string_lossy(),
            "creating folders preserves the original scan root"
        );
        let parent_id = app.browser.table_root.unwrap();
        assert_eq!(
            app.scan.report.as_ref().unwrap().folders[parent_id].path,
            fresh.parent().unwrap()
        );
        assert!(
            app.scan
                .index
                .children(0)
                .iter()
                .any(|id| app.scan.report.as_ref().unwrap().folders[*id].path
                    == folder.join("new_folder"))
        );
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(egui::Id::unique(("file-editor", Some(fresh.clone()))))
        );
        frame(
            &mut app,
            &ctx,
            &mut time,
            vec![egui::Event::Paste("new text".into())],
        )
        .drop_without_applying_deltas();
        assert!(app.editor.dirty());
        assert_eq!(fs::read(&fresh).unwrap(), b"");
        press(&mut app, &ctx, &mut time, Key::S, Modifiers::CTRL);
        settle(&mut app, &ctx, &mut time, |a| {
            a.editor.saving.is_none() && !a.editor.dirty()
        });
        assert_eq!(fs::read_to_string(&fresh).unwrap(), "new text");
    }
}
