//! Linux = X11, through x11rb (pure Rust: no libX11 at load time).
//!
//! Input is XTest on the core pointer and keyboard: exactly what the
//! person's own mouse and keyboard do, so every toolkit takes it (the
//! synthetic-event paths GTK, Qt and Chromium ignore are not used). Keys go
//! through the server's keyboard mapping; a character the layout has no key
//! for borrows a spare keycode for the keystroke, the way xdotool does.
//! Windows come from the window manager's EWMH properties and are
//! activated with `_NET_ACTIVE_WINDOW`.
//!
//! The lock is an XInput2 grab of every physical slave device (the
//! person's keyboards and pointers): a grabbed slave floats off its master
//! for the grab's duration, so nothing it does moves the pointer or reaches
//! a window, while XTest keeps driving the core master. The grab's events
//! come to us, which is how Esc is seen; a device plugged in meanwhile is
//! grabbed as it appears (the hierarchy events). No root, no uinput,
//! released by the server the moment our connection closes.
//!
//! Wayland is out for now: there is no per-window input and no grab a
//! client may take, so the session is refused with a note.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xinput::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{self, ConnectionExt as _};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::backend::{Backend, Button, DisplayInfo, Image, LockGuard, WindowInfo};
use crate::keys::{Key, Named};

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        _NET_CLIENT_LIST_STACKING,
        _NET_ACTIVE_WINDOW,
        _NET_WM_NAME,
        _NET_WM_PID,
        _NET_WM_STATE,
        _NET_WM_STATE_FULLSCREEN,
        _NET_WORKAREA,
        UTF8_STRING,
    }
}

const WAYLAND_NOTE: &str = "This is a Wayland session; computer use needs an X11 session for now (sign in with the \"on Xorg\" session).";

pub(crate) fn unsupported() -> Option<String> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind == "wayland");
    if wayland {
        return Some(WAYLAND_NOTE.to_string());
    }
    if std::env::var_os("DISPLAY").is_none() {
        return Some("No display session (DISPLAY is not set).".to_string());
    }
    None
}

pub(crate) fn open() -> Result<Box<dyn Backend>, String> {
    Ok(Box::new(X11::connect()?))
}

/// The screen as the window manager lays it out, in physical pixels: the
/// root (every monitor side by side), the primary monitor (RandR) and the
/// work area the panels leave (`_NET_WORKAREA`, the first desktop's).
pub(crate) fn screen_layout() -> Option<crate::ScreenLayout> {
    let x11 = X11::connect().ok()?;
    let root = reply("root geometry", x11.conn.get_geometry(x11.root)).ok()?;
    let displays = x11.displays().ok()?;
    let primary = displays.iter().find(|display| display.primary).or_else(|| displays.first())?;
    let area = x11.property32(x11.root, x11.atoms._NET_WORKAREA, xproto::AtomEnum::CARDINAL.into());
    let work_area = match area.as_slice() {
        [x, y, width, height, ..] if *width > 0 && *height > 0 => {
            Some(crate::ScreenRect { x: *x as i32, y: *y as i32, width: *width, height: *height })
        }
        _ => None,
    };
    Some(crate::ScreenLayout {
        root: (u32::from(root.width), u32::from(root.height)),
        primary: crate::ScreenRect { x: primary.x, y: primary.y, width: primary.width, height: primary.height },
        work_area,
    })
}

/// Keycode → keysyms, as the server reports it.
struct KeyMap {
    min: u8,
    max: u8,
    per: u8,
    syms: Vec<u32>,
}

impl KeyMap {
    fn fetch(conn: &RustConnection) -> Result<KeyMap, String> {
        let setup = conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let mapping = reply("keyboard mapping", conn.get_keyboard_mapping(min, max - min + 1))?;
        Ok(KeyMap { min, max, per: mapping.keysyms_per_keycode, syms: mapping.keysyms })
    }

    fn syms_of(&self, keycode: u8) -> &[u32] {
        let per = usize::from(self.per);
        let start = usize::from(keycode - self.min) * per;
        self.syms.get(start..start + per).unwrap_or(&[])
    }

    /// The keycode producing `keysym`, and whether Shift is needed for it
    /// (column 1 of the mapping, the shifted level).
    fn find(&self, keysym: u32) -> Option<(u8, bool)> {
        let mut shifted = None;
        for keycode in self.min..=self.max {
            let syms = self.syms_of(keycode);
            if syms.first() == Some(&keysym) {
                return Some((keycode, false));
            }
            if shifted.is_none() && syms.get(1) == Some(&keysym) {
                shifted = Some((keycode, true));
            }
        }
        shifted
    }

    /// A keycode with no keysym bound, from the top (a scratch key).
    fn spare(&self) -> Option<u8> {
        (self.min..=self.max).rev().find(|&keycode| self.syms_of(keycode).iter().all(|&sym| sym == 0))
    }
}

/// The X keysym for a character: Latin-1 is itself, the rest is the
/// Unicode keysym range.
fn char_keysym(ch: char) -> u32 {
    let cp = u32::from(ch);
    if (0x20..0x7f).contains(&cp) || (0xa0..=0xff).contains(&cp) {
        cp
    } else {
        0x0100_0000 | cp
    }
}

fn keysym_of(key: Key) -> u32 {
    match key {
        Key::Named(named) => named.keysym(),
        Key::Char(ch) => char_keysym(ch),
    }
}

pub(crate) struct X11 {
    conn: RustConnection,
    /// The connection's default screen: `root` is `roots[screen]`'s, and so
    /// must be the visual a capture is decoded with.
    screen: usize,
    root: xproto::Window,
    atoms: Atoms,
    keymap: KeyMap,
    /// The XTEST slave devices our input rides on (XI1 ids), for the
    /// release read-back; `None` when the server does not name them.
    xtest_keyboard: Option<u8>,
    xtest_pointer: Option<u8>,
}

fn x<T>(what: &str, result: Result<T, impl std::fmt::Display>) -> Result<T, String> {
    result.map_err(|err| format!("{what}: {err}"))
}

/// A request's reply, both failure legs (the send, the reply) named.
fn reply<R: x11rb::x11_utils::TryParse>(
    what: &str,
    cookie: Result<x11rb::cookie::Cookie<'_, RustConnection, R>, x11rb::errors::ConnectionError>,
) -> Result<R, String> {
    x(what, x(what, cookie)?.reply())
}

impl X11 {
    fn connect() -> Result<X11, String> {
        let (conn, screen) = x("X connection", x11rb::connect(None))?;
        let root = conn.setup().roots[screen].root;
        let atoms = x("atoms", x("atoms", Atoms::new(&conn))?.reply())?;
        let keymap = KeyMap::fetch(&conn)?;
        let (xtest_keyboard, xtest_pointer) = xtest_devices(&conn);
        Ok(X11 { conn, screen, root, atoms, keymap, xtest_keyboard, xtest_pointer })
    }

    fn flush(&self) -> Result<(), String> {
        x("flush", self.conn.flush())
    }

    fn fake(&self, kind: u8, detail: u8, x_: i16, y_: i16) -> Result<(), String> {
        x("xtest", self.conn.xtest_fake_input(kind, detail, x11rb::CURRENT_TIME, self.root, x_, y_, 0))?;
        self.flush()
    }

    fn fake_key(&self, keycode: u8, down: bool) -> Result<(), String> {
        self.fake(if down { xproto::KEY_PRESS_EVENT } else { xproto::KEY_RELEASE_EVENT }, keycode, 0, 0)?;
        if down {
            return Ok(());
        }
        // The server drops a fake release now and then (seen on X.Org 21
        // with the person's devices floating under the lock: about one key
        // in forty), which leaves the key held on the XTEST slave and
        // autorepeating until this connection closes; the core keyboard
        // reads released meanwhile, so the slave itself is read back, and
        // the release sent again while it still says down.
        let Some(device) = self.xtest_keyboard else { return Ok(()) };
        for attempt in 1..=RELEASE_RETRIES {
            if !self.device_key_down(device, keycode)? {
                return Ok(());
            }
            log::warn!("[computer] key {keycode} still down after its release (try {attempt}), releasing again");
            std::thread::sleep(RELEASE_SETTLE);
            self.fake(xproto::KEY_RELEASE_EVENT, keycode, 0, 0)?;
        }
        Err(format!("Key {keycode} stays pressed."))
    }

    /// [`Backend::mouse_button`]'s release leg, with the same read-back as
    /// [`X11::fake_key`]: a button the server kept down would drag
    /// everything the pointer crosses next.
    fn fake_button_up(&self, detail: u8) -> Result<(), String> {
        self.fake(xproto::BUTTON_RELEASE_EVENT, detail, 0, 0)?;
        let Some(device) = self.xtest_pointer else { return Ok(()) };
        for attempt in 1..=RELEASE_RETRIES {
            if !self.device_button_down(device, detail)? {
                return Ok(());
            }
            log::warn!("[computer] button {detail} still down after its release (try {attempt}), releasing again");
            std::thread::sleep(RELEASE_SETTLE);
            self.fake(xproto::BUTTON_RELEASE_EVENT, detail, 0, 0)?;
        }
        Err(format!("Button {detail} stays pressed."))
    }

    /// Whether `keycode` is down on XI1 device `device` (`QueryDeviceState`).
    fn device_key_down(&self, device: u8, keycode: u8) -> Result<bool, String> {
        let state = reply("device state", self.conn.xinput_query_device_state(device))?;
        Ok(state.classes.iter().any(|class| match &class.data {
            xinput::InputStateData::Key(key) => key_is_down(&key.keys, keycode),
            _ => false,
        }))
    }

    /// Whether button `detail` is down on XI1 device `device`.
    fn device_button_down(&self, device: u8, detail: u8) -> Result<bool, String> {
        let state = reply("device state", self.conn.xinput_query_device_state(device))?;
        Ok(state.classes.iter().any(|class| match &class.data {
            xinput::InputStateData::Button(button) => button_is_down(&button.buttons, detail),
            _ => false,
        }))
    }

    fn property32(&self, window: xproto::Window, property: xproto::Atom, kind: xproto::Atom) -> Vec<u32> {
        reply("property", self.conn.get_property(false, window, property, kind, 0, 4096))
            .ok()
            .and_then(|value| value.value32().map(|values| values.collect()))
            .unwrap_or_default()
    }

    fn property_text(&self, window: xproto::Window, property: xproto::Atom, kind: xproto::Atom) -> Option<String> {
        let value = reply("property", self.conn.get_property(false, window, property, kind, 0, 4096)).ok()?;
        if value.value.is_empty() {
            return None;
        }
        Some(String::from_utf8_lossy(&value.value).into_owned())
    }

    fn active_window(&self) -> Option<xproto::Window> {
        self.property32(self.root, self.atoms._NET_ACTIVE_WINDOW, xproto::AtomEnum::WINDOW.into()).first().copied()
    }

    /// Press (or release) `keysym` through its keycode, borrowing a spare
    /// keycode when the layout has none; `shift` says whether the layout's
    /// Shift level is needed.
    fn keysym_action(&self, keysym: u32, down: bool) -> Result<(), String> {
        if let Some((keycode, shift)) = self.keymap.find(keysym) {
            if shift && down {
                self.fake_key(self.shift_keycode()?, true)?;
            }
            self.fake_key(keycode, down)?;
            if shift && !down {
                self.fake_key(self.shift_keycode()?, false)?;
            }
            return Ok(());
        }
        // A keystroke the layout cannot make: bind the keysym to a spare
        // keycode for the length of the stroke.
        let keycode = self.keymap.spare().ok_or("No spare keycode for a character outside the keyboard layout.")?;
        self.remap(keycode, keysym)?;
        self.fake_key(keycode, down)?;
        if !down {
            self.remap(keycode, 0)?;
        }
        Ok(())
    }

    fn remap(&self, keycode: u8, keysym: u32) -> Result<(), String> {
        let per = usize::from(self.keymap.per);
        let mut syms = vec![0u32; per];
        if let Some(first) = syms.first_mut() {
            *first = keysym;
        }
        x("keyboard mapping", self.conn.change_keyboard_mapping(1, keycode, self.keymap.per, &syms))?;
        // Clients learn of the change through MappingNotify; let the server
        // hand it out before the keystroke rides on it.
        x("sync", self.conn.sync())?;
        std::thread::sleep(Duration::from_millis(8));
        Ok(())
    }

    fn shift_keycode(&self) -> Result<u8, String> {
        self.keymap.find(Named::Shift.keysym()).map(|(keycode, _)| keycode).ok_or_else(|| "No Shift key in the layout.".to_string())
    }
}

impl X11 {
    /// The whole root as one display (no RandR monitors).
    fn root_display(&self) -> Result<DisplayInfo, String> {
        let geometry = reply("root geometry", self.conn.get_geometry(self.root))?;
        Ok(DisplayInfo {
            id: "output:screen".into(),
            x: 0,
            y: 0,
            width: u32::from(geometry.width),
            height: u32::from(geometry.height),
            primary: true,
            name: "screen".into(),
        })
    }

    /// #FEED-161: a monitor's stable id off its first RandR output:
    /// `edid:` + 16 hex of the EDID's hash (the panel itself, whatever
    /// port), else `output:` + the output's name, else the monitor's.
    fn display_id(&self, outputs: &[x11rb::protocol::randr::Output], edid: xproto::Atom, monitor: &str) -> String {
        let Some(&output) = outputs.first() else {
            return format!("output:{monitor}");
        };
        if edid != x11rb::NONE {
            let property = reply(
                "EDID",
                self.conn.randr_get_output_property(output, edid, xproto::AtomEnum::ANY, 0, 256, false, false),
            );
            if let Some(data) = property.ok().map(|property| property.data).filter(|data| !data.is_empty()) {
                return format!("edid:{:016x}", super::stable_hash(&data));
            }
        }
        match reply("output info", self.conn.randr_get_output_info(output, x11rb::CURRENT_TIME)) {
            Ok(info) if !info.name.is_empty() => format!("output:{}", String::from_utf8_lossy(&info.name)),
            _ => format!("output:{monitor}"),
        }
    }
}

impl Backend for X11 {
    fn displays(&self) -> Result<Vec<DisplayInfo>, String> {
        let monitors = match reply("monitors", self.conn.randr_get_monitors(self.root, true)) {
            Ok(monitors) => monitors.monitors,
            Err(reason) => {
                log::debug!("[computer] no RandR monitors ({reason}); the root is the display");
                return Ok(vec![self.root_display()?]);
            }
        };
        let edid = reply("EDID atom", self.conn.intern_atom(true, b"EDID")).map(|atom| atom.atom).unwrap_or(x11rb::NONE);
        let mut displays: Vec<DisplayInfo> = monitors
            .iter()
            .filter(|monitor| monitor.width > 0 && monitor.height > 0)
            .map(|monitor| {
                let name = reply("atom name", self.conn.get_atom_name(monitor.name))
                    .map(|name| String::from_utf8_lossy(&name.name).into_owned())
                    .unwrap_or_default();
                DisplayInfo {
                    id: self.display_id(&monitor.outputs, edid, &name),
                    x: i32::from(monitor.x),
                    y: i32::from(monitor.y),
                    width: u32::from(monitor.width),
                    height: u32::from(monitor.height),
                    primary: monitor.primary,
                    name,
                }
            })
            .collect();
        if displays.is_empty() {
            return Ok(vec![self.root_display()?]);
        }
        if !displays.iter().any(|display| display.primary) {
            displays[0].primary = true;
        }
        super::finish_ids(&mut displays);
        Ok(displays)
    }

    fn screenshot(&self, display: &DisplayInfo) -> Result<Image, String> {
        let (width, height) = (display.width, display.height);
        let shot = reply(
            "screen capture",
            self.conn.get_image(
                xproto::ImageFormat::Z_PIXMAP,
                self.root,
                display.x as i16,
                display.y as i16,
                width as u16,
                height as u16,
                !0,
            ),
        )?;
        let setup = self.conn.setup();
        let bits = setup
            .pixmap_formats
            .iter()
            .find(|format| format.depth == shot.depth)
            .map(|format| format.bits_per_pixel)
            .ok_or_else(|| format!("No pixmap format for depth {}.", shot.depth))?;
        let visual = setup.roots[self.screen]
            .allowed_depths
            .iter()
            .flat_map(|depth| depth.visuals.iter())
            .find(|visual| visual.visual_id == shot.visual)
            .ok_or("The root visual is unknown.")?;
        let bytes = usize::from(bits / 8);
        if bytes != 4 && bytes != 3 {
            return Err(format!("Unsupported screen format: {bits} bits per pixel."));
        }
        let little = setup.image_byte_order == xproto::ImageOrder::LSB_FIRST;
        let channel = |pixel: u32, mask: u32| -> u8 {
            if mask == 0 {
                return 0;
            }
            let shift = mask.trailing_zeros();
            let width = (mask >> shift).count_ones();
            let value = (pixel & mask) >> shift;
            // Widen a narrow channel (a 565 screen) to 8 bits.
            (if width >= 8 { value >> (width - 8) } else { value << (8 - width) }) as u8
        };
        let count = (width as usize) * (height as usize);
        let mut rgba = Vec::with_capacity(count * 4);
        // The common screen (32-bit BGRX, little-endian): a straight copy
        // instead of the mask arithmetic, on fifteen million pixels.
        if bytes == 4 && little && (visual.red_mask, visual.green_mask, visual.blue_mask) == (0xff0000, 0xff00, 0xff) {
            for pixel in shot.data.chunks_exact(4).take(count) {
                rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
            }
            return finish(width, height, rgba, count);
        }
        for pixel in shot.data.chunks_exact(bytes).take(count) {
            let value = match (bytes, little) {
                (4, true) => u32::from_le_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]),
                (4, false) => u32::from_be_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]),
                (_, true) => u32::from_le_bytes([pixel[0], pixel[1], pixel[2], 0]),
                (_, false) => u32::from_be_bytes([0, pixel[0], pixel[1], pixel[2]]),
            };
            rgba.extend_from_slice(&[
                channel(value, visual.red_mask),
                channel(value, visual.green_mask),
                channel(value, visual.blue_mask),
                255,
            ]);
        }
        finish(width, height, rgba, count)
    }

    fn cursor_position(&self) -> Result<(i32, i32), String> {
        let pointer = reply("pointer", self.conn.query_pointer(self.root))?;
        Ok((i32::from(pointer.root_x), i32::from(pointer.root_y)))
    }

    fn list_windows(&self) -> Result<Vec<WindowInfo>, String> {
        let stacking = self.property32(self.root, self.atoms._NET_CLIENT_LIST_STACKING, xproto::AtomEnum::WINDOW.into());
        if stacking.is_empty() {
            return Err("The window manager publishes no window list (_NET_CLIENT_LIST_STACKING).".to_string());
        }
        let active = self.active_window();
        let mut windows = Vec::new();
        for &window in stacking.iter().rev() {
            let Ok(attributes) = reply("attributes", self.conn.get_window_attributes(window)) else { continue };
            if attributes.map_state != xproto::MapState::VIEWABLE {
                continue;
            }
            let Ok(geometry) = reply("geometry", self.conn.get_geometry(window)) else { continue };
            let Ok(origin) = reply("origin", self.conn.translate_coordinates(window, self.root, 0, 0)) else { continue };
            let title = self
                .property_text(window, self.atoms._NET_WM_NAME, self.atoms.UTF8_STRING)
                .or_else(|| self.property_text(window, xproto::AtomEnum::WM_NAME.into(), xproto::AtomEnum::STRING.into()))
                .unwrap_or_default();
            let class = self
                .property_text(window, xproto::AtomEnum::WM_CLASS.into(), xproto::AtomEnum::STRING.into())
                .unwrap_or_default();
            // WM_CLASS = "instance\0class\0": the class names the app.
            let app = class.split('\0').filter(|part| !part.is_empty()).last().unwrap_or_default().to_string();
            let pid = self.property32(window, self.atoms._NET_WM_PID, xproto::AtomEnum::CARDINAL.into()).first().copied();
            // NEW-59: the client list's (the stacking order of
            // `_NET_CLIENT_LIST`) windows in the EWMH fullscreen state.
            let fullscreen = self
                .property32(window, self.atoms._NET_WM_STATE, xproto::AtomEnum::ATOM.into())
                .contains(&self.atoms._NET_WM_STATE_FULLSCREEN);
            windows.push(WindowInfo {
                id: u64::from(window),
                title,
                app,
                pid,
                x: i32::from(origin.dst_x),
                y: i32::from(origin.dst_y),
                width: u32::from(geometry.width),
                height: u32::from(geometry.height),
                active: active == Some(window),
                fullscreen,
            });
        }
        Ok(windows)
    }

    fn activate_window(&self, id: u64) -> Result<(), String> {
        let window = u32::try_from(id).map_err(|_| "Not a window id.".to_string())?;
        // Source indication 2 = a pager: the WM honours it without the
        // focus-stealing-prevention it applies to applications.
        let message = xproto::ClientMessageEvent::new(32, window, self.atoms._NET_ACTIVE_WINDOW, [2, 0, 0, 0, 0]);
        x(
            "activate",
            self.conn.send_event(
                false,
                self.root,
                xproto::EventMask::SUBSTRUCTURE_REDIRECT | xproto::EventMask::SUBSTRUCTURE_NOTIFY,
                message,
            ),
        )?;
        self.flush()?;
        let until = Instant::now() + Duration::from_millis(600);
        while Instant::now() < until {
            if self.active_window() == Some(window) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err(format!("The window manager did not activate window {id}."))
    }

    fn mouse_move(&self, x_: i32, y_: i32) -> Result<(), String> {
        self.fake(xproto::MOTION_NOTIFY_EVENT, 0, x_.clamp(0, i32::from(i16::MAX)) as i16, y_.clamp(0, i32::from(i16::MAX)) as i16)
    }

    fn mouse_button(&self, button: Button, down: bool) -> Result<(), String> {
        let detail = match button {
            Button::Left => 1,
            Button::Middle => 2,
            Button::Right => 3,
        };
        if down {
            self.fake(xproto::BUTTON_PRESS_EVENT, detail, 0, 0)
        } else {
            self.fake_button_up(detail)
        }
    }

    fn scroll(&self, dx: i32, dy: i32) -> Result<(), String> {
        let notches = |button: u8, count: i32| -> Result<(), String> {
            for _ in 0..count {
                self.fake(xproto::BUTTON_PRESS_EVENT, button, 0, 0)?;
                self.fake_button_up(button)?;
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(())
        };
        notches(if dy > 0 { 5 } else { 4 }, dy.abs())?;
        notches(if dx > 0 { 7 } else { 6 }, dx.abs())
    }

    fn key(&self, key: Key, down: bool) -> Result<(), String> {
        self.keysym_action(keysym_of(key), down)
    }

    fn type_char(&self, ch: char) -> Result<(), String> {
        let keysym = char_keysym(ch);
        self.keysym_action(keysym, true)?;
        std::thread::sleep(Duration::from_millis(6));
        self.keysym_action(keysym, false)
    }

    fn lock(&self, on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Box<dyn LockGuard>, String> {
        Ok(Box::new(Lock::take(on_esc)?))
    }
}

/// How often a dropped release is sent again before the call fails.
const RELEASE_RETRIES: u32 = 5;
/// Between a release read back as still down and the next one.
const RELEASE_SETTLE: Duration = Duration::from_millis(5);

/// The XTEST slave keyboard and pointer (XI1 ids) the fake input rides
/// on: the slaves the server names "Virtual core XTEST …".
fn xtest_devices(conn: &RustConnection) -> (Option<u8>, Option<u8>) {
    let Ok(infos) = reply("devices", conn.xinput_xi_query_device(xinput::Device::ALL)) else { return (None, None) };
    let find = |kind: xinput::DeviceType, name: &str| {
        infos
            .infos
            .iter()
            .find(|info| info.type_ == kind && String::from_utf8_lossy(&info.name).starts_with(name))
            .and_then(|info| u8::try_from(info.deviceid).ok())
    };
    (
        find(xinput::DeviceType::SLAVE_KEYBOARD, "Virtual core XTEST keyboard"),
        find(xinput::DeviceType::SLAVE_POINTER, "Virtual core XTEST pointer"),
    )
}

/// Whether `keycode` reads pressed in a device's key bitmap (one bit per
/// keycode, least significant first).
fn key_is_down(keys: &[u8; 32], keycode: u8) -> bool {
    keys[usize::from(keycode / 8)] & (1 << (keycode % 8)) != 0
}

/// Whether button `detail` reads pressed in a device's button bitmap (bit
/// `detail`; buttons count from 1).
fn button_is_down(buttons: &[u8; 32], detail: u8) -> bool {
    buttons[usize::from(detail / 8)] & (1 << (detail % 8)) != 0
}

fn finish(width: u32, height: u32, rgba: Vec<u8>, count: usize) -> Result<Image, String> {
    if rgba.len() != count * 4 {
        return Err("The screen capture came back short.".to_string());
    }
    Ok(Image { width, height, rgba })
}

/// Which XInput2 devices the lock takes: the person's own, never the XTest
/// slaves our input rides on (or a leftover virtual pair of an old driver).
fn lockable(info: &xinput::XIDeviceInfo) -> bool {
    let kind = info.type_;
    if kind != xinput::DeviceType::SLAVE_KEYBOARD && kind != xinput::DeviceType::SLAVE_POINTER {
        return false;
    }
    if !info.enabled {
        return false;
    }
    let name = String::from_utf8_lossy(&info.name);
    !name.starts_with("Virtual core XTEST") && !name.starts_with("CUA v1.")
}

/// The grab, alive on its own thread with its own connection until dropped
/// (or until Esc, after which the thread ends on its own and the drop only
/// collects it).
struct Lock {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Lock {
    fn take(on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Lock, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let thread = std::thread::Builder::new()
            .name("computer-use-lock".into())
            .spawn({
                let stop = stop.clone();
                move || match Grabs::take() {
                    Ok(grabs) => {
                        let _ = ready_tx.send(Ok(()));
                        grabs.hold(&stop, &*on_esc);
                    }
                    Err(reason) => {
                        let _ = ready_tx.send(Err(reason));
                    }
                }
            })
            .map_err(|err| format!("lock thread: {err}"))?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Lock { stop, thread: Some(thread) }),
            Ok(Err(reason)) => Err(reason),
            Err(_) => {
                stop.store(true, Ordering::Release);
                Err("The X server did not answer the grab.".to_string())
            }
        }
    }
}

impl LockGuard for Lock {}

impl Drop for Lock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            if thread.thread().id() != std::thread::current().id() {
                let _ = thread.join();
            }
        }
    }
}

/// X11's keycode for Escape under evdev (`KEY_ESC` 1 + the 8 X offsets),
/// the stand-in when a keymap names no Escape keysym at all.
const EVDEV_ESCAPE_KEYCODE: u8 = 9;

struct Grabs {
    conn: RustConnection,
    root: xproto::Window,
    devices: Vec<xinput::DeviceId>,
    escape: Option<u8>,
}

/// The grab's event selection: keys and buttons (Esc), motion (nothing).
fn grab_mask() -> u32 {
    (xinput::XIEventMask::KEY_PRESS
        | xinput::XIEventMask::KEY_RELEASE
        | xinput::XIEventMask::BUTTON_PRESS
        | xinput::XIEventMask::BUTTON_RELEASE
        | xinput::XIEventMask::MOTION)
        .into()
}

impl Grabs {
    fn take() -> Result<Grabs, String> {
        let (conn, screen) = x("X connection", x11rb::connect(None))?;
        let root = conn.setup().roots[screen].root;
        reply("XInput 2", conn.xinput_xi_query_version(2, 0))?;
        let keymap = KeyMap::fetch(&conn)?;
        let escape = keymap.find(Named::Escape.keysym()).map(|(keycode, _)| keycode).or_else(|| {
            // A layout without an Escape keysym would leave the hotkey dead.
            log::warn!("[computer] the keymap has no Escape keysym; falling back to keycode {EVDEV_ESCAPE_KEYCODE}");
            Some(EVDEV_ESCAPE_KEYCODE)
        });
        // A keyboard plugged in during the hold must not slip past it.
        let hierarchy = [xinput::EventMask { deviceid: xinput::Device::ALL.into(), mask: vec![xinput::XIEventMask::HIERARCHY.into()] }];
        x("hierarchy events", conn.xinput_xi_select_events(root, &hierarchy))?;
        let mut grabs = Grabs { conn, root, devices: Vec::new(), escape };
        if let Err(reason) = grabs.grab_new() {
            grabs.release();
            return Err(reason);
        }
        log::debug!("[computer] grabbed {} input device(s)", grabs.devices.len());
        Ok(grabs)
    }

    /// Grab every lockable device not grabbed yet.
    fn grab_new(&mut self) -> Result<(), String> {
        let infos = reply("devices", self.conn.xinput_xi_query_device(xinput::Device::ALL))?.infos;
        let new: Vec<&xinput::XIDeviceInfo> =
            infos.iter().filter(|info| lockable(info) && !self.devices.contains(&info.deviceid)).collect();
        for info in new {
            let grabbed = reply(
                "grab",
                self.conn.xinput_xi_grab_device(
                    self.root,
                    x11rb::CURRENT_TIME,
                    x11rb::NONE,
                    info.deviceid,
                    xproto::GrabMode::ASYNC,
                    xproto::GrabMode::ASYNC,
                    xinput::GrabOwner::NO_OWNER,
                    &[grab_mask()],
                ),
            )?;
            if grabbed.status != xproto::GrabStatus::SUCCESS {
                let name = String::from_utf8_lossy(&info.name).into_owned();
                return Err(format!("another program holds \"{name}\" ({:?})", grabbed.status));
            }
            self.devices.push(info.deviceid);
        }
        x("flush", self.conn.flush())
    }

    /// Watch the grabbed devices until `stop`, or until Esc (then `on_esc`
    /// runs on this thread and the grabs go).
    fn hold(mut self, stop: &AtomicBool, on_esc: &(dyn Fn() + Send + Sync)) {
        while !stop.load(Ordering::Acquire) {
            match self.conn.poll_for_event() {
                Ok(Some(Event::XinputKeyPress(key))) if Some(key.detail as u8) == self.escape => {
                    log::info!("[computer] Esc from device {}", key.sourceid);
                    self.release();
                    on_esc();
                    return;
                }
                Ok(Some(Event::XinputKeyPress(key))) => {
                    log::debug!("[computer] lock saw key {} from device {}", key.detail, key.sourceid);
                }
                Ok(Some(Event::XinputButtonPress(button))) => {
                    log::debug!("[computer] lock saw button {} from device {}", button.detail, button.sourceid);
                }
                Ok(Some(Event::XinputHierarchy(_))) => {
                    if let Err(reason) = self.grab_new() {
                        log::warn!("[computer] a new input device escaped the lock: {reason}");
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(err) => {
                    log::warn!("[computer] the lock's connection broke: {err}");
                    return;
                }
            }
        }
        self.release();
    }

    fn release(&mut self) {
        for device in self.devices.drain(..) {
            let _ = self.conn.xinput_xi_ungrab_device(x11rb::CURRENT_TIME, device);
        }
        let _ = self.conn.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(kind: xinput::DeviceType, enabled: bool, name: &str) -> xinput::XIDeviceInfo {
        xinput::XIDeviceInfo { deviceid: 1, type_: kind, attachment: 0, enabled, name: name.as_bytes().to_vec(), classes: Vec::new() }
    }

    #[test]
    fn the_lock_takes_the_persons_slaves_and_leaves_the_xtest_ones() {
        assert!(lockable(&device(xinput::DeviceType::SLAVE_KEYBOARD, true, "AT Translated Set 2 keyboard")));
        assert!(lockable(&device(xinput::DeviceType::SLAVE_POINTER, true, "Logitech USB Receiver Mouse")));
        assert!(!lockable(&device(xinput::DeviceType::SLAVE_POINTER, true, "Virtual core XTEST pointer")));
        assert!(!lockable(&device(xinput::DeviceType::SLAVE_KEYBOARD, true, "Virtual core XTEST keyboard")));
        assert!(!lockable(&device(xinput::DeviceType::MASTER_POINTER, true, "Virtual core pointer")));
        assert!(!lockable(&device(xinput::DeviceType::SLAVE_KEYBOARD, false, "Power Button")));
        assert!(!lockable(&device(xinput::DeviceType::SLAVE_POINTER, true, "CUA v1.123 uinput pointer")));
    }

    #[test]
    fn a_dropped_release_is_read_back_from_the_core_state() {
        let mut keys = [0u8; 32];
        assert!(!key_is_down(&keys, 36));
        keys[36 / 8] |= 1 << (36 % 8);
        assert!(key_is_down(&keys, 36));
        assert!(!key_is_down(&keys, 37));
        let mut buttons = [0u8; 32];
        assert!(!button_is_down(&buttons, 1));
        buttons[0] |= 1 << 1;
        assert!(button_is_down(&buttons, 1));
        assert!(!button_is_down(&buttons, 3));
    }

    #[test]
    fn keysyms_follow_x11() {
        assert_eq!(char_keysym('a'), 0x61);
        assert_eq!(char_keysym('ü'), 0xfc);
        assert_eq!(char_keysym('€'), 0x0100_20ac);
        assert_eq!(keysym_of(Key::Named(Named::Return)), 0xff0d);
        assert_eq!(keysym_of(Key::Named(Named::F(12))), 0xffc9);
    }

    #[test]
    fn the_keymap_finds_plain_and_shifted_keys_and_a_spare_keycode() {
        // Keycodes 8..=11, two keysyms each: 8 = a/A, 9 = 1/!, 10 = empty, 11 = Shift.
        let keymap = KeyMap { min: 8, max: 11, per: 2, syms: vec![0x61, 0x41, 0x31, 0x21, 0, 0, 0xffe1, 0] };
        assert_eq!(keymap.find(0x61), Some((8, false)));
        assert_eq!(keymap.find(0x41), Some((8, true)));
        assert_eq!(keymap.find(0x21), Some((9, true)));
        assert_eq!(keymap.find(0xff0d), None);
        assert_eq!(keymap.spare(), Some(10));
    }
}
