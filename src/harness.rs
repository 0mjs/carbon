//! Harness adapters: one per coding agent Carbon can recognise.
//!
//! A harness is the program a person talks to (Claude Code, Codex, Gemini CLI, Grok CLI), as opposed to the model
//! behind it. Each adapter answers two questions: "is this process one of mine?" and, where the harness keeps
//! records Carbon can read, "which session is this, and what is it called?". Adding a harness is adding one
//! adapter here and one line to `ALL`; its mark is chosen by `vendor`, which the page turns into a logo.

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
    pub version: Option<String>,
}

pub trait Harness: Sync {
    /// A stable key, e.g. "claude-code"
    fn id(&self) -> &'static str;
    /// What people call it, e.g. "Claude Code"
    fn name(&self) -> &'static str;
    /// Whose mark to show: "claude", "openai", "gemini" or "grok"
    fn vendor(&self) -> &'static str;
    /// The name of its program, and the package it ships as when it is a script run by node or bun
    fn program(&self) -> (&'static str, &'static str);
    /// What it can tell about the session running as `pid`
    fn session(&self, _pid: i32, _home: &Path) -> Session {
        Session::default()
    }

    fn is(&self, proc: &Proc) -> bool {
        let (program, package) = self.program();
        let name = proc.name();
        if name == program {
            return true;
        }
        // Some are a script run by node or bun: look at the script, not the runtime
        ["node", "bun", "deno"].contains(&name) && {
            let script = proc.args.split_whitespace().nth(1).unwrap_or("");
            script.contains(package) || script.rsplit('/').next() == Some(program)
        }
    }
}

pub static ALL: [&dyn Harness; 4] = [&ClaudeCode, &Codex, &GeminiCli, &GrokCli];

/// The harness a process is, if it is one
pub fn of(proc: &Proc) -> Option<&'static dyn Harness> {
    ALL.iter().copied().find(|h| h.is(proc))
}

/* ---------- Claude Code ---------- */

/// Claude Code writes one small file per running session at ~/.claude/sessions/<pid>.json (its id, its name and
/// whether it is busy), and each session's conversation at ~/.claude/projects/<folder>/<id>.jsonl, where it also
/// records the title it gave the conversation.
pub struct ClaudeCode;

impl ClaudeCode {
    /// The latest title Claude Code gave a conversation. Titles are rewritten as the conversation goes on, so only
    /// the end of the file is read.
    fn title(home: &Path, id: &str) -> Option<String> {
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
        String::from_utf8_lossy(&tail)
            .lines()
            .rev()
            .filter(|line| line.contains("\"ai-title\""))
            .find_map(|line| {
                serde_json::from_str::<Value>(line).ok()?["aiTitle"]
                    .as_str()
                    .map(String::from)
            })
    }
}

impl Harness for ClaudeCode {
    fn id(&self) -> &'static str {
        "claude-code"
    }
    fn name(&self) -> &'static str {
        "Claude Code"
    }
    fn vendor(&self) -> &'static str {
        "claude"
    }
    fn program(&self) -> (&'static str, &'static str) {
        ("claude", "@anthropic-ai/claude-code")
    }

    fn session(&self, pid: i32, home: &Path) -> Session {
        let record = fs::read_to_string(home.join(format!(".claude/sessions/{pid}.json")))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok());
        let Some(record) = record else {
            return Session::default();
        };
        let field = |key: &str| record[key].as_str().map(String::from);
        let id = field("sessionId");
        // A name the person chose wins; otherwise the conversation's own title, then the name Claude Code derived
        let named = (record["nameSource"] == "user")
            .then(|| field("name"))
            .flatten();
        let title = named
            .or_else(|| id.as_deref().and_then(|id| Self::title(home, id)))
            .or_else(|| field("name"));
        Session {
            id,
            title,
            status: field("status"),
            version: field("version"),
        }
    }
}

/* ---------- The others: recognised, with nothing more known about their sessions yet ---------- */

pub struct Codex;
impl Harness for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }
    fn name(&self) -> &'static str {
        "Codex"
    }
    fn vendor(&self) -> &'static str {
        "openai"
    }
    fn program(&self) -> (&'static str, &'static str) {
        ("codex", "@openai/codex")
    }
}

pub struct GeminiCli;
impl Harness for GeminiCli {
    fn id(&self) -> &'static str {
        "gemini-cli"
    }
    fn name(&self) -> &'static str {
        "Gemini CLI"
    }
    fn vendor(&self) -> &'static str {
        "gemini"
    }
    fn program(&self) -> (&'static str, &'static str) {
        ("gemini", "@google/gemini-cli")
    }
}

pub struct GrokCli;
impl Harness for GrokCli {
    fn id(&self) -> &'static str {
        "grok-cli"
    }
    fn name(&self) -> &'static str {
        "Grok CLI"
    }
    fn vendor(&self) -> &'static str {
        "grok"
    }
    fn program(&self) -> (&'static str, &'static str) {
        ("grok", "grok-cli")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(args: &str) -> Proc {
        Proc {
            parent: 1,
            up: 60,
            args: args.into(),
        }
    }

    #[test]
    fn knows_each_harness() {
        assert_eq!(
            of(&proc("claude --resume abc")).map(|h| h.id()),
            Some("claude-code")
        );
        assert_eq!(
            of(&proc("/opt/homebrew/bin/codex")).map(|h| h.id()),
            Some("codex")
        );
        assert_eq!(
            of(&proc(
                "node /usr/lib/node_modules/@google/gemini-cli/dist/index.js"
            ))
            .map(|h| h.id()),
            Some("gemini-cli")
        );
        assert_eq!(
            of(&proc("bun /Users/me/.bun/bin/grok")).map(|h| h.id()),
            Some("grok-cli")
        );
    }

    #[test]
    fn is_not_fooled_by_a_name_in_a_path() {
        assert!(of(&proc("node /Users/me/dev/claude/server.js")).is_none());
        assert!(
            of(&proc(
                "/bin/zsh -c source /Users/me/.claude/shell-snapshots/x.sh"
            ))
            .is_none()
        );
        assert!(of(&proc("node /Users/me/dev/saas/app/node_modules/.bin/vite")).is_none());
    }
}
