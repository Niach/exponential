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
//! A grab that fails says why ([`Status`]): the tray item and Settings →
//! Voice show it, and the shell's key listener ([`super::on_focused_key`])
//! takes the key while an IDE window is focused.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use domain::voice::{ShortcutReason, ShortcutSupport, VoiceOs, VoiceSession};

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

/// The key of a shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Key {
    Space,
    F13,
    /// A side button: 4 = back, 5 = forward (X11 buttons 8 and 9, Windows
    /// XBUTTON1 and XBUTTON2).
    Mouse(u8),
}

/// One `shortcuts` value, parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: Key,
}

impl Shortcut {
    /// `ctrl+space`, `alt+space`, `ctrl+shift+space`, `f13`, `mouse4`, `mouse5`.
    pub(crate) fn parse(value: &str) -> Option<Shortcut> {
        let mut shortcut = Shortcut { ctrl: false, alt: false, shift: false, key: Key::Space };
        let mut key = None;
        for part in value.split('+') {
            match part.trim().to_ascii_lowercase().as_str() {
                "ctrl" => shortcut.ctrl = true,
                "alt" => shortcut.alt = true,
                "shift" => shortcut.shift = true,
                "space" => key = Some(Key::Space),
                "f13" => key = Some(Key::F13),
                "mouse4" => key = Some(Key::Mouse(4)),
                "mouse5" => key = Some(Key::Mouse(5)),
                _ => return None,
            }
        }
        shortcut.key = key?;
        Some(shortcut)
    }

    /// The portal's preferred trigger (the shortcuts XDG spec).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn portal_trigger(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("CTRL");
        }
        if self.alt {
            parts.push("ALT");
        }
        if self.shift {
            parts.push("SHIFT");
        }
        parts.push(match self.key {
            Key::Space => "space",
            Key::F13 => "F13",
            Key::Mouse(_) => "",
        });
        parts.join("+")
    }

    /// Whether a gpui keystroke is this shortcut (the focused fallback).
    pub(crate) fn matches_keystroke(&self, keystroke: &gpui::Keystroke) -> bool {
        let modifiers = &keystroke.modifiers;
        self.key_matches(&keystroke.key)
            && modifiers.control == self.ctrl
            && modifiers.alt == self.alt
            && modifiers.shift == self.shift
            && !modifiers.platform
    }

    /// Whether a gpui key name is this shortcut's key (a release ends the
    /// hold whatever the modifiers do).
    pub(crate) fn key_matches(&self, key: &str) -> bool {
        match self.key {
            Key::Space => key == "space",
            Key::F13 => key == "f13",
            Key::Mouse(_) => false,
        }
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
    match domain::voice::shortcut_support(os(), session(), value, true) {
        ShortcutSupport::Unavailable => {
            let _ = tx.send(HotkeyEvent::Status(Status::FocusedOnly(ShortcutReason::Mouse)));
        }
        ShortcutSupport::Portal | ShortcutSupport::FocusedOnly => {
            #[cfg(target_os = "linux")]
            portal::grab(shortcut, tx, stop);
            #[cfg(not(target_os = "linux"))]
            let _ = (shortcut, tx, stop);
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

    use domain::voice::ShortcutReason;
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::{ButtonIndex, ConnectionExt as _, EventMask, GrabMode, ModMask};
    use x11rb::protocol::Event;

    use super::{HotkeyEvent, Key, Shortcut, Status};

    /// The poll's sleep when the queue is empty (the grab thread's only
    /// wake-up while idle).
    const IDLE: Duration = Duration::from_millis(15);

    const XK_SPACE: u32 = 0x0020;
    const XK_F13: u32 = 0xffca;

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
        // Caps Lock and Num Lock must not break the grab.
        let variants = [ModMask::from(0u16), ModMask::LOCK, ModMask::M2, ModMask::LOCK | ModMask::M2].map(|extra| modifiers | extra);
        let target = match shortcut.key {
            Key::Mouse(n) => Target::Button(if n == 4 { 8 } else { 9 }),
            Key::Space => Target::Key(keycode(&conn, XK_SPACE)?),
            Key::F13 => Target::Key(keycode(&conn, XK_F13)?),
        };
        let grabbed = match target {
            Target::Key(code) => variants.iter().try_for_each(|mods| {
                conn.grab_key(false, root, *mods, code, GrabMode::ASYNC, GrabMode::ASYNC)
                    .map_err(|err| err.to_string())?
                    .check()
                    .map_err(|err| err.to_string())
            }),
            Target::Button(button) => conn
                .grab_button(
                    false,
                    root,
                    EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                    x11rb::NONE,
                    x11rb::NONE,
                    ButtonIndex::from(button),
                    ModMask::ANY,
                )
                .map_err(|err| err.to_string())?
                .check()
                .map_err(|err| err.to_string()),
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
                let _ = conn.ungrab_button(ButtonIndex::from(button), root, ModMask::ANY);
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
    use domain::voice::ShortcutReason;
    use futures_util::StreamExt as _;

    use super::{HotkeyEvent, Shortcut, Status};

    const ID: &str = "push-to-talk";

    pub(super) fn grab(shortcut: Shortcut, tx: flume::Sender<HotkeyEvent>, stop: Arc<AtomicBool>) {
        let spawned = std::thread::Builder::new().name("voice-ptt-portal".into()).spawn(move || {
            futures_lite_block_on(run(shortcut, tx, stop));
        });
        if let Err(err) = spawned {
            log::warn!("[voice] push to talk: no portal thread: {err}");
        }
    }

    async fn run(shortcut: Shortcut, tx: flume::Sender<HotkeyEvent>, stop: Arc<AtomicBool>) {
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
        let wanted = [NewShortcut::new(ID, domain::voice::VOICE.copy.push_to_talk.clone()).preferred_trigger(Some(trigger.as_str()))];
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
        let description = bound.shortcuts().iter().find(|shortcut| shortcut.id() == ID).map(|shortcut| shortcut.trigger_description().to_string());
        let (Ok(activated), Ok(deactivated)) = (portal.receive_activated().await, portal.receive_deactivated().await) else {
            let _ = session.close().await;
            return focused(ShortcutReason::NoPortal);
        };
        let _ = tx.send(HotkeyEvent::Status(Status::Portal { trigger: description }));
        let mut activated = activated.filter(|event| std::future::ready(event.shortcut_id() == ID)).map(|_| HotkeyEvent::Down);
        let mut deactivated = deactivated.filter(|event| std::future::ready(event.shortcut_id() == ID)).map(|_| HotkeyEvent::Up);
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

    use domain::voice::ShortcutReason;
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

    use super::{HotkeyEvent, Key, Shortcut, Status};

    /// Where the process-wide hot-key callback sends: the live hot key's id
    /// and its channel.
    static ROUTE: Mutex<Option<(u32, flume::Sender<HotkeyEvent>)>> = Mutex::new(None);

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
            if let Ok(mut route) = ROUTE.lock() {
                if route.as_ref().is_some_and(|(id, _)| Some(*id) == self.hotkey.map(|hotkey| hotkey.id())) {
                    *route = None;
                }
            }
        }
    }

    pub(super) fn grab(shortcut: Shortcut, tx: flume::Sender<HotkeyEvent>, stop: Arc<AtomicBool>) -> Option<Native> {
        if let Key::Mouse(button) = shortcut.key {
            #[cfg(target_os = "windows")]
            {
                let hook = mouse::Hook::start(button, tx.clone(), stop);
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
        let code = if shortcut.key == Key::F13 { Code::F13 } else { Code::Space };
        let hotkey = HotKey::new(Some(modifiers).filter(|modifiers| !modifiers.is_empty()), code);
        GlobalHotKeyEvent::set_event_handler(Some(|event: GlobalHotKeyEvent| {
            let Ok(route) = ROUTE.lock() else { return };
            if let Some((id, tx)) = route.as_ref().filter(|(id, _)| *id == event.id()) {
                let _ = id;
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
            *route = Some((hotkey.id(), tx.clone()));
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
    /// on the thread that set it); the picked side button is swallowed so a
    /// browser does not also go back.
    #[cfg(target_os = "windows")]
    mod mouse {
        use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
        use std::sync::{Arc, Mutex};

        use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::System::Threading::GetCurrentThreadId;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
            UnhookWindowsHookEx, MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_QUIT, WM_XBUTTONDOWN, WM_XBUTTONUP,
        };

        use super::super::HotkeyEvent;

        /// The button the hook answers (XBUTTON1 = 1, XBUTTON2 = 2) and its channel.
        static TARGET: Mutex<Option<(u16, flume::Sender<HotkeyEvent>)>> = Mutex::new(None);

        pub(crate) struct Hook {
            thread: AtomicU32,
            stop: Arc<AtomicBool>,
        }

        impl Hook {
            pub(crate) fn start(button: u8, tx: flume::Sender<HotkeyEvent>, stop: Arc<AtomicBool>) -> Option<Hook> {
                let xbutton = if button == 4 { 1 } else { 2 };
                if let Ok(mut target) = TARGET.lock() {
                    *target = Some((xbutton, tx));
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

        unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
            if code >= 0 && (wparam as u32 == WM_XBUTTONDOWN || wparam as u32 == WM_XBUTTONUP) {
                // SAFETY: WH_MOUSE_LL hands an MSLLHOOKSTRUCT in lparam.
                let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
                let button = (info.mouseData >> 16) as u16;
                if let Ok(target) = TARGET.lock() {
                    if let Some((wanted, tx)) = target.as_ref().filter(|(wanted, _)| *wanted == button) {
                        let _ = wanted;
                        let _ = tx.send(if wparam as u32 == WM_XBUTTONDOWN { HotkeyEvent::Down } else { HotkeyEvent::Up });
                        return 1;
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
    fn every_fixture_shortcut_parses() {
        for shortcut in &domain::voice::VOICE.shortcuts {
            assert!(Shortcut::parse(&shortcut.value).is_some(), "{}", shortcut.value);
        }
        assert_eq!(
            Shortcut::parse("ctrl+shift+space"),
            Some(Shortcut { ctrl: true, alt: false, shift: true, key: Key::Space })
        );
        assert_eq!(Shortcut::parse("mouse5").map(|shortcut| shortcut.key), Some(Key::Mouse(5)));
        assert_eq!(Shortcut::parse("ctrl+q"), None);
        assert_eq!(Shortcut::parse("ctrl"), None, "a key is required");
    }

    #[test]
    fn the_portal_is_asked_for_the_same_trigger() {
        assert_eq!(Shortcut::parse("ctrl+space").unwrap().portal_trigger(), "CTRL+space");
        assert_eq!(Shortcut::parse("ctrl+shift+space").unwrap().portal_trigger(), "CTRL+SHIFT+space");
        assert_eq!(Shortcut::parse("f13").unwrap().portal_trigger(), "F13");
    }

    #[test]
    fn the_focused_fallback_matches_exact_modifiers() {
        let shortcut = Shortcut::parse("ctrl+space").unwrap();
        assert!(shortcut.matches_keystroke(&gpui::Keystroke::parse("ctrl-space").unwrap()));
        assert!(!shortcut.matches_keystroke(&gpui::Keystroke::parse("ctrl-shift-space").unwrap()));
        assert!(!shortcut.matches_keystroke(&gpui::Keystroke::parse("space").unwrap()));
        assert!(shortcut.key_matches("space"));
        assert!(Shortcut::parse("f13").unwrap().matches_keystroke(&gpui::Keystroke::parse("f13").unwrap()));
    }

    #[test]
    fn a_status_names_its_reason_and_trigger() {
        assert_eq!(Status::FocusedOnly(ShortcutReason::Taken).reason(), Some(ShortcutReason::Taken));
        assert_eq!(Status::Global.reason(), None);
        assert_eq!(Status::Portal { trigger: Some("Super+V".into()) }.trigger(), Some("Super+V"));
        assert_eq!(Status::Portal { trigger: Some(" ".into()) }.trigger(), None);
    }
}
