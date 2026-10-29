//! #NEW-54: the push-to-talk shortcut held system-wide, press AND release
//! (`fixtures/voice.json` `shortcutSupport`).
//!
//! - X11: a passive grab on the root window over our own x11rb connection
//!   (XGrabKey for the keys, with and without Caps/Num Lock; XGrabButton for
//!   the side buttons), XKB's detectable auto-repeat so a held key is one
//!   press and one release.
//! - Wayland: the xdg GlobalShortcuts portal (`ashpd`): one session, one
//!   `push-to-talk` shortcut, Activated / Deactivated = press / release; the
//!   compositor may bind another trigger, whose `trigger_description` then
//!   labels the shortcut. No portal = the key works only while an IDE window
//!   is focused. Never a mouse button.
//! - macOS and Windows: `global-hotkey` (Carbon hot keys; RegisterHotKey plus
//!   release polling), created on gpui's main thread. Windows' side buttons
//!   go through a low-level mouse hook on its own thread; macOS has none.
//!
//! S-3: any `shortcutGrammar` binding (any modifiers incl. Super, a letter,
//! digit, F1-F24, a named key or mouse3-mouse9) on every backend; the X11
//! side buttons take their modifiers too, Windows' hook reads the middle
//! button and XBUTTON1/2 with the held modifiers.
//!
//! A grab that fails says why ([`Status`]): the tray item and Settings →
//! Voice show it, and the shell's key listener ([`super::on_focused_key`])
//! takes the key while an IDE window is focused.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use etalk::voice::{ShortcutReason, ShortcutSupport, VoiceOs, VoiceSession};

/// What a grab reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HotkeyEvent {
    Down,
    Up,
    Status(Status),
}

/// How the shortcut is held right now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Status {
    /// Not grabbed (push to talk off here), or the grab is still starting.
    #[default]
    Idle,
    /// Held system-wide.
    Global,
    /// Bound through the portal; `trigger` = the compositor's description.
    Portal { trigger: Option<String> },
    /// Only while an IDE window is focused, and why.
    FocusedOnly(ShortcutReason),
}

impl Status {
    /// The tray's `shortcutUnavailable` / the Shortcut row's reason.
    pub(crate) fn reason(&self) -> Option<ShortcutReason> {
        match self {
            Status::FocusedOnly(reason) => Some(*reason),
            _ => None,
        }
    }

    /// The portal's trigger description (the tray's `shortcutLabel`).
    pub(crate) fn trigger(&self) -> Option<&str> {
        match self {
            Status::Portal { trigger } => trigger.as_deref().filter(|trigger| !trigger.trim().is_empty()),
            _ => None,
        }
    }
}

/// S-3: the grammar's named keys (`shortcutGrammar.named`, test-locked) with
/// their X11 keysym, xkb name (the portal's trigger) and W3C code
/// (`global-hotkey`).
const NAMED: [(&str, u32, &str, &str); 28] = [
    ("space", 0x0020, "space", "Space"),
    ("enter", 0xff0d, "Return", "Enter"),
    ("tab", 0xff09, "Tab", "Tab"),
    ("backspace", 0xff08, "BackSpace", "Backspace"),
    ("delete", 0xffff, "Delete", "Delete"),
    ("insert", 0xff63, "Insert", "Insert"),
    ("home", 0xff50, "Home", "Home"),
    ("end", 0xff57, "End", "End"),
    ("pageup", 0xff55, "Prior", "PageUp"),
    ("pagedown", 0xff56, "Next", "PageDown"),
    ("up", 0xff52, "Up", "ArrowUp"),
    ("down", 0xff54, "Down", "ArrowDown"),
    ("left", 0xff51, "Left", "ArrowLeft"),
    ("right", 0xff53, "Right", "ArrowRight"),
    ("minus", 0x002d, "minus", "Minus"),
    ("equal", 0x003d, "equal", "Equal"),
    ("bracketleft", 0x005b, "bracketleft", "BracketLeft"),
    ("bracketright", 0x005d, "bracketright", "BracketRight"),
    ("backslash", 0x005c, "backslash", "Backslash"),
    ("semicolon", 0x003b, "semicolon", "Semicolon"),
    ("quote", 0x0027, "apostrophe", "Quote"),
    ("comma", 0x002c, "comma", "Comma"),
    ("period", 0x002e, "period", "Period"),
    ("slash", 0x002f, "slash", "Slash"),
    ("backquote", 0x0060, "grave", "Backquote"),
    ("pause", 0xff13, "Pause", "Pause"),
    ("scrolllock", 0xff14, "Scroll_Lock", "ScrollLock"),
    ("printscreen", 0xff61, "Print", "PrintScreen"),
];

/// The key of a binding (`shortcutGrammar`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Key {
    /// A letter or digit, lowercase ASCII.
    Char(u8),
    /// f1-f24.
    F(u8),
    /// One of `named` (an entry of [`NAMED`]).
    Named(&'static str),
    /// A mouse button 3-9 (3 = middle, 4 = back, 5 = forward).
    Mouse(u8),
}

impl Key {
    /// The grammar's name of a key (`k`, `f13`, `pageup`, `mouse4`).
    fn parse(part: &str) -> Option<Key> {
        let bytes = part.as_bytes();
        if bytes.len() == 1 && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit()) {
            return Some(Key::Char(bytes[0]));
        }
        if let Some(n) = part.strip_prefix("mouse").and_then(|n| n.parse::<u8>().ok()).filter(|_| part.len() == 6) {
            return (3..=9).contains(&n).then_some(Key::Mouse(n));
        }
        if let Some(n) = part.strip_prefix('f').filter(|n| !n.starts_with('0')).and_then(|n| n.parse::<u8>().ok()) {
            return (1..=24).contains(&n).then_some(Key::F(n));
        }
        NAMED.iter().find(|(name, ..)| *name == part).map(|(name, ..)| Key::Named(*name))
    }

    fn named(name: &str) -> Option<&'static (&'static str, u32, &'static str, &'static str)> {
        NAMED.iter().find(|(named, ..)| *named == name)
    }

    /// The X11 keysym (None = a mouse button).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn keysym(self) -> Option<u32> {
        match self {
            // Latin-1 letters and digits: the keysym is the ASCII code.
            Key::Char(byte) => Some(u32::from(byte)),
            Key::F(n) => Some(0xffbe + u32::from(n) - 1),
            Key::Named(name) => Key::named(name).map(|(_, keysym, ..)| *keysym),
            Key::Mouse(_) => None,
        }
    }

    /// The W3C key code `global-hotkey` registers (None = a mouse button).
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
    fn w3c(self) -> Option<String> {
        match self {
            Key::Char(byte) if byte.is_ascii_digit() => Some(format!("Digit{}", byte as char)),
            Key::Char(byte) => Some(format!("Key{}", byte.to_ascii_uppercase() as char)),
            Key::F(n) => Some(format!("F{n}")),
            Key::Named(name) => Key::named(name).map(|(.., code)| code.to_string()),
            Key::Mouse(_) => None,
        }
    }
}

/// S-3: a gpui key name as the grammar's (`escape` kept for the recorder),
/// and whether the name already implies Shift (gpui reports `shift-1` as
/// `!` without Shift on a US layout). None = a key the grammar has no name
/// for.
pub(crate) fn grammar_key(gpui_key: &str) -> Option<(String, bool)> {
    const SHIFTED: [(&str, &str); 21] = [
        ("!", "1"), ("@", "2"), ("#", "3"), ("$", "4"), ("%", "5"), ("^", "6"), ("&", "7"), ("*", "8"), ("(", "9"),
        (")", "0"), ("_", "minus"), ("+", "equal"), ("{", "bracketleft"), ("}", "bracketright"), ("|", "backslash"),
        (":", "semicolon"), ("\"", "quote"), ("<", "comma"), (">", "period"), ("?", "slash"), ("~", "backquote"),
    ];
    const SYMBOLS: [(&str, &str); 11] = [
        ("-", "minus"), ("=", "equal"), ("[", "bracketleft"), ("]", "bracketright"), ("\\", "backslash"),
        (";", "semicolon"), ("'", "quote"), (",", "comma"), (".", "period"), ("/", "slash"), ("`", "backquote"),
    ];
    let key = gpui_key.to_ascii_lowercase();
    if let Some((_, name)) = SHIFTED.iter().find(|(symbol, _)| *symbol == key) {
        return Some((name.to_string(), true));
    }
    if let Some((_, name)) = SYMBOLS.iter().find(|(symbol, _)| *symbol == key) {
        return Some((name.to_string(), false));
    }
    let name = match key.as_str() {
        "escape" => return Some((key, false)),
        "scroll_lock" => "scrolllock".to_string(),
        "print" => "printscreen".to_string(),
        "return" => "enter".to_string(),
        _ => key,
    };
    Key::parse(&name).filter(|key| !matches!(key, Key::Mouse(_))).map(|_| (name, false))
}

/// One binding (`shortcutGrammar`), parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The Super (Windows / Command) key.
    pub super_key: bool,
    pub key: Key,
}

impl Shortcut {
    /// Any grammar value: modifiers in any order, then exactly one key
    /// (`ctrl+shift+k`, `f13`, `alt+super+1`, `ctrl+mouse5`).
    pub(crate) fn parse(value: &str) -> Option<Shortcut> {
        let mut shortcut = Shortcut { ctrl: false, alt: false, shift: false, super_key: false, key: Key::Named("space") };
        let mut key = None;
        for part in value.split('+') {
            match part.trim().to_ascii_lowercase().as_str() {
                "ctrl" => shortcut.ctrl = true,
                "alt" => shortcut.alt = true,
                "shift" => shortcut.shift = true,
                "super" => shortcut.super_key = true,
                other if key.is_none() => key = Some(Key::parse(other)?),
                _ => return None,
            }
        }
        shortcut.key = key?;
        Some(shortcut)
    }

    /// No modifier is part of the binding.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn bare(&self) -> bool {
        !(self.ctrl || self.alt || self.shift || self.super_key)
    }

    /// The portal's preferred trigger (the shortcuts XDG spec).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn portal_trigger(&self) -> String {
        let mut parts = Vec::new();
        for (on, name) in [(self.ctrl, "CTRL"), (self.alt, "ALT"), (self.shift, "SHIFT"), (self.super_key, "LOGO")] {
            if on {
                parts.push(name.to_string());
            }
        }
        parts.push(match self.key {
            Key::Char(byte) => (byte as char).to_string(),
            Key::F(n) => format!("F{n}"),
            Key::Named(name) => Key::named(name).map(|(_, _, xkb, _)| xkb.to_string()).unwrap_or_default(),
            Key::Mouse(_) => String::new(),
        });
        parts.join("+")
    }

    /// Whether a gpui keystroke is this shortcut (the focused fallback).
    pub(crate) fn matches_keystroke(&self, keystroke: &gpui::Keystroke) -> bool {
        let modifiers = &keystroke.modifiers;
        let Some((name, shifted)) = grammar_key(&keystroke.key) else { return false };
        self.key_is(&name)
            && modifiers.control == self.ctrl
            && modifiers.alt == self.alt
            && (modifiers.shift || shifted) == self.shift
            && modifiers.platform == self.super_key
    }

    /// Whether a gpui key name is this shortcut's key (a release ends the
    /// hold whatever the modifiers do).
    pub(crate) fn key_matches(&self, key: &str) -> bool {
        grammar_key(key).is_some_and(|(name, _)| self.key_is(&name))
    }

    fn key_is(&self, name: &str) -> bool {
        !matches!(self.key, Key::Mouse(_)) && Key::parse(name) == Some(self.key)
    }
}

/// This IDE's OS.
pub(crate) fn os() -> VoiceOs {
    if cfg!(target_os = "macos") {
        VoiceOs::Macos
    } else if cfg!(target_os = "windows") {
        VoiceOs::Windows
    } else {
        VoiceOs::Linux
    }
}

/// This IDE's Linux session (gpui runs Wayland whenever it is there).
pub(crate) fn session() -> Option<VoiceSession> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty()) {
        Some(VoiceSession::Wayland)
    } else {
        Some(VoiceSession::X11)
    }
}

/// A live grab; dropping it lets the shortcut go.
pub(crate) struct Grab {
    stop: Arc<AtomicBool>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    _native: Option<native::Native>,
}

impl Drop for Grab {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Hold `value` and report on `tx` (on any thread).
pub(crate) fn grab(value: &str, tx: flume::Sender<HotkeyEvent>) -> Grab {
    grab_as(value, PUSH_TO_TALK_ID, &etalk::voice::VOICE.copy.push_to_talk, tx)
}

/// The portal shortcut id push to talk binds.
const PUSH_TO_TALK_ID: &str = "push-to-talk";

/// [`grab`] under another portal shortcut `id` and description (#NEW-57:
/// the pause hotkey beside push to talk).
pub(crate) fn grab_as(value: &str, id: &'static str, description: &str, tx: flume::Sender<HotkeyEvent>) -> Grab {
    let stop = Arc::new(AtomicBool::new(false));
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(unused_mut))]
    let mut grab = Grab {
        stop: stop.clone(),
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        _native: None,
    };
    let Some(shortcut) = Shortcut::parse(value) else {
        log::warn!("[voice] push to talk: unknown shortcut {value:?}");
        let _ = tx.send(HotkeyEvent::Status(Status::FocusedOnly(ShortcutReason::Taken)));
        return grab;
    };
    match etalk::voice::shortcut_support(os(), session(), value, true) {
        ShortcutSupport::Unavailable => {
            let _ = tx.send(HotkeyEvent::Status(Status::FocusedOnly(ShortcutReason::Mouse)));
        }
        ShortcutSupport::Portal | ShortcutSupport::FocusedOnly => {
            #[cfg(target_os = "linux")]
            portal::grab(shortcut, id, description.to_string(), tx, stop);
            #[cfg(not(target_os = "linux"))]
            let _ = (shortcut, id, description, tx, stop);
        }
        ShortcutSupport::Global => {
            #[cfg(target_os = "linux")]
            x11::grab(shortcut, tx, stop);
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            {
                grab._native = native::grab(shortcut, tx, stop);
            }
            #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
            let _ = (shortcut, tx, stop);
            let _ = (id, description);
        }
    }
    grab
}

/// X11: passive grabs on the root window, polled on their own thread.
#[cfg(target_os = "linux")]
mod x11 {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use etalk::voice::ShortcutReason;
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::{ButtonIndex, ConnectionExt as _, EventMask, GrabMode, ModMask};
    use x11rb::protocol::Event;

    use super::{HotkeyEvent, Key, Shortcut, Status};

    /// The poll's sleep when the queue is empty (the grab thread's only
    /// wake-up while idle).
    const IDLE: Duration = Duration::from_millis(15);

    pub(super) fn grab(shortcut: Shortcut, tx: flume::Sender<HotkeyEvent>, stop: Arc<AtomicBool>) {
        let spawned = std::thread::Builder::new().name("voice-ptt-x11".into()).spawn(move || {
            if let Err(err) = run(shortcut, &tx, &stop) {
                log::warn!("[voice] push to talk: the X11 grab ended: {err}");
                let _ = tx.send(HotkeyEvent::Status(Status::FocusedOnly(ShortcutReason::Taken)));
            }
        });
        if let Err(err) = spawned {
            log::warn!("[voice] push to talk: no grab thread: {err}");
        }
    }

    fn run(shortcut: Shortcut, tx: &flume::Sender<HotkeyEvent>, stop: &AtomicBool) -> Result<(), String> {
        let (conn, screen) = x11rb::connect(None).map_err(|err| err.to_string())?;
        let root = conn.setup().roots[screen].root;
        // A held key repeats as release + press pairs without this.
        {
            use x11rb::protocol::xkb::{self, ConnectionExt as _};
            if conn.xkb_use_extension(1, 0).ok().and_then(|cookie| cookie.reply().ok()).is_some_and(|reply| reply.supported) {
                let flag = xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT;
                let _ = conn.xkb_per_client_flags(xkb::ID::USE_CORE_KBD.into(), flag, flag, 0u32.into(), 0u32.into(), 0u32.into());
            }
        }
        let mut modifiers = ModMask::from(0u16);
        if shortcut.ctrl {
            modifiers |= ModMask::CONTROL;
        }
        if shortcut.alt {
            modifiers |= ModMask::M1;
        }
        if shortcut.shift {
            modifiers |= ModMask::SHIFT;
        }
        if shortcut.super_key {
            modifiers |= ModMask::M4;
        }
        // Caps Lock and Num Lock must not break the grab.
        let variants = [ModMask::from(0u16), ModMask::LOCK, ModMask::M2, ModMask::LOCK | ModMask::M2].map(|extra| modifiers | extra);
        let target = match (shortcut.key, shortcut.key.keysym()) {
            // X11 buttons 4-7 are the wheel: mouse3 = the middle button (2),
            // mouse4 and up = X11 8 and up.
            (Key::Mouse(n), _) => Target::Button(if n == 3 { 2 } else { n + 4 }),
            (_, Some(keysym)) => Target::Key(keycode(&conn, keysym)?),
            (key, None) => return Err(format!("no keysym for {key:?}")),
        };
        // A bare button is held whatever the modifiers (as before S-3); a
        // combination only with exactly its modifiers, like a key.
        let button_mods: Vec<ModMask> = if shortcut.bare() { vec![ModMask::ANY] } else { variants.to_vec() };
        let grabbed = match target {
            Target::Key(code) => variants.iter().try_for_each(|mods| {
                conn.grab_key(false, root, *mods, code, GrabMode::ASYNC, GrabMode::ASYNC)
                    .map_err(|err| err.to_string())?
                    .check()
                    .map_err(|err| err.to_string())
            }),
            Target::Button(button) => button_mods.iter().try_for_each(|mods| {
                conn.grab_button(
                    false,
                    root,
                    EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                    x11rb::NONE,
                    x11rb::NONE,
                    ButtonIndex::from(button),
                    *mods,
                )
                .map_err(|err| err.to_string())?
                .check()
                .map_err(|err| err.to_string())
            }),
        };
        let variants = match target {
            Target::Key(_) => variants.to_vec(),
            Target::Button(_) => button_mods,
        };
        if let Err(err) = grabbed {
            // BadAccess: another client holds it.
            log::info!("[voice] push to talk: the shortcut is taken ({err})");
            let _ = tx.send(HotkeyEvent::Status(Status::FocusedOnly(ShortcutReason::Taken)));
            ungrab(&conn, root, target, &variants);
            return Ok(());
        }
        let _ = tx.send(HotkeyEvent::Status(Status::Global));
        let mut held = false;
        while !stop.load(Ordering::SeqCst) {
            let event = conn.poll_for_event().map_err(|err| err.to_string())?;
            let Some(event) = event else {
                std::thread::sleep(IDLE);
                continue;
            };
            let edge = match (event, target) {
                (Event::KeyPress(press), Target::Key(code)) if press.detail == code => Some(true),
                (Event::KeyRelease(release), Target::Key(code)) if release.detail == code => Some(false),
                (Event::ButtonPress(press), Target::Button(button)) if press.detail == button => Some(true),
                (Event::ButtonRelease(release), Target::Button(button)) if release.detail == button => Some(false),
                _ => None,
            };
            if let Some(down) = edge {
                if down != held {
                    held = down;
                    let _ = tx.send(if down { HotkeyEvent::Down } else { HotkeyEvent::Up });
                }
            }
        }
        if held {
            let _ = tx.send(HotkeyEvent::Up);
        }
        ungrab(&conn, root, target, &variants);
        Ok(())
    }

    #[derive(Clone, Copy)]
    enum Target {
        Key(u8),
        Button(u8),
    }

    fn ungrab(conn: &impl x11rb::connection::Connection, root: u32, target: Target, variants: &[ModMask]) {
        match target {
            Target::Key(code) => {
                for mods in variants {
                    let _ = conn.ungrab_key(code, root, *mods);
                }
            }
            Target::Button(button) => {
                for mods in variants {
                    let _ = conn.ungrab_button(ButtonIndex::from(button), root, *mods);
                }
            }
        }
        let _ = conn.flush();
    }

    /// The keycode that types `keysym` (any level).
    fn keycode(conn: &impl x11rb::connection::Connection, keysym: u32) -> Result<u8, String> {
        let setup = conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let mapping = conn
            .get_keyboard_mapping(min, max - min + 1)
            .map_err(|err| err.to_string())?
            .reply()
            .map_err(|err| err.to_string())?;
        let per = usize::from(mapping.keysyms_per_keycode).max(1);
        mapping
            .keysyms
            .chunks(per)
            .position(|syms| syms.contains(&keysym))
            .map(|index| min + index as u8)
            .ok_or_else(|| format!("no keycode types keysym {keysym:#x}"))
    }
}

/// Wayland: the xdg GlobalShortcuts portal.
#[cfg(target_os = "linux")]
mod portal {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
    use ashpd::desktop::CreateSessionOptions;
    use etalk::voice::ShortcutReason;
    use futures_util::StreamExt as _;

    use super::{HotkeyEvent, Shortcut, Status};

    pub(super) fn grab(shortcut: Shortcut, id: &'static str, description: String, tx: flume::Sender<HotkeyEvent>, stop: Arc<AtomicBool>) {
        let spawned = std::thread::Builder::new().name("voice-ptt-portal".into()).spawn(move || {
            futures_lite_block_on(run(shortcut, id, description, tx, stop));
        });
        if let Err(err) = spawned {
            log::warn!("[voice] push to talk: no portal thread: {err}");
        }
    }

    async fn run(shortcut: Shortcut, id: &'static str, description: String, tx: flume::Sender<HotkeyEvent>, stop: Arc<AtomicBool>) {
        let focused = |reason| {
            let _ = tx.send(HotkeyEvent::Status(Status::FocusedOnly(reason)));
        };
        let Ok(portal) = GlobalShortcuts::new().await else {
            log::info!("[voice] push to talk: no GlobalShortcuts portal; the key works while the IDE is focused");
            return focused(ShortcutReason::NoPortal);
        };
        let session = match portal.create_session(CreateSessionOptions::default()).await {
            Ok(session) => session,
            Err(err) => {
                log::info!("[voice] push to talk: the portal refused a session: {err}");
                return focused(ShortcutReason::NoPortal);
            }
        };
        let trigger = shortcut.portal_trigger();
        let wanted = [NewShortcut::new(id, description).preferred_trigger(Some(trigger.as_str()))];
        let bound = match portal.bind_shortcuts(&session, &wanted, None, BindShortcutsOptions::default()).await {
            Ok(request) => request.response().ok(),
            Err(err) => {
                log::info!("[voice] push to talk: binding failed: {err}");
                None
            }
        };
        let Some(bound) = bound else {
            let _ = session.close().await;
            return focused(ShortcutReason::Declined);
        };
        let description = bound.shortcuts().iter().find(|shortcut| shortcut.id() == id).map(|shortcut| shortcut.trigger_description().to_string());
        let (Ok(activated), Ok(deactivated)) = (portal.receive_activated().await, portal.receive_deactivated().await) else {
            let _ = session.close().await;
            return focused(ShortcutReason::NoPortal);
        };
        let _ = tx.send(HotkeyEvent::Status(Status::Portal { trigger: description }));
        let mut activated = activated.filter(|event| std::future::ready(event.shortcut_id() == id)).map(|_| HotkeyEvent::Down);
        let mut deactivated = deactivated.filter(|event| std::future::ready(event.shortcut_id() == id)).map(|_| HotkeyEvent::Up);
        // Both streams and the stop flag, a poll at a time.
        let mut held = false;
        while !stop.load(Ordering::SeqCst) {
            let tick = std::pin::pin!(async_io_timer(Duration::from_millis(200)));
            let next = futures_util::future::select(futures_util::future::select(activated.next(), deactivated.next()), tick).await;
            let event = match next {
                futures_util::future::Either::Left((futures_util::future::Either::Left((event, _)), _)) => event,
                futures_util::future::Either::Left((futures_util::future::Either::Right((event, _)), _)) => event,
                futures_util::future::Either::Right(_) => continue,
            };
            let Some(event) = event else { break };
            let down = event == HotkeyEvent::Down;
            if down != held {
                held = down;
                let _ = tx.send(event);
            }
        }
        if held {
            let _ = tx.send(HotkeyEvent::Up);
        }
        let _ = session.close().await;
    }

    async fn async_io_timer(after: Duration) {
        async_io::Timer::after(after).await;
    }

    fn futures_lite_block_on<F: std::future::Future>(future: F) -> F::Output {
        async_io::block_on(future)
    }
}

/// macOS and Windows: `global-hotkey` on gpui's main thread; Windows' side
/// buttons through a low-level mouse hook.
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod native {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    use etalk::voice::ShortcutReason;
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

    use super::{HotkeyEvent, Key, Shortcut, Status};

    /// Where the process-wide hot-key callback sends: the live hot key's id
    /// and its channel.
    /// #NEW-57: one per live hot key (push to talk and the pause hotkey).
    static ROUTE: Mutex<Vec<(u32, flume::Sender<HotkeyEvent>)>> = Mutex::new(Vec::new());

    /// Holds the registration; dropping it unregisters.
    pub(crate) struct Native {
        manager: Option<GlobalHotKeyManager>,
        hotkey: Option<HotKey>,
        #[cfg(target_os = "windows")]
        _mouse: Option<mouse::Hook>,
    }

    impl Drop for Native {
        fn drop(&mut self) {
            if let (Some(manager), Some(hotkey)) = (self.manager.as_ref(), self.hotkey) {
                let _ = manager.unregister(hotkey);
            }
            if let (Ok(mut route), Some(hotkey)) = (ROUTE.lock(), self.hotkey) {
                route.retain(|(id, _)| *id != hotkey.id());
            }
        }
    }

    pub(super) fn grab(shortcut: Shortcut, tx: flume::Sender<HotkeyEvent>, stop: Arc<AtomicBool>) -> Option<Native> {
        if let Key::Mouse(button) = shortcut.key {
            #[cfg(target_os = "windows")]
            {
                // WH_MOUSE_LL reports the middle button and XBUTTON1/2 only.
                if !(3..=5).contains(&button) {
                    let _ = tx.send(HotkeyEvent::Status(Status::FocusedOnly(ShortcutReason::Mouse)));
                    return None;
                }
                let hook = mouse::Hook::start(shortcut, tx.clone(), stop);
                let status = if hook.is_some() { Status::Global } else { Status::FocusedOnly(ShortcutReason::Taken) };
                let _ = tx.send(HotkeyEvent::Status(status));
                return Some(Native { manager: None, hotkey: None, _mouse: hook });
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = (button, stop);
                let _ = tx.send(HotkeyEvent::Status(Status::FocusedOnly(ShortcutReason::Mouse)));
                return None;
            }
        }
        let _ = stop;
        let manager = match GlobalHotKeyManager::new() {
            Ok(manager) => manager,
            Err(err) => {
                log::warn!("[voice] push to talk: no hot-key manager: {err}");
                let _ = tx.send(HotkeyEvent::Status(Status::FocusedOnly(ShortcutReason::Taken)));
                return None;
            }
        };
        let mut modifiers = Modifiers::empty();
        if shortcut.ctrl {
            modifiers |= Modifiers::CONTROL;
        }
        if shortcut.alt {
            modifiers |= Modifiers::ALT;
        }
        if shortcut.shift {
            modifiers |= Modifiers::SHIFT;
        }
        if shortcut.super_key {
            modifiers |= Modifiers::SUPER;
        }
        let Some(code) = shortcut.key.w3c().and_then(|code| code.parse::<Code>().ok()) else {
            let _ = tx.send(HotkeyEvent::Status(Status::FocusedOnly(ShortcutReason::Taken)));
            return None;
        };
        let hotkey = HotKey::new(Some(modifiers).filter(|modifiers| !modifiers.is_empty()), code);
        GlobalHotKeyEvent::set_event_handler(Some(|event: GlobalHotKeyEvent| {
            let Ok(route) = ROUTE.lock() else { return };
            if let Some((_, tx)) = route.iter().find(|(id, _)| *id == event.id()) {
                let _ = tx.send(match event.state() {
                    HotKeyState::Pressed => HotkeyEvent::Down,
                    HotKeyState::Released => HotkeyEvent::Up,
                });
            }
        }));
        if let Err(err) = manager.register(hotkey) {
            log::info!("[voice] push to talk: the shortcut is taken ({err})");
            let _ = tx.send(HotkeyEvent::Status(Status::FocusedOnly(ShortcutReason::Taken)));
            return Some(Native {
                manager: Some(manager),
                hotkey: None,
                #[cfg(target_os = "windows")]
                _mouse: None,
            });
        }
        if let Ok(mut route) = ROUTE.lock() {
            route.retain(|(id, _)| *id != hotkey.id());
            route.push((hotkey.id(), tx.clone()));
        }
        let _ = tx.send(HotkeyEvent::Status(Status::Global));
        Some(Native {
            manager: Some(manager),
            hotkey: Some(hotkey),
            #[cfg(target_os = "windows")]
            _mouse: None,
        })
    }

    /// Windows: WH_MOUSE_LL on its own thread (a hook needs a message loop
    /// on the thread that set it); the picked button is swallowed so a
    /// browser does not also go back. S-3: the middle button too, and a
    /// combination fires only while exactly its modifiers are held.
    #[cfg(target_os = "windows")]
    mod mouse {
        use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
        use std::sync::{Arc, Mutex};

        use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::System::Threading::GetCurrentThreadId;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
            UnhookWindowsHookEx, MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_QUIT, WM_XBUTTONDOWN,
            WM_XBUTTONUP,
        };

        use super::super::{HotkeyEvent, Key, Shortcut};

        /// What the hook answers: the button (0 = middle, XBUTTON1 = 1,
        /// XBUTTON2 = 2), the modifiers (ctrl, alt, shift, super) and the channel.
        static TARGET: Mutex<Option<(u16, [bool; 4], flume::Sender<HotkeyEvent>)>> = Mutex::new(None);
        /// The press went out: its release goes out too, whatever the modifiers.
        static PRESSED: AtomicBool = AtomicBool::new(false);

        pub(crate) struct Hook {
            thread: AtomicU32,
            stop: Arc<AtomicBool>,
        }

        impl Hook {
            pub(crate) fn start(shortcut: Shortcut, tx: flume::Sender<HotkeyEvent>, stop: Arc<AtomicBool>) -> Option<Hook> {
                let button = match shortcut.key {
                    Key::Mouse(3) => 0,
                    Key::Mouse(4) => 1,
                    _ => 2,
                };
                let mods = [shortcut.ctrl, shortcut.alt, shortcut.shift, shortcut.super_key];
                PRESSED.store(false, Ordering::SeqCst);
                if let Ok(mut target) = TARGET.lock() {
                    *target = Some((button, mods, tx));
                }
                let (ready_tx, ready) = std::sync::mpsc::channel::<Option<u32>>();
                std::thread::Builder::new()
                    .name("voice-ptt-mouse".into())
                    .spawn(move || unsafe {
                        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), GetModuleHandleW(std::ptr::null()), 0);
                        if hook.is_null() {
                            let _ = ready_tx.send(None);
                            return;
                        }
                        let _ = ready_tx.send(Some(GetCurrentThreadId()));
                        let mut msg: MSG = std::mem::zeroed();
                        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                        UnhookWindowsHookEx(hook);
                    })
                    .ok()?;
                let thread = ready.recv().ok().flatten()?;
                Some(Hook { thread: AtomicU32::new(thread), stop })
            }
        }

        impl Drop for Hook {
            fn drop(&mut self) {
                self.stop.store(true, Ordering::SeqCst);
                if let Ok(mut target) = TARGET.lock() {
                    *target = None;
                }
                // SAFETY: posting WM_QUIT to the hook thread's own queue.
                unsafe {
                    PostThreadMessageW(self.thread.load(Ordering::SeqCst), WM_QUIT, 0, 0);
                }
            }
        }

        /// The modifiers held now (ctrl, alt, shift, super).
        fn held() -> [bool; 4] {
            // SAFETY: GetAsyncKeyState only reads the key state.
            let down = |vk: u16| unsafe { GetAsyncKeyState(i32::from(vk)) } as u16 & 0x8000 != 0;
            [down(VK_CONTROL), down(VK_MENU), down(VK_SHIFT), down(VK_LWIN) || down(VK_RWIN)]
        }

        unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
            let message = wparam as u32;
            let edge = match message {
                WM_XBUTTONDOWN | WM_MBUTTONDOWN => Some(true),
                WM_XBUTTONUP | WM_MBUTTONUP => Some(false),
                _ => None,
            };
            if let (true, Some(down)) = (code >= 0, edge) {
                let button = if message == WM_MBUTTONDOWN || message == WM_MBUTTONUP {
                    0
                } else {
                    // SAFETY: WH_MOUSE_LL hands an MSLLHOOKSTRUCT in lparam.
                    let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
                    (info.mouseData >> 16) as u16
                };
                if let Ok(target) = TARGET.lock() {
                    if let Some((_, mods, tx)) = target.as_ref().filter(|(wanted, ..)| *wanted == button) {
                        let fire = if down { held() == *mods } else { PRESSED.load(Ordering::SeqCst) };
                        if fire {
                            PRESSED.store(down, Ordering::SeqCst);
                            let _ = tx.send(if down { HotkeyEvent::Down } else { HotkeyEvent::Up });
                            return 1;
                        }
                    }
                }
            }
            // SAFETY: passing the event on, as every hook must.
            unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_grammar_value_parses() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../../../../packages/etalk/fixtures/voice.json")).unwrap();
        for case in fixture["shortcutParseCases"].as_array().unwrap() {
            if let Some(value) = case["value"].as_str().filter(|value| !value.is_empty()) {
                assert!(Shortcut::parse(value).is_some(), "{value}");
            }
        }
        let named: Vec<&str> = NAMED.iter().map(|(name, ..)| *name).collect();
        assert_eq!(named, etalk::voice::VOICE.shortcut_grammar.named, "NAMED mirrors the grammar");
        assert_eq!(
            Shortcut::parse("ctrl+shift+space"),
            Some(Shortcut { ctrl: true, alt: false, shift: true, super_key: false, key: Key::Named("space") })
        );
        assert_eq!(Shortcut::parse("alt+super+1").map(|shortcut| (shortcut.super_key, shortcut.key)), Some((true, Key::Char(b'1'))));
        assert_eq!(Shortcut::parse("ctrl+f24").map(|shortcut| shortcut.key), Some(Key::F(24)));
        assert_eq!(Shortcut::parse("mouse5").map(|shortcut| shortcut.key), Some(Key::Mouse(5)));
        assert_eq!(Shortcut::parse("ctrl+mouse3").map(|shortcut| shortcut.key), Some(Key::Mouse(3)));
        assert_eq!(Shortcut::parse("ctrl+enter").map(|shortcut| shortcut.key), Some(Key::Named("enter")));
        for refused in ["ctrl", "mouse2", "f25", "ctrl+a+b", "hyper+k", "media-play"] {
            assert_eq!(Shortcut::parse(refused), None, "{refused}");
        }
        // #NEW-57: the pause hotkey.
        let pause = Shortcut::parse(&etalk::meeting::MEETING.pause.shortcut).unwrap();
        assert_eq!(pause, Shortcut { ctrl: true, alt: true, shift: false, super_key: false, key: Key::Char(b'p') });
        assert_eq!(pause.portal_trigger(), "CTRL+ALT+p");
        assert!(pause.key_matches("p"));
    }

    #[test]
    fn every_key_has_a_keysym_and_a_code() {
        for (name, ..) in NAMED {
            let key = Key::parse(name).unwrap();
            assert!(key.keysym().is_some() && key.w3c().is_some(), "{name}");
        }
        assert_eq!(Key::F(13).keysym(), Some(0xffca));
        assert_eq!(Key::F(1).keysym(), Some(0xffbe));
        assert_eq!(Key::Char(b'k').w3c().as_deref(), Some("KeyK"));
        assert_eq!(Key::Char(b'1').w3c().as_deref(), Some("Digit1"));
        assert_eq!(Key::Mouse(4).keysym(), None);
    }

    #[test]
    fn the_portal_is_asked_for_the_same_trigger() {
        assert_eq!(Shortcut::parse("ctrl+space").unwrap().portal_trigger(), "CTRL+space");
        assert_eq!(Shortcut::parse("ctrl+shift+space").unwrap().portal_trigger(), "CTRL+SHIFT+space");
        assert_eq!(Shortcut::parse("f13").unwrap().portal_trigger(), "F13");
        assert_eq!(Shortcut::parse("alt+super+pageup").unwrap().portal_trigger(), "ALT+LOGO+Prior");
        assert_eq!(Shortcut::parse("ctrl+backquote").unwrap().portal_trigger(), "CTRL+grave");
    }

    #[test]
    fn the_focused_fallback_matches_exact_modifiers() {
        let shortcut = Shortcut::parse("ctrl+space").unwrap();
        assert!(shortcut.matches_keystroke(&gpui::Keystroke::parse("ctrl-space").unwrap()));
        assert!(!shortcut.matches_keystroke(&gpui::Keystroke::parse("ctrl-shift-space").unwrap()));
        assert!(!shortcut.matches_keystroke(&gpui::Keystroke::parse("space").unwrap()));
        assert!(shortcut.key_matches("space"));
        assert!(Shortcut::parse("f13").unwrap().matches_keystroke(&gpui::Keystroke::parse("f13").unwrap()));
        let symbol = Shortcut::parse("alt+backquote").unwrap();
        assert!(symbol.matches_keystroke(&gpui::Keystroke::parse("alt-`").unwrap()));
        let superkey = Shortcut::parse("super+k").unwrap();
        assert!(superkey.matches_keystroke(&gpui::Keystroke::parse("super-k").unwrap()));
        assert!(!superkey.matches_keystroke(&gpui::Keystroke::parse("k").unwrap()));
    }

    #[test]
    fn gpui_key_names_read_as_the_grammar() {
        assert_eq!(grammar_key("k"), Some(("k".into(), false)));
        assert_eq!(grammar_key("!"), Some(("1".into(), true)), "shift-1 on a US layout");
        assert_eq!(grammar_key("`"), Some(("backquote".into(), false)));
        assert_eq!(grammar_key("pageup"), Some(("pageup".into(), false)));
        assert_eq!(grammar_key("scroll_lock"), Some(("scrolllock".into(), false)));
        assert_eq!(grammar_key("escape"), Some(("escape".into(), false)));
        assert_eq!(grammar_key("f13"), Some(("f13".into(), false)));
        assert_eq!(grammar_key("audioplay"), None);
    }

    #[test]
    fn a_status_names_its_reason_and_trigger() {
        assert_eq!(Status::FocusedOnly(ShortcutReason::Taken).reason(), Some(ShortcutReason::Taken));
        assert_eq!(Status::Global.reason(), None);
        assert_eq!(Status::Portal { trigger: Some("Super+V".into()) }.trigger(), Some("Super+V"));
        assert_eq!(Status::Portal { trigger: Some(" ".into()) }.trigger(), None);
    }
}
