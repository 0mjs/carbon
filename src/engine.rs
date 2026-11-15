//! Carbon's engine: it finds the projects on this machine, starts and stops their dev servers, and asks the OS
//! what is listening.
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
use crate::identity;
use crate::places;
use crate::stack;

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
    /// Its folder, relative to home: "dev/work/thing/web"
    pub id: String,
    pub group: String,
    pub project: String,
    /// The folder inside the project, when the app isn't the project root
    pub part: Option<String>,
    #[serde(skip)]
    pub dir: PathBuf,
    #[serde(skip)]
    project_dir: PathBuf,
    /// Where its code stands: branch, last commit, uncommitted changes
    pub git: Option<Git>,
    pub role: Role,
    pub kind: &'static str,
    pub runtime: Option<&'static str>,
    /// What it is built with, for the logos beside its name
    pub stack: Vec<&'static stack::Tech>,
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

/// What git says about a folder
#[derive(Clone, Serialize)]
pub struct Git {
    pub branch: String,
    /// When the last commit was made, in milliseconds
    pub last: u64,
    /// The last commit's first line
    pub subject: String,
    /// Files changed or added since then
    pub changes: u32,
}

/// One place code lives, as setup shows it
pub struct Place {
    /// "~/dev"
    pub name: String,
    pub dir: PathBuf,
    /// Whether Carbon looks here
    pub on: bool,
    pub projects: Vec<Choice>,
}

/// One project in a place, as setup shows it
pub struct Choice {
    pub key: String,
    pub title: String,
    pub logo: Option<PathBuf>,
    /// "~/dev/work/thing"
    pub place: String,
    pub dir: PathBuf,
    /// Whether it is shown in the list
    pub on: bool,
}

/// A project: one or more apps that belong together
#[derive(Serialize)]
pub struct Project {
    /// "<group>/<name>"
    pub key: String,
    /// Its folder's name
    pub name: String,
    /// What to call it: the name it was given by hand, else the one its manifest gives, else its folder's
    pub title: String,
    /// A picture of it: the logo it was given by hand, else the icon it ships
    pub logo: Option<PathBuf>,
    /// Whether the title and the logo were given by hand
    pub renamed: bool,
    pub logo_chosen: bool,
    /// Kept at the top of the list
    pub pinned: bool,
    /// The folder it is in, relative to home
    pub group: String,
    /// What it says it is, from its manifest or README
    pub description: Option<String>,
    pub apps: Vec<String>,
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
    pub port: u16,
    pub role: Role,
    pub label: &'static str,
    /// idle: nothing there. starting: launched, not answering yet. running: answering. exited: it stopped by itself
    pub status: &'static str,
    pub app: Option<String>,
    /// What holds the port when it isn't one of the apps Carbon knows
    pub holder: Option<String>,
    pub pid: Option<i32>,
    /// Carbon started it, so there is output to show
    pub has_logs: bool,
    /// When it stopped by itself: the last thing it said, which is usually why
    pub note: Option<String>,
    pub since: Option<u64>,
    /// The harness of the agent that started what is on this port, if one did, by its id
    pub by: Option<&'static str>,
}

/// One server an agent started
#[derive(Serialize)]
pub struct Started {
    pub pid: i32,
    pub command: String,
    pub cwd: Option<String>,
    pub app: Option<String>,
    /// The ports its process tree is listening on
    pub ports: Vec<u16>,
    pub since: u64,
}

/// One agent session that has servers running, and those servers
#[derive(Serialize)]
pub struct Agent {
    /// The harness's key and name, e.g. "claude-code" and "Claude Code"
    pub harness: &'static str,
    pub name: &'static str,
    /// The app the session is running inside: an editor, a terminal, or an app made for running agents
    pub host: Option<agents::Host>,
    pub pid: i32,
    pub cwd: Option<String>,
    pub since: u64,
    /// What the session is called, when the harness says
    pub title: Option<String>,
    /// The model it last answered with, when the harness says
    pub model: Option<String>,
    /// "busy" or "idle", when the harness says
    pub status: Option<String>,
    pub servers: Vec<Started>,
}

#[derive(Serialize)]
pub struct JobState {
    pub key: String,
    pub title: String,
    /// running, done or failed
    pub status: &'static str,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub slots: Vec<Slot>,
    pub apps: Vec<App>,
    pub others: Vec<Listener>,
    /// The agent sessions that have servers running, with those servers
    pub agents: Vec<Agent>,
    /// The apps gathered into their projects
    pub projects: Vec<Project>,
    /// The folders a new project can go in, relative to home
    pub groups: Vec<String>,
    pub jobs: Vec<JobState>,
    pub mise: bool,
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

/// A picture of what a port is serving, and whether a new one is being taken
#[derive(Default)]
struct Preview {
    at: Option<Instant>,
    png: Option<Arc<Vec<u8>>>,
    busy: bool,
}

pub struct Hub {
    pub home: PathBuf,
    roots: Mutex<Option<(Instant, Vec<PathBuf>)>>,
    groups: Mutex<Vec<String>>,
    git: Mutex<HashMap<PathBuf, (Instant, Option<Git>)>>,
    found: Mutex<HashMap<PathBuf, (Instant, identity::Found)>>,
    previews: Mutex<HashMap<u16, Preview>>,
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

fn role_named(name: &str) -> Option<Role> {
    match name {
        "website" => Some(Role::Website),
        "admin" => Some(Role::Admin),
        "pro" => Some(Role::Pro),
        "other" => Some(Role::Other),
        _ => None,
    }
}

/// The role a folder's name suggests. A project's own folder is its website; anything unrecognised is "other".
fn role_of(part: Option<&str>) -> Role {
    match part.map(|p| p.rsplit('/').next().unwrap_or(p)) {
        None | Some("web" | "website" | "site" | "www" | "landing" | "frontend" | "client") => {
            Role::Website
        }
        Some("admin" | "backoffice") => Role::Admin,
        Some("pro" | "dashboard" | "console" | "portal") => Role::Pro,
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

/// The Chromium-family browsers a preview can be taken with, in order of preference
fn chromium() -> Option<PathBuf> {
    let home = PathBuf::from(env::var("HOME").unwrap_or_default());
    [
        "Google Chrome",
        "Chromium",
        "Brave Browser",
        "Microsoft Edge",
        "Helium",
        "Vivaldi",
    ]
    .iter()
    .flat_map(|name| {
        [PathBuf::from("/Applications"), home.join("Applications")]
            .map(|apps| apps.join(format!("{name}.app/Contents/MacOS/{name}")))
    })
    .find(|binary| binary.is_file())
}

/// Takes a picture of http://localhost:<port> with a browser run without a window. Half size, so it is light to
/// send and still sharp as a thumbnail.
async fn capture(port: u16) -> Option<Vec<u8>> {
    let browser = chromium()?;
    let scratch = env::temp_dir().join(format!("carbon-preview-{port}"));
    let shot = scratch.join("shot.png");
    let _ = fs::create_dir_all(&scratch);
    let mut cmd = Command::new(browser);
    cmd.args([
        "--headless=new",
        "--disable-gpu",
        "--hide-scrollbars",
        "--no-first-run",
        "--no-default-browser-check",
        "--mute-audio",
        // A picture needs no saved passwords: never ask the system's keychain for anything
        "--use-mock-keychain",
        "--password-store=basic",
    ])
    .args([
        "--window-size=1280,800",
        "--force-device-scale-factor=0.5",
        "--virtual-time-budget=6000",
    ])
    .arg(format!(
        "--user-data-dir={}",
        scratch.join("profile").display()
    ))
    .arg(format!("--screenshot={}", shot.display()))
    .arg(format!("http://localhost:{port}/"))
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .kill_on_drop(true);
    let finished = tokio::time::timeout(Duration::from_secs(25), cmd.status()).await;
    let png = finished
        .ok()
        .and_then(Result::ok)
        .and_then(|_| fs::read(&shot).ok());
    let _ = fs::remove_file(&shot);
    png
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
            home,
            roots: Mutex::new(None),
            groups: Mutex::new(Vec::new()),
            git: Mutex::new(HashMap::new()),
            found: Mutex::new(HashMap::new()),
            previews: Mutex::new(HashMap::new()),
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
            // The places code lives are ours, so their mise files are trusted without a prompt
            .env(
                "MISE_TRUSTED_CONFIG_PATHS",
                env::join_paths(self.roots()).unwrap_or_default(),
            )
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

    /// The name and logo a project ships, looked for again every few minutes: icons don't come and go
    fn found(&self, project: &Path) -> identity::Found {
        let mut found = self.found.lock().unwrap();
        match found.get(project) {
            Some((when, known)) if when.elapsed() < Duration::from_secs(300) => known.clone(),
            _ => {
                let fresh = identity::find(project);
                found.insert(project.to_path_buf(), (Instant::now(), fresh.clone()));
                fresh
            }
        }
    }

    /// Keeps a project at the top of the list, or lets it go back to its place
    pub fn pin_project(&self, key: &str, pinned: bool) -> Result<(), String> {
        identity::pin(&self.home, key, pinned)
    }

    /// One place to look, for setup: the projects in it that Carbon could run, each with whether it is shown
    pub fn look_in(&self, folder: &Path) -> Result<Place, String> {
        if !folder.is_dir() {
            return Err(format!("{} isn't a folder", folder.display()));
        }
        if !folder.starts_with(&self.home) || folder == self.home {
            return Err("Carbon looks in folders inside your home folder".into());
        }
        let config = places::config(&self.home);
        let chosen = identity::chosen(&self.home);
        let (spots, _) = places::survey(&self.home, &[folder.to_path_buf()]);
        let mut projects: Vec<Choice> = Vec::new();
        for spot in spots {
            let known = projects.iter().any(|p| p.dir == spot.project_dir);
            if known || spot.dir.starts_with(&self.me) || detect::detect(&spot.dir).is_none() {
                continue;
            }
            let key = if spot.group.is_empty() {
                spot.project.clone()
            } else {
                format!("{}/{}", spot.group, spot.project)
            };
            let found = self.found(&spot.project_dir);
            let given = chosen.get(&key);
            let logo = given
                .and_then(|g| g.logo.as_ref())
                .map(|file| identity::logos(&self.home).join(file))
                .filter(|path| path.is_file());
            projects.push(Choice {
                title: given
                    .and_then(|g| g.name.clone())
                    .or(found.name)
                    .unwrap_or_else(|| spot.project.clone()),
                logo: logo.or(found.logo),
                place: places::tidy(&self.home, &spot.project_dir),
                on: !config.hidden.contains(&spot.project_dir),
                key,
                dir: spot.project_dir,
            });
        }
        projects.sort_by_key(|p| p.title.to_lowercase());
        // A place is looked in unless places were chosen and this isn't one of them
        let on = config
            .roots
            .as_ref()
            .is_none_or(|roots| roots.iter().any(|root| root == folder));
        Ok(Place {
            name: places::tidy(&self.home, folder),
            dir: folder.to_path_buf(),
            on,
            projects,
        })
    }

    /// Everything setup offers: each place code could live (the ones Carbon finds by itself, and any chosen
    /// before), with its projects
    pub fn setup(&self) -> Vec<Place> {
        let config = places::config(&self.home);
        let mut folders = places::roots(&self.home, &places::Config::default());
        folders.extend(config.roots.into_iter().flatten());
        folders.sort();
        folders.dedup();
        folders
            .iter()
            .filter_map(|folder| self.look_in(folder).ok())
            .collect()
    }

    /// Whether setup has been through: the places to look were chosen rather than worked out
    pub fn is_set_up(&self) -> bool {
        places::config(&self.home).roots.is_some()
    }

    /// Keeps what setup chose: where to look, and which projects to leave out
    pub fn save_setup(&self, roots: &[PathBuf], hidden: &[PathBuf]) -> Result<(), String> {
        places::save(&self.home, roots, hidden)?;
        *self.roots.lock().unwrap() = None;
        self.forget();
        Ok(())
    }

    /// Gives a project a name; an empty one goes back to the name it has by itself
    pub fn rename(&self, key: &str, name: &str) -> Result<(), String> {
        identity::rename(&self.home, key, name)
    }

    /// Gives a project a logo from an image file; none goes back to the icon it ships
    pub fn set_logo(&self, key: &str, image: Option<&Path>) -> Result<(), String> {
        identity::set_logo(&self.home, key, image)
    }

    /// The places code lives on this machine. Worked out from the home folder (or read from the config) and kept
    /// for a minute, since folders like these don't come and go
    pub fn roots(&self) -> Vec<PathBuf> {
        if let Some((at, roots)) = self.roots.lock().unwrap().as_ref() {
            if at.elapsed() < Duration::from_secs(60) {
                return roots.clone();
            }
        }
        let roots = places::roots(&self.home, &places::config(&self.home));
        *self.roots.lock().unwrap() = Some((Instant::now(), roots.clone()));
        roots
    }

    /// True for a folder inside one of the places code lives
    pub fn within(&self, path: &Path) -> bool {
        self.roots()
            .iter()
            .any(|root| path.starts_with(root) && path != root)
    }

    /// The folders a new project can be created in, relative to home
    pub fn groups(&self) -> Vec<String> {
        self.discover();
        self.groups.lock().unwrap().clone()
    }

    /// Opens an address in the browser the config names, or the system's default
    pub fn open(&self, url: &str) {
        let named = places::config(&self.home).browser.is_some_and(|browser| {
            std::process::Command::new("open")
                .args(["-a", &browser, url])
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        });
        if !named {
            let _ = std::process::Command::new("open").arg(url).status();
        }
    }

    /// Every folder in the places code lives that Carbon knows how to run
    fn discover(&self) -> Vec<App> {
        if let Some((at, apps)) = self.cache.lock().unwrap().as_ref() {
            if at.elapsed() < Duration::from_secs(5) {
                return apps.clone();
            }
        }
        let config = places::config(&self.home);
        let (spots, groups) = places::survey(&self.home, &self.roots());
        let mut apps = Vec::new();
        for spot in spots {
            // Carbon doesn't list itself, or the page it is made of, or a project that was left out in setup
            if spot.dir.starts_with(&self.me) || config.hidden.contains(&spot.project_dir) {
                continue;
            }
            let Some(plan) = detect::detect(&spot.dir) else {
                continue;
            };
            let id = spot
                .dir
                .strip_prefix(&self.home)
                .unwrap_or(&spot.dir)
                .iter()
                .filter_map(|p| p.to_str())
                .collect::<Vec<_>>()
                .join("/");
            // A role given by hand in the config wins over what the folder's name suggests
            let role = config
                .roles
                .get(&spot.dir)
                .and_then(|r| role_named(r))
                .unwrap_or_else(|| role_of(spot.part.as_deref()));
            apps.push(App {
                id,
                group: spot.group,
                project: spot.project,
                role,
                part: spot.part,
                kind: plan.kind,
                runtime: plan.runtime,
                stack: stack::of(&spot.dir, plan.kind, plan.runtime),
                pinned: detect::pinned(&spot.dir),
                by_env: plan.by_env(),
                plan,
                dir: spot.dir,
                project_dir: spot.project_dir,
                git: None,
                port: 0,
                command: String::new(),
            });
        }
        apps.sort_by(|a, b| a.id.cmp(&b.id));
        *self.groups.lock().unwrap() = groups;
        *self.cache.lock().unwrap() = Some((Instant::now(), apps.clone()));
        apps
    }

    /// What git says about a folder, asked at most every half minute
    async fn git(&self, dir: &Path) -> Option<Git> {
        if let Some((at, git)) = self.git.lock().unwrap().get(dir) {
            if at.elapsed() < Duration::from_secs(30) {
                return git.clone();
            }
        }
        let run = async |args: &[&str]| {
            let out = Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(args)
                .stderr(Stdio::null())
                .output()
                .await
                .ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
        };
        let git = async {
            let log = run(&["log", "-1", "--format=%ct%x09%s"]).await?;
            let (when, subject) = log.trim().split_once('\t')?;
            let status = run(&["status", "--porcelain=v2", "--branch"]).await?;
            let branch = status
                .lines()
                .find_map(|l| l.strip_prefix("# branch.head "))
                .unwrap_or("")
                .to_string();
            let changes = status.lines().filter(|l| !l.starts_with('#')).count() as u32;
            Some(Git {
                branch,
                last: when.parse::<u64>().ok()? * 1000,
                subject: subject.chars().take(120).collect(),
                changes,
            })
        }
        .await;
        self.git
            .lock()
            .unwrap()
            .insert(dir.to_path_buf(), (Instant::now(), git.clone()));
        git
    }

    /// A picture of what a port is serving, for the page to show. The picture is taken in the background by a
    /// Chromium-family browser run without a window, and kept for twenty seconds; asking again just returns the
    /// last one while a new one is taken. None until the first is ready, or when no such browser is installed.
    pub fn preview(self: &Arc<Self>, port: u16) -> Option<Arc<Vec<u8>>> {
        let mut previews = self.previews.lock().unwrap();
        let entry = previews.entry(port).or_default();
        let stale = entry
            .at
            .is_none_or(|at| at.elapsed() > Duration::from_secs(20));
        if stale && !entry.busy {
            entry.busy = true;
            let hub = self.clone();
            tokio::spawn(async move {
                let png = capture(port).await;
                let mut previews = hub.previews.lock().unwrap();
                let entry = previews.entry(port).or_default();
                entry.busy = false;
                entry.at = Some(Instant::now());
                if let Some(png) = png {
                    entry.png = Some(Arc::new(png));
                }
            });
        }
        entry.png.clone()
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
    /// that process, climb to the highest ancestor that is still a program running where code lives (never the shell,
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
            let inside = cwds.get(&p).is_some_and(|c| self.within(Path::new(c)));
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

    /// Stops one server outside the slots. Only ever a process listening from inside one of the places code lives
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
                && l.cwd.as_deref().is_some_and(|c| self.within(Path::new(c)))
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
            .env(
                "MISE_TRUSTED_CONFIG_PATHS",
                env::join_paths(self.roots()).unwrap_or_default(),
            )
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
                harness: harness.id,
                name: harness.name,
                host: None,
                pid,
                cwd: cwds.get(&pid).map(|c| tidy(c)),
                since: ago(pid),
                title: None,
                model: None,
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
                by.insert(*member, agent.harness);
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
        // The app each session is running inside. A program the process table knows only by a bare name is
        // asked for by its real path, since that is what says which app it belongs to
        let bare: Vec<i32> = agents
            .iter()
            .flat_map(|a| agents::lineage(&table, a.pid))
            .filter(|pid| {
                table.get(pid).is_some_and(|p| {
                    !p.program.starts_with('/') && !agents::SHELLS.contains(&p.name())
                })
            })
            .collect();
        let paths: HashMap<i32, String> = if bare.is_empty() {
            HashMap::new()
        } else {
            fields(&lsof(&["-a", "-p", &csv(&bare), "-d", "txt", "-Fpn"]).await)
                .into_iter()
                .filter_map(|(pid, _, names)| Some((pid, names.into_iter().next()?)))
                .collect()
        };
        for agent in &mut agents {
            agent.host = agents::host(&table, agent.pid, &paths);
        }
        for agent in &mut agents {
            if let Some((_, harness)) = found.sessions.iter().find(|s| s.0 == agent.pid) {
                let session = harness.session(agent.pid, &home_dir);
                agent.title = session.title;
                agent.model = session.model;
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
            app.git = self.git(&app.dir).await;
        }
        // Apps gathered into their projects, each project saying what it is
        let chosen = identity::chosen(&self.home);
        let logos = identity::logos(&self.home);
        let mut projects: Vec<Project> = Vec::new();
        for app in &apps {
            let key = if app.group.is_empty() {
                app.project.clone()
            } else {
                format!("{}/{}", app.group, app.project)
            };
            match projects.iter_mut().find(|p| p.key == key) {
                Some(project) => project.apps.push(app.id.clone()),
                None => {
                    // Only what the project itself says: a part's README describes the part, not the whole
                    let places = [app.project_dir.as_path()];
                    let found = self.found(&app.project_dir);
                    let given = chosen.get(&key);
                    let name = given.and_then(|g| g.name.clone());
                    let logo = given
                        .and_then(|g| g.logo.as_ref())
                        .map(|file| logos.join(file))
                        .filter(|path| path.is_file());
                    projects.push(Project {
                        pinned: given.is_some_and(|g| g.pinned),
                        renamed: name.is_some(),
                        logo_chosen: logo.is_some(),
                        title: name.or(found.name).unwrap_or_else(|| app.project.clone()),
                        logo: logo.or(found.logo),
                        key,
                        name: app.project.clone(),
                        group: app.group.clone(),
                        description: places::describe(&places),
                        apps: vec![app.id.clone()],
                    });
                }
            }
        }

        // Elsewhere: dev servers in the places code lives that are not on a Carbon port (started by hand, or by an agent).
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
            .filter(|l| l.cwd.as_deref().is_some_and(|c| self.within(Path::new(c))))
            .cloned()
            .collect();

        let groups = self.groups.lock().unwrap().clone();
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
            projects,
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
