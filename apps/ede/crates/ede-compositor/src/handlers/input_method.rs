//! Input methods (ibus, fcitx5): input-method-v2 for the IME, text-input-v3
//! for apps, and the virtual keyboard the IME forwards keys through.
//! Protected surfaces and the lock screen stay out of reach of all three
//! (see [`crate::protected`]).

use smithay::{
    backend::input::{Event, InputEvent, KeyboardKeyEvent},
    desktop::{PopupKind, PopupManager},
    input::keyboard::XkbConfig,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Rectangle, SERIAL_COUNTER},
    wayland::{
        input_method::{InputMethodHandler, InputMethodSeat, PopupSurface},
        text_input::{TextInputActivation, TextInputSeat},
        virtual_keyboard::{VirtualKeyboardBackend, VirtualKeyboardDevice, VirtualKeyboardHandler, VirtualKeyboardSpecialEvent},
    },
};

use crate::state::Ede;

impl InputMethodHandler for Ede {
    fn new_popup(&mut self, surface: PopupSurface) {
        if let Err(e) = self.popups.track_popup(PopupKind::from(surface)) {
            tracing::warn!("input method popup: {e}");
        }
    }

    fn popup_repositioned(&mut self, _surface: PopupSurface) {}

    fn dismiss_popup(&mut self, surface: PopupSurface) {
        if let Some(parent) = surface.get_parent().map(|p| p.surface.clone()) {
            let _ = PopupManager::dismiss_popup(&parent, &PopupKind::from(surface));
        }
    }

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        self.window_for_surface(parent).and_then(|w| self.space.element_geometry(&w)).unwrap_or_default()
    }
}

// text-input-v3 through smithay, except that a protected surface (or the
// lock screen) never turns the input method on: the IME neither sees its
// text nor commits into it. smithay drops text-input requests from a client
// whose surface is not the text-input focus, so a protected keyboard focus
// takes that focus away (see `focus_changed`).
impl TextInputActivation for Ede {}

impl Ede {
    pub(crate) fn keep_text_input_off_protected(&mut self, focused: Option<&WlSurface>) {
        if !(self.is_locked() || focused.is_some_and(|s| self.is_protected_surface(s))) {
            return;
        }
        let text_input = self.seat.text_input();
        if self.seat.input_method().has_instance() || text_input.compositor_input_method() {
            text_input.leave();
        }
        text_input.set_focus(None);
    }

    /// A real key goes under the configured keymap again after a virtual
    /// keyboard swapped in its own.
    pub(crate) fn restore_seat_keymap(&mut self) {
        if self.virtual_keymap.take().is_none() {
            return;
        }
        let kb = self.config.keyboard.clone();
        let xkb = XkbConfig { layout: &kb.layout, variant: &kb.variant, options: Some(kb.options.clone()).filter(|o| !o.is_empty()), ..Default::default() };
        if let Err(e) = self.seat.get_keyboard().unwrap().set_xkb_config(self, xkb) {
            tracing::warn!("keyboard layout {:?} not restored: {e:?}", kb.layout);
        }
    }

    /// The device's keycodes only mean anything under its own keymap (none =
    /// it uses the seat's).
    fn use_virtual_keymap(&mut self, device: &VirtualKeyboardDevice) {
        match device.keymap() {
            Some(keymap) if self.virtual_keymap != Some(device.source()) => {
                let keyboard = self.seat.get_keyboard().unwrap();
                match keyboard.set_keymap_from_keymap_file(self, keymap) {
                    Ok(()) => self.virtual_keymap = Some(device.source()),
                    Err(e) => tracing::warn!("virtual keyboard keymap: {e:?}"),
                }
            }
            Some(_) => {}
            None => self.restore_seat_keymap(),
        }
    }
}

// The virtual keyboard types straight into whatever has focus (no compositor
// bindings, no input-method grab, not person activity), so its keys stop at
// protected surfaces and the lock screen.
impl VirtualKeyboardHandler for Ede {
    fn process_virtual_keyboard_event(&mut self, event: InputEvent<VirtualKeyboardBackend>) {
        let keyboard = self.seat.get_keyboard().unwrap();
        match event {
            InputEvent::Keyboard { event } => {
                if self.keyboard_focus_protected() {
                    return;
                }
                let device = event.device();
                self.use_virtual_keymap(&device);
                let (code, state) = (event.key_code(), event.state());
                let Some(((), mods_changed)) = keyboard.input_intercept(device.source(), self, code, state, |_, _, _| ()) else { return };
                keyboard.input_forward_bypassing_grab(self, code, state, SERIAL_COUNTER.next_serial(), event.time(), mods_changed);
            }
            InputEvent::Special(VirtualKeyboardSpecialEvent::Modifiers { device, mods_depressed, mods_latched, mods_locked, group }) => {
                if self.keyboard_focus_protected() {
                    return;
                }
                self.use_virtual_keymap(&device);
                keyboard.with_xkb_state(self, |mut xkb| xkb.set_modifier_mask(mods_depressed, mods_latched, mods_locked, group));
            }
            InputEvent::Special(VirtualKeyboardSpecialEvent::KeymapChanged { device }) => {
                if self.virtual_keymap == Some(device.source()) {
                    self.virtual_keymap = None;
                }
            }
            InputEvent::DeviceRemoved { device } => {
                keyboard.release_source(self, device.source());
                if self.virtual_keymap == Some(device.source()) {
                    self.restore_seat_keymap();
                }
            }
            _ => {}
        }
    }
}
