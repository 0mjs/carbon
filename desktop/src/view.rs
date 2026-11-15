//! The window: a list on the left that chooses what to look at, and a sheet on the right that shows it.
//! The list is "Running" (every server on the machine), the projects pinned to the top, and then every project
//! in the folders they live in.

use carbon::engine::{Project, Snapshot};
use rune::{
    Color, Cursor, Cx, El, Key, ScrollSpec, TextEdit, Tx, col, el, row, spacer, text, text_input,
};

use crate::{
    Page, State, TITLEBAR,
    look::{Kind, Look, button, dot, icon, look, mark, mono, sans, tile},
    model::{self, Server},
    pages, setup,
};

const LIST: f32 = 252.0;

/// One row of the list
fn entry(key: String, chosen: bool, l: &Look, page: Page) -> El {
    let entry = row()
        .key(key)
        .h(34.0)
        .mx(10.0)
        .px(8.0)
        .gap(10.0)
        .center_y()
        .radius(9.0)
        .cursor(Cursor::Pointer)
        .hover_group()
        .on_press(move |s: &mut State| match &page {
            Page::Setup => s.open_setup(),
            page => s.go(page.clone()),
        });
    if chosen {
        entry
            .gradient(l.quiet.0, l.quiet.1)
            .border_gradient(1.0, l.rim.0, l.rim.1)
    } else {
        entry.hover(l.over)
    }
}

fn search(s: &State, l: &Look) -> El {
    row()
        .key("search-field")
        .h(32.0)
        .mx(12.0)
        .px(10.0)
        .gap(8.0)
        .center_y()
        .radius(9.0)
        .fill(l.well)
        .border(1.0, l.line)
        .child(icon(s, "search", 13.0).opacity(0.45))
        .child(
            text_input(&s.search, sans(13.0, 400.0))
                .key("search")
                .single_line()
                .grow(1.0)
                .color(l.ink)
                .placeholder("Search", l.faint)
                .caret(l.ink, l.ink.alpha(0.25))
                .on_edit(|s: &mut State| &mut s.search)
                // Return goes to the first project found, and Escape gives the whole list back
                .on_submit(|s: &mut State, _: &mut Tx| {
                    let first = s.snap.as_ref().and_then(|snap| {
                        found(snap, &s.search.text)
                            .first()
                            .map(|project| project.key.clone())
                    });
                    if let Some(key) = first {
                        s.search = TextEdit::default();
                        s.go(Page::Project(key));
                    }
                })
                .on_input_key(|s: &mut State, event, _| {
                    let clearing = event.key == Key::Escape && !s.search.text.is_empty();
                    if clearing {
                        s.search = TextEdit::default();
                    }
                    clearing
                }),
        )
}

/// The projects that are what was typed in the search field (all of them when nothing was), in the folders
/// they live in and by name within each
fn found<'a>(snap: &'a Snapshot, typed: &str) -> Vec<&'a Project> {
    let wanted = typed.trim().to_lowercase();
    let mut projects: Vec<&Project> = snap
        .projects
        .iter()
        .filter(|project| {
            wanted.is_empty()
                || [&project.title, &project.name, &project.group]
                    .iter()
                    .any(|text| text.to_lowercase().contains(&wanted))
        })
        .collect();
    projects.sort_by_key(|project| (project.group.clone(), project.title.to_lowercase()));
    projects
}

/// A small heading over part of the list
fn heading(key: String, name: &str, l: &Look) -> El {
    text(name)
        .key(key)
        .font(sans(11.5, 600.0))
        .color(l.faint)
        .mx(18.0)
        .mt(18.0)
        .mb(6.0)
}

/// A project in the list: its face and name, a pin that shows under the pointer, and a light when it is live
fn project_entry(s: &State, project: &Project, servers: &[Server], l: &Look) -> El {
    let live = servers.iter().any(|server| {
        server.live() && server.app.is_some_and(|app| project.apps.contains(&app.id))
    });
    let (key, pinned) = (project.key.clone(), project.pinned);
    let pin = row()
        .key(format!("pin-{}", project.key))
        .size(22.0, 22.0)
        .center()
        .radius(6.0)
        .hover(l.over)
        .group_opacity(0.0, if pinned { 1.0 } else { 0.5 })
        .help(if pinned { "Unpin" } else { "Pin to the top" })
        .on_press(move |s: &mut State| s.pin(key.clone(), !pinned))
        .child(icon(s, "pin", 13.0));
    let mut entry = entry(
        format!("go-{}", project.key),
        s.page == Page::Project(project.key.clone()),
        l,
        Page::Project(project.key.clone()),
    )
    .child(tile(s, project, 22.0, l))
    .child(
        text(project.title.clone())
            .font(sans(13.0, 500.0))
            .color(l.ink)
            .truncate()
            .grow(1.0)
            .min_w(0.0),
    )
    .child(pin);
    if live {
        entry = entry.child(dot("running", l));
    }
    entry
}

fn list(s: &State, snap: &Snapshot, servers: &[Server], lead: f32, l: &Look) -> El {
    let running = servers.iter().filter(|server| server.live()).count();
    let mut home = entry(
        "go-running".into(),
        s.page == Page::Running,
        l,
        Page::Running,
    )
    .child(
        row()
            .size(22.0, 22.0)
            .center()
            .child(dot(if running > 0 { "running" } else { "idle" }, l)),
    )
    .child(text("Running").font(sans(13.0, 500.0)).color(l.ink))
    .child(spacer());
    if running > 0 {
        home = home.child(
            text(running.to_string())
                .font(sans(12.0, 500.0))
                .color(l.soft),
        );
    }

    let projects = found(snap, &s.search.text);
    let mut rows = col().key("projects").pb(12.0);
    // Pinned projects come first, under a heading of their own, and keep out of the folders below
    let pinned: Vec<&&Project> = projects.iter().filter(|project| project.pinned).collect();
    if !pinned.is_empty() {
        rows = rows.child(heading("folder-pinned".into(), "Pinned", l));
        for project in pinned {
            rows = rows.child(project_entry(s, project, servers, l));
        }
    }
    let mut folder: Option<&str> = None;
    for project in projects.iter().filter(|project| !project.pinned) {
        if folder != Some(project.group.as_str()) {
            folder = Some(project.group.as_str());
            // The folder's own name is enough: "work", not "dev/work"
            let name = project
                .group
                .rsplit('/')
                .next()
                .filter(|name| !name.is_empty())
                .unwrap_or("home");
            rows = rows.child(heading(format!("folder-{}", project.group), name, l));
        }
        rows = rows.child(project_entry(s, project, servers, l));
    }
    if projects.is_empty() {
        let none = if s.search.text.trim().is_empty() {
            "No projects yet."
        } else {
            "No project is called that."
        };
        rows = rows.child(
            text(none)
                .font(sans(12.5, 400.0))
                .color(l.faint)
                .mx(18.0)
                .mt(18.0),
        );
    }

    let choose = entry("go-setup".into(), s.page == Page::Setup, l, Page::Setup)
        .child(
            row()
                .size(22.0, 22.0)
                .center()
                .child(icon(s, "folder-search", 14.0).opacity(0.7)),
        )
        .child(
            text("Choose projects")
                .font(sans(13.0, 500.0))
                .color(l.soft),
        );

    // Carbon's name, beside the window's own buttons where it has them
    let brand = row()
        .h(TITLEBAR)
        .pl(lead)
        .gap(9.0)
        .center_y()
        .window_drag()
        .child(mark(s, "carbon", 22.0, l))
        .child(text("Carbon").font(sans(14.5, 600.0)).color(l.ink));

    col()
        .key("list")
        .w(LIST)
        .h_full()
        .child(brand)
        .child(search(s, l))
        .child(el().h(10.0))
        .child(home)
        .child(
            rows.scroll(ScrollSpec {
                stick_bottom: false,
            })
            .grow(1.0)
            .min_h(0.0),
        )
        .child(el().h(8.0))
        .child(choose)
        .child(el().h(12.0))
}

/// Something the engine refused, said plainly at the top of the sheet until dismissed
fn refusal(why: &str, l: &Look) -> El {
    row()
        .key("refusal")
        .mx(32.0)
        .mt(20.0)
        .px(14.0)
        .py(10.0)
        .gap(12.0)
        .center_y()
        .radius(11.0)
        .fill(l.bad.alpha(0.12))
        .border(1.0, l.bad.alpha(0.25))
        .child(
            text(why.to_string())
                .font(mono(12.0))
                .color(l.bad)
                .wrap()
                .grow(1.0)
                .min_w(0.0),
        )
        .child(button(
            "refusal-close",
            "Dismiss",
            Kind::Quiet,
            true,
            l,
            |s: &mut State| s.error = None,
        ))
}

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    let l = look(cx);
    let window = row().w_full().h_full().fill(l.void);
    let Some(snap) = &s.snap else {
        return window.center().window_drag().child(
            text("Looking at what is running…")
                .font(sans(13.0, 400.0))
                .color(l.faint),
        );
    };
    let servers = model::servers(snap);
    // As many cards across as fit comfortably, going by how wide the sheet was last drawn
    let across = cx.layout_size("sheet").map_or(2, |[width, _]| {
        ((width - 48.0) / 340.0).floor().clamp(1.0, 4.0) as usize
    });

    let chosen = match &s.page {
        Page::Project(key) => snap.projects.iter().find(|project| &project.key == key),
        _ => None,
    };
    let page = match (&s.page, chosen) {
        (Page::Setup, _) => setup::page(s, &l),
        (_, Some(project)) => pages::project(s, snap, &servers, project, across, &l),
        _ => pages::running(s, snap, &servers, across, &l),
    };

    // The first time, setup is the whole window: there is no list to choose from yet
    let alone = s.first && s.page == Page::Setup;
    let mut sheet = col()
        .key("sheet")
        .grow(1.0)
        .min_w(0.0)
        .mt(8.0)
        .mr(8.0)
        .mb(8.0)
        .ml(if alone { 8.0 } else { 0.0 })
        .radius(16.0)
        .continuous_corners()
        .clip_rounded(16.0)
        .gradient(l.sheet.0, l.sheet.1)
        .border_gradient(1.0, l.rim.0.alpha(0.1), l.rim.1);
    // A soft light falling on the sheet from above, so the black has depth
    sheet = sheet.child(
        el().key("light")
            .absolute(0.0, 0.0)
            .w_full()
            .h(420.0)
            .radial_gradient(
                [0.5, 0.0],
                [760.0, 420.0],
                [
                    (0.0, Color::WHITE.alpha(0.055)),
                    (1.0, Color::WHITE.alpha(0.0)),
                ],
            ),
    );
    if let Some(why) = &s.error {
        sheet = sheet.child(refusal(why, &l));
    }
    let sheet = sheet.child(page);
    if alone {
        return window.child(sheet);
    }
    // Carbon's name starts clear of the window's own buttons, where the window has them
    let lead = match cx.chrome_insets().leading {
        buttons if buttons > 0.0 => buttons + 10.0,
        _ => 18.0,
    };
    window.child(list(s, snap, &servers, lead, &l)).child(sheet)
}
