//! Carbon: the local hub for the dev servers on this machine. Always on port 3000, and only on this machine.
//!
//! The same engine sits behind three fronts: the page (served by `server`), the `carbon` command (which talks to a
//! running hub through `client`), and the desktop app (which runs the hub inside itself).

pub mod agents;
pub mod client;
pub mod create;
pub mod detect;
pub mod engine;
pub mod harness;
pub mod identity;
pub mod places;
pub mod server;
