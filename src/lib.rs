//! Carbon: the local hub for the dev servers on this machine, and only on this machine.
//!
//! The same engine sits behind two fronts: the desktop app (which runs the hub inside itself), and the `carbon`
//! command (which talks to a running hub through `client`, over the small API in `server`).

pub mod agents;
pub mod client;
pub mod create;
pub mod detect;
pub mod engine;
pub mod harness;
pub mod identity;
pub mod places;
pub mod ports;
pub mod server;
pub mod stack;
