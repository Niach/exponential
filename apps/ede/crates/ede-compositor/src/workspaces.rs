//! Workspaces (#EDE-18): numbered 1 to 9, shared by every output as on
//! Cinnamon. A window lives on one workspace; the windows of the others
//! are parked (unmapped from the space, their place remembered) so
//! rendering, hit-testing, the switcher and the taskbar only ever see the
//! active one. Super+1…9 switch, Super+Shift+1…9 move the focused window
//! along, Ctrl+Alt+Left/Right step (Shift moves the window). The shell
//! hears `workspace.switched` and can switch or move over ede-protocol.

use serde_json::json;
use smithay::desktop::Window;

use crate::{
    desk::DeskId,
    state::{set_window_state, window_state, Ede},
};

/// Never more than this many.
pub const MAX: usize = 9;
/// Always at least this many listed (Cinnamon's default of four).
pub const MIN_LISTED: usize = 4;

#[derive(Debug, Default)]
pub struct Workspaces {
    /// The active index (0-based).
    pub active: usize,
    /// Windows on the other workspaces, bottom to top of their stacking.
    pub parked: Vec<Window>,
}

impl Ede {
    pub fn active_workspace(&self) -> usize {
        self.desk.workspaces.active
    }

    /// How many to list: the highest in use plus one, at least [`MIN_LISTED`].
    pub fn workspace_count(&self) -> usize {
        let highest = self.desk.workspaces.parked.iter().chain(&self.desk.minimized).map(|w| window_state(w).workspace).chain(std::iter::once(self.desk.workspaces.active)).max().unwrap_or(0);
        (highest + 1).max(MIN_LISTED).min(MAX)
    }

    /// Switches to `index` (clamped to [`MAX`]); `false` = already there.
    pub fn switch_workspace(&mut self, index: usize) -> bool {
        let index = index.min(MAX - 1);
        if index == self.desk.workspaces.active || self.is_locked() {
            return false;
        }
        // Park the active workspace's windows, bottom to top.
        let leaving: Vec<Window> = self.desk.space.elements().cloned().collect();
        for w in &leaving {
            let mut st = window_state(w);
            st.parked_at = self.desk.space.element_location(w);
            set_window_state(w, st);
            self.desk.space.unmap_elem(w);
        }
        self.desk.workspaces.parked.extend(leaving);
        self.desk.workspaces.active = index;
        // Bring the new one's windows back where they were.
        let arriving: Vec<Window> = self.desk.workspaces.parked.iter().filter(|w| window_state(w).workspace == index).cloned().collect();
        self.desk.workspaces.parked.retain(|w| window_state(w).workspace != index);
        for w in &arriving {
            self.map_parked(w, false);
        }
        let top = arriving.last().cloned();
        self.focus(DeskId::PERSON, top);
        let pos = self.desk.seat.get_pointer().unwrap().current_location();
        self.pointer_motion_to(pos, 0);
        self.emit(ede_protocol::events::WORKSPACE_SWITCHED, json!({ "workspace": index }));
        true
    }

    /// Moves `window` to workspace `index`; it leaves the screen unless that
    /// is the active one. `follow` switches along with it.
    pub fn move_to_workspace(&mut self, window: &Window, index: usize, follow: bool) {
        let index = index.min(MAX - 1);
        let mut st = window_state(window);
        if st.workspace == index {
            if follow {
                self.switch_workspace(index);
            }
            return;
        }
        st.workspace = index;
        if st.minimized {
            // Stays in the taskbar; it comes back on its new workspace.
            set_window_state(window, st);
            if follow {
                self.switch_workspace(index);
            }
        } else if index != self.desk.workspaces.active {
            st.parked_at = self.desk.space.element_location(window);
            set_window_state(window, st);
            let had_focus = self.focused_window(DeskId::PERSON).as_ref() == Some(window);
            self.desk.space.unmap_elem(window);
            self.desk.workspaces.parked.push(window.clone());
            if had_focus {
                let next = self.desk.space.elements().last().cloned();
                self.focus(DeskId::PERSON, next);
            }
            if follow {
                self.switch_workspace(index);
                self.focus(DeskId::PERSON, Some(window.clone()));
            }
        } else {
            set_window_state(window, st);
            self.desk.workspaces.parked.retain(|w| w != window);
            self.map_parked(window, true);
            self.focus(DeskId::PERSON, Some(window.clone()));
        }
        self.window_event(ede_protocol::events::WINDOW_CHANGED, window);
    }

    /// Maps a parked or minimized `window` back at its place (#EDE-139): a
    /// tiled or fullscreen one fills its output as it is now (its scale,
    /// mode or panel may have changed meanwhile); one whose place is on no
    /// output any more (it went away) opens on the first one.
    pub(crate) fn map_parked(&mut self, window: &Window, activate: bool) {
        let st = window_state(window);
        let fills = st.tile.is_some() || st.fullscreen.is_some();
        let output = st.parked_at.and_then(|at| {
            // A tiled or fullscreen one: the output its corner is on (its
            // size is the old output's until it is fitted again).
            let probe = if fills { at.to_f64() } else { at.to_f64() + window.geometry().size.to_f64().to_point().downscale(2.0) };
            self.desk.space.output_under(probe).next().cloned().map(|o| (at, o))
        });
        let Some((at, output)) = output else {
            self.desk.space.map_element(window.clone(), (0, 0), activate);
            self.rehome_lost_windows(&[window.clone()]);
            return;
        };
        self.desk.space.map_element(window.clone(), at, activate);
        if st.fullscreen.is_some() {
            self.fit_fullscreen(window, &output);
        } else if let Some(tile) = st.tile {
            self.tile_on(window, tile, &output);
        }
    }

    /// A window that closed while parked.
    pub fn forget_parked(&mut self, window: &Window) {
        self.desk.workspaces.parked.retain(|w| w != window);
    }

    /// Every window: on screen, parked on another workspace, or minimized.
    pub fn all_windows(&self) -> Vec<Window> {
        self.desk.space.elements().cloned().chain(self.desk.workspaces.parked.iter().cloned()).chain(self.desk.minimized.iter().cloned()).collect()
    }

    /// The workspaces as ede-protocol lists them.
    pub fn workspace_infos(&self) -> Vec<ede_protocol::WorkspaceInfo> {
        let count = self.workspace_count();
        (0..count)
            .map(|i| ede_protocol::WorkspaceInfo {
                index: i,
                active: i == self.desk.workspaces.active,
                windows: self.all_windows().iter().filter(|w| window_state(w).workspace == i).map(crate::ipc::window_id).collect(),
            })
            .collect()
    }
}
