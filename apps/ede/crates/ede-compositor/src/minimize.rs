//! Minimize (#EDE-48): Mint's click on the focused taskbar button. A
//! minimized window leaves the space (unmapped: not drawn, not hit, not
//! focusable, skipped by the fallback Alt+Tab and by the lock's focus
//! fallback as any unmapped window is) but keeps its geometry and its
//! workspace, so a restore puts it back where it was. The shell hears
//! `window.minimized` and `window.restored`; a `focus_window` on a
//! minimized window restores it first.

use smithay::desktop::Window;

use crate::state::{set_window_state, window_state, Ede};

impl Ede {
    /// Into the taskbar. A window on another workspace (parked) can be
    /// minimized too; one already minimized is left alone.
    pub fn minimize_window(&mut self, window: &Window) {
        let mut st = window_state(window);
        if st.minimized {
            return;
        }
        let in_space = self.space.element_location(window);
        if in_space.is_none() && !self.workspaces.parked.contains(window) {
            return;
        }
        if let Some(at) = in_space {
            st.parked_at = Some(at);
        }
        st.minimized = true;
        set_window_state(window, st);
        let had_focus = self.focused_window().as_ref() == Some(window);
        self.space.unmap_elem(window);
        self.workspaces.parked.retain(|w| w != window);
        self.minimized.push(window.clone());
        window.set_activated(false);
        if let Some(t) = window.toplevel() {
            t.send_pending_configure();
        }
        #[cfg(feature = "xwayland")]
        if let Some(x11) = window.x11_surface() {
            // WM_STATE Iconic: the X client knows it is minimized.
            let _ = x11.set_mapped(false);
        }
        self.window_event(ede_protocol::events::WINDOW_MINIMIZED, window);
        if had_focus {
            // Focus falls to the window now on top, as on Cinnamon.
            let next = self.space.elements().last().cloned();
            self.focus(next);
        }
    }

    /// Back from the taskbar, where it was. On the active workspace it maps
    /// again; on another it is parked there until that workspace shows (a
    /// `focus_window` switches to it). `true` = it was minimized.
    pub fn restore_window(&mut self, window: &Window) -> bool {
        let mut st = window_state(window);
        if !st.minimized {
            return false;
        }
        st.minimized = false;
        set_window_state(window, st);
        self.minimized.retain(|w| w != window);
        #[cfg(feature = "xwayland")]
        if let Some(x11) = window.x11_surface() {
            let _ = x11.set_mapped(true);
        }
        if st.workspace != self.workspaces.active {
            self.workspaces.parked.push(window.clone());
        } else {
            let at = st.parked_at.filter(|at| {
                let center = at.to_f64() + window.geometry().size.to_f64().to_point().downscale(2.0);
                self.space.output_under(center).next().is_some()
            });
            match at {
                Some(at) => self.space.map_element(window.clone(), at, true),
                // Its output went away meanwhile: a fresh place on the first one.
                None => {
                    self.space.map_element(window.clone(), (0, 0), true);
                    self.rehome_lost_windows(&[window.clone()]);
                }
            }
        }
        self.window_event(ede_protocol::events::WINDOW_RESTORED, window);
        true
    }

    /// A minimized window that closed.
    pub fn forget_minimized(&mut self, window: &Window) {
        self.minimized.retain(|w| w != window);
    }
}
