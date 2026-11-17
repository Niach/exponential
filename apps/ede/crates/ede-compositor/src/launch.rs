//! The person's own launches (#EDE-76). A launch the person made (the
//! shell's menu and taskbar over `launch_token`, a hotkey of ours) carries a
//! one-shot token in `XDG_ACTIVATION_TOKEN` and `DESKTOP_STARTUP_ID`. The
//! window of the process that holds it takes the keyboard as it maps, and
//! an app that hands the token to its running instance activates that
//! instance's window over xdg-activation. Anything else follows
//! [`crate::focus`].

use std::time::{Duration, Instant};

use smithay::{
    desktop::Window,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    wayland::{
        seat::WaylandFocus,
        xdg_activation::{XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData},
    },
};

use crate::state::Ede;

/// How long a launch may take to show its window (a cold start of a large
/// app on a loaded machine).
pub const LAUNCH_TOKEN_TTL: Duration = Duration::from_secs(ede_protocol::LAUNCH_TOKEN_SECS);

/// On a token: minted for a launch the person made.
struct PersonLaunch;
/// On a window that has not mapped yet: activated with a valid token.
struct Activated;

impl Ede {
    /// A one-shot token for a launch the person made.
    pub fn mint_launch_token(&mut self) -> String {
        self.xdg_activation_state.retain_tokens(|_, data| data.timestamp.elapsed() < LAUNCH_TOKEN_TTL);
        let (token, data) = self.xdg_activation_state.create_external_token(None);
        data.user_data.insert_if_missing_threadsafe(|| PersonLaunch);
        token.as_str().to_string()
    }

    /// Uses up `token` when it is a launch token still in time at `now`.
    pub fn take_launch_token(&mut self, token: &str, now: Instant) -> bool {
        let token = XdgActivationToken::from(token.to_string());
        let live = self.xdg_activation_state.data_for_token(&token).is_some_and(|d| d.user_data.get::<PersonLaunch>().is_some() && now.saturating_duration_since(d.timestamp) < LAUNCH_TOKEN_TTL);
        live && self.xdg_activation_state.remove_token(&token)
    }

    /// Did the person start `window`: its process carries a launch token
    /// (X11: `_NET_STARTUP_ID` too), or it was activated with one before it
    /// mapped? Uses the token up.
    pub fn person_started(&mut self, window: &Window) -> bool {
        let mut tokens: Vec<String> = self.window_pid(window).map(environ_tokens).unwrap_or_default();
        #[cfg(feature = "xwayland")]
        tokens.extend(window.x11_surface().and_then(|x| x.startup_id()));
        let now = Instant::now();
        // Every one is used up, not only the first.
        let launched = tokens.iter().filter(|t| self.take_launch_token(t, now)).count() > 0;
        launched || window.user_data().get::<Activated>().is_some()
    }

    /// [`Self::spawn`] for a launch the person made here (a hotkey).
    pub fn spawn_for_person(&mut self, argv: &[String]) {
        let token = self.mint_launch_token();
        if let Some(child) = self.spawn_with_env(argv, &[("XDG_ACTIVATION_TOKEN", &token), ("DESKTOP_STARTUP_ID", &token)]) {
            drop(child);
        }
    }
}

/// The launch tokens in a process's environment as it started.
fn environ_tokens(pid: u32) -> Vec<String> {
    let environ = std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
    environ
        .split(|b| *b == 0)
        .filter_map(|var| std::str::from_utf8(var).ok())
        .filter_map(|var| var.strip_prefix("XDG_ACTIVATION_TOKEN=").or_else(|| var.strip_prefix("DESKTOP_STARTUP_ID=")))
        .map(String::from)
        .collect()
}

impl XdgActivationHandler for Ede {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.xdg_activation_state
    }

    // A client's own token counts only while that client has the keyboard:
    // the focused app may bring another window forward, nobody else.
    fn token_created(&mut self, _token: XdgActivationToken, data: XdgActivationTokenData) -> bool {
        let focus = self.desk.seat.get_keyboard().and_then(|k| k.current_focus());
        focus.and_then(|f| f.client()).is_some_and(|c| Some(c.id()) == data.client_id)
    }

    fn request_activation(&mut self, token: XdgActivationToken, data: XdgActivationTokenData, surface: WlSurface) {
        // One-shot; a client's own token was checked when it was made.
        let valid = self.xdg_activation_state.remove_token(&token) && data.timestamp.elapsed() < LAUNCH_TOKEN_TTL;
        if !valid {
            return;
        }
        let mapped = self.desk.space.elements().find(|w| w.wl_surface().is_some_and(|s| *s == surface)).cloned();
        if let Some(window) = mapped {
            self.focus(Some(window));
        } else if let Some(window) = self.desk.unmapped.iter().find(|w| w.wl_surface().is_some_and(|s| *s == surface)) {
            window.user_data().insert_if_missing(|| Activated);
        }
    }
}
