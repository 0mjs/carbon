//! The window: a list on the left that chooses what to look at, and a sheet on the right that shows it.
//! The list is "Running" (every server on the machine) and then every project, in the folders they live in.

use carbon::engine::{Project, Snapshot};
use rune::{Cursor, Cx, El, Key, ScrollSpec, TextEdit, Tx, col, el, row, spacer, text, text_input};

use crate::{
    Page, State, TITLEBAR,
    look::{Kind, Look, button, dot, look, mono, sans, tile},
    model::{self, Server},
    pages,
};

const LIST: f32 = 248.0;

/// One row of the list
fn entry(key: String, chosen: bool, l: &Look, page: Page) -> El {
    let entry = row()
        .key(key)
        .h(34.0)
        .mx(10.0)
        .px(8.0)
        .gap(10.0)
        .center_y()
        .radius(8.0)
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut State| s.go(page.clone()));
    if chosen {
        entry.fill(l.chosen)
    } else {
        entry.hover(l.over)
    }
}

fn search(s: &State, l: &Look) -> El {
    row()
        .key("search-field")
        .h(30.0)
        .mx(12.0)
        .px(10.0)
        .center_y()
        .radius(8.0)
        .fill(l.over)
        .child(
            text_input(&s.search, sans(13.0, 400.0))
                .key("search")
                .single_line()
                .grow(1.0)
                .color(l.text)
                .placeholder("Search", l.faint)
                .caret(l.accent, l.accent.alpha(0.3))
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

fn list(s: &State, snap: &Snapshot, servers: &[Server], top: f32, l: &Look) -> El {
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
    .child(text("Running").font(sans(13.0, 500.0)).color(l.text))
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
    let mut folder: Option<&str> = None;
    for project in &projects {
        if folder != Some(project.group.as_str()) {
            folder = Some(project.group.as_str());
            // The folder's own name is enough: "saas", not "dev/saas"
            let name = project
                .group
                .rsplit('/')
                .next()
                .filter(|name| !name.is_empty())
                .unwrap_or("home");
            rows = rows.child(
                text(name)
                    .key(format!("folder-{}", project.group))
                    .font(sans(11.5, 600.0))
                    .color(l.faint)
                    .mx(18.0)
                    .mt(18.0)
                    .mb(6.0),
            );
        }
        let live = servers.iter().any(|server| {
            server.live() && server.app.is_some_and(|app| project.apps.contains(&app.id))
        });
        let mut one = entry(
            format!("go-{}", project.key),
            s.page == Page::Project(project.key.clone()),
            l,
            Page::Project(project.key.clone()),
        )
        .child(tile(s, project, 22.0, l))
        .child(
            text(project.title.clone())
                .font(sans(13.0, 500.0))
                .color(l.text)
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        );
        if live {
            one = one.child(dot("running", l));
        }
        rows = rows.child(one);
    }
    if projects.is_empty() {
        rows = rows.child(
            text("No project is called that.")
                .font(sans(12.5, 400.0))
                .color(l.faint)
                .mx(18.0)
                .mt(18.0),
        );
    }

    col()
        .key("list")
        .w(LIST)
        .h_full()
        // The window's own buttons sit over this band, and it moves the window
        .child(el().h(top).window_drag())
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
        .radius(10.0)
        .fill(l.bad.alpha(0.12))
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
    let window = row().w_full().h_full().fill(l.window);
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
        ((width - 48.0) / 330.0).floor().clamp(1.0, 4.0) as usize
    });

    let chosen = match &s.page {
        Page::Project(key) => snap.projects.iter().find(|project| &project.key == key),
        Page::Running => None,
    };
    let page = match chosen {
        Some(project) => pages::project(s, snap, &servers, project, &l),
        None => pages::running(s, snap, &servers, across, &l),
    };

    let mut sheet = col()
        .key("sheet")
        .grow(1.0)
        .min_w(0.0)
        .mt(8.0)
        .mr(8.0)
        .mb(8.0)
        .radius(14.0)
        .continuous_corners()
        .fill(l.sheet)
        .border(l.hair, l.line)
        .shadow(0.0, 2.0, l.glow);
    if let Some(why) = &s.error {
        sheet = sheet.child(refusal(why, &l));
    }
    // Room for the window's own buttons where they sit over the list; full screen has none
    let top = if cx.chrome_insets().leading > 0.0 {
        TITLEBAR
    } else {
        14.0
    };
    window
        .child(list(s, snap, &servers, top, &l))
        .child(sheet.child(page))
}
