//! Input, backend-agnostic: every backend turns its events into these calls,
//! and the headless tests call them directly.
//!
//! Compositor bindings, Cinnamon's defaults (#EDE-243): Alt+drag moves a
//! window, Alt+right-drag resizes it, Alt+F4 closes it, Super+arrows walk
//! Muffin's tiles (halves, quarters by combination), Super+Shift+arrows move
//! it to the next output, Alt+F10 maximizes, Alt+F5 unmaximizes, Alt+F7 /
//! Alt+F8 move / resize it with the pointer, Super+D shows the desktop,
//! Super+Space switches the keyboard layout, Ctrl+Alt+L and Super+L lock,
//! Super+Shift+E ends the session, Ctrl+Alt+F1…F12 switch VTs. While locked only VT switching stays. The keys the shell
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
    desk::DeskId,
    grabs::{MoveSurfaceGrab, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT},
    layout::{Push, Tile},
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
    /// Ctrl+Alt+Left/Right (and ede-protocol's switch).
    Workspace(usize),
    WorkspaceStep(i32),
    /// Super+Shift+1…9, Ctrl+Alt+Shift+Left/Right: the focused window along.
    MoveToWorkspace(usize),
    MoveToWorkspaceStep(i32),
    /// Super+Shift+R: the stock shell documents back, the shell restarted (#EDE-16).
    RestoreShell,
    /// #EDE-243, Muffin's keys: Super+arrows (push-tile).
    PushTile(Push),
    /// Alt+F10.
    ToggleMaximized,
    /// Alt+F5.
    Unmaximize,
    /// Alt+F7 / Alt+F8: the window follows the pointer, a click ends it.
    BeginMove,
    BeginResize,
    /// Super+D.
    ShowDesktop,
    /// Super+Shift+arrows: the focused window to the next output that way.
    MoveToOutput(Push),
    /// Super+Space / Shift+Super+Space: the next / previous keyboard layout.
    NextLayout(bool),
    /// Alt+Space (#EDE-253): the focused window's menu.
    WindowMenu,
    /// A global shortcut's key (by keycode) went down / up (#EDE-15).
    Shortcut(u32, bool),
    /// #EDE-266: the person's key, for the agent space they clicked into.
    SpaceKey,
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
                let pos = self.desk.seat.get_pointer().unwrap().current_location() + event.delta();
                if !self.person_pointer_motion_to(pos, event.time().millis()) || self.space_view_routed() {
                    return;
                }
                let pointer = self.desk.seat.get_pointer().unwrap();
                let under = self.surface_under(pointer.current_location());
                pointer.relative_motion(
                    self,
                    under,
                    &RelativeMotionEvent { delta: event.delta(), delta_unaccel: event.delta_unaccel(), time: event.time() },
                );
                pointer.frame(self);
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let Some(output) = self.desk.space.outputs().next().cloned() else { return };
                let Some(geo) = self.desk.space.output_geometry(&output) else { return };
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
                // #EDE-266: the wheel over a taken-over space's picture scrolls it.
                if self.space_view_axis(frame) {
                    return;
                }
                self.refresh_pointer_focus(event.time().millis());
                let pointer = self.desk.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }

    /// Moves the pointer to `pos` (clamped onto the outputs) and updates focus.
    pub fn pointer_motion_to(&mut self, pos: Point<f64, Logical>, time: u32) {
        let pos = self.clamp_to_outputs(pos);
        let pointer = self.desk.seat.get_pointer().unwrap();
        let under = self.surface_under(pos);
        pointer.motion(self, under, &MotionEvent { location: pos, serial: SERIAL_COUNTER.next_serial(), time: smithay::backend::input::InputTime::from_millis(time) });
        pointer.frame(self);
    }

    /// #EDE-103: a raise, a map or a close changes what lies under a still
    /// pointer; a press or the wheel goes to what is there now, not to the
    /// surface the last motion found.
    pub fn refresh_pointer_focus(&mut self, time: u32) {
        let pointer = self.desk.seat.get_pointer().unwrap();
        // #EDE-266: over a taken-over space's view nothing of the person's is focused.
        if pointer.is_grabbed() || self.is_locked() || self.space_view_routed() {
            return;
        }
        let location = pointer.current_location();
        let under = self.surface_under(location);
        if under.as_ref().map(|(s, _)| s) == pointer.current_focus().as_ref() {
            return;
        }
        pointer.motion(self, under, &MotionEvent { location, serial: SERIAL_COUNTER.next_serial(), time: smithay::backend::input::InputTime::from_millis(time) });
        pointer.frame(self);
    }

    pub fn pointer_button(&mut self, button: u32, state: ButtonState, time: u32) {
        let pointer = self.desk.seat.get_pointer().unwrap();
        let serial = SERIAL_COUNTER.next_serial();

        if state == ButtonState::Pressed {
            self.refresh_pointer_focus(time);
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
            // So is one on a popup of such a layer, the start menu (#EDE-123).
            let on_layer = under.as_ref().is_some_and(|s| self.is_layer_surface(s)) || self.layer_above_windows(location);
            let window = under.as_ref().and_then(|s| self.window_for_surface(s)).or_else(|| if on_layer { None } else { self.desk.space.element_under(location).map(|(w, _)| w.clone()) });
            match (&window, under) {
                (Some(_), _) => self.focus(DeskId::PERSON, window.clone()),
                // A layer surface that takes the keyboard (the shell's menu,
                // a search) gets it on a click; the panel and a wallpaper
                // leave the app's focus alone (as on Cinnamon).
                (None, Some(surface)) => {
                    if self.layer_takes_keyboard(&surface) || self.popups.find_popup(&surface).is_some() {
                        self.focus_layer(surface, serial);
                    }
                }
                (None, None) => self.focus(DeskId::PERSON, None),
            }
            let alt = self.desk.seat.get_keyboard().unwrap().modifier_state().alt;
            match (window, alt, button) {
                (Some(window), true, BTN_LEFT) => {
                    // Alt+drag: move from anywhere in the window.
                    if let Some(loc) = self.desk.space.element_location(&window) {
                        let start = GrabStartData { focus: None, button, location };
                        pointer.set_grab(self, MoveSurfaceGrab::new(start, window, loc), serial, Focus::Clear);
                    }
                }
                // #EDE-243: Alt+right-drag resizes (Muffin's
                // resize-with-right-button, Mint's default), from the edge
                // or corner nearest the pointer.
                (Some(window), true, BTN_RIGHT) => {
                    if let Some(geo) = self.desk.space.element_geometry(&window) {
                        let start = GrabStartData { focus: None, button, location };
                        let edges = alt_resize_edges(geo, location);
                        pointer.set_grab(self, crate::grabs::ResizeSurfaceGrab::start(start, window, edges, geo), serial, Focus::Clear);
                    }
                }
                // #EDE-253: Alt+middle-click = the window menu (Muffin's
                // resize-with-right-button moves the menu to the middle
                // button), at the pointer; the window never sees the click.
                (Some(window), true, BTN_MIDDLE) if self.shell_listening() => {
                    self.window_menu(&window, (location.x.floor() as i32, location.y.floor() as i32).into());
                    self.desk.swallowed_button = Some(button);
                    return;
                }
                _ => {}
            }
        }
        if state == ButtonState::Released && self.desk.swallowed_button == Some(button) {
            self.desk.swallowed_button = None;
            return;
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
        let keyboard = self.desk.seat.get_keyboard().unwrap();
        let locked = self.is_locked();
        let listening = self.shell_listening();
        // #EDE-213: Escape also closes a shell popup that never took the
        // keyboard (the Agent spaces list), as a click outside it does.
        let popup_focused = keyboard.current_focus().is_some_and(|f| self.popups.find_popup(&f).is_some()) || self.shell_popups_open();
        let action = keyboard.input::<Action, _>(self, keycode, state, serial, smithay::backend::input::InputTime::from_millis(time), |ede, mods, handle| {
            let decided = (|| {
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
                    // #EDE-253: no shell to draw the window menu: the app gets Alt+Space.
                    Some(Action::WindowMenu) if !listening => FilterResult::Forward,
                    Some(action) => FilterResult::Intercept(action),
                    None if ede.keyboard_focus_protected(DeskId::PERSON) => FilterResult::Forward,
                    None => match ede.ipc.shortcuts.lookup(mods, handle.modified_sym()).map(|b| (b.owner.clone(), b.id.clone())) {
                        Some((owner, id)) => {
                            ede.ipc.shortcuts.active.push((code, owner, id));
                            FilterResult::Intercept(Action::Shortcut(code, true))
                        }
                        None => FilterResult::Forward,
                    },
                }
            })();
            // #EDE-266: what the person's bindings and hotkeys let pass goes
            // to the space they clicked into (never while locked).
            if bindings && !locked && matches!(decided, FilterResult::Forward) && ede.space_keyed_takes(handle.raw_code(), state) {
                return FilterResult::Intercept(Action::SpaceKey);
            }
            decided
        });
        // The lock always wins: the keys' way into a space ends with it.
        if locked {
            self.space_keyed_end();
        }
        if let Some(Action::SpaceKey) = action {
            self.space_keyed_key(keycode, state, time);
        }
        let action = action.filter(|a| *a != Action::SpaceKey);
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
                if let Some(w) = self.focused_window(DeskId::PERSON) {
                    self.move_to_workspace(&w, i, true);
                }
            }
            Action::MoveToWorkspaceStep(d) => {
                if let Some(w) = self.focused_window(DeskId::PERSON) {
                    let n = self.workspace_count() as i32;
                    let next = (self.active_workspace() as i32 + d).rem_euclid(n.max(1));
                    self.move_to_workspace(&w, next as usize, true);
                }
            }
            Action::RestoreShell => self.restore_shell(),
            Action::Shortcut(..) | Action::SpaceKey => {}
            Action::Close => self.close_focused(),
            Action::Tile(tile) => {
                if let Some(w) = self.focused_window(DeskId::PERSON) {
                    self.tile(&w, tile);
                }
            }
            Action::Restore => {
                if let Some(w) = self.focused_window(DeskId::PERSON) {
                    if window_state(&w).tile.is_some() {
                        self.untile(&w, None);
                    }
                }
            }
            Action::PushTile(push) => {
                if let Some(w) = self.focused_window(DeskId::PERSON) {
                    self.push_tile(&w, push);
                }
            }
            Action::ToggleMaximized => {
                if let Some(w) = self.focused_window(DeskId::PERSON) {
                    self.toggle_maximized(&w);
                }
            }
            Action::Unmaximize => {
                if let Some(w) = self.focused_window(DeskId::PERSON) {
                    if window_state(&w).tile.is_some() {
                        self.untile(&w, None);
                    }
                }
            }
            Action::BeginMove => self.begin_keyboard_grab(false),
            Action::BeginResize => self.begin_keyboard_grab(true),
            Action::ShowDesktop => self.toggle_desktop(),
            Action::MoveToOutput(push) => {
                if let Some(w) = self.focused_window(DeskId::PERSON) {
                    self.move_to_output(&w, push);
                }
            }
            Action::NextLayout(forward) => self.next_layout(forward),
            Action::WindowMenu => self.window_menu_focused(),
        }
    }

    /// Muffin's `do_tile_move`: the next tile of the walk
    /// ([`crate::layout::push_tile`]); none = restored; the same = nothing.
    pub fn push_tile(&mut self, window: &smithay::desktop::Window, push: Push) {
        let current = window_state(window).tile;
        let next = crate::layout::push_tile(current, push);
        if next == current {
            return;
        }
        match next {
            None => self.untile(window, None),
            Some(tile) => {
                self.tile(window, tile);
            }
        }
    }

    /// Alt+F7 / Alt+F8 (Muffin's begin-move / begin-resize): the pointer
    /// jumps to the window's centre (move) or its bottom-right corner
    /// (resize) and the window follows it until a click.
    fn begin_keyboard_grab(&mut self, resize: bool) {
        let Some(window) = self.focused_window(DeskId::PERSON) else { return };
        self.begin_keyboard_grab_on(&window, resize);
    }

    /// [`Self::begin_keyboard_grab`] on `window` (#EDE-253: the window
    /// menu's Move and Resize).
    pub(crate) fn begin_keyboard_grab_on(&mut self, window: &smithay::desktop::Window, resize: bool) {
        let window = window.clone();
        let Some(geo) = self.desk.space.element_geometry(&window) else { return };
        let pointer = self.desk.seat.get_pointer().unwrap();
        if pointer.is_grabbed() {
            return;
        }
        let at: Point<f64, Logical> = if resize { (geo.loc + geo.size.to_point()).to_f64() - Point::from((1.0, 1.0)) } else { geo.loc.to_f64() + geo.size.to_f64().to_point().downscale(2.0) };
        let serial = SERIAL_COUNTER.next_serial();
        let time = self.start_time.elapsed().as_millis() as u32;
        self.pointer_motion_to(at, time);
        let start = GrabStartData { focus: None, button: 0, location: at };
        if resize {
            let grab = crate::grabs::ResizeSurfaceGrab::start(start, window, crate::grabs::ResizeEdge::BOTTOM_RIGHT, geo);
            pointer.set_grab(self, grab, serial, Focus::Clear);
        } else {
            let loc = self.desk.space.element_location(&window).unwrap_or(geo.loc);
            pointer.set_grab(self, MoveSurfaceGrab::new(start, window, loc), serial, Focus::Clear);
        }
    }

    /// Muffin's move-to-monitor-*: the nearest output that way, the window at
    /// the same place relative to it (a tiled one keeps its tile there).
    pub fn move_to_output(&mut self, window: &smithay::desktop::Window, push: Push) {
        let Some(geo) = self.desk.space.element_geometry(window) else { return };
        let center = geo.loc.to_f64() + geo.size.to_f64().to_point().downscale(2.0);
        let Some(from) = self.output_at(center) else { return };
        let Some(from_geo) = self.desk.space.output_geometry(&from) else { return };
        let outputs: Vec<_> = self.desk.space.outputs().filter(|o| *o != &from).cloned().collect();
        let fc = from_geo.loc.to_f64() + from_geo.size.to_f64().to_point().downscale(2.0);
        let target = outputs
            .into_iter()
            .filter_map(|o| self.desk.space.output_geometry(&o).map(|g| (o, g)))
            .filter_map(|(o, g)| {
                let c = g.loc.to_f64() + g.size.to_f64().to_point().downscale(2.0);
                let (dx, dy) = (c.x - fc.x, c.y - fc.y);
                let ahead = match push {
                    Push::Left => -dx,
                    Push::Right => dx,
                    Push::Up => -dy,
                    Push::Down => dy,
                };
                (ahead > 0.0).then(|| (o, g, dx * dx + dy * dy))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2));
        let Some((to, to_geo, _)) = target else { return };
        if let Some(tile) = window_state(window).tile {
            self.tile_on(window, tile, &to);
            return;
        }
        let loc = self.desk.space.element_location(window).unwrap_or(geo.loc);
        let rel = loc - from_geo.loc;
        let x = to_geo.loc.x + rel.x.clamp(0, (to_geo.size.w - geo.size.w).max(0));
        let y = to_geo.loc.y + rel.y.clamp(0, (to_geo.size.h - geo.size.h).max(0));
        self.desk.space.map_element(window.clone(), (x, y), true);
    }

    /// Super+Space (Cinnamon's switch-input-source): the next layout of
    /// `[keyboard] layout` (`us,de`), wrapping; the shell hears
    /// `keyboard.layout` {index, layouts} for the keyboard applet.
    pub fn next_layout(&mut self, forward: bool) {
        let keyboard = self.desk.seat.get_keyboard().unwrap();
        keyboard.with_xkb_state(self, |mut ctx| {
            if forward {
                ctx.cycle_next_layout();
            } else {
                ctx.cycle_prev_layout();
            }
        });
        let now = self.keyboard_layouts();
        self.emit(ede_protocol::events::KEYBOARD_LAYOUT, serde_json::to_value(now).unwrap());
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
        let pointer = self.desk.seat.get_pointer().unwrap();
        let location = pointer.current_location();
        let Some((window, hit)) = self.frame_under(location) else { return false };
        self.focus(DeskId::PERSON, Some(window.clone()));
        // #EDE-253: action-right-click-titlebar = the window menu, at the pointer.
        if button == BTN_RIGHT && hit == crate::frames::Hit::Title {
            self.window_menu(&window, (location.x.floor() as i32, location.y.floor() as i32).into());
            return true;
        }
        if button != BTN_LEFT {
            return true;
        }
        match hit {
            crate::frames::Hit::Close => self.close_window(&window),
            crate::frames::Hit::Maximize => self.toggle_maximized(&window),
            crate::frames::Hit::Minimize => self.minimize_window(&window),
            crate::frames::Hit::Title => {
                let double = self.desk.title_click.take().is_some_and(|(w, at)| w == window && at.elapsed() < std::time::Duration::from_millis(400));
                if double {
                    self.toggle_maximized(&window);
                } else {
                    self.desk.title_click = Some((window.clone(), std::time::Instant::now()));
                    if let Some(loc) = self.desk.space.element_location(&window) {
                        let start = GrabStartData { focus: None, button, location };
                        pointer.set_grab(self, MoveSurfaceGrab::new(start, window, loc), serial, Focus::Clear);
                    }
                }
            }
            crate::frames::Hit::Edge(edges) => {
                if !edges.is_empty() {
                    if let Some(loc) = self.desk.space.element_location(&window) {
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
        self.desk.space.outputs().any(|o| {
            let map = smithay::desktop::layer_map_for_output(o);
            map.layer_for_surface(&root, smithay::desktop::WindowSurfaceType::TOPLEVEL).is_some_and(&test)
        })
    }

    pub(crate) fn clamp_to_outputs(&self, pos: Point<f64, Logical>) -> Point<f64, Logical> {
        if self.desk.space.output_under(pos).next().is_some() {
            return pos;
        }
        // Off every output: pull it to the nearest point of the nearest one.
        self.desk.space
            .outputs()
            .filter_map(|o| self.desk.space.output_geometry(o))
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
/// Muffin's Alt+right-drag: the window in thirds, the pointer's third
/// picks the edges (the middle one: bottom-right).
pub fn alt_resize_edges(geo: smithay::utils::Rectangle<i32, Logical>, at: Point<f64, Logical>) -> crate::grabs::ResizeEdge {
    use crate::grabs::ResizeEdge as E;
    let fx = (at.x - geo.loc.x as f64) / geo.size.w.max(1) as f64;
    let fy = (at.y - geo.loc.y as f64) / geo.size.h.max(1) as f64;
    let h = if fx < 1.0 / 3.0 { E::LEFT } else if fx > 2.0 / 3.0 { E::RIGHT } else { E::empty() };
    let v = if fy < 1.0 / 3.0 { E::TOP } else if fy > 2.0 / 3.0 { E::BOTTOM } else { E::empty() };
    match h | v {
        e if e.is_empty() => E::BOTTOM_RIGHT,
        e => e,
    }
}

pub(crate) fn binding(mods: &ModifiersState, sym: u32) -> Option<Action> {
    if (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12).contains(&sym) {
        return Some(Action::SwitchVt((sym - keysyms::KEY_XF86Switch_VT_1 + 1) as i32));
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
        // #EDE-243: Cinnamon's defaults (org.cinnamon.desktop.keybindings.wm).
        (false, true, false, false, keysyms::KEY_Left) => Some(Action::PushTile(Push::Left)),
        (false, true, false, false, keysyms::KEY_Right) => Some(Action::PushTile(Push::Right)),
        (false, true, false, false, keysyms::KEY_Up) => Some(Action::PushTile(Push::Up)),
        (false, true, false, false, keysyms::KEY_Down) => Some(Action::PushTile(Push::Down)),
        (false, true, false, true, keysyms::KEY_Left) => Some(Action::MoveToOutput(Push::Left)),
        (false, true, false, true, keysyms::KEY_Right) => Some(Action::MoveToOutput(Push::Right)),
        (false, true, false, true, keysyms::KEY_Up) => Some(Action::MoveToOutput(Push::Up)),
        (false, true, false, true, keysyms::KEY_Down) => Some(Action::MoveToOutput(Push::Down)),
        (true, false, false, false, keysyms::KEY_F10) => Some(Action::ToggleMaximized),
        (true, false, false, false, keysyms::KEY_F5) => Some(Action::Unmaximize),
        (true, false, false, false, keysyms::KEY_F7) => Some(Action::BeginMove),
        (true, false, false, false, keysyms::KEY_F8) => Some(Action::BeginResize),
        (false, true, false, false, keysyms::KEY_d | keysyms::KEY_D) => Some(Action::ShowDesktop),
        (true, false, true, false, keysyms::KEY_l | keysyms::KEY_L) => Some(Action::Lock),
        (false, true, false, shift, keysyms::KEY_space) => Some(Action::NextLayout(!shift)),
        (true, false, false, false, keysyms::KEY_space) => Some(Action::WindowMenu),
        _ => None,
    }
}
