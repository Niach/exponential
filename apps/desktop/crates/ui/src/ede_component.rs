//! #EDE-227: eDE mode. On Exponential Desktop the app runs as a component of
//! the session (`exponential-app`, started by the compositor) and is the
//! login's holder: the account, the install, the keys, the device and the
//! coding sessions. The desktop around it is its chrome, so here it has
//!
//! - no login and no onboarding: the shell's Connect is the one sign-in; a
//!   window opened while signed out offers the way there and nothing else;
//! - no title bar or window frame of its own: its windows ask the compositor
//!   for server-side decorations (eDE's frame, the theme's tokens);
//! - no rail or navigation of its own: the shell links into it;
//! - no updater: the app updates with the desktop's package.
//!
//! The standalone app (Cinnamon, macOS, Windows, an AppImage on eDE) is
//! untouched: none of this is set there.

use std::sync::atomic::{AtomicBool, Ordering};

static COMPONENT: AtomicBool = AtomicBool::new(false);

/// This process is eDE's component. Called once from `app`, before
/// `app.run`.
pub fn set_component() {
    COMPONENT.store(true, Ordering::SeqCst);
}

/// This process is eDE's component (eDE mode).
pub fn is_component() -> bool {
    COMPONENT.load(Ordering::Relaxed)
}
