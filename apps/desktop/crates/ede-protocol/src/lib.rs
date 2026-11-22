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
//!
//! #EDE-172, channels: a grant can live on connections instead of in a
//! string. A connection the compositor made (inherited at spawn,
//! [`Client::inherited`], or a test's `--control-fd`) already holds its
//! grant and says hello without a token. [`Request::OpenChannel`] answers
//! with one more such connection, a descriptor next to its reply frame
//! ([`Client::open_channel`]): on the caller's own grant, or on a child of
//! it no wider and no longer-lived. A child is bound: its token is never
//! taken at hello (the wire [`Grant`] keeps the field; for a bound grant it
//! is the grant's id, which [`Request::RevokeGrant`] takes), and revoking
//! it, or the end of its parent, ends its connections. Until eDE's flip
//! the token file stays and every reader falls back to it; an older
//! compositor answers `open_channel` with [`Error::Unsupported`].

use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::{
    collections::VecDeque,
    os::{fd::OwnedFd, unix::net::UnixStream},
};

pub use ecore::{Capability, Envelope, ErrorCode, Failure, Grant, RunId, SessionId, SpaceId, Token, WindowId};
use serde::{Deserialize, Deserializer, Serialize};

#[cfg(target_os = "linux")]
pub mod intake;
pub mod process;

/// The hello's version, which both sides must speak. An additive change (an
/// op, a field, a capability, an error code, a message type, a membership)
/// never bumps it: each side reads what it does not know (#EXP-1994), and
/// [`Hello::features`] says what a compositor does beyond v1.
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

/// #EDE-172: the descriptor number of the control connection a process was
/// started with ([`Client::inherited`]); the compositor sets it for what it
/// spawns.
pub const CONTROL_FD_ENV: &str = "EDE_CONTROL_FD";

/// Where the compositor serving `wayland_display` keeps its agent spaces'
/// directories, each `space-n` (#EDE-125): a launch into a space has its
/// directory as `XDG_RUNTIME_DIR` ([`process::space_in`]).
pub fn spaces_path(runtime_dir: &Path, wayland_display: &str) -> PathBuf {
    runtime_dir.join("exponential-desktop").join(format!("{wayland_display}.spaces"))
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
    /// First call on every connection. `token` = the grant to act under;
    /// none on a connection that already holds its grant (#EDE-172: one the
    /// compositor made, [`Request::OpenChannel`]'s), whose reply also names
    /// that grant (`grant`, its token or a bound grant's id). A bound
    /// grant's token is refused here.
    Hello {
        version: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token: Option<Token>,
    },
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
    /// Ends a grant: by its token, or a bound one (#EDE-172) by its id, the
    /// `token` of the reply that opened it (also read as `grant`). A bound
    /// grant's connections end with it, and so do its children's. `system`
    /// ends any grant but the root; any grant ends itself and what was
    /// opened on it.
    RevokeGrant {
        #[serde(alias = "grant")]
        token: Token,
    },
    /// #EDE-172: one more connection holding a grant, as a descriptor
    /// (`SCM_RIGHTS`) that comes with the reply frame; the reply is that
    /// connection's [`Grant`] (for a child: bound, its `token` its id).
    /// `session_id` none = the caller's own grant as it is (`capabilities`
    /// exactly what it holds, no `expires_at`); named = a child of it,
    /// holding no more than it and expiring no later. Any valid grant asks;
    /// never a space grant, and no child holds `space` (a space grant is
    /// minted: [`Request::MintGrant`]).
    OpenChannel {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<SessionId>,
        capabilities: Vec<Capability>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_at: Option<i64>,
    },
    /// #EDE-172 (P4): a Wayland connection the compositor made, as a
    /// descriptor (`SCM_RIGHTS`) that comes with the reply frame (`null`):
    /// for the `WAYLAND_SOCKET` of the shell the caller starts next. Only
    /// on the channel the compositor handed the shell's supervisor at its
    /// start (`[session] helpers = "compositor"`): the compositor knows the
    /// client it made by construction, never by its executable.
    OpenWayland,
    /// #EDE-172 (P13): a connection to the session's services daemon, as a
    /// descriptor (`SCM_RIGHTS`) that comes with the reply frame (`null`):
    /// the compositor hands the daemon's end over its intake (`intake`)
    /// with `capabilities`, which the daemon's `call` holds.
    /// No more than the asking grant holds, never for the `computer-use`
    /// principal or a space. [`ErrorCode::Unavailable`] = the compositor
    /// runs no services daemon (its units do: `services.sock`).
    OpenServices { capabilities: Vec<Capability> },
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
    /// answer within [`CLIPBOARD_READ_MS`] loses that type. Every type left
    /// out is named in [`ClipboardSnapshot::unread`].
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
    /// A one-shot token for a launch the person made (the shell's menu,
    /// taskbar and hotkeys; #EDE-76): [`LaunchToken`]. The shell's root
    /// grant only. In the launched process's `XDG_ACTIVATION_TOKEN` and
    /// `DESKTOP_STARTUP_ID` it lets that app's window take the keyboard,
    /// for [`LAUNCH_TOKEN_SECS`]; any other new window opens without it.
    LaunchToken,
    /// An agent space for the caller's session (#EDE-126): a desk of its
    /// own with one output of `width` x `height`, a seat and a Wayland
    /// socket: [`SpaceInfo`]. A session has one space: a second call
    /// answers the same one. A space grant only ([`Capability::Space`]
    /// and nothing else).
    CreateSpace { width: i32, height: i32 },
    /// Closes a space: its clients and what was launched into it end.
    DestroySpace { space: SpaceId },
    /// The caller's space, or every space for `system`: `Vec<SpaceInfo>`.
    ListSpaces,
    /// Starts `argv` inside the caller's space: [`Launched`]. The
    /// compositor decides the environment (the space's socket and runtime
    /// directory, never the person's displays or session bus); `env` goes
    /// on top, without the names the compositor sets.
    Launch {
        argv: Vec<String>,
        #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        env: std::collections::BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    /// The agent space process `pid` belongs to (#EDE-127), and whether it
    /// is the person's session's (#EDE-143): a [`ProcessAnswer`]. eDE's
    /// polkit agent asks before it shows the person a prompt: it refuses a
    /// request from a space, and shows one from outside the session
    /// without the keyboard. Any grant (#EDE-172; `system` before).
    SpaceOfProcess { pid: u32 },
    /// Whether this desktop gives a person's runs an agent space now
    /// (#EXP-1514): [`SpaceReadiness`]. Any valid person-scope grant (as
    /// `list_windows`); never a space grant's. Asked only of a compositor
    /// whose hello says [`FEATURE_SPACE_READINESS`]; an older one cannot
    /// read it ([`Error::Unsupported`]).
    SpaceReadiness,
    /// The caller's space counts as busy for [`SPACE_BUSY_SECS`] (#EXP-1514,
    /// D 10): its session's turn runs, so a full desktop does not close it
    /// for a new space. A space grant only, its own space only.
    SpaceBusy,
}

/// How long one [`Request::SpaceBusy`] keeps a space from giving way.
pub const SPACE_BUSY_SECS: u64 = 90;

/// In [`Hello::features`]: this compositor makes agent spaces.
pub const FEATURE_SPACES: &str = "spaces";

/// In [`Hello::features`]: this compositor answers
/// [`Request::SpaceReadiness`] (#EXP-1514). `spaces` alone (eDE 0.2.29)
/// does not offer a space to anyone: a client makes one only after a
/// `ready` answer.
pub const FEATURE_SPACE_READINESS: &str = "space-readiness";

/// A [`Request::SpaceReadiness`] reply.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpaceReadiness {
    pub ready: bool,
    /// Why not, when not ready: `no_user_manager`, `guard` (the space
    /// guard or polkit, #EDE-146..148), `session`, `driver`, or a block
    /// this build does not know (still a no).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    /// How many spaces this desktop holds at most: a refused
    /// [`Request::CreateSpace`] (`limit`) with that many spaces open is a
    /// full desktop, with fewer it is out of memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u32>,
}

/// #EXP-2024: the layer-shell namespace of the shell's pinned view of an
/// agent space (`space-view.json`): black in every capture of the person's
/// output, so a space's pixels never reach a screenshot, screen memory or a
/// screen share through it.
pub const SPACE_VIEW_NAMESPACE: &str = "exponential-shell:space-view";

/// #EXP-2041: the namespace of the taskbar's Agent spaces list, a hosted
/// Popover (an `xdg_popup` of the taskbar layer) that shows each space's
/// picture: the shell names the popup with it through
/// `protocols/exponential-popup-namespace-v1.xml`, and the compositor draws
/// it black in every capture of the person's output, as it draws the view.
pub const SPACE_LIST_NAMESPACE: &str = "exponential-shell:taskbar:agent_spaces";

/// An agent space ([`Request::CreateSpace`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpaceInfo {
    pub id: SpaceId,
    /// The session that owns it (its grant's).
    pub session_id: SessionId,
    /// Its one output's name, for [`CaptureTarget::Output`].
    pub output: String,
    pub width: i32,
    pub height: i32,
    /// The absolute path of its Wayland socket (a `WAYLAND_DISPLAY`).
    pub wayland_display: String,
}

/// What ties a process to an agent space ([`ProcessSpace::by`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Membership {
    /// It runs in a scope of the space's own systemd slice (#EDE-138): the
    /// first sign looked at, and the one a process keeps when it leaves
    /// its group, its environment and its parent.
    Cgroup,
    /// It was launched into the space ([`Request::Launch`]).
    Launch,
    /// It is in the process group of a launch (also after its parent died).
    Group,
    /// It is connected to the space's socket.
    Client,
    /// Its environment names the space's directory (`XDG_RUNTIME_DIR` or
    /// `WAYLAND_DISPLAY`), as every launch's does.
    Environment,
    /// A tie this build does not know (a newer compositor's, #EXP-1994):
    /// still the space's.
    #[serde(other)]
    Unknown,
}

/// The asked process, or the process `pid` it descends from, is tied to
/// `space` `by` that ([`ProcessAnswer::space`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessSpace {
    pub space: SpaceId,
    pub by: Membership,
    pub pid: u32,
}

/// A [`Request::SpaceOfProcess`] reply (#EDE-127, #EDE-143): the space the
/// process belongs to, if any, and what the polkit agent needs to tell the
/// person's own processes from everything else of their user.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessAnswer {
    pub space: Option<ProcessSpace>,
    /// It runs in the compositor's own cgroup (the person's login session
    /// scope): a process no space can get back into.
    #[serde(default)]
    pub session: bool,
    /// A space of this compositor has launches alive outside systemd
    /// scopes (no user manager): such a process can sit in the session's
    /// cgroup too, so `session` vouches for no one meanwhile.
    #[serde(default)]
    pub unscoped_launches: bool,
    /// No agent space of this compositor exists: none open, no closed one's
    /// slice still stopping, no launch alive. The polkit agent then gives
    /// every prompt the keyboard, as before #EDE-143. `false` from a
    /// compositor that does not say.
    #[serde(default)]
    pub no_space: bool,
}

impl ProcessAnswer {
    /// The reply as it comes: this answer, or an older compositor's (`null`
    /// for the person's, a bare [`ProcessSpace`]), which says nothing of
    /// the session.
    pub fn from_wire(value: serde_json::Value) -> Result<Self, serde_json::Error> {
        match value {
            serde_json::Value::Null => Ok(Self::default()),
            serde_json::Value::Object(ref fields) if fields.contains_key("by") => Ok(Self { space: Some(serde_json::from_value(value)?), ..Self::default() }),
            other => serde_json::from_value(other),
        }
    }
}

/// A [`Request::Launch`] reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Launched {
    pub pid: u32,
}

/// How long a [`Request::LaunchToken`] waits for its window.
pub const LAUNCH_TOKEN_SECS: u64 = 120;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchToken {
    pub token: String,
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
    /// The types the selection offers that are NOT in `items` (#EXP-1900):
    /// too large, past the total, or their read failed or timed out.
    /// `Some(empty)` = `items` is the whole selection; `None` = a
    /// compositor from before the field, which dropped types without
    /// saying so: the snapshot may be partial.
    #[serde(default)]
    pub unread: Option<Vec<String>>,
}

impl ClipboardSnapshot {
    /// `items` is everything the selection held.
    pub fn is_whole(&self) -> bool {
        self.unread.as_ref().is_some_and(Vec::is_empty)
    }
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
            Request::Hello { .. } | Request::ListWindows | Request::ListOutputs | Request::ListWorkspaces | Request::Subscribe | Request::PointerPosition | Request::SpaceReadiness => None,
            // #EDE-172: what the channel holds is checked against the grant.
            Request::OpenChannel { .. } => None,
            // #EDE-172 (P4): the supervisor's own channel only (the arm checks).
            Request::OpenWayland => None,
            // #EDE-172 (P13): what the connection holds, the arm checks.
            Request::OpenServices { .. } => None,
            Request::FocusWindow { .. } | Request::CloseWindow { .. } | Request::MoveWindow { .. } | Request::MinimizeWindow { .. } | Request::RestoreWindow { .. } | Request::SwitchWorkspace { .. } | Request::MoveToWorkspace { .. } | Request::BindShortcuts { .. } | Request::UnbindShortcuts { .. } => Some(Capability::Ui),
            Request::SetOutput { .. } => Some(Capability::System),
            Request::Capture { .. } => Some(Capability::Capture),
            Request::Input { .. } | Request::HoldInput | Request::ReleaseInput => Some(Capability::InputPerson),
            Request::ClipboardSerial | Request::ClipboardSnapshot | Request::ClipboardSet { .. } | Request::ClipboardTaken { .. } | Request::ClipboardText => {
                Some(Capability::InputPerson)
            }
            Request::ReloadConfig | Request::Lock | Request::Logout | Request::Unlock | Request::MintGrant { .. } | Request::LaunchToken => Some(Capability::System),
            // #EDE-172: `system` ends any grant, a holder its own lineage (checked in the arm).
            Request::RevokeGrant { .. } => None,
            Request::CreateSpace { .. } | Request::Launch { .. } | Request::SpaceBusy => Some(Capability::Space),
            Request::DestroySpace { .. } | Request::ListSpaces => Some(Capability::System),
            // #EDE-172: whether a pid is in an agent space tells nothing of
            // the desk: any grant (the polkit agent's holds no capability).
            Request::SpaceOfProcess { .. } => None,
        }
    }

    /// #EDE-172: may an anonymous connection (a hello without a token on
    /// the socket: it holds no grant) ask this? An allowlist with no
    /// wildcard arm: the outputs' sizes and whether a space is ready.
    /// Everything else is refused.
    pub fn anonymous(&self) -> bool {
        match self {
            Request::Hello { .. } | Request::ListOutputs | Request::SpaceReadiness => true,
            Request::ListWindows | Request::FocusWindow { .. } | Request::CloseWindow { .. } | Request::MoveWindow { .. } | Request::MinimizeWindow { .. } | Request::RestoreWindow { .. } => false,
            Request::Capture { .. } | Request::Input { .. } | Request::PointerPosition | Request::HoldInput | Request::ReleaseInput => false,
            Request::ReloadConfig | Request::Lock | Request::Logout | Request::Unlock | Request::ListWorkspaces | Request::SwitchWorkspace { .. } | Request::MoveToWorkspace { .. } | Request::SetOutput { .. } => false,
            Request::Subscribe | Request::MintGrant { .. } | Request::RevokeGrant { .. } | Request::OpenChannel { .. } | Request::OpenWayland | Request::OpenServices { .. } | Request::BindShortcuts { .. } | Request::UnbindShortcuts { .. } => false,
            Request::ClipboardSerial | Request::ClipboardSnapshot | Request::ClipboardSet { .. } | Request::ClipboardTaken { .. } | Request::ClipboardText | Request::LaunchToken => false,
            Request::CreateSpace { .. } | Request::DestroySpace { .. } | Request::ListSpaces | Request::Launch { .. } | Request::SpaceOfProcess { .. } | Request::SpaceBusy => false,
        }
    }

    /// May a space grant (`[space]` and nothing else) ask this at all
    /// (#EDE-126)? An allowlist: every op here acts inside the grant's own
    /// space and needs only `space`, whatever [`Request::capability`]
    /// names for the person's side; everything else is refused. No
    /// wildcard arm: a new op is outside until it is written here.
    pub fn in_space(&self) -> bool {
        match self {
            Request::Hello { .. } | Request::CreateSpace { .. } | Request::DestroySpace { .. } | Request::ListSpaces | Request::Launch { .. } | Request::SpaceBusy => true,
            Request::ListWindows | Request::ListOutputs | Request::FocusWindow { .. } | Request::CloseWindow { .. } | Request::PointerPosition => true,
            Request::ClipboardSerial | Request::ClipboardSnapshot | Request::ClipboardSet { .. } | Request::ClipboardTaken { .. } | Request::ClipboardText => true,
            Request::Capture { target, .. } => matches!(target, CaptureTarget::Output { .. } | CaptureTarget::Space { .. }),
            Request::Input { bindings, .. } => !bindings,
            Request::MoveWindow { .. } | Request::MinimizeWindow { .. } | Request::RestoreWindow { .. } | Request::ListWorkspaces | Request::SwitchWorkspace { .. } | Request::MoveToWorkspace { .. } => false,
            Request::HoldInput | Request::ReleaseInput | Request::Subscribe | Request::BindShortcuts { .. } | Request::UnbindShortcuts { .. } | Request::SetOutput { .. } => false,
            Request::ReloadConfig | Request::Lock | Request::Logout | Request::Unlock | Request::MintGrant { .. } | Request::RevokeGrant { .. } | Request::LaunchToken => false,
            Request::SpaceOfProcess { .. } | Request::SpaceReadiness | Request::OpenChannel { .. } | Request::OpenWayland | Request::OpenServices { .. } => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CaptureTarget {
    Output { name: String },
    Window { window: WindowId },
    /// An agent space's output, by the space (#EDE-126): its owner, or `system`.
    Space { space: SpaceId },
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
    Error {
        id: u64,
        #[serde(deserialize_with = "lenient_failure")]
        error: Failure,
    },
    Event { event: Envelope },
    /// A message type this build does not know (a newer compositor's):
    /// read and skipped (#EXP-1994).
    #[serde(other)]
    Unknown,
}

/// #EXP-1994: a [`Failure`] with a code this build does not know (a newer
/// compositor's) still reads: as [`ErrorCode::Internal`], its code kept in
/// front of the message. `retryable` absent = the code's default.
fn lenient_failure<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Failure, D::Error> {
    #[derive(Deserialize)]
    struct Wire {
        code: String,
        message: String,
        #[serde(default)]
        retryable: Option<bool>,
    }
    let wire = Wire::deserialize(deserializer)?;
    let (code, message) = match ErrorCode::parse(&wire.code) {
        Some(code) => (code, wire.message),
        None => (ErrorCode::Internal, ecore::clamp_message(&format!("{}: {}", wire.code, wire.message))),
    };
    Ok(Failure { code, message, retryable: wire.retryable.unwrap_or(code.default_retryable()) })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "WireHello", into = "WireHello")]
pub struct Hello {
    pub version: u32,
    /// What the grant holds, of the capabilities this build knows.
    pub capabilities: Vec<Capability>,
    pub session_id: SessionId,
    /// What this compositor can do beyond v1 ([`FEATURE_SPACES`]); an
    /// older one sends none. Absent on the wire when empty.
    pub features: Vec<String>,
    /// #EXP-1994: what else the grant holds, capabilities a newer
    /// compositor names that this build does not know: the hello still
    /// reads, and a client that needs an exact grant sees it is not.
    pub unknown_capabilities: Vec<String>,
}

/// [`Hello`] on the wire: capabilities as their names, so one this build
/// does not know parses.
#[derive(Serialize, Deserialize)]
struct WireHello {
    version: u32,
    capabilities: Vec<String>,
    session_id: SessionId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    features: Vec<String>,
}

impl From<WireHello> for Hello {
    fn from(wire: WireHello) -> Self {
        let mut hello = Hello { version: wire.version, capabilities: Vec::new(), session_id: wire.session_id, features: wire.features, unknown_capabilities: Vec::new() };
        for name in wire.capabilities {
            match Capability::parse(&name) {
                Some(capability) => hello.capabilities.push(capability),
                None => hello.unknown_capabilities.push(name),
            }
        }
        hello
    }
}

impl From<Hello> for WireHello {
    fn from(hello: Hello) -> Self {
        let capabilities = hello.capabilities.iter().map(|capability| capability.as_str().to_string()).chain(hello.unknown_capabilities).collect();
        WireHello { version: hello.version, capabilities, session_id: hello.session_id, features: hello.features }
    }
}

impl Hello {
    /// The compositor makes agent spaces. A client reads this before it
    /// asks for one and never falls back to the person's seat.
    pub fn offers_spaces(&self) -> bool {
        self.features.iter().any(|f| f == FEATURE_SPACES)
    }

    /// The compositor answers [`Request::SpaceReadiness`] (#EXP-1514).
    pub fn says_space_readiness(&self) -> bool {
        self.features.iter().any(|f| f == FEATURE_SPACE_READINESS)
    }
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
    /// The agent space it lives in; `None` = the person's desk (#EDE-91).
    /// Absent on the wire when `None`: a reply reads as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SpaceId>,
}

impl WindowInfo {
    /// On the person's desk, not in an agent space.
    pub fn is_persons(&self) -> bool {
        self.space.is_none()
    }
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
    /// The agent space it shows; `None` = the person's desk (#EDE-91).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SpaceId>,
}

impl OutputInfo {
    /// One of the person's outputs, not an agent space's.
    pub fn is_persons(&self) -> bool {
        self.space.is_none()
    }
}

/// `data` of the `window.*` events.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowEvent {
    pub window: WindowId,
    pub app_id: Option<String>,
    /// As [`WindowInfo::space`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SpaceId>,
}

/// `data` of [`events::OUTPUT_CHANGED`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputEvent {
    pub output: String,
    /// As [`OutputInfo::space`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SpaceId>,
}

/// The person's windows or outputs of a list (#EDE-91).
pub fn persons<T: OnDesk>(list: Vec<T>) -> Vec<T> {
    list.into_iter().filter(OnDesk::is_persons).collect()
}

/// An event about the person's desk: its `data` names no `space` (every
/// event without one, the hotkeys and the session's too).
pub fn is_persons_event(event: &Envelope) -> bool {
    event.data.get("space").is_none_or(serde_json::Value::is_null)
}

/// What [`persons`] filters.
pub trait OnDesk {
    fn is_persons(&self) -> bool;
}

impl OnDesk for WindowInfo {
    fn is_persons(&self) -> bool {
        WindowInfo::is_persons(self)
    }
}

impl OnDesk for OutputInfo {
    fn is_persons(&self) -> bool {
        OutputInfo::is_persons(self)
    }
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
    /// #EDE-162: what the space's own agent must know, once (an app whose
    /// GPU path failed and the space that now draws in software).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
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
    /// #EXP-1994: the compositor could not read the call (it answered
    /// without the call's id): an op, or a value in one, it does not know.
    /// It is older than this client; what it said is kept.
    Unsupported(Failure),
    Protocol(String),
}

impl Error {
    /// The compositor answered the call with `code` ([`Error::Failed`]):
    /// what a client matches, never the message's words.
    pub fn has_code(&self, code: ErrorCode) -> bool {
        matches!(self, Error::Failed(failure) if failure.code == code)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Failed(e) | Error::Unsupported(e) => write!(f, "{e}"),
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
    /// The descriptors that came with the frames read so far (#EDE-172:
    /// an `open_channel` reply's), oldest first.
    fds: VecDeque<OwnedFd>,
    /// What the hello said, or for a connection that held its grant
    /// already, the `grant` its hello's reply named; `None` = anonymous.
    token: Option<Token>,
    pub hello: Hello,
    call_timeout: std::time::Duration,
    /// [`Client::connect_within`]: no call of this connection waits past it.
    deadline: Option<std::time::Instant>,
}

/// How long a call waits for the compositor to take it and to answer
/// (#EDE-74); a deferred clipboard read answers within 3 s.
#[cfg(unix)]
pub const CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[cfg(unix)]
impl Client {
    /// Connects to `socket` and says hello with `token`.
    pub fn connect(socket: &Path, token: Token) -> Result<Self, Error> {
        Self::over(UnixStream::connect(socket)?, token)
    }

    /// [`Client::connect`] under ONE deadline over the connect, the hello
    /// and every call after it (#EXP-1514: a client asking whether a space
    /// is ready must not wait [`CALL_TIMEOUT`] per step on a desktop that
    /// does not answer). Past it a call fails `TimedOut` at once.
    pub fn connect_within(socket: &Path, token: Token, deadline: std::time::Duration) -> Result<Self, Error> {
        let until = std::time::Instant::now() + deadline;
        let stream = UnixStream::connect(socket)?;
        Self::over_until(stream, token, CALL_TIMEOUT, Some(until))
    }

    /// Hello over an already connected stream.
    pub fn over(stream: UnixStream, token: Token) -> Result<Self, Error> {
        Self::over_within(stream, token, CALL_TIMEOUT)
    }

    fn over_within(stream: UnixStream, token: Token, call_timeout: std::time::Duration) -> Result<Self, Error> {
        Self::over_until(stream, token, call_timeout, None)
    }

    fn over_until(stream: UnixStream, token: Token, call_timeout: std::time::Duration, deadline: Option<std::time::Instant>) -> Result<Self, Error> {
        Self::handshake(stream, Some(token), call_timeout, deadline)
    }

    /// The hello: with `token`, or none on a connection that holds its
    /// grant already, whose reply names it (`grant`).
    fn handshake(stream: UnixStream, token: Option<Token>, call_timeout: std::time::Duration, deadline: Option<std::time::Instant>) -> Result<Self, Error> {
        stream.set_write_timeout(Some(call_timeout))?;
        let mut client = Self { stream, next_id: 1, events: VecDeque::new(), fds: VecDeque::new(), token: token.clone(), hello: Hello { version: 0, capabilities: vec![], session_id: SessionId::new("pending").unwrap(), features: vec![], unknown_capabilities: vec![] }, call_timeout, deadline };
        let hello = client.call(Request::Hello { version: PROTOCOL_VERSION, token: token.clone() })?;
        // A channel's reply names its grant; an anonymous one's names none.
        if token.is_none() {
            if let Some(grant) = hello.get("grant").and_then(serde_json::Value::as_str) {
                client.token = Some(Token::new(grant).map_err(|e| Error::Protocol(format!("{e:?}")))?);
            }
        }
        client.hello = serde_json::from_value(hello).map_err(|e| Error::Protocol(e.to_string()))?;
        Ok(client)
    }

    /// #EDE-172: hello over a connection that already holds its grant (one
    /// the compositor made: [`Client::open_channel`]'s descriptor, an
    /// inherited one). No token goes over it.
    pub fn from_fd(fd: OwnedFd) -> Result<Self, Error> {
        Self::handshake(UnixStream::from(fd), None, CALL_TIMEOUT, None)
    }

    /// #EDE-172: a connection to the compositor of `$WAYLAND_DISPLAY` that
    /// holds no grant: its hello names no token. It asks only what
    /// [`Request::anonymous`] lets it (the outputs, whether a space is
    /// ready). An older compositor refuses the hello
    /// ([`Error::Unsupported`]).
    pub fn anonymous() -> Result<Self, Error> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| Error::Protocol("XDG_RUNTIME_DIR unset".into()))?;
        let display = std::env::var("WAYLAND_DISPLAY").map_err(|_| Error::Protocol("WAYLAND_DISPLAY unset".into()))?;
        Self::anonymous_at(&socket_path(Path::new(&runtime), &display))
    }

    /// [`Client::anonymous`] on `socket`.
    pub fn anonymous_at(socket: &Path) -> Result<Self, Error> {
        Self::handshake(UnixStream::connect(socket)?, None, CALL_TIMEOUT, None)
    }

    /// #EDE-172: the control connection this process was started with: the
    /// descriptor [`CONTROL_FD_ENV`] names (the compositor's spawn hands it
    /// over at fd 3), or `None` without one. It is taken once: the variable
    /// is removed (call this before other threads read the environment) and
    /// the descriptor is closed on exec, so what this process starts
    /// inherits neither. A descriptor that is not a socket is refused.
    pub fn inherited() -> Option<Result<Self, Error>> {
        use std::os::fd::FromRawFd;
        static TAKEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let named = std::env::var(CONTROL_FD_ENV).ok()?;
        std::env::remove_var(CONTROL_FD_ENV);
        if TAKEN.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Some(Err(Error::Protocol("the inherited control connection was taken already".into())));
        }
        let Some(raw) = named.trim().parse::<i32>().ok().filter(|fd| *fd > 2) else {
            return Some(Err(Error::Protocol(format!("{CONTROL_FD_ENV}={named:?} names no descriptor"))));
        };
        // Safety: the descriptor is this process's by inheritance, named
        // for it by its parent, and taken once (above); nothing else here
        // owns it. A number that is not open fails `fstat` below and is
        // forgotten again without being closed.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        match rustix::fs::fstat(&fd) {
            Ok(stat) if rustix::fs::FileType::from_raw_mode(stat.st_mode) == rustix::fs::FileType::Socket => {}
            Ok(_) => {
                std::mem::forget(fd);
                return Some(Err(Error::Protocol(format!("descriptor {raw} is not a socket"))));
            }
            Err(e) => {
                std::mem::forget(fd);
                return Some(Err(Error::Io(e.into())));
            }
        }
        if let Err(e) = rustix::io::fcntl_setfd(&fd, rustix::io::FdFlags::CLOEXEC) {
            return Some(Err(Error::Io(e.into())));
        }
        Some(Self::from_fd(fd))
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
        let (socket, token) = Self::env_socket_and_token()?;
        Self::connect(&socket, token)
    }

    /// #EDE-172: [`Client::connect_env`] whose hello waits up to
    /// `hello_within` for its reply: with Exponential Desktop's gate on, the
    /// compositor holds it while the person reads a consent prompt
    /// (`exponential-ctl`). Later calls wait [`CALL_TIMEOUT`].
    pub fn connect_env_waiting(hello_within: std::time::Duration) -> Result<Self, Error> {
        let (socket, token) = Self::env_socket_and_token()?;
        Self::over_waiting(UnixStream::connect(socket)?, token, hello_within)
    }

    /// Hello over `stream` waiting up to `hello_within` for its reply;
    /// later calls wait [`CALL_TIMEOUT`] ([`Client::connect_env_waiting`]).
    pub fn over_waiting(stream: UnixStream, token: Token, hello_within: std::time::Duration) -> Result<Self, Error> {
        let mut client = Self::over_within(stream, token, hello_within)?;
        client.set_call_timeout(CALL_TIMEOUT);
        Ok(client)
    }

    /// The socket of `$WAYLAND_DISPLAY`'s compositor and the token
    /// `EDE_TOKEN` (else the root token file) names.
    fn env_socket_and_token() -> Result<(PathBuf, Token), Error> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| Error::Protocol("XDG_RUNTIME_DIR unset".into()))?;
        let display = std::env::var("WAYLAND_DISPLAY").map_err(|_| Error::Protocol("WAYLAND_DISPLAY unset".into()))?;
        let runtime = Path::new(&runtime);
        let token = match std::env::var("EDE_TOKEN") {
            Ok(t) => t,
            Err(_) => std::fs::read_to_string(token_path(runtime, &display))?,
        };
        let token = Token::new(token.trim()).map_err(|e| Error::Protocol(e.to_string()))?;
        Ok((socket_path(runtime, &display), token))
    }

    /// A grant of this process's own for `session_id`, holding
    /// `capabilities` (#EDE-22: the default for everything but the shell).
    /// The root token (or `EDE_TOKEN`) mints it and that connection is
    /// dropped; a token that cannot mint (a narrower grant handed to this
    /// process in `EDE_TOKEN`) is kept as it is.
    pub fn mint_env(session_id: SessionId, capabilities: Vec<Capability>, expires_at: Option<i64>) -> Result<Token, Error> {
        let mut root = Self::connect_env()?;
        if !root.hello.capabilities.contains(&Capability::System) {
            return Ok(root.token().clone());
        }
        Ok(root.mint(session_id, capabilities, expires_at)?.token)
    }

    /// Connects with a grant minted for `session_id` ([`Client::mint_env`]).
    pub fn connect_as(session_id: SessionId, capabilities: Vec<Capability>, expires_at: Option<i64>) -> Result<Self, Error> {
        Self::connect_with_token(Self::mint_env(session_id, capabilities, expires_at)?)
    }

    /// How long each call of this connection waits for its answer from now
    /// on ([`CALL_TIMEOUT`] by default); a call past it ends the connection.
    pub fn set_call_timeout(&mut self, timeout: std::time::Duration) {
        self.call_timeout = timeout;
        let _ = self.stream.set_write_timeout(Some(timeout));
    }

    /// The token this connection said hello with; on one that held its
    /// grant already (#EDE-172), the grant its hello's reply named: a
    /// bound grant's id, refused at hello and taken by
    /// [`Client::revoke_grant`].
    ///
    /// Panics on an anonymous connection ([`Client::anonymous`]), which
    /// holds no grant: [`Client::grant`] there.
    pub fn token(&self) -> &Token {
        self.token.as_ref().expect("an anonymous connection holds no grant")
    }

    /// The grant this connection holds ([`Client::token`]); `None` =
    /// anonymous.
    pub fn grant(&self) -> Option<&Token> {
        self.token.as_ref()
    }

    /// #EDE-172 ([`Request::OpenChannel`]): one more connection, holding
    /// this connection's own grant (`session_id` none, `capabilities`
    /// what it holds) or a child of it for `session_id`, no wider and no
    /// longer-lived; and that connection's grant (a child's `token` is its
    /// id). An older compositor answers [`Error::Unsupported`].
    pub fn open_channel(&mut self, session_id: Option<SessionId>, capabilities: Vec<Capability>, expires_at: Option<i64>) -> Result<(Client, Grant), Error> {
        let (fd, grant) = self.open_channel_fd(session_id, capabilities, expires_at)?;
        Ok((Client::from_fd(fd)?, grant))
    }

    /// [`Client::open_channel`] without the hello: the descriptor itself,
    /// for a process this one starts (#EDE-172 P4: the shell's supervisor
    /// hands each shell its own, [`process::hand_over`]).
    pub fn open_channel_fd(&mut self, session_id: Option<SessionId>, capabilities: Vec<Capability>, expires_at: Option<i64>) -> Result<(OwnedFd, Grant), Error> {
        // A descriptor left over from a call that failed after it arrived is no one's.
        self.fds.clear();
        let mut grant = self.call(Request::OpenChannel { session_id, capabilities, expires_at })?;
        let fd = self.fds.pop_front().ok_or_else(|| Error::Protocol("open_channel answered without a descriptor".into()))?;
        if let Some(serde_json::Value::Array(capabilities)) = grant.get_mut("capabilities") {
            capabilities.retain(|capability| capability.as_str().and_then(Capability::parse).is_some());
        }
        let grant: Grant = serde_json::from_value(grant).map_err(|e| Error::Protocol(e.to_string()))?;
        Ok((fd, grant))
    }

    /// #EDE-172 (P4, [`Request::OpenWayland`]): a Wayland connection the
    /// compositor made, for the `WAYLAND_SOCKET` of a shell this process
    /// starts. An older compositor answers [`Error::Unsupported`].
    pub fn open_wayland(&mut self) -> Result<OwnedFd, Error> {
        self.fds.clear();
        self.call(Request::OpenWayland)?;
        self.fds.pop_front().ok_or_else(|| Error::Protocol("open_wayland answered without a descriptor".into()))
    }

    /// #EDE-172 (P13, [`Request::OpenServices`]): a connection to the
    /// services daemon the compositor runs, whose `call` holds
    /// `capabilities`. A compositor that runs none answers
    /// [`ErrorCode::Unavailable`], an older one [`Error::Unsupported`].
    pub fn open_services(&mut self, capabilities: Vec<Capability>) -> Result<OwnedFd, Error> {
        self.fds.clear();
        self.call(Request::OpenServices { capabilities })?;
        self.fds.pop_front().ok_or_else(|| Error::Protocol("open_services answered without a descriptor".into()))
    }

    /// [`Request::RevokeGrant`]: `grant` = a token, or a bound grant's id.
    pub fn revoke_grant(&mut self, grant: &Token) -> Result<(), Error> {
        self.call(Request::RevokeGrant { token: grant.clone() }).map(|_| ())
    }

    pub fn call(&mut self, request: Request) -> Result<serde_json::Value, Error> {
        let id = self.next_id;
        self.next_id += 1;
        let bytes = serde_json::to_vec(&Call { id, request }).map_err(|e| Error::Protocol(e.to_string()))?;
        self.within(|client, deadline| {
            write_frame(&mut client.stream, &bytes)?;
            loop {
                match client.read_message()? {
                    Message::Event { event } => client.events.push_back(event),
                    Message::Unknown => {}
                    Message::Reply { id: rid, result } if rid == id => return Ok(result),
                    Message::Error { id: rid, error } if rid == id => return Err(Error::Failed(error)),
                    // A call the compositor could not parse (an op it does not
                    // know: an older compositor) comes back without its id.
                    Message::Error { id: 0, error } => return Err(Error::Unsupported(error)),
                    other => return Err(Error::Protocol(format!("unexpected {other:?}"))),
                }
                if std::time::Instant::now() >= deadline {
                    return Err(Error::Io(std::io::ErrorKind::TimedOut.into()));
                }
            }
        })
    }

    /// One exchange under the call timeout (#EDE-74): a compositor that
    /// does not answer fails the call instead of blocking it for good. The
    /// stream is out of step then, so it is closed (the compositor lets go
    /// of what this connection held). Waiting for events keeps its own
    /// timeout ([`Client::set_read_timeout`]).
    fn within<T>(&mut self, exchange: impl FnOnce(&mut Self, std::time::Instant) -> Result<T, Error>) -> Result<T, Error> {
        let now = std::time::Instant::now();
        let until = self.deadline.map_or(now + self.call_timeout, |deadline| deadline.min(now + self.call_timeout));
        let left = until.saturating_duration_since(now);
        if left.is_zero() {
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
            return Err(Error::Io(std::io::ErrorKind::TimedOut.into()));
        }
        let events = self.stream.read_timeout().ok().flatten();
        let _ = self.stream.set_read_timeout(Some(left));
        if self.deadline.is_some() {
            let _ = self.stream.set_write_timeout(Some(left));
        }
        let out = exchange(self, until);
        let _ = self.stream.set_read_timeout(events);
        if matches!(out, Err(Error::Io(_))) {
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
        }
        out
    }

    fn call_as<T: serde::de::DeserializeOwned>(&mut self, request: Request) -> Result<T, Error> {
        serde_json::from_value(self.call(request)?).map_err(|e| Error::Protocol(e.to_string()))
    }

    fn read_message(&mut self) -> Result<Message, Error> {
        let frame = read_frame(&mut Receiving { stream: &self.stream, fds: &mut self.fds })?;
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
        let pixels = self.within(|client, _| Ok(read_frame(&mut Receiving { stream: &client.stream, fds: &mut client.fds })?))?;
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

    /// The grant reads with the capabilities this build knows (#EXP-1994).
    pub fn mint(&mut self, session_id: SessionId, capabilities: Vec<Capability>, expires_at: Option<i64>) -> Result<Grant, Error> {
        let mut grant = self.call(Request::MintGrant { session_id, run_id: None, capabilities, expires_at })?;
        if let Some(serde_json::Value::Array(capabilities)) = grant.get_mut("capabilities") {
            capabilities.retain(|capability| capability.as_str().and_then(Capability::parse).is_some());
        }
        serde_json::from_value(grant).map_err(|e| Error::Protocol(e.to_string()))
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

    /// [`Request::CreateSpace`]: this grant's space (the same one again).
    pub fn create_space(&mut self, width: i32, height: i32) -> Result<SpaceInfo, Error> {
        self.call_as(Request::CreateSpace { width, height })
    }

    pub fn destroy_space(&mut self, space: SpaceId) -> Result<(), Error> {
        self.call(Request::DestroySpace { space }).map(|_| ())
    }

    pub fn spaces(&mut self) -> Result<Vec<SpaceInfo>, Error> {
        self.call_as(Request::ListSpaces)
    }

    /// [`Request::Launch`]: the pid of what started in this grant's space.
    pub fn launch(&mut self, argv: Vec<String>, env: std::collections::BTreeMap<String, String>, cwd: Option<String>) -> Result<u32, Error> {
        self.call_as::<Launched>(Request::Launch { argv, env, cwd }).map(|l| l.pid)
    }

    /// [`Request::SpaceReadiness`]: ask only when the hello
    /// [`Hello::says_space_readiness`].
    pub fn space_readiness(&mut self) -> Result<SpaceReadiness, Error> {
        self.call_as(Request::SpaceReadiness)
    }

    /// [`Request::SpaceBusy`]: this grant's space is busy for [`SPACE_BUSY_SECS`].
    pub fn space_busy(&mut self) -> Result<(), Error> {
        self.call(Request::SpaceBusy).map(|_| ())
    }

    /// [`Request::SpaceOfProcess`]: the space `pid` belongs to (`None` =
    /// none), whether it runs in the person's session, and whether spaces
    /// have launches outside systemd scopes.
    pub fn space_of_process(&mut self, pid: u32) -> Result<ProcessAnswer, Error> {
        ProcessAnswer::from_wire(self.call(Request::SpaceOfProcess { pid })?).map_err(|e| Error::Protocol(e.to_string()))
    }

    /// [`Request::LaunchToken`]: for the environment of a launch the person made.
    pub fn launch_token(&mut self) -> Result<String, Error> {
        self.call_as::<LaunchToken>(Request::LaunchToken).map(|t| t.token)
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

/// #EDE-172: the client's reads, which keep the descriptors that come with
/// the bytes (`SCM_RIGHTS`; a plain read drops them, closed). Only an
/// `open_channel` reply carries one.
#[cfg(unix)]
struct Receiving<'a> {
    stream: &'a UnixStream,
    fds: &'a mut VecDeque<OwnedFd>,
}

#[cfg(unix)]
impl Read for Receiving<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use rustix::net::{RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags};
        let mut space = [std::mem::MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(4))];
        let mut control = RecvAncillaryBuffer::new(&mut space);
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let flags = RecvFlags::CMSG_CLOEXEC;
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        let flags = RecvFlags::empty();
        let received = rustix::net::recvmsg(self.stream, &mut [std::io::IoSliceMut::new(buf)], &mut control, flags)?;
        for message in control.drain() {
            if let RecvAncillaryMessage::ScmRights(fds) = message {
                for fd in fds {
                    #[cfg(not(any(target_os = "linux", target_os = "android")))]
                    let _ = rustix::io::fcntl_setfd(&fd, rustix::io::FdFlags::CLOEXEC);
                    self.fds.push_back(fd);
                }
            }
        }
        Ok(received.bytes)
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
        assert_eq!(Request::LaunchToken.capability(), Some(Capability::System));
        assert_eq!(serde_json::to_value(Call { id: 6, request: Request::LaunchToken }).unwrap(), serde_json::json!({"id": 6, "op": "launch_token"}));
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
        let snapshot = ClipboardSnapshot { serial: 3, items, unread: Some(vec!["image/png".into()]) };
        assert_eq!(serde_json::from_value::<ClipboardSnapshot>(serde_json::to_value(&snapshot).unwrap()).unwrap(), snapshot);
        assert!(!snapshot.is_whole());
        // #EXP-1900: only a compositor that names nothing unread read it all;
        // one from before the field dropped types silently.
        let whole: ClipboardSnapshot = serde_json::from_str(r#"{"serial":4,"items":[],"unread":[]}"#).unwrap();
        assert!(whole.is_whole());
        let old: ClipboardSnapshot = serde_json::from_str(r#"{"serial":4,"items":[]}"#).unwrap();
        assert_eq!(old.unread, None);
        assert!(!old.is_whole());
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

    /// #EDE-91: `WindowInfo` and `OutputInfo` as they were before `space`,
    /// field for field: what an old client parses a reply into.
    #[derive(Deserialize)]
    #[allow(dead_code)]
    struct OldWindowInfo {
        id: WindowId,
        title: Option<String>,
        app_id: Option<String>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        focused: bool,
        fullscreen: bool,
        protected: bool,
        x11: bool,
        #[serde(default)]
        workspace: usize,
        #[serde(default)]
        minimized: bool,
    }

    #[derive(Deserialize)]
    #[allow(dead_code)]
    struct OldOutputInfo {
        name: String,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        scale: f64,
        #[serde(default)]
        modes: Vec<ModeInfo>,
        #[serde(default)]
        make: String,
        #[serde(default)]
        model: String,
        #[serde(default)]
        physical_mm: [i32; 2],
    }

    fn window(space: Option<&str>) -> WindowInfo {
        WindowInfo { id: WindowId::new("w1").unwrap(), title: Some("a".into()), app_id: None, x: 0, y: 0, width: 1, height: 1, focused: true, fullscreen: false, protected: false, x11: false, workspace: 0, minimized: false, space: space.map(|s| SpaceId::new(s).unwrap()) }
    }

    fn output(space: Option<&str>) -> OutputInfo {
        OutputInfo { name: "DP-1".into(), x: 0, y: 0, width: 800, height: 600, scale: 1.0, modes: vec![], make: String::new(), model: String::new(), physical_mm: [0, 0], space: space.map(|s| SpaceId::new(s).unwrap()) }
    }

    /// #EDE-91: the person's entries go out byte for byte as before `space`.
    #[test]
    fn a_person_entry_is_written_as_before_space() {
        assert_eq!(
            serde_json::to_string(&window(None)).unwrap(),
            r#"{"id":"w1","title":"a","app_id":null,"x":0,"y":0,"width":1,"height":1,"focused":true,"fullscreen":false,"protected":false,"x11":false,"workspace":0,"minimized":false}"#
        );
        assert_eq!(serde_json::to_string(&output(None)).unwrap(), r#"{"name":"DP-1","x":0,"y":0,"width":800,"height":600,"scale":1.0,"modes":[],"make":"","model":"","physical_mm":[0,0]}"#);
        let opened = WindowEvent { window: WindowId::new("w1").unwrap(), app_id: None, space: None };
        assert_eq!(serde_json::to_value(&opened).unwrap(), serde_json::json!({ "window": "w1", "appId": null }));
        let changed = OutputEvent { output: "DP-1".into(), space: None };
        assert_eq!(serde_json::to_value(&changed).unwrap(), serde_json::json!({ "output": "DP-1" }));
    }

    /// #EDE-91: `space` round trips; an old reply (none) reads as the person's.
    #[test]
    fn space_round_trips_and_an_old_reply_is_the_persons() {
        for w in [window(None), window(Some("sp1"))] {
            assert_eq!(serde_json::from_value::<WindowInfo>(serde_json::to_value(&w).unwrap()).unwrap(), w);
        }
        for o in [output(None), output(Some("sp1"))] {
            assert_eq!(serde_json::from_value::<OutputInfo>(serde_json::to_value(&o).unwrap()).unwrap(), o);
        }
        assert_eq!(serde_json::to_value(window(Some("sp1"))).unwrap()["space"], "sp1");
        let old = r#"{"id":"w1","title":"a","app_id":null,"x":0,"y":0,"width":1,"height":1,"focused":true,"fullscreen":false,"protected":false,"x11":false}"#;
        let w: WindowInfo = serde_json::from_str(old).unwrap();
        assert!(w.space.is_none() && w.is_persons());
        let o: OutputInfo = serde_json::from_str(r#"{"name":"DP-1","x":0,"y":0,"width":800,"height":600,"scale":1.0}"#).unwrap();
        assert!(o.space.is_none() && o.is_persons());
        assert!(!window(Some("sp1")).is_persons() && !output(Some("sp1")).is_persons());
    }

    /// #EDE-91: an old client parses a reply and an event that name a space.
    #[test]
    fn an_old_client_parses_entries_with_a_space() {
        let windows = serde_json::to_string(&vec![window(None), window(Some("sp1"))]).unwrap();
        let old: Vec<OldWindowInfo> = serde_json::from_str(&windows).unwrap();
        assert_eq!(old.len(), 2);
        let outputs = serde_json::to_string(&vec![output(Some("sp1"))]).unwrap();
        let old: Vec<OldOutputInfo> = serde_json::from_str(&outputs).unwrap();
        assert_eq!(old[0].name, "DP-1");
        let event = Envelope::new("ev-1", events::WINDOW_OPENED, 1, EVENT_SOURCE, None, serde_json::to_value(WindowEvent { window: WindowId::new("w1").unwrap(), app_id: Some("a".into()), space: Some(SpaceId::new("sp1").unwrap()) }).unwrap());
        let wire = serde_json::to_string(&Message::Event { event }).unwrap();
        let Message::Event { event } = serde_json::from_str(&wire).unwrap() else { panic!("an event") };
        assert_eq!(event.data["window"], "w1", "what an old shell reads of it");
    }

    /// #EDE-91: an event's space; one without (every event today) is the person's.
    #[test]
    fn an_event_without_a_space_is_the_persons() {
        let event = |data: serde_json::Value| Envelope::new("ev-1", events::OUTPUT_CHANGED, 1, EVENT_SOURCE, None, data);
        assert!(is_persons_event(&event(serde_json::json!({ "output": "DP-1" }))));
        assert!(is_persons_event(&event(serde_json::json!({ "name": "switcher.next" }))));
        assert!(is_persons_event(&event(serde_json::Value::Null)));
        assert!(!is_persons_event(&event(serde_json::to_value(OutputEvent { output: "DP-1".into(), space: Some(SpaceId::new("sp1").unwrap()) }).unwrap())));
        let both = vec![window(None), window(Some("sp1"))];
        assert_eq!(persons(both), vec![window(None)]);
    }

    /// #EDE-126: the ops that name a space, as they go over the wire.
    #[test]
    fn space_ops_are_flat_json_and_round_trip() {
        let space = || SpaceId::new("space-1").unwrap();
        for (request, json) in [
            (Request::CreateSpace { width: 1280, height: 800 }, serde_json::json!({"id": 1, "op": "create_space", "width": 1280, "height": 800})),
            (Request::DestroySpace { space: space() }, serde_json::json!({"id": 1, "op": "destroy_space", "space": "space-1"})),
            (Request::ListSpaces, serde_json::json!({"id": 1, "op": "list_spaces"})),
            (Request::Launch { argv: vec!["foot".into()], env: Default::default(), cwd: None }, serde_json::json!({"id": 1, "op": "launch", "argv": ["foot"]})),
            (
                Request::Launch { argv: vec!["sh".into(), "-c".into(), "true".into()], env: [("A".to_string(), "b".to_string())].into(), cwd: Some("/tmp".into()) },
                serde_json::json!({"id": 1, "op": "launch", "argv": ["sh", "-c", "true"], "env": {"A": "b"}, "cwd": "/tmp"}),
            ),
            (Request::Capture { target: CaptureTarget::Space { space: space() }, since: None }, serde_json::json!({"id": 1, "op": "capture", "target": {"kind": "space", "space": "space-1"}})),
        ] {
            let call = Call { id: 1, request };
            assert_eq!(serde_json::to_value(&call).unwrap(), json);
            assert_eq!(serde_json::from_value::<Call>(json).unwrap(), call);
        }
        let info = SpaceInfo { id: space(), session_id: SessionId::new("run-1").unwrap(), output: "space-1".into(), width: 1280, height: 800, wayland_display: "/run/user/1000/exponential-desktop/wayland-1.spaces/space-1/wayland-0".into() };
        assert_eq!(serde_json::from_value::<SpaceInfo>(serde_json::to_value(&info).unwrap()).unwrap(), info);
        assert_eq!(serde_json::to_value(Launched { pid: 7 }).unwrap(), serde_json::json!({"pid": 7}));
    }

    /// #EDE-127: `space_of_process` and its answer on the wire; `null` is the person's.
    #[test]
    fn space_of_process_is_flat_json_and_round_trips() {
        let call = Call { id: 1, request: Request::SpaceOfProcess { pid: 4242 } };
        let json = serde_json::json!({"id": 1, "op": "space_of_process", "pid": 4242});
        assert_eq!(serde_json::to_value(&call).unwrap(), json);
        assert_eq!(serde_json::from_value::<Call>(json).unwrap(), call);
        let tied = Some(ProcessSpace { space: SpaceId::new("space-2").unwrap(), by: Membership::Environment, pid: 4241 });
        let wire = serde_json::json!({"space": "space-2", "by": "environment", "pid": 4241});
        assert_eq!(serde_json::to_value(&tied).unwrap(), wire);
        assert_eq!(serde_json::from_value::<Option<ProcessSpace>>(wire).unwrap(), tied);
        assert_eq!(serde_json::from_value::<Option<ProcessSpace>>(serde_json::Value::Null).unwrap(), None);
        for (by, name) in [(Membership::Cgroup, "cgroup"), (Membership::Launch, "launch"), (Membership::Group, "group"), (Membership::Client, "client"), (Membership::Environment, "environment")] {
            assert_eq!(serde_json::to_value(by).unwrap(), serde_json::json!(name));
        }
        // #EDE-143: the answer with the session's flags, and what an older
        // compositor sends (`null`, a bare tie, no `no_space`) read as one;
        // one that does not say there is no space is taken to have one.
        let answer = ProcessAnswer { space: tied.clone(), session: false, unscoped_launches: true, no_space: false };
        let json = serde_json::to_value(&answer).unwrap();
        assert_eq!(json, serde_json::json!({"space": {"space": "space-2", "by": "environment", "pid": 4241}, "session": false, "unscoped_launches": true, "no_space": false}));
        assert_eq!(ProcessAnswer::from_wire(json).unwrap(), answer);
        let none = ProcessAnswer { space: None, session: false, unscoped_launches: false, no_space: true };
        assert_eq!(ProcessAnswer::from_wire(serde_json::to_value(&none).unwrap()).unwrap(), none);
        assert_eq!(ProcessAnswer::from_wire(serde_json::json!({"space": null, "session": true, "unscoped_launches": false})).unwrap(), ProcessAnswer { space: None, session: true, unscoped_launches: false, no_space: false });
        assert_eq!(ProcessAnswer::from_wire(serde_json::Value::Null).unwrap(), ProcessAnswer::default());
        assert_eq!(ProcessAnswer::from_wire(serde_json::json!({"space": "space-2", "by": "environment", "pid": 4241})).unwrap(), ProcessAnswer { space: tied, ..ProcessAnswer::default() });
    }

    /// #EXP-1514: `space_readiness` and its answer on the wire; a block and
    /// a count are absent when there is none, and a block this build does
    /// not know still reads (as a no).
    #[test]
    fn space_readiness_is_flat_json_and_round_trips() {
        for (request, op) in [(Request::SpaceReadiness, "space_readiness"), (Request::SpaceBusy, "space_busy")] {
            let call = Call { id: 1, request };
            let json = serde_json::json!({"id": 1, "op": op});
            assert_eq!(serde_json::to_value(&call).unwrap(), json);
            assert_eq!(serde_json::from_value::<Call>(json).unwrap(), call);
        }
        let ready = SpaceReadiness { ready: true, block: None, max: Some(8) };
        assert_eq!(serde_json::to_value(&ready).unwrap(), serde_json::json!({"ready": true, "max": 8}));
        let blocked: SpaceReadiness = serde_json::from_value(serde_json::json!({"ready": false, "block": "later"})).unwrap();
        assert_eq!(blocked, SpaceReadiness { ready: false, block: Some("later".into()), max: None });
        assert_eq!(serde_json::from_value::<SpaceReadiness>(serde_json::to_value(&blocked).unwrap()).unwrap(), blocked);
    }

    /// `Hello` as it was before `features`: what an old client parses a reply into.
    #[derive(Deserialize)]
    #[allow(dead_code)]
    struct OldHello {
        version: u32,
        capabilities: Vec<Capability>,
        session_id: SessionId,
    }

    /// #EDE-126: a hello without features is written and read as before;
    /// an old client reads one that has them.
    #[test]
    fn hello_features_are_additive() {
        let hello = |features: &[&str]| Hello { version: 1, capabilities: vec![Capability::Space], session_id: SessionId::new("run-1").unwrap(), features: features.iter().map(|f| f.to_string()).collect(), unknown_capabilities: vec![] };
        assert_eq!(serde_json::to_string(&hello(&[])).unwrap(), r#"{"version":1,"capabilities":["space"],"session_id":"run-1"}"#);
        let old: Hello = serde_json::from_str(r#"{"version":1,"capabilities":["space"],"session_id":"run-1"}"#).unwrap();
        assert!(old.features.is_empty() && !old.offers_spaces(), "an older compositor offers no spaces");
        let new = hello(&[FEATURE_SPACES, "later"]);
        assert!(new.offers_spaces());
        // #EXP-1514: eDE 0.2.29 says `spaces` and cannot say a space is ready.
        assert!(!new.says_space_readiness() && hello(&[FEATURE_SPACES, FEATURE_SPACE_READINESS]).says_space_readiness());
        let wire = serde_json::to_string(&new).unwrap();
        assert_eq!(serde_json::from_str::<Hello>(&wire).unwrap(), new);
        assert_eq!(serde_json::from_str::<OldHello>(&wire).unwrap().version, 1);
    }

    /// #EXP-1994: what a NEWER compositor sends reads in this build: a
    /// capability, a feature, a field, an error code, a message type and a
    /// membership it does not know.
    #[test]
    fn a_newer_compositors_words_parse() {
        let hello: Hello = serde_json::from_value(serde_json::json!({"version": 1, "capabilities": ["capture", "telepathy", "input:person"], "session_id": "epilot", "features": ["spaces", "teleport"], "motd": "hi"})).unwrap();
        assert_eq!(hello.capabilities, [Capability::Capture, Capability::InputPerson]);
        assert_eq!(hello.unknown_capabilities, ["telepathy"], "kept, so an exact grant can tell");
        assert!(hello.offers_spaces());
        let wire = serde_json::to_value(&hello).unwrap();
        assert_eq!(wire["capabilities"], serde_json::json!(["capture", "input:person", "telepathy"]), "written back whole");
        assert_eq!(serde_json::from_value::<Hello>(wire).unwrap(), hello);

        let error: Message = serde_json::from_value(serde_json::json!({"type": "error", "id": 3, "error": {"code": "overloaded", "message": "try later", "retryable": true}})).unwrap();
        assert_eq!(error, Message::Error { id: 3, error: Failure { code: ErrorCode::Internal, message: "overloaded: try later".into(), retryable: true } });
        let known: Message = serde_json::from_value(serde_json::json!({"type": "error", "id": 3, "error": {"code": "timeout", "message": "slow"}})).unwrap();
        assert_eq!(known, Message::Error { id: 3, error: Failure::new(ErrorCode::Timeout, "slow") }, "a missing retryable is the code's default");
        assert_eq!(serde_json::from_value::<Message>(serde_json::json!({"type": "progress", "id": 3, "done": 0.5})).unwrap(), Message::Unknown);
        assert_eq!(serde_json::from_value::<Membership>(serde_json::json!("ptrace")).unwrap(), Membership::Unknown);

        // A failure this build writes reads as before.
        let failure = Failure::new(ErrorCode::NotFound, "no such window");
        let wire = serde_json::to_value(Message::Error { id: 4, error: failure.clone() }).unwrap();
        assert_eq!(wire, serde_json::json!({"type": "error", "id": 4, "error": {"code": "not_found", "message": "no such window", "retryable": false}}));
        assert_eq!(serde_json::from_value::<Message>(wire).unwrap(), Message::Error { id: 4, error: failure });
    }

    /// A compositor on a socketpair: it answers each call it reads with
    /// `answer`'s messages (`None` = it could not read the call).
    #[cfg(unix)]
    fn compositor(hello: serde_json::Value, answer: impl Fn(&serde_json::Value) -> Vec<serde_json::Value> + Send + 'static) -> Client {
        let (ours, mut theirs) = UnixStream::pair().unwrap();
        std::thread::spawn(move || {
            let call: serde_json::Value = serde_json::from_slice(&read_frame(&mut theirs).unwrap()).unwrap();
            let reply = serde_json::json!({"type": "reply", "id": call["id"], "result": hello});
            write_frame(&mut theirs, &serde_json::to_vec(&reply).unwrap()).unwrap();
            while let Ok(frame) = read_frame(&mut theirs) {
                let call: serde_json::Value = serde_json::from_slice(&frame).unwrap();
                for message in answer(&call) {
                    write_frame(&mut theirs, &serde_json::to_vec(&message).unwrap()).unwrap();
                }
            }
        });
        Client::over(ours, Token::new("0".repeat(64)).unwrap()).unwrap()
    }

    /// #EXP-1994, this client against a NEWER compositor: its hello with a
    /// capability we do not know, a message type we do not know before the
    /// reply, an error code we do not know; the calls go on.
    #[cfg(unix)]
    #[test]
    fn an_older_client_keeps_working_with_a_newer_compositor() {
        let hello = serde_json::json!({"version": PROTOCOL_VERSION, "capabilities": ["system", "capture", "lend:window"], "session_id": "shell", "features": ["spaces", "later"]});
        let mut client = compositor(hello, |call| {
            let id = call["id"].clone();
            match call["op"].as_str() {
                Some("list_windows") => vec![serde_json::json!({"type": "progress", "id": id}), serde_json::json!({"type": "reply", "id": id, "result": []})],
                Some("mint_grant") => vec![serde_json::json!({"type": "reply", "id": id, "result": {"token": "1".repeat(64), "sessionId": "run-1", "capabilities": ["capture", "capture:audio"]}})],
                _ => vec![serde_json::json!({"type": "error", "id": id, "error": {"code": "busy", "message": "the seat is busy", "retryable": true}})],
            }
        });
        assert!(client.hello.capabilities.contains(&Capability::System) && client.hello.offers_spaces());
        assert_eq!(client.hello.unknown_capabilities, ["lend:window"]);
        assert_eq!(client.windows().unwrap(), vec![], "the unknown message is skipped, the reply read");
        let grant = client.mint(SessionId::new("run-1").unwrap(), vec![Capability::Capture], None).unwrap();
        assert_eq!(grant.capabilities, [Capability::Capture]);
        match client.pointer() {
            Err(Error::Failed(failure)) => assert_eq!((failure.code, failure.retryable, failure.message.as_str()), (ErrorCode::Internal, true, "busy: the seat is busy")),
            other => panic!("{other:?}"),
        }
        assert_eq!(client.windows().unwrap(), vec![], "still in step");
    }

    /// #EXP-1994, this client against an OLDER compositor: a hello without
    /// features, an op it cannot read (answered without the call's id) is
    /// `Unsupported`, never a matched text; the connection goes on.
    #[cfg(unix)]
    #[test]
    fn a_newer_client_keeps_working_with_an_older_compositor() {
        /// What an older compositor reads a call into: no clipboard ops.
        #[derive(Deserialize)]
        #[serde(tag = "op", rename_all = "snake_case")]
        #[allow(dead_code)]
        enum OldRequest {
            ListWindows,
            FocusWindow { window: WindowId },
        }
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct OldCall {
            id: u64,
            #[serde(flatten)]
            request: OldRequest,
        }
        let hello = serde_json::json!({"version": PROTOCOL_VERSION, "capabilities": ["capture", "input:person", "ui"], "session_id": "epilot"});
        let mut client = compositor(hello, |call| match serde_json::from_value::<OldCall>(call.clone()) {
            Ok(OldCall { id, request: OldRequest::ListWindows }) => vec![serde_json::json!({"type": "reply", "id": id, "result": []})],
            Ok(OldCall { id, .. }) => vec![serde_json::json!({"type": "error", "id": id, "error": Failure::new(ErrorCode::NotFound, "no such window")})],
            // As the compositor does: it never read the call, so no id.
            Err(e) => vec![serde_json::to_value(Message::Error { id: 0, error: Failure::new(ErrorCode::Invalid, &e.to_string()) }).unwrap()],
        });
        assert!(!client.hello.offers_spaces() && client.hello.unknown_capabilities.is_empty());
        let unsupported = client.clipboard_serial().unwrap_err();
        assert!(matches!(unsupported, Error::Unsupported(_)), "{unsupported:?}");
        assert!(!unsupported.has_code(ErrorCode::Invalid), "a refusal is a call it read");
        let missing = client.focus(WindowId::new("w9").unwrap()).unwrap_err();
        assert!(missing.has_code(ErrorCode::NotFound), "{missing:?}");
        assert_eq!(client.windows().unwrap(), vec![], "still in step");
    }

    /// #EDE-126: what a space grant may ask at all. Every variant is here:
    /// `in_space` has no wildcard arm, and neither has `listed`.
    #[test]
    fn the_in_space_table() {
        let w = || WindowId::new("w1").unwrap();
        let space = || SpaceId::new("space-1").unwrap();
        let key = InputEvent::Key { key: 30, pressed: true };
        let table = [
            (Request::Hello { version: 1, token: Some(Token::new("0".repeat(64)).unwrap()) }, true),
            (Request::Hello { version: 1, token: None }, true),
            (Request::ListWindows, true),
            (Request::ListOutputs, true),
            (Request::FocusWindow { window: w() }, true),
            (Request::CloseWindow { window: w() }, true),
            (Request::MoveWindow { window: w(), x: 0, y: 0 }, false),
            (Request::MinimizeWindow { window: w() }, false),
            (Request::RestoreWindow { window: w() }, false),
            (Request::Capture { target: CaptureTarget::Output { name: "space-1".into() }, since: None }, true),
            (Request::Capture { target: CaptureTarget::Space { space: space() }, since: Some(3) }, true),
            (Request::Capture { target: CaptureTarget::Window { window: w() }, since: None }, false),
            (Request::Input { event: key.clone(), bindings: false }, true),
            (Request::Input { event: key, bindings: true }, false),
            (Request::PointerPosition, true),
            (Request::HoldInput, false),
            (Request::ReleaseInput, false),
            (Request::ReloadConfig, false),
            (Request::Lock, false),
            (Request::Logout, false),
            (Request::Unlock, false),
            (Request::ListWorkspaces, false),
            (Request::SwitchWorkspace { index: 0 }, false),
            (Request::MoveToWorkspace { window: w(), index: 0 }, false),
            (Request::SetOutput { name: "space-1".into(), mode: None, scale: None, position: None }, false),
            (Request::Subscribe, false),
            (Request::MintGrant { session_id: SessionId::new("run-1").unwrap(), run_id: None, capabilities: vec![Capability::Space], expires_at: None }, false),
            (Request::RevokeGrant { token: Token::new("0".repeat(64)).unwrap() }, false),
            (Request::BindShortcuts { owner: "p".into(), shortcuts: vec![] }, false),
            (Request::UnbindShortcuts { owner: "p".into() }, false),
            (Request::ClipboardSerial, true),
            (Request::ClipboardSnapshot, true),
            (Request::ClipboardSet { items: vec![], sensitive: false }, true),
            (Request::ClipboardTaken { serial: 1 }, true),
            (Request::ClipboardText, true),
            (Request::LaunchToken, false),
            (Request::CreateSpace { width: 1280, height: 800 }, true),
            (Request::DestroySpace { space: space() }, true),
            (Request::ListSpaces, true),
            (Request::Launch { argv: vec!["foot".into()], env: Default::default(), cwd: None }, true),
            (Request::SpaceOfProcess { pid: 42 }, false),
            (Request::SpaceReadiness, false),
            (Request::SpaceBusy, true),
            (Request::OpenChannel { session_id: None, capabilities: vec![Capability::Space], expires_at: None }, false),
            (Request::OpenWayland, false),
            (Request::OpenServices { capabilities: vec![Capability::System] }, false),
        ];
        let mut seen = std::collections::BTreeSet::new();
        for (request, allowed) in &table {
            assert_eq!(request.in_space(), *allowed, "{request:?}");
            seen.insert(listed(request));
        }
        assert_eq!(seen.len(), 42, "one row per variant at least");
        // The new ops name `space`, or `system` where the person's side may ask.
        assert_eq!(Request::CreateSpace { width: 1, height: 1 }.capability(), Some(Capability::Space));
        assert_eq!(Request::Launch { argv: vec![], env: Default::default(), cwd: None }.capability(), Some(Capability::Space));
        assert_eq!(Request::DestroySpace { space: space() }.capability(), Some(Capability::System));
        assert_eq!(Request::ListSpaces.capability(), Some(Capability::System));
        assert_eq!(Request::SpaceOfProcess { pid: 42 }.capability(), None, "#EDE-172: any grant");
        // #EXP-1514: any person-scope grant asks it, as `list_windows`.
        assert_eq!(Request::SpaceReadiness.capability(), None);
        // D 10: a space grant's own word for its own space.
        assert_eq!(Request::SpaceBusy.capability(), Some(Capability::Space));
    }

    /// The op's name; no wildcard, so a new variant fails to compile here
    /// until it has a row in [`the_in_space_table`].
    fn listed(request: &Request) -> &'static str {
        match request {
            Request::Hello { .. } => "hello",
            Request::ListWindows => "list_windows",
            Request::ListOutputs => "list_outputs",
            Request::FocusWindow { .. } => "focus_window",
            Request::CloseWindow { .. } => "close_window",
            Request::MoveWindow { .. } => "move_window",
            Request::MinimizeWindow { .. } => "minimize_window",
            Request::RestoreWindow { .. } => "restore_window",
            Request::Capture { .. } => "capture",
            Request::Input { .. } => "input",
            Request::PointerPosition => "pointer_position",
            Request::HoldInput => "hold_input",
            Request::ReleaseInput => "release_input",
            Request::ReloadConfig => "reload_config",
            Request::Lock => "lock",
            Request::Logout => "logout",
            Request::Unlock => "unlock",
            Request::ListWorkspaces => "list_workspaces",
            Request::SwitchWorkspace { .. } => "switch_workspace",
            Request::MoveToWorkspace { .. } => "move_to_workspace",
            Request::SetOutput { .. } => "set_output",
            Request::Subscribe => "subscribe",
            Request::MintGrant { .. } => "mint_grant",
            Request::RevokeGrant { .. } => "revoke_grant",
            Request::BindShortcuts { .. } => "bind_shortcuts",
            Request::UnbindShortcuts { .. } => "unbind_shortcuts",
            Request::ClipboardSerial => "clipboard_serial",
            Request::ClipboardSnapshot => "clipboard_snapshot",
            Request::ClipboardSet { .. } => "clipboard_set",
            Request::ClipboardTaken { .. } => "clipboard_taken",
            Request::ClipboardText => "clipboard_text",
            Request::LaunchToken => "launch_token",
            Request::CreateSpace { .. } => "create_space",
            Request::DestroySpace { .. } => "destroy_space",
            Request::ListSpaces => "list_spaces",
            Request::Launch { .. } => "launch",
            Request::SpaceOfProcess { .. } => "space_of_process",
            Request::SpaceReadiness => "space_readiness",
            Request::SpaceBusy => "space_busy",
            Request::OpenChannel { .. } => "open_channel",
            Request::OpenWayland => "open_wayland",
            Request::OpenServices { .. } => "open_services",
        }
    }

    /// #EDE-172: a hello may wait longer than a call (the compositor holds
    /// it while the person answers a consent prompt); the calls after it
    /// wait [`CALL_TIMEOUT`] again.
    #[cfg(unix)]
    #[test]
    fn a_hello_may_wait_for_the_person() {
        let late_hello = |delay: std::time::Duration| {
            let (ours, mut theirs) = UnixStream::pair().unwrap();
            std::thread::spawn(move || {
                let Ok(frame) = read_frame(&mut theirs) else { return };
                let call: serde_json::Value = serde_json::from_slice(&frame).unwrap();
                std::thread::sleep(delay);
                let hello = Hello { version: PROTOCOL_VERSION, capabilities: vec![], session_id: SessionId::new("test").unwrap(), features: vec![], unknown_capabilities: vec![] };
                let _ = write_frame(&mut theirs, &serde_json::to_vec(&Message::Reply { id: call["id"].as_u64().unwrap(), result: serde_json::to_value(hello).unwrap() }).unwrap());
                let _ = read_frame(&mut theirs);
            });
            ours
        };
        let token = || Token::new("0".repeat(64)).unwrap();
        let ms = std::time::Duration::from_millis;
        assert!(Client::over_waiting(late_hello(ms(400)), token(), ms(100)).is_err(), "a hello past its wait fails");
        let client = Client::over_waiting(late_hello(ms(400)), token(), ms(3000)).expect("a hello within its wait");
        assert_eq!(client.call_timeout, CALL_TIMEOUT, "the calls after it wait as usual");
    }

    /// A peer that answers hello and then nothing, and never reads again.
    #[cfg(unix)]
    fn silent_after_hello() -> (UnixStream, UnixStream, Token) {
        let (ours, mut theirs) = UnixStream::pair().unwrap();
        let token = Token::new("0".repeat(64)).unwrap();
        let hello = Hello { version: PROTOCOL_VERSION, capabilities: vec![], session_id: SessionId::new("test").unwrap(), features: vec![], unknown_capabilities: vec![] };
        let reply = Message::Reply { id: 1, result: serde_json::to_value(hello).unwrap() };
        write_frame(&mut theirs, &serde_json::to_vec(&reply).unwrap()).unwrap();
        (ours, theirs, token)
    }

    #[cfg(unix)]
    const SHORT: std::time::Duration = std::time::Duration::from_millis(200);

    #[cfg(unix)]
    fn finishes<R: Send + 'static>(what: &str, call: impl FnOnce() -> R + Send + 'static) -> R {
        let handle = std::thread::spawn(call);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !handle.is_finished() {
            assert!(std::time::Instant::now() < deadline, "{what} blocked");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        handle.join().unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn a_call_nobody_answers_fails_instead_of_blocking() {
        // No hello back at all (a compositor that hangs before it accepts).
        let (ours, _theirs) = UnixStream::pair().unwrap();
        let token = Token::new("0".repeat(64)).unwrap();
        assert!(finishes("hello", move || Client::over_within(ours, token, SHORT).is_err()));
        // Hello answered, the next call never.
        let (ours, _theirs, token) = silent_after_hello();
        let failed = finishes("a call", move || {
            let mut client = Client::over_within(ours, token, SHORT).unwrap();
            let first = client.windows().is_err();
            // The stream is out of step after a timeout: it stays failed.
            first && client.windows().is_err()
        });
        assert!(failed);
    }

    /// #EXP-1514: one deadline over the hello and every call after it, not
    /// [`CALL_TIMEOUT`] per step: a desktop that does not answer, or
    /// answers the hello and then nothing, fails inside it.
    #[cfg(unix)]
    #[test]
    fn connect_within_keeps_one_deadline() {
        let dir = std::env::temp_dir().join(format!("ede-within-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bind = |name: &str| {
            let socket = dir.join(name);
            let _ = std::fs::remove_file(&socket);
            (std::os::unix::net::UnixListener::bind(&socket).unwrap(), socket)
        };
        let token = Token::new("0".repeat(64)).unwrap();
        // Accepted by the kernel, never answered (a stopped compositor).
        let (_stopped, socket) = bind("stopped.sock");
        let started = std::time::Instant::now();
        assert!(finishes("a silent hello", { let token = token.clone(); move || Client::connect_within(&socket, token, SHORT).is_err() }));
        assert!(started.elapsed() < SHORT * 5, "{:?}", started.elapsed());
        // The hello answered, the call never: the call fails by the same deadline.
        let (listener, socket) = bind("silent.sock");
        let peer = std::thread::spawn(move || {
            let (mut theirs, _) = listener.accept().unwrap();
            let _ = read_frame(&mut theirs);
            let hello = Hello { version: PROTOCOL_VERSION, capabilities: vec![], session_id: SessionId::new("test").unwrap(), features: vec![FEATURE_SPACE_READINESS.into()], unknown_capabilities: vec![] };
            write_frame(&mut theirs, &serde_json::to_vec(&Message::Reply { id: 1, result: serde_json::to_value(hello).unwrap() }).unwrap()).unwrap();
            // Read the call, answer nothing, until the client lets go.
            while read_frame(&mut theirs).is_ok() {}
        });
        let started = std::time::Instant::now();
        let failed = finishes("a silent answer", move || {
            let mut client = Client::connect_within(&socket, token, SHORT * 2).unwrap();
            assert!(client.hello.says_space_readiness());
            client.space_readiness().is_err()
        });
        assert!(failed && started.elapsed() < SHORT * 5, "{:?}", started.elapsed());
        peer.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_call_nobody_reads_fails_instead_of_blocking() {
        let (ours, _theirs, token) = silent_after_hello();
        let failed = finishes("a write", move || {
            let mut client = Client::over_within(ours, token, SHORT).unwrap();
            client.clipboard_set(vec![ClipboardItem { mime: "a".into(), data: vec![0; 4 << 20] }], false).is_err()
        });
        assert!(failed);
    }

    /// #EDE-172: the channel ops on the wire. A hello without a token
    /// leaves the field out; `revoke_grant` takes a bound grant's id as
    /// `grant` too.
    #[test]
    fn channel_ops_are_flat_json_and_round_trip() {
        let token = || Token::new("ab".repeat(32)).unwrap();
        for (request, json) in [
            (Request::Hello { version: 1, token: None }, serde_json::json!({"id": 1, "op": "hello", "version": 1})),
            (Request::Hello { version: 1, token: Some(token()) }, serde_json::json!({"id": 1, "op": "hello", "version": 1, "token": "ab".repeat(32)})),
            (Request::OpenChannel { session_id: None, capabilities: vec![Capability::Ui], expires_at: None }, serde_json::json!({"id": 1, "op": "open_channel", "capabilities": ["ui"]})),
            (
                Request::OpenChannel { session_id: Some(SessionId::new("run-1").unwrap()), capabilities: vec![Capability::Capture, Capability::InputPerson], expires_at: Some(9) },
                serde_json::json!({"id": 1, "op": "open_channel", "session_id": "run-1", "capabilities": ["capture", "input:person"], "expires_at": 9}),
            ),
            (Request::RevokeGrant { token: token() }, serde_json::json!({"id": 1, "op": "revoke_grant", "token": "ab".repeat(32)})),
            (Request::OpenWayland, serde_json::json!({"id": 1, "op": "open_wayland"})),
            (Request::OpenServices { capabilities: vec![Capability::Ui, Capability::System] }, serde_json::json!({"id": 1, "op": "open_services", "capabilities": ["ui", "system"]})),
        ] {
            let call = Call { id: 1, request };
            assert_eq!(serde_json::to_value(&call).unwrap(), json);
            assert_eq!(serde_json::from_value::<Call>(json).unwrap(), call);
        }
        let by_id: Call = serde_json::from_value(serde_json::json!({"id": 2, "op": "revoke_grant", "grant": "ab".repeat(32)})).unwrap();
        assert_eq!(by_id.request, Request::RevokeGrant { token: token() });
        assert_eq!(Request::OpenChannel { session_id: None, capabilities: vec![], expires_at: None }.capability(), None, "any grant; the arm checks what it holds");
        assert_eq!(Request::RevokeGrant { token: token() }.capability(), None, "system, or the grant's own lineage: the arm checks");
    }

    /// One frame written with `fd` riding on its first byte, as the
    /// compositor sends an `open_channel` reply.
    #[cfg(unix)]
    fn send_with_fd(stream: &UnixStream, message: &serde_json::Value, fd: Option<&OwnedFd>) {
        use rustix::net::{SendAncillaryBuffer, SendAncillaryMessage, SendFlags};
        use std::os::fd::AsFd;
        let json = serde_json::to_vec(message).unwrap();
        let mut frame = (json.len() as u32).to_le_bytes().to_vec();
        frame.extend_from_slice(&json);
        let mut space = [std::mem::MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
        let mut control = SendAncillaryBuffer::new(&mut space);
        let fds = fd.map(|fd| [fd.as_fd()]);
        if let Some(fds) = &fds {
            assert!(control.push(SendAncillaryMessage::ScmRights(fds)));
        }
        let sent = rustix::net::sendmsg(stream, &[std::io::IoSlice::new(&frame)], &mut control, SendFlags::empty()).unwrap();
        assert_eq!(sent, frame.len());
    }

    /// A compositor's side of a connection that already holds its grant:
    /// it answers the tokenless hello, naming the grant, then `answer` per call.
    #[cfg(unix)]
    fn bound_peer(theirs: UnixStream, grant: &str, answer: impl Fn(&UnixStream, &serde_json::Value) + Send + 'static) -> std::thread::JoinHandle<()> {
        let grant = grant.to_string();
        std::thread::spawn(move || {
            let mut theirs = theirs;
            let Ok(frame) = read_frame(&mut theirs) else { return };
            let call: serde_json::Value = serde_json::from_slice(&frame).unwrap();
            assert_eq!(call["op"], "hello");
            assert!(call.get("token").is_none(), "no token over a bound connection: {call}");
            let hello = serde_json::json!({"version": PROTOCOL_VERSION, "capabilities": ["ui"], "session_id": "apps", "grant": grant});
            send_with_fd(&theirs, &serde_json::json!({"type": "reply", "id": call["id"], "result": hello}), None);
            while let Ok(frame) = read_frame(&mut theirs) {
                answer(&theirs, &serde_json::from_slice(&frame).unwrap());
            }
        })
    }

    /// #EDE-172: an `open_channel` reply brings its descriptor along, also
    /// when an event was read in the same go before it; the client says a
    /// tokenless hello over it and holds the grant the reply names. A
    /// reply without a descriptor is a protocol error, never a connection.
    #[cfg(unix)]
    #[test]
    fn a_descriptor_comes_with_its_reply_frame() {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let child_id = "c".repeat(64);
        let peer = bound_peer(theirs, &"e".repeat(64), {
            let child_id = child_id.clone();
            move |theirs, call| {
                let id = call["id"].clone();
                match call["op"].as_str() {
                    Some("open_channel") if call["session_id"] == "apps" => {
                        let (channel, other_end) = UnixStream::pair().unwrap();
                        let _ = bound_peer(channel, &child_id, |_, _| {});
                        send_with_fd(theirs, &serde_json::json!({"type": "event", "event": Envelope::new("ev-1", events::HOTKEY, 1, EVENT_SOURCE, None, serde_json::json!({"name": "menu.toggle"}))}), None);
                        let grant = serde_json::json!({"token": child_id, "sessionId": "apps", "capabilities": ["ui", "lend:window"]});
                        send_with_fd(theirs, &serde_json::json!({"type": "reply", "id": id, "result": grant}), Some(&OwnedFd::from(other_end)));
                    }
                    _ => send_with_fd(theirs, &serde_json::json!({"type": "reply", "id": id, "result": {"token": "d".repeat(64), "sessionId": "x", "capabilities": []}}), None),
                }
            }
        });
        let mut client = Client::from_fd(OwnedFd::from(ours)).unwrap();
        assert_eq!(client.token().as_str(), "e".repeat(64), "the grant the reply named");
        assert_eq!(client.hello.session_id.as_str(), "apps");
        let (child, grant) = client.open_channel(Some(SessionId::new("apps").unwrap()), vec![Capability::Ui], None).unwrap();
        assert_eq!((grant.token.as_str(), grant.capabilities.as_slice()), (child_id.as_str(), &[Capability::Ui][..]), "a capability this build does not know is dropped, as mint's");
        assert_eq!(child.token().as_str(), child_id, "the new connection said hello over the descriptor");
        assert_eq!(client.next_event().unwrap().kind, events::HOTKEY, "the event before the reply is kept");
        let missing = client.open_channel(Some(SessionId::new("other").unwrap()), vec![Capability::Ui], None).err().unwrap();
        assert!(matches!(missing, Error::Protocol(_)), "{missing:?}");
        drop(client);
        drop(child);
        peer.join().unwrap();
    }

    /// #EDE-172: what an anonymous connection may ask. Every variant has a
    /// row: `anonymous` has no wildcard arm, and neither has `listed`.
    #[test]
    fn the_anonymous_table() {
        let w = || WindowId::new("w1").unwrap();
        let space = || SpaceId::new("space-1").unwrap();
        let token = || Token::new("0".repeat(64)).unwrap();
        let allowed = [Request::Hello { version: 1, token: None }, Request::ListOutputs, Request::SpaceReadiness];
        let refused = [
            Request::ListWindows,
            Request::FocusWindow { window: w() },
            Request::CloseWindow { window: w() },
            Request::MoveWindow { window: w(), x: 0, y: 0 },
            Request::MinimizeWindow { window: w() },
            Request::RestoreWindow { window: w() },
            Request::Capture { target: CaptureTarget::Output { name: "o".into() }, since: None },
            Request::Input { event: InputEvent::Key { key: 30, pressed: true }, bindings: false },
            Request::PointerPosition,
            Request::HoldInput,
            Request::ReleaseInput,
            Request::ReloadConfig,
            Request::Lock,
            Request::Logout,
            Request::Unlock,
            Request::ListWorkspaces,
            Request::SwitchWorkspace { index: 0 },
            Request::MoveToWorkspace { window: w(), index: 0 },
            Request::SetOutput { name: "o".into(), mode: None, scale: None, position: None },
            Request::Subscribe,
            Request::MintGrant { session_id: SessionId::new("x").unwrap(), run_id: None, capabilities: vec![], expires_at: None },
            Request::RevokeGrant { token: token() },
            Request::OpenChannel { session_id: None, capabilities: vec![], expires_at: None },
            Request::OpenWayland,
            Request::OpenServices { capabilities: vec![Capability::System] },
            Request::BindShortcuts { owner: "p".into(), shortcuts: vec![] },
            Request::UnbindShortcuts { owner: "p".into() },
            Request::ClipboardSerial,
            Request::ClipboardSnapshot,
            Request::ClipboardSet { items: vec![], sensitive: false },
            Request::ClipboardTaken { serial: 1 },
            Request::ClipboardText,
            Request::LaunchToken,
            Request::CreateSpace { width: 1, height: 1 },
            Request::DestroySpace { space: space() },
            Request::ListSpaces,
            Request::Launch { argv: vec![], env: Default::default(), cwd: None },
            Request::SpaceOfProcess { pid: 1 },
            Request::SpaceBusy,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for request in &allowed {
            assert!(request.anonymous(), "{request:?}");
            seen.insert(listed(request));
        }
        for request in &refused {
            assert!(!request.anonymous(), "{request:?}");
            seen.insert(listed(request));
        }
        assert_eq!(seen.len(), 42, "every op has a row");
    }

    /// #EDE-172: only a socket is taken as the inherited control connection.
    #[cfg(unix)]
    #[test]
    fn an_inherited_descriptor_must_be_a_socket() {
        use std::os::fd::IntoRawFd;
        let file = std::fs::File::open("/dev/null").unwrap();
        let raw = file.into_raw_fd();
        std::env::set_var(CONTROL_FD_ENV, raw.to_string());
        let refused = Client::inherited().expect("named").err().unwrap();
        assert!(matches!(refused, Error::Protocol(_)), "{refused:?}");
        assert!(std::env::var_os(CONTROL_FD_ENV).is_none(), "the variable goes with the first look");
        // Not closed by the refusal: still this test's to close.
        // Safety: `raw` came from `into_raw_fd` above and is closed once here.
        drop(unsafe { <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(raw) });
        assert!(Client::inherited().is_none(), "nothing named any more");
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
