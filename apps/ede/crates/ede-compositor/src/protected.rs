//! Protected surfaces: the lock screen, polkit/keyring/password prompts and
//! denylisted apps (password managers, banking; `[protected] app_ids`) are
//! never captured, lent or driven. The compositor enforces it, not the agent
//! (plan v5, #NEW-106):
//!
//! - captured: [`crate::backend::capture_elements`] draws them solid black,
//!   and a locked session black altogether;
//! - lent: [`Ede::may_lend`] is false;
//! - driven: [`Ede::drive_key`] / [`Ede::drive_pointer_motion`] /
//!   [`Ede::drive_pointer_button`] (the agent input path) refuse them, the
//!   virtual-keyboard protocol cannot type into them, input methods lose
//!   their keyboard grab and text-input stays off on them.
//!
//! A window is protected when its app id (X11: WM_CLASS) or its process's
//! executable name is on the list. Spoofing an id only hides an app from
//! capture; it never unprotects one.

use smithay::{
    backend::input::{ButtonState, KeyState},
    desktop::{find_popup_root_surface, Window},
    input::keyboard::Keycode,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    utils::{Logical, Point},
    wayland::compositor::get_parent,
};

use crate::state::Ede;

/// Protected out of the box (case-insensitive): polkit agents, keyring and
/// pinentry prompts, ssh askpass, password managers.
pub const BUILTIN: &[&str] = &[
    "polkit-gnome-authentication-agent-1",
    "polkit-mate-authentication-agent-1",
    "polkit-kde-authentication-agent-1",
    "org.kde.polkit-kde-authentication-agent-1",
    "xfce-polkit",
    "polkit-xfce-authentication-agent-1",
    "lxpolkit",
    "gcr-prompter",
    "org.gnome.keyring.SystemPrompter",
    "pinentry",
    "pinentry-gnome3",
    "pinentry-gtk-2",
    "pinentry-qt",
    "ssh-askpass",
    "ksshaskpass",
    "org.keepassxc.KeePassXC",
    "keepassxc",
    "1password",
    "bitwarden",
    "com.bitwarden.desktop",
    "org.gnome.World.Secrets",
    "org.gnome.seahorse.Application",
    "seahorse",
];

/// Why driving input was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// The session is locked.
    Locked,
    /// The target is a protected surface.
    Protected,
}

/// The identities a window answers to: app id or WM_CLASS, and the process's
/// executable name.
pub fn window_ids(ede: &Ede, window: &Window) -> Vec<String> {
    let mut ids = Vec::new();
    let mut pid = None;
    if let Some(toplevel) = window.toplevel() {
        let app_id = smithay::wayland::compositor::with_states(toplevel.wl_surface(), |s| {
            s.data_map.get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>().and_then(|d| d.lock().unwrap().app_id.clone())
        });
        ids.extend(app_id);
        pid = ede.display_handle.get_client(toplevel.wl_surface().id()).ok().and_then(|c| c.get_credentials(&ede.display_handle).ok()).map(|c| c.pid as u32);
    }
    #[cfg(feature = "xwayland")]
    if let Some(x11) = window.x11_surface() {
        ids.push(x11.class());
        ids.push(x11.instance());
        pid = x11.pid();
    }
    if let Some(exe) = pid.and_then(|pid| std::fs::read_link(format!("/proc/{pid}/exe")).ok()) {
        ids.extend(exe.file_name().map(|n| n.to_string_lossy().into_owned()));
    }
    ids.retain(|id| !id.is_empty());
    ids
}

impl Ede {
    pub fn is_protected_window(&self, window: &Window) -> bool {
        let ids = window_ids(self, window);
        let listed = |id: &String| BUILTIN.iter().copied().chain(self.config.protected.app_ids.iter().map(String::as_str)).any(|p| p.eq_ignore_ascii_case(id));
        ids.iter().any(listed)
    }

    /// Any surface: a lock surface, or part of a protected window (its
    /// subsurfaces and popups included).
    pub fn is_protected_surface(&self, surface: &WlSurface) -> bool {
        if self.is_lock_surface(surface) {
            return true;
        }
        let root = self.toplevel_root(surface);
        self.is_lock_surface(&root) || self.window_for_surface(&root).is_some_and(|w| self.is_protected_window(&w))
    }

    /// The toplevel a surface belongs to, through subsurfaces and popups.
    fn toplevel_root(&self, surface: &WlSurface) -> WlSurface {
        let climb = |s: &WlSurface| {
            let mut root = s.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            root
        };
        let mut root = climb(surface);
        if let Some(popup) = self.popups.find_popup(&root) {
            if let Ok(top) = find_popup_root_surface(&popup) {
                root = climb(&top);
            }
        }
        root
    }

    /// The keyboard goes to a protected surface (or the lock screen): no
    /// input method, no virtual keyboard, no agent keys.
    pub fn keyboard_focus_protected(&self) -> bool {
        self.is_locked() || self.seat.get_keyboard().and_then(|k| k.current_focus()).is_some_and(|s| self.is_protected_surface(&s))
    }

    /// May `window` be lent to another seat (an agent's space)?
    pub fn may_lend(&self, window: &Window) -> bool {
        !self.is_locked() && !self.is_protected_window(window)
    }

    /// May a capture show `window`? (Protected ones are drawn black.)
    pub fn may_capture(&self, window: &Window) -> bool {
        self.may_lend(window)
    }

    /// A key from an agent (or any synthetic source) to the focused surface.
    /// Compositor bindings never fire from it, and it does not count as the
    /// person being active.
    pub fn drive_key(&mut self, keycode: Keycode, state: KeyState, time: u32) -> Result<(), Refused> {
        if self.is_locked() {
            return Err(Refused::Locked);
        }
        if self.keyboard_focus_protected() {
            return Err(Refused::Protected);
        }
        self.key(keycode, state, time, false);
        Ok(())
    }

    pub fn drive_pointer_motion(&mut self, pos: Point<f64, Logical>, time: u32) -> Result<(), Refused> {
        self.may_drive_at(pos)?;
        self.pointer_motion_to(pos, time);
        Ok(())
    }

    pub fn drive_pointer_button(&mut self, button: u32, state: ButtonState, time: u32) -> Result<(), Refused> {
        let pos = self.seat.get_pointer().unwrap().current_location();
        self.may_drive_at(pos)?;
        self.pointer_button(button, state, time);
        Ok(())
    }

    fn may_drive_at(&self, pos: Point<f64, Logical>) -> Result<(), Refused> {
        if self.is_locked() {
            return Err(Refused::Locked);
        }
        match self.surface_under(pos) {
            Some((s, _)) if self.is_protected_surface(&s) => Err(Refused::Protected),
            _ => Ok(()),
        }
    }

    /// Before a key reaches anyone: an input method holding the keyboard
    /// loses it while a protected surface (or the lock) has focus, so it
    /// never reads a password. It grabs again when its next text field
    /// activates it.
    pub(crate) fn release_ime_grab_if_protected(&mut self) {
        use smithay::wayland::input_method::InputMethodSeat;
        let keyboard = self.seat.get_keyboard().unwrap();
        if keyboard.is_grabbed() && self.seat.input_method().keyboard_grabbed() && self.keyboard_focus_protected() {
            keyboard.unset_grab(self);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BUILTIN;

    #[test]
    fn the_builtin_list_covers_the_prompts_ede_starts() {
        // packaging/ede-polkit-agent runs one of these.
        for agent in ["polkit-gnome-authentication-agent-1", "polkit-mate-authentication-agent-1", "gcr-prompter"] {
            assert!(BUILTIN.contains(&agent), "{agent}");
        }
    }
}
