//! Ordered stdio client. All filesystem/process/protocol work stays off the UI.
use super::{
    catalog::project_root,
    protocol::{self, Completion, Diagnostic},
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::BufReader,
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Debug, PartialEq)]
pub struct Launch {
    pub id: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub extensions: Vec<String>,
    pub languages: Vec<String>,
    pub markers: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct Doc {
    pub path: PathBuf,
    pub text: Arc<str>,
    pub version: u64,
}
#[derive(Clone)]
pub struct Sync {
    pub launches: Vec<Launch>,
    pub active: Option<Doc>,
    pub open: HashSet<PathBuf>,
}
#[derive(Clone, Debug)]
pub struct Info {
    pub server: String,
    pub root: PathBuf,
    pub pid: u32,
    pub documents: usize,
    pub ready: bool,
    pub compile_config: bool,
    pub rss: u64,
    pub cpu_ticks: u64,
}
#[derive(Clone, Debug)]
pub enum Event {
    Completion {
        path: PathBuf,
        version: u64,
        request: u64,
        items: Vec<Completion>,
    },
    Hover {
        path: PathBuf,
        version: u64,
        request: u64,
        text: String,
    },
    Signature {
        path: PathBuf,
        version: u64,
        request: u64,
        help: protocol::SignatureHelp,
    },
    Resolved {
        path: PathBuf,
        version: u64,
        request: u64,
        item: Value,
    },
    Diagnostics {
        path: PathBuf,
        version: u64,
        items: Vec<Diagnostic>,
    },
    Sessions(Vec<Info>),
    Error {
        server: String,
        message: String,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Completion,
    Hover,
    Signature,
    Resolve,
}
enum Control {
    Request {
        path: PathBuf,
        version: u64,
        index: usize,
        kind: Kind,
        request: u64,
    },
    Resolve {
        path: PathBuf,
        version: u64,
        request: u64,
        item: Value,
    },
    Stop(String, Option<mpsc::Sender<()>>),
    Restart(String),
    Quit,
}
type Mail = Arc<Mutex<VecDeque<Event>>>;
fn publish(mail: &Mail, ctx: &eframe::egui::Context, event: Event) {
    let mut q = mail.lock().unwrap();
    if q.len() >= 32 {
        q.pop_front();
    }
    q.push_back(event);
    drop(q);
    ctx.request_repaint();
}
pub struct Client {
    latest: Arc<Mutex<Option<Sync>>>,
    tx: SyncSender<Control>,
    mail: Mail,
    thread: Option<thread::JoinHandle<()>>,
    request: u64,
}
impl Client {
    pub fn new(ctx: &eframe::egui::Context) -> Self {
        Self::with_idle(ctx, Duration::from_secs(30))
    }
    fn with_idle(ctx: &eframe::egui::Context, idle: Duration) -> Self {
        let latest = Arc::new(Mutex::new(None));
        let input = latest.clone();
        let mail = Arc::new(Mutex::new(VecDeque::new()));
        let output = mail.clone();
        let (tx, rx) = mpsc::sync_channel(16);
        let ctx = ctx.clone();
        let thread = thread::spawn(move || run(input, rx, output, ctx, idle));
        Self {
            latest,
            tx,
            mail,
            thread: Some(thread),
            request: 0,
        }
    }
    pub fn sync(&self, sync: Sync) {
        *self.latest.lock().unwrap() = Some(sync);
    }
    pub fn request(&mut self, path: PathBuf, version: u64, index: usize, kind: Kind) -> u64 {
        self.request += 1;
        let _ = self.tx.try_send(Control::Request {
            path,
            version,
            index,
            kind,
            request: self.request,
        });
        self.request
    }
    pub fn resolve(&mut self, path: PathBuf, version: u64, item: Value) -> u64 {
        self.request += 1;
        let request = self.request;
        let _ = self.tx.try_send(Control::Resolve {
            path,
            version,
            request,
            item,
        });
        request
    }
    pub fn poll(&self) -> Vec<Event> {
        self.mail.lock().unwrap().drain(..).collect()
    }
    pub fn stop(&self, id: &str) -> Receiver<()> {
        let (tx, rx) = mpsc::channel();
        let _ = self.tx.try_send(Control::Stop(id.into(), Some(tx)));
        rx
    }
    pub fn restart(&self, id: &str) {
        let _ = self.tx.try_send(Control::Restart(id.into()));
    }
    pub fn shutdown(&mut self) {
        let _ = self.tx.send(Control::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.shutdown();
    }
}
#[derive(Clone)]
struct Pending {
    path: PathBuf,
    version: u64,
    kind: Kind,
    request: u64,
    at: Instant,
}
struct Open {
    doc: Doc,
    sent: bool,
}
struct Session {
    launch: Launch,
    root: PathBuf,
    child: Child,
    input: SyncSender<Value>,
    output: Receiver<Result<Value, String>>,
    docs: HashMap<PathBuf, Open>,
    pending: HashMap<u64, Pending>,
    next: u64,
    ready: bool,
    started: Instant,
    idle: Option<Instant>,
    log: Arc<Mutex<String>>,
    capabilities: Value,
    queued: Vec<(PathBuf, u64, usize, Kind, u64)>,
}
impl Session {
    fn start(launch: Launch, root: PathBuf) -> Result<Self, String> {
        let command = super::catalog::executable(&launch.command)
            .ok_or_else(|| format!("Language server not found: {}", launch.command.display()))?;
        let mut child = Command::new(command)
            .args(&launch.args)
            .args(
                (launch.id == "clangd"
                    && !launch.args.iter().any(|a| {
                        a == "--completion-style" || a.starts_with("--completion-style=")
                    }))
                .then_some("--completion-style=bundled"),
            )
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut stderr = child.stderr.take().unwrap();
        let log = Arc::new(Mutex::new(String::new()));
        let log_writer = log.clone();
        thread::spawn(move || {
            use std::io::Read;
            let mut bytes = [0; 1024];
            loop {
                match stderr.read(&mut bytes) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut log = log_writer.lock().unwrap();
                        log.push_str(&String::from_utf8_lossy(&bytes[..n]));
                        if log.len() > 16384 {
                            let mut cut = log.len() - 16384;
                            while !log.is_char_boundary(cut) {
                                cut += 1;
                            }
                            log.drain(..cut);
                        }
                    }
                }
            }
        });
        let mut stdin = child.stdin.take().unwrap();
        let (input, writes) = mpsc::sync_channel::<Value>(64);
        thread::spawn(move || {
            while let Ok(v) = writes.recv() {
                if protocol::write(&mut stdin, &v).is_err() {
                    break;
                }
            }
        });
        let stdout = child.stdout.take().unwrap();
        let (tx, output) = mpsc::sync_channel(8);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match protocol::read(&mut reader) {
                    Ok(Some(v)) => {
                        if tx.send(Ok(v)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = tx.send(Err("Language server closed stdout".into()));
                        break;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string()));
                        break;
                    }
                }
            }
        });
        let mut s = Self {
            launch,
            root,
            child,
            input,
            output,
            docs: HashMap::new(),
            pending: HashMap::new(),
            next: 2,
            ready: false,
            started: Instant::now(),
            idle: None,
            log,
            capabilities: Value::Null,
            queued: vec![],
        };
        let root = protocol::uri(&s.root)?;
        s.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":std::process::id(),"clientInfo":{"name":"disco","version":env!("CARGO_PKG_VERSION")},"rootUri":root,"workspaceFolders":[{"uri":root,"name":s.root.file_name().unwrap_or_default().to_string_lossy()}],"capabilities":{"general":{"positionEncodings":["utf-16"]},"workspace":{"configuration":false,"applyEdit":false},"textDocument":{"completion":{"completionItem":{"snippetSupport":false,"documentationFormat":["plaintext"],"resolveSupport":{"properties":["documentation","detail"]}}},"hover":{"contentFormat":["plaintext"]},"signatureHelp":{"signatureInformation":{"documentationFormat":["plaintext"],"parameterInformation":{"labelOffsetSupport":true},"activeParameterSupport":true}},"publishDiagnostics":{"versionSupport":true}}},"initializationOptions":if s.launch.id=="rust-analyzer"{json!({"checkOnSave":false,"cargo":{"buildScripts":{"enable":false}},"procMacro":{"enable":false}})}else{Value::Null}}}))?;
        Ok(s)
    }
    fn send(&mut self, v: Value) -> Result<(), String> {
        self.input
            .try_send(v)
            .map_err(|e| format!("Language server input queue: {e}"))
    }
    fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.send(json!({"jsonrpc":"2.0","method":method,"params":params}))
    }
    fn update(&mut self, doc: Doc) -> Result<(), String> {
        if self
            .docs
            .get(&doc.path)
            .is_some_and(|old| old.doc.version == doc.version)
        {
            return Ok(());
        }
        let stale: Vec<_> = self
            .pending
            .iter()
            .filter(|(_, p)| p.path == doc.path && p.version != doc.version)
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            self.pending.remove(&id);
            self.notify("$/cancelRequest", json!({"id":id}))?;
        }
        let sent = self.docs.get(&doc.path).is_some_and(|d| d.sent);
        self.docs.insert(
            doc.path.clone(),
            Open {
                doc: doc.clone(),
                sent,
            },
        );
        if self.ready {
            self.flush_doc(&doc.path)?;
        }
        self.idle = None;
        Ok(())
    }
    fn flush_doc(&mut self, path: &PathBuf) -> Result<(), String> {
        let open = self.docs.get(path).unwrap();
        let doc = open.doc.clone();
        let sent = open.sent;
        let uri = protocol::uri(path)?;
        if sent {
            self.notify("textDocument/didChange",json!({"textDocument":{"uri":uri,"version":doc.version},"contentChanges":[{"text":doc.text.as_ref()}]}))?;
        } else {
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            let language = self
                .launch
                .extensions
                .iter()
                .position(|e| e == ext)
                .and_then(|i| self.launch.languages.get(i))
                .cloned()
                .unwrap_or_default();
            self.notify("textDocument/didOpen",json!({"textDocument":{"uri":uri,"languageId":language,"version":doc.version,"text":doc.text.as_ref()}}))?;
        }
        self.docs.get_mut(path).unwrap().sent = true;
        Ok(())
    }
    fn close(&mut self, path: &PathBuf) -> Result<(), String> {
        if let Some(open) = self.docs.remove(path) {
            if open.sent {
                self.notify(
                    "textDocument/didClose",
                    json!({"textDocument":{"uri":protocol::uri(path)?}}),
                )?;
            }
        }
        if self.docs.is_empty() {
            self.idle = Some(Instant::now());
        }
        Ok(())
    }
    fn request(
        &mut self,
        path: PathBuf,
        version: u64,
        index: usize,
        kind: Kind,
        request: u64,
    ) -> Result<(), String> {
        if !self.ready {
            self.queued
                .retain(|(p, _, _, k, _)| p != &path || *k != kind);
            if self.queued.len() < 16 {
                self.queued.push((path, version, index, kind, request));
            }
            return Ok(());
        }
        let Some(doc) = self.docs.get(&path).filter(|d| d.doc.version == version) else {
            return Ok(());
        };
        let position = protocol::position(&doc.doc.text, index);
        super::trace(format_args!(
            "server request {kind:?} version={version} request={request} position={position:?}"
        ));

        let (method, capability) = match kind {
            Kind::Completion => ("textDocument/completion", "completionProvider"),
            Kind::Hover => ("textDocument/hover", "hoverProvider"),
            Kind::Signature => ("textDocument/signatureHelp", "signatureHelpProvider"),
            Kind::Resolve => return Ok(()),
        };
        if self.capabilities[capability].is_null()
            || self.capabilities[capability] == Value::Bool(false)
        {
            return Ok(());
        }
        let params = json!({"textDocument":{"uri":protocol::uri(&path)?},"position":position});
        self.request_with_params(path, version, kind, request, method, params)
    }
    fn resolve(
        &mut self,
        path: PathBuf,
        version: u64,
        request: u64,
        item: Value,
    ) -> Result<(), String> {
        if !self.ready
            || self
                .docs
                .get(&path)
                .is_none_or(|d| d.doc.version != version)
            || self.capabilities["completionProvider"]["resolveProvider"] != true
        {
            return Ok(());
        }
        self.request_with_params(
            path,
            version,
            Kind::Resolve,
            request,
            "completionItem/resolve",
            item,
        )
    }
    fn request_with_params(
        &mut self,
        path: PathBuf,
        version: u64,
        kind: Kind,
        request: u64,
        method: &str,
        params: Value,
    ) -> Result<(), String> {
        let old: Vec<_> = self
            .pending
            .iter()
            .filter(|(_, p)| p.kind == kind && p.path == path)
            .map(|(id, _)| *id)
            .collect();
        for id in old {
            self.pending.remove(&id);
            self.notify("$/cancelRequest", json!({"id":id}))?;
        }
        let id = self.next;
        self.next += 1;
        self.pending.insert(
            id,
            Pending {
                path,
                version,
                kind,
                request,
                at: Instant::now(),
            },
        );
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
    }
    fn process(
        &mut self,
        value: Value,
        mail: &Mail,
        ctx: &eframe::egui::Context,
    ) -> Result<(), String> {
        if value.get("method").is_some() && value.get("id").is_some() {
            let method = value["method"].as_str().unwrap_or("");
            let result = match method {
                "workspace/configuration" => Value::Array(
                    value["params"]["items"]
                        .as_array()
                        .map_or(vec![], |items| vec![json!({}); items.len()]),
                ),
                "window/workDoneProgress/create" => Value::Null,
                "workspace/workspaceFolders" => {
                    json!([{"uri":protocol::uri(&self.root)?,"name":self.root.file_name().unwrap_or_default().to_string_lossy()}])
                }
                _ => {
                    return self.send(json!({"jsonrpc":"2.0","id":value["id"],"error":{"code":-32601,"message":"Unsupported client request"}}));
                }
            };
            return self.send(json!({"jsonrpc":"2.0","id":value["id"],"result":result}));
        }
        if value["id"].as_u64() == Some(1) {
            if let Some(error) = value.get("error") {
                return Err(error.to_string());
            }
            self.capabilities = value["result"]["capabilities"].clone();
            if self.capabilities["positionEncoding"]
                .as_str()
                .is_some_and(|e| e != "utf-16")
            {
                return Err("Unsupported server position encoding".into());
            }
            self.ready = true;
            self.notify("initialized", json!({}))?;
            for path in self.docs.keys().cloned().collect::<Vec<_>>() {
                self.flush_doc(&path)?;
            }
            for (path, version, index, kind, request) in std::mem::take(&mut self.queued) {
                self.request(path, version, index, kind, request)?;
            }
            return Ok(());
        }
        if let Some(id) = value["id"].as_u64() {
            if let Some(p) = self.pending.remove(&id) {
                if self
                    .docs
                    .get(&p.path)
                    .is_none_or(|d| d.doc.version != p.version)
                {
                    return Ok(());
                }
                if let Some(e) = value.get("error") {
                    if ![-32800, -32801].contains(&e["code"].as_i64().unwrap_or(0)) {
                        publish(
                            mail,
                            ctx,
                            Event::Error {
                                server: self.launch.id.clone(),
                                message: e["message"]
                                    .as_str()
                                    .unwrap_or("LSP request failed")
                                    .into(),
                            },
                        );
                    }
                    return Ok(());
                }
                let result = &value["result"];
                if p.kind == Kind::Completion {
                    super::trace(format_args!(
                        "server completion reply request={} raw_items={}",
                        p.request,
                        result
                            .as_array()
                            .or_else(|| result["items"].as_array())
                            .map_or(0, Vec::len)
                    ));
                }
                let event = match p.kind {
                    Kind::Completion => Event::Completion {
                        path: p.path,
                        version: p.version,
                        request: p.request,
                        items: protocol::completion_items(result),
                    },
                    Kind::Hover => Event::Hover {
                        path: p.path,
                        version: p.version,
                        request: p.request,
                        text: protocol::hover_text(result),
                    },
                    Kind::Signature => Event::Signature {
                        path: p.path,
                        version: p.version,
                        request: p.request,
                        help: protocol::signature_help(result),
                    },
                    Kind::Resolve => Event::Resolved {
                        path: p.path,
                        version: p.version,
                        request: p.request,
                        item: result.clone(),
                    },
                };
                publish(mail, ctx, event);
            }
            return Ok(());
        }
        if value["method"] == "textDocument/publishDiagnostics" {
            if let Some(path) = value["params"]["uri"].as_str().and_then(protocol::path) {
                if let Some(open) = self.docs.get(&path) {
                    if value["params"]["version"]
                        .as_u64()
                        .is_some_and(|v| v != open.doc.version)
                    {
                        return Ok(());
                    }
                    let items = value["params"]["diagnostics"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .take(200)
                        .filter_map(|d| {
                            Some(Diagnostic {
                                range: serde_json::from_value(d["range"].clone()).ok()?,
                                severity: d["severity"].as_u64().unwrap_or(1) as u8,
                                message: d["message"].as_str()?.chars().take(2048).collect(),
                            })
                        })
                        .collect();
                    publish(
                        mail,
                        ctx,
                        Event::Diagnostics {
                            path,
                            version: open.doc.version,
                            items,
                        },
                    );
                }
            }
        }
        Ok(())
    }
    fn stop(&mut self) {
        let _ = self.send(json!({"jsonrpc":"2.0","id":0,"method":"shutdown","params":null}));
        let until = Instant::now() + Duration::from_millis(200);
        while Instant::now() < until {
            if let Ok(Ok(v)) = self.output.recv_timeout(Duration::from_millis(10)) {
                if v["id"] == 0 {
                    break;
                }
            } else if self.child.try_wait().ok().flatten().is_some() {
                break;
            }
        }
        let _ = self.notify("exit", Value::Null);
        let until = Instant::now() + Duration::from_millis(200);
        while Instant::now() < until {
            if self.child.try_wait().ok().flatten().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGTERM);
        }
        thread::sleep(Duration::from_millis(30));
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.wait();
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}
pub(crate) fn metrics(pid: u32) -> (u64, u64) {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    let tail = stat.rsplit_once(')').map_or("", |(_, s)| s);
    let f: Vec<_> = tail.split_whitespace().collect();
    let ticks = f.get(11).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0)
        + f.get(12).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let rss = std::fs::read_to_string(format!("/proc/{pid}/status"))
        .unwrap_or_default()
        .lines()
        .find(|s| s.starts_with("VmRSS:"))
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
        .unwrap_or(0)
        * 1024;
    (rss, ticks)
}
fn run(
    latest: Arc<Mutex<Option<Sync>>>,
    control: Receiver<Control>,
    mail: Mail,
    ctx: eframe::egui::Context,
    idle: Duration,
) {
    crate::scan::background_priority();
    let mut sessions: HashMap<(String, PathBuf), Session> = HashMap::new();
    let mut suppressed = HashSet::new();
    let mut last_sync: Option<Sync> = None;
    let mut deferred: Option<Control> = None;
    let mut last_status = Instant::now() - Duration::from_secs(2);
    loop {
        // Drop the mailbox lock before resolving roots, spawning or stopping
        // processes. UI sync only ever swaps a small snapshot under this lock.
        let sync = latest.lock().unwrap().take();
        if let Some(sync) = sync {
            let enabled: HashSet<_> = sync.launches.iter().map(|l| l.id.clone()).collect();
            suppressed.retain(|id| enabled.contains(id));
            sessions.retain(|_, s| {
                sync.launches.contains(&s.launch) && !suppressed.contains(&s.launch.id)
            });
            for s in sessions.values_mut() {
                for path in s
                    .docs
                    .keys()
                    .filter(|p| !sync.open.contains(*p))
                    .cloned()
                    .collect::<Vec<_>>()
                {
                    let _ = s.close(&path);
                }
            }
            if let Some(doc) = &sync.active {
                let ext = doc.path.extension().and_then(|s| s.to_str()).unwrap_or("");
                if let Some(launch) = sync
                    .launches
                    .iter()
                    .find(|l| l.extensions.iter().any(|e| e == ext) && !suppressed.contains(&l.id))
                {
                    let root = project_root(&doc.path, &launch.markers);
                    let key = (launch.id.clone(), root.clone());
                    if !sessions.contains_key(&key) && sessions.len() >= 4 {
                        publish(&mail,&ctx,Event::Error{server:launch.id.clone(),message:"Four active LSP projects; close tabs or stop a server before opening another".into()});
                    }
                    if !sessions.contains_key(&key) && sessions.len() < 4 {
                        match Session::start(launch.clone(), root) {
                            Ok(s) => {
                                sessions.insert(key.clone(), s);
                            }
                            Err(message) => publish(
                                &mail,
                                &ctx,
                                Event::Error {
                                    server: launch.id.clone(),
                                    message,
                                },
                            ),
                        }
                    }
                    let mut failed = false;
                    if let Some(s) = sessions.get_mut(&key) {
                        if let Err(message) = s.update(doc.clone()) {
                            publish(
                                &mail,
                                &ctx,
                                Event::Error {
                                    server: s.launch.id.clone(),
                                    message,
                                },
                            );
                            failed = true;
                        }
                    }
                    if failed {
                        sessions.remove(&key);
                        suppressed.insert(launch.id.clone());
                    }
                }
            }
            last_sync = Some(sync);
            last_status = Instant::now() - Duration::from_secs(2);
        }
        match deferred
            .take()
            .map(Ok)
            .unwrap_or_else(|| control.recv_timeout(Duration::from_millis(20)))
        {
            Ok(Control::Quit) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(Control::Stop(id, ack)) => {
                suppressed.insert(id.clone());
                sessions.retain(|(name, _), _| name != &id);
                if let Some(ack) = ack {
                    let _ = ack.send(());
                }
                last_status = Instant::now() - Duration::from_secs(2);
            }
            Ok(Control::Restart(id)) => {
                suppressed.remove(&id);
                sessions.retain(|(name, _), _| name != &id);
                if let Some(sync) = last_sync.clone() {
                    latest.lock().unwrap().get_or_insert(sync);
                }
            }
            Ok(Control::Request {
                path,
                version,
                index,
                kind,
                request,
            }) => {
                // A request can wake recv_timeout after the loop's mailbox
                // check. Apply its preceding document sync before validating
                // the request version, including on an otherwise idle UI.
                if latest.lock().unwrap().is_some() {
                    deferred = Some(Control::Request {
                        path,
                        version,
                        index,
                        kind,
                        request,
                    });
                    continue;
                }
                for s in sessions.values_mut() {
                    if s.docs.contains_key(&path) {
                        if let Err(message) = s.request(path.clone(), version, index, kind, request)
                        {
                            publish(
                                &mail,
                                &ctx,
                                Event::Error {
                                    server: s.launch.id.clone(),
                                    message,
                                },
                            );
                        }
                        break;
                    }
                }
            }
            Ok(Control::Resolve {
                path,
                version,
                request,
                item,
            }) => {
                if latest.lock().unwrap().is_some() {
                    deferred = Some(Control::Resolve {
                        path,
                        version,
                        request,
                        item,
                    });
                    continue;
                }
                for session in sessions.values_mut() {
                    if session.docs.contains_key(&path) {
                        if let Err(message) = session.resolve(path.clone(), version, request, item)
                        {
                            publish(
                                &mail,
                                &ctx,
                                Event::Error {
                                    server: session.launch.id.clone(),
                                    message,
                                },
                            );
                        }
                        break;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let mut dead = vec![];
        for (key, s) in &mut sessions {
            for _ in 0..64 {
                match s.output.try_recv() {
                    Ok(Ok(v)) => {
                        if let Err(message) = s.process(v, &mail, &ctx) {
                            publish(
                                &mail,
                                &ctx,
                                Event::Error {
                                    server: s.launch.id.clone(),
                                    message,
                                },
                            );
                            dead.push(key.clone());
                            break;
                        }
                    }
                    Ok(Err(message)) => {
                        let message = format!("{message}: {}", s.log.lock().unwrap());
                        publish(
                            &mail,
                            &ctx,
                            Event::Error {
                                server: s.launch.id.clone(),
                                message,
                            },
                        );
                        dead.push(key.clone());
                        break;
                    }
                    Err(_) => break,
                }
            }
            if !s.ready && s.started.elapsed() > Duration::from_secs(15) {
                publish(
                    &mail,
                    &ctx,
                    Event::Error {
                        server: s.launch.id.clone(),
                        message: format!(
                            "Language server initialization timed out: {}",
                            s.log.lock().unwrap()
                        ),
                    },
                );
                dead.push(key.clone());
            }
            if s.idle.is_some_and(|at| at.elapsed() > idle) {
                dead.push(key.clone());
            }
            let expired: Vec<_> = s
                .pending
                .iter()
                .filter(|(_, p)| p.at.elapsed() > Duration::from_secs(10))
                .map(|(id, _)| *id)
                .collect();
            for id in expired {
                s.pending.remove(&id);
                let _ = s.notify("$/cancelRequest", json!({"id":id}));
            }
        }
        for key in dead {
            sessions.remove(&key);
            last_status = Instant::now() - Duration::from_secs(2);
        }
        if last_status.elapsed() > Duration::from_secs(1) {
            last_status = Instant::now();
            publish(
                &mail,
                &ctx,
                Event::Sessions(
                    sessions
                        .values()
                        .map(|s| {
                            let (rss, cpu_ticks) = metrics(s.child.id());
                            Info {
                                compile_config: s.launch.id != "clangd"
                                    || s.root.join("compile_commands.json").exists()
                                    || s.root.join("compile_flags.txt").exists(),
                                server: s.launch.id.clone(),
                                root: s.root.clone(),
                                pid: s.child.id(),
                                documents: s.docs.len(),
                                ready: s.ready,
                                rss,
                                cpu_ticks,
                            }
                        })
                        .collect(),
                ),
            );
        }
    }
    drop(sessions);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_clangd_completion_hover_diagnostics_shared_session_and_idle_shutdown() {
        if Command::new("clangd").arg("--version").output().is_err() {
            return;
        }
        let root = std::env::temp_dir().join(format!("disco-lsp-clangd-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("main.cpp");
        let source = "#include <vector>\nint answer(){return 42;}\nvoid f(){ /* 🦀 */ std:: }\nint broken = ;\n";
        std::fs::write(&path, source).unwrap();
        std::fs::write(root.join("compile_flags.txt"), "-std=c++20\n").unwrap();
        let ctx = eframe::egui::Context::default();
        let mut client = Client::with_idle(&ctx, Duration::from_millis(150));
        let launch = Launch {
            id: "clangd".into(),
            command: "clangd".into(),
            args: vec![
                "--background-index=false".into(),
                "--clang-tidy=false".into(),
            ],
            extensions: vec!["cpp".into()],
            languages: vec!["cpp".into()],
            markers: vec!["compile_flags.txt".into()],
        };
        client.sync(Sync {
            launches: vec![launch.clone()],
            active: Some(Doc {
                path: path.clone(),
                text: Arc::from(source),
                version: 1,
            }),
            open: HashSet::from([path.clone()]),
        });
        let until = Instant::now() + Duration::from_secs(15);
        let mut pid = 0;
        let mut diagnostic = false;
        while Instant::now() < until {
            for event in client.poll() {
                match event {
                    Event::Sessions(info) => {
                        if let Some(s) = info.first().filter(|s| s.ready) {
                            pid = s.pid;
                        }
                    }
                    Event::Diagnostics { items, .. } => diagnostic |= !items.is_empty(),
                    Event::Error { message, .. } => panic!("{message}"),
                    _ => {}
                }
            }
            if pid != 0 && diagnostic {
                break;
            }
            thread::sleep(Duration::from_millis(15));
        }
        assert!(pid != 0 && diagnostic, "server readiness and diagnostics");
        let index = source[..source.find("std::").unwrap() + 5].chars().count();
        client.request(path.clone(), 1, index, Kind::Completion);
        let until = Instant::now() + Duration::from_secs(10);
        let mut found = false;
        let mut retry = Instant::now();
        while Instant::now() < until {
            for event in client.poll() {
                if let Event::Completion { items, .. } = event {
                    found = !items.is_empty();
                } else if let Event::Error { message, .. } = event {
                    eprintln!("request error: {message}");
                }
            }
            if found {
                break;
            }
            if retry.elapsed() > Duration::from_millis(200) {
                client.request(path.clone(), 1, index, Kind::Completion);
                retry = Instant::now();
            }
            thread::sleep(Duration::from_millis(15));
        }
        assert!(
            found,
            "std:: completion with a UTF-16 emoji before the cursor"
        );
        let index = source[..source.find("answer").unwrap() + 2].chars().count();
        client.request(path.clone(), 1, index, Kind::Hover);
        let until = Instant::now() + Duration::from_secs(10);
        let mut hover = false;
        while Instant::now() < until {
            for event in client.poll() {
                if let Event::Hover { text, .. } = event {
                    hover = !text.is_empty();
                }
            }
            if hover {
                break;
            }
            thread::sleep(Duration::from_millis(15));
        }
        assert!(hover);
        let second = root.join("second.cpp");
        std::fs::write(&second, "int second;\n").unwrap();
        client.sync(Sync {
            launches: vec![launch.clone()],
            active: Some(Doc {
                path: second.clone(),
                text: Arc::from("int second;\n"),
                version: 2,
            }),
            open: HashSet::from([path.clone(), second.clone()]),
        });
        let until = Instant::now() + Duration::from_secs(5);
        let mut shared = false;
        while Instant::now() < until {
            for event in client.poll() {
                if let Event::Sessions(info) = event {
                    shared = info.len() == 1 && info[0].pid == pid && info[0].documents == 2;
                    if shared {
                        eprintln!(
                            "clangd pid={pid} rss={} cpu_ticks={}",
                            info[0].rss, info[0].cpu_ticks
                        );
                    }
                }
            }
            if shared {
                break;
            }
            thread::sleep(Duration::from_millis(15));
        }
        assert!(shared);
        client.sync(Sync {
            launches: vec![launch],
            active: None,
            open: HashSet::new(),
        });
        let until = Instant::now() + Duration::from_secs(4);
        while std::path::Path::new(&format!("/proc/{pid}")).exists() && Instant::now() < until {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        client.shutdown();
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn obsolete_replies_and_diagnostics_never_reach_the_mailbox() {
        let launch = Launch {
            id: "test".into(),
            command: PathBuf::from("sleep"),
            args: vec!["60".into()],
            extensions: vec!["txt".into()],
            languages: vec!["text".into()],
            markers: vec![],
        };
        let mut s = Session::start(launch, std::env::temp_dir()).unwrap();
        let pid = s.child.id();
        let ctx = eframe::egui::Context::default();
        let mail = Arc::new(Mutex::new(VecDeque::new()));
        s.process(json!({"id":1,"result":{"capabilities":{"hoverProvider":true,"completionProvider":{}}}}),&mail,&ctx).unwrap();
        let path = std::env::temp_dir().join("disco-lsp-stale.txt");
        s.update(Doc {
            path: path.clone(),
            text: Arc::from("old"),
            version: 1,
        })
        .unwrap();
        s.request(path.clone(), 1, 2, Kind::Completion, 1).unwrap();
        let old = s.next - 1;
        s.update(Doc {
            path: path.clone(),
            text: Arc::from("new"),
            version: 2,
        })
        .unwrap();
        s.process(
            json!({"id":old,"result":[{"label":"obsolete"}]}),
            &mail,
            &ctx,
        )
        .unwrap();
        assert!(mail.lock().unwrap().is_empty());
        s.process(json!({"method":"textDocument/publishDiagnostics","params":{"uri":protocol::uri(&path).unwrap(),"version":1,"diagnostics":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"message":"old"}]}}),&mail,&ctx).unwrap();
        assert!(mail.lock().unwrap().is_empty());
        s.request(path.clone(), 2, 2, Kind::Completion, 2).unwrap();
        let current = s.next - 1;
        s.process(
            json!({"id":current,"result":[{"label":"current"}]}),
            &mail,
            &ctx,
        )
        .unwrap();
        assert!(matches!(
            mail.lock().unwrap().pop_front(),
            Some(Event::Completion { version: 2, .. })
        ));
        s.close(&path).unwrap();
        s.process(json!({"method":"textDocument/publishDiagnostics","params":{"uri":protocol::uri(&path).unwrap(),"version":2,"diagnostics":[]}}),&mail,&ctx).unwrap();
        assert!(mail.lock().unwrap().is_empty());
        drop(s);
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
    #[test]
    #[ignore = "Explicit offline smoke test against previously verified disposable managed artifacts"]
    fn managed_language_servers_initialize_and_stop_offline() {
        let fixture =
            PathBuf::from(std::env::var_os("DISCO_LSP_INSTALL_FIXTURE").expect("owned fixture"));
        assert!(fixture.join(".diagnostic-copy").exists());
        for (id, filename, source, marker) in [
            ("rust-analyzer", "main.rs", "fn main( {\n", "Cargo.toml"),
            ("pyright", "main.py", "x: int = 'bad'\n", "pyproject.toml"),
            (
                "typescript",
                "main.ts",
                "let x: number = 'bad';\n",
                "tsconfig.json",
            ),
        ] {
            let catalog = super::super::catalog::Catalog::builtin();
            let server = &catalog.servers[id];
            let root = fixture.join(format!("offline-{id}"));
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join(marker),match id{"rust-analyzer"=>"[package]\nname='lsp-fixture'\nversion='0.1.0'\nedition='2024'\n[[bin]]\nname='fixture'\npath='main.rs'\n","typescript"=>"{\"compilerOptions\":{\"strict\":true}}",_=>""}).unwrap();
            let path = root.join(filename);
            std::fs::write(&path, source).unwrap();
            let command = if server.provider == "npm" {
                fixture
                    .join("servers")
                    .join(id)
                    .join(&server.version)
                    .join("node_modules/.bin")
                    .join(&server.command)
            } else {
                fixture
                    .join("servers")
                    .join(id)
                    .join(&server.version)
                    .join(&server.command)
            };
            let launch = Launch {
                id: id.into(),
                command,
                args: server.args.clone(),
                extensions: server.extensions.clone(),
                languages: server.languages.clone(),
                markers: server.root_markers.clone(),
            };
            let ctx = eframe::egui::Context::default();
            let mut client = Client::new(&ctx);
            client.sync(Sync {
                launches: vec![launch],
                active: Some(Doc {
                    path: path.clone(),
                    text: Arc::from(source),
                    version: 1,
                }),
                open: HashSet::from([path]),
            });
            let until = Instant::now() + Duration::from_secs(30);
            let mut ready = false;
            let mut diagnostics = false;
            while Instant::now() < until {
                for event in client.poll() {
                    match event {
                        Event::Sessions(info) => ready |= info.iter().any(|s| s.ready),
                        Event::Diagnostics { items, .. } => diagnostics |= !items.is_empty(),
                        Event::Error { message, .. } => panic!("{id}: {message}"),
                        _ => {}
                    }
                }
                if ready && diagnostics {
                    break;
                }
                thread::sleep(Duration::from_millis(30));
            }
            assert!(ready, "{id} initialization");
            eprintln!("{id}: ready={ready} diagnostics={diagnostics}");
            client.shutdown();
        }
    }
    #[test]
    fn clangd_bundles_overloads_and_returns_signature_documentation() {
        if super::super::catalog::executable(std::path::Path::new("clangd")).is_none() {
            return;
        }
        let root = std::env::temp_dir().join(format!("disco-overloads-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("compile_flags.txt"), "-std=c++20\n").unwrap();
        let path = root.join("main.cpp");
        let source = "/** Add two integers.\n * @param first First value.\n * @param second Second value.\n */\nint combine(int first, int second);\n/** Add two decimal values. */\ndouble combine(double first, double second);\n/** Print text. */\nvoid combine(const char *text);\nvoid use() { comb }\n";
        std::fs::write(&path, source).unwrap();
        let launch = Launch {
            id: "clangd".into(),
            command: PathBuf::from("clangd"),
            args: vec!["--background-index=false".into()],
            extensions: vec!["cpp".into()],
            languages: vec!["cpp".into()],
            markers: vec!["compile_flags.txt".into()],
        };
        let ctx = eframe::egui::Context::default();
        let mut client = Client::new(&ctx);
        let sync = |text: &str, version| Sync {
            launches: vec![launch.clone()],
            active: Some(Doc {
                path: path.clone(),
                text: Arc::from(text),
                version,
            }),
            open: HashSet::from([path.clone()]),
        };
        client.sync(sync(source, 1));
        let until = Instant::now() + Duration::from_secs(8);
        let mut ready = false;
        while !ready && Instant::now() < until {
            for e in client.poll() {
                if let Event::Sessions(s) = e {
                    ready = s.first().is_some_and(|s| s.ready);
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(ready);
        let index = source[..source.find("comb }").unwrap() + 4].chars().count();
        client.request(path.clone(), 1, index, Kind::Completion);
        let until = Instant::now() + Duration::from_secs(5);
        let mut items = vec![];
        while items.is_empty() && Instant::now() < until {
            for e in client.poll() {
                if let Event::Completion { items: i, .. } = e {
                    items = i
                }
            }
            thread::sleep(Duration::from_millis(15));
        }
        let grouped: Vec<_> = items
            .iter()
            .filter(|i| i.label.contains("combine"))
            .collect();
        assert_eq!(
            grouped.len(),
            1,
            "clangd should bundle overloads once signature help is advertised: {:?}",
            items
                .iter()
                .map(|i| (&i.label, &i.detail))
                .collect::<Vec<_>>()
        );
        eprintln!(
            "bundled: {:?}",
            grouped
                .iter()
                .map(|i| (&i.label, &i.detail, &i.raw["documentation"]))
                .collect::<Vec<_>>()
        );
        let call = source.replace("comb }", "combine(1, ) }");
        let index = call[..call.rfind("combine(1, ").unwrap() + 11]
            .chars()
            .count();
        client.sync(sync(&call, 2));
        client.request(path.clone(), 2, index, Kind::Signature);
        let until = Instant::now() + Duration::from_secs(5);
        let mut help = None;
        while help.is_none() && Instant::now() < until {
            for e in client.poll() {
                if let Event::Signature { help: h, .. } = e {
                    help = Some(h)
                }
            }
            thread::sleep(Duration::from_millis(15));
        }
        let help = help.expect("signature reply");
        eprintln!("signatures: {help:?}");
        assert!(help.signatures.len() >= 2);
        assert!(
            help.signatures
                .iter()
                .any(|s| s.documentation.contains("Add"))
        );
        assert!(
            help.active_parameter == Some(1)
                || help
                    .signatures
                    .iter()
                    .any(|s| s.active_parameter == Some(1))
        );
        assert!(
            help.signatures
                .iter()
                .any(|s| s.documentation.contains("Second"))
        );
        assert!(help.signatures.iter().any(|s| {
            s.parameters
                .get(1)
                .and_then(|p| p.range.clone())
                .is_some_and(|r| s.label[r].contains("second"))
        }));
        client.shutdown();
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn lazy_documentation_responses_are_versioned_and_obsolete_ones_are_ignored() {
        let launch = Launch {
            id: "fixture".into(),
            command: PathBuf::from("sleep"),
            args: vec!["60".into()],
            extensions: vec!["txt".into()],
            languages: vec!["text".into()],
            markers: vec![],
        };
        let mut session = Session::start(launch, std::env::temp_dir()).unwrap();
        let ctx = eframe::egui::Context::default();
        let mail = Arc::new(Mutex::new(VecDeque::new()));
        session.process(json!({"id":1,"result":{"capabilities":{"completionProvider":{"resolveProvider":true}}}}),&mail,&ctx).unwrap();
        let path = std::env::temp_dir().join("disco-lsp-resolve.txt");
        session
            .update(Doc {
                path: path.clone(),
                text: Arc::from("first"),
                version: 1,
            })
            .unwrap();
        session
            .resolve(path.clone(), 1, 10, json!({"label":"first","data":1}))
            .unwrap();
        let old = session.next - 1;
        session
            .update(Doc {
                path: path.clone(),
                text: Arc::from("second"),
                version: 2,
            })
            .unwrap();
        session
            .process(
                json!({"id":old,"result":{"documentation":"outdated"}}),
                &mail,
                &ctx,
            )
            .unwrap();
        assert!(mail.lock().unwrap().is_empty());
        session
            .resolve(path, 2, 11, json!({"label":"second","data":2}))
            .unwrap();
        let fresh = session.next - 1;
        session.process(json!({"id":fresh,"result":{"documentation":{"kind":"plaintext","value":"Current documentation"}}}),&mail,&ctx).unwrap();
        match mail.lock().unwrap().pop_front() {
            Some(Event::Resolved {
                version: 2,
                request: 11,
                item,
                ..
            }) => assert_eq!(
                protocol::documentation(&item["documentation"]),
                "Current documentation"
            ),
            event => panic!("unexpected resolve event: {event:?}"),
        };
    }
}
