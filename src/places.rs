//! Where the code is: finding a person's projects without being told where they keep them.
//!
//! Carbon starts from the home folder. Any top-level folder there that holds a project within two levels is a
//! place code lives ("~/dev", "~/Projects", "~/code"…); the folders macOS guards or that never hold code roots
//! (Documents, Desktop, Downloads, Library and the media folders) are left alone unless asked for by name.
//! Inside such a place the layout is read, not assumed:
//!
//!   ~/code/thing            a project (it has a .git, a package.json, …)
//!   ~/code/thing/web        a part of that project
//!   ~/dev/work/thing        the same, one level down: "work" has no project markers, so it is a group of projects
//!
//! `~/.config/carbon/config.json` can override what was worked out: `roots` to say exactly where to look, `roles`
//! to put a folder on a particular port, `hidden` to leave projects out, `browser` to choose what opens links, and
//! `window` for how the desktop app's window opens. The app's setup page writes `roots` and `hidden`.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::Value;

/// Folders in a home directory that are never searched unless named in `roots`
const NOT_CODE: [&str; 9] = [
    "Applications",
    "Desktop",
    "Documents",
    "Downloads",
    "Library",
    "Movies",
    "Music",
    "Pictures",
    "Public",
];
/// What makes a folder a project
const MARKERS: [&str; 8] = [
    ".git",
    "package.json",
    "Cargo.toml",
    "go.mod",
    "deno.json",
    "pyproject.toml",
    "mise.toml",
    "Procfile",
];
/// Folders that are never worth walking into
const SKIP: [&str; 9] = [
    "node_modules",
    "dist",
    "build",
    "out",
    "target",
    "vendor",
    "tmp",
    "coverage",
    "venv",
];
/// Folders a larger repository keeps its apps in, searched one level further
const WORKSPACES: [&str; 4] = ["apps", "packages", "services", "sites"];

#[derive(Default, Deserialize)]
struct RawConfig {
    roots: Option<Vec<String>>,
    #[serde(default)]
    roles: HashMap<String, String>,
    browser: Option<String>,
    window: Option<String>,
    #[serde(default)]
    hidden: Vec<String>,
}

#[derive(Default)]
pub struct Config {
    /// Exactly where to look, instead of working it out
    pub roots: Option<Vec<PathBuf>>,
    /// Folders given a role by hand: "website", "admin", "pro" or "other"
    pub roles: HashMap<PathBuf, String>,
    /// The app links open in, e.g. "Helium". Without it, the system's default browser
    pub browser: Option<String>,
    /// How the desktop app's window opens: "fullscreen" or "maximized". Without it, at its own size
    pub window: Option<String>,
    /// Projects that were found but aren't wanted in the list
    pub hidden: Vec<PathBuf>,
}

/// A path the way it is written in the config: "~/dev" for a folder in the home folder
pub fn tidy(home: &Path, path: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(inside) if inside.as_os_str().is_empty() => "~".into(),
        Ok(inside) => format!("~/{}", inside.display()),
        Err(_) => path.display().to_string(),
    }
}

/// Keeps where to look and what to leave out, as chosen in the app. Everything else in the file is left as it is.
pub fn save(home: &Path, roots: &[PathBuf], hidden: &[PathBuf]) -> Result<(), String> {
    let path = home.join(".config/carbon/config.json");
    let mut all: serde_json::Map<String, Value> = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let written =
        |paths: &[PathBuf]| Value::from(paths.iter().map(|p| tidy(home, p)).collect::<Vec<_>>());
    all.insert("roots".into(), written(roots));
    if hidden.is_empty() {
        all.remove("hidden");
    } else {
        all.insert("hidden".into(), written(hidden));
    }
    let folder = path.parent().expect("the file is inside a folder");
    fs::create_dir_all(folder).map_err(|e| format!("Couldn't make {}: {e}", folder.display()))?;
    let text = serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?;
    fs::write(&path, text + "\n").map_err(|e| format!("Couldn't write {}: {e}", path.display()))
}

fn expand(home: &Path, path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if path == "~" => home.to_path_buf(),
        None => PathBuf::from(path),
    }
}

/// Reads ~/.config/carbon/config.json. A missing or broken file is the same as an empty one.
pub fn config(home: &Path) -> Config {
    let raw: RawConfig = fs::read_to_string(home.join(".config/carbon/config.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    Config {
        roots: raw
            .roots
            .map(|roots| roots.iter().map(|r| expand(home, r)).collect()),
        roles: raw
            .roles
            .iter()
            .map(|(path, role)| (expand(home, path), role.clone()))
            .collect(),
        browser: raw.browser,
        window: raw.window,
        hidden: raw.hidden.iter().map(|h| expand(home, h)).collect(),
    }
}

pub fn folders(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| !n.starts_with('.') && !SKIP.contains(&n))
        })
        .map(|e| e.path())
        .collect();
    found.sort();
    found
}

pub fn is_project(dir: &Path) -> bool {
    MARKERS.iter().any(|m| dir.join(m).exists())
}

/// The places code lives: the configured roots, or every top-level home folder holding a project within two levels
pub fn roots(home: &Path, config: &Config) -> Vec<PathBuf> {
    if let Some(roots) = &config.roots {
        return roots.iter().filter(|r| r.is_dir()).cloned().collect();
    }
    folders(home)
        .into_iter()
        .filter(|top| {
            top.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !NOT_CODE.contains(&n))
        })
        .filter(|top| {
            is_project(top)
                || folders(top)
                    .iter()
                    .any(|a| is_project(a) || folders(a).iter().any(|b| is_project(b)))
        })
        .collect()
}

/// One folder that might be runnable, and where it sits
pub struct Spot {
    pub dir: PathBuf,
    /// The folder its project is in, relative to home: "dev/work", or "" for a project directly in home
    pub group: String,
    pub project: String,
    pub project_dir: PathBuf,
    /// Its path inside the project, when it isn't the project itself: "web", "apps/site"
    pub part: Option<String>,
}

fn relative(home: &Path, dir: &Path) -> String {
    dir.strip_prefix(home)
        .unwrap_or(dir)
        .iter()
        .filter_map(|p| p.to_str())
        .collect::<Vec<_>>()
        .join("/")
}

/// A project and the folders inside it that could be apps of their own
fn project(home: &Path, dir: &Path, spots: &mut Vec<Spot>) {
    let group = dir.parent().map(|p| relative(home, p)).unwrap_or_default();
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let mut add = |at: PathBuf, part: Option<String>| {
        spots.push(Spot {
            dir: at,
            group: group.clone(),
            project: name.clone(),
            project_dir: dir.to_path_buf(),
            part,
        })
    };
    add(dir.to_path_buf(), None);
    for inner in folders(dir) {
        let part = relative(dir, &inner);
        if WORKSPACES.contains(&part.as_str()) {
            for deeper in folders(&inner) {
                let part = relative(dir, &deeper);
                add(deeper, Some(part));
            }
        } else {
            add(inner, Some(part));
        }
    }
}

/// Every folder worth testing, and every folder a new project could be created in (relative to home)
pub fn survey(home: &Path, roots: &[PathBuf]) -> (Vec<Spot>, Vec<String>) {
    let mut spots = Vec::new();
    let mut groups = Vec::new();
    for root in roots {
        if is_project(root) {
            project(home, root, &mut spots);
            continue;
        }
        let inside = folders(root);
        // A root that holds projects directly is itself somewhere a new one can go
        if inside.is_empty() || inside.iter().any(|a| is_project(a)) {
            groups.push(relative(home, root));
        }
        for a in inside {
            if is_project(&a) {
                project(home, &a, &mut spots);
            } else {
                // Not a project itself: a group of projects
                groups.push(relative(home, &a));
                for b in folders(&a) {
                    project(home, &b, &mut spots);
                }
            }
        }
    }
    groups.sort();
    groups.dedup();
    (spots, groups)
}

/// What a project says it is: its manifest's description, or the first line of prose in its README
pub fn describe(dirs: &[&Path]) -> Option<String> {
    let clean = |text: &str| {
        // Plain words: no markdown emphasis or code marks
        let text = text.replace(['*', '`'], "");
        let text = text.trim();
        (!text.is_empty()).then(|| text.chars().take(160).collect::<String>())
    };
    for dir in dirs {
        if let Some(found) = fs::read_to_string(dir.join("package.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["description"].as_str().and_then(clean))
        {
            return Some(found);
        }
        if let Some(found) = fs::read_to_string(dir.join("Cargo.toml"))
            .ok()
            .and_then(|t| {
                t.lines()
                    .find_map(|l| l.trim().strip_prefix("description"))
                    .and_then(|rest| clean(rest.trim_start_matches([' ', '=']).trim_matches('"')))
            })
        {
            return Some(found);
        }
        for readme in ["README.md", "readme.md", "README"] {
            let Ok(text) = fs::read_to_string(dir.join(readme)) else {
                continue;
            };
            // The first line that is a sentence: not a heading, a badge, an image, a rule or markup
            let prose = text.lines().map(str::trim).find(|l| {
                !l.is_empty() && !l.starts_with(['#', '!', '[', '<', '-', '=', '`', '|', '>'])
            });
            if let Some(found) = prose.and_then(clean) {
                return Some(found);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(paths: &[&str]) -> PathBuf {
        let home = std::env::temp_dir().join(format!(
            "carbon-places-{}-{}",
            std::process::id(),
            paths.len()
        ));
        let _ = fs::remove_dir_all(&home);
        for path in paths {
            let full = home.join(path);
            if path.ends_with('/') {
                fs::create_dir_all(&full).unwrap();
            } else {
                fs::create_dir_all(full.parent().unwrap()).unwrap();
                fs::write(&full, "{}").unwrap();
            }
        }
        home
    }

    #[test]
    fn finds_where_code_lives_and_reads_the_layout() {
        let home = tree(&[
            "code/thing/package.json",
            "code/thing/web/package.json",
            "dev/work/big/admin/package.json",
            "dev/work/big/site/package.json",
            "dev/work/solo/go.mod",
            "Documents/old/package.json",
            "Music/",
        ]);
        let found = roots(&home, &Config::default());
        assert_eq!(found, vec![home.join("code"), home.join("dev")]);

        let (spots, groups) = survey(&home, &found);
        let at = |dir: &str| {
            spots
                .iter()
                .find(|s| s.dir == home.join(dir))
                .map(|s| (s.group.as_str(), s.project.as_str(), s.part.as_deref()))
        };
        assert_eq!(at("code/thing"), Some(("code", "thing", None)));
        assert_eq!(at("code/thing/web"), Some(("code", "thing", Some("web"))));
        assert_eq!(
            at("dev/work/big/admin"),
            Some(("dev/work", "big", Some("admin")))
        );
        assert_eq!(at("dev/work/solo"), Some(("dev/work", "solo", None)));
        assert_eq!(groups, vec!["code", "dev/work"]);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn config_says_exactly_where_when_given() {
        let home = tree(&[
            "x/",
            "Documents/code/app/package.json",
            ".config/carbon/config.json",
        ]);
        fs::write(home.join(".config/carbon/config.json"), r#"{"roots":["~/Documents/code"],"roles":{"~/Documents/code/app":"pro"},"browser":"Safari"}"#).unwrap();
        let config = config(&home);
        assert_eq!(roots(&home, &config), vec![home.join("Documents/code")]);
        assert_eq!(
            config
                .roles
                .get(&home.join("Documents/code/app"))
                .map(String::as_str),
            Some("pro")
        );
        assert_eq!(config.browser.as_deref(), Some("Safari"));

        // What setup chooses is written beside what was there, which stays
        save(&home, &[home.join("x")], &[home.join("x/old")]).unwrap();
        let config = super::config(&home);
        assert_eq!(config.roots, Some(vec![home.join("x")]));
        assert_eq!(config.hidden, vec![home.join("x/old")]);
        assert_eq!(config.browser.as_deref(), Some("Safari"));
        save(&home, &[home.join("x")], &[]).unwrap();
        assert!(super::config(&home).hidden.is_empty());
        let _ = fs::remove_dir_all(&home);
    }
}
