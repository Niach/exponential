//! Input, backend-agnostic: every backend turns its events into these calls,
//! and the headless tests call them directly.
//!
//! Compositor bindings (Mint-like): Alt+drag moves a window, Alt+F4 closes
//! it, Super+Left/Right tile to a half, Super+Up maximizes, Super+Down
//! restores, Super+L locks, Super+Shift+E ends the session, Ctrl+Alt+F1…F12
//! switch VTs. While locked only VT switching stays. The keys the shell
//! answers (Alt+Tab, Super, the media keys) are [`crate::hotkeys`].

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent, KeyState, KeyboardKeyEvent,
        PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::{keysyms, FilterResult, Keycode, ModifiersState},
        pointer::{AxisFrame, ButtonEvent, Focus, GrabStartData, MotionEvent, RelativeMotionEvent},
    },
    utils::{Logical, Point, SERIAL_COUNTER},
};

use crate::{
    grabs::{MoveSurfaceGrab, BTN_LEFT},
    layout::Tile,
    state::{window_state, Ede},
};

/// What a key press did when the compositor kept it for itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Super+Shift+E: end the session (exponential-session logs out on a clean exit).
    Quit,
    /// Ctrl+Alt+F1…F12 (xkb's XF86Switch_VT_n).
    SwitchVt(i32),
    Close,
    Tile(Tile),
    Restore,
    /// Super+L: start the lock client.
    Lock,
    /// A key the shell answers ([`crate::hotkeys`]); the event is sent after delivery.
    Hotkey,
    /// Super+1…9, Ctrl+Alt+Left/Right.
    Workspace(usize),
    WorkspaceStep(i32),
    /// Super+Shift+1…9, Ctrl+Alt+Shift+Left/Right: the focused window along.
    MoveToWorkspace(usize),
    MoveToWorkspaceStep(i32),
    /// Super+Shift+R: the stock shell documents back, the shell restarted (#EDE-16).
    RestoreShell,
    /// A global shortcut's key (by keycode) went down / up (#EDE-15).
    Shortcut(u32, bool),
}

impl Ede {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { .. }
            | InputEvent::PointerMotion { .. }
            | InputEvent::PointerMotionAbsolute { .. }
            | InputEvent::PointerButton { .. }
            | InputEvent::PointerAxis { .. } => self.note_activity(),
            _ => {}
        }
        match event {
            InputEvent::Keyboard { event, .. } => self.keyboard_key(event.key_code(), event.state(), event.time().millis()),
            InputEvent::PointerMotion { event, .. } => {
                let pos = self.seat.get_pointer().unwrap().current_location() + event.delta();
                if !self.person_pointer_motion_to(pos, event.time().millis()) {
                    return;
                }
                let pointer = self.seat.get_pointer().unwrap();
                let under = self.surface_under(pointer.current_location());
                pointer.relative_motion(
                    self,
                    under,
                    &RelativeMotionEvent { delta: event.delta(), delta_unaccel: event.delta_unaccel(), time: event.time() },
                );
                pointer.frame(self);
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let Some(output) = self.space.outputs().next().cloned() else { return };
                let Some(geo) = self.space.output_geometry(&output) else { return };
                let pos = event.position_transformed(geo.size) + geo.loc.to_f64();
                self.person_pointer_motion_to(pos, event.time().millis());
            }
            InputEvent::PointerButton { event, .. } => self.person_pointer_button(event.button_code(), event.state(), event.time().millis()),
            // Held (#EDE-23): the person's wheel reaches no window.
            InputEvent::PointerAxis { .. } if self.person_input_held() => {}
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();
                let amount = |axis| event.amount(axis).unwrap_or_else(|| event.amount_v120(axis).unwrap_or(0.0) * 15.0 / 120.);
                let mut frame = AxisFrame::new(event.time()).source(source);
                for axis in [Axis::Horizontal, Axis::Vertical] {
                    let value = amount(axis);
                    if value != 0.0 {
                        frame = frame.value(axis, value);
                        if let Some(discrete) = event.amount_v120(axis) {
                            frame = frame.v120(axis, discrete as i32);
                        }
                    }
                    if source == AxisSource::Finger && event.amount(axis) == Some(0.0) {
                        frame = frame.stop(axis);
                    }
                }
                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }

    /// Moves the pointer to `pos` (clamped onto the outputs) and updates focus.
    pub fn pointer_motion_to(&mut self, pos: Point<f64, Logical>, time: u32) {
        let pos = self.clamp_to_outputs(pos);
        let pointer = self.seat.get_pointer().unwrap();
        let under = self.surface_under(pos);
        pointer.motion(self, under, &MotionEvent { location: pos, serial: SERIAL_COUNTER.next_serial(), time: smithay::backend::input::InputTime::from_millis(time) });
        pointer.frame(self);
    }

    pub fn pointer_button(&mut self, button: u32, state: ButtonState, time: u32) {
        let pointer = self.seat.get_pointer().unwrap();
        let serial = SERIAL_COUNTER.next_serial();

        if state == ButtonState::Pressed {
            let hit = self.surface_under(pointer.current_location()).map(|(s, _)| s);
            self.hotkeys_pointer_pressed(hit.as_ref());
        }
        if state == ButtonState::Pressed && self.is_locked() {
            self.lock_pointer_focus();
        } else if state == ButtonState::Pressed && !pointer.is_grabbed() && self.frame_press(button, serial) {
            // A title bar or a resize band took the press (#EDE-16).
        } else if state == ButtonState::Pressed && !pointer.is_grabbed() {
            let location = pointer.current_location();
            let under = self.surface_under(location).map(|(s, _)| s);
            // A press on a shell layer (the Alt-Tab switcher, Expo, Scale)
            // is the shell's: the window under it neither takes focus nor
            // starts an Alt+drag (#EDE-65: Alt is held while a tile is clicked).
            let on_layer = under.as_ref().is_some_and(|s| self.is_layer_surface(s));
            let window = under.as_ref().and_then(|s| self.window_for_surface(s)).or_else(|| if on_layer { None } else { self.space.element_under(location).map(|(w, _)| w.clone()) });
            match (&window, under) {
                (Some(_), _) => self.focus(window.clone()),
                // A layer surface that takes the keyboard (the shell's menu,
                // a search) gets it on a click; the panel and a wallpaper
                // leave the app's focus alone (as on Cinnamon).
                (None, Some(surface)) => {
                    if self.layer_takes_keyboard(&surface) || self.popups.find_popup(&surface).is_some() {
                        let keyboard = self.seat.get_keyboard().unwrap();
                        keyboard.set_focus(self, Some(surface), serial);
                    }
                }
                (None, None) => self.focus(None),
            }
            let alt = self.seat.get_keyboard().unwrap().modifier_state().alt;
            if let (Some(window), true, BTN_LEFT) = (window, alt, button) {
                // Alt+drag: move from anywhere in the window.
                if let Some(loc) = self.space.element_location(&window) {
                    let start = GrabStartData { focus: None, button, location };
                    pointer.set_grab(self, MoveSurfaceGrab::new(start, window, loc), serial, Focus::Clear);
                }
            }
        }

        pointer.button(self, &ButtonEvent { button, state, serial, time: smithay::backend::input::InputTime::from_millis(time) });
        pointer.frame(self);
    }

    /// A key from the person's keyboard; held back while an agent holds
    /// their input ([`crate::hold`]), where their Esc ends the hold.
    pub fn keyboard_key(&mut self, keycode: Keycode, state: KeyState, time: u32) {
        self.note_activity();
        if self.person_key_passes(keycode, state) {
            self.key(keycode, state, time, true);
        }
    }

    pub(crate) fn key(&mut self, keycode: Keycode, state: KeyState, time: u32, bindings: bool) {
        self.restore_seat_keymap();
        self.release_ime_grab_if_protected();
        let serial = SERIAL_COUNTER.next_serial();
        let keyboard = self.seat.get_keyboard().unwrap();
        let locked = self.is_locked();
        let listening = self.shell_listening();
        let popup_focused = keyboard.current_focus().is_some_and(|f| self.popups.find_popup(&f).is_some());
        let action = keyboard.input::<Action, _>(self, keycode, state, serial, smithay::backend::input::InputTime::from_millis(time), |ede, mods, handle| {
            if !bindings {
                return FilterResult::Forward;
            }
            let sym = handle.modified_sym().raw();
            // A global shortcut's key: its release always ends it (#EDE-15).
            let code = handle.raw_code().raw();
            if state == KeyState::Released && ede.ipc.shortcuts.active.iter().any(|(k, ..)| *k == code) {
                return FilterResult::Intercept(Action::Shortcut(code, false));
            }
            if locked {
                // Only VT switching; the lock screen gets everything else.
                return match binding(mods, sym) {
                    Some(action @ Action::SwitchVt(_)) if state == KeyState::Pressed => FilterResult::Intercept(action),
                    _ => FilterResult::Forward,
                };
            }
            if state == KeyState::Pressed && sym == keysyms::KEY_Escape && popup_focused {
                ede.hotkeys.fired = Some(crate::hotkeys::Hotkey::PopupsDismiss);
                return FilterResult::Intercept(Action::Hotkey);
            }
            // The key above Tab is Alt+`'s by position, whatever the layout makes of it.
            let hotkey_sym = if code == crate::hotkeys::ABOVE_TAB_KEYCODE && mods.alt { keysyms::KEY_grave } else { sym };
            let (keep, hotkey) = crate::hotkeys::decide(&mut ede.hotkeys, mods, hotkey_sym, state, listening);
            if hotkey.is_some() {
                ede.hotkeys.fired = hotkey;
            }
            if keep {
                return FilterResult::Intercept(Action::Hotkey);
            }
            if state != KeyState::Pressed {
                return FilterResult::Forward;
            }
            match binding(mods, sym) {
                Some(action) => FilterResult::Intercept(action),
                None if ede.keyboard_focus_protected() => FilterResult::Forward,
                None => match ede.ipc.shortcuts.lookup(mods, handle.modified_sym()).map(|b| (b.owner.clone(), b.id.clone())) {
                    Some((owner, id)) => {
                        ede.ipc.shortcuts.active.push((code, owner, id));
                        FilterResult::Intercept(Action::Shortcut(code, true))
                    }
                    None => FilterResult::Forward,
                },
            }
        });
        if let Some(Action::Shortcut(code, pressed)) = action {
            self.shortcut_event(code, pressed, time);
            return;
        }
        if let Some(action) = action {
            self.run(action);
        }
        if let Some(hotkey) = self.hotkeys.fired.take() {
            self.hotkey(hotkey);
        }
    }

    pub fn run(&mut self, action: Action) {
        match action {
            Action::Quit => self.loop_signal.stop(),
            Action::SwitchVt(vt) => {
                if let Some(switch) = self.vt_switch.as_mut() {
                    switch(vt);
                }
            }
            Action::Lock => self.request_lock(),
            Action::Hotkey => {}
            Action::Workspace(i) => {
                self.switch_workspace(i);
            }
            Action::WorkspaceStep(d) => {
                let n = self.workspace_count() as i32;
                let next = (self.active_workspace() as i32 + d).rem_euclid(n.max(1));
                self.switch_workspace(next as usize);
            }
            Action::MoveToWorkspace(i) => {
                if let Some(w) = self.focused_window() {
                    self.move_to_workspace(&w, i, true);
                }
            }
            Action::MoveToWorkspaceStep(d) => {
                if let Some(w) = self.focused_window() {
                    let n = self.workspace_count() as i32;
                    let next = (self.active_workspace() as i32 + d).rem_euclid(n.max(1));
                    self.move_to_workspace(&w, next as usize, true);
                }
            }
            Action::RestoreShell => self.restore_shell(),
            Action::Shortcut(..) => {}
            Action::Close => self.close_focused(),
            Action::Tile(tile) => {
                if let Some(w) = self.focused_window() {
                    self.tile(&w, tile);
                }
            }
            Action::Restore => {
                if let Some(w) = self.focused_window() {
                    if window_state(&w).tile.is_some() {
                        self.untile(&w, None);
                    }
                }
            }
        }
    }

    fn shortcut_event(&mut self, code: u32, pressed: bool, time: u32) {
        let active = &mut self.ipc.shortcuts.active;
        let Some(i) = active.iter().rposition(|(k, ..)| *k == code) else { return };
        let (_, owner, id) = if pressed { active[i].clone() } else { active.remove(i) };
        let kind = if pressed { ede_protocol::events::SHORTCUT_ACTIVATED } else { ede_protocol::events::SHORTCUT_DEACTIVATED };
        self.emit(kind, serde_json::to_value(ede_protocol::ShortcutEvent { owner, id, time }).unwrap());
    }

    /// A press on a window frame: the buttons, a drag of the title (a
    /// double-click maximizes), a resize from the band around the window.
    /// `true` = handled.
    fn frame_press(&mut self, button: u32, serial: smithay::utils::Serial) -> bool {
        let pointer = self.seat.get_pointer().unwrap();
        let location = pointer.current_location();
        let Some((window, hit)) = self.frame_under(location) else { return false };
        self.focus(Some(window.clone()));
        if button != BTN_LEFT {
            return true;
        }
        match hit {
            crate::frames::Hit::Close => self.close_window(&window),
            crate::frames::Hit::Maximize => self.toggle_maximized(&window),
            crate::frames::Hit::Minimize => self.minimize_window(&window),
            crate::frames::Hit::Title => {
                let double = self.title_click.take().is_some_and(|(w, at)| w == window && at.elapsed() < std::time::Duration::from_millis(400));
                if double {
                    self.toggle_maximized(&window);
                } else {
                    self.title_click = Some((window.clone(), std::time::Instant::now()));
                    if let Some(loc) = self.space.element_location(&window) {
                        let start = GrabStartData { focus: None, button, location };
                        pointer.set_grab(self, MoveSurfaceGrab::new(start, window, loc), serial, Focus::Clear);
                    }
                }
            }
            crate::frames::Hit::Edge(edges) => {
                // The resize grab speaks xdg-shell; X11 windows resize from inside.
                if window.toplevel().is_some() && !edges.is_empty() {
                    if let Some(loc) = self.space.element_location(&window) {
                        let start = GrabStartData { focus: None, button, location };
                        let rect = smithay::utils::Rectangle::new(loc, window.geometry().size);
                        pointer.set_grab(self, crate::grabs::ResizeSurfaceGrab::start(start, window, edges, rect), serial, Focus::Clear);
                    }
                }
            }
        }
        true
    }

    /// Maximized ⇄ floating.
    pub fn toggle_maximized(&mut self, window: &smithay::desktop::Window) {
        if window_state(window).tile == Some(Tile::Maximized) {
            self.untile(window, None);
        } else {
            self.tile(window, Tile::Maximized);
        }
    }

    /// Is `surface` (or its root) a layer surface whose keyboard
    /// interactivity is not `none`?
    pub fn layer_takes_keyboard(&self, surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface) -> bool {
        self.layer_of(surface, |l| l.can_receive_keyboard_focus())
    }

    /// Is the surface (or the surface its subsurface hangs off) a layer surface?
    pub fn is_layer_surface(&self, surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface) -> bool {
        self.layer_of(surface, |_| true)
    }

    fn layer_of(&self, surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface, test: impl Fn(&smithay::desktop::LayerSurface) -> bool) -> bool {
        let mut root = surface.clone();
        while let Some(parent) = smithay::wayland::compositor::get_parent(&root) {
            root = parent;
        }
        self.space.outputs().any(|o| {
            let map = smithay::desktop::layer_map_for_output(o);
            map.layer_for_surface(&root, smithay::desktop::WindowSurfaceType::TOPLEVEL).is_some_and(&test)
        })
    }

    fn clamp_to_outputs(&self, pos: Point<f64, Logical>) -> Point<f64, Logical> {
        if self.space.output_under(pos).next().is_some() {
            return pos;
        }
        // Off every output: pull it to the nearest point of the nearest one.
        self.space
            .outputs()
            .filter_map(|o| self.space.output_geometry(o))
            .map(|g| {
                let x = pos.x.clamp(g.loc.x as f64, (g.loc.x + g.size.w) as f64 - 1.0);
                let y = pos.y.clamp(g.loc.y as f64, (g.loc.y + g.size.h) as f64 - 1.0);
                Point::from((x, y))
            })
            .min_by(|a, b| {
                let d = |p: &Point<f64, Logical>| (p.x - pos.x).powi(2) + (p.y - pos.y).powi(2);
                d(a).total_cmp(&d(b))
            })
            .unwrap_or(pos)
    }
}

/// `1`…`9` from the digit keysyms and their US-shifted symbols.
fn digit(sym: u32) -> Option<usize> {
    match sym {
        keysyms::KEY_1..=keysyms::KEY_9 => Some((sym - keysyms::KEY_1 + 1) as usize),
        keysyms::KEY_exclam => Some(1),
        keysyms::KEY_at => Some(2),
        keysyms::KEY_numbersign => Some(3),
        keysyms::KEY_dollar => Some(4),
        keysyms::KEY_percent => Some(5),
        keysyms::KEY_asciicircum => Some(6),
        keysyms::KEY_ampersand => Some(7),
        keysyms::KEY_asterisk => Some(8),
        keysyms::KEY_parenleft => Some(9),
        _ => None,
    }
}

pub(crate) fn binding(mods: &ModifiersState, sym: u32) -> Option<Action> {
    if (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12).contains(&sym) {
        return Some(Action::SwitchVt((sym - keysyms::KEY_XF86Switch_VT_1 + 1) as i32));
    }
    // Super+1…9 (Shift: move the window along); the keysym of the digit
    // row stays a digit under Shift on most layouts only through its
    // keycode, so both the digit and its shifted symbol count.
    if mods.logo && !mods.alt && !mods.ctrl {
        if let Some(n) = digit(sym) {
            return Some(if mods.shift { Action::MoveToWorkspace(n - 1) } else { Action::Workspace(n - 1) });
        }
    }
    match (mods.alt, mods.logo, mods.ctrl, mods.shift, sym) {
        (true, false, true, false, keysyms::KEY_Left) => Some(Action::WorkspaceStep(-1)),
        (true, false, true, false, keysyms::KEY_Right) => Some(Action::WorkspaceStep(1)),
        (true, false, true, true, keysyms::KEY_Left) => Some(Action::MoveToWorkspaceStep(-1)),
        (true, false, true, true, keysyms::KEY_Right) => Some(Action::MoveToWorkspaceStep(1)),
        (false, true, false, true, keysyms::KEY_E) => Some(Action::Quit),
        (false, true, false, true, keysyms::KEY_R) => Some(Action::RestoreShell),
        (true, false, false, false, keysyms::KEY_F4) => Some(Action::Close),
        (false, true, false, false, keysyms::KEY_l) => Some(Action::Lock),
        (false, true, false, false, keysyms::KEY_Left) => Some(Action::Tile(Tile::Left)),
        (false, true, false, false, keysyms::KEY_Right) => Some(Action::Tile(Tile::Right)),
        (false, true, false, false, keysyms::KEY_Up) => Some(Action::Tile(Tile::Maximized)),
        (false, true, false, false, keysyms::KEY_Down) => Some(Action::Restore),
        _ => None,
    }
}
