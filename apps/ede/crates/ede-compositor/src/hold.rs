//! #EDE-23: the person's input held out while an agent drives the seat
//! (`ede-protocol` `hold_input`, ePilot's foreground lock). While any
//! connection holds it, the person's keys, pointer and wheel reach no
//! window; agent input (`drive_key` and friends) still lands. Their Esc
//! ends every hold and tells the holders ([`events::INPUT_ESCAPE`]). A
//! holder that disconnects lets go, so a crashed agent never leaves the
//! person locked out. The session lock screen always gets their input.
//!
//! A hold starts on a clean seat: what the person has down is released
//! first. Releases of keys and buttons they pressed under the lock still
//! go through, so nothing stays stuck down. It ends on a clean seat too
//! (#EDE-80): the last hold's end releases what every agent has down.

use ede_protocol::{events, Envelope, Message, EVENT_SOURCE};
use smithay::{
    backend::input::{ButtonState, KeyState},
    input::keyboard::Keycode,
    utils::{Logical, Point},
};

use crate::state::Ede;

/// xkb's keycode for Esc (evdev `KEY_ESC` 1 + 8).
pub const ESC: u32 = 9;

#[derive(Debug, Default)]
pub struct Hold {
    /// Connection ids holding the person's input.
    pub holders: Vec<u64>,
    /// Buttons the person has down (their releases always pass).
    buttons: Vec<u32>,
}

impl Ede {
    /// The person's input reaches no window right now.
    pub fn person_input_held(&self) -> bool {
        !self.desk.hold.holders.is_empty() && !self.is_locked()
    }

    pub(crate) fn hold_input(&mut self, conn: u64) {
        if self.desk.hold.holders.contains(&conn) {
            return;
        }
        self.desk.hold.holders.push(conn);
        if self.desk.hold.holders.len() == 1 && !self.is_locked() {
            self.release_person_input();
        }
    }

    /// A hold starts on a clean seat (#EDE-73): the keys and buttons the
    /// person has down are released (a drag ends), so they never merge with
    /// the agent's input. The release is not their act (#EDE-80): no
    /// binding or hotkey runs, an open switcher is cancelled; only a global
    /// shortcut they hold is told its key went up.
    fn release_person_input(&mut self) {
        let time = crate::ipc::now_ms() as u32;
        let mut keys: Vec<Keycode> = self.desk.seat.get_keyboard().unwrap().pressed_keys().into_iter().filter(|k| !self.desk.driven.has_key(*k)).collect();
        keys.sort_by_key(|k| k.raw());
        for key in keys {
            let shortcut = self.ipc.shortcuts.active.iter().any(|(k, ..)| *k == key.raw());
            self.key(key, KeyState::Released, time, shortcut);
        }
        self.hotkeys_let_go();
        for button in std::mem::take(&mut self.desk.hold.buttons) {
            self.pointer_button(button, ButtonState::Released, time);
        }
    }

    pub(crate) fn release_input(&mut self, conn: u64) {
        let held = !self.desk.hold.holders.is_empty();
        self.desk.hold.holders.retain(|c| *c != conn);
        if held && self.desk.hold.holders.is_empty() {
            self.release_driven_of(None);
        }
    }

    /// The person's Esc during a hold: every hold ends, every holder hears it.
    fn person_escape(&mut self) {
        let holders = std::mem::take(&mut self.desk.hold.holders);
        tracing::info!("Esc: the person took their input back from {} holder(s)", holders.len());
        self.release_driven_of(None);
        let envelope = Envelope::new(&format!("ev-{}", self.ipc.next_event_id()), events::INPUT_ESCAPE, crate::ipc::now_ms(), EVENT_SOURCE, None, serde_json::Value::Null);
        let message = Message::Event { event: envelope };
        for conn in holders {
            self.ipc_send(conn, &message, None);
        }
    }

    /// `true` = the person's key goes on to the seat.
    pub(crate) fn person_key_passes(&mut self, keycode: Keycode, state: KeyState) -> bool {
        if !self.person_input_held() {
            return true;
        }
        if state == KeyState::Pressed {
            if keycode.raw() == ESC {
                self.person_escape();
            }
            return false;
        }
        // Their release of a key the agent has down is not the agent's.
        !self.desk.driven.has_key(keycode) && self.desk.seat.get_keyboard().unwrap().pressed_keys().contains(&keycode)
    }

    /// A pointer button from the person's mouse.
    pub fn person_pointer_button(&mut self, button: u32, state: ButtonState, time: u32) {
        let was_down = self.desk.hold.buttons.contains(&button);
        match state {
            ButtonState::Pressed if !self.person_input_held() => self.desk.hold.buttons.push(button),
            ButtonState::Pressed => return,
            ButtonState::Released if !was_down && self.person_input_held() => return,
            ButtonState::Released => self.desk.hold.buttons.retain(|b| *b != button),
        }
        self.pointer_button(button, state, time);
    }

    /// The person's pointer moved to `pos`; `false` = held, nothing moved.
    pub fn person_pointer_motion_to(&mut self, pos: Point<f64, Logical>, time: u32) -> bool {
        if self.person_input_held() {
            return false;
        }
        self.pointer_motion_to(pos, time);
        true
    }
}
