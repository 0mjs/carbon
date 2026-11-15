//! Setup: where the projects are, and which of them to show. Carbon looks through the home folder by itself and
//! ticks everything it finds; this is where a person unticks what they don't want, or points it at a folder it
//! didn't think to look in. It is the whole window the first time Carbon is opened, and a page after that.

use carbon::engine::{Choice, Place};
use rune::{Cursor, El, FileDialogOptions, ScrollSpec, Tx, col, el, row, spacer, text};

use crate::{
    State,
    look::{Kind, Look, button, face, mark, plain_button, raised, sans, tick},
};

fn project_row(s: &State, at: (usize, usize), project: &Choice, l: &Look) -> El {
    let (place, index) = at;
    row()
        .key(format!("choice-{}", project.key))
        .h(40.0)
        .px(14.0)
        .gap(12.0)
        .center_y()
        .cursor(Cursor::Pointer)
        .hover(l.over)
        .on_press(move |s: &mut State| {
            if let Some(project) = s
                .places
                .as_mut()
                .and_then(|places| places.get_mut(place)?.projects.get_mut(index))
            {
                project.on = !project.on;
            }
        })
        .child(tick(s, project.on, l))
        .child(face(
            s.offered.get(&project.key),
            &project.title,
            &project.key,
            24.0,
            l,
        ))
        .child(
            text(project.title.clone())
                .font(sans(13.0, 500.0))
                .color(if project.on { l.ink } else { l.faint })
                .truncate(),
        )
        .child(spacer())
        .child(
            text(project.place.clone())
                .font(sans(12.0, 400.0))
                .color(l.faint)
                .truncate(),
        )
}

/// One place code lives: the folder, then its projects while it is ticked
fn place_card(s: &State, index: usize, place: &Place, l: &Look) -> El {
    let shown = place.projects.iter().filter(|project| project.on).count();
    let count = match (place.on, place.projects.len()) {
        (_, 0) => "Nothing here that Carbon can run".to_string(),
        (false, 1) => "1 project".to_string(),
        (false, all) => format!("{all} projects"),
        (true, all) if shown == all => format!("{all} of {all} shown"),
        (true, all) => format!("{shown} of {all} shown"),
    };
    let head = row()
        .key(format!("place-{}", place.name))
        .h(48.0)
        .px(14.0)
        .gap(12.0)
        .center_y()
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut State| {
            if let Some(place) = s.places.as_mut().and_then(|places| places.get_mut(index)) {
                place.on = !place.on;
            }
        })
        .child(tick(s, place.on, l))
        .child(
            text(place.name.clone())
                .font(sans(14.0, 600.0))
                .color(if place.on { l.ink } else { l.soft }),
        )
        .child(spacer())
        .child(text(count).font(sans(12.0, 400.0)).color(l.faint));
    let mut card = raised(col().key(format!("card-{}", place.name)), l.card, 14.0, l)
        .clip_rounded(14.0)
        .child(head);
    if place.on && !place.projects.is_empty() {
        let mut rows = col().py(6.0).border_sides([1.0, 0.0, 0.0, 0.0], l.line);
        for (i, project) in place.projects.iter().enumerate() {
            rows = rows.child(project_row(s, (index, i), project, l));
        }
        card = card.child(rows);
    }
    card
}

pub fn page(s: &State, l: &Look) -> El {
    let words = col()
        .gap(8.0)
        .child(mark(s, "carbon", 44.0, l))
        .child(el().h(6.0))
        .child(text("Where are your projects?").font(sans(26.0, 600.0)).color(l.ink))
        .child(
            text("Carbon looked through your home folder and found these. Untick a folder or a project to leave it out, or add a folder it missed. You can come back to this from the bottom of the list.")
                .font(sans(13.0, 400.0).lh(20.0))
                .color(l.soft)
                .wrap(),
        );

    let mut found = col().key("places").gap(14.0).pb(8.0);
    match &s.places {
        None => {
            found = found.child(
                text("Looking through your home folder…")
                    .font(sans(13.0, 400.0))
                    .color(l.faint),
            )
        }
        Some(places) if places.is_empty() => {
            found = found.child(
                text("Carbon found no projects by itself. Add the folder you keep them in.")
                    .font(sans(13.0, 400.0))
                    .color(l.soft),
            );
        }
        Some(places) => {
            for (index, place) in places.iter().enumerate() {
                found = found.child(place_card(s, index, place, l));
            }
        }
    }

    let add = plain_button("place-add", "Add a folder…", Kind::Quiet, l).on_press_tx(
        |_: &mut State, tx: &mut Tx| {
            let options = FileDialogOptions {
                title: "Where do you keep projects?".into(),
                ..Default::default()
            };
            tx.open_folder(options, |s: &mut State, picked, _| match picked {
                Ok(Some(folder)) => s.add_place(folder),
                Ok(None) => {}
                Err(why) => s.error = Some(why.0),
            });
        },
    );
    let chosen = s.places.as_ref().map_or(0, |places| {
        places
            .iter()
            .filter(|place| place.on)
            .map(|place| place.projects.iter().filter(|project| project.on).count())
            .sum()
    });
    let done = match (s.first, chosen) {
        (true, 0) => "Continue with none".to_string(),
        (true, 1) => "Continue with 1 project".to_string(),
        (true, n) => format!("Continue with {n} projects"),
        (false, _) => "Done".to_string(),
    };
    let foot = row()
        .gap(10.0)
        .center_y()
        .pt(14.0)
        .pb(26.0)
        .child(add)
        .child(spacer())
        .child(button(
            "setup-done",
            &done,
            Kind::Main,
            s.places.is_some(),
            l,
            |s: &mut State| s.finish_setup(),
        ));

    // One column down the middle of the sheet, however wide the window is
    row()
        .key("setup")
        .grow(1.0)
        .min_h(0.0)
        .justify_center()
        .child(
            col()
                .w(620.0)
                .h_full()
                .child(words.pt(44.0).pb(22.0).window_drag())
                .child(
                    found
                        .scroll(ScrollSpec {
                            stick_bottom: false,
                        })
                        .grow(1.0)
                        .min_h(0.0),
                )
                .child(foot),
        )
}
