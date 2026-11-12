//! `exponential-compositor`: eDE's Wayland compositor, built on smithay.
//!
//! Floating windows (Mint-like) with magnetic edges and edge tiling,
//! workspaces, xdg-shell, wlr-layer-shell, fractional scale, damage-tracked
//! rendering, and a config file (outputs: mode, scale, position).
//! ext-session-lock, idle notify/inhibit, input methods, and protected
//! surfaces that are never captured, lent or driven. `ede-protocol`, the
//! control socket with capability tokens ([`ipc`]); the hotkeys the shell
//! answers ([`hotkeys`]); the person's input held out while an agent
//! drives the seat ([`hold`]).
//! Backends: `winit` (nested, for development), `udev` (the real session:
//! DRM/KMS + libinput, feature `udev`) and `headless` (CI).

pub mod backend;
pub mod clipboard;
pub mod config;
pub mod doctor;
pub mod fallback;
pub mod frames;
pub mod grabs;
mod handlers;
pub mod hold;
pub mod hotkeys;
pub mod idle;
pub mod input;
pub mod ipc;
pub mod keysym;
pub mod layout;
pub mod lock;
pub mod logind;
pub mod minimize;
pub mod outputs;
pub mod protected;
pub mod recovery;
pub mod sealed;
pub mod shortcuts;
pub mod state;
pub mod text;
pub mod workspaces;

pub use config::Config;
pub use state::Ede;
