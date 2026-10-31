//! Explicit, cancellable server management. Never removes an external installation.
use super::{
    catalog::{self, Catalog, Server},
    client::Launch,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Installed {
    current: String,
    previous: String,
    source: String,
    bytes: u64,
    digest: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Database {
    servers: BTreeMap<String, Installed>,
}
#[derive(Clone, Copy, Debug)]
pub enum Action {
    Probe,
    Check,
    Install,
    Rollback,
    Remove,
}
struct ResultData {
    id: String,
    message: String,
    database: Database,
    update: Option<Server>,
    restart: bool,
}
struct Job {
    cancel: Arc<AtomicBool>,
    rx: Receiver<Result<ResultData, String>>,
    thread: Option<thread::JoinHandle<()>>,
}
pub struct Manager {
    pub catalog: Catalog,
    pub draft: Option<Catalog>,
    pub error: String,
    pub operation: String,
    pub messages: BTreeMap<String, String>,
    pub updates: BTreeMap<String, Server>,
    config: PathBuf,
    root: PathBuf,
    database: Database,
    job: Option<Job>,
    pub revision: u64,
}
fn data_root() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
        })
        .join("disco/lsp")
}
fn read_db(root: &Path) -> Database {
    fs::read_to_string(root.join("state.toml"))
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}
impl Manager {
    pub fn load(config: PathBuf) -> Self {
        Self::load_at(config, data_root())
    }
    fn load_at(config: PathBuf, root: PathBuf) -> Self {
        let config = config.join("lsp.toml");
        let mut error = String::new();
        let catalog = if config.exists() {
            match fs::read(&config).map_err(|e| e.to_string()).and_then(|b| {
                if b.len() > 1024 * 1024 {
                    return Err("LSP catalog exceeds 1 MiB".into());
                }
                let c: Catalog =
                    toml::from_str(std::str::from_utf8(&b).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                c.validate()?;
                Ok(c)
            }) {
                Ok(c) => c,
                Err(e) => {
                    error = e;
                    Catalog::builtin()
                }
            }
        } else {
            let c = Catalog::builtin();
            if let Err(e) = atomic(&config, toml::to_string_pretty(&c).unwrap().as_bytes()) {
                error = e;
            }
            c
        };
        let database = read_db(&root);
        Self {
            catalog,
            draft: None,
            error,
            operation: String::new(),
            messages: BTreeMap::new(),
            updates: BTreeMap::new(),
            config,
            root,
            database,
            job: None,
            revision: 0,
        }
    }
    pub fn begin_edit(&mut self) {
        if self.draft.is_none() {
            self.draft = Some(self.catalog.clone());
        }
    }
    pub fn cancel_edit(&mut self) {
        self.draft = None;
    }
    pub fn save(&mut self) -> Result<(), String> {
        let c = self.draft.as_ref().unwrap_or(&self.catalog);
        c.validate()?;
        atomic(
            &self.config,
            toml::to_string_pretty(c)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )?;
        self.catalog = c.clone();
        self.draft = None;
        self.error.clear();
        self.revision += 1;
        Ok(())
    }
    pub fn path(&self) -> &Path {
        &self.config
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn cancel_job(&self) {
        if let Some(j) = &self.job {
            j.cancel.store(true, Ordering::Relaxed);
        }
    }
    pub fn shutdown(&mut self) {
        self.cancel_job();
        if let Some(mut j) = self.job.take() {
            if let Some(t) = j.thread.take() {
                let _ = t.join();
            }
        }
    }
    pub fn launches(&self) -> Vec<Launch> {
        self.catalog
            .servers
            .iter()
            .filter(|(_, s)| s.enabled)
            .map(|(id, s)| {
                let mut args = s.args.clone();
                let command = if s.provider == "rustup" {
                    args.splice(
                        0..0,
                        [
                            "run".into(),
                            if s.version.is_empty() {
                                "stable".into()
                            } else {
                                s.version.clone()
                            },
                            s.command.clone(),
                        ],
                    );
                    PathBuf::from("rustup")
                } else if let Some(i) = self
                    .database
                    .servers
                    .get(id)
                    .filter(|i| catalog::segment(&i.current))
                {
                    binary(&self.root, id, &i.current, s)
                } else {
                    PathBuf::from(&s.command)
                };
                Launch {
                    id: id.clone(),
                    command,
                    args,
                    extensions: s.extensions.clone(),
                    languages: s.languages.clone(),
                    markers: s.root_markers.clone(),
                }
            })
            .collect()
    }
    pub fn owned_installation(&self, id: &str) -> bool {
        self.database.servers.contains_key(id)
    }
    pub fn can_rollback(&self, id: &str) -> bool {
        self.database
            .servers
            .get(id)
            .is_some_and(|i| catalog::segment(&i.previous))
    }
    pub fn can_install(&self, id: &str) -> bool {
        let target = self
            .updates
            .get(id)
            .or_else(|| self.catalog.servers.get(id));
        target.is_some_and(|s| {
            self.database
                .servers
                .get(id)
                .is_none_or(|i| i.current != s.version && i.previous != s.version)
        })
    }
    pub fn installed(&self, id: &str) -> String {
        self.database
            .servers
            .get(id)
            .map(|i| {
                format!(
                    "{} · {:.1} MiB · {}",
                    i.current,
                    i.bytes as f64 / 1048576.,
                    i.source
                )
            })
            .unwrap_or_else(|| {
                crate::tr!(
                    "Зовнішня установка або не встановлено",
                    "External installation or not installed"
                )
                .into()
            })
    }
    pub fn poll(&mut self) -> Option<(String, bool)> {
        let result = match self.job.as_ref()?.rx.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err(
                "Language server operation stopped unexpectedly; previous installation preserved"
                    .into(),
            ),
        };
        if let Some(mut j) = self.job.take() {
            if let Some(t) = j.thread.take() {
                let _ = t.join();
            }
        }
        match result {
            Ok(r) => {
                self.database = r.database;
                self.messages.insert(r.id.clone(), r.message);
                if let Some(s) = r.update {
                    self.updates.insert(r.id.clone(), s);
                }
                self.revision += 1;
                Some((r.id, r.restart))
            }
            Err(e) => {
                self.error = e;
                None
            }
        }
    }
    pub fn start(
        &mut self,
        id: &str,
        action: Action,
        ack: Option<Receiver<()>>,
        ctx: &eframe::egui::Context,
    ) {
        if self.busy() {
            return;
        }
        let Some(server) = self.catalog.servers.get(id).cloned() else {
            return;
        };
        self.operation = format!(
            "{} · {}",
            server.name,
            match action {
                Action::Probe => crate::tr!("Перевірка установки…", "Checking installation…"),
                Action::Check => crate::tr!("Перевірка оновлень…", "Checking updates…"),
                Action::Install => crate::tr!("Встановлення…", "Installing…"),
                Action::Rollback => crate::tr!("Відкат…", "Rolling back…"),
                Action::Remove => crate::tr!("Видалення…", "Removing…"),
            }
        );
        let id = id.to_owned();
        let root = self.root.clone();
        let update = self.updates.get(&id).cloned();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        self.error.clear();
        let t = thread::spawn(move || {
            crate::scan::background_priority();
            let r = (|| {
                if let Some(ack) = ack {
                    ack.recv_timeout(Duration::from_secs(5))
                        .map_err(|_| "Server did not stop; files were preserved")?;
                }
                execute(&root, &id, &server, update, action, &stop)
            })();
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.job = Some(Job {
            cancel,
            rx,
            thread: Some(t),
        });
    }
}
impl Drop for Manager {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn binary(root: &Path, id: &str, version: &str, s: &Server) -> PathBuf {
    let d = root.join(id).join(version);
    if s.provider == "npm" {
        d.join("node_modules/.bin").join(&s.command)
    } else {
        d.join(&s.command)
    }
}
fn cancelled(c: &AtomicBool) -> Result<(), String> {
    if c.load(Ordering::Relaxed) {
        Err("Cancelled; previous installation preserved".into())
    } else {
        Ok(())
    }
}
fn tool(name: &str) -> Result<PathBuf, String> {
    catalog::executable(Path::new(name))
        .ok_or_else(|| format!("Required runtime not found: {name}"))
}
fn run(command: &mut Command, dir: &Path, c: &AtomicBool) -> Result<String, String> {
    cancelled(c)?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let log = dir.join("command.log");
    let output = fs::File::create(&log).map_err(|e| e.to_string())?;
    let mut child = command
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(output.try_clone().map_err(|e| e.to_string())?)
        .stderr(output)
        .process_group(0)
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        if c.load(Ordering::Relaxed) || start.elapsed() > Duration::from_secs(300) {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            cancelled(c)?;
            return Err("Server operation timed out; previous installation preserved".into());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let mut f = fs::File::open(&log).map_err(|e| e.to_string())?;
            use std::io::{Seek, SeekFrom};
            let len = f.metadata().map_err(|e| e.to_string())?.len();
            let _ = f.seek(SeekFrom::Start(len.saturating_sub(16384)));
            let mut text = String::new();
            let _ = f.read_to_string(&mut text);
            if status.success() {
                return Ok(text);
            }
            return Err(format!("{status}: {text}"));
        }
        thread::sleep(Duration::from_millis(30));
    }
}
fn fetch(url: &str, dir: &Path, c: &AtomicBool) -> Result<Vec<u8>, String> {
    let out = dir.join("download");
    run(
        Command::new(tool("curl")?)
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--max-time",
                "120",
                "--max-filesize",
                "67108864",
                "--user-agent",
                "disco-lsp",
            ])
            .arg("--output")
            .arg(&out)
            .arg(url),
        dir,
        c,
    )?;
    fs::read(out).map_err(|e| e.to_string())
}
fn digest(path: &Path, dir: &Path, c: &AtomicBool) -> Result<String, String> {
    let v = run(Command::new(tool("sha256sum")?).arg(path), dir, c)?;
    Ok(v.split_whitespace().next().unwrap_or_default().to_owned())
}
fn size(path: &Path) -> u64 {
    let Ok(m) = fs::symlink_metadata(path) else {
        return 0;
    };
    if m.is_symlink() {
        0
    } else if m.is_file() {
        m.len()
    } else {
        fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| size(&e.path()))
            .sum()
    }
}
fn owned(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !m.is_symlink())
        && fs::read_to_string(path.join(".disco-owned")).is_ok_and(|s| s == "disco-lsp-v1")
}
struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn lock(root: &Path) -> Result<Lock, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let p = root.join(".lock");
    if let Ok(pid) = fs::read_to_string(&p) {
        if pid
            .trim()
            .parse::<u32>()
            .is_ok_and(|n| !Path::new(&format!("/proc/{n}")).exists())
        {
            let _ = fs::remove_file(&p);
        }
    }
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&p)
        .map_err(|_| "Another disco instance is managing language servers")?;
    write!(f, "{}", std::process::id()).map_err(|e| e.to_string())?;
    Ok(Lock(p))
}
fn execute(
    root: &Path,
    id: &str,
    s: &Server,
    update: Option<Server>,
    action: Action,
    c: &AtomicBool,
) -> Result<ResultData, String> {
    if !catalog::segment(id) {
        return Err("Invalid server ID".into());
    }
    let _lock = lock(root)?;
    let mut db = read_db(root);
    let work = root.join(format!(".stage-{}", std::process::id()));
    fs::create_dir(&work).map_err(|e| e.to_string())?;
    let _cleanup = Cleanup(work.clone());
    let mut result = ResultData {
        id: id.into(),
        message: String::new(),
        database: db.clone(),
        update: None,
        restart: matches!(action, Action::Install | Action::Rollback),
    };
    match action {
        Action::Probe => {
            let executable = if s.provider == "rustup" {
                tool("rustup")?
            } else if let Some(i) = db.servers.get(id) {
                binary(root, id, &i.current, s)
            } else {
                tool(&s.command)?
            };
            let v = if s.provider == "npm" && db.servers.contains_key(id) {
                tool("node")?;
                if !executable.exists() {
                    return Err("Managed executable is missing".into());
                }
                format!("{} {}", s.package, db.servers[id].current)
            } else if s.provider == "rustup" {
                run(
                    Command::new(tool("rustup")?).args([
                        "run",
                        if s.version.is_empty() {
                            "stable"
                        } else {
                            &s.version
                        },
                        &s.command,
                        "--version",
                    ]),
                    &work,
                    c,
                )?
            } else {
                run(Command::new(&executable).arg("--version"), &work, c)?
            };
            result.message = format!(
                "{} · {} · {:.1} KiB",
                executable.display(),
                v.trim(),
                fs::metadata(&executable)
                    .map(|m| m.len())
                    .unwrap_or_default() as f64
                    / 1024.
            );
            return Ok(result);
        }
        Action::Check => {
            let mut next = s.clone();
            if s.provider == "github_release" {
                let v: Value = serde_json::from_slice(&fetch(
                    &format!(
                        "https://api.github.com/repos/{}/releases/latest",
                        s.repository
                    ),
                    &work,
                    c,
                )?)
                .map_err(|e| e.to_string())?;
                next.version = v["tag_name"].as_str().ok_or("Missing release tag")?.into();
                let asset = v["assets"]
                    .as_array()
                    .and_then(|a| a.iter().find(|a| a["name"] == s.asset))
                    .ok_or("No artifact for this platform")?;
                next.sha256 = asset["digest"]
                    .as_str()
                    .and_then(|s| s.strip_prefix("sha256:"))
                    .ok_or("Release has no verified SHA-256 digest")?
                    .into();
            } else if s.provider == "npm" {
                let v: Value = serde_json::from_slice(&fetch(
                    &format!("https://registry.npmjs.org/{}/latest", s.package),
                    &work,
                    c,
                )?)
                .map_err(|e| e.to_string())?;
                next.version = v["version"]
                    .as_str()
                    .ok_or("Missing package version")?
                    .into();
                next.integrity = v["dist"]["integrity"]
                    .as_str()
                    .ok_or("Missing npm integrity")?
                    .into();
            } else {
                return Err("External server: use its system package manager or rustup".into());
            }
            let check = Catalog {
                version: 1,
                servers: BTreeMap::from([(id.into(), next.clone())]),
            };
            check.validate()?;
            result.message = crate::tr!(
                "Доступна версія: {v}",
                "Available version: {v}",
                v = next.version
            );
            result.update = Some(next);
            return Ok(result);
        }
        Action::Remove => {
            let i = db
                .servers
                .get(id)
                .ok_or("External installation is not owned by disco")?;
            let path = root.join(id);
            if !owned(&path) {
                return Err("Missing ownership marker; nothing removed".into());
            }
            if !catalog::segment(&i.current) {
                return Err("Invalid installation record".into());
            }
            let trash = root.join(format!(".removed-{}", std::process::id()));
            fs::rename(&path, &trash).map_err(|e| e.to_string())?;
            db.servers.remove(id);
            if let Err(e) = atomic(
                &root.join("state.toml"),
                toml::to_string(&db).unwrap().as_bytes(),
            ) {
                let _ = fs::rename(&trash, &path);
                return Err(e);
            }
            let bytes = size(&trash);
            fs::remove_dir_all(trash).map_err(|e| e.to_string())?;
            result.message = crate::tr!(
                "Звільнено {n} MiB",
                "Freed {n} MiB",
                n = format!("{:.1}", bytes as f64 / 1048576.)
            );
        }
        Action::Rollback => {
            let i = db.servers.get_mut(id).ok_or("No managed installation")?;
            if !catalog::segment(&i.previous) || !owned(&root.join(id).join(&i.previous)) {
                return Err("No previous verified version".into());
            }
            let previous = i.current.clone();
            let manifest = root.join(id).join(&i.previous).join("install.toml");
            let mut target: Installed =
                toml::from_str(&fs::read_to_string(manifest).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("Invalid rollback manifest: {e}"))?;
            if target.current != i.previous {
                return Err("Rollback manifest/version mismatch".into());
            }
            target.previous = previous;
            *i = target;
            atomic(
                &root.join("state.toml"),
                toml::to_string(&db).unwrap().as_bytes(),
            )?;
        }
        Action::Install => {
            let s = update.as_ref().unwrap_or(s);
            Catalog {
                version: 1,
                servers: BTreeMap::from([(id.into(), s.clone())]),
            }
            .validate()?;
            if !matches!(s.provider.as_str(), "npm" | "github_release") {
                return Err(
                    "External installation: use the system package manager or rustup".into(),
                );
            }
            let parent = root.join(id);
            if parent.exists() && !owned(&parent) {
                return Err("Directory is not owned by disco".into());
            }
            let dest = parent.join(&s.version);
            if dest.exists() {
                return Err("Version already installed; use rollback or check updates".into());
            }
            let stage = work.join("payload");
            fs::create_dir(&stage).map_err(|e| e.to_string())?;
            let verified = if s.provider == "github_release" {
                if s.platform != format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH) {
                    return Err(format!(
                        "Unsupported platform: {}-{}",
                        std::env::consts::OS,
                        std::env::consts::ARCH
                    ));
                }
                let url = format!(
                    "https://github.com/{}/releases/download/{}/{}",
                    s.repository, s.version, s.asset
                );
                let data = fetch(&url, &work, c)?;
                let archive = work.join("artifact.gz");
                fs::write(&archive, &data).map_err(|e| e.to_string())?;
                if digest(&archive, &work, c)? != s.sha256.to_ascii_lowercase() {
                    return Err("Artifact SHA-256 mismatch; previous version preserved".into());
                }
                let mut decoder = flate2::read::GzDecoder::new(&data[..]);
                let mut out =
                    fs::File::create(stage.join(&s.command)).map_err(|e| e.to_string())?;
                let mut buffer = [0; 65536];
                let mut written = 0;
                loop {
                    cancelled(c)?;
                    let n = decoder.read(&mut buffer).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    written += n;
                    if written > 256 * 1024 * 1024 {
                        return Err("Expanded artifact exceeds 256 MiB".into());
                    }
                    out.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
                }
                fs::set_permissions(stage.join(&s.command), fs::Permissions::from_mode(0o755))
                    .map_err(|e| e.to_string())?;
                s.sha256.clone()
            } else {
                tool("node")?;
                let npm = tool("npm")?;
                let v: Value = serde_json::from_slice(&fetch(
                    &format!("https://registry.npmjs.org/{}/{}", s.package, s.version),
                    &work,
                    c,
                )?)
                .map_err(|e| e.to_string())?;
                if v["dist"]["integrity"] != s.integrity {
                    return Err("Pinned npm integrity does not match registry".into());
                }
                let dep = BTreeMap::from([(s.package.clone(), s.version.clone())]);
                fs::write(
                    stage.join("package.json"),
                    serde_json::to_vec(&serde_json::json!({"private":true,"dependencies":dep}))
                        .unwrap(),
                )
                .map_err(|e| e.to_string())?;
                let mut command = Command::new(npm);
                command
                    .args([
                        "install",
                        "--ignore-scripts",
                        "--no-audit",
                        "--no-fund",
                        "--engine-strict",
                        "--package-lock",
                    ])
                    .arg("--prefix")
                    .arg(&stage)
                    .arg(format!("{}@{}", s.package, s.version))
                    .args(&s.extra_packages);
                run(&mut command, &work, c)?;
                let lock: Value = serde_json::from_slice(
                    &fs::read(stage.join("package-lock.json")).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                if lock["packages"][format!("node_modules/{}", s.package)]["integrity"]
                    != s.integrity
                {
                    return Err("Installed package integrity differs from pinned artifact".into());
                }
                s.integrity.clone()
            };
            if !if s.provider == "npm" {
                stage.join("node_modules/.bin").join(&s.command).exists()
            } else {
                stage.join(&s.command).exists()
            } {
                return Err("Server executable missing from artifact".into());
            }
            cancelled(c)?;
            fs::write(stage.join(".disco-owned"), "disco-lsp-v1").map_err(|e| e.to_string())?;
            fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
            fs::write(parent.join(".disco-owned"), "disco-lsp-v1").map_err(|e| e.to_string())?;
            fs::rename(&stage, &dest).map_err(|e| e.to_string())?;
            let mut uncommitted = Cleanup(dest.clone());
            let old = db.servers.get(id).cloned().unwrap_or_default();
            let entry = Installed {
                current: s.version.clone(),
                previous: old.current.clone(),
                source: if s.provider == "npm" {
                    format!("npm:{}", s.package)
                } else {
                    format!("GitHub:{}", s.repository)
                },
                bytes: size(&dest),
                digest: verified,
            };
            atomic(
                &dest.join("install.toml"),
                toml::to_string(&entry).unwrap().as_bytes(),
            )?;
            db.servers.insert(id.into(), entry);
            if let Err(e) = atomic(
                &root.join("state.toml"),
                toml::to_string(&db).unwrap().as_bytes(),
            ) {
                let _ = fs::remove_dir_all(&dest);
                return Err(e);
            }
            uncommitted.0 = PathBuf::new();
            if catalog::segment(&old.previous)
                && old.previous != old.current
                && old.previous != s.version
            {
                let p = parent.join(old.previous);
                if owned(&p) {
                    let _ = fs::remove_dir_all(p);
                }
            }
        }
    }
    result.database = db;
    if result.message.is_empty() {
        result.message = crate::tr!("Готово", "Done").into();
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_rollback_remove_cancellation_and_external_guard() {
        let root = std::env::temp_dir().join(format!("disco-lsp-manager-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let _cleanup = Cleanup(root.clone());
        let server = Catalog::builtin().servers["rust-analyzer"].clone();
        let id = "rust-analyzer";
        let mut db = Database::default();
        for version in ["one", "two"] {
            let dir = root.join(id).join(version);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(".disco-owned"), "disco-lsp-v1").unwrap();
            let i = Installed {
                current: version.into(),
                previous: if version == "two" { "one" } else { "" }.into(),
                source: "fixture".into(),
                bytes: 42,
                digest: version.into(),
            };
            fs::write(dir.join("install.toml"), toml::to_string(&i).unwrap()).unwrap();
            db.servers.insert(id.into(), i);
        }
        fs::write(root.join(id).join(".disco-owned"), "disco-lsp-v1").unwrap();
        atomic(
            &root.join("state.toml"),
            toml::to_string(&db).unwrap().as_bytes(),
        )
        .unwrap();
        let stop = AtomicBool::new(false);
        let r = execute(&root, id, &server, None, Action::Rollback, &stop).unwrap();
        assert_eq!(r.database.servers[id].current, "one");
        assert_eq!(r.database.servers[id].digest, "one");
        assert_eq!(r.database.servers[id].previous, "two");
        let cancel = AtomicBool::new(true);
        let before = fs::read(root.join("state.toml")).unwrap();
        assert!(execute(&root, id, &server, None, Action::Install, &cancel).is_err());
        assert_eq!(fs::read(root.join("state.toml")).unwrap(), before);
        let external = root.join("external");
        fs::create_dir(&external).unwrap();
        fs::write(external.join("keep"), "external").unwrap();
        assert!(execute(&root, "external", &server, None, Action::Remove, &stop).is_err());
        assert!(external.join("keep").exists());
        execute(&root, id, &server, None, Action::Remove, &stop).unwrap();
        assert!(!root.join(id).exists());
        assert!(read_db(&root).servers.is_empty());
        assert!(execute(&root, "../external", &server, None, Action::Remove, &stop).is_err());
        let l = lock(&root).unwrap();
        assert!(lock(&root).is_err());
        drop(l);
        assert!(lock(&root).is_ok());
        assert!(catalog::executable(Path::new("./missing-local-command")).is_none());
    }
    #[test]
    fn cancelling_a_running_download_command_reaps_it_and_preserves_state() {
        let root = std::env::temp_dir().join(format!("disco-lsp-cancel-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let _cleanup = Cleanup(root.clone());
        let original = b"previous verified installation";
        fs::write(root.join("state.toml"), original).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let t = thread::spawn(move || {
            thread::sleep(Duration::from_millis(80));
            stop.store(true, Ordering::Relaxed);
        });
        let at = Instant::now();
        let result = run(
            Command::new(tool("sleep").unwrap()).arg("60"),
            &root,
            &cancel,
        );
        t.join().unwrap();
        assert!(result.unwrap_err().contains("Cancelled"));
        assert!(at.elapsed() < Duration::from_secs(2));
        assert_eq!(fs::read(root.join("state.toml")).unwrap(), original);
        let mut server = Catalog::builtin().servers["rust-analyzer"].clone();
        server.platform = "unsupported".into();
        assert!(
            execute(
                &root,
                "rust-analyzer",
                &server,
                None,
                Action::Install,
                &AtomicBool::new(false)
            )
            .err()
            .unwrap()
            .contains("Unsupported platform")
        );
        assert_eq!(fs::read(root.join("state.toml")).unwrap(), original);
        server.provider = "system".into();
        server.command = "disco-no-such-language-runtime-fixture".into();
        assert!(
            execute(
                &root,
                "external",
                &server,
                None,
                Action::Probe,
                &AtomicBool::new(false)
            )
            .is_err()
        );
    }
    #[test]
    #[ignore = "Explicit network test: requires a disposable DISCO_LSP_INSTALL_FIXTURE directory"]
    fn managed_official_artifacts_install_update_rollback_and_remove() {
        let root =
            PathBuf::from(std::env::var_os("DISCO_LSP_INSTALL_FIXTURE").expect("owned test root"));
        assert!(root.is_absolute() && root.join(".diagnostic-copy").exists());
        let data = root.join("servers");
        fs::create_dir_all(&data).unwrap();
        let stop = AtomicBool::new(false);
        let catalog = Catalog::builtin();
        for id in ["rust-analyzer", "pyright", "typescript"] {
            let server = &catalog.servers[id];
            let r = execute(&data, id, server, None, Action::Install, &stop).unwrap();
            assert_eq!(r.database.servers[id].current, server.version);
            let probe = execute(&data, id, server, None, Action::Probe, &stop).unwrap();
            eprintln!("{id}: {}", probe.message);
            let check = execute(&data, id, server, None, Action::Check, &stop).unwrap();
            assert!(check.update.is_some());
            if id == "rust-analyzer" {
                let v: Value = serde_json::from_slice(
                    &fetch(
                        &format!(
                            "https://api.github.com/repos/{}/releases/tags/2026-10-19",
                            server.repository
                        ),
                        &root,
                        &stop,
                    )
                    .unwrap(),
                )
                .unwrap();
                let mut old = server.clone();
                old.version = "2026-10-19".into();
                old.sha256 = v["assets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|v| v["name"] == server.asset)
                    .unwrap()["digest"]
                    .as_str()
                    .unwrap()
                    .trim_start_matches("sha256:")
                    .into();
                execute(&data, id, server, Some(old), Action::Install, &stop).unwrap();
                execute(&data, id, server, None, Action::Rollback, &stop).unwrap();
                assert_eq!(read_db(&data).servers[id].current, server.version);
                let mut bad = server.clone();
                bad.version = "2026-10-12".into();
                bad.sha256 = "0".repeat(64);
                assert!(execute(&data, id, server, Some(bad), Action::Install, &stop).is_err());
                assert_eq!(read_db(&data).servers[id].current, server.version);
            }
        }
        // Leave verified installations in the disposable profile for stdio/UI probes.
    }
}
