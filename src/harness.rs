//! Harness adapters: one per coding agent Carbon can recognise.
//!
//! A harness is the program that runs commands for a person (Claude Code, Codex, pi, OpenCode…), as opposed to the
//! model behind it or the app it runs in. Each adapter answers "is this process one of mine?" and, where the
//! harness keeps records Carbon can read, "which session is this, what is it called, and which model is it
//! using?". Adding a harness is adding one entry to `ALL`, and a mark named after its id in `marks/` if it has one.

use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use serde_json::Value;

use crate::agents::Proc;

/// What an adapter could find out about one running session. Anything it couldn't learn is left empty.
#[derive(Default)]
pub struct Session {
    /// The session's own id in the harness's records
    pub id: Option<String>,
    /// What the session is called: a name the person gave it, or the harness's own title for the conversation
    pub title: Option<String>,
    /// "busy" while the agent is working, "idle" while it waits for the person
    pub status: Option<String>,
    /// The model it last answered with, as the harness records it
    pub model: Option<String>,
    pub version: Option<String>,
}

pub struct Harness {
    /// A stable key, e.g. "claude-code". Its mark, when there is one, is the file of this name in `marks/`.
    pub id: &'static str,
    /// What people call it, e.g. "Claude Code"
    pub name: &'static str,
    /// What its program is called
    programs: &'static [&'static str],
    /// The packages it ships as, for when it is a script run by node, bun, deno or python
    packages: &'static [&'static str],
    /// What it can tell about the session running as a pid, given the home folder
    records: fn(i32, &Path) -> Session,
}

/// Programs that run a script: the script says what is running, not the runtime
const RUNTIMES: [&str; 4] = ["node", "bun", "deno", "python"];

impl Harness {
    pub fn is(&self, proc: &Proc) -> bool {
        let name = proc.name();
        if self.programs.contains(&name) {
            return true;
        }
        if !RUNTIMES.iter().any(|runtime| name.starts_with(runtime)) {
            return false;
        }
        // The script is the first thing after the runtime that isn't one of the runtime's own flags
        let script = proc
            .given()
            .split_whitespace()
            .find(|word| !word.starts_with('-'))
            .unwrap_or("");
        let file = script.rsplit('/').next().unwrap_or("");
        self.packages
            .iter()
            .any(|package| script.contains(&format!("/{package}/")))
            || self.programs.contains(&file)
    }

    pub fn session(&self, pid: i32, home: &Path) -> Session {
        (self.records)(pid, home)
    }
}

fn nothing(_: i32, _: &Path) -> Session {
    Session::default()
}

/// A harness Carbon recognises by its program, with nothing known about its sessions yet
const fn known(
    id: &'static str,
    name: &'static str,
    programs: &'static [&'static str],
    packages: &'static [&'static str],
) -> Harness {
    Harness {
        id,
        name,
        programs,
        packages,
        records: nothing,
    }
}

/// Program and package names are the ones each project publishes. oh-my-pi comes before pi: it is a fork, and its
/// package name ends the same way.
pub static ALL: [Harness; 18] = [
    Harness {
        id: "claude-code",
        name: "Claude Code",
        programs: &["claude"],
        packages: &["@anthropic-ai/claude-code"],
        records: claude_code,
    },
    known("codex", "Codex", &["codex"], &["@openai/codex"]),
    known(
        "gemini-cli",
        "Gemini CLI",
        &["gemini"],
        &["@google/gemini-cli"],
    ),
    known(
        "grok-cli",
        "Grok CLI",
        &["grok"],
        &["@xai-official/grok", "@vibe-kit/grok-cli"],
    ),
    known(
        "oh-my-pi",
        "oh-my-pi",
        &["omp"],
        &["@oh-my-pi/pi-coding-agent"],
    ),
    known(
        "pi",
        "pi",
        &["pi"],
        &[
            "@earendil-works/pi-coding-agent",
            "@mariozechner/pi-coding-agent",
        ],
    ),
    known("opencode", "OpenCode", &["opencode"], &["opencode-ai"]),
    known(
        "amp",
        "Amp",
        &["amp"],
        &["@ampcode/cli", "@sourcegraph/amp"],
    ),
    known("cursor-agent", "Cursor Agent", &["cursor-agent"], &[]),
    known("copilot", "Copilot CLI", &["copilot"], &["@github/copilot"]),
    known("droid", "Droid", &["droid"], &["@factory/cli"]),
    known(
        "qwen-code",
        "Qwen Code",
        &["qwen"],
        &["@qwen-code/qwen-code"],
    ),
    known(
        "kilo",
        "Kilo Code",
        &["kilo", "kilocode"],
        &["@kilocode/cli"],
    ),
    known("cline", "Cline", &["cline"], &[]),
    known("auggie", "Auggie", &["auggie"], &["@augmentcode/auggie"]),
    known("crush", "Crush", &["crush"], &["@charmland/crush"]),
    known("goose", "Goose", &["goose"], &[]),
    known("aider", "Aider", &["aider"], &[]),
];

/// The harness a process is, if it is one
pub fn of(proc: &Proc) -> Option<&'static Harness> {
    ALL.iter().find(|harness| harness.is(proc))
}

/* ---------- Claude Code ---------- */

/// The end of a session's conversation. Claude Code keeps each one at ~/.claude/projects/<folder>/<id>.jsonl and
/// rewrites its title as it goes on, so only the end is read.
fn claude_tail(home: &Path, id: &str) -> Option<String> {
    let transcript: PathBuf = fs::read_dir(home.join(".claude/projects"))
        .ok()?
        .flatten()
        .map(|d| d.path().join(format!("{id}.jsonl")))
        .find(|p| p.is_file())?;
    let mut file = fs::File::open(transcript).ok()?;
    let size = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(size.saturating_sub(256 * 1024)))
        .ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).ok()?;
    Some(String::from_utf8_lossy(&tail).into_owned())
}

/// The latest title Claude Code gave a conversation
fn claude_title(tail: &str) -> Option<String> {
    tail.lines()
        .rev()
        .filter(|line| line.contains("\"ai-title\""))
        .find_map(|line| {
            serde_json::from_str::<Value>(line).ok()?["aiTitle"]
                .as_str()
                .map(String::from)
        })
}

/// The model behind the latest answer in a conversation
fn claude_model(tail: &str) -> Option<String> {
    tail.lines()
        .rev()
        .filter(|line| line.contains("\"type\":\"assistant\""))
        .find_map(|line| {
            serde_json::from_str::<Value>(line).ok()?["message"]["model"]
                .as_str()
                .map(String::from)
        })
        // Messages Claude Code writes itself carry a placeholder, not a model
        .filter(|model| !model.starts_with('<'))
}

/// Claude Code writes one small file per running session at ~/.claude/sessions/<pid>.json: its id, its name and
/// whether it is busy.
fn claude_code(pid: i32, home: &Path) -> Session {
    let record = fs::read_to_string(home.join(format!(".claude/sessions/{pid}.json")))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let Some(record) = record else {
        return Session::default();
    };
    let field = |key: &str| record[key].as_str().map(String::from);
    let id = field("sessionId");
    let tail = id.as_deref().and_then(|id| claude_tail(home, id));
    // A name the person chose wins; otherwise the conversation's own title, then the name Claude Code derived
    let named = (record["nameSource"] == "user")
        .then(|| field("name"))
        .flatten();
    let title = named
        .or_else(|| tail.as_deref().and_then(claude_title))
        .or_else(|| field("name"));
    Session {
        id,
        title,
        status: field("status"),
        model: tail.as_deref().and_then(claude_model),
        version: field("version"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(args: &str) -> Proc {
        Proc::of(1, 60, args)
    }

    fn id(args: &str) -> Option<&'static str> {
        of(&proc(args)).map(|harness| harness.id)
    }

    #[test]
    fn knows_each_harness() {
        assert_eq!(id("claude --resume abc"), Some("claude-code"));
        assert_eq!(id("/opt/homebrew/bin/codex"), Some("codex"));
        assert_eq!(
            id("node /usr/lib/node_modules/@google/gemini-cli/dist/index.js"),
            Some("gemini-cli")
        );
        assert_eq!(id("bun /Users/me/.bun/bin/grok"), Some("grok-cli"));
        // A program in a folder with a space in its name is known by its path, not by its command line
        let spaced = Proc {
            parent: 1,
            up: 60,
            program: "/Users/me/Library/Application Support/Tern/omp/bin/omp".into(),
            args: "omp --mode rpc".into(),
        };
        assert_eq!(of(&spaced).map(|harness| harness.id), Some("oh-my-pi"));
        assert_eq!(id("/Users/me/.local/bin/omp --model x"), Some("oh-my-pi"));
        assert_eq!(
            id("node --no-warnings /x/node_modules/@earendil-works/pi-coding-agent/dist/cli.js"),
            Some("pi")
        );
        assert_eq!(
            id("bun /x/node_modules/@oh-my-pi/pi-coding-agent/src/cli.ts"),
            Some("oh-my-pi")
        );
        assert_eq!(id("opencode"), Some("opencode"));
        assert_eq!(
            id("/Users/me/.venv/bin/python3.13 /Users/me/.local/bin/aider --model x"),
            Some("aider")
        );
    }

    #[test]
    fn is_not_fooled_by_a_name_in_a_path() {
        assert_eq!(id("node /Users/me/dev/claude/server.js"), None);
        assert_eq!(
            id("/bin/zsh -c source /Users/me/.claude/shell-snapshots/x.sh"),
            None
        );
        assert_eq!(
            id("node /Users/me/dev/saas/app/node_modules/.bin/vite"),
            None
        );
        assert_eq!(id("node /Users/me/dev/pi/server.js"), None);
        assert_eq!(id("node /Users/me/dev/opencode-ai-notes/index.js"), None);
    }

    #[test]
    fn reads_the_title_and_the_model_from_the_end_of_a_conversation() {
        let tail = concat!(
            "{\"type\":\"ai-title\",\"aiTitle\":\"First\"}\n",
            "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-one\"}}\n",
            "{\"type\":\"ai-title\",\"aiTitle\":\"Second\"}\n",
            "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-two\"}}\n",
            "{\"type\":\"user\",\"message\":{\"content\":\"hello\"}}\n",
        );
        assert_eq!(claude_title(tail).as_deref(), Some("Second"));
        assert_eq!(claude_model(tail).as_deref(), Some("claude-two"));
    }
}
