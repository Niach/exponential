//! Hotkeys the shell answers (#EDE-12): the compositor keeps the key and
//! tells the shell over ede-protocol (`hotkey` events, [`names`]), because
//! the shell's layer surfaces never hold the keyboard while an app has it.
//! Alt+Tab opens the switcher and cycles it (Shift reverses), Alt+` (the
//! key above Tab, whatever the layout) cycles the focused app's windows, the
//! arrow keys move while it is held, releasing Alt commits, Escape cancels; Super alone toggles the menu; Super+A the
//! agent panel (#EDE-243: Super+Space is Cinnamon's layout switch); Super+1…9 the taskbar's nth app (Cinnamon's
//! grouped window list: focus or launch, Shift: a new instance); Super+N the notifications, Shift+Super+C clears
//! them; Alt+F1 Expo as Ctrl+Alt+Up; Ctrl+Alt+Up Expo and Ctrl+Alt+Down Scale (#EDE-49); Print
//! and its Shift/Alt/Ctrl variants the screenshots (Cinnamon's media keys); Ctrl+Alt+T a terminal; the media keys
//! (volume, microphone, brightness, play/next/previous) show the OSD.
//!
//! With no shell listening (dead, or not started yet) the compositor falls
//! back (#EDE-16): Alt+Tab cycles focus itself, most-recent first, and the
//! media keys reach the focused app as they would on a bare compositor.
//! Behaviour follows Cinnamon's defaults; no code is shared with it.

use serde_json::json;
use smithay::{backend::input::KeyState, input::keyboard::{keysyms, ModifiersState}, utils::IsAlive};

use crate::{desk::DeskId, state::Ede};

/// `name` of every `hotkey` event the compositor sends.
pub mod names {
    pub const SWITCHER_NEXT: &str = "switcher.next";
    pub const SWITCHER_PREV: &str = "switcher.prev";
    /// Alt+` / Shift+Alt+`: the focused app's windows only (#EDE-65).
    pub const SWITCHER_NEXT_APP: &str = "switcher.next-app";
    pub const SWITCHER_PREV_APP: &str = "switcher.prev-app";
    pub const SWITCHER_COMMIT: &str = "switcher.commit";
    pub const SWITCHER_CANCEL: &str = "switcher.cancel";
    pub const MENU_TOGGLE: &str = "menu.toggle";
    pub const AGENT_TOGGLE: &str = "agent.toggle";
    /// Super+1…9 (#EDE-243): `taskbar.activate.<n>` (1-based), with Shift
    /// `taskbar.launch.<n>`: the grouped window list's nth app.
    pub const TASKBAR_ACTIVATE: [&str; 9] = ["taskbar.activate.1", "taskbar.activate.2", "taskbar.activate.3", "taskbar.activate.4", "taskbar.activate.5", "taskbar.activate.6", "taskbar.activate.7", "taskbar.activate.8", "taskbar.activate.9"];
    pub const TASKBAR_LAUNCH: [&str; 9] = ["taskbar.launch.1", "taskbar.launch.2", "taskbar.launch.3", "taskbar.launch.4", "taskbar.launch.5", "taskbar.launch.6", "taskbar.launch.7", "taskbar.launch.8", "taskbar.launch.9"];
    /// Super+N / Shift+Super+C: Cinnamon's notifications applet keys.
    pub const NOTIFICATIONS_OPEN: &str = "notifications.open";
    pub const NOTIFICATIONS_CLEAR: &str = "notifications.clear";
    /// Ctrl+Alt+Up: the workspace overview (#EDE-49); Ctrl+Alt+Down: the
    /// current workspace's windows tiled. Both shell overlays, no fallback.
    pub const EXPO_TOGGLE: &str = "expo.toggle";
    pub const SCALE_TOGGLE: &str = "scale.toggle";
    /// Cinnamon's six screenshot keys (its `media-keys` defaults): Print
    /// the whole screen, Shift+Print an area, Alt+Print the focused window,
    /// each with Ctrl to the clipboard instead of a file.
    pub const SCREENSHOT: &str = "screenshot";
    pub const SCREENSHOT_CLIP: &str = "screenshot.clip";
    pub const SCREENSHOT_AREA: &str = "screenshot.area";
    pub const SCREENSHOT_AREA_CLIP: &str = "screenshot.area-clip";
    pub const SCREENSHOT_WINDOW: &str = "screenshot.window";
    pub const SCREENSHOT_WINDOW_CLIP: &str = "screenshot.window-clip";
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
    /// A click outside the shell's open popups, or Escape in one: close them.
    pub const POPUPS_DISMISS: &str = "popups.dismiss";
    /// #EDE-263: Escape while Expo or Scale is open: close it.
    pub const OVERLAY_CLOSE: &str = "overlay.close";
}

/// What a key did, decided inside the keyboard filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotkey {
    SwitcherNext,
    SwitcherPrev,
    SwitcherNextApp,
    SwitcherPrevApp,
    SwitcherCommit,
    SwitcherCancel,
    MenuToggle,
    AgentToggle,
    /// Super+n (0-based), Shift = a new instance.
    Taskbar(usize, bool),
    NotificationsOpen,
    NotificationsClear,
    ExpoToggle,
    ScaleToggle,
    /// `(what, to the clipboard)`.
    Screenshot(Shot, bool),
    Terminal,
    /// A media key the shell shows an OSD for; `None` of a shell = the app gets it.
    Media(&'static str),
    /// Escape in a shell popup.
    PopupsDismiss,
    /// Escape while Expo or Scale is open (#EDE-263).
    OverlayClose,
}

impl Hotkey {
    pub fn name(self) -> &'static str {
        match self {
            Hotkey::SwitcherNext => names::SWITCHER_NEXT,
            Hotkey::SwitcherPrev => names::SWITCHER_PREV,
            Hotkey::SwitcherNextApp => names::SWITCHER_NEXT_APP,
            Hotkey::SwitcherPrevApp => names::SWITCHER_PREV_APP,
            Hotkey::SwitcherCommit => names::SWITCHER_COMMIT,
            Hotkey::SwitcherCancel => names::SWITCHER_CANCEL,
            Hotkey::MenuToggle => names::MENU_TOGGLE,
            Hotkey::AgentToggle => names::AGENT_TOGGLE,
            Hotkey::Taskbar(n, false) => names::TASKBAR_ACTIVATE[n.min(8)],
            Hotkey::Taskbar(n, true) => names::TASKBAR_LAUNCH[n.min(8)],
            Hotkey::NotificationsOpen => names::NOTIFICATIONS_OPEN,
            Hotkey::NotificationsClear => names::NOTIFICATIONS_CLEAR,
            Hotkey::ExpoToggle => names::EXPO_TOGGLE,
            Hotkey::ScaleToggle => names::SCALE_TOGGLE,
            Hotkey::Screenshot(Shot::Screen, false) => names::SCREENSHOT,
            Hotkey::Screenshot(Shot::Screen, true) => names::SCREENSHOT_CLIP,
            Hotkey::Screenshot(Shot::Area, false) => names::SCREENSHOT_AREA,
            Hotkey::Screenshot(Shot::Area, true) => names::SCREENSHOT_AREA_CLIP,
            Hotkey::Screenshot(Shot::Window, false) => names::SCREENSHOT_WINDOW,
            Hotkey::Screenshot(Shot::Window, true) => names::SCREENSHOT_WINDOW_CLIP,
            Hotkey::Terminal => names::TERMINAL,
            Hotkey::Media(name) => name,
            Hotkey::PopupsDismiss => names::POPUPS_DISMISS,
            Hotkey::OverlayClose => names::OVERLAY_CLOSE,
        }
    }
}

/// What a screenshot key takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shot {
    Screen,
    Area,
    Window,
}

/// Cinnamon's bindings: `Print`, `<Shift>Print`, `<Alt>Print`, each also
/// with `<Control>`; Super or Shift+Alt together = none (the app gets it).
/// Alt+Print is `Sys_Req` on the pc layouts' second level, so both count.
pub fn screenshot_key(mods: &ModifiersState, sym: u32) -> Option<(Shot, bool)> {
    use keysyms::*;
    if !(sym == KEY_Print || (sym == KEY_Sys_Req && mods.alt)) || mods.logo {
        return None;
    }
    let shot = match (mods.shift, mods.alt) {
        (false, false) => Shot::Screen,
        (true, false) => Shot::Area,
        (false, true) => Shot::Window,
        (true, true) => return None,
    };
    Some((shot, mods.ctrl))
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
    /// The switcher's own position while the compositor cycles for a dead
    /// shell (#EDE-225): Tab steps forward, Shift+Tab back, over
    /// `fallback_order`.
    fallback_steps: isize,
    /// The windows, most recent first, as they stood at the first Tab.
    fallback_order: Vec<smithay::desktop::Window>,
}

/// The keysym `decide` sees for the key above Tab (xkb keycode 49, evdev
/// KEY_GRAVE) while Alt is held: `grave` on a US layout, a dead key on
/// others, so the compositor names it by position as Cinnamon does
/// (`<Alt>Above_Tab`).
pub const ABOVE_TAB_KEYCODE: u32 = 49;

/// The digit row's 1…9, and its shifted symbols on a US layout.
fn digit(sym: u32) -> Option<usize> {
    use keysyms::*;
    match sym {
        KEY_1..=KEY_9 => Some((sym - KEY_1 + 1) as usize),
        KEY_exclam => Some(1),
        KEY_at => Some(2),
        KEY_numbersign => Some(3),
        KEY_dollar => Some(4),
        KEY_percent => Some(5),
        KEY_asciicircum => Some(6),
        KEY_ampersand => Some(7),
        KEY_asterisk => Some(8),
        KEY_parenleft => Some(9),
        _ => None,
    }
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
            KEY_grave if mods.alt => (true, Some(if mods.shift { Hotkey::SwitcherPrevApp } else { Hotkey::SwitcherNextApp })),
            KEY_Right => (true, Some(Hotkey::SwitcherNext)),
            KEY_Left => (true, Some(Hotkey::SwitcherPrev)),
            KEY_Escape => {
                h.switcher = false;
                (true, Some(Hotkey::SwitcherCancel))
            }
            // Anything else while the switcher is up stays with it.
            _ => (true, None),
        };
    }
    match sym {
        KEY_Tab | KEY_ISO_Left_Tab if mods.alt && !mods.ctrl && !mods.logo => {
            h.switcher = true;
            (true, Some(if mods.shift { Hotkey::SwitcherPrev } else { Hotkey::SwitcherNext }))
        }
        // Alt+`: a shell that listens cycles the app's windows; without one the app keeps the key.
        KEY_grave if mods.alt && !mods.ctrl && !mods.logo && shell_listening => {
            h.switcher = true;
            (true, Some(if mods.shift { Hotkey::SwitcherPrevApp } else { Hotkey::SwitcherNextApp }))
        }
        KEY_a | KEY_A if mods.logo && !mods.alt && !mods.ctrl && !mods.shift => (true, Some(Hotkey::AgentToggle)),
        // Super+1…9 (the digit row by keysym; Shift's symbols on a US layout too).
        _ if mods.logo && !mods.alt && !mods.ctrl && shell_listening && digit(sym).is_some() => (true, Some(Hotkey::Taskbar(digit(sym).unwrap() - 1, mods.shift))),
        KEY_n | KEY_N if mods.logo && !mods.alt && !mods.ctrl && !mods.shift => (shell_listening, Some(Hotkey::NotificationsOpen)),
        KEY_c | KEY_C if mods.logo && mods.shift && !mods.alt && !mods.ctrl => (shell_listening, Some(Hotkey::NotificationsClear)),
        // Alt+F1: Cinnamon's switch-to-workspace-up, Expo.
        KEY_F1 if mods.alt && !mods.ctrl && !mods.logo && !mods.shift => (shell_listening, Some(Hotkey::ExpoToggle)),
        // Expo and Scale are shell overlays (#EDE-49): with no shell the
        // key goes to the app, as Print does.
        KEY_Up if mods.ctrl && mods.alt && !mods.logo && !mods.shift => (shell_listening, Some(Hotkey::ExpoToggle)),
        KEY_Down if mods.ctrl && mods.alt && !mods.logo && !mods.shift => (shell_listening, Some(Hotkey::ScaleToggle)),
        KEY_Print | KEY_Sys_Req if screenshot_key(mods, sym).is_some() => (shell_listening, screenshot_key(mods, sym).map(|(shot, clip)| Hotkey::Screenshot(shot, clip))),
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
            Hotkey::SwitcherCommit => self.cycle_commit(),
            Hotkey::SwitcherCancel => self.cycle_reset(),
            // No shell to launch one: the compositor opens the terminal itself.
            Hotkey::Terminal => self.spawn_for_person(&["x-terminal-emulator".to_string()]),
            _ => {}
        }
    }

    /// Super+Shift+R (#EDE-16): the stock shell documents back as a commit
    /// (`exponential-ui reset`), then the shell's unit restarted. Works with a dead
    /// shell: nothing here needs it. `EDE_RESTORE_COMMAND` replaces the
    /// command (the dev harness, a test).
    pub fn restore_shell(&self) {
        let command = std::env::var("EDE_RESTORE_COMMAND").ok().filter(|c| !c.is_empty()).unwrap_or_else(|| "exponential-ui reset; systemctl --user restart exponential-shell.service".into());
        tracing::warn!(%command, "restoring the shell");
        self.spawn(&["sh".to_string(), "-c".to_string(), command]);
    }

    /// The fallback switcher (#EDE-16): each Tab steps to the next window
    /// in most-recent-first order (the stacking order at the first Tab),
    /// Shift+Tab back; releasing Alt raises and focuses the pick
    /// ([`Self::cycle_commit`]), as Alt+Tab did on window managers before
    /// switchers drew anything. #EDE-225: it reads its own position, so
    /// three windows and Tab twice reach the third.
    pub fn cycle_focus(&mut self, reverse: bool) {
        if self.hotkeys.fallback_order.is_empty() {
            self.hotkeys.fallback_order = self.desk.space.elements().rev().cloned().collect();
            self.hotkeys.fallback_steps = 0;
        }
        if self.hotkeys.fallback_order.len() < 2 {
            self.cycle_reset();
            return;
        }
        self.hotkeys.fallback_steps += if reverse { -1 } else { 1 };
    }

    /// Alt released after the fallback's Tabs: the pick on top, focused.
    fn cycle_commit(&mut self) {
        let (steps, order) = (self.hotkeys.fallback_steps, std::mem::take(&mut self.hotkeys.fallback_order));
        self.cycle_reset();
        if order.len() < 2 {
            return;
        }
        let pick = order[steps.rem_euclid(order.len() as isize) as usize].clone();
        // A window that closed (or was minimized) during the hold is no pick.
        if pick.alive() && self.desk.space.elements().any(|w| *w == pick) {
            self.focus(DeskId::PERSON, Some(pick));
        }
    }

    fn cycle_reset(&mut self) {
        self.hotkeys.fallback_steps = 0;
        self.hotkeys.fallback_order.clear();
    }

    /// A hold released the person's keys (#EDE-80): a Super that was down
    /// alone opens nothing, and an open switcher is cancelled.
    pub(crate) fn hotkeys_let_go(&mut self) {
        self.hotkeys.super_pending = false;
        if std::mem::take(&mut self.hotkeys.switcher) {
            self.cycle_reset();
            self.hotkey(Hotkey::SwitcherCancel);
        }
    }

    /// A pointer press ends a pending Super (Super+click is not a menu); a
    /// press outside the shell's open popups dismisses them, as menus do.
    pub fn hotkeys_pointer_pressed(&mut self, hit: Option<&smithay::reexports::wayland_server::protocol::wl_surface::WlSurface>) {
        self.hotkeys.super_pending = false;
        if self.layer_popups_open() && !hit.is_some_and(|s| self.popups.find_popup(s).is_some()) {
            self.emit(ede_protocol::events::HOTKEY, json!({ "name": names::POPUPS_DISMISS }));
        }
    }

    /// Does a layer surface (the shell) have a popup mapped?
    pub fn layer_popups_open(&self) -> bool {
        self.layer_popups_open_where(|_| true)
    }

    /// #EDE-213: does a layer surface other than a sealed one (the shell's
    /// taskbar, whose Agent spaces list never takes the keyboard) have a
    /// popup mapped? Escape closes those, wherever the keyboard is.
    pub fn shell_popups_open(&self) -> bool {
        self.layer_popups_open_where(|l| !crate::sealed::layer_origin(l).sealed)
    }

    /// #EDE-263: is the shell's Expo or Scale mapped (Cinnamon closes
    /// them on Escape)? Only the shell's own layer counts: another client
    /// naming the namespace keeps no key.
    pub fn escape_closing_overlay_open(&self) -> bool {
        self.desk.space.outputs().any(|o| {
            let map = smithay::desktop::layer_map_for_output(o);
            let any = map.layers().any(|l| crate::sealed::layer_origin(l).shell_overlay && ede_protocol::ESCAPE_CLOSES_NAMESPACES.contains(&l.namespace()));
            any
        })
    }

    fn layer_popups_open_where(&self, which: impl Fn(&smithay::desktop::LayerSurface) -> bool) -> bool {
        use smithay::desktop::PopupManager;
        self.desk.space.outputs().any(|o| {
            let map = smithay::desktop::layer_map_for_output(o);
            let any = map.layers().filter(|l| which(l)).any(|l| PopupManager::popups_for_surface(l.wl_surface()).next().is_some());
            any
        })
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
    fn alt_grave_cycles_the_apps_windows_and_arrows_move_while_alt_is_held() {
        let mut h = Hotkeys::default();
        let alt = mods(true, false, false, false);
        assert_eq!(decide(&mut h, &alt, keysyms::KEY_grave, KeyState::Pressed, true), (true, Some(Hotkey::SwitcherNextApp)));
        assert_eq!(Hotkey::SwitcherNextApp.name(), "switcher.next-app");
        assert_eq!(decide(&mut h, &mods(true, false, false, true), keysyms::KEY_grave, KeyState::Pressed, true), (true, Some(Hotkey::SwitcherPrevApp)));
        assert_eq!(decide(&mut h, &alt, keysyms::KEY_Right, KeyState::Pressed, true), (true, Some(Hotkey::SwitcherNext)));
        assert_eq!(decide(&mut h, &alt, keysyms::KEY_Left, KeyState::Pressed, true), (true, Some(Hotkey::SwitcherPrev)));
        assert_eq!(decide(&mut h, &mods(false, false, false, false), keysyms::KEY_Alt_L, KeyState::Released, true), (false, Some(Hotkey::SwitcherCommit)));
        // Arrows without the switcher are the app's; Alt+` without a shell too.
        assert_eq!(decide(&mut h, &alt, keysyms::KEY_Right, KeyState::Pressed, true), (false, None));
        assert_eq!(decide(&mut h, &alt, keysyms::KEY_grave, KeyState::Pressed, false), (false, None));
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
        assert_eq!(decide(&mut h, &mods(false, true, false, false), keysyms::KEY_a, KeyState::Pressed, true), (true, Some(Hotkey::AgentToggle)));
        assert_eq!(decide(&mut h, &mods(true, false, true, false), keysyms::KEY_t, KeyState::Pressed, true), (true, Some(Hotkey::Terminal)));
        assert_eq!(decide(&mut h, &mods(false, false, false, false), keysyms::KEY_Print, KeyState::Pressed, true), (true, Some(Hotkey::Screenshot(Shot::Screen, false))));
    }

    /// Cinnamon's media-keys defaults, name for name (`org.cinnamon.desktop.keybindings.media-keys`).
    #[test]
    fn the_print_keys_are_cinnamons() {
        let mut h = Hotkeys::default();
        let mut name = |alt, ctrl, shift, sym| decide(&mut h, &mods(alt, false, ctrl, shift), sym, KeyState::Pressed, true).1.map(Hotkey::name);
        assert_eq!(name(false, false, false, keysyms::KEY_Print), Some("screenshot"));
        assert_eq!(name(false, true, false, keysyms::KEY_Print), Some("screenshot.clip"));
        assert_eq!(name(false, false, true, keysyms::KEY_Print), Some("screenshot.area"));
        assert_eq!(name(false, true, true, keysyms::KEY_Print), Some("screenshot.area-clip"));
        assert_eq!(name(true, false, false, keysyms::KEY_Print), Some("screenshot.window"));
        assert_eq!(name(true, false, false, keysyms::KEY_Sys_Req), Some("screenshot.window"), "Alt+Print as xkb's pc layouts send it");
        assert_eq!(name(true, true, false, keysyms::KEY_Sys_Req), Some("screenshot.window-clip"));
        assert_eq!(name(false, false, false, keysyms::KEY_Sys_Req), None, "SysRq alone is no binding");
        assert_eq!(name(true, false, true, keysyms::KEY_Print), None, "Shift+Alt+Print has none");
        // No shell: the key stays the app's.
        assert_eq!(decide(&mut h, &mods(false, false, false, true), keysyms::KEY_Print, KeyState::Pressed, false).0, false);
    }

    #[test]
    fn ctrl_alt_up_and_down_toggle_expo_and_scale_for_a_listening_shell_only() {
        let mut h = Hotkeys::default();
        let ctrl_alt = mods(true, false, true, false);
        assert_eq!(decide(&mut h, &ctrl_alt, keysyms::KEY_Up, KeyState::Pressed, true), (true, Some(Hotkey::ExpoToggle)));
        assert_eq!(decide(&mut h, &ctrl_alt, keysyms::KEY_Down, KeyState::Pressed, true), (true, Some(Hotkey::ScaleToggle)));
        assert_eq!(Hotkey::ExpoToggle.name(), "expo.toggle");
        assert_eq!(Hotkey::ScaleToggle.name(), "scale.toggle");
        // No shell: nothing to open, the app keeps the key.
        assert_eq!(decide(&mut h, &ctrl_alt, keysyms::KEY_Up, KeyState::Pressed, false), (false, Some(Hotkey::ExpoToggle)));
        // Releases and other modifier sets are not Expo (Ctrl+Alt+Shift+Up is free, Super+Up maximizes).
        assert_eq!(decide(&mut h, &ctrl_alt, keysyms::KEY_Up, KeyState::Released, true), (false, None));
        assert_eq!(decide(&mut h, &mods(true, false, true, true), keysyms::KEY_Up, KeyState::Pressed, true), (false, None));
        assert_eq!(decide(&mut h, &mods(false, true, false, false), keysyms::KEY_Up, KeyState::Pressed, true), (false, None));
        // #EDE-243: Super+Space is the layout switch now (a compositor binding), the agent panel Super+A.
        assert_eq!(decide(&mut h, &mods(false, true, false, false), keysyms::KEY_space, KeyState::Pressed, true), (false, None));
        // Alt+F1: Expo, as Cinnamon's switch-to-workspace-up.
        assert_eq!(decide(&mut h, &mods(true, false, false, false), keysyms::KEY_F1, KeyState::Pressed, true), (true, Some(Hotkey::ExpoToggle)));
    }

    /// #EDE-243: the grouped window list's Super+1…9 and the notifications keys.
    #[test]
    fn super_digits_reach_the_taskbar_and_super_n_the_notifications() {
        let mut h = Hotkeys::default();
        let sup = mods(false, true, false, false);
        let sup_shift = mods(false, true, false, true);
        assert_eq!(decide(&mut h, &sup, keysyms::KEY_3, KeyState::Pressed, true), (true, Some(Hotkey::Taskbar(2, false))));
        assert_eq!(Hotkey::Taskbar(2, false).name(), "taskbar.activate.3");
        assert_eq!(decide(&mut h, &sup_shift, keysyms::KEY_numbersign, KeyState::Pressed, true), (true, Some(Hotkey::Taskbar(2, true))));
        assert_eq!(Hotkey::Taskbar(2, true).name(), "taskbar.launch.3");
        assert_eq!(decide(&mut h, &sup, keysyms::KEY_3, KeyState::Pressed, false), (false, None), "no shell: the app keeps it");
        assert_eq!(decide(&mut h, &sup, keysyms::KEY_n, KeyState::Pressed, true), (true, Some(Hotkey::NotificationsOpen)));
        assert_eq!(decide(&mut h, &sup_shift, keysyms::KEY_C, KeyState::Pressed, true), (true, Some(Hotkey::NotificationsClear)));
    }
}
