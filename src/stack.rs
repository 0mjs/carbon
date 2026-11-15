//! What an app is built with: the tools worth a logo beside its name.
//!
//! Read from what the app's folder already says: the tool its dev command runs, its runtime, the packages it
//! depends on, and the manifests of other languages. Each tool has an id, which is also the name of its mark in
//! `marks/tech/`. Adding one is adding a line here and, if it has a logo, a file there.

use std::{fs, path::Path};

use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub struct Tech {
    pub id: &'static str,
    pub name: &'static str,
    /// The packages that mean an app uses it
    #[serde(skip)]
    packages: &'static [&'static str],
}

const fn tech(id: &'static str, name: &'static str, packages: &'static [&'static str]) -> Tech {
    Tech { id, name, packages }
}

/// In the order they are shown: what the app is, what runs it, then what it is made with
static ALL: [Tech; 29] = [
    tech("next", "Next.js", &["next"]),
    tech("astro", "Astro", &["astro"]),
    tech("nuxt", "Nuxt", &["nuxt"]),
    tech("remix", "Remix", &["@remix-run/react", "@remix-run/node"]),
    tech(
        "tanstack",
        "TanStack",
        &[
            "@tanstack/react-start",
            "@tanstack/react-router",
            "@tanstack/solid-start",
        ],
    ),
    tech("expo", "Expo", &["expo"]),
    tech("vite", "Vite", &["vite"]),
    tech("django", "Django", &[]),
    tech("fastapi", "FastAPI", &[]),
    tech("flask", "Flask", &[]),
    tech("bun", "Bun", &[]),
    tech("node", "Node.js", &[]),
    tech("deno", "Deno", &[]),
    tech("go", "Go", &[]),
    tech("rust", "Rust", &[]),
    tech("python", "Python", &[]),
    tech("react", "React", &["react"]),
    tech("vue", "Vue", &["vue"]),
    tech("svelte", "Svelte", &["svelte"]),
    tech("solid", "Solid", &["solid-js"]),
    tech("hono", "Hono", &["hono"]),
    tech("express", "Express", &["express"]),
    tech("tailwind", "Tailwind CSS", &["tailwindcss"]),
    tech("typescript", "TypeScript", &["typescript"]),
    tech("convex", "Convex", &["convex"]),
    tech("supabase", "Supabase", &["@supabase/supabase-js"]),
    tech("prisma", "Prisma", &["prisma", "@prisma/client"]),
    tech("drizzle", "Drizzle", &["drizzle-orm"]),
    tech("cloudflare", "Cloudflare", &["wrangler"]),
];

/// The names of everything a package.json depends on, for running or for building
fn packages(dir: &Path) -> Vec<String> {
    let Some(manifest) = fs::read_to_string(dir.join("package.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return Vec::new();
    };
    ["dependencies", "devDependencies"]
        .iter()
        .filter_map(|key| manifest[*key].as_object())
        .flat_map(|deps| deps.keys().cloned())
        .collect()
}

/// What an app is built with, given the tool its dev command runs ("vite", "cargo"…) and its runtime
pub fn of(dir: &Path, kind: &str, runtime: Option<&str>) -> Vec<&'static Tech> {
    let packages = packages(dir);
    let python = fs::read_to_string(dir.join("pyproject.toml"))
        .unwrap_or_default()
        .to_lowercase();
    let uses = |tech: &Tech| {
        let by_kind = match kind {
            "cargo" => "rust",
            other => other,
        };
        tech.id == by_kind
            || Some(tech.id) == runtime
            || tech
                .packages
                .iter()
                .any(|package| packages.iter().any(|have| have == package))
            || match tech.id {
                "rust" => dir.join("Cargo.toml").is_file(),
                "go" => dir.join("go.mod").is_file(),
                "python" => !python.is_empty(),
                "django" | "fastapi" | "flask" => python.contains(tech.id),
                _ => false,
            }
    };
    ALL.iter().filter(|tech| uses(tech)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_an_app_is_built_with() {
        let dir = std::env::temp_dir().join(format!("carbon-stack-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("package.json"), r#"{"dependencies":{"react":"19","@tanstack/react-router":"1"},"devDependencies":{"vite":"8","tailwindcss":"4","typescript":"6"}}"#).unwrap();
        let ids: Vec<&str> = of(&dir, "vite", Some("bun"))
            .iter()
            .map(|tech| tech.id)
            .collect();
        assert_eq!(
            ids,
            ["tanstack", "vite", "bun", "react", "tailwind", "typescript"]
        );

        fs::remove_file(dir.join("package.json")).unwrap();
        fs::write(dir.join("go.mod"), "module x").unwrap();
        let ids: Vec<&str> = of(&dir, "go", Some("go"))
            .iter()
            .map(|tech| tech.id)
            .collect();
        assert_eq!(ids, ["go"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
