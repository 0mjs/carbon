//! What the window shows: the rack of ports, the servers agents started, servers started by hand, the projects,
//! and the output of whichever server is selected. It is the same information as the page at localhost:3000,
//! drawn natively.

use std::time::{SystemTime, UNIX_EPOCH};

use carbon::engine::{Agent, App, Project, Slot, Snapshot};
use rune::{Color, Cx, El, ScrollSpec, TextStyle, col, color, el, row, spacer, text, tok::Family};

use crate::{State, hub};

/// Carbon's own palette: near-black, hairlines, one green for "live"
struct Look {
    bg: Color,
    panel: Color,
    line: Color,
    line_strong: Color,
    fg: Color,
    muted: Color,
    faint: Color,
    on: Color,
    warn: Color,
    bad: Color,
}

fn look() -> Look {
    let hex = |s: &str| color::hex(s).expect("a colour written as hex");
    Look {
        bg: hex("#000000"),
        panel: hex("#0a0a0a"),
        line: hex("#1f1f1f"),
        line_strong: hex("#2e2e2e"),
        fg: hex("#ededed"),
        muted: hex("#a1a1a1"),
        faint: hex("#6b6b6b"),
        on: hex("#3ddc84"),
        warn: hex("#f5a524"),
        bad: hex("#e5484d"),
    }
}

const fn sans(size: f32, weight: f32) -> TextStyle {
    TextStyle::new(size, weight)
}
const fn mono(size: f32) -> TextStyle {
    TextStyle::new(size, 400.0).family(Family::SystemMono)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// How long something has been up, in the largest unit that fits
fn up(since: u64) -> String {
    let secs = now().saturating_sub(since) / 1000;
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        _ => format!("{}h {}m", secs / 3600, (secs % 3600) / 60),
    }
}

/// How long ago something happened
fn ago(when: u64) -> String {
    let secs = now().saturating_sub(when) / 1000;
    match secs {
        0..3600 => format!("{}m ago", (secs / 60).max(1)),
        3600..86_400 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86_400),
    }
}

fn app_name(app: &App) -> String {
    match &app.part {
        Some(part) => format!("{} / {part}", app.project),
        None => app.project.clone(),
    }
}

/// A small light: off, amber while starting, green once answering, red if it fell over
fn led(status: &str, l: &Look) -> El {
    let fill = match status {
        "running" => l.on,
        "starting" => l.warn,
        "exited" => l.bad,
        _ => l.line_strong,
    };
    el().size(7.0, 7.0).radius(3.5).fill(fill)
}

/// A button. `solid` is the one to press; a disabled button is dimmed and does nothing.
fn button(
    key: String,
    label: &str,
    solid: bool,
    enabled: bool,
    l: &Look,
    press: impl Fn(&mut State) + 'static,
) -> El {
    let mut b = row().key(key).h(26.0).px(10.0).center().radius(6.0).child(
        text(label)
            .font(mono(11.0))
            .color(if solid { l.bg } else { l.muted }),
    );
    b = if solid {
        b.fill(l.fg)
    } else {
        b.border(1.0, l.line)
    };
    if enabled {
        b.hover(if solid { l.muted } else { l.panel })
            .on_press(press)
    } else {
        b.opacity(0.4)
    }
}

fn tag(label: &str, l: &Look) -> El {
    row()
        .h(18.0)
        .px(6.0)
        .center()
        .radius(4.0)
        .border(1.0, l.line)
        .child(text(label).font(mono(10.0)).color(l.faint))
}

/// A picture of what a port is serving, or an empty pane until one has been taken
fn picture(s: &State, port: u16, l: &Look) -> El {
    let pane = el().radius(6.0).border(1.0, l.line).fill(l.panel);
    match s.shots.get(&port) {
        Some(shot) => pane.pixels(shot.pixels()),
        None => pane,
    }
}

fn mark(s: &State, name: &str, size: f32) -> El {
    match s.marks.get(name) {
        Some(image) => el().size(size, size).pixels(image.pixels()),
        None => el().size(size, size),
    }
}

/* ---------- The rack: one tile per port ---------- */

fn tile(s: &State, snap: &Snapshot, slot: &Slot, pictures: bool, l: &Look) -> El {
    let live = slot.status == "running";
    let app = slot
        .app
        .as_ref()
        .and_then(|id| snap.apps.iter().find(|a| &a.id == id));
    let word = match slot.status {
        "running" => "Live",
        "starting" => "Starting",
        "exited" => "Stopped",
        _ => "Free",
    };
    let port = slot.port;
    let key = port.to_string();
    let idle = s.busy.is_none();

    let mut head = row()
        .center_y()
        .gap(6.0)
        .child(text(slot.label).font(mono(11.0)).color(l.faint));
    if let Some(by) = slot.by {
        head = head.child(mark(s, by, 13.0));
    }
    head = head.child(spacer());
    if let (true, Some(since)) = (live, slot.since) {
        head = head.child(text(up(since)).font(mono(11.0)).color(l.faint));
    }
    head = head
        .child(text(word).font(mono(11.0)).color(l.muted))
        .child(led(slot.status, l));

    let what = match (app, &slot.holder) {
        (Some(app), _) => text(app_name(app)).color(l.fg),
        (None, Some(holder)) => text(holder.clone()).color(l.fg),
        (None, None) => text("Nothing here").color(l.faint),
    };
    let showing = s.console.as_deref() == Some(key.as_str());

    let mut tile = col()
        .key(format!("port-{port}"))
        .grow(1.0)
        .min_w(0.0)
        .padding(16.0)
        .gap(14.0)
        .border_sides([0.0, 1.0, 1.0, 0.0], l.line)
        .child(head);
    // Pictures appear for the whole rack once anything is live, so the tiles stay one height
    if pictures {
        tile = tile.child(picture(s, port, l).w_full().aspect_ratio(1.6));
    }
    tile.child(
        col()
            .gap(6.0)
            .child(
                text(port.to_string())
                    .font(mono(30.0))
                    .color(if live { l.fg } else { l.faint }),
            )
            .child(what.font(sans(13.0, 400.0)).truncate())
            // Why it stopped, when it stopped by itself. The line is always there, so the tiles stay level
            .child(
                text(slot.note.clone().unwrap_or_default())
                    .font(mono(11.0))
                    .color(l.bad)
                    .truncate()
                    .h(14.0),
            ),
    )
    .child(
        row()
            .gap(6.0)
            .child(
                button(
                    format!("open-{port}"),
                    "Open",
                    live,
                    live,
                    l,
                    move |_: &mut State| hub().open(&format!("http://localhost:{port}")),
                )
                .grow(1.0),
            )
            .child(
                button(
                    format!("output-{port}"),
                    "Output",
                    false,
                    slot.has_logs,
                    l,
                    move |s: &mut State| {
                        s.show_output(if showing {
                            None
                        } else {
                            Some(port.to_string())
                        });
                    },
                )
                .grow(1.0),
            )
            .child(
                button(
                    format!("stop-{port}"),
                    if slot.status == "exited" {
                        "Clear"
                    } else {
                        "Stop"
                    },
                    false,
                    slot.status != "idle" && idle,
                    l,
                    move |s: &mut State| s.stop(port),
                )
                .grow(1.0),
            ),
    )
}

/* ---------- Servers: one row for anything serving off the rack ---------- */

fn heading(title: &str, note: &str, l: &Look) -> El {
    row()
        .px(16.0)
        .h(44.0)
        .center_y()
        .gap(8.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], l.line)
        .child(text(title).font(sans(13.0, 500.0)).color(l.fg))
        .child(text(note).font(sans(13.0, 400.0)).color(l.faint))
}

fn server_row(
    s: &State,
    key: String,
    port: u16,
    name: String,
    detail: String,
    pid: i32,
    l: &Look,
) -> El {
    let idle = s.busy.is_none();
    row()
        .key(key)
        .px(16.0)
        .py(8.0)
        .gap(14.0)
        .center_y()
        .child(picture(s, port, l).size(72.0, 45.0))
        .child(text(port.to_string()).font(mono(13.0)).color(l.fg).w(48.0))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(text(name).font(sans(13.0, 400.0)).color(l.fg).truncate())
                .child(text(detail).font(mono(11.0)).color(l.faint).truncate()),
        )
        .child(button(
            format!("open-{port}"),
            "Open",
            true,
            true,
            l,
            move |_: &mut State| hub().open(&format!("http://localhost:{port}")),
        ))
        .child(button(
            format!("kill-{pid}"),
            "Stop",
            false,
            idle,
            l,
            move |s: &mut State| s.kill(pid),
        ))
}

fn agent(s: &State, snap: &Snapshot, agent: &Agent, l: &Look) -> El {
    let mut head = row()
        .px(16.0)
        .pt(12.0)
        .pb(4.0)
        .gap(10.0)
        .center_y()
        .child(mark(s, agent.vendor, 16.0))
        .child(
            text(
                agent
                    .title
                    .clone()
                    .unwrap_or_else(|| agent.name.to_string()),
            )
            .font(sans(13.0, 500.0))
            .color(l.fg)
            .truncate(),
        );
    head = head.child(tag(agent.name, l));
    if let Some(status) = &agent.status {
        let working = status == "busy";
        head = head
            .child(led(if working { "starting" } else { "idle" }, l))
            .child(
                text(if working { "working" } else { "waiting" })
                    .font(mono(11.0))
                    .color(l.faint),
            );
    }
    if let Some(cwd) = &agent.cwd {
        head = head.child(text(cwd.clone()).font(mono(11.0)).color(l.faint).truncate());
    }
    let mut block = col()
        .key(format!("agent-{}", agent.pid))
        .border_sides([0.0, 0.0, 1.0, 0.0], l.line)
        .pb(6.0)
        .child(head);
    for server in &agent.servers {
        let Some(&port) = server.ports.first() else {
            continue;
        };
        let app = server
            .app
            .as_ref()
            .and_then(|id| snap.apps.iter().find(|a| &a.id == id));
        let name = match app {
            Some(app) => format!("{}   up {}", app_name(app), up(server.since)),
            None => format!(
                "{}   up {}",
                server.cwd.clone().unwrap_or_default(),
                up(server.since)
            ),
        };
        block = block.child(server_row(
            s,
            format!("server-{}", server.pid),
            port,
            name,
            server.command.clone(),
            server.pid,
            l,
        ));
    }
    block
}

/* ---------- Projects ---------- */

fn app_row(s: &State, snap: &Snapshot, app: &App, l: &Look) -> El {
    let slot = snap
        .slots
        .iter()
        .find(|slot| slot.app.as_deref() == Some(app.id.as_str()) && slot.status != "idle");
    let idle = s.busy.is_none();
    let id = app.id.clone();
    let mut title = row()
        .gap(8.0)
        .center_y()
        .child(led(slot.map_or("idle", |slot| slot.status), l))
        .child(
            text(app.part.clone().unwrap_or_else(|| app.project.clone()))
                .font(sans(13.0, 500.0))
                .color(l.fg)
                .truncate(),
        );
    title = title.child(tag(app.kind, l));
    if app.pinned {
        title = title.child(tag("pinned", l));
    }
    // Where the code stands: one line, always there, so rows stay the same height
    let stands = match &app.git {
        Some(git) => {
            let changes = if git.changes > 0 {
                format!(", {} uncommitted", git.changes)
            } else {
                String::new()
            };
            format!(
                "{}, {}{changes}: {}",
                git.branch,
                ago(git.last),
                git.subject
            )
        }
        None => app.command.clone(),
    };
    let action = match slot {
        Some(slot) => {
            let port = slot.port;
            button(
                format!("stop-app-{id}"),
                "Stop",
                false,
                idle,
                l,
                move |s: &mut State| s.stop(port),
            )
        }
        None => {
            let starting = s.busy.as_deref() == Some(format!("start:{id}").as_str());
            let run = id.clone();
            button(
                format!("run-{id}"),
                if starting { "…" } else { "Run" },
                true,
                idle,
                l,
                move |s: &mut State| s.start(run.clone()),
            )
        }
    };
    row()
        .key(format!("app-{}", app.id))
        .py(8.0)
        .gap(12.0)
        .center_y()
        .child(
            col().grow(1.0).min_w(0.0).gap(3.0).child(title).child(
                text(stands)
                    .font(mono(11.0))
                    .color(l.faint)
                    .truncate()
                    .pl(15.0),
            ),
        )
        .child(text(app.port.to_string()).font(mono(12.0)).color(l.muted))
        .child(action.w(56.0))
}

fn card(s: &State, snap: &Snapshot, project: &Project, l: &Look) -> El {
    let apps: Vec<&App> = project
        .apps
        .iter()
        .filter_map(|id| snap.apps.iter().find(|a| &a.id == id))
        .collect();
    let live = apps
        .iter()
        .filter(|a| {
            snap.slots
                .iter()
                .any(|slot| slot.app.as_deref() == Some(a.id.as_str()) && slot.status == "running")
        })
        .count();

    let mut name = row().gap(8.0).center_y().child(
        text(project.name.clone())
            .font(sans(15.0, 500.0))
            .color(l.fg)
            .truncate(),
    );
    if live > 0 {
        name = name.child(text(format!("{live} live")).font(mono(11.0)).color(l.on));
    }
    let mut head = row().gap(12.0).child(
        col().grow(1.0).min_w(0.0).gap(3.0).child(name).child(
            text(format!("~/{}", project.group))
                .font(mono(11.0))
                .color(l.faint)
                .truncate(),
        ),
    );
    if apps.len() > 1 {
        let (group, project_name) = (project.group.clone(), project.name.clone());
        head = head.child(button(
            format!("all-{}", project.key),
            &format!("Run all {}", apps.len()),
            false,
            s.busy.is_none(),
            l,
            move |s: &mut State| {
                s.start_project(group.clone(), project_name.clone());
            },
        ));
    }
    // What it says it is. Two lines kept whether or not it says anything, so cards in a row line up
    let says = text(project.description.clone().unwrap_or_default())
        .font(sans(13.0, 400.0).lh(20.0))
        .color(l.muted)
        .wrap()
        .max_lines(2)
        .h(40.0);
    let mut rows = col().border_sides([1.0, 0.0, 0.0, 0.0], l.line).pt(4.0);
    for app in apps {
        rows = rows.child(app_row(s, snap, app, l));
    }
    col()
        .key(format!("project-{}", project.key))
        .grow(1.0)
        .w_frac(0.5)
        .min_w(0.0)
        .padding(16.0)
        .gap(10.0)
        .border_sides([0.0, 1.0, 1.0, 0.0], l.line)
        .child(head)
        .child(says)
        .child(rows)
}

/* ---------- The console: output of a port or a job, docked at the bottom ---------- */

fn console(s: &State, key: &str, l: &Look) -> El {
    let mut lines = col().key("console-lines").px(16.0).py(12.0).gap(3.0);
    if s.lines.is_empty() {
        lines = lines.child(text("No output yet.").font(mono(11.5)).color(l.faint));
    }
    for (i, line) in s.lines.iter().enumerate() {
        lines = lines.child(
            text(line.clone())
                .key(format!("line-{i}"))
                .font(mono(11.5))
                .color(l.muted)
                .wrap(),
        );
    }
    col()
        .key("console")
        .h(240.0)
        .fill(l.panel)
        .border_sides([1.0, 0.0, 0.0, 0.0], l.line_strong)
        .child(
            row()
                .h(40.0)
                .px(16.0)
                .center_y()
                .gap(8.0)
                .border_sides([0.0, 0.0, 1.0, 0.0], l.line)
                .child(text(format!("Output  {key}")).font(mono(11.0)).color(l.fg))
                .child(spacer())
                .child(button(
                    "console-close".into(),
                    "Close",
                    false,
                    true,
                    l,
                    |s: &mut State| s.show_output(None),
                )),
        )
        .child(
            lines
                .scroll(ScrollSpec { stick_bottom: true })
                .grow(1.0)
                .min_h(0.0),
        )
}

/* ---------- The window ---------- */

pub fn view(s: &State, _: &mut Cx<State>) -> El {
    let l = look();
    let Some(snap) = &s.snap else {
        return col().w_full().h_full().center().fill(l.bg).child(
            text("Looking at what is running…")
                .font(mono(12.0))
                .color(l.faint),
        );
    };

    let live = snap
        .slots
        .iter()
        .filter(|slot| slot.status == "running")
        .count()
        + snap.others.len()
        + snap.agents.iter().map(|a| a.servers.len()).sum::<usize>();
    let header = row()
        .h(46.0)
        .px(16.0)
        .gap(10.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], l.line)
        .child(mark(s, "carbon", 18.0))
        .child(text("Carbon").font(sans(14.0, 600.0)).color(l.fg))
        .child(text("3000").font(mono(11.0)).color(l.faint))
        .child(spacer())
        .child(
            text(format!("{live} live of {}", snap.apps.len()))
                .font(mono(11.0))
                .color(l.muted),
        );

    let pictures = snap.slots.iter().any(|slot| slot.status == "running");
    let mut rack = row().key("rack").border_sides([0.0, 0.0, 0.0, 0.0], l.line);
    for slot in &snap.slots {
        rack = rack.child(tile(s, snap, slot, pictures, &l));
    }

    let mut page = col().key("page").child(rack);

    if !snap.agents.is_empty() {
        page = page.child(heading(
            "From agents",
            "servers a coding agent started, by session",
            &l,
        ));
        for a in &snap.agents {
            page = page.child(agent(s, snap, a, &l));
        }
    }

    if !snap.others.is_empty() {
        page = page.child(heading(
            "Elsewhere",
            "dev servers started by hand, outside Carbon",
            &l,
        ));
        let mut list = col().border_sides([0.0, 0.0, 1.0, 0.0], l.line).py(4.0);
        for other in &snap.others {
            let app = other
                .app
                .as_ref()
                .and_then(|id| snap.apps.iter().find(|a| &a.id == id));
            let name = app.map(app_name).unwrap_or_else(|| other.command.clone());
            let detail = format!(
                "{}, pid {}, {}",
                other.command,
                other.pid,
                other.cwd.clone().unwrap_or_default()
            );
            list = list.child(server_row(
                s,
                format!("other-{}", other.pid),
                other.port,
                name,
                detail,
                other.pid,
                &l,
            ));
        }
        page = page.child(list);
    }

    page = page.child(heading(
        "Projects",
        "running an app puts it on its port and clears what was there",
        &l,
    ));
    for pair in snap.projects.chunks(2) {
        let mut line = row().key(format!("row-{}", pair[0].key));
        for project in pair {
            line = line.child(card(s, snap, project, &l));
        }
        // A blank cell finishes an odd last row, so the last card keeps its width
        if pair.len() == 1 {
            line = line.child(el().grow(1.0).w_frac(0.5));
        }
        page = page.child(line);
    }

    let mut window = col().w_full().h_full().fill(l.bg).child(header);
    if let Some(why) = &s.error {
        window = window.child(
            row()
                .px(16.0)
                .py(8.0)
                .fill(l.panel)
                .border_sides([0.0, 0.0, 1.0, 0.0], l.line)
                .child(text(why.clone()).font(mono(12.0)).color(l.bad).wrap()),
        );
    }
    window = window.child(
        page.scroll(ScrollSpec {
            stick_bottom: false,
        })
        .grow(1.0)
        .min_h(0.0),
    );
    if let Some(key) = &s.console {
        window = window.child(console(s, key, &l));
    }
    window
}
