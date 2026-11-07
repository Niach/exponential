//! `InputEvent::Keysym` (#EDE-22): a key named by what it types. ePilot
//! speaks X keysyms (`Return`, `a`, `A`, `€`); the compositor owns the
//! layout, so it finds the key that makes the keysym and the Shift or
//! AltGr its level needs, holds those around the key on press and lets go
//! of them with its release. No key for it on this layout = `Ok(false)`,
//! which the socket answers as `not_found`.

use smithay::{
    backend::input::KeyState,
    input::keyboard::{xkb, Keycode},
};

use crate::{protected::Refused, state::Ede};

#[derive(Debug, Default)]
pub struct Keysyms {
    /// Modifier keys a keysym press added, per key, released with it.
    mods: Vec<(Keycode, Vec<Keycode>)>,
}

impl Ede {
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

    /// `InputEvent::Keysym`: `Ok(false)` = no key makes it. `bindings` =
    /// the person's path (the root grant), otherwise an agent's, refused on
    /// protected surfaces and the lock like any driven key.
    pub fn drive_keysym(&mut self, keysym: u32, pressed: bool, bindings: bool, time: u32) -> Result<bool, Refused> {
        let Some((key, mods)) = self.key_for_keysym(keysym) else { return Ok(false) };
        let send = |ede: &mut Ede, keycode: Keycode, state: KeyState| -> Result<(), Refused> {
            if bindings {
                ede.keyboard_key(keycode, state, time);
                Ok(())
            } else {
                ede.drive_key(keycode, state, time, false)
            }
        };
        if pressed {
            let down = self.seat.get_keyboard().unwrap().pressed_keys();
            let added: Vec<Keycode> = mods.into_iter().filter(|m| !down.contains(m)).collect();
            for m in &added {
                send(self, *m, KeyState::Pressed)?;
            }
            send(self, key, KeyState::Pressed)?;
            self.ipc.keysyms.mods.retain(|(k, _)| *k != key);
            self.ipc.keysyms.mods.push((key, added));
        } else {
            send(self, key, KeyState::Released)?;
            let at = self.ipc.keysyms.mods.iter().position(|(k, _)| *k == key);
            let added = at.map(|i| self.ipc.keysyms.mods.remove(i).1).unwrap_or_default();
            for m in added.iter().rev() {
                send(self, *m, KeyState::Released)?;
            }
        }
        Ok(true)
    }
}
