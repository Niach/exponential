//! Hotkeys the shell answers (#EDE-12): the compositor keeps the key and
//! tells the shell over ede-protocol (`hotkey` events, [`names`]), because
//! the shell's layer surfaces never hold the keyboard while an app has it.
//! Alt+Tab opens the switcher and cycles it (Shift reverses), releasing Alt
//! commits, Escape cancels; Super alone toggles the menu; Super+Space the
//! agent panel; Print the screenshot; Ctrl+Alt+T a terminal; the media keys
//! (volume, microphone, brightness, play/next/previous) show the OSD.
//!
//! With no shell listening (dead, or not started yet) the compositor falls
//! back (#EDE-16): Alt+Tab cycles focus itself, most-recent first, and the
//! media keys reach the focused app as they would on a bare compositor.
//! Behaviour follows Cinnamon's defaults; no code is shared with it.

use serde_json::json;
use smithay::{backend::input::KeyState, input::keyboard::{keysyms, ModifiersState}};

use crate::state::Ede;

/// `name` of every `hotkey` event the compositor sends.
pub mod names {
    pub const SWITCHER_NEXT: &str = "switcher.next";
    pub const SWITCHER_PREV: &str = "switcher.prev";
    pub const SWITCHER_COMMIT: &str = "switcher.commit";
    pub const SWITCHER_CANCEL: &str = "switcher.cancel";
    pub const MENU_TOGGLE: &str = "menu.toggle";
    pub const AGENT_TOGGLE: &str = "agent.toggle";
    pub const SCREENSHOT: &str = "screenshot";
    pub const TERMINAL: &str = "terminal";
    pub const VOLUME_UP: &str = "volume.up";
    pub const VOLUME_DOWN: &str = "volume.down";
    pub const VOLUME_MUTE: &str = "volume.mute";
    pub const MIC_MUTE: &str = "mic.mute";
    pub const BRIGHTNESS_UP: &str = "brightness.up";
    pub const BRIGHTNESS_DOWN: &str = "brightness.down";
    pub const MEDIA_PLAY: &str = "media.play";
    pub const MEDIA_NEXT: &str = "media.next";
    pub const MEDIA_PREV: &str = "media.prev";
}

/// What a key did, decided inside the keyboard filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotkey {
    SwitcherNext,
    SwitcherPrev,
    SwitcherCommit,
    SwitcherCancel,
    MenuToggle,
    AgentToggle,
    Screenshot,
    Terminal,
    /// A media key the shell shows an OSD for; `None` of a shell = the app gets it.
    Media(&'static str),
}

impl Hotkey {
    pub fn name(self) -> &'static str {
        match self {
            Hotkey::SwitcherNext => names::SWITCHER_NEXT,
            Hotkey::SwitcherPrev => names::SWITCHER_PREV,
            Hotkey::SwitcherCommit => names::SWITCHER_COMMIT,
            Hotkey::SwitcherCancel => names::SWITCHER_CANCEL,
            Hotkey::MenuToggle => names::MENU_TOGGLE,
            Hotkey::AgentToggle => names::AGENT_TOGGLE,
            Hotkey::Screenshot => names::SCREENSHOT,
            Hotkey::Terminal => names::TERMINAL,
            Hotkey::Media(name) => name,
        }
    }
}

/// The key state between presses.
#[derive(Debug, Default)]
pub struct Hotkeys {
    /// Alt+Tab is held: Tab cycles, releasing Alt commits, Escape cancels.
    pub switcher: bool,
    /// Super went down and nothing else since (its release opens the menu).
    super_pending: bool,
    /// Decided in the keyboard filter, run after the key was delivered.
    pub(crate) fired: Option<Hotkey>,
    /// The switcher's own position while the compositor cycles for a dead shell.
    fallback_steps: usize,
}

/// `(keep the key from the app?, the hotkey)`, pure so it can be tested.
#[allow(non_upper_case_globals)]
pub fn decide(h: &mut Hotkeys, mods: &ModifiersState, sym: u32, state: KeyState, shell_listening: bool) -> (bool, Option<Hotkey>) {
    use keysyms::*;
    let is_super = sym == KEY_Super_L || sym == KEY_Super_R;
    let is_alt = sym == KEY_Alt_L || sym == KEY_Alt_R;
    if state == KeyState::Released {
        if h.switcher && is_alt {
            h.switcher = false;
            h.fallback_steps = 0;
            return (false, Some(Hotkey::SwitcherCommit));
        }
        if is_super && std::mem::take(&mut h.super_pending) {
            return (false, Some(Hotkey::MenuToggle));
        }
        return (false, None);
    }
    if is_super {
        h.super_pending = true;
        return (false, None);
    }
    h.super_pending = false;
    if h.switcher {
        return match sym {
            KEY_Tab | KEY_ISO_Left_Tab if mods.alt => (true, Some(if mods.shift { Hotkey::SwitcherPrev } else { Hotkey::SwitcherNext })),
            KEY_Escape => {
                h.switcher = false;
                h.fallback_steps = 0;
                (true, Some(Hotkey::SwitcherCancel))
            }
            // Anything else while the switcher is up stays with it.
            _ => (true, None),
        };
    }
    let plain = |mods: &ModifiersState| !mods.ctrl && !mods.shift && !mods.logo;
    match sym {
        KEY_Tab | KEY_ISO_Left_Tab if mods.alt && !mods.ctrl && !mods.logo => {
            h.switcher = true;
            (true, Some(if mods.shift { Hotkey::SwitcherPrev } else { Hotkey::SwitcherNext }))
        }
        KEY_space if mods.logo && !mods.alt && !mods.ctrl && !mods.shift => (true, Some(Hotkey::AgentToggle)),
        KEY_Print if plain(mods) && !mods.alt => (shell_listening, Some(Hotkey::Screenshot)),
        KEY_t | KEY_T if mods.ctrl && mods.alt && !mods.logo => (true, Some(Hotkey::Terminal)),
        _ => {
            let media = match sym {
                KEY_XF86AudioRaiseVolume => Some(names::VOLUME_UP),
                KEY_XF86AudioLowerVolume => Some(names::VOLUME_DOWN),
                KEY_XF86AudioMute => Some(names::VOLUME_MUTE),
                KEY_XF86AudioMicMute => Some(names::MIC_MUTE),
                KEY_XF86MonBrightnessUp => Some(names::BRIGHTNESS_UP),
                KEY_XF86MonBrightnessDown => Some(names::BRIGHTNESS_DOWN),
                KEY_XF86AudioPlay => Some(names::MEDIA_PLAY),
                KEY_XF86AudioNext => Some(names::MEDIA_NEXT),
                KEY_XF86AudioPrev => Some(names::MEDIA_PREV),
                _ => None,
            };
            // The shell shows the OSD and changes the level; without one
            // the focused app may handle the key itself.
            (media.is_some() && shell_listening, media.map(Hotkey::Media))
        }
    }
}

impl Ede {
    /// A decided hotkey: the shell hears it; with no shell the compositor
    /// does what it can itself.
    pub fn hotkey(&mut self, hotkey: Hotkey) {
        let listening = self.shell_listening();
        tracing::debug!(name = hotkey.name(), listening, "hotkey");
        if listening {
            self.emit(ede_protocol::events::HOTKEY, json!({ "name": hotkey.name() }));
            return;
        }
        match hotkey {
            Hotkey::SwitcherNext => self.cycle_focus(false),
            Hotkey::SwitcherPrev => self.cycle_focus(true),
            _ => {}
        }
    }

    /// The fallback switcher (#EDE-16): focus moves to the next window in
    /// most-recent-first order (the stacking order), as Alt+Tab did on
    /// window managers before switchers drew anything.
    pub fn cycle_focus(&mut self, reverse: bool) {
        let windows: Vec<_> = self.space.elements().cloned().collect();
        if windows.len() < 2 {
            return;
        }
        let target = if reverse { windows.first().cloned() } else { windows.iter().rev().nth(1).cloned() };
        self.hotkeys.fallback_steps += 1;
        self.focus(target);
    }

    /// A pointer press ends a pending Super (Super+click is not a menu).
    pub fn hotkeys_pointer_pressed(&mut self) {
        self.hotkeys.super_pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods(alt: bool, logo: bool, ctrl: bool, shift: bool) -> ModifiersState {
        ModifiersState { alt, logo, ctrl, shift, ..Default::default() }
    }

    #[test]
    fn alt_tab_opens_cycles_and_commits_on_alt_release() {
        let mut h = Hotkeys::default();
        assert_eq!(decide(&mut h, &mods(true, false, false, false), keysyms::KEY_Tab, KeyState::Pressed, true), (true, Some(Hotkey::SwitcherNext)));
        assert!(h.switcher);
        assert_eq!(decide(&mut h, &mods(true, false, false, false), keysyms::KEY_Tab, KeyState::Released, true), (false, None));
        assert_eq!(decide(&mut h, &mods(true, false, false, true), keysyms::KEY_ISO_Left_Tab, KeyState::Pressed, true), (true, Some(Hotkey::SwitcherPrev)));
        // Other keys stay with the switcher while it is up.
        assert_eq!(decide(&mut h, &mods(true, false, false, false), keysyms::KEY_a, KeyState::Pressed, true), (true, None));
        assert_eq!(decide(&mut h, &mods(false, false, false, false), keysyms::KEY_Alt_L, KeyState::Released, true), (false, Some(Hotkey::SwitcherCommit)));
        assert!(!h.switcher);
    }

    #[test]
    fn escape_cancels_the_switcher() {
        let mut h = Hotkeys::default();
        decide(&mut h, &mods(true, false, false, false), keysyms::KEY_Tab, KeyState::Pressed, true);
        assert_eq!(decide(&mut h, &mods(true, false, false, false), keysyms::KEY_Escape, KeyState::Pressed, true), (true, Some(Hotkey::SwitcherCancel)));
        assert_eq!(decide(&mut h, &mods(false, false, false, false), keysyms::KEY_Alt_L, KeyState::Released, true), (false, None));
    }

    #[test]
    fn super_alone_toggles_the_menu_but_super_plus_a_key_does_not() {
        let mut h = Hotkeys::default();
        assert_eq!(decide(&mut h, &mods(false, false, false, false), keysyms::KEY_Super_L, KeyState::Pressed, true), (false, None));
        assert_eq!(decide(&mut h, &mods(false, true, false, false), keysyms::KEY_Super_L, KeyState::Released, true), (false, Some(Hotkey::MenuToggle)));
        decide(&mut h, &mods(false, false, false, false), keysyms::KEY_Super_L, KeyState::Pressed, true);
        assert_eq!(decide(&mut h, &mods(false, true, false, false), keysyms::KEY_l, KeyState::Pressed, true), (false, None));
        assert_eq!(decide(&mut h, &mods(false, true, false, false), keysyms::KEY_Super_L, KeyState::Released, true), (false, None));
    }

    #[test]
    fn media_keys_go_to_the_shell_or_else_to_the_app() {
        let mut h = Hotkeys::default();
        assert_eq!(decide(&mut h, &mods(false, false, false, false), keysyms::KEY_XF86AudioRaiseVolume, KeyState::Pressed, true), (true, Some(Hotkey::Media(names::VOLUME_UP))));
        assert_eq!(decide(&mut h, &mods(false, false, false, false), keysyms::KEY_XF86AudioRaiseVolume, KeyState::Pressed, false), (false, Some(Hotkey::Media(names::VOLUME_UP))));
        assert_eq!(decide(&mut h, &mods(false, true, false, false), keysyms::KEY_space, KeyState::Pressed, true), (true, Some(Hotkey::AgentToggle)));
        assert_eq!(decide(&mut h, &mods(true, false, true, false), keysyms::KEY_t, KeyState::Pressed, true), (true, Some(Hotkey::Terminal)));
        assert_eq!(decide(&mut h, &mods(false, false, false, false), keysyms::KEY_Print, KeyState::Pressed, true), (true, Some(Hotkey::Screenshot)));
    }
}
