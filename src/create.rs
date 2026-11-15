//! Creating a new project under ~/dev from a template, with its tools pinned by mise from the first commit.

use serde::Serialize;

use crate::engine::Hub;

#[derive(Serialize)]
pub struct Template {
    pub id: &'static str,
    pub label: &'static str,
    pub detail: &'static str,
}

pub const TEMPLATES: [Template; 3] = [
    Template {
        id: "start",
        label: "TanStack Start",
        detail: "Server rendered. Vite, TanStack Router, Tailwind, Bun.",
    },
    Template {
        id: "spa",
        label: "TanStack Router",
        detail: "Client only. Vite, TanStack Router, Tailwind, Bun.",
    },
    Template {
        id: "bun",
        label: "Empty Bun project",
        detail: "bun init, and nothing else.",
    },
];

/// Starts creating `~/dev/<group>/<name>` and returns the key of the job doing it
pub fn create(hub: &Hub, group: &str, name: &str, template: &str) -> Result<String, String> {
    let named_well = !name.is_empty()
        && name.len() <= 40
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-');
    if !named_well {
        return Err("Use lowercase letters, numbers and dashes for the name".into());
    }
    // The group must be one of the folders that is really there: never a path
    let parent = hub.dev.join(group);
    if group.contains(['/', '.']) || !parent.is_dir() {
        return Err(format!("There is no ~/dev/{group}"));
    }
    if parent.join(name).exists() {
        return Err(format!("~/dev/{group}/{name} already exists"));
    }

    let scaffold = |extra: &str| {
        format!(
            "bunx @tanstack/cli@latest create {name} --framework React --build-tool vite --package-manager bun \
             --no-examples --no-toolchain --no-intent --git --yes{extra}"
        )
    };
    let make = match template {
        "start" => scaffold(""),
        "spa" => scaffold(" --router-only"),
        "bun" => format!("mkdir {name} && cd {name} && bun init -y && cd .."),
        other => return Err(format!("No template called {other}")),
    };
    // Pin the runtime in the new project's own mise.toml, so it runs the same next year
    let line = format!("{make} && cd {name} && mise use --pin bun");
    let cmd = hub.command(&parent, &[], &line);
    let key = hub.job(
        format!("create:{group}/{name}"),
        format!("Create {group}/{name}"),
        cmd,
        &format!("$ {line}"),
    )?;
    hub.forget();
    Ok(key)
}
