//! How Carbon looks: deep black that lifts a little towards the top of each surface, a lit rim along its upper
//! edge, white for the one thing to press, and green for what is live. It is the app's icon, carried through the
//! window. There is one look, whatever the system's appearance.

use carbon::{
    engine::{Agent, Project},
    stack::Tech,
};
use rune::{
    Color, Cursor, Cx, El, Image, SurfaceFit, TextStyle,
    color::{hex, oklch},
    el, row, text,
    tok::Family,
};

use crate::State;

pub struct Look {
    /// The window itself, behind everything
    pub void: Color,
    /// The sheet the page is on: lighter at its top than its bottom
    pub sheet: (Color, Color),
    /// A card on the sheet
    pub card: (Color, Color),
    /// A quiet button, and the row chosen in the list
    pub quiet: (Color, Color),
    pub quiet_over: Color,
    /// Wells: text fields, the place a picture goes, output
    pub well: Color,
    /// The lit rim of a surface: bright along the top, nearly gone at the bottom
    pub rim: (Color, Color),
    pub line: Color,
    pub ink: Color,
    pub soft: Color,
    pub faint: Color,
    /// A row under the pointer
    pub over: Color,
    /// The thing to press: white, with black on it
    pub main: (Color, Color),
    pub on_main: Color,
    pub shadow: Color,
    pub live: Color,
    pub starting: Color,
    pub bad: Color,
}

pub fn look(_: &Cx<State>) -> Look {
    let grey = |code: &str| hex(code).expect("a colour written as hex");
    Look {
        void: grey("#050506"),
        sheet: (grey("#101012"), grey("#0a0a0b")),
        card: (grey("#1b1b1e"), grey("#121214")),
        quiet: (grey("#2a2a2e"), grey("#1c1c1f")),
        quiet_over: grey("#323237"),
        well: grey("#09090a"),
        rim: (Color::WHITE.alpha(0.16), Color::WHITE.alpha(0.04)),
        line: Color::WHITE.alpha(0.07),
        ink: grey("#f4f4f5"),
        soft: grey("#a3a3ab"),
        faint: grey("#6c6c75"),
        over: Color::WHITE.alpha(0.05),
        main: (grey("#ffffff"), grey("#d6d6dc")),
        on_main: grey("#09090a"),
        shadow: Color::BLACK.alpha(0.55),
        live: oklch(0.8, 0.19, 150.0),
        starting: oklch(0.82, 0.15, 80.0),
        bad: oklch(0.7, 0.19, 25.0),
    }
}

pub const fn sans(size: f32, weight: f32) -> TextStyle {
    TextStyle::new(size, weight)
}
/// For output and commands only: what a terminal would print
pub const fn mono(size: f32) -> TextStyle {
    TextStyle::new(size, 400.0).family(Family::SystemMono)
}

/// Makes an element a raised surface: shaded from its top, with a lit rim and a shadow under it
pub fn raised(surface: El, shade: (Color, Color), radius: f32, l: &Look) -> El {
    surface
        .radius(radius)
        .continuous_corners()
        .gradient(shade.0, shade.1)
        .border_gradient(1.0, l.rim.0, l.rim.1)
        .shadow(1.0, 2.0, l.shadow)
        .shadow(14.0, 32.0, l.shadow)
}

/// A small light: green once answering, amber while starting, red if it fell over, hollow when off
pub fn dot(status: &str, l: &Look) -> El {
    let light = el().size(8.0, 8.0).radius(4.0);
    match status {
        "running" => light.fill(l.live).shadow(0.0, 8.0, l.live.alpha(0.6)),
        "starting" => light
            .fill(l.starting)
            .shadow(0.0, 8.0, l.starting.alpha(0.5)),
        "exited" => light.fill(l.bad),
        _ => light.border(1.5, l.faint),
    }
}

pub enum Kind {
    /// The one thing to press
    Main,
    Quiet,
}

/// How a button looks, for the few whose press needs more than the state (a file dialog)
pub fn plain_button(key: impl Into<String>, label: &str, kind: Kind, l: &Look) -> El {
    let face = row()
        .key(key.into())
        .h(30.0)
        .px(14.0)
        .center()
        .radius(9.0)
        .press_scale(0.97)
        .cursor(Cursor::Pointer);
    match kind {
        Kind::Main => face
            .gradient(l.main.0, l.main.1)
            .hover(l.main.0)
            .shadow(0.0, 10.0, Color::WHITE.alpha(0.1))
            .child(text(label).font(sans(12.5, 600.0)).color(l.on_main)),
        Kind::Quiet => face
            .gradient(l.quiet.0, l.quiet.1)
            .border_gradient(1.0, l.rim.0, l.rim.1)
            .hover(l.quiet_over)
            .child(text(label).font(sans(12.5, 500.0)).color(l.ink)),
    }
}

/// A button. A disabled one is dimmed and does nothing.
pub fn button(
    key: impl Into<String>,
    label: &str,
    kind: Kind,
    enabled: bool,
    l: &Look,
    press: impl Fn(&mut State) + 'static,
) -> El {
    if enabled {
        return plain_button(key, label, kind, l).on_press(press);
    }
    let face = row()
        .key(key.into())
        .h(30.0)
        .px(14.0)
        .center()
        .radius(9.0)
        .opacity(0.38);
    match kind {
        Kind::Main => face
            .gradient(l.main.0, l.main.1)
            .child(text(label).font(sans(12.5, 600.0)).color(l.on_main)),
        Kind::Quiet => face
            .gradient(l.quiet.0, l.quiet.1)
            .border_gradient(1.0, l.rim.0, l.rim.1)
            .child(text(label).font(sans(12.5, 500.0)).color(l.ink)),
    }
}

/// A word about a thing
pub fn chip(label: &str, l: &Look) -> El {
    row()
        .h(20.0)
        .px(8.0)
        .center()
        .radius(7.0)
        .fill(l.over)
        .child(text(label).font(sans(11.0, 500.0)).color(l.soft))
}

/// A tick box: white with a black tick when on, an outline when off
pub fn tick(s: &State, on: bool, l: &Look) -> El {
    let frame = row().size(18.0, 18.0).radius(6.0).center();
    match (on, s.icons.get("check")) {
        (true, Some(check)) => frame
            .gradient(l.main.0, l.main.1)
            .child(el().size(12.0, 12.0).pixels(check.pixels())),
        (true, None) => frame.gradient(l.main.0, l.main.1),
        (false, _) => frame.border(1.5, l.faint),
    }
}

/// One of the app's interface icons, in the ink colour
pub fn icon(s: &State, name: &str, size: f32) -> El {
    match s.icons.get(name) {
        Some(image) => el().size(size, size).pixels(image.pixels()),
        None => el().size(size, size),
    }
}

/// A face for something with a name: its picture, or its first letter on a colour of its own until it has one
pub fn face(picture: Option<&Image>, title: &str, seed: &str, size: f32, l: &Look) -> El {
    let round = (size * 0.24).round();
    match picture {
        Some(picture) => el()
            .size(size, size)
            .radius(round)
            .continuous_corners()
            .fill(l.well)
            .pixels_fit(picture.pixels(), SurfaceFit::Fill),
        None => letter(title, seed, size, round),
    }
}

/// A project's face: its logo, or its first letter
pub fn tile(s: &State, project: &Project, size: f32, l: &Look) -> El {
    face(
        s.logos.get(&project.key).map(|(_, logo)| logo),
        &project.title,
        &project.key,
        size,
        l,
    )
}

/// The colour a name gets when it has no picture: worked out from the name, so it stays the same
pub fn hue(seed: &str) -> f32 {
    (seed.bytes().fold(7u32, |sum, byte| {
        sum.wrapping_mul(31).wrapping_add(u32::from(byte))
    }) % 360) as f32
}

/// The stand-in for a picture: the first letter, on the name's own colour
pub fn letter(title: &str, seed: &str, size: f32, round: f32) -> El {
    let first = title
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    row()
        .size(size, size)
        .radius(round)
        .continuous_corners()
        .center()
        .gradient(oklch(0.66, 0.13, hue(seed)), oklch(0.5, 0.14, hue(seed)))
        .child(
            text(first)
                .font(sans((size * 0.46).round(), 600.0))
                .color(Color::WHITE),
        )
}

/// A mark on a black tile of its own, like the app's icon
fn on_tile(image: Option<&Image>, size: f32, l: &Look) -> El {
    let inside = (size * 0.62).round();
    let tile = row()
        .size(size, size)
        .radius((size * 0.28).round())
        .continuous_corners()
        .center()
        .gradient(hex("#1f1f22").unwrap_or(l.well), l.well)
        .border_gradient(1.0, l.rim.0, l.rim.1);
    match image {
        Some(image) => tile.child(el().size(inside, inside).pixels(image.pixels())),
        None => tile,
    }
}

/// Carbon's own mark, or a harness's
pub fn mark(s: &State, name: &str, size: f32, l: &Look) -> El {
    on_tile(s.marks.get(name), size, l)
}

/// The mark of a tool an app is built with. Resting the pointer on it says what it is.
pub fn tech_mark(s: &State, tech: &Tech, size: f32, l: &Look) -> El {
    on_tile(s.techs.get(tech.id), size, l).help(tech.name)
}

/// The main thing an app is built with, by mark and name
pub fn tech_chip(s: &State, tech: &Tech, l: &Look) -> El {
    row()
        .h(22.0)
        .pl(3.0)
        .pr(9.0)
        .gap(6.0)
        .center_y()
        .radius(7.0)
        .fill(l.over)
        .child(on_tile(s.techs.get(tech.id), 16.0, l))
        .child(text(tech.name).font(sans(11.5, 500.0)).color(l.soft))
}

/// Who an agent is at a glance: its harness's mark (its first letter where Carbon has no mark for it), with the
/// icon of the app it is running in tucked into the corner
pub fn agent_mark(s: &State, agent: &Agent, size: f32, l: &Look) -> El {
    let face = match s.marks.contains_key(agent.harness) {
        true => mark(s, agent.harness, size, l),
        false => letter(agent.name, agent.harness, size, (size * 0.28).round()),
    };
    let badge = (size * 0.56).round();
    let app = agent
        .host
        .as_ref()
        .and_then(|host| host.app.as_ref())
        .and_then(|app| s.apps.get(app));
    match app {
        Some(app) => el()
            .size(size + badge * 0.4, size + badge * 0.3)
            .child(face.absolute(0.0, 0.0))
            .child(
                el().size(badge, badge)
                    .absolute(size - badge * 0.6, size - badge * 0.7)
                    .pixels(app.pixels()),
            ),
        None => face,
    }
}
