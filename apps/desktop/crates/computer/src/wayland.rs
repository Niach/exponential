//! The Wayland backend: no protocol lets one client see or drive the others,
//! so everything goes through the desktop's own D-Bus services (zbus, pure
//! Rust, no PipeWire):
//!
//! - pixels: the Screenshot portal (one full-desktop PNG per call);
//! - input: a RemoteDesktop portal session with the monitors selected as
//!   ScreenCast sources, so absolute pointer positions name a monitor stream.
//!   The person approves it ONCE; the restore token keeps the grant;
//! - windows and focus: AT-SPI frames (Wayland hides window positions, so a
//!   window is listed, focused and read, but not captured alone);
//! - the person's idle time: GNOME's Mutter IdleMonitor.
//!
//! An app that exposes no AT-SPI tree (most terminals) cannot be seen holding
//! the keyboard; typing still goes to it, the answer just names no app.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use zbus::blocking::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use crate::atspi::{Atspi, Frame as AtspiFrame};
use crate::backend::*;
use crate::linux::{char_keysym, key_keysym, modifier_keysym};

const DESKTOP: &str = "org.freedesktop.portal.Desktop";
const DESKTOP_PATH: &str = "/org/freedesktop/portal/desktop";
const REMOTE_DESKTOP: &str = "org.freedesktop.portal.RemoteDesktop";
/// How long a portal dialog may wait for the person.
const RESPONSE_WAIT: Duration = Duration::from_secs(120);
/// evdev button codes.
const BTN_LEFT: i32 = 0x110;
const BTN_RIGHT: i32 = 0x111;
const BTN_MIDDLE: i32 = 0x112;

pub fn is_wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty())
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind == "wayland")
}

type Results = HashMap<String, OwnedValue>;

fn dbus(err: zbus::Error) -> String {
    format!("The desktop portal request failed: {err}")
}

/// One monitor of the RemoteDesktop session, in the compositor's logical
/// coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Stream {
    node: u32,
    logical: Rect,
}

struct Session {
    conn: Connection,
    handle: OwnedObjectPath,
    streams: Vec<Stream>,
}

pub struct WaylandBackend {
    session: Mutex<Option<Session>>,
    /// Screen pixels (the screenshot's) per logical unit, from the last shot.
    scale: Mutex<f64>,
    pointer: Mutex<(f64, f64)>,
    tokens: AtomicU32,
}

fn restore_token_path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))?;
    Some(state.join("exponential").join("computer-use-restore-token"))
}

/// The portal's Request path for `token`, known before the call so the
/// Response cannot be missed.
fn request_path(conn: &Connection, token: &str) -> String {
    let sender = conn.unique_name().map(|name| name.as_str().to_string()).unwrap_or_default();
    format!(
        "{DESKTOP_PATH}/request/{}/{token}",
        sender.trim_start_matches(':').replace('.', "_")
    )
}

/// A portal call answered by a Request's Response signal.
fn portal_request<B>(
    conn: &Connection,
    token: &str,
    iface: &str,
    method: &str,
    body: &B,
) -> Result<Results, String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let path = request_path(conn, token);
    let request = zbus::blocking::Proxy::new(conn, DESKTOP, path, "org.freedesktop.portal.Request")
        .map_err(dbus)?;
    let mut responses = request.receive_signal("Response").map_err(dbus)?;
    conn.call_method(Some(DESKTOP), DESKTOP_PATH, Some(iface), method, body).map_err(dbus)?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(responses.next());
    });
    let message = receiver
        .recv_timeout(RESPONSE_WAIT)
        .map_err(|_| "The desktop did not answer (a permission dialog may be waiting).".to_string())?
        .ok_or("The desktop portal closed the request.")?;
    let (code, results): (u32, Results) = message.body().deserialize().map_err(dbus)?;
    match code {
        0 => Ok(results),
        1 => Err("The person declined the desktop's permission dialog.".to_string()),
        _ => Err("The desktop portal refused the request.".to_string()),
    }
}

fn string(results: &Results, key: &str) -> Option<String> {
    results.get(key).and_then(|value| String::try_from(value.try_clone().ok()?).ok())
}

/// `file:///a%20b.png` → `/a b.png`.
fn file_path(uri: &str) -> Option<PathBuf> {
    let encoded = uri.strip_prefix("file://")?;
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&encoded[index + 1..index + 3], 16) {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    Some(PathBuf::from(String::from_utf8(decoded).ok()?))
}

/// The streams' `(node, {position, size})` list.
fn parse_streams(results: &Results) -> Vec<Stream> {
    let Some(value) = results.get("streams").and_then(|value| value.try_clone().ok()) else {
        return Vec::new();
    };
    let Ok(streams) = <Vec<(u32, HashMap<String, OwnedValue>)>>::try_from(value) else {
        return Vec::new();
    };
    streams
        .into_iter()
        .filter_map(|(node, props)| {
            let pair = |key: &str| {
                props.get(key).and_then(|value| <(i32, i32)>::try_from(value.try_clone().ok()?).ok())
            };
            let (width, height) = pair("size")?;
            let (x, y) = pair("position").unwrap_or((0, 0));
            Some(Stream {
                node,
                logical: Rect {
                    x: f64::from(x),
                    y: f64::from(y),
                    width: f64::from(width),
                    height: f64::from(height),
                },
            })
        })
        .collect()
}

/// The streams' bounding box: the desktop the screenshot shows.
fn desktop_bounds(streams: &[Stream]) -> Option<Rect> {
    let first = streams.first()?.logical;
    let (mut left, mut top) = (first.x, first.y);
    let (mut right, mut bottom) = (first.x + first.width, first.y + first.height);
    for stream in streams {
        left = left.min(stream.logical.x);
        top = top.min(stream.logical.y);
        right = right.max(stream.logical.x + stream.logical.width);
        bottom = bottom.max(stream.logical.y + stream.logical.height);
    }
    Some(Rect { x: left, y: top, width: right - left, height: bottom - top })
}

/// A screen pixel (the screenshot's) as `(stream node, x, y)` in that
/// stream's logical space.
fn locate(streams: &[Stream], scale: f64, x: f64, y: f64) -> Option<(u32, f64, f64)> {
    let bounds = desktop_bounds(streams)?;
    let (lx, ly) = (bounds.x + x / scale, bounds.y + y / scale);
    streams
        .iter()
        .find(|stream| stream.logical.contains(lx, ly))
        .map(|stream| (stream.node, lx - stream.logical.x, ly - stream.logical.y))
}

/// A stable id for an AT-SPI frame.
fn frame_id(frame: &AtspiFrame) -> u32 {
    // FNV-1a over bus name + path; 0 is never an id.
    let mut hash: u32 = 0x811c_9dc5;
    for byte in frame.node.bus.bytes().chain(frame.node.path.as_str().bytes()) {
        hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
    }
    hash.max(1)
}

fn exe(pid: u32) -> String {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|comm| comm.trim().to_string())
        .unwrap_or_default()
}

impl WaylandBackend {
    pub fn connect() -> Result<Self, String> {
        // Fail early when the desktop has no portal at all.
        let conn = Connection::session()
            .map_err(|err| format!("Computer use needs the desktop's D-Bus session: {err}"))?;
        let version = conn
            .call_method(
                Some(DESKTOP),
                DESKTOP_PATH,
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &(REMOTE_DESKTOP, "version"),
            )
            .ok();
        if version.is_none() {
            return Err("This Wayland desktop has no remote-desktop portal \
                        (xdg-desktop-portal-gnome or -kde), so computer use cannot send input."
                .to_string());
        }
        Ok(Self {
            session: Mutex::new(None),
            scale: Mutex::new(1.0),
            pointer: Mutex::new((0.0, 0.0)),
            tokens: AtomicU32::new(0),
        })
    }

    fn token(&self) -> String {
        format!("exp{}_{}", std::process::id(), self.tokens.fetch_add(1, Ordering::Relaxed))
    }

    /// Start the RemoteDesktop session; shows the permission dialog unless a
    /// restore token from an earlier grant is accepted.
    fn start_session(&self) -> BackendResult<Session> {
        let conn = Connection::session().map_err(dbus)?;
        let token = self.token();
        let created = portal_request(
            &conn,
            &token,
            REMOTE_DESKTOP,
            "CreateSession",
            &HashMap::from([
                ("handle_token", Value::from(token.as_str())),
                ("session_handle_token", Value::from(self.token())),
            ]),
        )?;
        let handle = string(&created, "session_handle")
            .and_then(|path| OwnedObjectPath::try_from(path).ok())
            .ok_or("The desktop portal returned no session.")?;
        let session_path = ObjectPath::from(&handle);

        let token = self.token();
        let mut options = HashMap::from([
            ("handle_token", Value::from(token.as_str())),
            // Keyboard + pointer; persist until the person revokes it.
            ("types", Value::from(3u32)),
            ("persist_mode", Value::from(2u32)),
        ]);
        let restore = restore_token_path().and_then(|path| std::fs::read_to_string(path).ok());
        if let Some(restore) = restore.as_deref().map(str::trim).filter(|token| !token.is_empty()) {
            options.insert("restore_token", Value::from(restore.to_string()));
        }
        portal_request(&conn, &token, REMOTE_DESKTOP, "SelectDevices", &(&session_path, options))?;

        let token = self.token();
        portal_request(
            &conn,
            &token,
            "org.freedesktop.portal.ScreenCast",
            "SelectSources",
            &(
                &session_path,
                HashMap::from([
                    ("handle_token", Value::from(token.as_str())),
                    // Monitors, all of them: absolute positions name one.
                    ("types", Value::from(1u32)),
                    ("multiple", Value::from(true)),
                ]),
            ),
        )?;

        let token = self.token();
        let started = portal_request(
            &conn,
            &token,
            REMOTE_DESKTOP,
            "Start",
            &(&session_path, "", HashMap::from([("handle_token", Value::from(token.as_str()))])),
        )?;
        if let (Some(restore), Some(path)) = (string(&started, "restore_token"), restore_token_path()) {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, restore);
        }
        let streams = parse_streams(&started);
        if streams.is_empty() {
            return Err("The desktop shared no monitor, so pointer positions cannot be sent.".to_string());
        }
        Ok(Session { conn, handle, streams })
    }

    /// Run `act` on the live session, starting it first; a session the
    /// person closed meanwhile is restarted once.
    fn with_session<T>(&self, act: impl Fn(&Session) -> BackendResult<T>) -> BackendResult<T> {
        let mut session = self.session.lock().unwrap();
        if session.is_none() {
            *session = Some(self.start_session()?);
        }
        match act(session.as_ref().unwrap()) {
            Ok(value) => Ok(value),
            Err(_) => {
                *session = Some(self.start_session()?);
                act(session.as_ref().unwrap())
            }
        }
    }

    fn notify<B>(session: &Session, method: &str, body: &B) -> BackendResult<()>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        session
            .conn
            .call_method(Some(DESKTOP), DESKTOP_PATH, Some(REMOTE_DESKTOP), method, body)
            .map(|_| ())
            .map_err(dbus)
    }

    fn no_options() -> HashMap<&'static str, Value<'static>> {
        HashMap::new()
    }

    fn move_to(&self, session: &Session, x: f64, y: f64) -> BackendResult<()> {
        let scale = *self.scale.lock().unwrap();
        let (node, lx, ly) = locate(&session.streams, scale, x, y)
            .ok_or("That position is on no monitor.")?;
        Self::notify(
            session,
            "NotifyPointerMotionAbsolute",
            &(ObjectPath::from(&session.handle), Self::no_options(), node, lx, ly),
        )?;
        *self.pointer.lock().unwrap() = (x, y);
        settle();
        Ok(())
    }

    fn button(session: &Session, button: i32, pressed: bool) -> BackendResult<()> {
        Self::notify(
            session,
            "NotifyPointerButton",
            &(ObjectPath::from(&session.handle), Self::no_options(), button, u32::from(pressed)),
        )
    }

    fn keysym(session: &Session, keysym: u32, pressed: bool) -> BackendResult<()> {
        Self::notify(
            session,
            "NotifyKeyboardKeysym",
            &(ObjectPath::from(&session.handle), Self::no_options(), keysym as i32, u32::from(pressed)),
        )
    }

    fn tap(session: &Session, keysym: u32) -> BackendResult<()> {
        Self::keysym(session, keysym, true)?;
        settle();
        Self::keysym(session, keysym, false)?;
        settle();
        Ok(())
    }

    /// The full desktop as the Screenshot portal hands it over.
    fn shoot(&self) -> BackendResult<image::RgbaImage> {
        let conn = Connection::session().map_err(dbus)?;
        let token = self.token();
        let results = portal_request(
            &conn,
            &token,
            "org.freedesktop.portal.Screenshot",
            "Screenshot",
            &(
                "",
                HashMap::from([
                    ("handle_token", Value::from(token.as_str())),
                    ("interactive", Value::from(false)),
                ]),
            ),
        )?;
        let path = string(&results, "uri")
            .and_then(|uri| file_path(&uri))
            .ok_or("The desktop returned no screenshot.")?;
        let image = image::open(&path).map_err(|err| format!("Could not read the screenshot: {err}"));
        // The portal saves into the person's files; it was only ever ours.
        let _ = std::fs::remove_file(&path);
        Ok(image?.into_rgba8())
    }

    fn frames(&self) -> BackendResult<Vec<AtspiFrame>> {
        Ok(Atspi::connect()?.frames())
    }

    fn info(frame: &AtspiFrame, scale: f64) -> WindowInfo {
        WindowInfo {
            id: frame_id(frame),
            pid: frame.pid,
            app: frame.app.clone(),
            exe: exe(frame.pid),
            title: frame.title.clone(),
            // Wayland toolkits rarely know where their window is: a zero
            // rect lists the window but never hit-tests it.
            rect: frame
                .rect
                .filter(|rect| rect.x != 0.0 || rect.y != 0.0)
                .map(|rect| Rect {
                    x: rect.x * scale,
                    y: rect.y * scale,
                    width: rect.width * scale,
                    height: rect.height * scale,
                })
                .unwrap_or(Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 }),
            focused: frame.active,
            layer: 0,
        }
    }
}

fn settle() {
    std::thread::sleep(Duration::from_millis(15));
}

impl Backend for WaylandBackend {
    fn readiness(&self, prompt: bool) -> Readiness {
        if self.session.lock().unwrap().is_some() {
            return Readiness::Ready;
        }
        if !prompt {
            let granted = restore_token_path().is_some_and(|path| path.exists());
            return if granted {
                Readiness::Ready
            } else {
                Readiness::MissingPermission(
                    "Computer use on Wayland needs the desktop's remote-desktop permission; \
                     the first action asks the person for it."
                        .to_string(),
                )
            };
        }
        let mut session = self.session.lock().unwrap();
        match self.start_session() {
            Ok(started) => {
                *session = Some(started);
                Readiness::Ready
            }
            Err(reason) => Readiness::MissingPermission(reason),
        }
    }

    /// The remote-desktop dialog, then one screenshot for the screenshot
    /// permission (GNOME and Cinnamon ask once per app).
    fn prepare(&self) -> Readiness {
        match self.readiness(true) {
            Readiness::Ready => match self.shoot() {
                Ok(_) => Readiness::Ready,
                Err(reason) => Readiness::MissingPermission(reason),
            },
            other => other,
        }
    }

    fn capture(&self, target: Target) -> BackendResult<Frame> {
        let image = self.shoot()?;
        let whole = Rect { x: 0.0, y: 0.0, width: f64::from(image.width()), height: f64::from(image.height()) };
        let streams = self.session.lock().unwrap().as_ref().map(|session| session.streams.clone());
        let bounds = streams.as_deref().and_then(desktop_bounds);
        let scale = bounds.map_or(1.0, |bounds| whole.width / bounds.width);
        *self.scale.lock().unwrap() = scale;
        let area = match target {
            Target::Display(index) => match (streams.as_deref(), bounds) {
                (Some(streams), Some(bounds)) => {
                    let count = streams.len();
                    let stream = streams.get(index).ok_or_else(|| {
                        format!("There is no display {index}; this computer has {count}.")
                    })?;
                    Rect {
                        x: (stream.logical.x - bounds.x) * scale,
                        y: (stream.logical.y - bounds.y) * scale,
                        width: stream.logical.width * scale,
                        height: stream.logical.height * scale,
                    }
                }
                _ if index == 0 => whole,
                _ => return Err(format!("There is no display {index} yet; take a screenshot of display 0.")),
            },
            Target::Window(id) => self
                .windows()?
                .into_iter()
                .find(|window| window.id == id)
                .ok_or_else(|| format!("No window has id {id}; call list_windows."))?
                .rect,
        };
        if area.width < 1.0 || area.height < 1.0 {
            return Err("Wayland does not tell where that window is; screenshot its display instead.".to_string());
        }
        let cropped = image::imageops::crop_imm(
            &image,
            area.x.max(0.0) as u32,
            area.y.max(0.0) as u32,
            area.width as u32,
            area.height as u32,
        )
        .to_image();
        Ok(Frame { image: cropped, area })
    }

    fn windows(&self) -> BackendResult<Vec<WindowInfo>> {
        let scale = *self.scale.lock().unwrap();
        let mut frames = self.frames()?;
        // No stacking order on Wayland: the focused window first.
        frames.sort_by_key(|frame| !frame.active);
        Ok(frames.iter().map(|frame| Self::info(frame, scale)).collect())
    }

    fn focus_window(&self, id: u32) -> BackendResult<()> {
        let atspi = Atspi::connect()?;
        let frame = atspi
            .frames()
            .into_iter()
            .find(|frame| frame_id(frame) == id)
            .ok_or_else(|| format!("No window has id {id}; call list_windows."))?;
        atspi.grab_focus(&frame.node);
        std::thread::sleep(Duration::from_millis(250));
        let active = atspi.frames().into_iter().any(|frame| frame_id(&frame) == id && frame.active);
        if active {
            Ok(())
        } else {
            Err("Wayland does not let another app raise a window; click into it on a screenshot instead."
                .to_string())
        }
    }

    fn pointer(&self) -> BackendResult<(f64, f64)> {
        Ok(*self.pointer.lock().unwrap())
    }

    fn click(&self, x: f64, y: f64, button: Button, count: u8) -> BackendResult<()> {
        let code = match button {
            Button::Left => BTN_LEFT,
            Button::Right => BTN_RIGHT,
            Button::Middle => BTN_MIDDLE,
        };
        self.with_session(|session| {
            self.move_to(session, x, y)?;
            for _ in 0..count {
                Self::button(session, code, true)?;
                settle();
                Self::button(session, code, false)?;
                settle();
            }
            Ok(())
        })
    }

    fn scroll(&self, x: f64, y: f64, dx: i32, dy: i32) -> BackendResult<()> {
        self.with_session(|session| {
            self.move_to(session, x, y)?;
            let handle = ObjectPath::from(&session.handle);
            // Axis 0 = vertical, 1 = horizontal; positive = down / right.
            for (axis, steps) in [(0u32, dy), (1u32, dx)] {
                if steps != 0 {
                    Self::notify(session, "NotifyPointerAxisDiscrete", &(&handle, Self::no_options(), axis, steps))?;
                    settle();
                }
            }
            Ok(())
        })
    }

    fn type_text(&self, text: &str) -> BackendResult<()> {
        // Keysyms, not keycodes: the compositor finds the key and its level
        // in the person's own layout.
        self.with_session(|session| text.chars().try_for_each(|c| Self::tap(session, char_keysym(c))))
    }

    fn key(&self, chord: &Chord) -> BackendResult<()> {
        self.with_session(|session| {
            let modifiers: Vec<u32> = chord.modifiers.iter().map(|modifier| modifier_keysym(*modifier)).collect();
            let mut pressed = Ok(());
            for modifier in &modifiers {
                pressed = pressed.and_then(|()| Self::keysym(session, *modifier, true));
            }
            settle();
            let pressed = pressed.and_then(|()| Self::tap(session, key_keysym(chord.key)));
            for modifier in modifiers.iter().rev() {
                let _ = Self::keysym(session, *modifier, false);
            }
            pressed
        })
    }

    fn idle(&self) -> Option<Duration> {
        let conn = Connection::session().ok()?;
        let reply = conn
            .call_method(
                Some("org.gnome.Mutter.IdleMonitor"),
                "/org/gnome/Mutter/IdleMonitor/Core",
                Some("org.gnome.Mutter.IdleMonitor"),
                "GetIdletime",
                &(),
            )
            .ok()?;
        let millis: u64 = reply.body().deserialize().ok()?;
        Some(Duration::from_millis(millis))
    }

    fn read_ui(&self, window: Option<u32>) -> Option<BackendResult<Vec<UiNode>>> {
        let info = match window {
            Some(id) => self.windows().and_then(|windows| {
                windows
                    .into_iter()
                    .find(|info| info.id == id)
                    .ok_or_else(|| format!("No window has id {id}; call list_windows."))
            }),
            None => self
                .focused()
                .ok_or_else(|| "No window has focus; pass a window id from list_windows.".to_string()),
        };
        let window_rect = |info: &WindowInfo| (info.rect.width > 0.0).then_some(info.rect);
        Some(info.and_then(|info| crate::atspi::read_ui(info.pid, &info.app, &info.title, window_rect(&info))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(node: u32, x: f64, width: f64) -> Stream {
        Stream { node, logical: Rect { x, y: 0.0, width, height: 1440.0 } }
    }

    #[test]
    fn a_screenshot_pixel_lands_on_its_monitor_stream_in_logical_units() {
        // Two 2560-point monitors side by side, shot at 2x.
        let streams = [stream(40, 0.0, 2560.0), stream(41, 2560.0, 2560.0)];
        assert_eq!(locate(&streams, 2.0, 100.0, 200.0), Some((40, 50.0, 100.0)));
        assert_eq!(locate(&streams, 2.0, 5200.0, 200.0), Some((41, 40.0, 100.0)));
        assert_eq!(locate(&streams, 2.0, 99_999.0, 200.0), None);
    }

    /// Live: the Screenshot portal and the AT-SPI window list, which work in
    /// an X11 session too (`cargo test -p computer -- --ignored`).
    #[test]
    #[ignore]
    fn live_portal_screenshot_and_atspi_windows() {
        let backend = WaylandBackend {
            session: Mutex::new(None),
            scale: Mutex::new(1.0),
            pointer: Mutex::new((0.0, 0.0)),
            tokens: AtomicU32::new(0),
        };
        let image = backend.shoot().unwrap();
        assert!(image.width() > 100 && image.height() > 100);
        image.save("/tmp/exp-wayland-portal-shot.png").unwrap();
        let windows = backend.windows().unwrap();
        assert!(!windows.is_empty());
        for window in &windows {
            eprintln!("{} {} | {} | {:?} focused={}", window.id, window.app, window.title, window.rect, window.focused);
        }
    }

    #[test]
    fn portal_uris_decode_to_paths() {
        assert_eq!(
            file_path("file:///home/a/Bilder/Bildschirmfoto%20vom%202026.png"),
            Some(PathBuf::from("/home/a/Bilder/Bildschirmfoto vom 2026.png"))
        );
        assert_eq!(file_path("https://x"), None);
    }
}
