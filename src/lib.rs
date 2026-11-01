//! RustMix Crimson Edition. The window and the socket share one planner.
//! The demo is the desk until a free 828 control port replaces it.

pub const APP_NAME: &str = "RustMix Crimson Edition";
/// Binary name, window class, socket, and settings folder.
pub const PROCESS_NAME: &str = "RustMix";

pub mod desk;
pub mod dummy;
pub mod engine;
pub mod gear;
pub mod guard;
pub mod ipc;
pub mod mcp;
pub mod midi;
pub mod pipewire;
pub mod plan;
pub mod protocol;
pub mod session;
pub mod settings;
pub mod topology;

#[cfg(feature = "ui")]
pub mod ui;
