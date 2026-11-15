//! Carbon as a desktop app, drawn natively with Rune (no web view), so the same app can run on macOS, Windows
//! and Linux.
//!
//! The app is the hub: it runs the same engine and server as `carbon serve`, inside its own process, so the
//! `carbon` command and the page at localhost:3000 keep working beside it. Closing the window leaves Carbon and
//! every server running (click the Dock icon to bring the window back); quitting stops Carbon and the servers it
//! started.
//!
//! How it is put together: `State` holds the latest snapshot from the engine, `poll` asks for a new one every
//! second and a half on a worker thread, and `view` draws whatever the snapshot says. Nothing is drawn from
//! memory of what should be running.

mod view;

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::Duration,
};

use carbon::{
    client,
    engine::{Hub, Snapshot},
    server,
};
use rune::{AppSpec, Image, ImageCache, Latest, Tx, Waker};

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

/// What an action asked of the engine came back with: the port to show output for, or why it failed
type Outcome = Result<Option<u16>, String>;

#[derive(Default)]
pub struct State {
    /// The last thing the engine said about the machine
    pub snap: Option<Arc<Snapshot>>,
    fresh: Latest<Arc<Snapshot>>,
    /// What is mid-flight, e.g. "start:dev/thing/web" or "stop:3001", so its button can say so
    pub busy: Option<String>,
    pub error: Option<String>,
    done: Latest<Outcome>,
    /// Whose output the console shows (a port, or a job's key), and that output
    pub console: Option<String>,
    pub lines: Vec<String>,
    tail: Latest<Vec<String>>,
    /// Pictures of what each live port is serving
    pub shots: HashMap<u16, Image>,
    pictures: Latest<Vec<(u16, Arc<Vec<u8>>)>>,
    images: Option<ImageCache>,
    /// Carbon's own mark and each agent's, by name
    pub marks: HashMap<&'static str, Image>,
    /// When to ask the engine again, on the app's clock
    next: f64,
}

impl State {
    /// Hands the engine some work off the UI thread and marks it as in flight
    pub fn act(&mut self, label: String, work: impl FnOnce() -> Outcome + Send + 'static) {
        self.busy = Some(label);
        self.error = None;
        self.done.request((), move |()| work());
    }

    pub fn start(&mut self, id: String) {
        self.act(format!("start:{id}"), move || {
            runtime().block_on(hub().start(&id)).map(Some)
        });
    }

    pub fn start_project(&mut self, group: String, project: String) {
        self.act(format!("project:{group}/{project}"), move || {
            runtime()
                .block_on(hub().start_project(&group, &project))
                .map(|ports| ports.first().copied())
        });
    }

    pub fn stop(&mut self, port: u16) {
        self.act(format!("stop:{port}"), move || {
            runtime().block_on(hub().stop(port)).map(|()| None)
        });
    }

    pub fn kill(&mut self, pid: i32) {
        self.act(format!("kill:{pid}"), move || {
            runtime().block_on(hub().kill(pid)).map(|()| None)
        });
    }

    pub fn show_output(&mut self, key: Option<String>) {
        self.console = key;
        self.lines.clear();
        // Read it now rather than at the next tick
        self.next = 0.0;
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

/// Runs before every frame: takes in whatever the workers have finished, and asks the engine again when it is time.
/// Returns true when something changed and the view should be rebuilt.
fn poll(s: &mut State, clock: f64, tx: &mut Tx) -> bool {
    let mut changed = false;

    if let Some(snap) = s.fresh.take() {
        // Ask for a picture of everything that is serving. The engine keeps them, so this is cheap to repeat
        let mut ports: Vec<u16> = snap
            .slots
            .iter()
            .filter(|slot| slot.status == "running")
            .map(|slot| slot.port)
            .collect();
        ports.extend(snap.others.iter().map(|other| other.port));
        ports.extend(snap.agents.iter().flat_map(|agent| {
            agent
                .servers
                .iter()
                .filter_map(|server| server.ports.first().copied())
        }));
        s.pictures.request(ports, |ports| {
            // The engine starts a browser in the background to take a new one, which needs the runtime
            let _inside = runtime().enter();
            ports
                .into_iter()
                .filter_map(|port| hub().preview(port).map(|png| (port, png)))
                .collect()
        });
        s.snap = Some(snap);
        changed = true;
    }

    if let Some(pictures) = s.pictures.take() {
        if let Some(images) = &s.images {
            for (port, png) in pictures {
                // The cache knows an image it has already decoded, so an unchanged picture costs nothing
                s.shots.insert(port, images.load(png.as_ref().clone()));
            }
            changed = true;
        }
    }

    if let Some(outcome) = s.done.take() {
        s.busy = None;
        match outcome {
            Ok(Some(port)) => s.show_output(Some(port.to_string())),
            Ok(None) => {}
            Err(why) => s.error = Some(why),
        }
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
        if let Some(key) = s.console.clone() {
            s.tail.request(key, |key| hub().logs(&key));
        }
    }
    // Keep the clock ticking while the window shows; a hidden window sleeps until this fires
    tx.wake_after(Duration::from_millis(500));
    changed
}

/// The machine as it is this moment, read before the first frame
fn now(s: &mut State) {
    s.snap = Some(Arc::new(runtime().block_on(hub().snapshot())));
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
        // For looking at the window without opening one (`Carbon --snap out.png --scene now`): the machine as it
        // is this moment, read before the first frame rather than a tick later
        scenes: &[("now", now)],
        min_size: Some((760.0, 520.0)),
        setup: Some(setup),
        poll: Some(poll),
        ..AppSpec::new("Carbon", (1240.0, 860.0), State::default, view::view)
    };
    // Closing the window is not quitting: Carbon and its servers carry on, and the Dock icon reopens it
    rune::App::new(spec).keep_running().run();

    // Quit: Carbon's servers go down with Carbon
    self::hub().shutdown();
}
