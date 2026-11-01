//! #NEW-112: the person's input held out while an agent drives the seat
//! (`ede-protocol` `hold_input`, ePilot's foreground lock). While any
//! connection holds it, the person's keys, pointer and wheel reach no
//! window; agent input ([`Ede::drive_key`] and friends) still lands. Their
//! Esc ends every hold and tells the holders ([`events::INPUT_ESCAPE`]). A
//! holder that disconnects lets go, so a crashed agent never leaves the
//! person locked out. The session lock screen always gets their input.
//!
//! Releases of keys and buttons the person pressed before the hold still
//! go through, so nothing stays stuck down.

use ede_protocol::{events, Envelope, Message, EVENT_SOURCE};
use smithay::{
    backend::input::{ButtonState, KeyState},
    input::keyboard::{xkb, Keycode},
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
    /// Modifier keys a `keysym` press added, per key, released with it.
    keysym_mods: Vec<(Keycode, Vec<Keycode>)>,
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

    /// The key making `keysym` on the active layout and the modifier keys
    /// its level needs (Shift, AltGr), found by trying each on a fresh xkb
    /// state; the plainest wins.
    pub fn key_for_keysym(&mut self, keysym: u32) -> Option<(Keycode, Vec<Keycode>)> {
        let keyboard = self.seat.get_keyboard().unwrap();
        keyboard.with_xkb_state(self, |ctx| {
            let xkb = ctx.xkb().lock().unwrap();
            let layout = xkb.active_layout();
            // SAFETY: the keymap is only borrowed inside this callback.
            let keymap = unsafe { xkb.keymap() };
            let (min, max) = (keymap.min_keycode().raw(), keymap.max_keycode().raw());
            let level0 = |sym: u32| (min..=max).map(Keycode::new).find(|kc| keymap.key_get_syms_by_level(*kc, layout.0, 0).iter().any(|s| s.raw() == sym));
            let shift = level0(xkb::keysyms::KEY_Shift_L);
            let altgr = level0(xkb::keysyms::KEY_ISO_Level3_Shift);
            let combos: Vec<Vec<Keycode>> = [vec![], vec![shift], vec![altgr], vec![shift, altgr]]
                .into_iter()
                .filter_map(|mods| mods.into_iter().collect::<Option<Vec<_>>>())
                .collect();
            for kc in (min..=max).map(Keycode::new) {
                let levels = keymap.num_levels_for_key(kc, layout.0);
                if !(0..levels).any(|l| keymap.key_get_syms_by_level(kc, layout.0, l).iter().any(|s| s.raw() == keysym)) {
                    continue;
                }
                for mods in &combos {
                    let mut state = xkb::State::new(keymap);
                    state.update_mask(0, 0, 0, 0, 0, layout.0);
                    for m in mods {
                        state.update_key(*m, xkb::KeyDirection::Down);
                    }
                    if state.key_get_one_sym(kc).raw() == keysym {
                        return Some((kc, mods.clone()));
                    }
                }
            }
            None
        })
    }

    /// An agent's keysym (`InputEvent::Keysym`): `Ok(false)` = no key makes it.
    pub fn drive_keysym(&mut self, keysym: u32, pressed: bool, time: u32) -> Result<bool, crate::protected::Refused> {
        let Some((key, mods)) = self.key_for_keysym(keysym) else { return Ok(false) };
        if pressed {
            let down = self.seat.get_keyboard().unwrap().pressed_keys();
            let added: Vec<Keycode> = mods.into_iter().filter(|m| !down.contains(m)).collect();
            for m in &added {
                self.drive_key(*m, KeyState::Pressed, time)?;
            }
            self.drive_key(key, KeyState::Pressed, time)?;
            self.ipc.hold.keysym_mods.retain(|(k, _)| *k != key);
            self.ipc.hold.keysym_mods.push((key, added));
        } else {
            self.drive_key(key, KeyState::Released, time)?;
            let at = self.ipc.hold.keysym_mods.iter().position(|(k, _)| *k == key);
            for m in at.map(|i| self.ipc.hold.keysym_mods.remove(i).1).unwrap_or_default().iter().rev() {
                self.drive_key(*m, KeyState::Released, time)?;
            }
        }
        Ok(true)
    }
}
