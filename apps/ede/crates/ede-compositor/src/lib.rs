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
pub mod consent;
pub mod desk;
pub mod doctor;
pub mod fallback;
pub mod focus;
pub mod frames;
pub mod grabs;
pub mod helpers;
mod handlers;
pub mod hold;
pub mod hotkeys;
pub mod idle;
pub mod input;
pub mod ipc;
pub mod keysym;
pub mod launch;
pub mod layout;
#[cfg(any(feature = "udev", test))]
mod libinput_log;
pub mod lock;
pub mod logind;
pub mod minimize;
pub mod outputs;
pub mod peer;
pub mod popup_namespace;
pub mod protected;
pub mod recovery;
pub mod sealed;
pub mod services;
pub mod shortcuts;
pub mod spaces;
pub mod state;
pub mod text;
pub mod workspaces;
pub mod xauth;

pub use config::Config;
/// #EDE-172 (P3): who sees the virtual keyboard and input-method managers.
pub use handlers::input_method::InputMethods;
pub use state::Ede;
