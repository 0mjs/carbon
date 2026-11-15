//! How Carbon looks: its colours in light and dark, its type, and the few small pieces every view is made of.
//! Soft surfaces, round corners, one accent (the system's) for the thing to press and green for what is live.

use carbon::engine::Project;
use rune::{
    Color, Cursor, Cx, El, SurfaceFit, TextStyle,
    color::{oklch, oklcha},
    el, row, text,
    tok::Family,
};

use crate::State;

pub struct Look {
    /// The window itself, which the list of projects sits on
    pub window: Color,
    /// The sheet the page is on, and the cards on the sheet
    pub sheet: Color,
    pub card: Color,
    /// Wells: text fields, the place a picture goes, output
    pub well: Color,
    pub line: Color,
    pub text: Color,
    pub soft: Color,
    pub faint: Color,
    /// A row in the list under the pointer, and the one chosen
    pub over: Color,
    pub chosen: Color,
    /// A quiet button, and under the pointer
    pub quiet: Color,
    pub quiet_over: Color,
    pub shadow: Color,
    pub glow: Color,
    pub accent: Color,
    pub live: Color,
    pub starting: Color,
    pub bad: Color,
    /// One device pixel, for hairlines
    pub hair: f32,
}

pub fn look(cx: &Cx<State>) -> Look {
    let hair = 1.0 / cx.env().scale;
    let accent = cx.accent();
    // Every neutral leans very slightly blue, so greys read as cool paper rather than dead grey
    let grey = |l: f32| oklch(l, 0.005, 265.0);
    if cx.system_dark() {
        Look {
            window: grey(0.168),
            sheet: grey(0.212),
            card: grey(0.252),
            well: grey(0.19),
            line: grey(0.305),
            text: grey(0.96),
            soft: grey(0.74),
            faint: grey(0.56),
            over: grey(0.215),
            chosen: grey(0.265),
            quiet: grey(0.315),
            quiet_over: grey(0.355),
            shadow: oklcha(0.0, 0.0, 0.0, 0.3),
            glow: oklcha(0.0, 0.0, 0.0, 0.22),
            accent,
            live: oklch(0.76, 0.18, 150.0),
            starting: oklch(0.8, 0.15, 80.0),
            bad: oklch(0.7, 0.19, 25.0),
            hair,
        }
    } else {
        Look {
            window: grey(0.945),
            sheet: grey(0.985),
            card: oklch(1.0, 0.0, 0.0),
            well: grey(0.957),
            line: grey(0.9),
            text: grey(0.22),
            soft: grey(0.45),
            faint: grey(0.62),
            over: grey(0.915),
            chosen: grey(0.885),
            quiet: grey(0.945),
            quiet_over: grey(0.915),
            shadow: oklcha(0.2, 0.02, 265.0, 0.1),
            glow: oklcha(0.2, 0.02, 265.0, 0.06),
            accent,
            live: oklch(0.64, 0.19, 150.0),
            starting: oklch(0.72, 0.16, 75.0),
            bad: oklch(0.58, 0.21, 25.0),
            hair,
        }
    }
}

pub const fn sans(size: f32, weight: f32) -> TextStyle {
    TextStyle::new(size, weight)
}
/// For output and commands only: what a terminal would print
pub const fn mono(size: f32) -> TextStyle {
    TextStyle::new(size, 400.0).family(Family::SystemMono)
}

/// A small light: green once answering, amber while starting, red if it fell over, hollow when off
pub fn dot(status: &str, l: &Look) -> El {
    let light = el().size(8.0, 8.0).radius(4.0);
    match status {
        "running" => light.fill(l.live).shadow(0.0, 6.0, l.live.alpha(0.45)),
        "starting" => light.fill(l.starting),
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
    let (fill, over, ink) = match kind {
        Kind::Main => (l.accent, l.accent.mix(Color::WHITE, 0.12), Color::WHITE),
        Kind::Quiet => (l.quiet, l.quiet_over, l.text),
    };
    row()
        .key(key.into())
        .h(30.0)
        .px(13.0)
        .center()
        .radius(8.0)
        .fill(fill)
        .hover(over)
        .press_scale(0.97)
        .cursor(Cursor::Pointer)
        .child(text(label).font(sans(12.5, 500.0)).color(ink))
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
    let (fill, ink) = match kind {
        Kind::Main => (l.accent, Color::WHITE),
        Kind::Quiet => (l.quiet, l.text),
    };
    if enabled {
        plain_button(key, label, kind, l).on_press(press)
    } else {
        row()
            .key(key.into())
            .h(30.0)
            .px(13.0)
            .center()
            .radius(8.0)
            .fill(fill)
            .opacity(0.45)
            .child(text(label).font(sans(12.5, 500.0)).color(ink))
    }
}

/// A word about a thing: what kind of app it is
pub fn chip(label: &str, l: &Look) -> El {
    row()
        .h(19.0)
        .px(7.0)
        .center()
        .radius(6.0)
        .fill(l.quiet)
        .child(text(label).font(sans(11.0, 500.0)).color(l.soft))
}

/// A project's face: its logo, or its first letter on a colour of its own until it has one
pub fn tile(s: &State, project: &Project, size: f32, l: &Look) -> El {
    let round = (size * 0.24).round();
    let face = el().size(size, size).radius(round).continuous_corners();
    match s.logos.get(&project.key) {
        Some((_, logo)) => face
            .fill(l.well)
            .pixels_fit(logo.pixels(), SurfaceFit::Fill),
        None => letter(&project.title, &project.key, size, round),
    }
}

/// The stand-in for a logo: the first letter, on a hue worked out from the name so it stays the same
pub fn letter(title: &str, seed: &str, size: f32, round: f32) -> El {
    let hue = seed.bytes().fold(7u32, |sum, byte| {
        sum.wrapping_mul(31).wrapping_add(u32::from(byte))
    }) % 360;
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
        .gradient(oklch(0.68, 0.13, hue as f32), oklch(0.56, 0.14, hue as f32))
        .child(
            text(first)
                .font(sans((size * 0.46).round(), 600.0))
                .color(Color::WHITE),
        )
}

/// Carbon's mark or an agent's, on a dark tile of its own so a light mark shows in a light window too
pub fn mark(s: &State, name: &str, size: f32) -> El {
    let inside = (size * 0.62).round();
    let tile = row()
        .size(size, size)
        .radius((size * 0.28).round())
        .continuous_corners()
        .center()
        .fill(oklch(0.21, 0.005, 265.0));
    match s.marks.get(name) {
        Some(image) => tile.child(el().size(inside, inside).pixels(image.pixels())),
        None => tile,
    }
}
