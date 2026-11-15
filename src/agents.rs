//! Who started what: finding the coding agents running on this machine and everything they have launched.
//!
//! An agent (any harness in `harness.rs`) runs commands by handing them to a shell. So a process counts as
//! "started by an agent" when its parent is a shell and every ancestor between it and the agent is a shell too:
//! that is the command the agent ran, not the agent's own helpers, and not something that command went on to spawn
//! (those belong to it, and are stopped with it).

use std::{collections::HashMap, path::PathBuf};

use serde::Serialize;
use tokio::process::Command;

use crate::harness::{self, Harness};

pub struct Proc {
    pub parent: i32,
    /// Seconds it has been running
    pub up: u64,
    /// The program it is running, as a path where the system gives one. Kept apart from the command line because
    /// a path can have spaces in it ("/Applications/Some App.app/…"), and a command line can't say where they are
    pub program: String,
    /// Its whole command line
    pub args: String,
}

impl Proc {
    /// A process known only by its command line, taking its first word as its program
    pub fn of(parent: i32, up: u64, args: &str) -> Self {
        Proc {
            parent,
            up,
            program: args.split_whitespace().next().unwrap_or("").into(),
            args: args.into(),
        }
    }

    /// The program's own name: "zsh" for "/bin/zsh", and for a login shell's "-zsh"
    pub fn name(&self) -> &str {
        self.program
            .rsplit('/')
            .next()
            .unwrap_or("")
            .trim_start_matches('-')
    }

    /// What it was given to run: its command line after the program
    pub fn given(&self) -> &str {
        match self.args.strip_prefix(&self.program) {
            Some(rest) => rest,
            None => self
                .args
                .split_once(char::is_whitespace)
                .map_or("", |(_, rest)| rest),
        }
    }
}

/// The app an agent is running inside: an editor, a terminal, or one of the apps made for running agents
#[derive(Clone, Serialize)]
pub struct Host {
    /// What the app is called, e.g. "Zed"
    pub name: String,
    /// Where the app is, when it is one with an icon of its own
    pub app: Option<PathBuf>,
}

/// The app a process belongs to, if it runs from inside one (macOS: "/Applications/Zed.app/Contents/…")
fn bundle(program: &str) -> Option<Host> {
    let end = program.find(".app/")? + ".app".len();
    let app = PathBuf::from(&program[..end]);
    let name = app.file_stem()?.to_str()?.to_string();
    Some(Host {
        name,
        app: Some(app),
    })
}

/// A process and everything above it, nearest first
pub fn lineage(table: &HashMap<i32, Proc>, pid: i32) -> Vec<i32> {
    let mut line = Vec::new();
    let mut at = pid;
    while at > 1 && line.len() < 64 {
        let Some(proc) = table.get(&at) else { break };
        line.push(at);
        at = proc.parent;
    }
    line
}

/// The app a session is running inside: the nearest one above it, or the one its own program is part of.
/// `paths` are where programs really are, for the ones the process table knows only by a bare name (an app's
/// background helper started as "tern", say).
pub fn host(
    table: &HashMap<i32, Proc>,
    session: i32,
    paths: &HashMap<i32, String>,
) -> Option<Host> {
    for pid in lineage(table, session) {
        let proc = table.get(&pid)?;
        if let Some(host) = bundle(paths.get(&pid).unwrap_or(&proc.program)) {
            return Some(host);
        }
        // T3 Code also runs without its app, as a command that serves it to a browser
        if proc.name() == "t3" {
            return Some(Host {
                name: "T3 Code".into(),
                app: None,
            });
        }
    }
    None
}

pub const SHELLS: [&str; 6] = ["zsh", "bash", "sh", "fish", "dash", "ksh"];

/// ps prints elapsed time as [[days-]hours:]minutes:seconds
fn seconds(elapsed: &str) -> u64 {
    let (days, clock) = match elapsed.split_once('-') {
        Some((days, clock)) => (days.parse().unwrap_or(0), clock),
        None => (0, elapsed),
    };
    days * 86_400
        + clock.split(':').fold(0, |total, part| {
            total * 60 + part.parse::<u64>().unwrap_or(0)
        })
}

/// One column of `ps` for every process: its pid, then the rest of the line with its own spaces kept
async fn column(name: &str) -> Vec<(i32, String)> {
    let out = Command::new("ps")
        .args(["-axo", &format!("pid=,{name}=")])
        .output()
        .await;
    let text = String::from_utf8_lossy(&out.map(|o| o.stdout).unwrap_or_default()).into_owned();
    text.lines()
        .filter_map(|line| {
            let (pid, rest) = line.trim_start().split_once(char::is_whitespace)?;
            Some((pid.parse().ok()?, rest.trim().to_string()))
        })
        .collect()
}

/// Every process on the machine, by pid
pub async fn processes() -> HashMap<i32, Proc> {
    let out = Command::new("ps")
        .args(["-axo", "pid=,ppid=,etime=,args="])
        .output();
    // The program is asked for by itself: it is the one column besides the command line that can hold spaces
    let (out, programs) = tokio::join!(out, column("comm"));
    let programs: HashMap<i32, String> = programs.into_iter().collect();
    let text = String::from_utf8_lossy(&out.map(|o| o.stdout).unwrap_or_default()).into_owned();
    let mut table = HashMap::new();
    for line in text.lines() {
        // Three columns, then the command line with its own spaces kept
        let mut rest = line.trim_start();
        let mut column = || {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let (word, tail) = rest.split_at(end);
            rest = tail.trim_start();
            word
        };
        let (Ok(pid), Ok(parent)) = (column().parse::<i32>(), column().parse::<i32>()) else {
            continue;
        };
        let up = seconds(column());
        let proc = match programs.get(&pid) {
            Some(program) => Proc {
                parent,
                up,
                program: program.clone(),
                args: rest.to_string(),
            },
            None => Proc::of(parent, up, rest),
        };
        table.insert(pid, proc);
    }
    table
}

/// The harness a process is, if it is a coding agent
pub fn agent_of(proc: &Proc) -> Option<&'static Harness> {
    harness::of(proc)
}

pub struct Found {
    /// Each agent session: its pid and which harness it is
    pub sessions: Vec<(i32, &'static Harness)>,
    /// Each thing an agent started: its pid, the session's pid, and its whole process tree (itself first)
    pub started: Vec<(i32, i32, Vec<i32>)>,
}

/// Finds the agents and what they started. Nothing under `me` (Carbon itself) is counted: what Carbon runs is
/// Carbon's, whoever started Carbon.
pub fn find(table: &HashMap<i32, Proc>, me: i32) -> Found {
    let mut children: HashMap<i32, Vec<i32>> = HashMap::new();
    for (pid, proc) in table {
        children.entry(proc.parent).or_default().push(*pid);
    }
    let tree = |root: i32| {
        let mut all = vec![root];
        let mut i = 0;
        while i < all.len() {
            all.extend(children.get(&all[i]).into_iter().flatten().copied());
            i += 1;
        }
        all
    };

    let mut sessions = Vec::new();
    let mut started = Vec::new();
    for (&pid, proc) in table {
        // Walk up: note the nearest agent, the outermost agent (the session), and what lies in between
        let (mut nearest, mut session, mut direct, mut under_me) = (None, None, true, false);
        let (mut at, mut steps) = (proc.parent, 0);
        while at > 1 && steps < 64 {
            if at == me {
                under_me = true;
                break;
            }
            let Some(ancestor) = table.get(&at) else {
                break;
            };
            if let Some(kind) = agent_of(ancestor) {
                nearest.get_or_insert(at);
                session = Some((at, kind));
            } else if nearest.is_none() && !SHELLS.contains(&ancestor.name()) {
                direct = false;
            }
            at = ancestor.parent;
            steps += 1;
        }
        if under_me || pid == me {
            continue;
        }
        if let Some(kind) = agent_of(proc) {
            // An agent with no agent above it is a session of its own
            if session.is_none() {
                sessions.push((pid, kind));
            }
            continue;
        }
        let from_a_shell = table
            .get(&proc.parent)
            .is_some_and(|p| SHELLS.contains(&p.name()));
        if let Some((session, _)) = session {
            // Skip shells themselves, and anything too new to be more than a passing command
            if direct && from_a_shell && !SHELLS.contains(&proc.name()) && proc.up >= 2 {
                started.push((pid, session, tree(pid)));
            }
        }
    }
    sessions.sort_by_key(|s| s.0);
    started.sort_by_key(|s| s.0);
    Found { sessions, started }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(parent: i32, args: &str) -> Proc {
        Proc::of(parent, 60, args)
    }

    #[test]
    fn finds_the_app_a_session_runs_inside() {
        let app = |parent, program: &str| Proc {
            parent,
            up: 60,
            program: program.into(),
            args: program.into(),
        };
        let table = HashMap::from([
            (
                5,
                app(
                    1,
                    "/Applications/Some Editor.app/Contents/MacOS/Some Editor",
                ),
            ),
            (
                6,
                app(
                    5,
                    "/Applications/Some Editor.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper",
                ),
            ),
            (7, proc(6, "-zsh")),
            (8, proc(7, "claude")),
            (20, proc(1, "t3 serve")),
            (21, proc(20, "codex app-server")),
            (30, proc(1, "claude")),
        ]);
        let none = HashMap::new();
        let inside = host(&table, 8, &none).unwrap();
        assert_eq!(inside.name, "Some Editor");
        assert_eq!(
            inside.app,
            Some(PathBuf::from("/Applications/Some Editor.app"))
        );
        assert_eq!(
            host(&table, 21, &none).map(|h| h.name).as_deref(),
            Some("T3 Code")
        );
        assert!(host(&table, 30, &none).is_none());
        // Known only as "claude" in the table, but really inside an app
        let real = HashMap::from([(
            30,
            "/Applications/Desk.app/Contents/Resources/claude".to_string(),
        )]);
        assert_eq!(
            host(&table, 30, &real).map(|h| h.name).as_deref(),
            Some("Desk")
        );
    }

    #[test]
    fn reads_elapsed_time() {
        assert_eq!(seconds("01:45"), 105);
        assert_eq!(seconds("04:43:42"), 4 * 3600 + 43 * 60 + 42);
        assert_eq!(seconds("01-03:22:23"), 86_400 + 3 * 3600 + 22 * 60 + 23);
    }

    #[test]
    fn finds_what_an_agent_ran_and_nothing_else() {
        let table = HashMap::from([
            (10, proc(1, "claude")),
            (11, proc(10, "node /mcp/server.js")), // the agent's own helper: no shell in between
            (20, proc(10, "/bin/zsh -c bun run dev")),
            (21, proc(20, "bun run dev")), // what the agent ran
            (22, proc(21, "node vite")),   // what that went on to spawn: part of 21
            (30, proc(1, "-zsh")),
            (31, proc(30, "node vite")), // typed by a person
            (40, proc(10, "/bin/zsh -c ./carbon")),
            (41, proc(40, "./carbon")), // Carbon itself, started by the agent
            (42, proc(41, "mise exec -- sh -c vite")), // …and what Carbon runs is Carbon's
        ]);
        let found = find(&table, 41);
        assert_eq!(
            found
                .sessions
                .iter()
                .map(|s| (s.0, s.1.id))
                .collect::<Vec<_>>(),
            vec![(10, "claude-code")]
        );
        assert_eq!(found.started.len(), 1);
        assert_eq!(found.started[0].0, 21);
        assert_eq!(found.started[0].1, 10);
        assert_eq!(found.started[0].2, vec![21, 22]);
    }
}
