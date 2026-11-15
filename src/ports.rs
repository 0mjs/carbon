//! Each app has a port of its own.
//!
//! The first time Carbon sees an app it picks a port for it at random and remembers it in
//! `~/.config/carbon/ports.json`, so the app is always at the same address and no two apps want the same one.
//! A port is only ever used after checking that nothing is on it; if something is, the app is given another.

use std::{
    collections::HashMap,
    fs,
    hash::{BuildHasher, Hasher},
    net::TcpListener,
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

/// Where apps' ports come from: four figures to read and type, clear of the ports tools pick by default
/// (3000, 5173, 8000, 8080…)
pub const RANGE: RangeInclusive<u16> = 4100..=4999;
/// Ports in the range that well-known tools use by themselves, left for them: Vite's preview, Angular, Astro,
/// Selenium, Sinatra
const SPOKEN_FOR: [u16; 6] = [4173, 4200, 4321, 4443, 4444, 4567];

fn file(home: &Path) -> PathBuf {
    home.join(".config/carbon/ports.json")
}

/// The port each app was given, by the app's id. A missing or broken file is the same as an empty one.
pub fn kept(home: &Path) -> HashMap<String, u16> {
    fs::read_to_string(file(home))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn keep(home: &Path, ports: &HashMap<String, u16>) {
    let path = file(home);
    let sorted: std::collections::BTreeMap<_, _> = ports.iter().collect();
    if let (Some(folder), Ok(text)) = (path.parent(), serde_json::to_string_pretty(&sorted)) {
        let _ = fs::create_dir_all(folder);
        let _ = fs::write(path, text + "\n");
    }
}

/// Whether nothing is listening on a port, on this machine's own addresses or on all of them. A server can be on
/// either, and a check of one doesn't see the other, so each is tried.
pub fn free(port: u16) -> bool {
    ["127.0.0.1", "::1", "0.0.0.0", "::"]
        .iter()
        .all(|address| TcpListener::bind((*address, port)).is_ok())
}

/// A number nobody could predict, from the standard library's own source of randomness
fn random() -> u64 {
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}

/// A port for an app that has none: at random from the range, not one another app has, and free right now.
/// After enough misses it walks the range in order, so it finds the last free port if there is one.
pub fn pick(taken: &dyn Fn(u16) -> bool) -> Option<u16> {
    let size = u64::from(RANGE.end() - RANGE.start()) + 1;
    let usable = |port: &u16| !SPOKEN_FOR.contains(port) && !taken(*port) && free(*port);
    (0..64)
        .map(|_| RANGE.start() + (random() % size) as u16)
        .find(usable)
        .or_else(|| RANGE.clone().find(usable))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_a_free_port_nobody_has_and_sees_one_in_use() {
        let port = pick(&|_| false).expect("a free port in the range");
        assert!(RANGE.contains(&port) && !SPOKEN_FOR.contains(&port));

        // Something listening on it, on either kind of address, makes it taken
        let held = TcpListener::bind(("127.0.0.1", port)).unwrap();
        assert!(!free(port));
        drop(held);
        let held = TcpListener::bind(("0.0.0.0", port)).unwrap();
        assert!(!free(port));
        drop(held);
        assert!(free(port));

        // With every port but one spoken for by other apps, that one is found
        let only = *RANGE.end();
        if free(only) {
            assert_eq!(pick(&|port| port != only), Some(only));
        }
    }

    #[test]
    fn remembers_what_each_app_was_given() {
        let home = std::env::temp_dir().join(format!("carbon-ports-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        assert!(kept(&home).is_empty());
        keep(&home, &HashMap::from([("work/shop/web".to_string(), 4242)]));
        assert_eq!(kept(&home).get("work/shop/web"), Some(&4242));
        let _ = fs::remove_dir_all(&home);
    }
}
