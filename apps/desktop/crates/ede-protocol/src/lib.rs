//! `ede-protocol` v1: eDE's control plane (#NEW-104).
//!
//! One unix socket per compositor, `$XDG_RUNTIME_DIR/exponential-desktop/<wayland-N>.sock`
//! in a 0700 directory. Frames are a little-endian `u32` length and then the
//! bytes: JSON for calls, replies and events, raw pixels for a capture (the
//! frame right after its reply). A connection starts with `hello` and a
//! token; every later call needs the eCore [`Capability`] its op names
//! ([`Request::capability`]). Failures are eCore's [`Failure`], events are
//! eCore's [`Envelope`].
//!
//! The compositor writes a root token (every capability) to
//! `$XDG_RUNTIME_DIR/exponential-desktop/<wayland-N>.token` (0600) for the shell; whoever
//! holds `system` mints narrower, expiring grants for agents with
//! [`Request::MintGrant`]. A grant never mints more than it holds.
//!
//! The root token is the shell's (and the test harness's): everything else
//! comes in through [`Client::connect_as`], which mints a grant of its own
//! holding only what the caller names and drops the root connection
//! (#EDE-22: grants by default). A process handed a grant in `EDE_TOKEN`
//! (the sealed lock, a sandboxed agent) keeps that one.

use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::{collections::VecDeque, os::unix::net::UnixStream};

pub use ecore::{Capability, Envelope, ErrorCode, Failure, Grant, RunId, SessionId, Token, WindowId};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
/// Largest frame either side accepts (an 8K capture fits).
pub const MAX_FRAME: usize = 160 << 20;
/// `source` of every event envelope.
pub const EVENT_SOURCE: &str = "exponential-compositor";

/// Event kinds (`Envelope::kind`).
pub mod events {
    pub const WINDOW_OPENED: &str = "window.opened";
    pub const WINDOW_CLOSED: &str = "window.closed";
    pub const WINDOW_FOCUSED: &str = "window.focused";
    /// A window's title or app id changed.
    pub const WINDOW_CHANGED: &str = "window.changed";
    /// A window left the screen into the taskbar (#EDE-48); `window.restored` brings it back.
    pub const WINDOW_MINIMIZED: &str = "window.minimized";
    pub const WINDOW_RESTORED: &str = "window.restored";
    /// A key the shell answers (#EDE-12): `{"name": "switcher.next" | "menu.toggle" | "volume.up" | …}`.
    pub const HOTKEY: &str = "hotkey";
    pub const SESSION_LOCKED: &str = "session.locked";
    pub const SESSION_UNLOCKED: &str = "session.unlocked";
    pub const CONFIG_RELOADED: &str = "config.reloaded";
    /// `{"workspace": n}`: the active workspace changed (#EDE-18).
    pub const WORKSPACE_SWITCHED: &str = "workspace.switched";
    /// `{"output": name}`: an output appeared, moved, or changed mode or scale.
    pub const OUTPUT_CHANGED: &str = "output.changed";
    /// A bound global shortcut went down / up (data: [`super::ShortcutEvent`], #EDE-15).
    pub const SHORTCUT_ACTIVATED: &str = "shortcut.activated";
    pub const SHORTCUT_DEACTIVATED: &str = "shortcut.deactivated";
    /// The person pressed Esc during a [`super::Request::HoldInput`]: every
    /// hold is over (#EDE-23). Sent to the holders only, subscribed or not.
    pub const INPUT_ESCAPE: &str = "input.escape";
}

/// The socket and token file of the compositor serving `wayland_display`.
pub fn socket_path(runtime_dir: &Path, wayland_display: &str) -> PathBuf {
    runtime_dir.join("exponential-desktop").join(format!("{wayland_display}.sock"))
}

pub fn token_path(runtime_dir: &Path, wayland_display: &str) -> PathBuf {
    runtime_dir.join("exponential-desktop").join(format!("{wayland_display}.token"))
}

/// A call: `{"id": 1, "op": "list_windows", …}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Call {
    pub id: u64,
    #[serde(flatten)]
    pub request: Request,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// First call on every connection.
    Hello { version: u32, token: Token },
    ListWindows,
    ListOutputs,
    /// Raises and activates a window; a parked one brings its workspace
    /// along and a minimized one comes back first (#EDE-48).
    FocusWindow { window: WindowId },
    CloseWindow { window: WindowId },
    MoveWindow { window: WindowId, x: i32, y: i32 },
    /// Into the taskbar (#EDE-48): the window leaves the screen but keeps
    /// its geometry and workspace; `restore_window` or `focus_window`
    /// brings it back.
    MinimizeWindow { window: WindowId },
    RestoreWindow { window: WindowId },
    /// Pixels of an output or a window; protected windows are black in an
    /// output capture and refused as a window capture, a locked session is
    /// black. `since` = the `serial` of the last capture: when nothing was
    /// drawn since, the reply says `unchanged` and carries no pixels (a
    /// stream polls without a re-render per frame, #EDE-15).
    Capture {
        target: CaptureTarget,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        since: Option<u64>,
    },
    /// Drive the person's seat. Protected surfaces and the lock refuse it.
    /// `bindings` = the key goes through the compositor's bindings and
    /// hotkeys as a person's would (the root grant only: exponential-ctl in a test
    /// harness; an agent's keys never reach Alt+Tab or Super).
    Input {
        event: InputEvent,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        bindings: bool,
    },
    /// Where the pointer is (global logical coordinates): [`PointerInfo`].
    PointerPosition,
    /// Hold the person's own keyboard and pointer out while this connection
    /// lives (or until [`Request::ReleaseInput`]): nothing they do reaches a
    /// window, [`Request::Input`] still does (#EDE-23, ePilot's foreground
    /// lock). Their Esc ends every hold and arrives as
    /// [`events::INPUT_ESCAPE`]. The session lock screen always takes their
    /// input.
    HoldInput,
    ReleaseInput,
    ReloadConfig,
    Lock,
    /// End the session cleanly (the compositor exits 0; exponential-session logs out).
    Logout,
    /// The sealed lock client (#EDE-17) authenticated: open the session.
    /// Only the grant the compositor minted for that client is accepted.
    Unlock,
    /// The workspaces, the active one and each one's windows (#EDE-18).
    ListWorkspaces,
    SwitchWorkspace { index: usize },
    MoveToWorkspace { window: WindowId, index: usize },
    /// An output's mode (`"WxH@R"`), scale and position; each optional.
    SetOutput {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        position: Option<(i32, i32)>,
    },
    /// Start receiving events.
    Subscribe,
    /// A narrower grant for someone else (an agent): a subset of the
    /// caller's capabilities, expiring no later than the caller's grant.
    MintGrant {
        session_id: SessionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<RunId>,
        capabilities: Vec<Capability>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_at: Option<i64>,
    },
    RevokeGrant { token: Token },
    /// Global shortcuts for `owner` (the portal's session), replacing what
    /// it held (#EDE-15). Triggers follow the XDG shortcuts spec
    /// (`CTRL+SHIFT+a`, `LOGO+F5`); one that does not parse, takes a plain
    /// key (no CTRL, ALT or LOGO on a character key) or clashes with a
    /// compositor binding or another owner's shortcut comes back unbound.
    /// They never fire while the session is locked or a protected surface
    /// has the keyboard, and they go when the connection does.
    BindShortcuts { owner: String, shortcuts: Vec<ShortcutSpec> },
    UnbindShortcuts { owner: String },
    /// The seat's CLIPBOARD selection counter: [`ClipboardSerial`]. It
    /// moves on every change (a client's set or clear, Xwayland's, our own
    /// [`Request::ClipboardSet`]). ePilot's paste (`input:person`).
    ClipboardSerial,
    /// Every type the selection offers, read into memory now:
    /// [`ClipboardSnapshot`]. A type past [`CLIPBOARD_ITEM_MAX`] is skipped,
    /// reading stops past [`CLIPBOARD_TOTAL_MAX`], a source that does not
    /// answer within [`CLIPBOARD_READ_MS`] loses that type.
    ClipboardSnapshot,
    /// The compositor owns the selection itself and serves `items` to the
    /// focused client (and Xwayland's): [`ClipboardSerial`] of the new
    /// selection. Empty = clear it. `sensitive` = a secret: never logged,
    /// [`CLIPBOARD_SECRET_HINT`] offered alongside.
    ClipboardSet {
        items: Vec<ClipboardItem>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        sensitive: bool,
    },
    /// Did a client read a type of the selection set at `serial` (the
    /// password hint aside)? Never waits: [`ClipboardTaken`].
    ClipboardTaken { serial: u64 },
    /// The selection's UTF-8 text ([`CLIPBOARD_TEXT_MIMES`], the first one
    /// offered): [`ClipboardText`].
    ClipboardText,
}

/// The mime that marks a selection as a secret (KDE's, the one clipboard
/// managers honour); its data is `secret`.
pub const CLIPBOARD_SECRET_HINT: &str = "x-kde-passwordManagerHint";
/// The text types, best first (Wayland's, then X11's).
pub const CLIPBOARD_TEXT_MIMES: [&str; 5] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING", "TEXT", "STRING"];
/// A [`Request::ClipboardSnapshot`] skips a type larger than this.
pub const CLIPBOARD_ITEM_MAX: usize = 8 << 20;
/// … and stops reading past this many bytes in all.
pub const CLIPBOARD_TOTAL_MAX: usize = 32 << 20;
/// How long one type's read may take before it is dropped.
pub const CLIPBOARD_READ_MS: u64 = 500;

/// One type of a selection: its mime and its bytes (base64 on the wire).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardItem {
    pub mime: String,
    #[serde(with = "base64")]
    pub data: Vec<u8>,
}

/// Never prints the bytes: it may be a secret.
impl std::fmt::Debug for ClipboardItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ClipboardItem({:?}, {} bytes)", self.mime, self.data.len())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardSerial {
    pub serial: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipboardSnapshot {
    pub serial: u64,
    pub items: Vec<ClipboardItem>,
}

/// `serial` = the selection's serial now: another one than asked = someone
/// else took the clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardTaken {
    pub serial: u64,
    pub taken: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardText {
    pub serial: u64,
    pub text: Option<String>,
}

impl std::fmt::Debug for ClipboardText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ClipboardText({}, {:?} chars)", self.serial, self.text.as_ref().map(|t| t.chars().count()))
    }
}

/// Standard base64 (with padding) for [`ClipboardItem::data`].
pub mod base64 {
    use serde::{Deserialize, Deserializer, Serializer};

    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    pub fn decode(text: &str) -> Result<Vec<u8>, String> {
        let text = text.trim_end_matches('=').as_bytes();
        let mut out = Vec::with_capacity(text.len() * 3 / 4);
        let (mut acc, mut bits) = (0u32, 0u32);
        for &c in text {
            let v = match c {
                b'A'..=b'Z' => c - b'A',
                b'a'..=b'z' => c - b'a' + 26,
                b'0'..=b'9' => c - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => return Err(format!("not base64: {:?}", c as char)),
            };
            acc = (acc << 6) | u32::from(v);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
                acc &= (1 << bits) - 1;
            }
        }
        Ok(out)
    }

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        decode(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShortcutSpec {
    pub id: String,
    pub trigger: String,
}

/// A [`Request::BindShortcuts`] reply entry: `trigger` is the normalized
/// trigger, `None` = not bound.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoundShortcut {
    pub id: String,
    pub trigger: Option<String>,
}

/// `data` of the shortcut events.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShortcutEvent {
    pub owner: String,
    pub id: String,
    /// Milliseconds (the input event's clock).
    pub time: u32,
}

impl Request {
    /// The capability the op needs; `None` = any valid grant.
    pub fn capability(&self) -> Option<Capability> {
        match self {
            Request::Hello { .. } | Request::ListWindows | Request::ListOutputs | Request::ListWorkspaces | Request::Subscribe | Request::PointerPosition => None,
            Request::FocusWindow { .. } | Request::CloseWindow { .. } | Request::MoveWindow { .. } | Request::MinimizeWindow { .. } | Request::RestoreWindow { .. } | Request::SwitchWorkspace { .. } | Request::MoveToWorkspace { .. } | Request::BindShortcuts { .. } | Request::UnbindShortcuts { .. } => Some(Capability::Ui),
            Request::SetOutput { .. } => Some(Capability::System),
            Request::Capture { .. } => Some(Capability::Capture),
            Request::Input { .. } | Request::HoldInput | Request::ReleaseInput => Some(Capability::InputPerson),
            Request::ClipboardSerial | Request::ClipboardSnapshot | Request::ClipboardSet { .. } | Request::ClipboardTaken { .. } | Request::ClipboardText => {
                Some(Capability::InputPerson)
            }
            Request::ReloadConfig | Request::Lock | Request::Logout | Request::Unlock | Request::MintGrant { .. } | Request::RevokeGrant { .. } => Some(Capability::System),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CaptureTarget {
    Output { name: String },
    Window { window: WindowId },
}

/// Input in global logical coordinates; keys and buttons are evdev codes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputEvent {
    Key { key: u32, pressed: bool },
    PointerMotion { x: f64, y: f64 },
    PointerButton { button: u32, pressed: bool },
    /// A move by `dx`, `dy` logical px from where the pointer is.
    PointerMotionRelative { dx: f64, dy: f64 },
    /// Smooth scrolling in logical px.
    PointerAxis { dx: f64, dy: f64 },
    /// Wheel clicks (`v120` = 120 per click).
    PointerAxisDiscrete { dx: i32, dy: i32 },
    /// The key that makes `keysym` (an X keysym) on the active layout, with
    /// the Shift or AltGr its level needs held around it (press = modifiers
    /// then key, release = key then modifiers); `not_found` when no key
    /// makes it. ePilot's keys and typed characters (#EDE-23).
    Keysym { keysym: u32, pressed: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PointerInfo {
    pub x: f64,
    pub y: f64,
}

/// What the compositor sends.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Reply { id: u64, result: serde_json::Value },
    Error { id: u64, error: Failure },
    Event { event: Envelope },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub version: u32,
    pub capabilities: Vec<Capability>,
    pub session_id: SessionId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: WindowId,
    /// `None` for protected windows (a password manager's title can say
    /// too much).
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub focused: bool,
    pub fullscreen: bool,
    pub protected: bool,
    pub x11: bool,
    /// The workspace it lives on (0-based).
    #[serde(default)]
    pub workspace: usize,
    /// In the taskbar only (#EDE-48): unmapped, not focusable, its
    /// geometry kept for the restore.
    #[serde(default)]
    pub minimized: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub index: usize,
    pub active: bool,
    pub windows: Vec<WindowId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModeInfo {
    pub width: i32,
    pub height: i32,
    /// mHz.
    pub refresh: i32,
    pub preferred: bool,
    pub current: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputInfo {
    pub name: String,
    /// Logical geometry (the mode divided by the scale).
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
    /// The modes the output offers (#EDE-18).
    #[serde(default)]
    pub modes: Vec<ModeInfo>,
    #[serde(default)]
    pub make: String,
    #[serde(default)]
    pub model: String,
    /// Physical size in mm, `[0, 0]` when unknown.
    #[serde(default)]
    pub physical_mm: [i32; 2],
}

/// A capture reply; the pixels follow as the next frame unless `unchanged`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaptureInfo {
    pub width: i32,
    pub height: i32,
    /// Bytes per row.
    pub stride: i32,
    /// `"argb8888"`: little-endian `0xAARRGGBB` words.
    pub format: String,
    /// Counts the compositor's presented frames; pass it back as `since`.
    #[serde(default)]
    pub serial: u64,
    /// Nothing was drawn since `since`: no pixel frame follows.
    #[serde(default)]
    pub unchanged: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Capture {
    pub info: CaptureInfo,
    pub pixels: Vec<u8>,
}

impl Capture {
    pub fn at(&self, x: i32, y: i32) -> u32 {
        let i = (y * self.info.stride + x * 4) as usize;
        u32::from_le_bytes(self.pixels[i..i + 4].try_into().unwrap())
    }
}

pub fn write_frame(w: &mut impl Write, bytes: &[u8]) -> std::io::Result<()> {
    if bytes.len() > MAX_FRAME {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "frame too large"));
    }
    w.write_all(&(bytes.len() as u32).to_le_bytes())?;
    w.write_all(bytes)
}

pub fn read_frame(r: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut buf = vec![0; len];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

/// Splits whole frames off the front of `buf` (a non-blocking reader's
/// buffer). `Err` = a frame over [`MAX_FRAME`].
pub fn take_frames(buf: &mut Vec<u8>) -> Result<Vec<Vec<u8>>, ()> {
    let mut frames = Vec::new();
    let mut at = 0;
    while buf.len() - at >= 4 {
        let len = u32::from_le_bytes(buf[at..at + 4].try_into().unwrap()) as usize;
        if len > MAX_FRAME {
            return Err(());
        }
        if buf.len() - at - 4 < len {
            break;
        }
        frames.push(buf[at + 4..at + 4 + len].to_vec());
        at += 4 + len;
    }
    buf.drain(..at);
    Ok(frames)
}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// The compositor said no (or failed).
    Failed(Failure),
    Protocol(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Failed(e) => write!(f, "{e}"),
            Error::Protocol(e) => write!(f, "protocol: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// A blocking client. Events that arrive while waiting for a reply queue
/// up for [`Client::next_event`].
#[cfg(unix)]
pub struct Client {
    stream: UnixStream,
    next_id: u64,
    events: VecDeque<Envelope>,
    token: Token,
    pub hello: Hello,
}

#[cfg(unix)]
impl Client {
    /// Connects to `socket` and says hello with `token`.
    pub fn connect(socket: &Path, token: Token) -> Result<Self, Error> {
        Self::over(UnixStream::connect(socket)?, token)
    }

    /// Hello over an already connected stream.
    pub fn over(stream: UnixStream, token: Token) -> Result<Self, Error> {
        let mut client = Self { stream, next_id: 1, events: VecDeque::new(), token: token.clone(), hello: Hello { version: 0, capabilities: vec![], session_id: SessionId::new("pending").unwrap() } };
        let hello = client.call(Request::Hello { version: PROTOCOL_VERSION, token })?;
        client.hello = serde_json::from_value(hello).map_err(|e| Error::Protocol(e.to_string()))?;
        Ok(client)
    }

    /// The compositor of `$WAYLAND_DISPLAY` with `token` (a minted grant).
    pub fn connect_with_token(token: Token) -> Result<Self, Error> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| Error::Protocol("XDG_RUNTIME_DIR unset".into()))?;
        let display = std::env::var("WAYLAND_DISPLAY").map_err(|_| Error::Protocol("WAYLAND_DISPLAY unset".into()))?;
        Self::connect(&socket_path(Path::new(&runtime), &display), token)
    }

    /// The compositor of `$WAYLAND_DISPLAY` with the root token file (or
    /// `EDE_TOKEN`): the shell's and the harness's way in. Anything else
    /// uses [`Client::connect_as`].
    pub fn connect_env() -> Result<Self, Error> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| Error::Protocol("XDG_RUNTIME_DIR unset".into()))?;
        let display = std::env::var("WAYLAND_DISPLAY").map_err(|_| Error::Protocol("WAYLAND_DISPLAY unset".into()))?;
        let runtime = Path::new(&runtime);
        let token = match std::env::var("EDE_TOKEN") {
            Ok(t) => t,
            Err(_) => std::fs::read_to_string(token_path(runtime, &display))?,
        };
        let token = Token::new(token.trim()).map_err(|e| Error::Protocol(e.to_string()))?;
        Self::connect(&socket_path(runtime, &display), token)
    }

    /// A grant of this process's own for `session_id`, holding
    /// `capabilities` (#EDE-22: the default for everything but the shell).
    /// The root token (or `EDE_TOKEN`) mints it and that connection is
    /// dropped; a token that cannot mint (a narrower grant handed to this
    /// process in `EDE_TOKEN`) is kept as it is.
    pub fn mint_env(session_id: SessionId, capabilities: Vec<Capability>, expires_at: Option<i64>) -> Result<Token, Error> {
        let mut root = Self::connect_env()?;
        if !root.hello.capabilities.contains(&Capability::System) {
            return Ok(root.token.clone());
        }
        Ok(root.mint(session_id, capabilities, expires_at)?.token)
    }

    /// Connects with a grant minted for `session_id` ([`Client::mint_env`]).
    pub fn connect_as(session_id: SessionId, capabilities: Vec<Capability>, expires_at: Option<i64>) -> Result<Self, Error> {
        Self::connect_with_token(Self::mint_env(session_id, capabilities, expires_at)?)
    }

    /// The token this connection said hello with.
    pub fn token(&self) -> &Token {
        &self.token
    }

    pub fn call(&mut self, request: Request) -> Result<serde_json::Value, Error> {
        let id = self.next_id;
        self.next_id += 1;
        let bytes = serde_json::to_vec(&Call { id, request }).map_err(|e| Error::Protocol(e.to_string()))?;
        write_frame(&mut self.stream, &bytes)?;
        loop {
            match self.read_message()? {
                Message::Event { event } => self.events.push_back(event),
                Message::Reply { id: rid, result } if rid == id => return Ok(result),
                Message::Error { id: rid, error } if rid == id => return Err(Error::Failed(error)),
                // A call the compositor could not parse (an op it does not
                // know: an older compositor) comes back without its id.
                Message::Error { id: 0, error } => return Err(Error::Failed(error)),
                other => return Err(Error::Protocol(format!("unexpected {other:?}"))),
            }
        }
    }

    fn call_as<T: serde::de::DeserializeOwned>(&mut self, request: Request) -> Result<T, Error> {
        serde_json::from_value(self.call(request)?).map_err(|e| Error::Protocol(e.to_string()))
    }

    fn read_message(&mut self) -> Result<Message, Error> {
        let frame = read_frame(&mut self.stream)?;
        serde_json::from_slice(&frame).map_err(|e| Error::Protocol(e.to_string()))
    }

    pub fn windows(&mut self) -> Result<Vec<WindowInfo>, Error> {
        self.call_as(Request::ListWindows)
    }

    pub fn outputs(&mut self) -> Result<Vec<OutputInfo>, Error> {
        self.call_as(Request::ListOutputs)
    }

    pub fn capture(&mut self, target: CaptureTarget) -> Result<Capture, Error> {
        self.capture_since(target, None).map(|c| c.expect("a capture without `since` always carries pixels"))
    }

    /// `None` = nothing was drawn since `since` (no pixels came).
    pub fn capture_since(&mut self, target: CaptureTarget, since: Option<u64>) -> Result<Option<Capture>, Error> {
        let info: CaptureInfo = self.call_as(Request::Capture { target, since })?;
        if info.unchanged {
            return Ok(None);
        }
        let pixels = read_frame(&mut self.stream)?;
        if pixels.len() != (info.stride * info.height) as usize {
            return Err(Error::Protocol("capture size mismatch".into()));
        }
        Ok(Some(Capture { info, pixels }))
    }

    pub fn bind_shortcuts(&mut self, owner: &str, shortcuts: Vec<ShortcutSpec>) -> Result<Vec<BoundShortcut>, Error> {
        self.call_as(Request::BindShortcuts { owner: owner.into(), shortcuts })
    }

    pub fn input(&mut self, event: InputEvent) -> Result<(), Error> {
        self.call(Request::Input { event, bindings: false }).map(|_| ())
    }

    pub fn pointer(&mut self) -> Result<PointerInfo, Error> {
        self.call_as(Request::PointerPosition)
    }

    /// [`Request::HoldInput`]: the hold lasts until [`Client::release_input`]
    /// or this connection closes.
    pub fn hold_input(&mut self) -> Result<(), Error> {
        self.call(Request::HoldInput).map(|_| ())
    }

    pub fn release_input(&mut self) -> Result<(), Error> {
        self.call(Request::ReleaseInput).map(|_| ())
    }

    /// The socket itself, e.g. to [`UnixStream::shutdown`] a connection
    /// another thread blocks on in [`Client::next_event`].
    pub fn try_clone_stream(&self) -> std::io::Result<UnixStream> {
        self.stream.try_clone()
    }

    /// A person's key: through the bindings and hotkeys (root grant only).
    pub fn input_as_person(&mut self, event: InputEvent) -> Result<(), Error> {
        self.call(Request::Input { event, bindings: true }).map(|_| ())
    }

    pub fn mint(&mut self, session_id: SessionId, capabilities: Vec<Capability>, expires_at: Option<i64>) -> Result<Grant, Error> {
        self.call_as(Request::MintGrant { session_id, run_id: None, capabilities, expires_at })
    }

    pub fn subscribe(&mut self) -> Result<(), Error> {
        self.call(Request::Subscribe).map(|_| ())
    }

    pub fn focus(&mut self, window: WindowId) -> Result<(), Error> {
        self.call(Request::FocusWindow { window }).map(|_| ())
    }

    pub fn close_window(&mut self, window: WindowId) -> Result<(), Error> {
        self.call(Request::CloseWindow { window }).map(|_| ())
    }

    pub fn minimize(&mut self, window: WindowId) -> Result<(), Error> {
        self.call(Request::MinimizeWindow { window }).map(|_| ())
    }

    pub fn restore(&mut self, window: WindowId) -> Result<(), Error> {
        self.call(Request::RestoreWindow { window }).map(|_| ())
    }

    pub fn lock(&mut self) -> Result<(), Error> {
        self.call(Request::Lock).map(|_| ())
    }

    pub fn logout(&mut self) -> Result<(), Error> {
        self.call(Request::Logout).map(|_| ())
    }

    pub fn unlock(&mut self) -> Result<(), Error> {
        self.call(Request::Unlock).map(|_| ())
    }

    pub fn workspaces(&mut self) -> Result<Vec<WorkspaceInfo>, Error> {
        self.call_as(Request::ListWorkspaces)
    }

    pub fn switch_workspace(&mut self, index: usize) -> Result<(), Error> {
        self.call(Request::SwitchWorkspace { index }).map(|_| ())
    }

    pub fn move_to_workspace(&mut self, window: WindowId, index: usize) -> Result<(), Error> {
        self.call(Request::MoveToWorkspace { window, index }).map(|_| ())
    }

    pub fn clipboard_serial(&mut self) -> Result<u64, Error> {
        self.call_as::<ClipboardSerial>(Request::ClipboardSerial).map(|s| s.serial)
    }

    pub fn clipboard_snapshot(&mut self) -> Result<ClipboardSnapshot, Error> {
        self.call_as(Request::ClipboardSnapshot)
    }

    /// The serial of the selection it set.
    pub fn clipboard_set(&mut self, items: Vec<ClipboardItem>, sensitive: bool) -> Result<u64, Error> {
        self.call_as::<ClipboardSerial>(Request::ClipboardSet { items, sensitive }).map(|s| s.serial)
    }

    pub fn clipboard_taken(&mut self, serial: u64) -> Result<ClipboardTaken, Error> {
        self.call_as(Request::ClipboardTaken { serial })
    }

    pub fn clipboard_text(&mut self) -> Result<ClipboardText, Error> {
        self.call_as(Request::ClipboardText)
    }

    pub fn set_output(&mut self, name: &str, mode: Option<&str>, scale: Option<f64>, position: Option<(i32, i32)>) -> Result<(), Error> {
        self.call(Request::SetOutput { name: name.into(), mode: mode.map(String::from), scale, position }).map(|_| ())
    }

    /// The next event (blocks until one arrives).
    pub fn next_event(&mut self) -> Result<Envelope, Error> {
        if let Some(e) = self.events.pop_front() {
            return Ok(e);
        }
        loop {
            if let Message::Event { event } = self.read_message()? {
                return Ok(event);
            }
        }
    }

    pub fn set_read_timeout(&self, timeout: Option<std::time::Duration>) -> std::io::Result<()> {
        self.stream.set_read_timeout(timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_are_flat_json_with_an_op() {
        let call = Call { id: 3, request: Request::FocusWindow { window: WindowId::new("w1").unwrap() } };
        let json = serde_json::to_value(&call).unwrap();
        assert_eq!(json, serde_json::json!({"id": 3, "op": "focus_window", "window": "w1"}));
        assert_eq!(serde_json::from_value::<Call>(json).unwrap(), call);
        let input: Call = serde_json::from_str(r#"{"id":1,"op":"input","event":{"kind":"key","key":30,"pressed":true}}"#).unwrap();
        assert_eq!(input.request, Request::Input { event: InputEvent::Key { key: 30, pressed: true }, bindings: false });
        let sym = Call { id: 2, request: Request::Input { event: InputEvent::Keysym { keysym: 0x41, pressed: true }, bindings: false } };
        assert_eq!(serde_json::to_value(&sym).unwrap(), serde_json::json!({"id": 2, "op": "input", "event": {"kind": "keysym", "keysym": 65, "pressed": true}}));
        assert_eq!(serde_json::to_value(Call { id: 3, request: Request::PointerPosition }).unwrap(), serde_json::json!({"id": 3, "op": "pointer_position"}));
        assert_eq!(serde_json::to_value(Call { id: 4, request: Request::HoldInput }).unwrap(), serde_json::json!({"id": 4, "op": "hold_input"}));
    }

    #[test]
    fn every_op_names_its_capability() {
        assert_eq!(Request::Capture { target: CaptureTarget::Output { name: "x".into() }, since: None }.capability(), Some(Capability::Capture));
        assert_eq!(Request::BindShortcuts { owner: "p".into(), shortcuts: vec![] }.capability(), Some(Capability::Ui));
        assert_eq!(Request::Input { event: InputEvent::PointerMotion { x: 0.0, y: 0.0 }, bindings: false }.capability(), Some(Capability::InputPerson));
        assert_eq!(Request::Lock.capability(), Some(Capability::System));
        assert_eq!(Request::ListWindows.capability(), None);
        assert_eq!(Request::PointerPosition.capability(), None);
        assert_eq!(Request::HoldInput.capability(), Some(Capability::InputPerson));
        assert_eq!(Request::ReleaseInput.capability(), Some(Capability::InputPerson));
        assert_eq!(Request::Input { event: InputEvent::Keysym { keysym: 0x61, pressed: true }, bindings: false }.capability(), Some(Capability::InputPerson));
        assert_eq!(Request::MinimizeWindow { window: WindowId::new("w1").unwrap() }.capability(), Some(Capability::Ui));
        assert_eq!(Request::RestoreWindow { window: WindowId::new("w1").unwrap() }.capability(), Some(Capability::Ui));
        for clipboard in [Request::ClipboardSerial, Request::ClipboardSnapshot, Request::ClipboardSet { items: vec![], sensitive: false }, Request::ClipboardTaken { serial: 1 }, Request::ClipboardText] {
            assert_eq!(clipboard.capability(), Some(Capability::InputPerson));
        }
    }

    #[test]
    fn clipboard_ops_round_trip_with_base64_bytes() {
        let items = vec![ClipboardItem { mime: "text/plain".into(), data: b"hi\x00\xff".to_vec() }];
        let set = Call { id: 5, request: Request::ClipboardSet { items: items.clone(), sensitive: true } };
        let json = serde_json::to_value(&set).unwrap();
        assert_eq!(json, serde_json::json!({"id": 5, "op": "clipboard_set", "items": [{"mime": "text/plain", "data": "aGkA/w=="}], "sensitive": true}));
        assert_eq!(serde_json::from_value::<Call>(json).unwrap(), set);
        // `sensitive` defaults off and is left out when off.
        let plain: Call = serde_json::from_str(r#"{"id":1,"op":"clipboard_set","items":[]}"#).unwrap();
        assert_eq!(plain.request, Request::ClipboardSet { items: vec![], sensitive: false });
        assert_eq!(serde_json::to_value(&plain).unwrap(), serde_json::json!({"id": 1, "op": "clipboard_set", "items": []}));
        for (request, op) in [
            (Request::ClipboardSerial, "clipboard_serial"),
            (Request::ClipboardSnapshot, "clipboard_snapshot"),
            (Request::ClipboardText, "clipboard_text"),
            (Request::ClipboardTaken { serial: 7 }, "clipboard_taken"),
        ] {
            let call = Call { id: 2, request };
            let json = serde_json::to_value(&call).unwrap();
            assert_eq!(json["op"], op);
            assert_eq!(serde_json::from_value::<Call>(json).unwrap(), call);
        }
        let snapshot = ClipboardSnapshot { serial: 3, items };
        assert_eq!(serde_json::from_value::<ClipboardSnapshot>(serde_json::to_value(&snapshot).unwrap()).unwrap(), snapshot);
        let text: ClipboardText = serde_json::from_str(r#"{"serial":4,"text":null}"#).unwrap();
        assert_eq!(text.text, None);
        assert!(!format!("{:?}", ClipboardItem { mime: "a".into(), data: b"secret".to_vec() }).contains("secret"));
    }

    #[test]
    fn base64_round_trips_every_length() {
        let bytes: Vec<u8> = (0..=255u8).collect();
        for n in 0..40 {
            assert_eq!(base64::decode(&base64::encode(&bytes[..n])).unwrap(), &bytes[..n]);
        }
        assert_eq!(base64::encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64::encode(b"fo"), "Zm8=");
        assert!(base64::decode("a*b").is_err());
    }

    #[test]
    fn a_window_listed_by_an_older_compositor_is_not_minimized() {
        let old = r#"{"id":"w1","title":"a","app_id":null,"x":0,"y":0,"width":1,"height":1,"focused":false,"fullscreen":false,"protected":false,"x11":false}"#;
        let w: WindowInfo = serde_json::from_str(old).unwrap();
        assert!(!w.minimized && w.workspace == 0);
        let call = Call { id: 1, request: Request::MinimizeWindow { window: w.id } };
        assert_eq!(serde_json::to_value(&call).unwrap(), serde_json::json!({"id": 1, "op": "minimize_window", "window": "w1"}));
    }

    #[test]
    fn frames_split_across_reads() {
        let mut wire = Vec::new();
        write_frame(&mut wire, b"one").unwrap();
        write_frame(&mut wire, b"two!").unwrap();
        let mut buf = wire[..5].to_vec();
        assert_eq!(take_frames(&mut buf).unwrap(), Vec::<Vec<u8>>::new());
        buf.extend_from_slice(&wire[5..]);
        assert_eq!(take_frames(&mut buf).unwrap(), vec![b"one".to_vec(), b"two!".to_vec()]);
        assert!(buf.is_empty());
        let mut huge = ((MAX_FRAME + 1) as u32).to_le_bytes().to_vec();
        assert!(take_frames(&mut huge).is_err());
    }
}
