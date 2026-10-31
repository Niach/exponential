//! `ede-compositor`: eDE's Wayland compositor, built on smithay.
//!
//! Floating windows (Mint-like) with magnetic edges and edge tiling,
//! xdg-shell, wlr-layer-shell, damage-tracked rendering, and a config file.
//! Backends: `winit` (nested, for development), `udev` (the real session:
//! DRM/KMS + libinput, feature `udev`) and `headless` (CI).

pub mod backend;
pub mod config;
pub mod grabs;
mod handlers;
pub mod input;
pub mod layout;
pub mod state;

pub use config::Config;
pub use state::Ede;
