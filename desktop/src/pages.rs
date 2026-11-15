//! The two things the sheet shows: everything that is running, and one project.

use carbon::{
    engine::{App, Project, Snapshot},
    identity::IMAGES,
};
use rune::{Cursor, El, FileDialogOptions, Key, ScrollSpec, Tx, col, el, row, text, text_input};

use crate::{
    Page, State, hub,
    look::{Kind, Look, button, chip, dot, letter, mark, mono, plain_button, sans, tile},
    model::{self, Origin, Server},
};

fn open(port: u16) {
    hub().open(&format!("http://localhost:{port}"));
}

/// Stops a server: by its port when it is on one of Carbon's, by its process elsewhere
fn stop(s: &mut State, port: u16, pid: Option<i32>, slot: bool) {
    match (slot, pid) {
        (true, _) => s.stop(port),
        (false, Some(pid)) => s.kill(port, pid),
        (false, None) => {}
    }
}

/// The top of a page: what it is, in a band that also moves the window
fn heading(title: &str, note: String, l: &Look) -> El {
    col()
        .key("heading")
        .px(32.0)
        .pt(30.0)
        .pb(22.0)
        .gap(4.0)
        .window_drag()
        .child(text(title).font(sans(24.0, 600.0)).color(l.text))
        .child(text(note).font(sans(13.0, 400.0)).color(l.soft))
}

/// The page under its heading, scrolling when it is taller than the window
fn body(key: &str) -> El {
    col().key(key).px(32.0).pb(32.0)
}

fn scrolling(body: El) -> El {
    body.scroll(ScrollSpec {
        stick_bottom: false,
    })
    .grow(1.0)
    .min_h(0.0)
}

/// A picture of what a port is serving, which opens it when pressed. Until one has been taken, the place it goes.
fn picture(s: &State, server: &Server, l: &Look) -> El {
    let port = server.port;
    let frame = row()
        .key(format!("picture-{port}"))
        .center()
        .fill(l.well)
        .cursor(Cursor::Pointer)
        .on_press(move |_: &mut State| open(port));
    match s.shots.get(&port) {
        Some(shot) if server.live() => frame.pixels(shot.pixels()),
        _ => frame.child(dot(server.status, l)),
    }
}

/// Who started a server, in a few words
fn started(server: &Server) -> String {
    match &server.origin {
        Origin::Agent(agent) => match &agent.title {
            Some(title) => format!("{}: {title}", agent.name),
            None => agent.name.to_string(),
        },
        Origin::Carbon => match server.since {
            Some(since) if server.live() => format!("Up {}", model::up(since)),
            _ => "Starting".into(),
        },
        Origin::Hand => "Started by hand".into(),
    }
}

/* ---------- Running: every server on the machine ---------- */

fn server_card(s: &State, snap: &Snapshot, server: &Server, l: &Look) -> El {
    let port = server.port;
    let (pid, slot) = (server.pid, server.slot);
    let project = server.app.and_then(|app| model::project_of(snap, app));
    let title = project.map_or_else(|| server.what.clone(), |project| project.title.clone());
    let address = match server.app.and_then(|app| app.part.as_deref()) {
        Some(part) => format!("{} on localhost:{port}", model::named(part)),
        None => format!("localhost:{port}"),
    };

    let face = match project {
        Some(project) => tile(s, project, 34.0, l),
        None => letter(&title, &title, 34.0, 8.0),
    };
    let mut name = row().gap(11.0).center_y().child(face).child(
        col()
            .grow(1.0)
            .min_w(0.0)
            .gap(1.0)
            .child(text(title).font(sans(14.0, 600.0)).color(l.text).truncate())
            .child(
                text(address)
                    .font(sans(12.5, 400.0))
                    .color(l.soft)
                    .truncate(),
            ),
    );
    // The name leads to the project
    if let Some(project) = project {
        let page = Page::Project(project.key.clone());
        name = name
            .key(format!("to-{}", project.key))
            .cursor(Cursor::Pointer)
            .on_press(move |s: &mut State| s.go(page.clone()));
    }

    let mut who = row().gap(7.0).center_y().grow(1.0).min_w(0.0);
    if let Origin::Agent(agent) = &server.origin {
        who = who.child(mark(s, agent.vendor, 18.0));
    }
    who = who.child(
        text(started(server))
            .font(sans(12.0, 400.0))
            .color(l.faint)
            .truncate()
            .grow(1.0)
            .min_w(0.0),
    );

    col()
        .key(format!("server-{port}"))
        .radius(14.0)
        .continuous_corners()
        .fill(l.card)
        .border(l.hair, l.line)
        .shadow(1.0, 2.0, l.shadow)
        .shadow(10.0, 28.0, l.glow)
        .child(
            picture(s, server, l)
                .w_full()
                .aspect_ratio(1.6)
                .corners([13.0, 13.0, 0.0, 0.0]),
        )
        .child(
            col().padding(14.0).gap(14.0).child(name).child(
                row()
                    .gap(8.0)
                    .center_y()
                    .child(who)
                    .child(button(
                        format!("stop-{port}"),
                        "Stop",
                        Kind::Quiet,
                        s.busy.is_none(),
                        l,
                        move |s: &mut State| stop(s, port, pid, slot),
                    ))
                    .child(button(
                        format!("open-{port}"),
                        "Open",
                        Kind::Main,
                        server.live(),
                        l,
                        move |_: &mut State| open(port),
                    )),
            ),
        )
}

pub fn running(s: &State, snap: &Snapshot, servers: &[Server], across: usize, l: &Look) -> El {
    let up: Vec<&Server> = servers
        .iter()
        .filter(|server| server.status != "exited")
        .collect();
    if up.is_empty() {
        return col()
            .key("running")
            .grow(1.0)
            .window_drag()
            .center()
            .gap(6.0)
            .child(mark(s, "carbon", 44.0))
            .child(el().h(8.0))
            .child(
                text("Nothing is running")
                    .font(sans(17.0, 600.0))
                    .color(l.text),
            )
            .child(
                text("Choose a project on the left and press Run.")
                    .font(sans(13.0, 400.0))
                    .color(l.soft),
            );
    }
    let note = match up.len() {
        1 => "One server on this machine.".to_string(),
        n => format!("{n} servers on this machine."),
    };
    let mut cards = el().key("cards").equal_columns(across).gap(18.0);
    for server in up {
        cards = cards.child(server_card(s, snap, server, l));
    }
    col()
        .key("running")
        .grow(1.0)
        .min_h(0.0)
        .child(heading("Running", note, l))
        .child(scrolling(body("running-body").child(cards)))
}

/* ---------- One project ---------- */

/// Where the code stands: its branch, what is uncommitted, and the last commit
fn standing(app: &App) -> Option<String> {
    let git = app.git.as_ref()?;
    let loose = match git.changes {
        0 => String::new(),
        1 => ", 1 file uncommitted".into(),
        n => format!(", {n} files uncommitted"),
    };
    Some(format!(
        "{}{loose}. Last commit {}: {}",
        git.branch,
        model::ago(git.last),
        git.subject
    ))
}

/// What an app is doing, in a sentence
fn doing(app: &App, mine: &[&Server], servers: &[Server], snap: &Snapshot) -> String {
    let Some(server) = mine.first() else {
        // Something else on its port is worth saying before it is replaced
        let there = servers.iter().find(|other| other.port == app.port);
        return match there {
            Some(other) => {
                let name = other
                    .app
                    .and_then(|a| model::project_of(snap, a))
                    .map_or("something else", |p| p.title.as_str());
                format!(
                    "Runs on localhost:{}, where {name} is now. Running this takes its place.",
                    app.port
                )
            }
            None => format!("Runs on localhost:{}", app.port),
        };
    };
    match server.status {
        "starting" => format!("Starting on localhost:{}", server.port),
        "exited" => "Stopped by itself".into(),
        _ => {
            let mut says = format!("Live on localhost:{}", server.port);
            if let Some(since) = server.since {
                says += &format!(", up {}", model::up(since));
            }
            match &server.origin {
                Origin::Agent(agent) => says += &format!(", started by {}", agent.name),
                Origin::Hand => says += ", started by hand",
                Origin::Carbon => {}
            }
            says
        }
    }
}

/// What a server printed, in a well under its app
fn output(s: &State, l: &Look) -> El {
    let mut lines = col().key("output-lines").padding(14.0).gap(3.0);
    if s.lines.is_empty() {
        lines = lines.child(text("Nothing printed yet.").font(mono(11.5)).color(l.faint));
    }
    for (i, line) in s.lines.iter().enumerate() {
        lines = lines.child(
            text(line.clone())
                .key(format!("line-{i}"))
                .font(mono(11.5))
                .color(l.soft)
                .wrap()
                .selectable(true),
        );
    }
    col()
        .key("output")
        .h(232.0)
        .radius(10.0)
        .fill(l.well)
        .child(
            lines
                .scroll(ScrollSpec { stick_bottom: true })
                .grow(1.0)
                .min_h(0.0),
        )
}

fn app_card(
    s: &State,
    snap: &Snapshot,
    servers: &[Server],
    project: &Project,
    app: &App,
    l: &Look,
) -> El {
    let mine: Vec<&Server> = servers
        .iter()
        .filter(|server| server.app.is_some_and(|a| a.id == app.id))
        .collect();
    let first = mine.first().copied();
    let status = first.map_or("idle", |server| server.status);
    let idle = s.busy.is_none();
    let id = app.id.clone();

    let title = row()
        .gap(9.0)
        .center_y()
        .grow(1.0)
        .min_w(0.0)
        .child(dot(status, l))
        .child(
            text(
                app.part
                    .as_deref()
                    .map_or_else(|| project.title.clone(), model::named),
            )
            .font(sans(15.0, 600.0))
            .color(l.text)
            .truncate(),
        )
        .child(chip(app.kind, l));

    // What can be done with it, on the right, with the thing to press last
    let mut buttons = row().gap(8.0).center_y();
    match first {
        None => {
            let starting = s.busy.as_deref() == Some(format!("start:{id}").as_str());
            let run = id.clone();
            buttons = buttons.child(button(
                format!("run-{id}"),
                if starting { "Starting…" } else { "Run" },
                Kind::Main,
                idle,
                l,
                move |s: &mut State| s.start(run.clone()),
            ));
        }
        Some(server) => {
            let port = server.port;
            let (pid, slot) = (server.pid, server.slot);
            if server.logs {
                let showing = s.output == Some(port);
                buttons = buttons.child(button(
                    format!("output-{port}"),
                    if showing { "Hide output" } else { "Output" },
                    Kind::Quiet,
                    true,
                    l,
                    move |s: &mut State| {
                        s.show_output(if showing { None } else { Some(port) });
                    },
                ));
            }
            if server.status == "exited" {
                let run = id.clone();
                buttons = buttons
                    .child(button(
                        format!("clear-{port}"),
                        "Dismiss",
                        Kind::Quiet,
                        idle,
                        l,
                        move |s: &mut State| s.stop(port),
                    ))
                    .child(button(
                        format!("run-{id}"),
                        "Run again",
                        Kind::Main,
                        idle,
                        l,
                        move |s: &mut State| s.start(run.clone()),
                    ));
            } else {
                buttons = buttons
                    .child(button(
                        format!("stop-{port}"),
                        "Stop",
                        Kind::Quiet,
                        idle,
                        l,
                        move |s: &mut State| stop(s, port, pid, slot),
                    ))
                    .child(button(
                        format!("open-{port}"),
                        "Open",
                        Kind::Main,
                        server.live(),
                        l,
                        move |_: &mut State| open(port),
                    ));
            }
        }
    }

    let mut about = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(6.0)
        .child(row().gap(12.0).center_y().child(title).child(buttons))
        .child(
            text(doing(app, &mine, servers, snap))
                .font(sans(13.0, 400.0))
                .color(l.soft)
                .wrap(),
        );
    if let Some(note) = first.and_then(|server| server.note) {
        about = about.child(
            text(note.to_string())
                .font(mono(11.5))
                .color(l.bad)
                .wrap()
                .max_lines(3),
        );
    }
    if let Some(standing) = standing(app) {
        about = about.child(
            text(standing)
                .font(sans(12.0, 400.0))
                .color(l.faint)
                .truncate(),
        );
    }

    // Once it is live, a picture of it leads the card
    let mut top = row().gap(18.0);
    if let Some(server) = first.filter(|server| server.live()) {
        top = top.child(
            picture(s, server, l)
                .w(176.0)
                .aspect_ratio(1.6)
                .radius(9.0)
                .border(l.hair, l.line),
        );
    }
    top = top.child(about);
    let mut card = col()
        .key(format!("app-{}", app.id))
        .padding(18.0)
        .gap(14.0)
        .radius(14.0)
        .continuous_corners()
        .fill(l.card)
        .border(l.hair, l.line)
        .shadow(1.0, 2.0, l.shadow)
        .shadow(10.0, 28.0, l.glow)
        .child(top);

    // The same app serving somewhere else as well: an agent's copy beside Carbon's, say
    for server in mine.iter().skip(1) {
        let port = server.port;
        let (pid, slot) = (server.pid, server.slot);
        card = card.child(
            row()
                .key(format!("also-{port}"))
                .gap(8.0)
                .center_y()
                .child(dot(server.status, l))
                .child(
                    text(format!("Also on localhost:{port}. {}", started(server)))
                        .font(sans(12.5, 400.0))
                        .color(l.soft)
                        .truncate()
                        .grow(1.0)
                        .min_w(0.0),
                )
                .child(button(
                    format!("stop-{port}"),
                    "Stop",
                    Kind::Quiet,
                    idle,
                    l,
                    move |s: &mut State| stop(s, port, pid, slot),
                ))
                .child(button(
                    format!("open-{port}"),
                    "Open",
                    Kind::Quiet,
                    server.live(),
                    l,
                    move |_: &mut State| open(port),
                )),
        );
    }
    if first.is_some_and(|server| s.output == Some(server.port)) {
        card = card.child(output(s, l));
    }
    card
}

/// The name as a field, and the ways to change the logo
fn changing(s: &State, project: &Project, l: &Look) -> El {
    let field = row()
        .key("rename-field")
        .h(40.0)
        .px(12.0)
        .center_y()
        .radius(10.0)
        .fill(l.well)
        .border(1.5, l.accent)
        .child(
            text_input(&s.rename, sans(20.0, 600.0))
                .key("rename")
                .single_line()
                .grow(1.0)
                .color(l.text)
                .placeholder(project.name.clone(), l.faint)
                .caret(l.accent, l.accent.alpha(0.3))
                .on_edit(|s: &mut State| &mut s.rename)
                .on_submit(|s: &mut State, _: &mut Tx| s.finish())
                // Escape leaves the name as it was
                .on_input_key(|s: &mut State, event, _| {
                    let leaving = event.key == Key::Escape;
                    if leaving {
                        s.editing = false;
                    }
                    leaving
                }),
        );
    let choose = plain_button("logo-choose", "Choose a logo…", Kind::Quiet, l).on_press_tx(
        |_: &mut State, tx: &mut Tx| {
            let options = FileDialogOptions {
                title: "Choose a logo".into(),
                ..Default::default()
            }
            .filter("Images", &IMAGES);
            tx.open_file(options, |s: &mut State, picked, _| match picked {
                Ok(Some(image)) => s.logo(Some(image)),
                Ok(None) => {}
                Err(why) => s.error = Some(why.0),
            });
        },
    );
    let mut ways = row().gap(8.0).center_y().child(choose);
    if project.logo_chosen {
        ways = ways.child(button(
            "logo-own",
            "Use its own logo",
            Kind::Quiet,
            true,
            l,
            |s: &mut State| s.logo(None),
        ));
    }
    if project.renamed {
        ways = ways.child(button(
            "name-own",
            "Use its own name",
            Kind::Quiet,
            true,
            l,
            |s: &mut State| s.own_name(),
        ));
    }
    col().gap(10.0).child(field).child(ways).child(
        text("Or drop an image anywhere on this page.")
            .font(sans(12.0, 400.0))
            .color(l.faint),
    )
}

pub fn project(s: &State, snap: &Snapshot, servers: &[Server], project: &Project, l: &Look) -> El {
    let mut apps: Vec<&App> = project
        .apps
        .iter()
        .filter_map(|id| snap.apps.iter().find(|app| &app.id == id))
        .collect();
    // The site first, then what stands behind it: the order of their ports
    apps.sort_by_key(|app| app.port);
    let resting = apps
        .iter()
        .filter(|app| {
            !servers
                .iter()
                .any(|server| server.app.is_some_and(|a| a.id == app.id))
        })
        .count();

    let words = if s.editing {
        changing(s, project, l)
    } else {
        let mut words = col().gap(5.0).child(
            text(project.title.clone())
                .font(sans(24.0, 600.0))
                .color(l.text)
                .truncate(),
        );
        if let Some(says) = &project.description {
            words = words.child(
                text(says.clone())
                    .font(sans(13.0, 400.0).lh(19.0))
                    .color(l.soft)
                    .wrap()
                    .max_lines(2),
            );
        }
        words.child(
            text(model::folder(&project.group, &project.name))
                .font(sans(12.0, 400.0))
                .color(l.faint)
                .truncate(),
        )
    };

    let mut actions = row().gap(8.0);
    if s.editing {
        actions = actions.child(button(
            "edit-done",
            "Done",
            Kind::Main,
            true,
            l,
            |s: &mut State| s.finish(),
        ));
    } else {
        if apps.len() > 1 && resting > 0 {
            let (key, group, name) = (
                project.key.clone(),
                project.group.clone(),
                project.name.clone(),
            );
            let starting = s.busy.as_deref() == Some(format!("project:{key}").as_str());
            actions = actions.child(button(
                "run-all",
                if starting { "Starting…" } else { "Run all" },
                Kind::Quiet,
                s.busy.is_none(),
                l,
                move |s: &mut State| {
                    s.start_project(key.clone(), group.clone(), name.clone());
                },
            ));
        }
        let title = project.title.clone();
        actions = actions.child(button(
            "edit",
            "Edit",
            Kind::Quiet,
            true,
            l,
            move |s: &mut State| s.edit(&title),
        ));
    }

    let head = row()
        .key("project-head")
        .px(32.0)
        .pt(30.0)
        .pb(24.0)
        .gap(18.0)
        .window_drag()
        .child(tile(s, project, 64.0, l))
        .child(words.grow(1.0).min_w(0.0))
        .child(actions);

    let mut list = body("project-body").gap(14.0);
    for app in apps {
        list = list.child(app_card(s, snap, servers, project, app, l));
    }
    col()
        .key(format!("project-{}", project.key))
        .grow(1.0)
        .min_h(0.0)
        .child(head)
        .child(scrolling(list))
        // An image dropped anywhere on the page becomes the logo
        .on_drop_files(|s: &mut State, files, _| s.logo(files.first().cloned()))
        .accepts_extensions(&IMAGES)
}
