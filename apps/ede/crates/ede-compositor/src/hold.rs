//! #EDE-23: the person's input held out while an agent drives the seat
//! (`ede-protocol` `hold_input`, ePilot's foreground lock). While any
//! connection holds it, the person's keys, pointer and wheel reach no
//! window; agent input (`drive_key` and friends) still lands. Their Esc
//! ends every hold and tells the holders ([`events::INPUT_ESCAPE`]). A
//! holder that disconnects lets go, so a crashed agent never leaves the
//! person locked out. The session lock screen always gets their input.
//!
//! Releases of keys and buttons the person pressed before the hold still
//! go through, so nothing stays stuck down.

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
        !self.ipc.hold.holders.is_empty() && !self.is_locked()
    }

    pub(crate) fn hold_input(&mut self, conn: u64) {
        if !self.ipc.hold.holders.contains(&conn) {
            self.ipc.hold.holders.push(conn);
        }
    }

    pub(crate) fn release_input(&mut self, conn: u64) {
        self.ipc.hold.holders.retain(|c| *c != conn);
    }

    /// The person's Esc during a hold: every hold ends, every holder hears it.
    fn person_escape(&mut self) {
        let holders = std::mem::take(&mut self.ipc.hold.holders);
        tracing::info!("Esc: the person took their input back from {} holder(s)", holders.len());
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
        self.seat.get_keyboard().unwrap().pressed_keys().contains(&keycode)
    }

    /// A pointer button from the person's mouse.
    pub fn person_pointer_button(&mut self, button: u32, state: ButtonState, time: u32) {
        let was_down = self.ipc.hold.buttons.contains(&button);
        match state {
            ButtonState::Pressed if !self.person_input_held() => self.ipc.hold.buttons.push(button),
            ButtonState::Pressed => return,
            ButtonState::Released if !was_down && self.person_input_held() => return,
            ButtonState::Released => self.ipc.hold.buttons.retain(|b| *b != button),
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
