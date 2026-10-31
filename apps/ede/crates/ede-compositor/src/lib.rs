//! `ede-compositor`: eDE's Wayland compositor, built on smithay.
//!
//! Floating windows (Mint-like) with magnetic edges and edge tiling,
//! xdg-shell, wlr-layer-shell, damage-tracked rendering, and a config file.
//! ext-session-lock, idle notify/inhibit, input methods, and protected
//! surfaces that are never captured, lent or driven.
//! Backends: `winit` (nested, for development), `udev` (the real session:
//! DRM/KMS + libinput, feature `udev`) and `headless` (CI).

pub mod backend;
pub mod config;
pub mod doctor;
pub mod grabs;
mod handlers;
pub mod idle;
pub mod input;
pub mod layout;
pub mod lock;
pub mod logind;
pub mod protected;
pub mod state;

pub use config::Config;
pub use state::Ede;
