//! Carbon: the local hub for everything in ~/dev. Always on port 3000, and only on this machine.
//!
//! The same engine sits behind three fronts: the page (served by `server`), the `carbon` command (which talks to a
//! running hub through `client`), and the macOS app (which runs the hub inside itself).

pub mod agents;
pub mod client;
pub mod create;
pub mod detect;
pub mod engine;
pub mod harness;
pub mod server;
