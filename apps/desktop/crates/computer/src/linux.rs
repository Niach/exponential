//! The Linux backend: X11 only, over x11rb's pure-Rust connection (no libxcb,
//! so the CLI builds without X headers). `GetImage` for pixels, XTest for
//! input, EWMH for the window list, RandR for the monitors. A Wayland session is refused up front:
//! XTest through XWayland would reach X clients only and capture nothing.

use std::fmt::Display;
use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::screensaver::ConnectionExt as _;
use x11rb::protocol::xproto::{
    self, Atom, AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, ImageFormat, Window,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

use crate::backend::*;

const NO_SYMBOL: u32 = 0;
const KEYSYM_RETURN: u32 = 0xff0d;
const KEYSYM_TAB: u32 = 0xff09;

pub struct X11Backend {
    conn: RustConnection,
    root: Window,
    width: u16,
    height: u16,
    min_keycode: u8,
    max_keycode: u8,
    /// The server's image byte order.
    lsb_first: bool,
    atoms: Atoms,
}

struct Atoms {
    client_list_stacking: Atom,
    client_list: Atom,
    active_window: Atom,
    wm_name: Atom,
    wm_pid: Atom,
    utf8_string: Atom,
}

fn x<T, E: Display>(result: Result<T, E>) -> BackendResult<T> {
    result.map_err(|err| format!("X11 request failed: {err}"))
}

/// Let the server and the client under the pointer process one event.
fn settle() {
    std::thread::sleep(Duration::from_millis(15));
}

impl X11Backend {
    pub fn connect() -> Result<Self, String> {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty())
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind == "wayland");
        if wayland {
            return Err("Computer use does not support Wayland sessions yet; this device needs \
                        an X11 session."
                .to_string());
        }
        if std::env::var_os("DISPLAY").is_none_or(|value| value.is_empty()) {
            return Err("Computer use needs a desktop session; this device has no display."
                .to_string());
        }
        let (conn, screen_index) = x11rb::connect(None)
            .map_err(|err| format!("Could not connect to the X server: {err}"))?;
        let setup = conn.setup();
        let screen = &setup.roots[screen_index];
        let (root, width, height) = (screen.root, screen.width_in_pixels, screen.height_in_pixels);
        let (min_keycode, max_keycode) = (setup.min_keycode, setup.max_keycode);
        let lsb_first = setup.image_byte_order == xproto::ImageOrder::LSB_FIRST;
        x(x(conn.xtest_get_version(2, 2))?.reply())
            .map_err(|_| "The X server has no XTEST extension, so input cannot be sent.".to_string())?;
        let atom = |name: &[u8]| -> BackendResult<Atom> {
            Ok(x(x(conn.intern_atom(false, name))?.reply())?.atom)
        };
        let atoms = Atoms {
            client_list_stacking: atom(b"_NET_CLIENT_LIST_STACKING")?,
            client_list: atom(b"_NET_CLIENT_LIST")?,
            active_window: atom(b"_NET_ACTIVE_WINDOW")?,
            wm_name: atom(b"_NET_WM_NAME")?,
            wm_pid: atom(b"_NET_WM_PID")?,
            utf8_string: atom(b"UTF8_STRING")?,
        };
        Ok(Self { conn, root, width, height, min_keycode, max_keycode, lsb_first, atoms })
    }

    /// The monitors on the one X screen, primary first. Without RandR 1.5
    /// (or with no monitor reported) the whole screen is the one display.
    fn monitors(&self) -> Vec<Rect> {
        let mut monitors: Vec<(bool, Rect)> = self
            .conn
            .randr_get_monitors(self.root, true)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| {
                reply
                    .monitors
                    .iter()
                    .map(|monitor| {
                        let rect = Rect {
                            x: f64::from(monitor.x),
                            y: f64::from(monitor.y),
                            width: f64::from(monitor.width),
                            height: f64::from(monitor.height),
                        };
                        (monitor.primary, rect)
                    })
                    .collect()
            })
            .unwrap_or_default();
        monitors.sort_by_key(|(primary, _)| !primary);
        if monitors.is_empty() {
            return vec![self.screen()];
        }
        monitors.into_iter().map(|(_, rect)| rect).collect()
    }

    fn screen(&self) -> Rect {
        Rect { x: 0.0, y: 0.0, width: f64::from(self.width), height: f64::from(self.height) }
    }

    fn property(&self, window: Window, property: Atom) -> Option<xproto::GetPropertyReply> {
        self.conn
            .get_property(false, window, property, AtomEnum::ANY, 0, u32::MAX)
            .ok()?
            .reply()
            .ok()
    }

    fn window_ids(&self, property: Atom) -> Vec<Window> {
        self.property(self.root, property)
            .and_then(|reply| reply.value32().map(Iterator::collect))
            .unwrap_or_default()
    }

    fn rect(&self, window: Window) -> Option<Rect> {
        let geometry = self.conn.get_geometry(window).ok()?.reply().ok()?;
        let origin = self.conn.translate_coordinates(window, self.root, 0, 0).ok()?.reply().ok()?;
        Some(Rect {
            x: f64::from(origin.dst_x),
            y: f64::from(origin.dst_y),
            width: f64::from(geometry.width),
            height: f64::from(geometry.height),
        })
    }

    fn info(&self, window: Window, active: Option<Window>) -> Option<WindowInfo> {
        let rect = self.rect(window)?;
        let text = |reply: xproto::GetPropertyReply| String::from_utf8_lossy(&reply.value).into_owned();
        let title = self
            .property(window, self.atoms.wm_name)
            .filter(|reply| reply.type_ == self.atoms.utf8_string && !reply.value.is_empty())
            .or_else(|| self.property(window, AtomEnum::WM_NAME.into()))
            .map(text)
            .unwrap_or_default();
        // WM_CLASS = `instance\0class\0`; the class is the app's name.
        let app = self
            .property(window, AtomEnum::WM_CLASS.into())
            .map(text)
            .and_then(|class| class.split('\0').filter(|part| !part.is_empty()).last().map(str::to_string))
            .unwrap_or_default();
        let pid = self
            .property(window, self.atoms.wm_pid)
            .and_then(|reply| reply.value32().and_then(|mut values| values.next()))
            .unwrap_or(0);
        let exe = std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .map(|comm| comm.trim().to_string())
            .unwrap_or_default();
        Some(WindowInfo {
            id: window,
            pid,
            app,
            exe,
            title,
            rect,
            focused: active == Some(window),
            layer: 0,
        })
    }

    fn fake(&self, kind: u8, detail: u8, x_pos: i16, y_pos: i16) -> BackendResult<()> {
        x(self.conn.xtest_fake_input(kind, detail, x11rb::CURRENT_TIME, self.root, x_pos, y_pos, 0))?;
        x(self.conn.flush())
    }

    fn move_to(&self, x_pos: f64, y_pos: f64) -> BackendResult<()> {
        self.fake(xproto::MOTION_NOTIFY_EVENT, 0, x_pos.round() as i16, y_pos.round() as i16)?;
        settle();
        Ok(())
    }

    fn button(&self, button: u8) -> BackendResult<()> {
        self.fake(xproto::BUTTON_PRESS_EVENT, button, 0, 0)?;
        settle();
        self.fake(xproto::BUTTON_RELEASE_EVENT, button, 0, 0)?;
        settle();
        Ok(())
    }

    fn mapping(&self) -> BackendResult<xproto::GetKeyboardMappingReply> {
        let count = self.max_keycode - self.min_keycode + 1;
        x(x(self.conn.get_keyboard_mapping(self.min_keycode, count))?.reply())
    }

    /// The keycode whose unshifted keysym is `keysym` in the current layout.
    fn keycode_for(&self, mapping: &xproto::GetKeyboardMappingReply, keysym: u32) -> Option<u8> {
        let per = usize::from(mapping.keysyms_per_keycode).max(1);
        mapping
            .keysyms
            .chunks(per)
            .position(|syms| syms.first() == Some(&keysym))
            .map(|index| self.min_keycode + index as u8)
    }

    /// A keycode the layout leaves unbound, to borrow for arbitrary keysyms.
    fn spare_keycode(&self, mapping: &xproto::GetKeyboardMappingReply) -> BackendResult<u8> {
        let per = usize::from(mapping.keysyms_per_keycode).max(1);
        mapping
            .keysyms
            .chunks(per)
            .rposition(|syms| syms.iter().all(|sym| *sym == NO_SYMBOL))
            .map(|index| self.min_keycode + index as u8)
            .ok_or_else(|| "The keyboard layout has no free keycode to type with.".to_string())
    }

    fn bind(&self, keycode: u8, keysym: u32) -> BackendResult<()> {
        x(self.conn.change_keyboard_mapping(1, keycode, 2, &[keysym, keysym]))?;
        // Round-trip so the mapping is in place before the key event.
        x(x(self.conn.get_input_focus())?.reply())?;
        settle();
        Ok(())
    }

    fn tap(&self, keycode: u8) -> BackendResult<()> {
        self.fake(xproto::KEY_PRESS_EVENT, keycode, 0, 0)?;
        settle();
        self.fake(xproto::KEY_RELEASE_EVENT, keycode, 0, 0)?;
        settle();
        Ok(())
    }
}

/// The keysym a character types as: Latin-1 maps to itself, everything else
/// is the Unicode keysym range.
fn char_keysym(c: char) -> u32 {
    match c {
        '\n' | '\r' => KEYSYM_RETURN,
        '\t' => KEYSYM_TAB,
        c if (0x20..=0x7e).contains(&(c as u32)) || (0xa0..=0xff).contains(&(c as u32)) => c as u32,
        c => 0x0100_0000 | c as u32,
    }
}

fn key_keysym(key: Key) -> u32 {
    match key {
        Key::Char(c) => char_keysym(c),
        Key::Enter => KEYSYM_RETURN,
        Key::Tab => KEYSYM_TAB,
        Key::Escape => 0xff1b,
        Key::Space => 0x20,
        Key::Backspace => 0xff08,
        Key::Delete => 0xffff,
        Key::Up => 0xff52,
        Key::Down => 0xff54,
        Key::Left => 0xff51,
        Key::Right => 0xff53,
        Key::Home => 0xff50,
        Key::End => 0xff57,
        Key::PageUp => 0xff55,
        Key::PageDown => 0xff56,
        Key::F(n) => 0xffbe + u32::from(n.clamp(1, 12)) - 1,
    }
}

fn modifier_keysym(modifier: Modifier) -> u32 {
    match modifier {
        Modifier::Shift => 0xffe1,
        Modifier::Control => 0xffe3,
        Modifier::Alt => 0xffe9,
        Modifier::Meta => 0xffeb,
    }
}

impl Backend for X11Backend {
    fn readiness(&self, _prompt: bool) -> Readiness {
        Readiness::Ready
    }

    fn capture(&self, target: Target) -> BackendResult<Frame> {
        let screen = self.screen();
        let area = match target {
            // One X screen spans every monitor; a display is its RandR part.
            Target::Display(index) => {
                let monitors = self.monitors();
                let count = monitors.len();
                monitors.into_iter().nth(index).ok_or_else(|| {
                    format!("There is no display {index}; this computer has {count}.")
                })?
            }
            Target::Window(id) => {
                let rect = self
                    .rect(id)
                    .ok_or_else(|| format!("No window has id {id}; call list_windows."))?;
                // The part of the window that is on screen.
                let left = rect.x.max(0.0);
                let top = rect.y.max(0.0);
                let right = (rect.x + rect.width).min(screen.width);
                let bottom = (rect.y + rect.height).min(screen.height);
                if right <= left || bottom <= top {
                    return Err("That window is off screen.".to_string());
                }
                Rect { x: left, y: top, width: right - left, height: bottom - top }
            }
        };
        let (width, height) = (area.width as u16, area.height as u16);
        let image = |drawable: Window, x_pos: f64, y_pos: f64| {
            x(x(self.conn.get_image(
                ImageFormat::Z_PIXMAP,
                drawable,
                x_pos as i16,
                y_pos as i16,
                width,
                height,
                u32::MAX,
            ))?
            .reply())
        };
        // A window reads its own pixels (under a compositor even where it is
        // covered); the screen region is the fallback.
        let reply = match target {
            Target::Window(id) => {
                let rect = self.rect(id).unwrap_or(area);
                image(id, area.x - rect.x, area.y - rect.y).or_else(|_| image(self.root, area.x, area.y))?
            }
            Target::Display(_) => image(self.root, area.x, area.y)?,
        };
        let pixels = usize::from(width) * usize::from(height);
        if reply.depth < 24 || reply.data.len() < pixels * 4 {
            return Err(format!("The X server returned a {}-bit image this build cannot read.", reply.depth));
        }
        // 24/32-bit ZPixmap is four bytes a pixel: B G R X in LSB-first
        // order, X R G B in MSB-first.
        let (r, g, b) = if self.lsb_first { (2, 1, 0) } else { (1, 2, 3) };
        let mut rgba = Vec::with_capacity(pixels * 4);
        for pixel in reply.data.chunks_exact(4).take(pixels) {
            rgba.extend_from_slice(&[pixel[r], pixel[g], pixel[b], 255]);
        }
        let image = image::RgbaImage::from_raw(u32::from(width), u32::from(height), rgba)
            .ok_or("The captured image has an unexpected size.")?;
        Ok(Frame { image, area })
    }

    fn windows(&self) -> BackendResult<Vec<WindowInfo>> {
        // Stacking order is bottom to top; the tools list front to back.
        let mut ids = self.window_ids(self.atoms.client_list_stacking);
        if ids.is_empty() {
            ids = self.window_ids(self.atoms.client_list);
        }
        ids.reverse();
        let active = self.window_ids(self.atoms.active_window).first().copied().filter(|id| *id != 0);
        Ok(ids.into_iter().filter_map(|id| self.info(id, active)).collect())
    }

    fn focus_window(&self, id: u32) -> BackendResult<()> {
        // Source indication 2 = a pager: the window manager honors it.
        let event = ClientMessageEvent::new(32, id, self.atoms.active_window, [2u32, 0, 0, 0, 0]);
        x(self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        ))?;
        x(self.conn.flush())?;
        std::thread::sleep(Duration::from_millis(250));
        Ok(())
    }

    fn pointer(&self) -> BackendResult<(f64, f64)> {
        let reply = x(x(self.conn.query_pointer(self.root))?.reply())?;
        Ok((f64::from(reply.root_x), f64::from(reply.root_y)))
    }

    fn click(&self, x_pos: f64, y_pos: f64, button: Button, count: u8) -> BackendResult<()> {
        self.move_to(x_pos, y_pos)?;
        let button = match button {
            Button::Left => 1,
            Button::Middle => 2,
            Button::Right => 3,
        };
        for _ in 0..count {
            self.button(button)?;
        }
        Ok(())
    }

    fn scroll(&self, x_pos: f64, y_pos: f64, dx: i32, dy: i32) -> BackendResult<()> {
        self.move_to(x_pos, y_pos)?;
        // The wheel is buttons 4 (up), 5 (down), 6 (left), 7 (right).
        for _ in 0..dy.unsigned_abs() {
            self.button(if dy > 0 { 5 } else { 4 })?;
        }
        for _ in 0..dx.unsigned_abs() {
            self.button(if dx > 0 { 7 } else { 6 })?;
        }
        Ok(())
    }

    fn type_text(&self, text: &str) -> BackendResult<()> {
        // Layout-independent: borrow a free keycode, bind it to each
        // character's keysym in turn, and give it back.
        let spare = self.spare_keycode(&self.mapping()?)?;
        let typed = text.chars().try_for_each(|c| {
            self.bind(spare, char_keysym(c))?;
            self.tap(spare)
        });
        let _ = self.bind(spare, NO_SYMBOL);
        typed
    }

    fn key(&self, chord: &Chord) -> BackendResult<()> {
        let mapping = self.mapping()?;
        let modifiers: Vec<u8> = chord
            .modifiers
            .iter()
            .map(|modifier| {
                self.keycode_for(&mapping, modifier_keysym(*modifier))
                    .ok_or_else(|| format!("The keyboard layout has no {modifier:?} key."))
            })
            .collect::<BackendResult<_>>()?;
        let keysym = key_keysym(chord.key);
        // A shortcut must hit the layout's real key; anything else borrows.
        let (keycode, borrowed) = match self.keycode_for(&mapping, keysym) {
            Some(keycode) => (keycode, false),
            None => {
                let spare = self.spare_keycode(&mapping)?;
                self.bind(spare, keysym)?;
                (spare, true)
            }
        };
        let mut pressed = Ok(());
        for modifier in &modifiers {
            pressed = pressed.and_then(|()| self.fake(xproto::KEY_PRESS_EVENT, *modifier, 0, 0));
        }
        settle();
        let pressed = pressed.and_then(|()| self.tap(keycode));
        for modifier in modifiers.iter().rev() {
            let _ = self.fake(xproto::KEY_RELEASE_EVENT, *modifier, 0, 0);
        }
        if borrowed {
            let _ = self.bind(keycode, NO_SYMBOL);
        }
        pressed
    }

    fn idle(&self) -> Option<Duration> {
        let reply = self.conn.screensaver_query_info(self.root).ok()?.reply().ok()?;
        Some(Duration::from_millis(u64::from(reply.ms_since_user_input)))
    }

    fn read_ui(&self, window: Option<u32>) -> Option<BackendResult<Vec<UiNode>>> {
        let info = match window {
            Some(id) => self.windows().and_then(|windows| {
                windows
                    .into_iter()
                    .find(|info| info.id == id)
                    .ok_or_else(|| format!("No window has id {id}; call list_windows."))
            }),
            None => self.focused().ok_or_else(|| {
                "No window has focus; pass a window id from list_windows.".to_string()
            }),
        };
        Some(info.and_then(|info| crate::atspi::read_ui(info.pid, &info.app, &info.title, Some(info.rect))))
    }
}
