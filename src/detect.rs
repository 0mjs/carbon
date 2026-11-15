//! Working out how to run a project, whatever it is written in.
//!
//! A folder is runnable when one of these is true, checked in this order:
//!   1. its mise.toml has a `dev` task            -> `mise run dev`
//!   2. its package.json has a `dev` script       -> that script (Vite, Next and Astro are told their port by flag)
//!   3. its deno.json has a `dev` task            -> `deno task dev`
//!   4. it has a Procfile with a `web:` line      -> that line
//!   5. it is a Rust crate built on a web server  -> `cargo run`
//!   6. it is a Go program that listens           -> `go run .`
//! Anything not told its port by flag gets it in the environment instead (PORT, and ADDRESS / ADDR as ":port").

use std::{fs, path::Path};

use serde_json::Value;

/// How one project is run.
#[derive(Clone)]
pub struct Plan {
    /// What it is, for the page: vite, next, astro, node, deno, cargo, go, mise or procfile
    pub kind: &'static str,
    /// The runtime mise should provide: bun, node, deno or go. Rust is left to rustup.
    pub runtime: Option<&'static str>,
    line: Line,
    /// A path that exists once the project's dependencies are installed, and the command that installs them
    pub install: Option<(&'static str, String)>,
}

#[derive(Clone)]
enum Line {
    /// A package.json script that starts a web tool. `at` is where the tool's name ends in the script.
    Web {
        script: String,
        at: usize,
        tool: &'static str,
    },
    /// A command that reads its port from the environment
    Env(String),
}

impl Plan {
    /// The command line that starts this project on `port`
    pub fn command(&self, port: u16) -> String {
        match &self.line {
            Line::Env(line) => line.clone(),
            Line::Web { script, at, tool } => {
                let (head, rest) = script.split_at(*at);
                // Only the tool's own arguments: up to the end of its quoted string or its command
                let end = ["\"", "'", "&&", "||", ";", "|"]
                    .iter()
                    .filter_map(|t| rest.find(t))
                    .min()
                    .unwrap_or(rest.len());
                let (own, tail) = rest.split_at(end);
                // Drop any port the script set itself, so there is exactly one and it is ours
                let mut kept = Vec::new();
                let mut words = own.split_whitespace().peekable();
                while let Some(word) = words.next() {
                    if word == "--strictPort" || word.starts_with("--port=") {
                        continue;
                    }
                    if (word == "--port" || word == "-p")
                        && words
                            .peek()
                            .is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()))
                    {
                        words.next();
                        continue;
                    }
                    kept.push(word);
                }
                let flags = match *tool {
                    "next" => format!("-p {port}"),
                    "astro" => format!("--port {port}"),
                    _ => format!("--port {port} --strictPort"),
                };
                let own = if kept.is_empty() {
                    String::new()
                } else {
                    format!(" {}", kept.join(" "))
                };
                format!("{head} {flags}{own}{tail}")
            }
        }
    }

    /// True when the port is passed in the environment rather than by flag
    pub fn by_env(&self) -> bool {
        matches!(self.line, Line::Env(_))
    }
}

/// Finds a web tool's command in a script: the tool's name and where the match ends
fn web_tool(script: &str) -> Option<(&'static str, usize)> {
    const TOOLS: [(&str, &str); 4] = [
        ("next dev", "next"),
        ("astro dev", "astro"),
        ("vite dev", "vite"),
        ("vite", "vite"),
    ];
    let part_of_word = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    for (needle, tool) in TOOLS {
        let mut from = 0;
        while let Some(i) = script[from..].find(needle) {
            let (start, end) = (from + i, from + i + needle.len());
            // A whole word: not "previte", not "./bin/vite", not "vite-node"
            let before = script[..start]
                .chars()
                .next_back()
                .is_none_or(|c| !part_of_word(c) && c != '.' && c != '/');
            let after = script[end..]
                .chars()
                .next()
                .is_none_or(|c| !part_of_word(c));
            if before && after {
                return Some((tool, end));
            }
            from = end;
        }
    }
    None
}

fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn json(path: &Path) -> Option<Value> {
    serde_json::from_str(&read(path)?).ok()
}

/// A mise.toml that defines a `dev` task, either as `[tasks.dev]` or as `dev = …` under `[tasks]`
fn has_mise_dev(dir: &Path) -> bool {
    let Some(text) = read(&dir.join("mise.toml")).or_else(|| read(&dir.join(".mise.toml"))) else {
        return false;
    };
    let mut in_tasks = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            if line == "[tasks.dev]" {
                return true;
            }
            in_tasks = line == "[tasks]";
        } else if in_tasks
            && line
                .split('=')
                .next()
                .is_some_and(|k| k.trim().trim_matches('"') == "dev")
            && line.contains('=')
        {
            return true;
        }
    }
    false
}

/// Scripts named `dev` that are not a server a browser can open
const NOT_A_SERVER: [&str; 5] = ["convex", "expo", "remotion", "tsc", "tsup"];

pub fn detect(dir: &Path) -> Option<Plan> {
    if has_mise_dev(dir) {
        return Some(Plan {
            kind: "mise",
            runtime: None,
            line: Line::Env("mise run dev".into()),
            install: None,
        });
    }

    if let Some(pkg) = json(&dir.join("package.json")) {
        if let Some(script) = pkg["scripts"]["dev"].as_str() {
            // The package manager is whichever left a lockfile; with none, bun
            let (manager, runtime) = if dir.join("pnpm-lock.yaml").exists() {
                ("pnpm", "node")
            } else if dir.join("yarn.lock").exists() {
                ("yarn", "node")
            } else if dir.join("package-lock.json").exists()
                && !dir.join("bun.lock").exists()
                && !dir.join("bun.lockb").exists()
            {
                ("npm", "node")
            } else {
                ("bun", "bun")
            };
            if let Some((tool, at)) = web_tool(script) {
                let bin: &'static str = match tool {
                    "next" => "node_modules/.bin/next",
                    "astro" => "node_modules/.bin/astro",
                    _ => "node_modules/.bin/vite",
                };
                return Some(Plan {
                    kind: tool,
                    runtime: Some(runtime),
                    line: Line::Web {
                        script: script.to_string(),
                        at,
                        tool,
                    },
                    install: Some((bin, format!("{manager} install"))),
                });
            }
            let first = script.split_whitespace().next().unwrap_or("");
            if !NOT_A_SERVER.contains(&first) {
                return Some(Plan {
                    kind: "node",
                    runtime: Some(runtime),
                    line: Line::Env(format!("{manager} run dev")),
                    install: Some(("node_modules", format!("{manager} install"))),
                });
            }
        }
    }

    if let Some(deno) = json(&dir.join("deno.json")).or_else(|| json(&dir.join("deno.jsonc"))) {
        if deno["tasks"]["dev"].is_string() || deno["tasks"]["dev"].is_object() {
            return Some(Plan {
                kind: "deno",
                runtime: Some("deno"),
                line: Line::Env("deno task dev".into()),
                install: None,
            });
        }
    }

    if let Some(procfile) = read(&dir.join("Procfile")) {
        if let Some(web) = procfile.lines().find_map(|l| l.strip_prefix("web:")) {
            return Some(Plan {
                kind: "procfile",
                runtime: None,
                line: Line::Env(web.trim().to_string()),
                install: None,
            });
        }
    }

    if dir.join("src/main.rs").exists() {
        if let Some(cargo) = read(&dir.join("Cargo.toml")) {
            // Only crates that serve something: a command-line tool is not a dev server
            if [
                "axum",
                "actix-web",
                "rocket",
                "warp",
                "poem",
                "salvo",
                "tide",
            ]
            .iter()
            .any(|f| cargo.lines().any(|l| l.trim_start().starts_with(f)))
            {
                return Some(Plan {
                    kind: "cargo",
                    runtime: None,
                    line: Line::Env("cargo run".into()),
                    install: None,
                });
            }
        }
    }

    if dir.join("go.mod").exists() {
        let listens = fs::read_dir(dir).ok()?.flatten().any(|e| {
            let p = e.path();
            p.extension().is_some_and(|x| x == "go")
                // A program, not a library that other programs use to listen
                && read(&p).is_some_and(|src| src.contains("package main") && ["ListenAndServe", ".Listen(", "http.Server{"].iter().any(|m| src.contains(m)))
        });
        if listens {
            return Some(Plan {
                kind: "go",
                runtime: Some("go"),
                line: Line::Env("go run .".into()),
                install: None,
            });
        }
    }

    None
}

/// True when the folder says which tool versions it wants, so mise has something to honour
pub fn pinned(dir: &Path) -> bool {
    ["mise.toml", ".mise.toml", ".tool-versions"]
        .iter()
        .any(|f| dir.join(f).exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn web(script: &str, port: u16) -> String {
        let (tool, at) = web_tool(script).expect("a web tool");
        Plan {
            kind: tool,
            runtime: None,
            line: Line::Web {
                script: script.into(),
                at,
                tool,
            },
            install: None,
        }
        .command(port)
    }

    #[test]
    fn sets_the_port_and_removes_the_scripts_own() {
        assert_eq!(web("vite", 3001), "vite --port 3001 --strictPort");
        assert_eq!(
            web("vite --port 3000", 3001),
            "vite --port 3001 --strictPort"
        );
        assert_eq!(
            web("vite dev --port 3000 --strictPort", 3004),
            "vite dev --port 3004 --strictPort"
        );
        assert_eq!(
            web("next dev --turbopack -p 3000", 3001),
            "next dev -p 3001 --turbopack"
        );
        assert_eq!(
            web("astro dev --force", 3001),
            "astro dev --port 3001 --force"
        );
    }

    #[test]
    fn only_touches_the_web_tool_in_a_longer_script() {
        assert_eq!(
            web(
                "mkdir -p dist && concurrently -k \"vite\" \"wrangler dev --port 8787\"",
                3002
            ),
            "mkdir -p dist && concurrently -k \"vite --port 3002 --strictPort\" \"wrangler dev --port 8787\""
        );
    }

    #[test]
    fn ignores_look_alikes() {
        assert!(web_tool("vite-node src/index.ts").is_none());
        assert!(web_tool("convex dev").is_none());
        assert!(web_tool("./scripts/previte").is_none());
    }
}
