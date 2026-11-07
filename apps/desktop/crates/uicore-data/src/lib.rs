//! `uicore-data` (#EDE-50): what the shell documents bind, without a
//! renderer. A [`Feed`] is the set of top-level paths every document may
//! read (`/time`, `/windows`, `/apps`, `/shell`, …) with their JSON; the
//! host (Exponential Desktop's shell today, the IDE later) fills it from
//! its [`Source`]s and pushes it into every open surface. The shaping from
//! the compositor's types and from the shell reducer ([`uicore_model`]) is
//! pure and tested here; the sources that read the machine live in
//! [`apps`] (the desktop entries) and [`clock`].

pub mod apps;
#[cfg(unix)]
pub mod clock;
pub mod feed;
pub mod windows;

pub use feed::{shell_feed, windows_feed, Feed};
pub use windows::{window_events, workspace_events};

use serde_json::Value;

/// One place the shell reads from: a path and its current JSON.
pub trait Source {
    /// The top-level path the documents bind (`/windows`).
    fn path(&self) -> &'static str;
    /// The current value; the host calls it when the source says it changed.
    fn value(&self) -> Value;
}
