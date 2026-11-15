//! The two things the sheet shows most: everything that is running, and one project.

use carbon::{
    engine::{Agent, App, Project, Snapshot},
    identity::IMAGES,
};
use rune::{
    Cursor, El, FileDialogOptions, Key, ScrollSpec, SurfaceFit, Tx, col, color::oklch, el, row,
    text, text_input,
};

use crate::{
    Page, State, hub,
    look::{
        Kind, Look, agent_mark, aura, button, chip, dot, hue, letter, lit, mark, mono,
        plain_button, raised, sans, tech_chip, tech_mark, tile, tone,
    },
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

/// The page under its heading, scrolling when it is taller than the window
fn scrolling(body: El) -> El {
    body.px(32.0)
        .pb(32.0)
        .scroll(ScrollSpec {
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

/// An agent and the app it is running in: "Claude Code in Zed"
fn by(agent: &Agent) -> String {
    match &agent.host {
        Some(host) => format!("{} in {}", agent.name, host.name),
        None => agent.name.to_string(),
    }
}

/// Who started a server, in a few words
fn started(server: &Server) -> String {
    match &server.origin {
        Origin::Agent(agent) => format!("Started by {}", by(agent)),
        Origin::Carbon => match server.since {
            Some(since) if server.live() => format!("Up {}", model::up(since)),
            _ => "Starting".into(),
        },
        Origin::Hand => "Started by hand".into(),
    }
}

/// Who started a server, as two lines beside the agent's mark: the session's name, then the harness, the app it
/// is running in and the model. Anything not started by an agent says so in one line.
fn who(s: &State, server: &Server, l: &Look) -> El {
    let block = row().gap(10.0).center_y().min_h(36.0);
    let Origin::Agent(agent) = &server.origin else {
        return block.child(
            text(started(server))
                .font(sans(12.5, 400.0))
                .color(l.faint)
                .truncate(),
        );
    };
    let detail = match &agent.model {
        Some(model) => format!("{}, {model}", by(agent)),
        None => by(agent),
    };
    let mut lines = col().grow(1.0).min_w(0.0).gap(1.0);
    match &agent.title {
        Some(title) => {
            lines = lines
                .child(
                    text(title.clone())
                        .font(sans(12.5, 500.0))
                        .color(l.ink)
                        .truncate(),
                )
                .child(
                    text(detail)
                        .font(sans(12.0, 400.0))
                        .color(l.faint)
                        .truncate(),
                );
        }
        None => lines = lines.child(text(detail).font(sans(12.5, 500.0)).color(l.ink).truncate()),
    }
    block.child(agent_mark(s, agent, 28.0, l)).child(lines)
}

/* ---------- Running: every server on the machine ---------- */

fn server_card(s: &State, snap: &Snapshot, server: &Server, l: &Look) -> El {
    let port = server.port;
    let (pid, slot) = (server.pid, server.slot);
    let project = server.app.and_then(|app| model::project_of(snap, app));
    // Something Carbon doesn't know as a project goes by the folder it runs in
    let title = project.map_or_else(
        || {
            server
                .what
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or(&server.what)
                .to_string()
        },
        |project| project.title.clone(),
    );
    let about = match (project, server.app.and_then(|app| app.part.as_deref())) {
        (_, Some(part)) => model::named(part),
        (Some(_), None) => String::new(),
        (None, None) => server.what.clone(),
    };
    let face = match project {
        Some(project) => tile(s, project, 34.0, l),
        None => letter(&title, &title, 34.0, 8.0),
    };
    let mut words = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(1.0)
        .child(text(title).font(sans(14.0, 600.0)).color(l.ink).truncate());
    if !about.is_empty() {
        words = words.child(text(about).font(sans(12.5, 400.0)).color(l.soft).truncate());
    }
    let mut name = row()
        .gap(11.0)
        .center_y()
        .grow(1.0)
        .min_w(0.0)
        .child(face)
        .child(words);
    // The name leads to the project
    if let Some(project) = project {
        let page = Page::Project(project.key.clone());
        name = name
            .key(format!("to-{}", project.key))
            .cursor(Cursor::Pointer)
            .on_press(move |s: &mut State| s.go(page.clone()));
    }

    let surface = col().key(format!("server-{port}"));
    let card = match tone(server.status, l) {
        Some(tone) => lit(surface, l.card, 16.0, tone, l),
        None => raised(surface, l.card, 16.0, l),
    };
    card.child(
        picture(s, server, l)
            .w_full()
            .aspect_ratio(1.6)
            .corners([15.0, 15.0, 0.0, 0.0]),
    )
    .child(
        col()
            .padding(14.0)
            .gap(12.0)
            .child(
                row()
                    .gap(10.0)
                    .center_y()
                    .child(name)
                    .child(address(port, l)),
            )
            .child(who(s, server, l))
            .child(
                row()
                    .gap(8.0)
                    .child(
                        button(
                            format!("stop-{port}"),
                            "Stop",
                            Kind::Quiet,
                            s.busy.is_none(),
                            l,
                            move |s: &mut State| stop(s, port, pid, slot),
                        )
                        .grow(1.0),
                    )
                    .child(
                        button(
                            format!("open-{port}"),
                            "Open",
                            Kind::Main,
                            server.live(),
                            l,
                            move |_: &mut State| open(port),
                        )
                        .grow(1.0),
                    ),
            ),
    )
}

/// A port as an address, the way a terminal would print it
fn address(port: u16, l: &Look) -> El {
    row()
        .h(24.0)
        .px(9.0)
        .center()
        .radius(8.0)
        .fill(l.well)
        .border(1.0, l.line)
        .child(
            text(format!("localhost:{port}"))
                .font(mono(11.5))
                .color(l.soft),
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
            .child(mark(s, "carbon", 56.0, l).shadow(0.0, 40.0, l.ink.alpha(0.12)))
            .child(el().h(12.0))
            .child(
                text("Nothing is running")
                    .font(sans(19.0, 600.0))
                    .color(l.ink),
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
    let heading = col()
        .key("heading")
        .px(32.0)
        .pt(30.0)
        .pb(22.0)
        .gap(4.0)
        .window_drag()
        // The machine is on: a little green in the air
        .child(aura(
            el().size(48.0, 48.0).radius(24.0).fill(l.live),
            30.0,
            10.0,
            0.16,
        ))
        .child(text("Running").font(sans(26.0, 600.0)).color(l.ink))
        .child(text(note).font(sans(13.0, 400.0)).color(l.ink.alpha(0.66)));
    col()
        .key("running")
        .grow(1.0)
        .min_h(0.0)
        .child(heading)
        .child(scrolling(col().key("running-body").child(cards)))
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
fn doing(app: &App, mine: &[&Server], servers: &[Server]) -> String {
    let Some(server) = mine.first() else {
        // Something else on its port is worth saying: Run will move it rather than push that aside
        return match servers.iter().any(|other| other.port == app.port) {
            true => "Something else is on its port, so Run will give it another.".into(),
            false => "Not running".into(),
        };
    };
    match server.status {
        "starting" => "Starting".into(),
        "exited" => "Stopped by itself".into(),
        _ => {
            let mut says = match server.since {
                Some(since) => format!("Live, up {}", model::up(since)),
                None => "Live".to_string(),
            };
            match &server.origin {
                Origin::Agent(agent) => {
                    says += &format!(", started by {}", by(agent));
                    if let Some(model) = &agent.model {
                        says += &format!(" ({model})");
                    }
                }
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
        .radius(11.0)
        .fill(l.well)
        .border(1.0, l.line)
        .child(
            lines
                .scroll(ScrollSpec { stick_bottom: true })
                .grow(1.0)
                .min_h(0.0),
        )
}

fn app_card(s: &State, servers: &[Server], project: &Project, app: &App, l: &Look) -> El {
    let mine: Vec<&Server> = servers
        .iter()
        .filter(|server| server.app.is_some_and(|a| a.id == app.id))
        .collect();
    let first = mine.first().copied();
    let status = first.map_or("idle", |server| server.status);
    let idle = s.busy.is_none();
    let id = app.id.clone();

    let mut title = row()
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
            .color(l.ink)
            .truncate(),
        );
    // What it is built with: the main thing by name, the rest as marks. Failing that, the kind of command it runs
    title = match app.stack.split_first() {
        Some((main, rest)) => rest
            .iter()
            .take(5)
            .fold(title.child(tech_chip(s, main, l)), |title, tech| {
                title.child(tech_mark(s, tech, 20.0, l))
            }),
        None => title.child(chip(app.kind, l)),
    };

    // Where it is, then what can be done with it, with the thing to press last
    let mut buttons = row()
        .gap(8.0)
        .center_y()
        .child(address(first.map_or(app.port, |server| server.port), l));
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
            text(doing(app, &mine, servers))
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
                .radius(10.0)
                .border(1.0, l.line),
        );
    }
    // A card with something on it is lit in that thing's colour
    let surface = col().key(format!("app-{}", app.id));
    let card = match tone(status, l) {
        Some(tone) => lit(surface, l.card, 16.0, tone, l),
        None => raised(surface, l.card, 16.0, l),
    };
    let mut card = card.padding(18.0).gap(14.0).child(top.child(about));

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
        .h(42.0)
        .px(12.0)
        .center_y()
        .radius(11.0)
        .fill(l.well)
        .border(1.0, l.ink.alpha(0.5))
        .child(
            text_input(&s.rename, sans(20.0, 600.0))
                .key("rename")
                .single_line()
                .grow(1.0)
                .color(l.ink)
                .placeholder(project.name.clone(), l.faint)
                .caret(l.ink, l.ink.alpha(0.25))
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

/// Light behind a project's name, in the project's own colours: its logo, far out of focus
fn glow(s: &State, project: &Project) -> El {
    let small = el().size(48.0, 48.0);
    let light = match s.logos.get(&project.key) {
        Some((_, logo)) => small.pixels_fit(logo.pixels(), SurfaceFit::Fill),
        None => small
            .radius(24.0)
            .fill(oklch(0.62, 0.16, hue(&project.key))),
    };
    aura(light, 40.0, 38.0, 0.34)
}

pub fn project(s: &State, snap: &Snapshot, servers: &[Server], project: &Project, l: &Look) -> El {
    let mut apps: Vec<&App> = project
        .apps
        .iter()
        .filter_map(|id| snap.apps.iter().find(|app| &app.id == id))
        .collect();
    // The site first, then what stands behind it: by the folder's name, with the project's own folder leading
    apps.sort_by_key(|app| {
        (
            app.part.is_some(),
            !matches!(
                app.part.as_deref(),
                Some("web" | "website" | "site" | "www")
            ),
            app.part.clone(),
        )
    });
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
        // These sit over the project's light, so they are white let through rather than a grey of their own
        let mut words = col().gap(5.0).child(
            text(project.title.clone())
                .font(sans(26.0, 600.0))
                .color(l.ink)
                .truncate(),
        );
        if let Some(says) = &project.description {
            words = words.child(
                text(says.clone())
                    .font(sans(13.0, 400.0).lh(19.0))
                    .color(l.ink.alpha(0.78))
                    .wrap()
                    .max_lines(2),
            );
        }
        words.child(
            text(model::folder(&project.group, &project.name))
                .font(sans(12.0, 400.0))
                .color(l.ink.alpha(0.62))
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
        let (key, pinned) = (project.key.clone(), project.pinned);
        actions = actions.child(button(
            "pin",
            if pinned { "Unpin" } else { "Pin" },
            Kind::Quiet,
            true,
            l,
            move |s: &mut State| s.pin(key.clone(), !pinned),
        ));
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
        .child(glow(s, project))
        .child(tile(s, project, 64.0, l).shadow(8.0, 24.0, l.shadow))
        .child(words.grow(1.0).min_w(0.0))
        .child(actions);

    let mut list = col().key("project-body").gap(14.0);
    for app in apps {
        list = list.child(app_card(s, servers, project, app, l));
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
