//! Carbon's engine: it reads ~/dev, starts and stops dev servers, and asks the OS what is listening.
//!
//! The rules it keeps:
//!   3000         Carbon itself, always.
//!   3001–3003    one port per role: website, admin, pro. Starting an app puts it on its role's port and clears
//!                whatever was there, so the same kind of thing is always at the same address.
//!   3004–3019    everything else, one port each, handed out in order and kept while the app runs.
//! What is "running" is read from the OS (lsof), never assumed from memory, so a server started from another
//! terminal or by an agent shows up too, and Carbon can be restarted without losing track of anything.
//! Every server is started through mise, so it gets the tool versions its project asks for.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    env, fs, io,
    ops::RangeInclusive,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Command,
};

use crate::agents::{self, Proc};
use crate::detect::{self, Plan};

pub const HUB_PORT: u16 = 3000;
const POOL: RangeInclusive<u16> = 3004..=3019;
const MAX_LOG: usize = 800;

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Website,
    Admin,
    Pro,
    Other,
}

const FIXED: [(u16, Role, &str); 3] = [
    (3001, Role::Website, "Website"),
    (3002, Role::Admin, "Admin"),
    (3003, Role::Pro, "Pro"),
];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct App {
    /// Path under ~/dev, e.g. "saas/cutwise/web"
    pub id: String,
    pub group: String,
    pub project: String,
    /// The folder inside the project, when the app isn't the project root
    pub part: Option<String>,
    #[serde(skip)]
    pub dir: PathBuf,
    pub role: Role,
    pub kind: &'static str,
    pub runtime: Option<&'static str>,
    /// The project names its tool versions (mise.toml or .tool-versions)
    pub pinned: bool,
    #[serde(skip)]
    plan: Plan,
    /// The port it is on now, or would be given
    pub port: u16,
    /// Exactly what Carbon runs for it
    pub command: String,
    /// It reads its port from the environment rather than a flag
    pub by_env: bool,
}

#[derive(Clone, Serialize)]
pub struct Listener {
    pub port: u16,
    pub pid: i32,
    pub command: String,
    pub cwd: Option<String>,
    pub app: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    port: u16,
    role: Role,
    label: &'static str,
    /// idle: nothing there. starting: launched, not answering yet. running: answering. exited: it stopped by itself
    status: &'static str,
    app: Option<String>,
    /// What holds the port when it isn't one of the apps Carbon knows
    holder: Option<String>,
    pid: Option<i32>,
    /// Carbon started it, so there is output to show
    has_logs: bool,
    /// When it stopped by itself: the last thing it said, which is usually why
    note: Option<String>,
    since: Option<u64>,
    /// The agent that started what is on this port, if one did: claude, openai, gemini or grok
    by: Option<&'static str>,
}

/// One server an agent started
#[derive(Serialize)]
pub struct Started {
    pid: i32,
    command: String,
    cwd: Option<String>,
    app: Option<String>,
    /// The ports its process tree is listening on
    ports: Vec<u16>,
    since: u64,
}

/// One agent session that has servers running, and those servers
#[derive(Serialize)]
pub struct Agent {
    /// The harness's key and name, e.g. "claude-code" and "Claude Code"
    harness: &'static str,
    name: &'static str,
    /// Whose mark to show: claude, openai, gemini or grok
    vendor: &'static str,
    pid: i32,
    cwd: Option<String>,
    since: u64,
    /// What the session is called, when the harness says
    title: Option<String>,
    /// "busy" or "idle", when the harness says
    status: Option<String>,
    servers: Vec<Started>,
}

#[derive(Serialize)]
pub struct JobState {
    key: String,
    title: String,
    /// running, done or failed
    status: &'static str,
}

#[derive(Serialize)]
pub struct Snapshot {
    slots: Vec<Slot>,
    apps: Vec<App>,
    others: Vec<Listener>,
    /// The agent sessions that have servers running, with those servers
    agents: Vec<Agent>,
    /// The folders directly under ~/dev, where a new project can go
    groups: Vec<String>,
    jobs: Vec<JobState>,
    mise: bool,
}

pub type Log = Arc<Mutex<VecDeque<String>>>;
pub type Exit = Arc<Mutex<Option<i32>>>;

pub fn say(log: &Log, line: impl Into<String>) {
    let mut log = log.lock().unwrap();
    log.push_back(line.into());
    while log.len() > MAX_LOG {
        log.pop_front();
    }
}

struct Managed {
    app: String,
    /// The process group, which is the whole tree the server spawned
    group: i32,
    log: Log,
    since: u64,
    exit: Exit,
}

/// A one-off piece of work with output: creating a project, pinning tools
pub struct Job {
    pub key: String,
    pub title: String,
    pub log: Log,
    pub exit: Exit,
}

pub struct Hub {
    pub dev: PathBuf,
    me: PathBuf,
    pub mise: Option<PathBuf>,
    managed: Mutex<HashMap<u16, Managed>>,
    pub jobs: Mutex<Vec<Job>>,
    cache: Mutex<Option<(Instant, Vec<App>)>>,
}

fn signal(pid: i32, sig: i32) {
    // SAFETY: kill(2) only sends a signal; a pid that has gone is reported as an error, which is ignored
    unsafe {
        libc::kill(pid, sig);
    }
}

fn role_of(part: Option<&str>) -> Role {
    match part {
        None | Some("web" | "website" | "site" | "www" | "landing") => Role::Website,
        Some("admin") => Role::Admin,
        Some("coach" | "pro") => Role::Pro,
        Some(_) => Role::Other,
    }
}

/// Removes terminal colour and cursor codes, so output reads as plain text on the page
fn plain(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                for c in chars.by_ref() {
                    if c == '\u{7}' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn follow<R: AsyncRead + Unpin + Send + 'static>(reader: R, log: Log) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            // A progress bar redraws one line with carriage returns: keep what it ended on
            let line = plain(
                line.rsplit('\r')
                    .find(|part| !part.trim().is_empty())
                    .unwrap_or(""),
            );
            if !line.trim().is_empty() {
                say(&log, line);
            }
        }
    });
}

/// lsof's -F output: one field per line, each starting with a letter that names it
fn fields(out: &str) -> Vec<(i32, String, Vec<String>)> {
    let mut procs: Vec<(i32, String, Vec<String>)> = Vec::new();
    for line in out.lines() {
        let Some(key) = line.chars().next() else {
            continue;
        };
        let value = &line[1..];
        match key {
            'p' => procs.push((value.parse().unwrap_or(0), String::new(), Vec::new())),
            'c' => {
                if let Some(p) = procs.last_mut() {
                    p.1 = value.to_string();
                }
            }
            'n' => {
                if let Some(p) = procs.last_mut() {
                    p.2.push(value.to_string());
                }
            }
            _ => {}
        }
    }
    procs
}

/// lsof exits 1 when nothing matches, which is an answer, not a failure: take whatever it printed
async fn lsof(args: &[&str]) -> String {
    match Command::new("lsof")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .await
    {
        Ok(out) => String::from_utf8_lossy(&out.stdout).into_owned(),
        Err(_) => String::new(),
    }
}

fn csv(pids: &[i32]) -> String {
    pids.iter()
        .map(i32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn folders(dir: &Path) -> Vec<PathBuf> {
    const SKIP: [&str; 8] = [
        "node_modules",
        "dist",
        "build",
        "out",
        "target",
        "vendor",
        ".next",
        ".git",
    ];
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| !n.starts_with('.') && !SKIP.contains(&n))
        })
        .map(|e| e.path())
        .collect();
    found.sort();
    found
}

impl Hub {
    pub fn new() -> Self {
        let home = PathBuf::from(env::var("HOME").unwrap_or_else(|_| "/".into()));
        // mise is looked for on PATH, then where its installer puts it
        let mise = env::var_os("PATH")
            .into_iter()
            .flat_map(|p| env::split_paths(&p).collect::<Vec<_>>())
            .map(|d| d.join("mise"))
            .chain([
                home.join(".local/bin/mise"),
                PathBuf::from("/opt/homebrew/bin/mise"),
            ])
            .find(|p| p.is_file());
        Hub {
            dev: home.join("dev"),
            // Where Carbon's own source lives, so it never lists itself as something to run
            me: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            mise,
            managed: Mutex::new(HashMap::new()),
            jobs: Mutex::new(Vec::new()),
            cache: Mutex::new(None),
        }
    }

    /// A command line run in `dir` the way a person would run it there, with the tools mise gives that folder.
    /// `tools` are extra tool specs for mise (e.g. "deno@latest") when the folder doesn't name its own.
    pub fn command(&self, dir: &Path, tools: &[String], line: &str) -> Command {
        let mut cmd = match &self.mise {
            Some(mise) => {
                let mut cmd = Command::new(mise);
                cmd.arg("exec")
                    .args(tools)
                    .arg("--")
                    .args(["sh", "-c", line]);
                cmd
            }
            None => {
                let mut cmd = Command::new("sh");
                cmd.args(["-c", line]);
                cmd
            }
        };
        // A person's environment, not an agent's: started from an agent's shell, Carbon inherits variables that
        // make some tools change behaviour (Next stops to print advice for the agent, for one)
        const AGENT: [&str; 8] = [
            "CLAUDE",
            "AI_AGENT",
            "ANTHROPIC",
            "CODEX",
            "CURSOR",
            "GEMINI_CLI",
            "OPENCODE",
            "REPL_ID",
        ];
        for (key, _) in env::vars_os() {
            if key
                .to_str()
                .is_some_and(|k| AGENT.iter().any(|a| k.starts_with(a)))
            {
                cmd.env_remove(key);
            }
        }
        let mut path = vec![dir.join("node_modules/.bin")];
        path.extend(env::var_os("PATH").iter().flat_map(env::split_paths));
        // Started from the Dock, an app gets a bare PATH: add the places developer tools live
        let home = PathBuf::from(env::var("HOME").unwrap_or_default());
        for extra in [
            home.join(".cargo/bin"),
            home.join(".local/bin"),
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ] {
            if extra.is_dir() && !path.contains(&extra) {
                path.push(extra);
            }
        }
        cmd.current_dir(dir)
            .env("PATH", env::join_paths(path).unwrap_or_default())
            // Everything under ~/dev is ours, so its mise files are trusted without a prompt
            .env("MISE_TRUSTED_CONFIG_PATHS", &self.dev)
            .env("MISE_YES", "1")
            .env("BROWSER", "none")
            .env("FORCE_COLOR", "0")
            .env("NO_COLOR", "1");
        cmd
    }

    /// Starts a command in its own process group, collecting its output. Returns the group's id.
    pub fn spawn(&self, mut cmd: Command, log: Log, exit: Exit) -> io::Result<i32> {
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()?;
        let group = child.id().map(|id| id as i32).unwrap_or(0);
        if let Some(out) = child.stdout.take() {
            follow(out, log.clone());
        }
        if let Some(err) = child.stderr.take() {
            follow(err, log.clone());
        }
        tokio::spawn(async move {
            let code = child.wait().await.ok().and_then(|s| s.code());
            *exit.lock().unwrap() = Some(code.unwrap_or(-1));
            say(
                &log,
                match code {
                    Some(c) if c != 0 => format!("Stopped with code {c}."),
                    _ => "Stopped.".to_string(),
                },
            );
        });
        Ok(group)
    }

    pub fn forget(&self) {
        *self.cache.lock().unwrap() = None;
    }

    /// Every folder under ~/dev (group/project, and one level inside) that Carbon knows how to run
    fn discover(&self) -> Vec<App> {
        if let Some((at, apps)) = self.cache.lock().unwrap().as_ref() {
            if at.elapsed() < Duration::from_secs(5) {
                return apps.clone();
            }
        }
        let projects: Vec<PathBuf> = folders(&self.dev).iter().flat_map(|g| folders(g)).collect();
        let mut dirs = projects.clone();
        dirs.extend(projects.iter().flat_map(|p| folders(p)));
        let mut apps = Vec::new();
        for dir in dirs {
            // Carbon doesn't list itself, or the page it is made of
            if dir.starts_with(&self.me) {
                continue;
            }
            let Some(plan) = detect::detect(&dir) else {
                continue;
            };
            let Ok(rel) = dir.strip_prefix(&self.dev) else {
                continue;
            };
            let parts: Vec<&str> = rel.iter().filter_map(|p| p.to_str()).collect();
            let part = parts.get(2).map(|p| p.to_string());
            apps.push(App {
                id: parts.join("/"),
                group: parts.first().unwrap_or(&"").to_string(),
                project: parts.get(1).unwrap_or(&"").to_string(),
                role: role_of(part.as_deref()),
                part,
                kind: plan.kind,
                runtime: plan.runtime,
                pinned: detect::pinned(&dir),
                by_env: plan.by_env(),
                plan,
                dir,
                port: 0,
                command: String::new(),
            });
        }
        apps.sort_by(|a, b| a.id.cmp(&b.id));
        *self.cache.lock().unwrap() = Some((Instant::now(), apps.clone()));
        apps
    }

    /// Every TCP port something on this machine is listening on, with the folder that process runs in
    async fn listening(&self, apps: &[App]) -> Vec<Listener> {
        let procs = fields(&lsof(&["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpcn"]).await);
        if procs.is_empty() {
            return Vec::new();
        }
        let pids: Vec<i32> = procs.iter().map(|p| p.0).collect();
        let cwds: HashMap<i32, String> =
            fields(&lsof(&["-a", "-p", &csv(&pids), "-d", "cwd", "-Fpn"]).await)
                .into_iter()
                .filter_map(|(pid, _, names)| Some((pid, names.into_iter().next()?)))
                .collect();
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for (pid, command, names) in procs {
            for name in names {
                let Some(port) = name.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()) else {
                    continue;
                };
                if !seen.insert((pid, port)) {
                    continue;
                }
                let cwd = cwds.get(&pid).cloned();
                // The app it belongs to is the deepest known app folder it runs inside
                let app = cwd.as_deref().and_then(|cwd| {
                    apps.iter()
                        .filter(|a| Path::new(cwd).starts_with(&a.dir))
                        .max_by_key(|a| a.dir.as_os_str().len())
                        .map(|a| a.id.clone())
                });
                out.push(Listener {
                    port,
                    pid,
                    command: command.clone(),
                    cwd,
                    app,
                });
            }
        }
        out.sort_by_key(|l| l.port);
        out
    }

    async fn holders(&self, port: u16) -> Vec<Listener> {
        self.listening(&[])
            .await
            .into_iter()
            .filter(|l| l.port == port)
            .collect()
    }

    /// A dev server is a small family: the command that was typed, the server it spawned, its helpers. Stopping
    /// only the process that holds the port leaves the rest running, or lets its parent start it again. So from
    /// that process, climb to the highest ancestor that is still a program running inside ~/dev (never the shell,
    /// terminal or agent it was started from), and return it with everything beneath it.
    async fn family(&self, pid: i32) -> Vec<i32> {
        const NOT_OURS: [&str; 13] = [
            "zsh", "bash", "sh", "fish", "dash", "login", "tmux", "screen", "launchd", "claude",
            "codex", "gemini", "grok",
        ];
        let me = std::process::id() as i32;
        let table = agents::processes().await;
        let mut chain = vec![pid];
        while let Some(parent) = table.get(chain.last().unwrap()).map(|p| p.parent) {
            if parent <= 1 || chain.len() >= 6 {
                break;
            }
            chain.push(parent);
        }
        let cwds: HashMap<i32, String> =
            fields(&lsof(&["-a", "-p", &csv(&chain), "-d", "cwd", "-Fpn"]).await)
                .into_iter()
                .filter_map(|(pid, _, names)| Some((pid, names.into_iter().next()?)))
                .collect();
        let mut top = pid;
        for &p in &chain[1..] {
            let proc = table.get(&p);
            let name = proc.map(Proc::name).unwrap_or("");
            // Never climb into an agent, whatever it is called on disk
            let agent = proc.is_some_and(|p| agents::agent_of(p).is_some());
            let inside = cwds
                .get(&p)
                .is_some_and(|c| Path::new(c).starts_with(&self.dev) && Path::new(c) != self.dev);
            if p == me || agent || NOT_OURS.contains(&name) || !inside {
                break;
            }
            top = p;
        }
        let mut all = vec![top];
        let mut i = 0;
        while i < all.len() {
            let parent = all[i];
            all.extend(
                table
                    .iter()
                    .filter(|(child, proc)| proc.parent == parent && **child != me)
                    .map(|(child, _)| *child),
            );
            i += 1;
        }
        all
    }

    /// Frees a port: stops what Carbon started there, then anything else still holding it
    pub async fn stop(&self, port: u16) -> Result<(), String> {
        if port == HUB_PORT {
            return Err("3000 is Carbon itself".into());
        }
        let mine = self.managed.lock().unwrap().remove(&port).map(|m| m.group);
        if let Some(group) = mine {
            signal(-group, libc::SIGTERM);
        }
        let me = std::process::id() as i32;
        for sig in [libc::SIGTERM, libc::SIGKILL] {
            for holder in self.holders(port).await {
                if holder.pid != me {
                    for pid in self.family(holder.pid).await {
                        signal(pid, sig);
                    }
                }
            }
            for _ in 0..15 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if self.holders(port).await.is_empty() {
                    return Ok(());
                }
            }
            if let Some(group) = mine {
                signal(-group, libc::SIGKILL);
            }
        }
        Ok(())
    }

    /// The port an app runs on: its role's, or for anything else the pool port it already has, or the next free
    fn port_for(&self, app: &App, live: &[Listener]) -> u16 {
        if let Some((port, _, _)) = FIXED.iter().find(|f| f.1 == app.role) {
            return *port;
        }
        let managed = self.managed.lock().unwrap();
        if let Some(port) = POOL
            .clone()
            .find(|p| managed.get(p).is_some_and(|m| m.app == app.id))
        {
            return port;
        }
        if let Some(l) = live
            .iter()
            .find(|l| POOL.contains(&l.port) && l.app.as_deref() == Some(&app.id))
        {
            return l.port;
        }
        POOL.clone()
            .find(|p| !managed.contains_key(p) && !live.iter().any(|l| l.port == *p))
            .unwrap_or(*POOL.start())
    }

    /// Starts an app on its port, taking the port from whatever had it
    pub async fn start(&self, id: &str) -> Result<u16, String> {
        let apps = self.discover();
        let app = apps
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| format!("No app at {id}"))?;
        let port = self.port_for(app, &self.listening(&apps).await);
        self.stop(port).await?;

        let mut line = app.plan.command(port);
        // A project that has never been installed gets its dependencies first, so Run works on a fresh checkout
        if let Some((marker, install)) = &app.plan.install {
            if !app.dir.join(marker).exists() {
                line = format!("{install} && {line}");
            }
        }
        // A runtime the machine doesn't have yet is fetched by mise, unless the project names its own versions
        let tools: Vec<String> = match app.runtime {
            Some("deno") if !app.pinned => vec!["deno@latest".into()],
            _ => Vec::new(),
        };
        let mut cmd = self.command(&app.dir, &tools, &line);
        cmd.env("PORT", port.to_string())
            .env("ADDRESS", format!(":{port}"))
            .env("ADDR", format!(":{port}"));

        let log: Log = Arc::default();
        let exit: Exit = Arc::default();
        say(&log, format!("$ {line}"));
        let group = self
            .spawn(cmd, log.clone(), exit.clone())
            .map_err(|e| format!("Could not start {id}: {e}"))?;
        self.managed.lock().unwrap().insert(
            port,
            Managed {
                app: app.id.clone(),
                group,
                log,
                since: now(),
                exit,
            },
        );
        Ok(port)
    }

    /// Starts every app in a project, each on its own port
    pub async fn start_project(&self, group: &str, project: &str) -> Result<Vec<u16>, String> {
        let ids: Vec<String> = self
            .discover()
            .into_iter()
            .filter(|a| a.group == group && a.project == project)
            .map(|a| a.id)
            .collect();
        if ids.is_empty() {
            return Err(format!("No apps in {group}/{project}"));
        }
        let mut ports = Vec::new();
        for id in ids {
            ports.push(self.start(&id).await?);
        }
        Ok(ports)
    }

    /// Stops one server outside the slots. Only ever a process that is listening from inside ~/dev
    pub async fn kill(&self, pid: i32) -> Result<(), String> {
        let me = std::process::id() as i32;
        // Something an agent started: stop it and everything it spawned
        let found = agents::find(&agents::processes().await, me);
        if let Some((_, _, tree)) = found.started.iter().find(|s| s.0 == pid) {
            for &pid in tree {
                signal(pid, libc::SIGTERM);
            }
            return Ok(());
        }
        let ours = self.listening(&[]).await.into_iter().any(|l| {
            l.pid == pid
                && l.pid != me
                && l.cwd
                    .as_deref()
                    .is_some_and(|c| Path::new(c).starts_with(&self.dev))
        });
        if !ours {
            return Err("Not something Carbon can stop".into());
        }
        for pid in self.family(pid).await {
            signal(pid, libc::SIGTERM);
        }
        Ok(())
    }

    /// Writes the project's tool version into its own mise.toml, so it runs the same everywhere
    pub fn pin(&self, id: &str) -> Result<String, String> {
        let app = self
            .discover()
            .into_iter()
            .find(|a| a.id == id)
            .ok_or_else(|| format!("No app at {id}"))?;
        let mise = self.mise.as_ref().ok_or("mise isn't installed")?;
        let tool = match app.runtime {
            Some("node") => "node@lts",
            Some(runtime) => runtime,
            None => return Err(format!("{id} doesn't use a runtime mise manages")),
        };
        let mut cmd = Command::new(mise);
        cmd.args(["use", "--pin", tool])
            .current_dir(&app.dir)
            .env("MISE_TRUSTED_CONFIG_PATHS", &self.dev)
            .env("MISE_YES", "1");
        let key = self.job(
            format!("pin:{id}"),
            format!("Pin {tool} for {id}"),
            cmd,
            &format!("$ mise use --pin {tool}"),
        )?;
        self.forget();
        Ok(key)
    }

    /// Runs a one-off command as a job whose output the page can show
    pub fn job(
        &self,
        key: String,
        title: String,
        cmd: Command,
        shown: &str,
    ) -> Result<String, String> {
        let log: Log = Arc::default();
        let exit: Exit = Arc::default();
        say(&log, shown);
        self.spawn(cmd, log.clone(), exit.clone())
            .map_err(|e| format!("Could not run it: {e}"))?;
        let mut jobs = self.jobs.lock().unwrap();
        jobs.retain(|j| j.key != key);
        jobs.push(Job {
            key: key.clone(),
            title,
            log,
            exit,
        });
        // Keep the last few only
        let extra = jobs.len().saturating_sub(6);
        jobs.drain(..extra);
        Ok(key)
    }

    /// The output of a slot (by port) or of a job (by key)
    pub fn logs(&self, key: &str) -> Vec<String> {
        let log = match key.parse::<u16>() {
            Ok(port) => self
                .managed
                .lock()
                .unwrap()
                .get(&port)
                .map(|m| m.log.clone()),
            Err(_) => self
                .jobs
                .lock()
                .unwrap()
                .iter()
                .find(|j| j.key == key)
                .map(|j| j.log.clone()),
        };
        log.map(|l| l.lock().unwrap().iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Everything the page shows, in one read
    pub async fn snapshot(&self) -> Snapshot {
        let mut apps = self.discover();
        let live = self.listening(&apps).await;
        let home = env::var("HOME").unwrap_or_default();
        let me = std::process::id() as i32;

        // The agents on this machine and what each has started, with the folder each thing runs in
        let table = agents::processes().await;
        let found = agents::find(&table, me);
        let asked: Vec<i32> = found
            .sessions
            .iter()
            .map(|s| s.0)
            .chain(found.started.iter().map(|s| s.0))
            .collect();
        let cwds: HashMap<i32, String> = if asked.is_empty() {
            HashMap::new()
        } else {
            fields(&lsof(&["-a", "-p", &csv(&asked), "-d", "cwd", "-Fpn"]).await)
                .into_iter()
                .filter_map(|(pid, _, names)| Some((pid, names.into_iter().next()?)))
                .collect()
        };
        let tidy = |text: &str| text.replace(&home, "~");
        let ago = |pid: i32| now().saturating_sub(table.get(&pid).map_or(0, |p| p.up) * 1000);
        // Every process in a tree an agent started, and which agent that was
        let mut by: HashMap<i32, &'static str> = HashMap::new();
        let home_dir = PathBuf::from(&home);
        let mut agents: Vec<Agent> = found
            .sessions
            .iter()
            .map(|&(pid, harness)| Agent {
                harness: harness.id(),
                name: harness.name(),
                vendor: harness.vendor(),
                pid,
                cwd: cwds.get(&pid).map(|c| tidy(c)),
                since: ago(pid),
                title: None,
                status: None,
                servers: Vec::new(),
            })
            .collect();
        for (pid, session, tree) in &found.started {
            let Some(agent) = agents.iter_mut().find(|a| a.pid == *session) else {
                continue;
            };
            // Carbon is about servers: only what is listening on a real port counts. A build, a search or a test
            // run that an agent kicked off is the agent's business, not something to open in a browser
            let mut ports: Vec<u16> = live
                .iter()
                .filter(|l| {
                    tree.contains(&l.pid) && l.port < 49152 && !(9229..=9239).contains(&l.port)
                })
                .map(|l| l.port)
                .collect();
            ports.sort();
            ports.dedup();
            if ports.is_empty() {
                continue;
            }
            for member in tree {
                by.insert(*member, agent.vendor);
            }
            let cwd = cwds.get(pid);
            let mut command = tidy(table.get(pid).map_or("", |p| p.args.as_str()));
            if command.chars().count() > 180 {
                command = command.chars().take(180).collect::<String>() + "…";
            }
            agent.servers.push(Started {
                pid: *pid,
                command,
                app: cwd.and_then(|cwd| {
                    apps.iter()
                        .filter(|a| Path::new(cwd).starts_with(&a.dir))
                        .max_by_key(|a| a.dir.as_os_str().len())
                        .map(|a| a.id.clone())
                }),
                cwd: cwd.map(|c| tidy(c)),
                ports,
                since: ago(*pid),
            });
        }
        // Only sessions that are serving something, each asked what it is called
        agents.retain(|a| !a.servers.is_empty());
        for agent in &mut agents {
            if let Some((_, harness)) = found.sessions.iter().find(|s| s.0 == agent.pid) {
                let session = harness.session(agent.pid, &home_dir);
                agent.title = session.title;
                agent.status = session.status;
            }
        }
        agents.sort_by_key(|a| a.since);

        let slot = |port: u16, role: Role, label: &'static str| -> Slot {
            let managed = self.managed.lock().unwrap();
            let mine = managed.get(&port);
            let here = live.iter().find(|l| l.port == port);
            let exited = mine.and_then(|m| *m.exit.lock().unwrap());
            let (status, app, holder) = match (here, mine) {
                (Some(l), _) => {
                    let app = l.app.clone().or_else(|| mine.map(|m| m.app.clone()));
                    let holder = app.is_none().then(|| match &l.cwd {
                        Some(cwd) => format!("{} in {}", l.command, cwd.replacen(&home, "~", 1)),
                        None => l.command.clone(),
                    });
                    ("running", app, holder)
                }
                (None, Some(m)) => (
                    if exited.is_some() {
                        "exited"
                    } else {
                        "starting"
                    },
                    Some(m.app.clone()),
                    None,
                ),
                (None, None) => ("idle", None, None),
            };
            // The last line a stopped server printed before "Stopped…" is nearly always the reason it stopped
            let note = (status == "exited")
                .then(|| {
                    mine.and_then(|m| {
                        m.log
                            .lock()
                            .unwrap()
                            .iter()
                            .rev()
                            .find(|l| !l.starts_with("Stopped"))
                            .cloned()
                    })
                })
                .flatten();
            Slot {
                port,
                role,
                label,
                status,
                app,
                holder,
                by: here.and_then(|l| by.get(&l.pid).copied()),
                pid: here.map(|l| l.pid),
                has_logs: mine.is_some(),
                note,
                since: mine.map(|m| m.since),
            }
        };

        let mut slots: Vec<Slot> = FIXED
            .iter()
            .map(|&(port, role, label)| slot(port, role, label))
            .collect();
        // The pool: every port in use, then the next free one, so there is always somewhere to see "Other" go
        let mut spare = true;
        for port in POOL {
            let s = slot(port, Role::Other, "Other");
            if s.status != "idle" {
                slots.push(s);
            } else if spare {
                spare = false;
                slots.push(s);
            }
        }

        for app in &mut apps {
            app.port = self.port_for(app, &live);
            app.command = app.plan.command(app.port);
        }

        // Elsewhere: dev servers under ~/dev that are not on a Carbon port (started by hand, or by an agent).
        // Ports the system hands out by itself (49152 and up) and debugger ports are a server's own plumbing
        let ours = |port: u16| {
            FIXED.iter().any(|f| f.0 == port) || POOL.contains(&port) || port == HUB_PORT
        };
        let plumbing = |port: u16| port >= 49152 || (9229..=9239).contains(&port);
        let others = live
            .iter()
            .filter(|l| !ours(l.port) && !plumbing(l.port) && l.pid != me)
            // What an agent started is listed under that agent, not here
            .filter(|l| !by.contains_key(&l.pid))
            .filter(|l| {
                l.cwd
                    .as_deref()
                    .is_some_and(|c| Path::new(c).starts_with(&self.dev))
            })
            .cloned()
            .collect();

        let groups = folders(&self.dev)
            .iter()
            .filter_map(|g| g.file_name()?.to_str().map(String::from))
            .collect();
        let jobs = self
            .jobs
            .lock()
            .unwrap()
            .iter()
            .map(|j| JobState {
                key: j.key.clone(),
                title: j.title.clone(),
                status: match *j.exit.lock().unwrap() {
                    None => "running",
                    Some(0) => "done",
                    Some(_) => "failed",
                },
            })
            .collect();
        Snapshot {
            slots,
            apps,
            others,
            agents,
            groups,
            jobs,
            mise: self.mise.is_some(),
        }
    }

    /// Carbon's servers go down with Carbon, so nothing is left running that it would no longer show as its own
    pub fn shutdown(&self) {
        for managed in self.managed.lock().unwrap().values() {
            signal(-managed.group, libc::SIGTERM);
        }
    }
}
