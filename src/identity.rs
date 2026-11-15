//! What a project is called and what it looks like.
//!
//! Carbon works both out from what the project already has: a name from its web manifest, its Expo config or
//! its Xcode project, a logo from the icon its site or app ships. Either can be replaced by hand; what was chosen is kept in
//! `~/.config/carbon/projects.json`, and a chosen logo is copied to `~/.config/carbon/logos/` so it survives
//! the original being moved.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::places;

/// Icon files, best first: the large square ones made for a home screen, then drawn marks, then small favicons
const ICONS: [&str; 11] = [
    "apple-touch-icon.png",
    "icon-512.png",
    "android-chrome-512x512.png",
    "icon.png",
    "icon.svg",
    "logo.svg",
    "favicon.svg",
    "logo.png",
    "favicon.png",
    "favicon-32x32.png",
    "favicon-32.png",
];
/// Where a project keeps them, relative to the project or one of its parts
const ICON_FOLDERS: [&str; 9] = [
    "public",
    "app",
    "src/app",
    "static",
    "assets",
    "assets/images",
    "public/images",
    "brand",
    "",
];
/// Files that say what a site is called
const MANIFESTS: [&str; 3] = [
    "public/manifest.webmanifest",
    "public/site.webmanifest",
    "public/manifest.json",
];
/// What a logo chosen by hand may be
pub const IMAGES: [&str; 6] = ["png", "jpg", "jpeg", "svg", "webp", "gif"];

/// What a project has been given by hand
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Chosen {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// A file name inside the logos folder
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
}

/// What a project says about itself
#[derive(Clone, Default)]
pub struct Found {
    pub name: Option<String>,
    pub logo: Option<PathBuf>,
}

fn file(home: &Path) -> PathBuf {
    home.join(".config/carbon/projects.json")
}

pub fn logos(home: &Path) -> PathBuf {
    home.join(".config/carbon/logos")
}

/// Everything chosen by hand, by project key. A missing or broken file is the same as an empty one.
pub fn chosen(home: &Path) -> HashMap<String, Chosen> {
    fs::read_to_string(file(home))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn keep(home: &Path, all: &HashMap<String, Chosen>) -> Result<(), String> {
    let path = file(home);
    let folder = path.parent().expect("the file is inside a folder");
    fs::create_dir_all(folder).map_err(|e| format!("Couldn't make {}: {e}", folder.display()))?;
    // Sorted, so the file reads the same from one save to the next
    let sorted: std::collections::BTreeMap<_, _> = all.iter().collect();
    let text = serde_json::to_string_pretty(&sorted).map_err(|e| e.to_string())?;
    fs::write(&path, text + "\n").map_err(|e| format!("Couldn't write {}: {e}", path.display()))
}

/// Changes what one project was given, and forgets the project once nothing is left
fn change(home: &Path, key: &str, edit: impl FnOnce(&mut Chosen)) -> Result<(), String> {
    let mut all = chosen(home);
    let mut one = all.remove(key).unwrap_or_default();
    edit(&mut one);
    if one.name.is_some() || one.logo.is_some() {
        all.insert(key.to_string(), one);
    }
    keep(home, &all)
}

/// Gives a project a name, or with an empty one goes back to the name it has by itself
pub fn rename(home: &Path, key: &str, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.chars().count() > 60 {
        return Err("A name can be 60 characters at most".into());
    }
    change(home, key, |one| {
        one.name = (!name.is_empty()).then(|| name.to_string())
    })
}

/// Gives a project a logo from an image file, or with none goes back to the one it has by itself
pub fn set_logo(home: &Path, key: &str, image: Option<&Path>) -> Result<(), String> {
    let folder = logos(home);
    let before = chosen(home).get(key).and_then(|one| one.logo.clone());
    let name = match image {
        None => None,
        Some(image) => {
            let kind = image
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .filter(|e| IMAGES.contains(&e.as_str()))
                .ok_or("A logo is a PNG, JPEG, SVG, WebP or GIF image")?;
            fs::create_dir_all(&folder)
                .map_err(|e| format!("Couldn't make {}: {e}", folder.display()))?;
            // The time is in the name so anything showing the old picture sees a new file
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_millis());
            let safe: String = key
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect();
            let name = format!("{safe}-{stamp}.{kind}");
            fs::copy(image, folder.join(&name))
                .map_err(|e| format!("Couldn't copy {}: {e}", image.display()))?;
            Some(name)
        }
    };
    change(home, key, |one| one.logo = name)?;
    if let Some(before) = before {
        let _ = fs::remove_file(folder.join(before));
    }
    Ok(())
}

/// The folders to look in: the project itself, then its parts, the ones most likely to be its public face first
fn faces(project: &Path) -> Vec<PathBuf> {
    let mut parts = places::folders(project);
    let rank = |dir: &PathBuf| match dir.file_name().and_then(|n| n.to_str()) {
        Some("web" | "website" | "site" | "www") => 0,
        Some("app" | "mobile") => 1,
        _ => 2,
    };
    parts.sort_by_key(rank);
    let mut all = vec![project.to_path_buf()];
    all.extend(parts);
    all
}

fn named(file: &Path, at: &[&str]) -> Option<String> {
    let value: Value = serde_json::from_str(&fs::read_to_string(file).ok()?).ok()?;
    let name = at.iter().fold(&value, |v, key| &v[*key]).as_str()?.trim();
    (!name.is_empty() && name.chars().count() <= 60).then(|| name.to_string())
}

/// What an Xcode project in a folder is called: "Thing" from "Thing.xcodeproj"
fn xcode(dir: &Path) -> Option<String> {
    fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        (path.extension()? == "xcodeproj")
            .then(|| path.file_stem()?.to_str().map(str::to_string))?
    })
}

/// The name and logo a project already has, if it has them
pub fn find(project: &Path) -> Found {
    let faces = faces(project);
    let name = faces.iter().find_map(|dir| {
        MANIFESTS
            .iter()
            .find_map(|m| named(&dir.join(m), &["name"]))
            .or_else(|| named(&dir.join("app.json"), &["expo", "name"]))
            .or_else(|| xcode(dir))
    });
    // The best kind of icon anywhere in the project beats a lesser one in a likelier place
    let logo = ICONS.iter().find_map(|icon| {
        faces.iter().find_map(|dir| {
            ICON_FOLDERS
                .iter()
                .map(|folder| dir.join(folder).join(icon))
                .find(|path| path.is_file())
        })
    });
    Found { name, logo }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("carbon-identity-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (path, text) in files {
            let full = root.join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, text).unwrap();
        }
        root
    }

    #[test]
    fn reads_the_name_and_the_best_icon_a_project_ships() {
        let root = tree(
            "find",
            &[
                ("thing/web/public/favicon-32.png", ""),
                ("thing/app/assets/images/icon.png", ""),
                ("thing/app/app.json", r#"{"expo":{"name":"Thing"}}"#),
                ("plain/package.json", "{}"),
                ("native/app/Native.xcodeproj/project.pbxproj", ""),
            ],
        );
        assert_eq!(find(&root.join("native")).name.as_deref(), Some("Native"));
        let found = find(&root.join("thing"));
        assert_eq!(found.name.as_deref(), Some("Thing"));
        assert_eq!(
            found.logo,
            Some(root.join("thing/app/assets/images/icon.png"))
        );
        let plain = find(&root.join("plain"));
        assert!(plain.name.is_none() && plain.logo.is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn keeps_what_was_chosen_and_forgets_it_when_cleared() {
        let home = tree("chosen", &[("pictures/mark.PNG", "x")]);
        rename(&home, "dev/thing", "  The Thing ").unwrap();
        set_logo(&home, "dev/thing", Some(&home.join("pictures/mark.PNG"))).unwrap();
        let one = chosen(&home).remove("dev/thing").unwrap();
        assert_eq!(one.name.as_deref(), Some("The Thing"));
        let logo = logos(&home).join(one.logo.unwrap());
        assert!(logo.is_file() && logo.extension().unwrap() == "png");

        assert!(set_logo(&home, "dev/thing", Some(&home.join("notes.txt"))).is_err());
        set_logo(&home, "dev/thing", None).unwrap();
        assert!(!logo.exists(), "the copy goes when the logo does");
        rename(&home, "dev/thing", "").unwrap();
        assert!(chosen(&home).is_empty());
        let _ = fs::remove_dir_all(&home);
    }
}
