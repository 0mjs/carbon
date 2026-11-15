//! Carbon as a desktop app, drawn natively with Rune (no web view), so the same app can run on macOS, Windows
//! and Linux.
//!
//! The app is the hub: it runs the same engine and server as `carbon serve`, inside its own process, so the
//! `carbon` command and the page at localhost:3000 keep working beside it. Closing the window leaves Carbon and
//! every server running (click the Dock icon to bring the window back); quitting stops Carbon and the servers it
//! started.
//!
//! There are two things to look at: what is running, and one project. The list on the left chooses between
//! them. `State` holds the latest snapshot from the engine, `poll` asks for a new one every second and a half on
//! a worker thread, and the views draw whatever the snapshot says. Nothing is drawn from memory of what should
//! be running.

mod look;
mod model;
mod pages;
mod view;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Duration,
};

use carbon::{
    client,
    engine::{Hub, Snapshot},
    server,
};
use rune::{AppSpec, Image, ImageCache, Key, KeyEvent, Latest, TextEdit, Titlebar, Tx, Waker};

/// The engine, and the runtime its async work runs on. Set once at start-up, before the window opens.
static HUB: OnceLock<Arc<Hub>> = OnceLock::new();
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

pub fn hub() -> &'static Arc<Hub> {
    HUB.get().expect("the hub is set before the window opens")
}

pub fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME
        .get()
        .expect("the runtime is set before the window opens")
}

/// The band at the top of the window that moves it, where the window's own buttons sit
pub const TITLEBAR: f32 = 52.0;

/// What the window is showing
#[derive(Clone, Default, PartialEq)]
pub enum Page {
    /// Every server on this machine
    #[default]
    Running,
    /// One project, by its key
    Project(String),
}

#[derive(Default)]
pub struct State {
    /// The last thing the engine said about the machine
    pub snap: Option<Arc<Snapshot>>,
    fresh: Latest<Arc<Snapshot>>,
    pub page: Page,
    /// What is typed in the list's search field
    pub search: TextEdit,
    /// Whether the project on show is having its name and logo changed, and the name as typed so far
    pub editing: bool,
    pub rename: TextEdit,
    /// What is mid-flight, e.g. "start:dev/thing/web" or "stop:3001", so its button can say so
    pub busy: Option<String>,
    pub error: Option<String>,
    done: Latest<Result<(), String>>,
    /// The port whose output is open, and that output
    pub output: Option<u16>,
    pub lines: Vec<String>,
    tail: Latest<Vec<String>>,
    /// Pictures of what each live port is serving: as they arrive from the engine, then decoded
    arrived: Vec<(u16, Arc<Vec<u8>>)>,
    pub shots: HashMap<u16, Image>,
    pictures: Latest<Vec<(u16, Arc<Vec<u8>>)>>,
    /// Each project's logo by its key, with the file it was read from so a new file is noticed
    pub logos: HashMap<String, (PathBuf, Image)>,
    images: Option<ImageCache>,
    /// Carbon's own mark and each agent's, by name
    pub marks: HashMap<&'static str, Image>,
    /// Set when the snapshot or the pictures changed and the images above need bringing up to date
    stale: bool,
    /// A picture of the window taken without opening one waits for its images, where a real window shows them
    /// as they arrive
    still: bool,
    /// When to ask the engine again, on the app's clock
    next: f64,
}

impl State {
    pub fn go(&mut self, page: Page) {
        if self.page != page {
            self.page = page;
            self.editing = false;
            self.output = None;
            self.lines.clear();
        }
    }

    /// Hands the engine some work off the UI thread and marks it as in flight
    fn act(&mut self, label: String, work: impl FnOnce() -> Result<(), String> + Send + 'static) {
        self.busy = Some(label);
        self.error = None;
        self.done.request((), move |()| work());
    }

    pub fn start(&mut self, id: String) {
        self.act(format!("start:{id}"), move || {
            runtime().block_on(hub().start(&id)).map(|_| ())
        });
    }

    pub fn start_project(&mut self, key: String, group: String, name: String) {
        self.act(format!("project:{key}"), move || {
            runtime()
                .block_on(hub().start_project(&group, &name))
                .map(|_| ())
        });
    }

    pub fn stop(&mut self, port: u16) {
        self.act(format!("stop:{port}"), move || {
            runtime().block_on(hub().stop(port))
        });
    }

    pub fn kill(&mut self, port: u16, pid: i32) {
        self.act(format!("stop:{port}"), move || {
            runtime().block_on(hub().kill(pid))
        });
    }

    pub fn show_output(&mut self, port: Option<u16>) {
        self.output = port;
        self.lines.clear();
        // Read it now rather than at the next tick
        self.next = 0.0;
    }

    /// Starts changing the name and logo of the project on show
    pub fn edit(&mut self, title: &str) {
        self.editing = true;
        self.rename = TextEdit::new(title);
        // The whole name selected, so typing replaces it
        self.rename.anchor = 0;
        self.rename.focus = title.len();
        self.rename.focused = true;
    }

    /// Keeps the name as typed. An empty one goes back to the name the project has by itself.
    pub fn finish(&mut self) {
        let Page::Project(key) = self.page.clone() else {
            return;
        };
        self.editing = false;
        let name = self.rename.text.trim().to_string();
        let same = self.snap.as_ref().is_some_and(|snap| {
            snap.projects
                .iter()
                .any(|p| p.key == key && p.title == name)
        });
        if !same {
            self.act("identity".into(), move || hub().rename(&key, &name));
        }
    }

    pub fn own_name(&mut self) {
        let Page::Project(key) = self.page.clone() else {
            return;
        };
        self.editing = false;
        self.act("identity".into(), move || hub().rename(&key, ""));
    }

    /// Gives the project on show a logo from an image file, or with none goes back to the icon it ships
    pub fn logo(&mut self, image: Option<PathBuf>) {
        let Page::Project(key) = self.page.clone() else {
            return;
        };
        self.act("identity".into(), move || {
            hub().set_logo(&key, image.as_deref())
        });
    }

    /// Brings the decoded images up to date with the snapshot and the pictures that have arrived
    fn images(&mut self) {
        self.stale = false;
        let (Some(images), Some(snap)) = (&self.images, &self.snap) else {
            return;
        };
        for (port, png) in self.arrived.drain(..) {
            // The cache knows an image it has already decoded, so an unchanged picture costs nothing
            self.shots
                .insert(port, images.load(png.as_slice().to_vec()));
        }
        let live = model::ports(snap);
        self.shots.retain(|port, _| live.contains(port));

        self.logos.retain(|key, (path, _)| {
            snap.projects
                .iter()
                .any(|p| &p.key == key && p.logo.as_ref() == Some(path))
        });
        for project in &snap.projects {
            if let (Some(path), false) = (&project.logo, self.logos.contains_key(&project.key)) {
                // Drawn marks are rastered at the largest size a logo is shown, on a dense screen
                self.logos.insert(
                    project.key.clone(),
                    (path.clone(), images.file(path, 72.0, 2.0)),
                );
            }
        }
        if self.still {
            for image in self
                .shots
                .values()
                .chain(self.logos.values().map(|(_, image)| image))
            {
                let _ = image.wait();
            }
        }
        // A project that has gone can't stay on show
        if let Page::Project(key) = &self.page {
            if !snap.projects.iter().any(|p| &p.key == key) {
                self.go(Page::Running);
            }
        }
    }
}

fn setup(s: &mut State, waker: Waker) {
    s.fresh.set_waker(waker.clone());
    s.done.set_waker(waker.clone());
    s.tail.set_waker(waker.clone());
    s.pictures.set_waker(waker.clone());
    let images = ImageCache::new(waker);
    for (name, svg) in [
        ("carbon", &include_bytes!("../assets/carbon.svg")[..]),
        ("claude", &include_bytes!("../assets/claude.svg")[..]),
        ("openai", &include_bytes!("../assets/openai.svg")[..]),
        ("gemini", &include_bytes!("../assets/gemini.svg")[..]),
        ("grok", &include_bytes!("../assets/grok.svg")[..]),
    ] {
        s.marks.insert(name, images.load(svg.to_vec()));
    }
    s.images = Some(images);
}

/// Runs before every frame: takes in whatever the workers have finished, and asks the engine again when it is
/// time. Returns true when something changed and the view should be rebuilt.
fn poll(s: &mut State, clock: f64, tx: &mut Tx) -> bool {
    let mut changed = false;

    if let Some(snap) = s.fresh.take() {
        // Ask for a picture of everything that is serving. The engine keeps them, so this is cheap to repeat
        s.pictures.request(model::ports(&snap), |ports| {
            // The engine starts a browser in the background to take a new one, which needs the runtime
            let _inside = runtime().enter();
            ports
                .into_iter()
                .filter_map(|port| hub().preview(port).map(|png| (port, png)))
                .collect()
        });
        s.snap = Some(snap);
        s.stale = true;
    }
    if let Some(pictures) = s.pictures.take() {
        s.arrived = pictures;
        s.stale = true;
    }
    if s.stale {
        s.images();
        changed = true;
    }

    if let Some(outcome) = s.done.take() {
        s.busy = None;
        s.error = outcome.err();
        s.next = 0.0;
        changed = true;
    }

    if let Some(lines) = s.tail.take() {
        if lines != s.lines {
            s.lines = lines;
            changed = true;
        }
    }

    if clock >= s.next {
        s.next = clock + 1.5;
        s.fresh
            .request((), |()| Arc::new(runtime().block_on(hub().snapshot())));
        if let Some(port) = s.output {
            s.tail.request(port.to_string(), |key| hub().logs(&key));
        }
    }
    // Keep the clock ticking while the window shows; a hidden window sleeps until this fires
    tx.wake_after(Duration::from_millis(500));
    changed
}

/// Keys nothing else took: ⌘K goes to the search field, and Escape backs out of whatever is open
fn on_key(s: &mut State, event: &KeyEvent, _: &mut Tx) -> bool {
    match event.key {
        Key::Command('k') => {
            s.search.focused = true;
            true
        }
        Key::Escape if s.editing => {
            s.editing = false;
            true
        }
        Key::Escape if !s.search.text.is_empty() => {
            s.search = TextEdit::default();
            true
        }
        _ => false,
    }
}

/* ---------- Scenes: the window as a picture, without opening one (`Carbon --snap out.png --scene now`) ---------- */

/// The machine as it is this moment, with a picture of everything serving, read before the first frame
fn now(s: &mut State) {
    let snap = Arc::new(runtime().block_on(hub().snapshot()));
    let _inside = runtime().enter();
    // The first ask starts a browser taking each picture; they are kept, so asking again collects them
    for _ in 0..40 {
        s.arrived = model::ports(&snap)
            .into_iter()
            .filter_map(|port| hub().preview(port).map(|png| (port, png)))
            .collect();
        if s.arrived.len() == model::ports(&snap).len() {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    s.snap = Some(snap);
    s.stale = true;
    s.still = true;
    // A still is of one moment: nothing newer is asked for while it is taken
    s.next = f64::MAX;
}

/// One project: the one named by CARBON_SCENE_PROJECT, else the one with the most apps
fn project(s: &mut State) {
    now(s);
    let Some(snap) = &s.snap else { return };
    let wanted = std::env::var("CARBON_SCENE_PROJECT").ok();
    let chosen = snap
        .projects
        .iter()
        .find(|p| Some(&p.key) == wanted.as_ref())
        .or_else(|| snap.projects.iter().max_by_key(|p| p.apps.len()));
    if let Some(project) = chosen {
        s.page = Page::Project(project.key.clone());
    }
}

/// The same project with its name and logo being changed
fn editing(s: &mut State) {
    project(s);
    let Page::Project(key) = &s.page else { return };
    let title = s
        .snap
        .as_ref()
        .and_then(|snap| snap.projects.iter().find(|p| &p.key == key))
        .map(|p| p.title.clone());
    if let Some(title) = title {
        s.edit(&title);
    }
}

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime for the engine");
    let hub = Arc::new(Hub::new());
    // A hub already answering on 3000 (a `carbon serve` in a terminal) keeps the port; this app then only shows
    // and controls the machine through its own engine
    let serving = !client::running();
    if serving {
        if let Err(why) = runtime.block_on(server::listen(hub.clone())) {
            eprintln!("{why}");
            std::process::exit(1);
        }
    }
    let _ = HUB.set(hub);
    let _ = RUNTIME.set(runtime);

    let spec = AppSpec::<State> {
        scenes: &[("now", now), ("project", project), ("editing", editing)],
        min_size: Some((860.0, 560.0)),
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(on_key),
        ..AppSpec::new("Carbon", (1180.0, 780.0), State::default, view::view)
    };
    rune::App::new(spec)
        // The window draws its own top edge, so the list runs to the top with the window's buttons over it
        .titlebar(Titlebar::new(TITLEBAR))
        // Cut, copy and paste for the text fields
        .standard_menus()
        // Closing the window is not quitting: Carbon and its servers carry on, and the Dock icon reopens it
        .keep_running()
        .run();

    // Quit: Carbon's servers go down with Carbon
    self::hub().shutdown();
}
