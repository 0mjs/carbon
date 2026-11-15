//! The `carbon` command. With no arguments (or `serve`) it is the hub; every other subcommand asks the running hub
//! to do something and prints the answer.

use std::{env, process::ExitCode, sync::Arc, thread, time::Duration};

use carbon::{
    client::{self, NOT_RUNNING},
    engine::{HUB_PORT, Hub},
    server,
};
use serde_json::{Value, json};

const HELP: &str = "\
carbon: the local hub for your dev servers, always on port 3000

  carbon                  run the hub (the same as `carbon serve`)
  carbon ls               what is on each port, and what else is running
  carbon agents           the servers coding agents have started, by session
  carbon run <app>        start an app on its port, e.g. `carbon run cutwise/web`
                          a project's name starts all of it: `carbon run cutwise`
  carbon stop <app|port>  stop what is on a port
  carbon open <app|port>  open it in the browser
  carbon logs <port> [-f] what a server Carbon started has printed; -f keeps following
  carbon new <folder>/<name> [start|spa|bun]
                          create a project (TanStack Start by default)

Set CARBON_OPEN=0 to stop `carbon serve` opening the dashboard.";

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}

fn app_name(app: &Value) -> String {
    match app["part"].as_str() {
        Some(part) => format!("{} / {part}", text(&app["project"])),
        None => text(&app["project"]).to_string(),
    }
}

/// How long ago a millisecond timestamp was, in the largest unit that fits
fn ago(ms: &Value) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let secs = now.saturating_sub(ms.as_u64().unwrap_or(now)) / 1000;
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        _ => format!("{}h {}m", secs / 3600, (secs % 3600) / 60),
    }
}

/// The apps a word could mean: an exact path first, then the end of a path, then anything containing it
fn matching<'a>(apps: &'a [Value], word: &str) -> Vec<&'a Value> {
    let id = |a: &&Value| text(&a["id"]).to_string();
    for test in [
        (|id: &str, w: &str| id == w) as fn(&str, &str) -> bool,
        |id, w| id.ends_with(&format!("/{w}")),
        |id, w| id.contains(w),
    ] {
        let found: Vec<&Value> = apps.iter().filter(|a| test(&id(a), word)).collect();
        if !found.is_empty() {
            return found;
        }
    }
    Vec::new()
}

/// Asks the hub to open a port in the browser it is set up to use
fn open(port: u64) -> Result<(), String> {
    client::post("/api/open", json!({ "port": port })).map(|_| ())
}

fn ls() -> Result<(), String> {
    let state = client::get("/api/state")?;
    let apps = state["apps"].as_array().cloned().unwrap_or_default();
    let name_of = |id: &Value| apps.iter().find(|a| a["id"] == *id).map(app_name);
    println!("{:<6}{:<10}{:<28}{}", "PORT", "STATE", "WHAT", "BY");
    for slot in state["slots"].as_array().into_iter().flatten() {
        let what = name_of(&slot["app"])
            .or_else(|| slot["holder"].as_str().map(String::from))
            .unwrap_or_else(|| "-".into());
        let state = match text(&slot["status"]) {
            "running" => "live",
            "idle" => "free",
            other => other,
        };
        let note = slot["note"]
            .as_str()
            .map(|n| format!("  {n}"))
            .unwrap_or_default();
        println!(
            "{:<6}{:<10}{:<28}{}{note}",
            slot["port"].to_string(),
            state,
            what,
            slot["by"].as_str().unwrap_or("")
        );
    }
    for other in state["others"].as_array().into_iter().flatten() {
        let what = name_of(&other["app"]).unwrap_or_else(|| text(&other["command"]).to_string());
        println!(
            "{:<6}{:<10}{:<28}by hand",
            other["port"].to_string(),
            "live",
            what
        );
    }
    for agent in state["agents"].as_array().into_iter().flatten() {
        for server in agent["servers"].as_array().into_iter().flatten() {
            for port in server["ports"].as_array().into_iter().flatten() {
                let what = name_of(&server["app"])
                    .unwrap_or_else(|| text(&server["command"]).chars().take(26).collect());
                println!(
                    "{:<6}{:<10}{:<28}{}",
                    port.to_string(),
                    "live",
                    what,
                    text(&agent["name"])
                );
            }
        }
    }
    Ok(())
}

fn agents() -> Result<(), String> {
    let state = client::get("/api/state")?;
    let agents = state["agents"].as_array().cloned().unwrap_or_default();
    if agents.is_empty() {
        println!("No coding agent has a server running.");
    }
    for agent in agents {
        let title = agent["title"].as_str().unwrap_or(text(&agent["name"]));
        println!(
            "{title}  ({}, {})",
            text(&agent["name"]),
            agent["cwd"].as_str().unwrap_or("?")
        );
        for server in agent["servers"].as_array().into_iter().flatten() {
            let ports: Vec<String> = server["ports"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|p| format!(":{p}"))
                .collect();
            println!(
                "  {:<12} up {:<8} {}",
                ports.join(" "),
                ago(&server["since"]),
                text(&server["command"])
            );
        }
    }
    Ok(())
}

fn run(word: &str) -> Result<(), String> {
    let state = client::get("/api/state")?;
    let apps = state["apps"].as_array().cloned().unwrap_or_default();
    // A project's name means all of it
    let project: Vec<&Value> = apps
        .iter()
        .filter(|a| text(&a["project"]) == word)
        .collect();
    if project.len() > 1 {
        let first = project[0];
        let reply = client::post(
            "/api/start-project",
            json!({ "group": first["group"], "project": first["project"] }),
        )?;
        for (app, port) in project
            .iter()
            .zip(reply["ports"].as_array().into_iter().flatten())
        {
            println!("{}  http://localhost:{port}", app_name(app));
        }
        return Ok(());
    }
    match matching(&apps, word).as_slice() {
        [] => Err(format!(
            "No project matches \"{word}\". `carbon ls` shows what is running; the dashboard lists every app."
        )),
        [app] => {
            let port = client::post("/api/start", json!({ "id": app["id"] }))?["port"].clone();
            println!("{}  http://localhost:{port}", app_name(app));
            Ok(())
        }
        many => Err(format!(
            "\"{word}\" could be any of:\n{}",
            many.iter()
                .map(|a| format!("  {}", text(&a["id"])))
                .collect::<Vec<_>>()
                .join("\n")
        )),
    }
}

/// The port a word names: a number as it stands, or the port an app is running on
fn port_of(word: &str) -> Result<u64, String> {
    if let Ok(port) = word.parse::<u64>() {
        return Ok(port);
    }
    let state = client::get("/api/state")?;
    let apps = state["apps"].as_array().cloned().unwrap_or_default();
    let ids: Vec<&str> = matching(&apps, word)
        .iter()
        .map(|a| text(&a["id"]))
        .collect();
    let slots = state["slots"].as_array().cloned().unwrap_or_default();
    let on: Vec<u64> = slots
        .iter()
        .filter(|s| text(&s["status"]) != "idle" && ids.contains(&text(&s["app"])))
        .filter_map(|s| s["port"].as_u64())
        .collect();
    match on.as_slice() {
        [port] => Ok(*port),
        [] => Err(format!(
            "Nothing matching \"{word}\" is running on a Carbon port."
        )),
        _ => Err(format!(
            "More than one thing matching \"{word}\" is running: name the port."
        )),
    }
}

fn logs(word: &str, follow: bool) -> Result<(), String> {
    let port = port_of(word)?;
    let mut shown = 0;
    loop {
        let lines = client::get(&format!("/api/logs/{port}"))?;
        let lines = lines.as_array().cloned().unwrap_or_default();
        // The hub keeps a rolling window: if it has rolled past us, start again from what it has
        if lines.len() < shown {
            shown = 0;
        }
        for line in &lines[shown..] {
            println!("{}", text(line));
        }
        shown = lines.len();
        if !follow {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(700));
    }
}

fn new(target: &str, template: &str) -> Result<(), String> {
    let (group, name) = target
        .rsplit_once('/')
        .ok_or("Say where and what: carbon new <folder>/<name>, e.g. carbon new dev/thing")?;
    let job = client::post(
        "/api/create",
        json!({ "group": group, "name": name, "template": template }),
    )?["job"]
        .clone();
    let key = text(&job).to_string();
    // Follow the job's output until it finishes
    let mut shown = 0;
    loop {
        let lines = client::get(&format!("/api/logs/{key}"))?
            .as_array()
            .cloned()
            .unwrap_or_default();
        for line in lines.iter().skip(shown) {
            println!("{}", text(line));
        }
        shown = lines.len();
        let state = client::get("/api/state")?;
        let status = state["jobs"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|j| j["key"] == job)
            .map(|j| text(&j["status"]).to_string());
        match status.as_deref() {
            Some("running") => thread::sleep(Duration::from_millis(500)),
            Some("done") => return Ok(println!("Created ~/{group}/{name}")),
            _ => return Err(format!("Creating ~/{group}/{name} failed.")),
        }
    }
}

async fn serve() -> Result<(), String> {
    if client::running() {
        return Err(format!(
            "Carbon is already running on http://localhost:{HUB_PORT}"
        ));
    }
    let hub = Arc::new(Hub::new());
    server::listen(hub.clone()).await?;
    println!(
        "Carbon is on http://localhost:{HUB_PORT}{}",
        if hub.mise.is_some() {
            ""
        } else {
            " (mise not found: servers run without it)"
        }
    );
    // Bring the dashboard up in the browser. CARBON_OPEN=0 skips it
    if env::var("CARBON_OPEN").as_deref() != Ok("0") {
        hub.open(&format!("http://localhost:{HUB_PORT}"));
    }
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|e| e.to_string())?;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
    hub.shutdown();
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str);
    let needs = |what: &str| format!("Say which: carbon {} <{what}>", arg(0).unwrap_or(""));
    let result = match (arg(0), arg(1)) {
        (None | Some("serve"), _) => serve().await,
        (Some("ls" | "status" | "ps"), _) => ls(),
        (Some("agents"), _) => agents(),
        (Some("run" | "start"), Some(app)) => run(app),
        (Some("stop"), Some(what)) => port_of(what).and_then(|port| {
            client::post("/api/stop", json!({ "port": port })).map(|_| println!("Stopped {port}"))
        }),
        (Some("open"), Some(what)) => port_of(what).and_then(open),
        (Some("open"), None) => client::post("/api/open", json!({ "port": HUB_PORT })).map(|_| ()),
        (Some("logs"), Some(what)) => logs(what, args.iter().any(|a| a == "-f" || a == "--follow")),
        (Some("new"), Some(target)) => new(target, arg(2).unwrap_or("start")),
        (Some("run" | "start" | "stop" | "logs"), None) => Err(needs("app or port")),
        (Some("new"), None) => Err(needs("group>/<name")),
        (Some("help" | "-h" | "--help"), _) => Ok(println!("{HELP}")),
        (Some("version" | "-V" | "--version"), _) => {
            Ok(println!("carbon {}", env!("CARGO_PKG_VERSION")))
        }
        (Some(other), _) => Err(format!("carbon doesn't know \"{other}\".\n\n{HELP}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            // Not running is its own code, so a script can tell "nothing to ask" from "it said no"
            if message == NOT_RUNNING {
                ExitCode::from(3)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}
