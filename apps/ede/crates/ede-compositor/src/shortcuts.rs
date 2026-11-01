//! Global shortcuts (#NEW-105): what the GlobalShortcuts portal binds over
//! `ede-protocol`. Triggers follow the XDG shortcuts spec
//! (`CTRL+SHIFT+a`). A shortcut fires only from the person's keyboard,
//! never while the session is locked or a protected surface has the
//! keyboard, and the key press it takes never reaches the focused client.

use ede_protocol::{BoundShortcut, ShortcutSpec};
use smithay::input::keyboard::{
    xkb::{self, Keysym},
    ModifiersState,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trigger {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
    /// Lowercase keysym (`a`, not `A`).
    pub sym: u32,
}

impl Trigger {
    /// Parses `CTRL+SHIFT+a`; modifier names are case-insensitive.
    pub fn parse(trigger: &str) -> Option<Trigger> {
        let mut parts: Vec<&str> = trigger.split('+').map(str::trim).collect();
        let key = parts.pop().filter(|k| !k.is_empty())?;
        let mut t = Trigger { ctrl: false, alt: false, shift: false, logo: false, sym: 0 };
        for m in parts {
            match m.to_ascii_uppercase().as_str() {
                "CTRL" | "CONTROL" => t.ctrl = true,
                "ALT" => t.alt = true,
                "SHIFT" => t.shift = true,
                "LOGO" | "SUPER" => t.logo = true,
                // NUM (Num Lock) is accepted and ignored.
                "NUM" => {}
                _ => return None,
            }
        }
        let sym = xkb::keysym_from_name(key, xkb::KEYSYM_CASE_INSENSITIVE);
        if sym.raw() == xkb::keysyms::KEY_NoSymbol {
            return None;
        }
        t.sym = lower(sym);
        Some(t)
    }

    /// The normalized form: `CTRL+ALT+SHIFT+LOGO+key`.
    pub fn describe(&self) -> String {
        let mut s = String::new();
        for (on, name) in [(self.ctrl, "CTRL+"), (self.alt, "ALT+"), (self.shift, "SHIFT+"), (self.logo, "LOGO+")] {
            if on {
                s.push_str(name);
            }
        }
        s + &xkb::keysym_get_name(Keysym::new(self.sym))
    }

    /// A plain key (no CTRL, ALT or LOGO) is only a trigger when it types
    /// nothing: F-keys, media keys, Print, Pause. Otherwise a shortcut
    /// would be a keylogger.
    pub fn allowed(&self) -> bool {
        if self.ctrl || self.alt || self.logo {
            return true;
        }
        let name = xkb::keysym_get_name(Keysym::new(self.sym));
        name.starts_with("XF86") || matches!(name.as_str(), "Print" | "Pause" | "Scroll_Lock") || (name.starts_with('F') && name[1..].parse::<u32>().is_ok())
    }

    pub fn mods(&self) -> ModifiersState {
        ModifiersState { ctrl: self.ctrl, alt: self.alt, shift: self.shift, logo: self.logo, ..Default::default() }
    }

    /// The keysym in both cases (`e`, `E`): what the key yields with and
    /// without Shift.
    pub fn syms(&self) -> [u32; 2] {
        let name = xkb::keysym_get_name(Keysym::new(self.sym));
        let upper = xkb::keysym_from_name(&name.to_uppercase(), xkb::KEYSYM_NO_FLAGS).raw();
        [self.sym, if upper == xkb::keysyms::KEY_NoSymbol { self.sym } else { upper }]
    }

    pub fn matches(&self, mods: &ModifiersState, sym: Keysym) -> bool {
        (mods.ctrl, mods.alt, mods.shift, mods.logo) == (self.ctrl, self.alt, self.shift, self.logo) && lower(sym) == self.sym
    }
}

fn lower(sym: Keysym) -> u32 {
    let name = xkb::keysym_get_name(sym);
    if name.chars().count() == 1 {
        let lowered = xkb::keysym_from_name(&name.to_lowercase(), xkb::KEYSYM_NO_FLAGS);
        if lowered.raw() != xkb::keysyms::KEY_NoSymbol {
            return lowered.raw();
        }
    }
    sym.raw()
}

#[derive(Clone, Debug)]
pub struct Binding {
    /// The ede-protocol connection that bound it.
    pub client: u64,
    pub owner: String,
    pub id: String,
    pub trigger: Trigger,
}

#[derive(Debug, Default)]
pub struct Shortcuts {
    pub bindings: Vec<Binding>,
    /// Held down: (keycode, owner, id).
    pub active: Vec<(u32, String, String)>,
}

impl Shortcuts {
    /// Replaces `owner`'s shortcuts. `taken` says whether a trigger is a
    /// compositor binding.
    pub fn bind(&mut self, client: u64, owner: &str, specs: Vec<ShortcutSpec>, taken: impl Fn(&Trigger) -> bool) -> Vec<BoundShortcut> {
        self.bindings.retain(|b| b.owner != owner);
        let mut out = Vec::new();
        for spec in specs {
            let trigger = Trigger::parse(&spec.trigger)
                .filter(Trigger::allowed)
                .filter(|t| !taken(t))
                .filter(|t| !self.bindings.iter().any(|b| &b.trigger == t));
            out.push(BoundShortcut { id: spec.id.clone(), trigger: trigger.as_ref().map(Trigger::describe) });
            if let Some(trigger) = trigger {
                self.bindings.push(Binding { client, owner: owner.into(), id: spec.id, trigger });
            }
        }
        out
    }

    pub fn unbind(&mut self, owner: &str) {
        self.bindings.retain(|b| b.owner != owner);
        self.active.retain(|(_, o, _)| o != owner);
    }

    /// A connection went away: its shortcuts go with it.
    pub fn drop_client(&mut self, client: u64) {
        let owners: Vec<String> = self.bindings.iter().filter(|b| b.client == client).map(|b| b.owner.clone()).collect();
        for o in owners {
            self.unbind(&o);
        }
    }

    pub fn lookup(&self, mods: &ModifiersState, sym: Keysym) -> Option<&Binding> {
        self.bindings.iter().find(|b| b.trigger.matches(mods, sym))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_parse_and_normalize() {
        let t = Trigger::parse("shift+ctrl+A").unwrap();
        assert_eq!(t.describe(), "CTRL+SHIFT+a");
        assert_eq!(Trigger::parse("LOGO+F5").unwrap().describe(), "LOGO+F5");
        assert!(Trigger::parse("CTRL+nonsense").is_none());
        assert!(Trigger::parse("HYPER+a").is_none());
        assert!(Trigger::parse("CTRL+").is_none());
    }

    #[test]
    fn plain_character_keys_are_refused() {
        assert!(!Trigger::parse("a").unwrap().allowed());
        assert!(!Trigger::parse("SHIFT+a").unwrap().allowed());
        assert!(Trigger::parse("F9").unwrap().allowed());
        assert!(Trigger::parse("XF86AudioPlay").unwrap().allowed());
        assert!(Trigger::parse("ALT+a").unwrap().allowed());
    }

    #[test]
    fn owners_cannot_take_each_others_triggers() {
        let mut s = Shortcuts::default();
        let spec = |id: &str, t: &str| ShortcutSpec { id: id.into(), trigger: t.into() };
        let a = s.bind(1, "/a", vec![spec("rec", "CTRL+ALT+r")], |_| false);
        assert_eq!(a[0].trigger.as_deref(), Some("CTRL+ALT+r"));
        let b = s.bind(2, "/b", vec![spec("x", "ctrl+alt+R"), spec("y", "CTRL+ALT+y")], |_| false);
        assert_eq!((b[0].trigger.as_deref(), b[1].trigger.as_deref()), (None, Some("CTRL+ALT+y")));
        s.drop_client(1);
        assert_eq!(s.bindings.len(), 1);
        let shifted = ModifiersState { ctrl: true, alt: true, ..Default::default() };
        assert_eq!(s.lookup(&shifted, Keysym::new(xkb::keysyms::KEY_y)).unwrap().owner, "/b");
    }
}
