//! Who takes the keyboard (#EDE-82). A window that maps, or asks to come in
//! front (fullscreen, an X11 unminimize), takes it only when the focused app
//! opened it or when nothing holds it. Any other window opens behind the one
//! being typed into, not activated; the taskbar lists it like any window.
//! While an Exclusive prompt holds the keyboard no window takes it
//! ([`Ede::focus`]).

use smithay::{
    desktop::Window,
    reexports::wayland_server::Resource,
    utils::{IsAlive, Logical, Point},
    wayland::seat::WaylandFocus,
};

use crate::state::Ede;

impl Ede {
    /// The window being typed into: the focused one, or the one that gets
    /// the keyboard back when the Exclusive prompt holding it closes.
    pub fn keyboard_window(&self) -> Option<Window> {
        #[cfg(feature = "xwayland")]
        let pending = self.desk.pending_x11_focus.as_ref().and_then(|x| self.desk.space.elements().find(|w| w.x11_surface() == Some(x)).cloned());
        #[cfg(not(feature = "xwayland"))]
        let pending = None;
        self.focused_window().or(pending).or_else(|| self.prompt_previous_window())
    }

    /// May `window`, mapping or asking to come in front, take the keyboard?
    pub fn takes_keyboard(&self, window: &Window) -> bool {
        if self.exclusive_prompt().is_some() {
            return false;
        }
        if let Some(holder) = self.keyboard_window() {
            return holder == *window || self.same_app(&holder, window);
        }
        // A shell surface (the menu, a search) or the lock holds it: theirs.
        // Nothing holds it: nobody loses what they type.
        self.desk.seat.get_keyboard().unwrap().current_focus().is_none_or(|f| !f.alive())
    }

    /// Did the app of `holder` open `window`? The same Wayland client, a
    /// process started by the holder's (a command typed into a terminal),
    /// or on X11 (one connection for every app) the same `_NET_WM_PID` or a
    /// window transient for the holder.
    fn same_app(&self, holder: &Window, window: &Window) -> bool {
        #[cfg(feature = "xwayland")]
        if let (Some(h), Some(w)) = (holder.x11_surface(), window.x11_surface()) {
            if w.is_transient_for() == Some(h.window_id()) {
                return true;
            }
        }
        let wayland = holder.toplevel().zip(window.toplevel());
        if wayland.is_some_and(|(h, w)| h.wl_surface().id().same_client_as(&w.wl_surface().id())) {
            return true;
        }
        let (Some(holder), Some(pid)) = (self.window_pid(holder), self.window_pid(window)) else { return false };
        // Two connections of one process are two clients; an X11 window's
        // process is all there is to compare.
        (holder == pid && wayland.is_none()) || descends(pid, holder)
    }

    /// The process behind a window: the client's, or `_NET_WM_PID` on X11.
    pub fn window_pid(&self, window: &Window) -> Option<u32> {
        #[cfg(feature = "xwayland")]
        if let Some(x11) = window.x11_surface() {
            return x11.pid();
        }
        let surface = window.wl_surface()?;
        let client = self.display_handle.get_client(surface.id()).ok()?;
        client.get_credentials(&self.display_handle).ok().map(|c| c.pid as u32)
    }

    /// `window` came without the keyboard: not activated, and behind the
    /// window being typed into.
    pub(crate) fn keep_behind(&mut self, window: &Window) {
        window.set_activated(false);
        if let Some(front) = self.keyboard_window().filter(|f| f != window) {
            self.desk.space.raise_element(&front, true);
            #[cfg(feature = "xwayland")]
            if let (Some(xwm), Some(x11)) = (self.xwm.as_mut(), front.x11_surface()) {
                let _ = xwm.raise_window(x11);
            }
        }
        for w in self.desk.space.elements() {
            if let Some(t) = w.toplevel() {
                t.send_pending_configure();
            }
        }
    }

    /// Moves `window` to `loc`; who is in front and who is activated stay.
    pub(crate) fn remap(&mut self, window: &Window, loc: Point<i32, Logical>) {
        if self.desk.space.element_location(window).is_some() {
            self.desk.space.relocate_element(window, loc);
        } else {
            self.desk.space.map_element(window.clone(), loc, false);
        }
    }
}

/// Is `ancestor` a parent, grandparent… of `pid` (never `pid` itself)?
pub fn descends(mut pid: u32, ancestor: u32) -> bool {
    for _ in 0..64 {
        let Some(parent) = parent_of(pid) else { return false };
        if parent == ancestor {
            return true;
        }
        if parent <= 1 {
            return false;
        }
        pid = parent;
    }
    false
}

fn parent_of(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // "pid (comm) state ppid …"; the name may hold spaces and brackets.
    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::descends;

    #[test]
    fn a_child_descends_from_its_parent_and_nobody_from_itself() {
        let me = std::process::id();
        let mut child = std::process::Command::new("sh").args(["-c", "sleep 30 & wait"]).spawn().unwrap();
        assert!(descends(child.id(), me));
        assert!(!descends(me, child.id()));
        assert!(!descends(me, me), "a second connection of one process is its own client");
        let _ = child.kill();
        let _ = child.wait();
    }
}
