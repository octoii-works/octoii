//! Octoii's node, the `octoii` program: Siphon and its core plugins on this computer — the browser, the terminals, the
//! MCP servers — its Home kept, served to the person's clients on the loopback address, and reached by its own commands
//! through its control socket.

pub mod access;
pub mod cli;
pub mod control;
pub mod credentials;
mod files;
pub mod home;
pub mod http;
pub mod pairing;
pub mod qr;
pub mod serve;
pub mod service;
