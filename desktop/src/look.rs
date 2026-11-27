//! How Carbon looks: flat black, surfaces told apart by a step of grey and a hairline, white for the one thing to
//! press, and green for what is live. No gradients and no glows: the pictures of the apps are the colour.
//! There is one look, whatever the system's appearance.

use carbon::{
    engine::{Agent, Project},
    stack::Tech,
};
use rune::{
    Color, Cursor, Cx, El, Image, LoopSpec, SurfaceFit, TextStyle, Transition,
    color::{hex, oklch},
    el, row, text,
    tok::Family,
};

use crate::State;

pub struct Look {
    /// The window itself, behind everything
    pub void: Color,
    /// The sheet the page is on
    pub sheet: Color,
    /// A card on the sheet
    pub card: Color,
    /// A quiet button, and the row chosen in the list
    pub quiet: Color,
    /// Wells: text fields, the place a picture goes, output
    pub well: Color,
    /// The hairline round a surface
    pub line: Color,
    /// A hairline that has something to say: the one round a chosen row
    pub edge: Color,
    pub ink: Color,
    pub soft: Color,
    pub faint: Color,
    /// A row under the pointer
    pub over: Color,
    /// The thing to press: white, with black on it
    pub main: Color,
    pub main_over: Color,
    pub on_main: Color,
    pub live: Color,
    pub starting: Color,
    pub bad: Color,
}

pub fn look(_: &Cx<State>) -> Look {
    let grey = |code: &str| hex(code).expect("a colour written as hex");
    Look {
        void: grey("#060607"),
        sheet: grey("#0b0b0c"),
        card: grey("#111113"),
        quiet: grey("#1b1b1e"),
        well: grey("#070708"),
        line: Color::WHITE.alpha(0.065),
        edge: Color::WHITE.alpha(0.11),
        ink: grey("#f4f4f5"),
        soft: grey("#a1a1aa"),
        faint: grey("#63636c"),
        over: Color::WHITE.alpha(0.045),
        main: grey("#f4f4f5"),
        main_over: grey("#ffffff"),
        on_main: grey("#09090a"),
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

/// A surface on the sheet: one step lighter than it, with a hairline round it
pub fn raised(surface: El, radius: f32, l: &Look) -> El {
    surface
        .radius(radius)
        .continuous_corners()
        .fill(l.card)
        .border(1.0, l.line)
}

/// How a card arrives when the page it is on is opened: it rises a little into place, one after another
pub fn arrive(card: El, index: usize) -> El {
    card.transition(Transition::offset(0.0, 10.0).combined(Transition::opacity()))
        .enter_delay((index.min(8) as f32) * 0.035)
}

/// A small light: green once answering, amber while starting, red if it fell over, hollow when off. A live or
/// starting one breathes, so what is up reads at a glance from across the window.
pub fn dot(status: &str, l: &Look) -> El {
    let light = el().size(7.0, 7.0).radius(3.5);
    let breathing = |colour: Color, period: f64| {
        el().size(15.0, 15.0)
            .child(
                el().size(15.0, 15.0)
                    .radius(7.5)
                    .absolute(0.0, 0.0)
                    .fill(colour.alpha(0.28))
                    .looping(LoopSpec::opacity(period, 0.0, 0.0, 1.0)),
            )
            .child(light.clone().absolute(4.0, 4.0).fill(colour))
    };
    match status {
        "running" => breathing(l.live, 2.6),
        "starting" => breathing(l.starting, 1.1),
        "exited" => el().size(15.0, 15.0).center().child(light.fill(l.bad)),
        _ => el()
            .size(15.0, 15.0)
            .center()
            .child(light.border(1.5, l.faint)),
    }
}

pub enum Kind {
    /// The one thing to press
    Main,
    /// A button with a face of its own
    Quiet,
    /// Words that can be pressed, with a face only under the pointer
    Ghost,
}

fn face_of(key: String, label: &str, kind: Kind, l: &Look) -> El {
    let face = row()
        .key(key)
        .h(28.0)
        .px(12.0)
        .center()
        .radius(8.0)
        .continuous_corners();
    match kind {
        Kind::Main => face
            .fill(l.main)
            .hover(l.main_over)
            .child(text(label).font(sans(12.5, 600.0)).color(l.on_main)),
        Kind::Quiet => face
            .fill(l.quiet)
            .border(1.0, l.line)
            .hover(Color::WHITE.alpha(0.05))
            .child(text(label).font(sans(12.5, 500.0)).color(l.ink)),
        Kind::Ghost => face
            .hover(l.over)
            .child(text(label).font(sans(12.5, 500.0)).color(l.soft))
            .press_color(l.ink),
    }
}

/// How a button looks, for the few whose press needs more than the state (a file dialog)
pub fn plain_button(key: impl Into<String>, label: &str, kind: Kind, l: &Look) -> El {
    face_of(key.into(), label, kind, l)
        .press_scale(0.96)
        .cursor(Cursor::Pointer)
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
    face_of(key.into(), label, kind, l).opacity(0.35)
}

/// A word about a thing
pub fn chip(label: &str, l: &Look) -> El {
    row()
        .h(20.0)
        .px(7.0)
        .center()
        .radius(6.0)
        .border(1.0, l.line)
        .child(text(label).font(sans(11.0, 500.0)).color(l.soft))
}

/// A tick box: white with a black tick when on, an outline when off
pub fn tick(s: &State, on: bool, l: &Look) -> El {
    let frame = row().size(18.0, 18.0).radius(6.0).center();
    match (on, s.icons.get("check")) {
        (true, Some(check)) => frame
            .fill(l.main)
            .child(el().size(12.0, 12.0).pixels(check.pixels())),
        (true, None) => frame.fill(l.main),
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
        .fill(oklch(0.56, 0.12, hue(seed)))
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
        .fill(l.quiet)
        .border(1.0, l.line);
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

/// What stands in for a picture of an app that isn't running: the mark of the main thing it is built with (its
/// project's face, failing that), quietly, on a wash of the mark's own colour
pub fn poster(s: &State, tech: Option<&Tech>, fallback: El, l: &Look) -> El {
    let tone = tech
        .and_then(|tech| s.tones.get(tech.id))
        .copied()
        .unwrap_or(l.ink);
    let front = match tech.and_then(|tech| s.techs.get(tech.id)) {
        Some(image) => on_tile(Some(image), 44.0, l),
        None => fallback,
    };
    row()
        .center()
        .fill(l.well)
        .child(
            el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .fill(tone.alpha(0.035)),
        )
        .child(front.opacity(0.9))
}

/// The place a picture of an app goes. Its size comes from its width and `ratio` alone, never from the
/// picture: what is shown inside is pinned to its edges, out of the layout, so a picture arriving (or one of any
/// shape) leaves the card exactly as it was. A hairline is drawn over the top of whatever is in it.
pub fn screen(content: El, ratio: f32, radius: f32, l: &Look) -> El {
    el().w_full()
        .min_w(0.0)
        .aspect_ratio(ratio)
        .radius(radius)
        .continuous_corners()
        .clip_rounded(radius)
        .fill(l.well)
        .child(content.pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)))
        .child(
            el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .radius(radius)
                .continuous_corners()
                .border(1.0, l.line),
        )
}

/// The main thing an app is built with, by mark and name
pub fn tech_chip(s: &State, tech: &Tech, l: &Look) -> El {
    row()
        .h(22.0)
        .gap(7.0)
        .center_y()
        .child(on_tile(s.techs.get(tech.id), 20.0, l))
        .child(text(tech.name).font(sans(12.0, 500.0)).color(l.soft))
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
