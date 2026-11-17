//! `InputEvent::Keysym` (#EDE-22): a key named by what it types. ePilot
//! speaks X keysyms (`Return`, `a`, `A`, `€`); the compositor owns the
//! layout, so it finds the key that makes the keysym and the Shift or
//! AltGr its level needs, holds those around the key on press and lets go
//! of them with its release. A keysym only another of the configured
//! layouts makes (`v` while the Russian one is active) is pressed in that
//! layout, which is switched to for the length of the stroke (#EXP-1907).
//! No key for it on any layout = `Ok(false)`, which the socket answers as
//! `not_found`.

use smithay::{
    backend::input::KeyState,
    input::keyboard::{xkb, Keycode, Layout},
};

use crate::{protected::Refused, state::Ede};

#[derive(Debug, Default)]
pub struct Keysyms {
    /// Per key a keysym press holds: the modifier keys it added, released
    /// with it, and the layout it switched away from, restored with it.
    mods: Vec<(Keycode, Vec<Keycode>, Option<Layout>)>,
}

impl Keysyms {
    /// The keys of a connection that went: their modifiers went with them.
    /// The layout the first of them switched away from comes back, for the
    /// caller to restore.
    pub(crate) fn forget(&mut self, keys: &[(u64, Keycode)]) -> Option<Layout> {
        let mut restore = None;
        self.mods.retain(|(key, _, layout)| {
            let gone = keys.iter().any(|(_, k)| k == key);
            if gone {
                restore = restore.or(*layout);
            }
            !gone
        });
        restore
    }
}

impl Ede {
    /// The key making `keysym` and the modifier keys its level needs
    /// (Shift, AltGr), found by trying each on a trial xkb state that has
    /// the seat's locked and latched modifiers (Caps Lock); the plainest
    /// wins. The active layout first, then the others: `Some(layout)` = the
    /// key is that layout's, not the active one's.
    pub fn key_for_keysym(&mut self, keysym: u32) -> Option<(Keycode, Vec<Keycode>, Option<Layout>)> {
        let keyboard = self.desk.seat.get_keyboard().unwrap();
        keyboard.with_xkb_state(self, |ctx| {
            let xkb = ctx.xkb().lock().unwrap();
            let active = xkb.active_layout().0;
            // SAFETY: the keymap and the state are only borrowed inside this callback.
            let (keymap, seat) = unsafe { (xkb.keymap(), xkb.state()) };
            // Under a held Ctrl, Alt or Super the key is a shortcut's: it is
            // pressed as the person would, Caps Lock or not (Shift would
            // turn ctrl+z into ctrl+shift+z).
            let chord = [xkb::MOD_NAME_CTRL, xkb::MOD_NAME_ALT, xkb::MOD_NAME_LOGO].iter().any(|name| seat.mod_name_is_active(name, xkb::STATE_MODS_DEPRESSED));
            let (latched, locked) = if chord { (0, 0) } else { (seat.serialize_mods(xkb::STATE_MODS_LATCHED), seat.serialize_mods(xkb::STATE_MODS_LOCKED)) };
            let (min, max) = (keymap.min_keycode().raw(), keymap.max_keycode().raw());
            for layout in std::iter::once(active).chain((0..keymap.num_layouts()).filter(|layout| *layout != active)) {
                let level0 = |sym: u32| (min..=max).map(Keycode::new).find(|kc| keymap.key_get_syms_by_level(*kc, layout, 0).iter().any(|s| s.raw() == sym));
                let shift = level0(xkb::keysyms::KEY_Shift_L);
                let altgr = level0(xkb::keysyms::KEY_ISO_Level3_Shift);
                let combos: Vec<Vec<Keycode>> = [vec![], vec![shift], vec![altgr], vec![shift, altgr]]
                    .into_iter()
                    .filter_map(|mods| mods.into_iter().collect::<Option<Vec<_>>>())
                    .collect();
                for kc in (min..=max).map(Keycode::new) {
                    let levels = keymap.num_levels_for_key(kc, layout);
                    if !(0..levels).any(|l| keymap.key_get_syms_by_level(kc, layout, l).iter().any(|s| s.raw() == keysym)) {
                        continue;
                    }
                    for mods in &combos {
                        let mut state = xkb::State::new(keymap);
                        state.update_mask(0, latched, locked, 0, 0, layout);
                        for m in mods {
                            state.update_key(*m, xkb::KeyDirection::Down);
                        }
                        if state.key_get_one_sym(kc).raw() == keysym {
                            return Some((kc, mods.clone(), (layout != active).then_some(Layout(layout))));
                        }
                    }
                }
            }
            None
        })
    }

    /// Make `layout` the active one; the one it replaces comes back.
    pub fn swap_layout(&mut self, layout: Layout) -> Layout {
        let keyboard = self.desk.seat.get_keyboard().unwrap();
        keyboard.with_xkb_state(self, |mut ctx| {
            let was = ctx.xkb().lock().unwrap().active_layout();
            ctx.set_layout(layout);
            was
        })
    }

    /// `InputEvent::Keysym`: `Ok(false)` = no key makes it. `bindings` =
    /// the person's path (the root grant), otherwise an agent's, refused on
    /// protected surfaces and the lock like any driven key.
    pub fn drive_keysym(&mut self, keysym: u32, pressed: bool, bindings: bool, time: u32) -> Result<bool, Refused> {
        let Some((key, mods, layout)) = self.key_for_keysym(keysym) else { return Ok(false) };
        let send = |ede: &mut Ede, keycode: Keycode, state: KeyState| -> Result<(), Refused> {
            if bindings {
                ede.keyboard_key(keycode, state, time);
                Ok(())
            } else {
                ede.drive_key(keycode, state, time, false)
            }
        };
        if pressed {
            let down = self.desk.seat.get_keyboard().unwrap().pressed_keys();
            let added: Vec<Keycode> = mods.into_iter().filter(|m| !down.contains(m)).collect();
            // Another layout's key: that layout for the length of the stroke.
            let restore = layout.map(|layout| self.swap_layout(layout));
            let sent = added.iter().chain([&key]).try_for_each(|k| send(self, *k, KeyState::Pressed));
            if let Err(refused) = sent {
                if let Some(layout) = restore {
                    self.swap_layout(layout);
                }
                return Err(refused);
            }
            self.desk.keysyms.mods.retain(|(k, _, _)| *k != key);
            self.desk.keysyms.mods.push((key, added, restore));
        } else {
            send(self, key, KeyState::Released)?;
            let at = self.desk.keysyms.mods.iter().position(|(k, _, _)| *k == key);
            let (_, added, restore) = at.map(|i| self.desk.keysyms.mods.remove(i)).unwrap_or((key, Vec::new(), None));
            if let Some(layout) = restore {
                self.swap_layout(layout);
            }
            for m in added.iter().rev() {
                send(self, *m, KeyState::Released)?;
            }
        }
        Ok(true)
    }
}
