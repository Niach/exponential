//! The window menu (#EDE-253), Cinnamon's windowMenu.js over Muffin: the
//! compositor says where and for which window (`window.menu`), the shell
//! draws it (`window-menu.express`, the panel's one Menu look) and answers
//! with ede-protocol's `window_action`, `move_to_workspace` and
//! `close_window`. Opened by Alt+Space (activate-window-menu: at the
//! window's top-left corner), Alt+middle-click (Muffin's mouse-button
//! modifier with resize-with-right-button: right resizes, middle is the
//! menu) and a right click on our own title bar (action-right-click-titlebar).
//!
//! Also what its entries need of the compositor: Always on Top (a window
//! kept above the others: [`Ede::restack_above`]), Always on Visible
//! Workspace (a sticky window rides along on a switch, [`crate::workspaces`])
//! and Move Titlebar Onscreen. And the keyboard applet's layouts
//! (keyboard@cinnamon.org): [`Ede::keyboard_layouts`].

use ede_protocol::{KeyboardLayouts, WindowAction, WindowMenu};
use smithay::{
    desktop::Window,
    utils::{Logical, Point, Rectangle},
};

use crate::{
    desk::DeskId,
    layout::Tile,
    state::{set_window_state, window_state, Ede},
};

/// How much of a title bar must stay on an output (Muffin keeps a strip
/// of the frame reachable; it shoves at least this much back).
const TITLE_KEEP: i32 = 75;

impl Ede {
    /// Alt+Space: the focused window's menu at its top-left corner (Muffin:
    /// the client rectangle's origin).
    pub fn window_menu_focused(&mut self) {
        let Some(window) = self.focused_window(DeskId::PERSON) else { return };
        let Some(geo) = self.desk.space.element_geometry(&window) else { return };
        self.window_menu(&window, geo.loc);
    }

    /// `window.menu` for `window` at `at` (global logical coordinates).
    pub fn window_menu(&mut self, window: &Window, at: Point<i32, Logical>) {
        if self.is_locked() || !self.shell_listening() || crate::ipc::x11_override_redirect(window) {
            return;
        }
        let st = window_state(window);
        let (min, max) = crate::state::size_limits(window);
        let fixed = max.w > 0 && max.h > 0 && min == max;
        let menu = WindowMenu {
            window: crate::ipc::window_id(window),
            x: at.x,
            y: at.y,
            maximized: st.tile == Some(Tile::Maximized),
            above: st.above,
            sticky: st.sticky,
            offscreen: self.titlebar_offscreen(window),
            resizable: !fixed,
            workspace: st.workspace,
            workspaces: self.workspace_count(),
        };
        self.emit(ede_protocol::events::WINDOW_MENU, serde_json::to_value(menu).unwrap());
    }

    /// One window menu entry ([`WindowAction`]) on `window`.
    pub fn window_action(&mut self, window: &Window, action: WindowAction) {
        match action {
            WindowAction::Minimize => self.minimize_window(window),
            WindowAction::Maximize => {
                if window_state(window).tile != Some(Tile::Maximized) {
                    self.tile(window, Tile::Maximized);
                }
            }
            WindowAction::Unmaximize => {
                if window_state(window).tile.is_some() {
                    self.untile(window, None);
                }
            }
            WindowAction::Move | WindowAction::Resize => {
                self.focus(DeskId::PERSON, Some(window.clone()));
                self.begin_keyboard_grab_on(window, action == WindowAction::Resize);
            }
            WindowAction::MoveTitlebarOnscreen => self.shove_titlebar_onscreen(window),
            WindowAction::Above | WindowAction::NotAbove => {
                let mut st = window_state(window);
                st.above = action == WindowAction::Above;
                set_window_state(window, st);
                if st.above {
                    self.restack_above();
                } else {
                    // Back among the others, in front of them as it was.
                    self.desk.space.raise_element(window, false);
                    self.restack_above();
                }
            }
            WindowAction::Stick | WindowAction::Unstick => {
                let mut st = window_state(window);
                st.sticky = action == WindowAction::Stick;
                // A sticky window lives on whichever workspace is shown.
                if st.sticky && !st.minimized && self.desk.space.element_location(window).is_some() {
                    st.workspace = self.active_workspace();
                }
                set_window_state(window, st);
            }
        }
        self.window_event(ede_protocol::events::WINDOW_CHANGED, window);
    }

    /// Always on Top: every such window back in front of the others, in
    /// their own order (Muffin's above layer).
    pub fn restack_above(&mut self) {
        let above: Vec<Window> = self.desk.space.elements().filter(|w| window_state(w).above).cloned().collect();
        for w in above {
            self.desk.space.raise_element(&w, false);
            #[cfg(feature = "xwayland")]
            if let (Some(xwm), Some(x11)) = (self.xwm.as_mut(), w.x11_surface()) {
                let _ = xwm.raise_window(x11);
            }
        }
    }

    /// The title bar (our frame, else the window's top strip) on no output.
    pub fn titlebar_offscreen(&self, window: &Window) -> bool {
        let Some(bar) = self.titlebar_rect(window) else { return false };
        !self.desk.space.outputs().filter_map(|o| self.desk.space.output_geometry(o)).any(|g| g.overlaps(bar))
    }

    fn titlebar_rect(&self, window: &Window) -> Option<Rectangle<i32, Logical>> {
        let framed = self.framed_geometry(window)?;
        let h = self.frame_height(window).max(crate::frames::HEIGHT);
        Some(Rectangle::new(framed.loc, (framed.size.w, h).into()))
    }

    /// Muffin's `meta_window_shove_titlebar_onscreen`: onto the nearest
    /// output, the bar's top on it and at least [`TITLE_KEEP`] of it inside.
    pub fn shove_titlebar_onscreen(&mut self, window: &Window) {
        let (Some(bar), Some(loc)) = (self.titlebar_rect(window), self.desk.space.element_location(window)) else { return };
        let centre = bar.loc.to_f64() + bar.size.to_f64().to_point().downscale(2.0);
        let nearest = self
            .desk
            .space
            .outputs()
            .filter_map(|o| self.desk.space.output_geometry(o))
            .min_by_key(|g| {
                let x = centre.x.clamp(g.loc.x as f64, (g.loc.x + g.size.w) as f64);
                let y = centre.y.clamp(g.loc.y as f64, (g.loc.y + g.size.h) as f64);
                ((x - centre.x).powi(2) + (y - centre.y).powi(2)) as i64
            });
        let Some(out) = nearest else { return };
        let keep = TITLE_KEEP.min(bar.size.w);
        let x = bar.loc.x.clamp(out.loc.x - bar.size.w + keep, out.loc.x + out.size.w - keep);
        let y = bar.loc.y.clamp(out.loc.y, out.loc.y + out.size.h - bar.size.h);
        let to = loc + Point::from((x - bar.loc.x, y - bar.loc.y));
        if window_state(window).tile.is_some() {
            self.untile(window, Some(to));
        } else {
            self.move_window(window, to, false);
        }
    }

    /// The layouts of `[keyboard] layout` and the active one.
    pub fn keyboard_layouts(&mut self) -> KeyboardLayouts {
        let keyboard = self.desk.seat.get_keyboard().unwrap();
        let (index, layouts) = keyboard.with_xkb_state(self, |ctx| {
            let xkb = ctx.xkb().lock().unwrap();
            let layouts: Vec<String> = xkb.layouts().map(|l| xkb.layout_name(l).to_string()).collect();
            (xkb.active_layout().0 as usize, layouts)
        });
        let short = self.config.keyboard.layout.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        KeyboardLayouts { index, layouts, short }
    }

    /// Makes layout `index` the active one; `None` = no such layout.
    pub fn set_keyboard_layout(&mut self, index: usize) -> Option<KeyboardLayouts> {
        let count = self.keyboard_layouts().layouts.len();
        if index >= count {
            return None;
        }
        let keyboard = self.desk.seat.get_keyboard().unwrap();
        keyboard.with_xkb_state(self, |mut ctx| ctx.set_layout(smithay::input::keyboard::Layout(index as u32)));
        let now = self.keyboard_layouts();
        self.emit(ede_protocol::events::KEYBOARD_LAYOUT, serde_json::to_value(&now).unwrap());
        Some(now)
    }
}
