//! Linux = X11, through x11rb (pure Rust: no libX11 at load time).
//!
//! Input is XTest on the core pointer and keyboard: exactly what the
//! person's own mouse and keyboard do, so every toolkit takes it (the
//! synthetic-event paths GTK, Qt and Chromium ignore are not used). Keys go
//! through the server's keyboard map (XKB: the layout group in effect, its
//! Shift and AltGr levels); a character no key makes borrows a spare
//! keycode for the call, the way xdotool does.
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
//! Wayland is out (there is no per-window input and no grab a client may
//! take), so the session is refused with a note; except on eDE, whose own
//! control plane drives it (`ede.rs`, #EDE-23).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xinput::{self, ConnectionExt as _};
use x11rb::protocol::xkb::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{self, ConnectionExt as _};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::backend::{Backend, Button, Clipboard, DisplayInfo, Image, LockGuard, WindowInfo};
use crate::keys::{Key, Named};

#[path = "x11_clipboard.rs"]
mod clipboard;

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
    Ok(Box::new(X11::connect(None)?))
}

/// The screen as the window manager lays it out, in physical pixels: the
/// root (every monitor side by side), the primary monitor (RandR) and the
/// work area the panels leave (`_NET_WORKAREA`, the first desktop's).
pub(crate) fn screen_layout() -> Option<crate::ScreenLayout> {
    let x11 = X11::connect(None).ok()?;
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

/// One key of the keyboard map: `width` keysyms for each of its layout
/// groups, and the key type (the index into [`KeyMap::types`]) that says
/// which modifiers pick which of a group's levels.
#[derive(Default, Clone)]
struct KeySyms {
    groups: u8,
    /// A group past the key's last one is the last (else it wraps around).
    clamp: bool,
    types: [u8; 4],
    width: u8,
    syms: Vec<u32>,
}

/// A key type: of the modifiers in `mask`, which set picks which level
/// (any other set, none included, is level 0).
struct Levels {
    mask: u8,
    map: Vec<(u8, u8)>,
}

const SHIFT: u8 = 1 << 0;
/// What `ISO_Level3_Shift` (AltGr) sets: Mod5 on every xkeyboard-config layout.
const LEVEL3: u8 = 1 << 7;
const ISO_LEVEL3_SHIFT: u32 = 0xfe03;

/// The server's keyboard map, every layout group of it (XKB).
struct KeyMap {
    min: u8,
    max: u8,
    /// Keysyms per keycode of the core mapping, which `remap` writes.
    per: u8,
    keys: Vec<KeySyms>,
    types: Vec<Levels>,
}

impl KeyMap {
    fn fetch(conn: &RustConnection) -> Result<KeyMap, String> {
        let setup = conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let mapping = reply("keyboard mapping", conn.get_keyboard_mapping(min, max - min + 1))?;
        let mut keymap = KeyMap::core(min, max, mapping.keysyms_per_keycode, &mapping.keysyms);
        if let Err(reason) = keymap.fetch_groups(conn) {
            log::debug!("[computer] no XKB map ({reason}); the core mapping's first layout is the keyboard");
        }
        Ok(keymap)
    }

    /// The core mapping's first two columns: one layout, plain and shifted.
    fn core(min: u8, max: u8, per: u8, syms: &[u32]) -> KeyMap {
        let keys = syms
            .chunks(usize::from(per).max(1))
            .map(|syms| KeySyms { groups: 1, width: per.min(2), syms: syms.iter().take(2).copied().collect(), ..KeySyms::default() })
            .collect();
        KeyMap { min, max, per, keys, types: vec![Levels { mask: SHIFT, map: vec![(SHIFT, 1)] }] }
    }

    /// #EXP-1907: every group and level of every key, with the key types.
    fn fetch_groups(&mut self, conn: &RustConnection) -> Result<(), String> {
        reply("XKB", conn.xkb_use_extension(1, 0))?;
        let parts = xkb::MapPart::KEY_TYPES | xkb::MapPart::KEY_SYMS;
        let none = xkb::MapPart::from(0u16);
        let cookie = conn.xkb_get_map(CORE_KEYBOARD, parts, none, 0, 0, 0, 0, 0, 0, 0, 0, 0u16.into(), 0, 0, 0, 0, 0, 0);
        let map = reply("XKB map", cookie)?;
        let (Some(types), Some(syms)) = (map.map.types_rtrn, map.map.syms_rtrn) else {
            return Err("the map came back without its types or keysyms".to_string());
        };
        let mods = |mask: xproto::ModMask| u16::from(mask) as u8;
        self.types = types
            .iter()
            .map(|kind| Levels {
                mask: mods(kind.mods_mask),
                map: kind.map.iter().filter(|entry| entry.active).map(|entry| (mods(entry.mods_mask), entry.level)).collect(),
            })
            .collect();
        self.keys = vec![KeySyms::default(); usize::from(self.max - self.min) + 1];
        for (keycode, key) in (map.first_key_sym..=u8::MAX).zip(syms) {
            let Some(slot) = keycode.checked_sub(self.min).and_then(|at| self.keys.get_mut(usize::from(at))) else { continue };
            *slot = KeySyms {
                groups: key.group_info & 0x0f,
                clamp: key.group_info & 0xc0 == 0x40,
                types: key.kt_index,
                width: key.width,
                syms: key.syms,
            };
        }
        Ok(())
    }

    /// What `keycode` makes in layout group `group` with `mods` in effect.
    fn sym_at(&self, keycode: u8, group: u8, mods: u8) -> Option<u32> {
        let key = self.keys.get(usize::from(keycode.checked_sub(self.min)?))?;
        let group = match key.groups {
            0 => return None,
            groups if group < groups => group,
            groups if key.clamp => groups - 1,
            groups => group % groups,
        };
        let levels = self.types.get(usize::from(*key.types.get(usize::from(group))?))?;
        let level = levels.map.iter().find(|(set, _)| *set == mods & levels.mask).map_or(0, |(_, level)| *level);
        let at = usize::from(group) * usize::from(key.width) + usize::from(level);
        key.syms.get(at).copied().filter(|&sym| sym != 0)
    }

    /// The keycode making `keysym` in layout group `group` while the
    /// `locked` modifiers (Caps Lock) are on, and what to hold for it
    /// ([`SHIFT`], [`LEVEL3`]); the plainest wins.
    fn find(&self, keysym: u32, group: u8, locked: u8) -> Option<(u8, u8)> {
        let level3 = self.key_of(ISO_LEVEL3_SHIFT).map_or(0, |_| LEVEL3);
        [0, SHIFT, LEVEL3, SHIFT | LEVEL3].into_iter().filter(|held| held & LEVEL3 & !level3 == 0).find_map(|held| {
            (self.min..=self.max).find(|&keycode| self.sym_at(keycode, group, locked | held) == Some(keysym)).map(|keycode| (keycode, held))
        })
    }

    /// The first keycode with `keysym` anywhere on it (a modifier's key).
    fn key_of(&self, keysym: u32) -> Option<u8> {
        (self.min..=self.max).zip(&self.keys).find(|(_, key)| key.syms.contains(&keysym)).map(|(keycode, _)| keycode)
    }

    /// A keycode with no keysym bound and not in `taken`, from the top (a
    /// scratch key).
    fn spare(&self, taken: &[u8]) -> Option<u8> {
        let free = |key: &KeySyms| key.syms.iter().all(|&sym| sym == 0);
        (self.min..=self.max).zip(&self.keys).rev().find(|(keycode, key)| free(key) && !taken.contains(keycode)).map(|(keycode, _)| keycode)
    }
}

/// XKB's name for the core keyboard (`XkbUseCoreKbd`).
const CORE_KEYBOARD: xkb::DeviceSpec = 0x100;

/// The X keysym for a character: Latin-1 is itself, the rest is the
/// Unicode keysym range.
pub(super) fn char_keysym(ch: char) -> u32 {
    let cp = u32::from(ch);
    if (0x20..0x7f).contains(&cp) || (0xa0..=0xff).contains(&cp) {
        cp
    } else {
        0x0100_0000 | cp
    }
}

/// The keysyms a layout may name `ch` by: its own and the legacy one (`€`
/// is `EuroSign` 0x20ac on a key, Cyrillic and Greek letters likewise).
fn char_keysyms(ch: char) -> Vec<u32> {
    let mut keysyms = vec![char_keysym(ch)];
    let legacy = xkeysym::Keysym::from_char(ch).raw();
    if legacy != 0 && !keysyms.contains(&legacy) {
        keysyms.push(legacy);
    }
    keysyms
}

pub(super) fn keysym_of(key: Key) -> u32 {
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
    /// The layout group in effect and the locked modifiers (Caps Lock), as
    /// the call found them: the person's input is held while it runs.
    group: u8,
    locked: u8,
    /// Keysyms no key makes, on the spare keycodes they borrowed for this
    /// call (unbound on drop).
    borrowed: std::cell::RefCell<Vec<(u32, u8)>>,
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
    /// `display` = the X display (`None`: `DISPLAY`).
    fn connect(display: Option<&str>) -> Result<X11, String> {
        let (conn, screen) = x("X connection", x11rb::connect(display))?;
        let root = conn.setup().roots[screen].root;
        let atoms = x("atoms", x("atoms", Atoms::new(&conn))?.reply())?;
        let keymap = KeyMap::fetch(&conn)?;
        let (group, locked) = match reply("XKB state", conn.xkb_get_state(CORE_KEYBOARD)) {
            Ok(state) => (u8::from(state.group), u16::from(state.locked_mods) as u8),
            Err(_) => (0, 0),
        };
        let (xtest_keyboard, xtest_pointer) = xtest_devices(&conn);
        let borrowed = std::cell::RefCell::new(Vec::new());
        Ok(X11 { conn, screen, root, atoms, keymap, group, locked, borrowed, xtest_keyboard, xtest_pointer })
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

    /// Press (or release) the first of `keysyms` the active layout has a
    /// key for, holding the Shift and AltGr its level needs on top of the
    /// `locked` modifiers; a keysym no key makes borrows a spare keycode.
    fn keysym_action(&self, keysyms: &[u32], locked: u8, down: bool) -> Result<(), String> {
        let Some((keycode, held)) = keysyms.iter().find_map(|&keysym| self.keymap.find(keysym, self.group, locked)) else {
            let keycode = self.borrow(keysyms[0])?;
            return self.fake_key(keycode, down);
        };
        let mut holds = Vec::new();
        for (bit, keysym, name) in [(SHIFT, Named::Shift.keysym(), "Shift"), (LEVEL3, ISO_LEVEL3_SHIFT, "AltGr")] {
            if held & bit != 0 {
                holds.push(self.keymap.key_of(keysym).ok_or_else(|| format!("No {name} key in the layout."))?);
            }
        }
        if down {
            for &hold in &holds {
                self.fake_key(hold, true)?;
            }
        }
        self.fake_key(keycode, down)?;
        if !down {
            for &hold in holds.iter().rev() {
                self.fake_key(hold, false)?;
            }
        }
        Ok(())
    }

    /// The spare keycode `keysym` is bound to for this call: bound once,
    /// when first needed, and unbound when the call ends ([`Drop`]), so a
    /// stroke never races the remap of its own key (#EXP-1907).
    fn borrow(&self, keysym: u32) -> Result<u8, String> {
        let mut borrowed = self.borrowed.borrow_mut();
        if let Some((_, keycode)) = borrowed.iter().find(|(bound, _)| *bound == keysym) {
            return Ok(*keycode);
        }
        let taken: Vec<u8> = borrowed.iter().map(|(_, keycode)| *keycode).collect();
        // Out of spare keycodes: the oldest binding (its stroke is over).
        let keycode = match self.keymap.spare(&taken) {
            Some(keycode) => keycode,
            None if !borrowed.is_empty() => borrowed.remove(0).1,
            None => return Err("No spare keycode for a character outside the keyboard layout.".to_string()),
        };
        self.remap(keycode, keysym)?;
        // Clients learn of the change through MappingNotify; let the server
        // hand it out before the keystroke rides on it.
        x("sync", self.conn.sync())?;
        std::thread::sleep(Duration::from_millis(8));
        borrowed.push((keysym, keycode));
        Ok(keycode)
    }

    /// `keysym` on every level of `keycode` (0 unbinds it), so whatever
    /// group and Caps Lock are in effect the key makes it.
    fn remap(&self, keycode: u8, keysym: u32) -> Result<(), String> {
        let syms = vec![keysym; usize::from(self.keymap.per)];
        x("keyboard mapping", self.conn.change_keyboard_mapping(1, keycode, self.keymap.per, &syms))?;
        Ok(())
    }
}

impl Drop for X11 {
    fn drop(&mut self) {
        let borrowed = std::mem::take(self.borrowed.get_mut());
        for (_, keycode) in &borrowed {
            let _ = self.remap(*keycode, 0);
        }
        if !borrowed.is_empty() {
            let _ = self.conn.sync();
        }
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
    /// port), else `output:` + the output's name, else the monitor's; with
    /// its connector (#FEED-164: the output's name, else the monitor's),
    /// which tells twins apart.
    fn display_id(&self, outputs: &[x11rb::protocol::randr::Output], edid: xproto::Atom, monitor: &str) -> (String, Option<String>) {
        let fallback = Some(monitor.to_string()).filter(|monitor| !monitor.is_empty());
        let Some(&output) = outputs.first() else {
            return (format!("output:{monitor}"), fallback);
        };
        let connector = match reply("output info", self.conn.randr_get_output_info(output, x11rb::CURRENT_TIME)) {
            Ok(info) if !info.name.is_empty() => Some(String::from_utf8_lossy(&info.name).into_owned()),
            _ => fallback,
        };
        if edid != x11rb::NONE {
            let property = reply(
                "EDID",
                self.conn.randr_get_output_property(output, edid, xproto::AtomEnum::ANY, 0, 256, false, false),
            );
            if let Some(data) = property.ok().map(|property| property.data).filter(|data| !data.is_empty()) {
                return (format!("edid:{:016x}", super::stable_hash(&data)), connector);
            }
        }
        (format!("output:{}", connector.as_deref().unwrap_or(monitor)), connector)
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
        let mut connectors: Vec<Option<String>> = Vec::with_capacity(monitors.len());
        let mut displays: Vec<DisplayInfo> = monitors
            .iter()
            .filter(|monitor| monitor.width > 0 && monitor.height > 0)
            .map(|monitor| {
                let name = reply("atom name", self.conn.get_atom_name(monitor.name))
                    .map(|name| String::from_utf8_lossy(&name.name).into_owned())
                    .unwrap_or_default();
                let (id, connector) = self.display_id(&monitor.outputs, edid, &name);
                connectors.push(connector);
                DisplayInfo {
                    id,
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
        super::finish_ids(&mut displays, &connectors);
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
        // A key press is the key itself, as the person's own: Caps Lock is
        // not countered (Shift would turn ctrl+z into ctrl+shift+z).
        self.keysym_action(&[keysym_of(key)], 0, down)
    }

    fn type_char(&self, ch: char) -> Result<(), String> {
        let keysyms = char_keysyms(ch);
        self.keysym_action(&keysyms, self.locked, true)?;
        std::thread::sleep(Duration::from_millis(6));
        self.keysym_action(&keysyms, self.locked, false)
    }

    fn lock(&self, on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Box<dyn LockGuard>, String> {
        Ok(Box::new(Lock::take(None, on_esc)?))
    }

    fn clipboard(&self) -> Result<Box<dyn Clipboard + '_>, String> {
        Ok(Box::new(clipboard::X11Clipboard::open(self)?))
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
/// (or until Esc or the loss of its connection, after which the thread ends
/// on its own and the drop only collects it).
struct Lock {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Lock {
    /// `display` = the X display the grab is taken on (`None`: `DISPLAY`).
    fn take(display: Option<String>, on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Lock, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let thread = std::thread::Builder::new()
            .name("computer-use-lock".into())
            .spawn({
                let stop = stop.clone();
                move || match Grabs::take(display.as_deref()) {
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

/// #EXP-1906: the ids of `devices` a hierarchy event says the server removed
/// are no longer grabbed ones. X.Org hands a removed device's id to the next
/// one, the same keyboard back from a KVM switch or a Bluetooth sleep: left
/// in the list, it would never be grabbed again. A device that was only
/// disabled keeps its id and its grab.
fn forget_gone(devices: &mut Vec<xinput::DeviceId>, infos: &[xinput::HierarchyInfo]) {
    let removed = u32::from(xinput::HierarchyMask::SLAVE_REMOVED);
    devices.retain(|device| !infos.iter().any(|info| info.deviceid == *device && u32::from(info.flags) & removed != 0));
}

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
    fn take(display: Option<&str>) -> Result<Grabs, String> {
        let (conn, screen) = x("X connection", x11rb::connect(display))?;
        let root = conn.setup().roots[screen].root;
        reply("XInput 2", conn.xinput_xi_query_version(2, 0))?;
        let keymap = KeyMap::fetch(&conn)?;
        let escape = keymap.find(Named::Escape.keysym(), 0, 0).map(|(keycode, _)| keycode).or_else(|| {
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

    /// Watch the grabbed devices until `stop`, or until Esc or the loss of
    /// the connection (then `on_esc` runs on this thread and the grabs go).
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
                Ok(Some(Event::XinputHierarchy(event))) => {
                    forget_gone(&mut self.devices, &event.infos);
                    if let Err(reason) = self.grab_new() {
                        log::warn!("[computer] a new input device escaped the lock: {reason}");
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                // An event x11rb cannot read: the connection is fine.
                Err(x11rb::errors::ConnectionError::ParseError(err)) => {
                    log::debug!("[computer] the lock skipped an event: {err}");
                }
                // The connection went and the server freed every grab with
                // it: the host must not believe the input is still held, so
                // it hears an abort (#EXP-1906), unless the stop was ours.
                Err(err) => {
                    if !stop.load(Ordering::Acquire) {
                        log::error!("[computer] the lock's connection broke: {err}");
                        on_esc();
                    }
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

    fn hierarchy(deviceid: xinput::DeviceId, flags: xinput::HierarchyMask) -> xinput::HierarchyInfo {
        xinput::HierarchyInfo { deviceid, attachment: 3, type_: xinput::DeviceType::SLAVE_KEYBOARD, enabled: true, flags }
    }

    /// #EXP-1906: X.Org hands a removed device's id to the next one, the
    /// same keyboard back from a KVM switch: an id the server took away is
    /// no longer a grabbed one, so `grab_new` takes whatever carries it next.
    #[test]
    fn a_removed_device_leaves_the_grabbed_list() {
        let mut devices = vec![9, 10, 11, 12];
        forget_gone(
            &mut devices,
            &[
                hierarchy(9, xinput::HierarchyMask::SLAVE_REMOVED),
                hierarchy(10, xinput::HierarchyMask::DEVICE_DISABLED),
                hierarchy(11, xinput::HierarchyMask::SLAVE_DETACHED),
                hierarchy(13, xinput::HierarchyMask::SLAVE_REMOVED),
            ],
        );
        // 10 keeps its grab while disabled, 11 only floated (what our own
        // grab does to it), 12 is not named.
        assert_eq!(devices, [10, 11, 12]);
    }

    /// A private Xvfb for the tests that need a server. It picks a free
    /// display number itself (`-displayfd`), so it is never the person's,
    /// and it dies with this value.
    struct Xvfb {
        server: std::process::Child,
        display: String,
    }

    impl Xvfb {
        /// `None` (the test is skipped) where no Xvfb is installed.
        fn start() -> Option<Xvfb> {
            use std::io::BufRead as _;
            use std::process::Stdio;
            let server = std::process::Command::new("Xvfb")
                .args(["-displayfd", "1", "-nolisten", "tcp", "-screen", "0", "640x480x24"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            let Ok(server) = server else {
                eprintln!("skipped: no Xvfb on this machine");
                return None;
            };
            let mut xvfb = Xvfb { server, display: String::new() };
            let mut number = String::new();
            std::io::BufReader::new(xvfb.server.stdout.take().unwrap()).read_line(&mut number).unwrap();
            xvfb.display = format!(":{}", number.trim());
            Some(xvfb)
        }

        fn stop(&mut self) {
            let _ = self.server.kill();
            let _ = self.server.wait();
        }

        /// A second client, with the id of the server's own keyboard slave.
        fn client(&self) -> (RustConnection, xproto::Window, xinput::DeviceId) {
            let (conn, screen) = x11rb::connect(Some(&self.display)).unwrap();
            let root = conn.setup().roots[screen].root;
            conn.xinput_xi_query_version(2, 0).unwrap().reply().unwrap();
            let infos = conn.xinput_xi_query_device(xinput::Device::ALL).unwrap().reply().unwrap().infos;
            let keyboard = infos.iter().find(|info| info.name == b"Xvfb keyboard").expect("Xvfb's keyboard").deviceid;
            (conn, root, keyboard)
        }
    }

    impl Drop for Xvfb {
        fn drop(&mut self) {
            self.stop();
        }
    }

    /// Whether somebody else holds `device`: our own grab of it is refused.
    fn grabbed_elsewhere(conn: &RustConnection, root: xproto::Window, device: xinput::DeviceId) -> bool {
        let (mode, owner) = (xproto::GrabMode::ASYNC, xinput::GrabOwner::NO_OWNER);
        let status = conn
            .xinput_xi_grab_device(root, x11rb::CURRENT_TIME, x11rb::NONE, device, mode, mode, owner, &[grab_mask()])
            .unwrap()
            .reply()
            .unwrap()
            .status;
        if status == xproto::GrabStatus::SUCCESS {
            conn.xinput_xi_ungrab_device(x11rb::CURRENT_TIME, device).unwrap();
            conn.sync().unwrap();
        }
        status == xproto::GrabStatus::ALREADY_GRABBED
    }

    /// Disable or enable a device (a server without hotplug removes none).
    fn set_enabled(conn: &RustConnection, device: xinput::DeviceId, on: bool) {
        let property = conn.intern_atom(false, b"Device Enabled").unwrap().reply().unwrap().atom;
        let value = xinput::XIChangePropertyAux::Data8(vec![u8::from(on)]);
        conn.xinput_xi_change_property(device, xproto::PropMode::REPLACE, property, xproto::AtomEnum::INTEGER.into(), 1, &value)
            .unwrap()
            .check()
            .unwrap();
    }

    /// The keyboard-mapping changes a client sees until the server is idle.
    fn mapping_changes(conn: &RustConnection) -> usize {
        conn.sync().unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let mut seen = 0;
        while let Some(event) = conn.poll_for_event().unwrap() {
            if let Event::MappingNotify(notify) = event {
                seen += usize::from(notify.request == xproto::Mapping::KEYBOARD);
            }
        }
        seen
    }

    /// #EXP-1907: a real server's map and state. Its Y and Z keys get a
    /// second layout group the German way round, which is then locked, with
    /// Caps Lock: a keysym is found in the group in effect.
    #[test]
    fn the_servers_map_is_read_by_layout_group() {
        let Some(xvfb) = Xvfb::start() else { return };
        let (conn, _, _) = xvfb.client();
        let x11 = X11::connect(Some(&xvfb.display)).unwrap();
        // evdev keycodes on a US board: 52 = Z, 29 = Y, 11 = 2.
        assert_eq!((x11.group, x11.locked), (0, 0));
        assert_eq!(x11.keymap.find(0x7a, 0, 0), Some((52, 0)));
        assert_eq!(x11.keymap.find(0x40, 0, 0), Some((11, SHIFT)));
        assert_eq!(x11.keymap.find(0x7a, 0, 0x02), Some((52, SHIFT)), "Shift under Caps Lock makes z");
        assert_eq!(x11.keymap.find(0x5a, 0, 0x02), Some((52, 0)), "Caps Lock makes Z");
        conn.change_keyboard_mapping(1, 29, 4, &[0x79, 0x59, 0x7a, 0x5a]).unwrap().check().unwrap();
        conn.change_keyboard_mapping(1, 52, 4, &[0x7a, 0x5a, 0x79, 0x59]).unwrap().check().unwrap();
        conn.xkb_use_extension(1, 0).unwrap().reply().unwrap();
        let (caps, none) = (xproto::ModMask::LOCK, xproto::ModMask::from(0u16));
        conn.xkb_latch_lock_state(CORE_KEYBOARD, caps, caps, true, xkb::Group::M2, none, false, 0).unwrap().check().unwrap();
        let x11 = X11::connect(Some(&xvfb.display)).unwrap();
        assert_eq!((x11.group, x11.locked), (1, 0x02));
        assert_eq!(x11.keymap.find(0x7a, x11.group, 0), Some((29, 0)));
        assert_eq!(x11.keymap.find(0x79, x11.group, 0), Some((52, 0)));
        assert_eq!(x11.keymap.find(0x7a, x11.group, x11.locked), Some((29, SHIFT)));
        // A key with one group is the same key in the second.
        assert_eq!(x11.keymap.find(0x40, x11.group, 0), Some((11, SHIFT)));
    }

    /// #EXP-1907: a character no key makes is bound to its spare keycode
    /// once a call and unbound at the call's end, not around every stroke
    /// (three rewrites of the server's map a character before).
    #[test]
    fn a_character_outside_the_layout_borrows_its_keycode_once_a_call() {
        let Some(xvfb) = Xvfb::start() else { return };
        let (conn, _, _) = xvfb.client();
        // The server's first change of its map tells twice: not counted.
        X11::connect(Some(&xvfb.display)).unwrap().type_char('\u{2603}').unwrap();
        let x11 = X11::connect(Some(&xvfb.display)).unwrap();
        mapping_changes(&conn);
        for ch in "\u{2603}\u{2603}\u{2603}".chars() {
            x11.type_char(ch).unwrap();
        }
        assert_eq!(mapping_changes(&conn), 1, "one binding for the three strokes");
        drop(x11);
        assert_eq!(mapping_changes(&conn), 1, "unbound at the call's end");
    }

    fn escapes() -> (Box<dyn Fn() + Send + Sync>, std::sync::mpsc::Receiver<()>) {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let tx = std::sync::Mutex::new(tx);
        let on_esc = move || {
            let _ = tx.lock().unwrap().send(());
        };
        (Box::new(on_esc), rx)
    }

    /// A device disabled and enabled under the lock (no removal: the id
    /// stays its own) is still held after the hierarchy events.
    #[test]
    fn a_device_disabled_and_enabled_under_the_lock_stays_grabbed() {
        let Some(xvfb) = Xvfb::start() else { return };
        let (on_esc, _escaped) = escapes();
        let lock = Lock::take(Some(xvfb.display.clone()), on_esc).unwrap();
        let (conn, root, keyboard) = xvfb.client();
        assert!(grabbed_elsewhere(&conn, root, keyboard), "the lock holds the keyboard");
        set_enabled(&conn, keyboard, false);
        set_enabled(&conn, keyboard, true);
        // Past the lock's own poll, so it has handled both events.
        std::thread::sleep(Duration::from_millis(100));
        assert!(grabbed_elsewhere(&conn, root, keyboard), "the keyboard came back outside the lock");
        drop(lock);
        assert!(!grabbed_elsewhere(&conn, root, keyboard), "the released lock holds nothing");
    }

    /// #EXP-1906 (DESKTOP-2I/2J/2K): the lock's X connection broke, so the
    /// server freed every grab: the host hears an abort, as on Esc.
    #[test]
    fn a_lock_whose_connection_broke_is_an_abort() {
        let Some(mut xvfb) = Xvfb::start() else { return };
        let (on_esc, escaped) = escapes();
        let lock = Lock::take(Some(xvfb.display.clone()), on_esc).unwrap();
        assert!(escaped.recv_timeout(Duration::from_millis(200)).is_err(), "nothing happened yet");
        xvfb.stop();
        escaped.recv_timeout(Duration::from_secs(5)).expect("the broken lock reaches on_esc");
        drop(lock);
    }

    /// Dropping the guard is the host's own release, never an abort.
    #[test]
    fn dropping_the_lock_is_not_an_abort() {
        let Some(xvfb) = Xvfb::start() else { return };
        let (on_esc, escaped) = escapes();
        drop(Lock::take(Some(xvfb.display.clone()), on_esc).unwrap());
        assert!(escaped.recv_timeout(Duration::from_millis(300)).is_err(), "on_esc fired on the guard's own drop");
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
        let keymap = KeyMap::core(8, 11, 2, &[0x61, 0x41, 0x31, 0x21, 0, 0, 0xffe1, 0]);
        assert_eq!(keymap.find(0x61, 0, 0), Some((8, 0)));
        assert_eq!(keymap.find(0x41, 0, 0), Some((8, SHIFT)));
        assert_eq!(keymap.find(0x21, 0, 0), Some((9, SHIFT)));
        assert_eq!(keymap.find(0xff0d, 0, 0), None);
        assert_eq!(keymap.spare(&[]), Some(10));
        assert_eq!(keymap.spare(&[10]), None);
    }

    /// #EXP-1907: a US and a German layout (groups 0 and 1) on keycodes 8..
    fn two_layouts() -> KeyMap {
        const LOCK: u8 = 1 << 1;
        let key = |groups: u8, types: [u8; 2], width: u8, syms: &[u32]| KeySyms {
            groups,
            clamp: false,
            types: [types[0], types[1], 0, 0],
            width,
            syms: syms.to_vec(),
        };
        let types = vec![
            Levels { mask: 0, map: vec![] },
            Levels { mask: SHIFT | LOCK, map: vec![(SHIFT, 1), (LOCK, 1)] },
            Levels { mask: SHIFT | LEVEL3, map: vec![(SHIFT, 1), (LEVEL3, 2), (SHIFT | LEVEL3, 3)] },
        ];
        let keys = vec![
            key(2, [1, 1], 2, &[0x79, 0x59, 0x7a, 0x5a]),                 // 8: y Y | z Z
            key(2, [1, 1], 2, &[0x7a, 0x5a, 0x79, 0x59]),                 // 9: z Z | y Y
            key(2, [1, 2], 4, &[0x71, 0x51, 0, 0, 0x71, 0x51, 0x40, 0x7d9]), // 10: q Q | q Q @ Ω
            key(1, [0, 0], 1, &[0xffe1]),                                 // 11: Shift
            key(1, [0, 0], 1, &[ISO_LEVEL3_SHIFT]),                       // 12: AltGr
            key(0, [0, 0], 0, &[]),                                       // 13: spare
        ];
        KeyMap { min: 8, max: 13, per: 4, keys, types }
    }

    #[test]
    fn a_keysym_is_found_in_the_layout_group_in_effect() {
        let keymap = two_layouts();
        assert_eq!(keymap.find(0x7a, 0, 0), Some((9, 0)));
        // With the German layout active the US `z` key makes `y`: ctrl+z
        // must not become ctrl+y.
        assert_eq!(keymap.find(0x7a, 1, 0), Some((8, 0)));
        assert_eq!(keymap.find(0x5a, 1, 0), Some((8, SHIFT)));
        // A key with one group is the same key in every group.
        assert_eq!(keymap.find(0xffe1, 1, 0), Some((11, 0)));
        assert_eq!(keymap.spare(&[]), Some(13));
    }

    #[test]
    fn altgr_levels_are_keys_of_the_layout() {
        let keymap = two_layouts();
        assert_eq!(keymap.find(0x40, 1, 0), Some((10, LEVEL3)));
        assert_eq!(keymap.find(0x7d9, 1, 0), Some((10, SHIFT | LEVEL3)));
        // The US group has no `@` here: a spare keycode's work.
        assert_eq!(keymap.find(0x40, 0, 0), None);
        // No AltGr key in the layout, no AltGr level.
        let mut keymap = keymap;
        keymap.keys[4] = KeySyms::default();
        assert_eq!(keymap.find(0x40, 1, 0), None);
    }

    #[test]
    fn caps_lock_is_countered_for_a_typed_character() {
        let keymap = two_layouts();
        let caps = 1 << 1;
        assert_eq!(keymap.find(0x7a, 0, caps), Some((9, SHIFT)));
        assert_eq!(keymap.find(0x5a, 0, caps), Some((9, 0)));
        assert_eq!(keymap.find(0x40, 1, caps), Some((10, LEVEL3)));
    }

    #[test]
    fn a_character_is_also_looked_up_by_its_legacy_keysym() {
        assert_eq!(char_keysyms('a'), [0x61]);
        assert_eq!(char_keysyms('\u{20ac}'), [0x0100_20ac, 0x20ac]);
        assert_eq!(char_keysyms('\u{444}'), [0x0100_0444, 0x6c6]);
        assert_eq!(char_keysyms('\u{2603}'), [0x0100_2603]);
    }
}
