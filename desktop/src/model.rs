//! One way of seeing what is serving. The engine reports servers in three places (on Carbon's ports, under the
//! agent that started them, and started by hand elsewhere); the window shows a server as a server, and says who
//! started it as a detail.

use std::time::{SystemTime, UNIX_EPOCH};

use carbon::engine::{Agent, App, Project, Snapshot};

/// Who started a server
pub enum Origin<'a> {
    Carbon,
    Agent(&'a Agent),
    Hand,
}

pub struct Server<'a> {
    pub port: u16,
    pub pid: Option<i32>,
    /// "running", "starting", or "exited" for one Carbon started that stopped by itself
    pub status: &'static str,
    pub app: Option<&'a App>,
    /// What to call it when it isn't one of the apps Carbon knows: its command and where it runs
    pub what: String,
    pub origin: Origin<'a>,
    pub since: Option<u64>,
    /// Whether Carbon holds its output
    pub logs: bool,
    /// Why it stopped, when it stopped by itself
    pub note: Option<&'a str>,
    /// Whether it is on one of Carbon's ports, where stopping is by port; elsewhere it is by process
    pub slot: bool,
}

impl Server<'_> {
    pub fn live(&self) -> bool {
        self.status == "running"
    }
}

/// Every server on the machine, once each, lowest port first
pub fn servers(snap: &Snapshot) -> Vec<Server<'_>> {
    let app = |id: &Option<String>| {
        id.as_ref()
            .and_then(|id| snap.apps.iter().find(|a| &a.id == id))
    };
    let mut all: Vec<Server> = Vec::new();

    for slot in snap.slots.iter().filter(|slot| slot.status != "idle") {
        let agent = slot.by.and_then(|_| {
            snap.agents
                .iter()
                .find(|a| a.servers.iter().any(|s| s.ports.contains(&slot.port)))
        });
        all.push(Server {
            port: slot.port,
            pid: slot.pid,
            status: slot.status,
            app: app(&slot.app),
            what: slot.holder.clone().unwrap_or_default(),
            origin: match agent {
                Some(agent) => Origin::Agent(agent),
                None if slot.has_logs => Origin::Carbon,
                None => Origin::Hand,
            },
            since: slot.since.or_else(|| {
                agent
                    .and_then(|a| a.servers.iter().find(|s| s.ports.contains(&slot.port)))
                    .map(|s| s.since)
            }),
            logs: slot.has_logs,
            note: slot.note.as_deref(),
            slot: true,
        });
    }
    for agent in &snap.agents {
        for server in &agent.servers {
            let Some(&port) = server.ports.first() else {
                continue;
            };
            if all.iter().any(|known| known.port == port) {
                continue;
            }
            all.push(Server {
                port,
                pid: Some(server.pid),
                status: "running",
                app: app(&server.app),
                what: server.cwd.clone().unwrap_or_else(|| server.command.clone()),
                origin: Origin::Agent(agent),
                since: Some(server.since),
                logs: false,
                note: None,
                slot: false,
            });
        }
    }
    for other in &snap.others {
        if all.iter().any(|known| known.port == other.port) {
            continue;
        }
        all.push(Server {
            port: other.port,
            pid: Some(other.pid),
            status: "running",
            app: app(&other.app),
            what: other.cwd.clone().unwrap_or_else(|| other.command.clone()),
            origin: Origin::Hand,
            since: None,
            logs: false,
            note: None,
            slot: false,
        });
    }
    all.sort_by_key(|server| server.port);
    all
}

/// The ports that are answering, which are the ones worth a picture
pub fn ports(snap: &Snapshot) -> Vec<u16> {
    servers(snap)
        .iter()
        .filter(|server| server.live())
        .map(|server| server.port)
        .collect()
}

pub fn project_of<'a>(snap: &'a Snapshot, app: &App) -> Option<&'a Project> {
    snap.projects
        .iter()
        .find(|project| project.apps.contains(&app.id))
}

/// A part of a project as a name: "Web" for the folder "web"
pub fn named(part: &str) -> String {
    let mut letters = part.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// A folder as a person would write it
pub fn folder(group: &str, name: &str) -> String {
    if group.is_empty() {
        format!("~/{name}")
    } else {
        format!("~/{group}/{name}")
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// How long something has been up, in the largest unit that fits
pub fn up(since: u64) -> String {
    let secs = now().saturating_sub(since) / 1000;
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} min", secs / 60),
        3600..86_400 => format!("{} h {} min", secs / 3600, (secs % 3600) / 60),
        _ => format!("{} days", secs / 86_400),
    }
}

/// How long ago something happened
pub fn ago(when: u64) -> String {
    let secs = now().saturating_sub(when) / 1000;
    match secs {
        0..3600 => format!("{} min ago", (secs / 60).max(1)),
        3600..86_400 => format!("{} h ago", secs / 3600),
        _ => format!("{} days ago", secs / 86_400),
    }
}
