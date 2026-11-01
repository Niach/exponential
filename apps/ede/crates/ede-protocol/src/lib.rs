//! `ede-protocol` v1: eDE's control plane (#NEW-104).
//!
//! One unix socket per compositor, `$XDG_RUNTIME_DIR/ede/<wayland-N>.sock`
//! in a 0700 directory. Frames are a little-endian `u32` length and then the
//! bytes: JSON for calls, replies and events, raw pixels for a capture (the
//! frame right after its reply). A connection starts with `hello` and a
//! token; every later call needs the eCore [`Capability`] its op names
//! ([`Request::capability`]). Failures are eCore's [`Failure`], events are
//! eCore's [`Envelope`].
//!
//! The compositor writes a root token (every capability) to
//! `$XDG_RUNTIME_DIR/ede/<wayland-N>.token` (0600) for the shell; whoever
//! holds `system` mints narrower, expiring grants for agents with
//! [`Request::MintGrant`]. A grant never mints more than it holds.

use std::{
    collections::VecDeque,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
};

pub use ecore::{Capability, Envelope, ErrorCode, Failure, Grant, RunId, SessionId, Token, WindowId};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
/// Largest frame either side accepts (an 8K capture fits).
pub const MAX_FRAME: usize = 160 << 20;
/// `source` of every event envelope.
pub const EVENT_SOURCE: &str = "ede-compositor";

/// Event kinds (`Envelope::kind`).
pub mod events {
    pub const WINDOW_OPENED: &str = "window.opened";
    pub const WINDOW_CLOSED: &str = "window.closed";
    pub const WINDOW_FOCUSED: &str = "window.focused";
    pub const SESSION_LOCKED: &str = "session.locked";
    pub const SESSION_UNLOCKED: &str = "session.unlocked";
    pub const CONFIG_RELOADED: &str = "config.reloaded";
}

/// The socket and token file of the compositor serving `wayland_display`.
pub fn socket_path(runtime_dir: &Path, wayland_display: &str) -> PathBuf {
    runtime_dir.join("ede").join(format!("{wayland_display}.sock"))
}

pub fn token_path(runtime_dir: &Path, wayland_display: &str) -> PathBuf {
    runtime_dir.join("ede").join(format!("{wayland_display}.token"))
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
    FocusWindow { window: WindowId },
    CloseWindow { window: WindowId },
    MoveWindow { window: WindowId, x: i32, y: i32 },
    /// Pixels of an output or a window; protected windows are black in an
    /// output capture and refused as a window capture, a locked session is
    /// black.
    Capture { target: CaptureTarget },
    /// Drive the person's seat. Protected surfaces and the lock refuse it.
    Input { event: InputEvent },
    ReloadConfig,
    Lock,
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
}

impl Request {
    /// The capability the op needs; `None` = any valid grant.
    pub fn capability(&self) -> Option<Capability> {
        match self {
            Request::Hello { .. } | Request::ListWindows | Request::ListOutputs | Request::Subscribe => None,
            Request::FocusWindow { .. } | Request::CloseWindow { .. } | Request::MoveWindow { .. } => Some(Capability::Ui),
            Request::Capture { .. } => Some(Capability::Capture),
            Request::Input { .. } => Some(Capability::InputPerson),
            Request::ReloadConfig | Request::Lock | Request::MintGrant { .. } | Request::RevokeGrant { .. } => Some(Capability::System),
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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
}

/// A capture reply; the pixels follow as the next frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaptureInfo {
    pub width: i32,
    pub height: i32,
    /// Bytes per row.
    pub stride: i32,
    /// `"argb8888"`: little-endian `0xAARRGGBB` words.
    pub format: String,
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
pub struct Client {
    stream: UnixStream,
    next_id: u64,
    events: VecDeque<Envelope>,
    pub hello: Hello,
}

impl Client {
    /// Connects to `socket` and says hello with `token`.
    pub fn connect(socket: &Path, token: Token) -> Result<Self, Error> {
        Self::over(UnixStream::connect(socket)?, token)
    }

    /// Hello over an already connected stream.
    pub fn over(stream: UnixStream, token: Token) -> Result<Self, Error> {
        let mut client = Self { stream, next_id: 1, events: VecDeque::new(), hello: Hello { version: 0, capabilities: vec![], session_id: SessionId::new("pending").unwrap() } };
        let hello = client.call(Request::Hello { version: PROTOCOL_VERSION, token })?;
        client.hello = serde_json::from_value(hello).map_err(|e| Error::Protocol(e.to_string()))?;
        Ok(client)
    }

    /// The compositor of `$WAYLAND_DISPLAY`, with the root token file.
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
        let info: CaptureInfo = self.call_as(Request::Capture { target })?;
        let pixels = read_frame(&mut self.stream)?;
        if pixels.len() != (info.stride * info.height) as usize {
            return Err(Error::Protocol("capture size mismatch".into()));
        }
        Ok(Capture { info, pixels })
    }

    pub fn input(&mut self, event: InputEvent) -> Result<(), Error> {
        self.call(Request::Input { event }).map(|_| ())
    }

    pub fn mint(&mut self, session_id: SessionId, capabilities: Vec<Capability>, expires_at: Option<i64>) -> Result<Grant, Error> {
        self.call_as(Request::MintGrant { session_id, run_id: None, capabilities, expires_at })
    }

    pub fn subscribe(&mut self) -> Result<(), Error> {
        self.call(Request::Subscribe).map(|_| ())
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
        assert_eq!(input.request, Request::Input { event: InputEvent::Key { key: 30, pressed: true } });
    }

    #[test]
    fn every_op_names_its_capability() {
        assert_eq!(Request::Capture { target: CaptureTarget::Output { name: "x".into() } }.capability(), Some(Capability::Capture));
        assert_eq!(Request::Input { event: InputEvent::PointerMotion { x: 0.0, y: 0.0 } }.capability(), Some(Capability::InputPerson));
        assert_eq!(Request::Lock.capability(), Some(Capability::System));
        assert_eq!(Request::ListWindows.capability(), None);
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
