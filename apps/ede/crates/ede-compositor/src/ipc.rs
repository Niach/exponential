//! The compositor's end of `ede-protocol` (#NEW-104): the socket, the
//! grants, and every op. Calls are served on the event loop, one frame at a
//! time. Nothing written to a client blocks the loop (#EDE-74): what its
//! socket does not take waits in the connection's own queue, its next call
//! is served once that queue is empty, and a client that lets [`OUT_MAX`]
//! writes pile up (it stopped reading its events) is dropped.
//!
//! #EDE-172, channels: a connection can hold its grant from the start (one
//! the compositor made: [`Ede::ipc_insert_channel`], `--control-fd`, an
//! `open_channel` reply's descriptor) and says hello without a token. A
//! child opened on a channel is bound: its token is never taken at hello
//! (the reply carries it as the grant's id), and its connections end when
//! it is revoked, or when its parent is (or loses its last connection).
//! Until eDE's flip the root grant stays a bearer in its file.
//!
//! #EDE-172 (P2): a hello without a token on a socket connection is
//! anonymous: it holds no grant and asks only what
//! [`Request::anonymous`] lets it. Every socket connection's peer is taken
//! as it comes in ([`crate::peer::Peer`]).
//!
//! #EDE-172 (P5', design v3): with the gate on, the root token is honoured
//! by who presents it ([`crate::consent`]); a connection the person allowed
//! acts as the `computer-use` principal.

use std::{
    io::{Read, Write},
    os::{
        fd::OwnedFd,
        unix::{
            fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use ede_protocol::{
    events, take_frames, Call, Capability, CaptureInfo, CaptureTarget, Envelope, ErrorCode, Failure, Grant, Hello, InputEvent, Message,
    OutputInfo, PointerInfo, Request, SessionId, Token, WindowId, WindowInfo, EVENT_SOURCE, MAX_FRAME, PROTOCOL_VERSION,
};
use smithay::{
    backend::input::{ButtonState, KeyState},
    desktop::Window,
    reexports::calloop::{generic::Generic, Interest, Mode, PostAction},
};

use crate::{
    consent::{Key, Principal, Verdict, Waiter},
    desk::DeskId,
    spaces::scope::{Scope, Scoped},
    state::Ede,
};

/// The writes (a frame's length, its bytes) one connection may have
/// waiting. Replies never pile up (a call waits for an empty queue, however
/// large the capture in it), so only unread events reach this.
const OUT_MAX: usize = 1024;

/// The root grant's session: the shell.
pub const ROOT_SESSION: &str = "exponential-shell";

/// #EDE-172: what an anonymous connection's hello names as its session.
pub const ANONYMOUS_SESSION: &str = "anonymous";

#[derive(Debug, Default)]
pub struct Ipc {
    pub grants: Vec<Grant>,
    /// #EDE-172: the bound grants (by their token, which is their id) and
    /// the grant each was opened on.
    bound: std::collections::HashMap<Token, Bound>,
    /// #EDE-172 (P5'): who may present the root token, and what the person
    /// answered this login.
    pub consent: crate::consent::Gate,
    /// The grants the `computer-use` principal minted or opened, and the
    /// sessions of its space grants: its lineage (one per person).
    cu_grants: std::collections::HashSet<Token>,
    cu_sessions: std::collections::HashSet<SessionId>,
    /// The gate's channel on the loop, made on first use.
    consent_tx: Option<smithay::reexports::calloop::channel::Sender<ConsentEvent>>,
    clients: Vec<Conn>,
    next_client: u64,
    next_event: u64,
    /// The socket and token file, removed on exit.
    pub paths: Vec<PathBuf>,
    /// The portal's global shortcuts (#EDE-15).
    pub shortcuts: crate::shortcuts::Shortcuts,
}

/// #EDE-172 (P5'): what the gate hears on the loop.
#[derive(Debug)]
pub(crate) enum ConsentEvent {
    /// An executable's hash, read off the loop: every hello waiting for
    /// that file ([`crate::consent::Gate::hashing`]) goes on with it.
    Hashed { file: crate::consent::FileKey, hash: Result<[u8; 32], String> },
    /// The person's answer to the question `key`.
    Answered { key: Key, allowed: bool },
}

/// A bound grant (#EDE-172): never taken by its token, it lives on its
/// connections.
#[derive(Debug, Clone)]
struct Bound {
    /// The grant it was opened on; it ends with that one.
    parent: Token,
}

/// One write a connection has waiting: bytes, and a descriptor that goes
/// with their first byte (an `open_channel` reply's).
#[derive(Debug)]
struct Out {
    bytes: Vec<u8>,
    fd: Option<OwnedFd>,
}

#[derive(Debug)]
struct Conn {
    id: u64,
    writer: UnixStream,
    /// #EDE-172: it holds its grant (`token`) from the start: its hello
    /// names none, and may not name another.
    channel: bool,
    /// The descriptor the reply being made goes out with.
    pending_fd: Option<OwnedFd>,
    /// #EDE-172: it said hello without a token and holds no grant.
    anonymous: bool,
    /// Who connected it (a socket's peer; `None` for a channel the
    /// compositor made), taken before its first call.
    peer: Option<crate::peer::Peer>,
    /// #EDE-172 (P5'): what a root hello on it acts as.
    principal: Principal,
    inbuf: Vec<u8>,
    /// Whole frames not served yet: they wait while a call's reply is
    /// deferred (a clipboard read), so replies keep the calls' order.
    queued: std::collections::VecDeque<Vec<u8>>,
    /// The call whose reply is deferred.
    busy: Option<u64>,
    token: Option<Token>,
    /// Whose desk it acts on: its first hello's, never changed (#EDE-126).
    scope: Option<Scope>,
    subscribed: bool,
    dead: bool,
    /// What the socket did not take yet; the first one is sent up to `out_at`.
    out: std::collections::VecDeque<Out>,
    out_at: usize,
    /// A writable source drains `out`.
    flushing: bool,
}

impl Conn {
    fn kill(&mut self) {
        self.dead = true;
        self.out.clear();
        let _ = self.writer.shutdown(std::net::Shutdown::Both);
    }

    /// Writes what the socket takes now. `Ok(true)` = nothing is left.
    fn write_out(&mut self) -> std::io::Result<bool> {
        while let Some(front) = self.out.front_mut() {
            let wrote = match front.fd.take() {
                // Its first byte carries the descriptor; once a byte went,
                // so did the descriptor (ours closes here).
                Some(fd) => send_with_fd(&self.writer, &front.bytes[self.out_at..], &fd).inspect_err(|_| front.fd = Some(fd)),
                None => (&self.writer).write(&front.bytes[self.out_at..]),
            };
            let len = front.bytes.len();
            match wrote {
                Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
                Ok(n) if self.out_at + n == len => {
                    self.out.pop_front();
                    self.out_at = 0;
                }
                Ok(n) => self.out_at += n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(false),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(true)
    }
}

/// `bytes` with `fd` (`SCM_RIGHTS`) on their first byte.
fn send_with_fd(stream: &UnixStream, bytes: &[u8], fd: &OwnedFd) -> std::io::Result<usize> {
    use smithay::reexports::rustix::net::{sendmsg, SendAncillaryBuffer, SendAncillaryMessage, SendFlags};
    use std::os::fd::AsFd;
    let mut space = [std::mem::MaybeUninit::uninit(); smithay::reexports::rustix::cmsg_space!(ScmRights(1))];
    let mut control = SendAncillaryBuffer::new(&mut space);
    let fds = [fd.as_fd()];
    if !control.push(SendAncillaryMessage::ScmRights(&fds)) {
        return Err(std::io::Error::other("no room for the descriptor"));
    }
    Ok(sendmsg(stream, &[std::io::IoSlice::new(bytes)], &mut control, SendFlags::NOSIGNAL | SendFlags::DONTWAIT)?)
}

impl Drop for Ipc {
    fn drop(&mut self) {
        for p in &self.paths {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// #EDE-172 (3.6), `--control-fd N`: the connection a test's parent handed
/// over; it holds the root grant once the socket listens.
static CONTROL_FD: std::sync::Mutex<Option<OwnedFd>> = std::sync::Mutex::new(None);
/// `--test-token-file PATH` (headless and nested only): the root token is
/// written there too (0600), for scripts that drive a test compositor.
static TEST_TOKEN_FILE: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// `--consent-answer allow|deny` (headless and nested only): the gate is on
/// and this is the person's answer to every question, without a prompt.
static CONSENT_ANSWER: std::sync::Mutex<Option<bool>> = std::sync::Mutex::new(None);

/// `--consent-answer`: see [`CONSENT_ANSWER`].
pub fn answer_consent_with(allow: bool) {
    *CONSENT_ANSWER.lock().unwrap_or_else(|p| p.into_inner()) = Some(allow);
}

/// `--control-fd N`: `fd` becomes a root channel when the socket listens.
pub fn hand_control_fd(fd: OwnedFd) {
    *CONTROL_FD.lock().unwrap_or_else(|p| p.into_inner()) = Some(fd);
}

/// `--test-token-file PATH`: the root token goes to `path` too.
pub fn write_test_token_to(path: PathBuf) {
    *TEST_TOKEN_FILE.lock().unwrap_or_else(|p| p.into_inner()) = Some(path);
}

/// A root token file at `path`, 0600, written anew.
fn write_token_file(path: &Path, token: &Token) -> std::io::Result<()> {
    let _ = std::fs::remove_file(path);
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
    f.write_all(token.as_str().as_bytes())
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// 32 random bytes as eCore's 64-hex token.
pub fn new_token() -> Token {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).expect("/dev/urandom");
    Token::new(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()).expect("64 hex")
}

fn fail(code: ErrorCode, message: &str) -> Failure {
    Failure::new(code, message)
}

impl Ipc {
    /// The grant with every capability, for the shell.
    pub fn root_grant(&mut self) -> Grant {
        if let Some(g) = self.grants.iter().find(|g| g.session_id.as_str() == ROOT_SESSION) {
            return g.clone();
        }
        let grant = Grant {
            token: new_token(),
            session_id: SessionId::new(ROOT_SESSION).unwrap(),
            run_id: None,
            capabilities: Capability::ALL.to_vec(),
            expires_at: None,
        };
        self.grants.push(grant.clone());
        grant
    }

    fn grant(&self, token: &Token) -> Option<&Grant> {
        self.grants.iter().find(|g| &g.token == token)
    }

    /// #EDE-172: a bound grant's token is its id, never a way in.
    pub fn is_bound(&self, token: &Token) -> bool {
        self.bound.contains_key(token)
    }

    /// `root` and every grant opened on it, at any depth (`root` first).
    fn lineage(&self, root: &Token) -> Vec<Token> {
        let mut out = vec![root.clone()];
        let mut i = 0;
        while i < out.len() {
            let parent = out[i].clone();
            out.extend(self.bound.iter().filter(|(_, b)| b.parent == parent).map(|(t, _)| t.clone()));
            i += 1;
        }
        out
    }

    /// Ends `tokens`' grants and kills the connections holding them (the
    /// connections' own drop releases what they held). Bound entries of
    /// grants that are gone otherwise (expired) go too.
    fn end_grants(&mut self, tokens: &[Token]) {
        self.grants.retain(|g| !tokens.contains(&g.token));
        self.bound.retain(|t, _| !tokens.contains(t));
        let live: std::collections::HashSet<Token> = self.grants.iter().map(|g| g.token.clone()).collect();
        self.bound.retain(|t, _| live.contains(t));
        for c in self.clients.iter_mut().filter(|c| c.channel && c.token.as_ref().is_some_and(|t| tokens.contains(t))) {
            c.kill();
        }
    }

    /// Ends the bound grants opened on `parent` and theirs, with their
    /// connections; `parent` itself stays.
    fn end_children(&mut self, parent: &Token) {
        let children: Vec<Token> = self.lineage(parent).into_iter().skip(1).collect();
        if !children.is_empty() {
            self.end_grants(&children);
        }
    }

    pub(crate) fn next_event_id(&mut self) -> u64 {
        self.next_event += 1;
        self.next_event
    }

    /// A grant the compositor hands a process it starts (the sealed lock).
    pub fn mint_for(&mut self, session: &str, capabilities: Vec<Capability>) -> Grant {
        let grant = Grant { token: new_token(), session_id: SessionId::new(session).unwrap(), run_id: None, capabilities, expires_at: None };
        self.grants.push(grant.clone());
        grant
    }

    /// Is the shell (the root grant) subscribed to events? Hotkeys go to it
    /// then; otherwise the compositor falls back ([`crate::hotkeys`]).
    pub fn shell_subscribed(&self) -> bool {
        self.clients.iter().any(|c| c.subscribed && !c.dead && c.token.as_ref().and_then(|t| self.grant(t)).is_some_and(|g| g.session_id.as_str() == ROOT_SESSION))
    }
}

impl Ede {
    pub fn shell_listening(&self) -> bool {
        self.ipc.shell_subscribed()
    }

    /// Opens `$XDG_RUNTIME_DIR/exponential-desktop/<wayland-N>.sock` (directory 0700) and
    /// writes the root token next to it (0600).
    pub fn ipc_listen(&mut self, runtime_dir: &Path, wayland_display: &str) -> std::io::Result<PathBuf> {
        let dir = runtime_dir.join("exponential-desktop");
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        let socket = ede_protocol::socket_path(runtime_dir, wayland_display);
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket)?;
        let token_file = ede_protocol::token_path(runtime_dir, wayland_display);
        let root = self.ipc.root_grant().token;
        write_token_file(&token_file, &root)?;
        self.ipc.paths = vec![socket.clone(), token_file];
        if let Some(path) = TEST_TOKEN_FILE.lock().unwrap_or_else(|p| p.into_inner()).take() {
            write_token_file(&path, &root)?;
            self.ipc.paths.push(path);
        }
        // #EDE-172 (P5'): the gate, from the config read at the start (a
        // later edit of the person's file does not turn it off).
        let answer = *CONSENT_ANSWER.lock().unwrap_or_else(|p| p.into_inner());
        self.ipc.consent.on = self.config.control.consent || answer.is_some();
        self.ipc.consent.answer_with = answer;
        if let Some(fd) = CONTROL_FD.lock().unwrap_or_else(|p| p.into_inner()).take() {
            if !self.ipc_insert_root_channel(UnixStream::from(fd)) {
                tracing::warn!("--control-fd: the connection could not be served");
            }
        }

        listener.set_nonblocking(true)?;
        self.loop_handle
            .insert_source(Generic::new(listener, Interest::READ, Mode::Level), |_, listener, state| {
                while let Ok((stream, _)) = listener.accept() {
                    state.ipc_insert(stream);
                }
                Ok(PostAction::Continue)
            })
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(socket)
    }

    /// Serves one connection (the socket's, or a test's socketpair); its
    /// peer is taken now (#EDE-172).
    pub fn ipc_insert(&mut self, stream: UnixStream) {
        let peer = crate::peer::Peer::of_socket(std::os::fd::AsFd::as_fd(&stream));
        if let Some(id) = self.ipc_insert_holding(stream, None) {
            if let Some(c) = self.ipc.clients.iter_mut().find(|c| c.id == id) {
                c.peer = peer;
            }
        }
    }

    /// #EDE-172: the peer of connection `conn` as it connected.
    pub fn ipc_peer(&self, conn: u64) -> Option<&crate::peer::Peer> {
        self.ipc.clients.iter().find(|c| c.id == conn).and_then(|c| c.peer.as_ref())
    }

    /// The peers of the live socket connections: pid and executable.
    pub fn ipc_peers(&self) -> Vec<(u32, Option<PathBuf>)> {
        self.ipc.clients.iter().filter(|c| !c.dead).filter_map(|c| c.peer.as_ref()).map(|p| (p.pid, p.exe_path())).collect()
    }

    /// #EDE-172: serves a connection that holds the grant `token` from the
    /// start (a channel): its hello names no token. `false` = no such grant,
    /// or the connection could not be served.
    pub fn ipc_insert_channel(&mut self, stream: UnixStream, token: &Token) -> bool {
        if self.ipc.grant(token).is_none() {
            return false;
        }
        self.ipc_insert_holding(stream, Some(token.clone())).is_some()
    }

    /// #EDE-172 (`--control-fd`, a test's channel): a connection holding
    /// the root grant from the start.
    pub fn ipc_insert_root_channel(&mut self, stream: UnixStream) -> bool {
        let root = self.ipc.root_grant().token;
        self.ipc_insert_channel(stream, &root)
    }

    fn ipc_insert_holding(&mut self, stream: UnixStream, held: Option<Token>) -> Option<u64> {
        if stream.set_nonblocking(true).is_err() {
            return None;
        }
        let Ok(reader) = stream.try_clone() else { return None };
        let id = self.ipc.next_client;
        self.ipc.next_client += 1;
        let scope = held.as_ref().and_then(|t| self.ipc.grant(t)).map(Scope::of);
        self.ipc.clients.push(Conn { id, writer: stream, channel: held.is_some(), pending_fd: None, anonymous: false, peer: None, principal: Principal::Root, inbuf: Vec::new(), queued: Default::default(), busy: None, token: held, scope, subscribed: false, dead: false, out: Default::default(), out_at: 0, flushing: false });
        let inserted = self.loop_handle.insert_source(Generic::new(reader, Interest::READ, Mode::Level), move |_, reader, state| {
            let mut buf = [0u8; 65536];
            let mut r: &UnixStream = reader;
            let n = match r.read(&mut buf) {
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => return Ok(PostAction::Continue),
                Ok(0) | Err(_) => 0,
                Ok(n) => n,
            };
            if n == 0 || !state.ipc_received(id, &buf[..n]) {
                state.ipc_dropped(id);
                return Ok(PostAction::Remove);
            }
            Ok(PostAction::Continue)
        });
        if inserted.is_err() {
            self.ipc.clients.retain(|c| c.id != id);
            return None;
        }
        Some(id)
    }

    /// A connection went; its hold and what it pressed go with it, the
    /// shell's last one starts the recovery clock.
    fn ipc_dropped(&mut self, id: u64) {
        let was_shell = self.ipc.shell_subscribed();
        // #EDE-126: the scope its hello gave it, read before it goes and
        // never from its grant: a foreground run revokes its grant before
        // its hold connection drops, and the hold must still end here.
        let scope = self.ipc.clients.iter().find(|c| c.id == id).and_then(|c| c.scope.clone()).unwrap_or(Scope::Person);
        let held = self.ipc.clients.iter().find(|c| c.id == id && c.channel).and_then(|c| c.token.clone());
        self.ipc.clients.retain(|c| c.id != id);
        // #EDE-172: a bound grant lives on its connections; with the last
        // one it ends, and what was opened on it with it.
        if let Some(token) = held.filter(|t| self.ipc.is_bound(t)) {
            if !self.ipc.clients.iter().any(|c| !c.dead && c.token.as_ref() == Some(&token)) {
                let lineage = self.ipc.lineage(&token);
                self.ipc.end_grants(&lineage);
            }
        }
        self.ipc.shortcuts.drop_client(id);
        if let Some(desk) = self.desk_of_scope(&scope) {
            self.release_driven(desk, id);
            self.release_input(desk, id);
        }
        if was_shell && !self.ipc.shell_subscribed() {
            self.shell_left();
        }
    }

    /// `false` = drop the connection.
    fn ipc_received(&mut self, id: u64, bytes: &[u8]) -> bool {
        {
            let Some(conn) = self.ipc.clients.iter_mut().find(|c| c.id == id) else { return false };
            conn.inbuf.extend_from_slice(bytes);
            match take_frames(&mut conn.inbuf) {
                Ok(f) => conn.queued.extend(f),
                Err(()) => return false,
            }
        }
        self.ipc_serve(id)
    }

    /// Serves the queued frames until one defers its reply or the socket
    /// is behind on the last one. `false` = drop the connection.
    fn ipc_serve(&mut self, id: u64) -> bool {
        loop {
            let frame = {
                let Some(conn) = self.ipc.clients.iter_mut().find(|c| c.id == id) else { return false };
                if conn.busy.is_some() || !conn.out.is_empty() {
                    break;
                }
                match conn.queued.pop_front() {
                    Some(frame) => frame,
                    None => break,
                }
            };
            let (call_id, outcome) = match serde_json::from_slice::<Call>(&frame) {
                // #EXP-1514: a compositor that makes no spaces reads the op as
                // an older one does (it cannot parse it): its hello does
                // not say `space-readiness`.
                Ok(Call { request: Request::SpaceReadiness, .. }) if !self.says_space_readiness() => (0, Err(fail(ErrorCode::Invalid, "unknown variant `space_readiness`"))),
                Ok(call) => {
                    // Busy before the call runs (#EDE-74): a deferred read
                    // that finishes inside it (the selection's owner is
                    // gone) answers through `ipc_reply_later` all the same.
                    if let Some(conn) = self.ipc.clients.iter_mut().find(|c| c.id == id) {
                        conn.busy = Some(call.id);
                    }
                    (call.id, self.ipc_call(id, call.id, call.request))
                }
                Err(e) => (0, Err(fail(ErrorCode::Invalid, &e.to_string()))),
            };
            let (message, extra) = match outcome {
                Ok(None) => continue,
                Ok(Some((result, extra))) => (Message::Reply { id: call_id, result }, extra),
                Err(error) => (Message::Error { id: call_id, error }, None),
            };
            // #EDE-172: an `open_channel` reply's descriptor goes with it.
            let mut fd = None;
            if let Some(conn) = self.ipc.clients.iter_mut().find(|c| c.id == id) {
                conn.busy = None;
                fd = conn.pending_fd.take().filter(|_| matches!(message, Message::Reply { .. }));
            }
            if !self.ipc_send_with(id, &message, extra, fd) {
                return false;
            }
        }
        self.ipc.clients.iter().any(|c| c.id == id && !c.dead)
    }

    /// The reply to a call [`Ede::ipc_call`] deferred; the connection's
    /// next calls are served after it.
    pub(crate) fn ipc_reply_later(&mut self, id: u64, call_id: u64, outcome: Result<serde_json::Value, Failure>) {
        self.ipc_reply_later_with(id, call_id, outcome.map(|result| (result, None)));
    }

    /// [`Self::ipc_reply_later`] with a capture's pixels (#EDE-162: a
    /// space's capture that waited its turn).
    pub(crate) fn ipc_reply_later_with(&mut self, id: u64, call_id: u64, outcome: Result<(serde_json::Value, Option<Vec<u8>>), Failure>) {
        let Some(conn) = self.ipc.clients.iter_mut().find(|c| c.id == id) else { return };
        if conn.busy != Some(call_id) {
            return;
        }
        conn.busy = None;
        let (message, extra) = match outcome {
            Ok((result, extra)) => (Message::Reply { id: call_id, result }, extra),
            Err(error) => (Message::Error { id: call_id, error }, None),
        };
        if !self.ipc_send(id, &message, extra) || !self.ipc_serve(id) {
            if let Some(conn) = self.ipc.clients.iter_mut().find(|c| c.id == id) {
                conn.kill();
            }
        }
    }

    /// Queues `message` (and a capture's pixels) and writes what the socket
    /// takes now; the rest goes out as it drains. `false` = the connection
    /// is gone (or was dropped: [`OUT_MAX`]).
    pub(crate) fn ipc_send(&mut self, id: u64, message: &Message, extra: Option<Vec<u8>>) -> bool {
        self.ipc_send_with(id, message, extra, None)
    }

    /// [`Self::ipc_send`] with `fd` on the message's first byte (#EDE-172).
    fn ipc_send_with(&mut self, id: u64, message: &Message, extra: Option<Vec<u8>>, mut fd: Option<OwnedFd>) -> bool {
        let Some(conn) = self.ipc.clients.iter_mut().find(|c| c.id == id && !c.dead) else { return false };
        let json = serde_json::to_vec(message).expect("messages serialize");
        if conn.out.len() >= OUT_MAX || json.len() > MAX_FRAME || extra.as_ref().is_some_and(|b| b.len() > MAX_FRAME) {
            conn.kill();
            return false;
        }
        for bytes in std::iter::once(json).chain(extra) {
            conn.out.push_back(Out { bytes: (bytes.len() as u32).to_le_bytes().to_vec(), fd: fd.take() });
            if !bytes.is_empty() {
                conn.out.push_back(Out { bytes, fd: None });
            }
        }
        match conn.write_out() {
            Ok(true) => return true,
            Ok(false) if conn.flushing => return true,
            Ok(false) => {}
            Err(_) => {
                conn.kill();
                return false;
            }
        }
        let writable = conn.writer.try_clone().map_err(|_| ()).and_then(|writer| {
            self.loop_handle.insert_source(Generic::new(writer, Interest::WRITE, Mode::Level), move |_, _, state| Ok(state.ipc_writable(id))).map_err(|_| ())
        });
        match writable {
            Ok(_) => conn.flushing = true,
            Err(()) => conn.kill(),
        }
        writable.is_ok()
    }

    /// The socket takes more: the queue goes on, and once it is empty the
    /// calls that waited behind it are served.
    fn ipc_writable(&mut self, id: u64) -> PostAction {
        let Some(conn) = self.ipc.clients.iter_mut().find(|c| c.id == id && !c.dead) else { return PostAction::Remove };
        match conn.write_out() {
            Ok(false) => return PostAction::Continue,
            Ok(true) => conn.flushing = false,
            Err(_) => {
                conn.kill();
                return PostAction::Remove;
            }
        }
        if !self.ipc_serve(id) {
            if let Some(conn) = self.ipc.clients.iter_mut().find(|c| c.id == id) {
                conn.kill();
            }
        }
        PostAction::Remove
    }

    /// Sends an event to every subscribed connection whose grant is still
    /// valid.
    pub fn emit(&mut self, kind: &str, data: serde_json::Value) {
        if !self.ipc.clients.iter().any(|c| c.subscribed) {
            return;
        }
        let envelope = Envelope::new(&format!("ev-{}", self.ipc.next_event_id()), kind, now_ms(), EVENT_SOURCE, None, data);
        let message = Message::Event { event: envelope };
        let now = now_ms();
        let ids: Vec<u64> = self
            .ipc
            .clients
            .iter()
            .filter(|c| c.subscribed && !c.dead)
            .filter(|c| c.token.as_ref().and_then(|t| self.ipc.grant(t)).is_some_and(|g| g.expires_at.is_none_or(|e| now < e)))
            .map(|c| c.id)
            .collect();
        for id in ids {
            self.ipc_send(id, &message, None);
        }
    }

    /// `Ok(None)` = the reply comes later ([`Ede::ipc_reply_later`]).
    fn ipc_call(&mut self, id: u64, call_id: u64, request: Request) -> Result<Option<(serde_json::Value, Option<Vec<u8>>)>, Failure> {
        let now = now_ms();
        let token = self.ipc.clients.iter().find(|c| c.id == id).and_then(|c| c.token.clone());
        let channel = self.ipc.clients.iter().any(|c| c.id == id && c.channel);
        let anonymous = self.ipc.clients.iter().any(|c| c.id == id && c.anonymous);
        if let Request::Hello { version, token: named } = &request {
            if *version != PROTOCOL_VERSION {
                return Err(fail(ErrorCode::Invalid, &format!("this compositor speaks ede-protocol v{PROTOCOL_VERSION}")));
            }
            // #EDE-172: a channel holds its grant already and names no
            // other; a bound grant is never taken by its token.
            let held = match (named, channel) {
                (None, true) => token.clone(),
                (Some(_), true) => return Err(fail(ErrorCode::Forbidden, "this connection holds its grant: say hello without a token")),
                // A hello without a token on the socket: anonymous (P2).
                (None, false) if token.is_some() => return Err(fail(ErrorCode::Forbidden, "this connection said hello with a token")),
                (None, false) => return Ok(Some((self.ipc_anonymous_hello(id), None))),
                (Some(_), false) if anonymous => return Err(fail(ErrorCode::Forbidden, "an anonymous connection stays anonymous: connect again with a token")),
                (Some(named), false) if self.ipc.is_bound(named) => return Err(fail(ErrorCode::Unauthorized, "a bound grant is held by its connections, never by its token")),
                (Some(named), false) => Some(named.clone()),
            };
            let grant = held.and_then(|t| self.ipc.grant(&t).cloned()).filter(|g| g.expires_at.is_none_or(|e| now < e));
            let Some(grant) = grant else { return Err(fail(ErrorCode::Unauthorized, "unknown or expired token")) };
            // #EDE-172 (P5'): the root token, presented on a socket
            // connection, goes through the gate: by who presents it.
            let principal = if named.is_some() && !channel && grant.session_id.as_str() == ROOT_SESSION && self.ipc.consent.on {
                match self.ipc_gate(id, call_id, None)? {
                    Some(principal) => principal,
                    None => return Ok(None),
                }
            } else {
                Principal::Root
            };
            return self.ipc_bind_hello(id, grant, principal, named.is_some()).map(|hello| Some((hello, None)));
        }
        // #EDE-172: an anonymous connection holds nothing: the allowlist.
        if anonymous {
            return match request {
                Request::ListOutputs => Ok(Some((serde_json::to_value(self.output_infos()).unwrap(), None))),
                Request::SpaceReadiness => Ok(Some((serde_json::to_value(self.space_readiness_reply()).unwrap(), None))),
                other => {
                    debug_assert!(!other.anonymous(), "{other:?} is on the allowlist and has no arm here");
                    Err(fail(ErrorCode::Forbidden, "an anonymous connection asks only list_outputs and space_readiness"))
                }
            };
        }
        let Some(grant) = token.and_then(|t| self.ipc.grant(&t).cloned()) else {
            return Err(fail(ErrorCode::Unauthorized, "say hello with a valid token first"));
        };
        if grant.expires_at.is_some_and(|e| now >= e) {
            return Err(fail(ErrorCode::Unauthorized, "the grant expired"));
        }
        // #EDE-172 (P5'): the person's yes to the app is computer use, not
        // the shell's power.
        let principal = self.ipc.clients.iter().find(|c| c.id == id).map_or(Principal::Root, |c| c.principal);
        if principal == Principal::ComputerUse {
            self.ipc_computer_use(&request)?;
        }
        // #EDE-126: a space connection never reaches the arms below.
        let (desk, request) = if let Some(Scope::Space(session)) = self.ipc.clients.iter().find(|c| c.id == id).and_then(|c| c.scope.clone()) {
            match self.space_scope_call(&session, &grant, now, request, crate::spaces::scope::Caller { conn: id, call: call_id })? {
                Scoped::Reply(reply) => return Ok(reply),
                // #EDE-125: the few ops whose arm below takes any desk.
                Scoped::Through(desk, request) => {
                    debug_assert!(matches!(request, Request::Input { bindings: false, .. } | Request::PointerPosition | Request::ClipboardSerial | Request::ClipboardSet { .. } | Request::ClipboardTaken { .. } | Request::ClipboardSnapshot | Request::ClipboardText));
                    (desk, request)
                }
            }
        } else {
            if let Some(cap) = request.capability() {
                if !grant.admits(cap, now) {
                    return Err(fail(ErrorCode::Forbidden, &format!("needs the {} capability", cap.as_str())));
                }
            }
            (self.desk_of_conn(id), request)
        };
        let json = |v: serde_json::Value| Ok(Some((v, None)));
        match request {
            Request::Hello { .. } => unreachable!(),
            Request::ListWindows => json(serde_json::to_value(self.window_infos()).unwrap()),
            Request::ListOutputs => json(serde_json::to_value(self.output_infos()).unwrap()),
            Request::Subscribe => {
                if let Some(c) = self.ipc.clients.iter_mut().find(|c| c.id == id) {
                    c.subscribed = true;
                }
                if grant.session_id.as_str() == ROOT_SESSION {
                    self.shell_arrived();
                }
                json(serde_json::Value::Null)
            }
            Request::FocusWindow { window } => {
                let w = self.any_window_by_id(&window)?;
                if self.is_locked() {
                    return Err(fail(ErrorCode::Forbidden, "the session is locked"));
                }
                // A minimized window comes back first; a parked one brings
                // its workspace along (the taskbar).
                self.restore_window(&w);
                let ws = crate::state::window_state(&w).workspace;
                if ws != self.active_workspace() {
                    self.switch_workspace(ws);
                }
                self.focus(desk, Some(w));
                json(serde_json::Value::Null)
            }
            Request::CloseWindow { window } => {
                // Parked and minimized windows close from the taskbar too.
                let w = self.any_window_by_id(&window)?;
                self.close_window(&w);
                json(serde_json::Value::Null)
            }
            Request::MinimizeWindow { window } => {
                let w = self.any_window_by_id(&window)?;
                if self.is_locked() {
                    return Err(fail(ErrorCode::Forbidden, "the session is locked"));
                }
                self.minimize_window(&w);
                json(serde_json::Value::Null)
            }
            Request::RestoreWindow { window } => {
                let w = self.any_window_by_id(&window)?;
                if self.is_locked() {
                    return Err(fail(ErrorCode::Forbidden, "the session is locked"));
                }
                if self.restore_window(&w) {
                    let ws = crate::state::window_state(&w).workspace;
                    if ws != self.active_workspace() {
                        self.switch_workspace(ws);
                    }
                    self.focus(desk, Some(w));
                }
                json(serde_json::Value::Null)
            }
            Request::MoveWindow { window, x, y } => {
                let w = self.window_by_id(&window)?;
                if crate::state::window_state(&w).tile.is_some() {
                    self.untile(&w, Some((x, y).into()));
                } else {
                    self.move_window(&w, (x, y).into(), false);
                }
                json(serde_json::Value::Null)
            }
            Request::Capture { target, since } => {
                // An output's pixels change only with a presented frame; a
                // window capture is always drawn (a parked window's commits
                // are never presented, #EDE-100).
                if since == Some(self.frame_serial) && matches!(target, CaptureTarget::Output { .. }) {
                    let info = CaptureInfo { width: 0, height: 0, stride: 0, format: "argb8888".into(), serial: self.frame_serial, unchanged: true, notice: None };
                    return Ok(Some((serde_json::to_value(info).unwrap(), None)));
                }
                let frame = match target {
                    CaptureTarget::Output { name } => {
                        let output = self.desk.space.outputs().find(|o| o.name() == name).cloned().ok_or_else(|| fail(ErrorCode::NotFound, "no such output"))?;
                        crate::backend::capture_output(self, &output)
                    }
                    // #EDE-125: any space, for `system` alone.
                    CaptureTarget::Space { .. } if !grant.admits(Capability::System, now) => return Err(fail(ErrorCode::Forbidden, "another's space needs the system capability")),
                    CaptureTarget::Space { space } => {
                        let desk = self.spaces.iter().find(|s| s.id == space && (principal == Principal::Root || self.ipc.cu_sessions.contains(&s.owner))).map(|s| s.desk.id).ok_or_else(|| fail(ErrorCode::NotFound, "no such space"))?;
                        return self.space_capture(desk, since, crate::spaces::scope::Caller { conn: id, call: call_id }, false);
                    }
                    CaptureTarget::Window { window } => {
                        // A parked or minimized window still has its last
                        // frame (the taskbar's hover preview).
                        let w = self.any_window_by_id(&window)?;
                        if !self.may_capture(&w) {
                            return Err(fail(ErrorCode::Forbidden, "a protected window is never captured"));
                        }
                        crate::backend::capture_window(self, &w)
                    }
                }
                .map_err(|e| fail(ErrorCode::Internal, &e))?;
                let info = CaptureInfo { width: frame.width, height: frame.height, stride: frame.width * 4, format: "argb8888".into(), serial: self.frame_serial, unchanged: false, notice: None };
                let bytes = frame.pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
                Ok(Some((serde_json::to_value(info).unwrap(), Some(bytes))))
            }
            Request::Input { event, bindings } => {
                self.ipc_input(id, desk, &grant, event, bindings, now)?;
                json(serde_json::Value::Null)
            }
            Request::PointerPosition => {
                let at = self.desk(desk).seat.get_pointer().unwrap().current_location();
                json(serde_json::to_value(PointerInfo { x: at.x, y: at.y }).unwrap())
            }
            Request::HoldInput => {
                self.hold_input(desk, id);
                json(serde_json::Value::Null)
            }
            Request::ReleaseInput => {
                self.release_input(desk, id);
                json(serde_json::Value::Null)
            }
            Request::ReloadConfig => {
                self.reload_config().map_err(|e| fail(ErrorCode::Invalid, &e))?;
                json(serde_json::Value::Null)
            }
            Request::ListWorkspaces => json(serde_json::to_value(self.workspace_infos()).unwrap()),
            Request::SwitchWorkspace { index } => {
                if index >= crate::workspaces::MAX {
                    return Err(fail(ErrorCode::Invalid, &format!("workspaces go up to {}", crate::workspaces::MAX)));
                }
                self.switch_workspace(index);
                json(serde_json::Value::Null)
            }
            Request::MoveToWorkspace { window, index } => {
                if index >= crate::workspaces::MAX {
                    return Err(fail(ErrorCode::Invalid, &format!("workspaces go up to {}", crate::workspaces::MAX)));
                }
                let w = self.any_window_by_id(&window)?;
                self.move_to_workspace(&w, index, false);
                json(serde_json::Value::Null)
            }
            Request::SetOutput { name, mode, scale, position } => {
                let mode = match mode {
                    Some(m) => Some(crate::config::parse_mode(&m).ok_or_else(|| fail(ErrorCode::Invalid, "mode must be WxH or WxH@R"))?),
                    None => None,
                };
                self.set_output(&name, &crate::outputs::OutputChange { mode, scale, position }).map_err(|e| fail(ErrorCode::Invalid, &e))?;
                json(serde_json::Value::Null)
            }
            Request::BindShortcuts { owner, shortcuts } => {
                let taken = |t: &crate::shortcuts::Trigger| t.syms().iter().any(|&sym| crate::input::binding(&t.mods(), sym).is_some());
                let bound = self.ipc.shortcuts.bind(id, &owner, shortcuts, taken);
                json(serde_json::to_value(bound).unwrap())
            }
            Request::UnbindShortcuts { owner } => {
                self.ipc.shortcuts.unbind(&owner);
                json(serde_json::Value::Null)
            }
            Request::ClipboardSerial => json(serde_json::to_value(ede_protocol::ClipboardSerial { serial: self.desk(desk).clipboard.serial }).unwrap()),
            Request::ClipboardSet { items, sensitive } => {
                let serial = self.clipboard_set(desk, items, sensitive)?;
                json(serde_json::to_value(ede_protocol::ClipboardSerial { serial }).unwrap())
            }
            Request::ClipboardTaken { serial } => json(serde_json::to_value(self.clipboard_taken(desk, serial)).unwrap()),
            Request::ClipboardSnapshot => Ok(self.clipboard_read(desk, id, call_id, crate::clipboard::Want::Snapshot).map(|v| (v, None))),
            Request::ClipboardText => Ok(self.clipboard_read(desk, id, call_id, crate::clipboard::Want::Text).map(|v| (v, None))),
            Request::Unlock => {
                let conn = self.ipc.clients.iter().find(|c| c.id == id);
                let token = conn.and_then(|c| c.token.clone()).ok_or_else(|| fail(ErrorCode::Unauthorized, "no token"))?;
                let peer = conn.and_then(|c| smithay::reexports::rustix::net::sockopt::socket_peercred(&c.writer).ok()).map(|c| c.pid.as_raw_nonzero().get() as u32);
                // #EDE-172 (P4): a lock client on its channel unlocks on that connection alone.
                self.sealed_unlock(&token, peer, Some(id)).map_err(|e| fail(ErrorCode::Forbidden, e))?;
                json(serde_json::Value::Null)
            }
            Request::Logout => {
                tracing::info!("logout requested over ede-protocol");
                self.loop_signal.stop();
                json(serde_json::Value::Null)
            }
            Request::Lock => {
                self.request_lock();
                json(serde_json::Value::Null)
            }
            Request::MintGrant { session_id, run_id, capabilities, expires_at } => {
                if let Some(why) = crate::spaces::scope::mint_refused(&session_id, &capabilities) {
                    return Err(fail(ErrorCode::Invalid, why));
                }
                if let Some(extra) = capabilities.iter().find(|c| !grant.capabilities.contains(c)) {
                    return Err(fail(ErrorCode::Forbidden, &format!("cannot grant {}: not held", extra.as_str())));
                }
                if let Some(limit) = grant.expires_at {
                    if expires_at.is_none_or(|e| e > limit) {
                        return Err(fail(ErrorCode::Forbidden, "cannot outlive the minting grant"));
                    }
                }
                if !principal.may_mint(&capabilities) {
                    return Err(fail(ErrorCode::Forbidden, "computer use mints {capture, input:person, ui} or [space], nothing wider"));
                }
                let minted = Grant { token: new_token(), session_id, run_id, capabilities, expires_at };
                if principal == Principal::ComputerUse {
                    self.ipc.cu_grants.insert(minted.token.clone());
                    if minted.capabilities == [Capability::Space] {
                        self.ipc.cu_sessions.insert(minted.session_id.clone());
                    }
                }
                self.ipc.grants.push(minted.clone());
                json(serde_json::to_value(minted).unwrap())
            }
            Request::RevokeGrant { token } => {
                // #EDE-172: `system` ends any grant; a holder ends its own
                // and what was opened on it (asked before the lookup: no
                // other grant's existence shows).
                if principal == Principal::ComputerUse && !self.ipc.cu_grants.contains(&token) {
                    return Err(fail(ErrorCode::Forbidden, "computer use ends only the grants it made"));
                }
                if !grant.admits(Capability::System, now) && !self.ipc.lineage(&grant.token).contains(&token) {
                    return Err(fail(ErrorCode::Forbidden, "needs the system capability, or a grant opened on the caller's"));
                }
                let g = self.ipc.grant(&token).ok_or_else(|| fail(ErrorCode::NotFound, "no such grant"))?;
                if g.session_id.as_str() == ROOT_SESSION {
                    return Err(fail(ErrorCode::Forbidden, "the root grant lives as long as the compositor"));
                }
                let scope = Scope::of(g);
                // #EDE-172: what was opened on it ends with it, connections
                // and all; a bound grant's own connections too (a bearer
                // grant's keep what they hold until they drop).
                if self.ipc.is_bound(&token) {
                    let lineage = self.ipc.lineage(&token);
                    self.ipc.end_grants(&lineage);
                } else {
                    self.ipc.end_children(&token);
                }
                self.ipc.grants.retain(|g| g.token != token);
                // #EDE-125: a space ends with its session's last space grant.
                if let Scope::Space(session) = scope {
                    self.space_grant_revoked(&session);
                }
                json(serde_json::Value::Null)
            }
            Request::LaunchToken => {
                // #EDE-76: only the shell says what the person launched.
                if grant.session_id.as_str() != ROOT_SESSION {
                    return Err(fail(ErrorCode::Forbidden, "only the shell's grant marks a launch as the person's"));
                }
                json(serde_json::to_value(ede_protocol::LaunchToken { token: self.mint_launch_token() }).unwrap())
            }
            // #EDE-125: the person's side, `system` only: every space;
            // #EDE-172 (P5'): computer use, its own lineage's.
            Request::ListSpaces => {
                let mut spaces = self.space_infos(None);
                if principal == Principal::ComputerUse {
                    spaces.retain(|s| self.ipc.cu_sessions.contains(&s.session_id));
                }
                json(serde_json::to_value(spaces).unwrap())
            }
            Request::DestroySpace { space } => {
                if principal == Principal::ComputerUse && !self.spaces.iter().any(|s| s.id == space && self.ipc.cu_sessions.contains(&s.owner)) {
                    return Err(fail(ErrorCode::NotFound, "no such space"));
                }
                self.space_destroy(None, &space)?;
                json(serde_json::Value::Null)
            }
            // Only a space grant has a space to make or to launch into.
            Request::CreateSpace { .. } | Request::Launch { .. } | Request::SpaceBusy => Err(fail(ErrorCode::Forbidden, "a space grant only")),
            // #EDE-127: the polkit agent asks before it shows the person a prompt.
            Request::SpaceOfProcess { pid } => json(serde_json::to_value(self.process_answer(pid)).unwrap()),
            // #EXP-1514 (#EXP-2011, the flip): from what holds now (`spaces::readiness`).
            Request::SpaceReadiness => json(serde_json::to_value(self.space_readiness_reply()).unwrap()),
            Request::OpenChannel { session_id, capabilities, expires_at } => {
                if principal == Principal::ComputerUse && (session_id.is_none() || capabilities.contains(&Capability::Space) || !principal.may_mint(&capabilities)) {
                    return Err(fail(ErrorCode::Forbidden, "computer use opens children of {capture, input:person, ui}, never root's own"));
                }
                let opened = self.ipc_open_channel(&grant, session_id, capabilities, expires_at, now)?;
                if principal == Principal::ComputerUse {
                    self.ipc.cu_grants.insert(opened.token.clone());
                }
                let (ours, theirs) = UnixStream::pair().map_err(|e| fail(ErrorCode::Internal, &e.to_string()))?;
                if !self.ipc_insert_channel(ours, &opened.token) {
                    if opened.token != grant.token {
                        self.ipc.end_grants(std::slice::from_ref(&opened.token));
                    }
                    return Err(fail(ErrorCode::Internal, "the channel could not be served"));
                }
                if let Some(c) = self.ipc.clients.iter_mut().find(|c| c.id == id) {
                    c.pending_fd = Some(OwnedFd::from(theirs));
                }
                json(serde_json::to_value(opened).unwrap())
            }
            // #EDE-172 (P4): the shell's supervisor, for each shell it starts.
            Request::OpenWayland => {
                let fd = self.helper_wayland_for(id).ok_or_else(|| fail(ErrorCode::Forbidden, "only the channel the compositor started a helper with asks for a Wayland connection"))?;
                if let Some(c) = self.ipc.clients.iter_mut().find(|c| c.id == id) {
                    c.pending_fd = Some(fd);
                }
                json(serde_json::Value::Null)
            }
        }
    }

    /// #EDE-172 (P4): a channel for a helper the compositor starts
    /// ([`crate::helpers`]): the connection's id, the helper's end, and the
    /// bound child grant it holds (`None` = root's own). `None` = it could
    /// not be made.
    pub(crate) fn ipc_helper_channel(&mut self, channel: &crate::helpers::Channel) -> Option<(u64, OwnedFd, Option<Token>)> {
        let root = self.ipc.root_grant();
        let grant = match channel {
            crate::helpers::Channel::Root => root.clone(),
            crate::helpers::Channel::Child { session, capabilities } => {
                let session = SessionId::new(session.as_str()).ok()?;
                self.ipc_open_channel(&root, Some(session), capabilities.clone(), None, now_ms()).map_err(|e| tracing::warn!("a helper's channel: {}", e.message)).ok()?
            }
        };
        let child = (grant.token != root.token).then(|| grant.token.clone());
        let made = UnixStream::pair().ok().and_then(|(ours, theirs)| Some((self.ipc_insert_holding(ours, Some(grant.token.clone()))?, OwnedFd::from(theirs))));
        match made {
            Some((id, theirs)) => Some((id, theirs, child)),
            None => {
                if let Some(child) = &child {
                    self.ipc.end_grants(std::slice::from_ref(child));
                }
                None
            }
        }
    }

    /// #EDE-172 (P4): helper channel `conn` is over (its helper ended): the
    /// connection goes (what it held with it), and so does the bound grant
    /// `grant` with everything opened on it.
    pub(crate) fn ipc_end_channel(&mut self, conn: u64, grant: Option<&Token>) {
        if let Some(c) = self.ipc.clients.iter_mut().find(|c| c.id == conn) {
            c.kill();
        }
        if let Some(grant) = grant {
            let lineage = self.ipc.lineage(grant);
            self.ipc.end_grants(&lineage);
        }
    }

    /// The hello's end: connection `id` holds `grant` as `principal` from
    /// now on (its desk is its first hello's), and the reply. `named` =
    /// it said the token (else a channel learns which grant it holds).
    fn ipc_bind_hello(&mut self, id: u64, grant: Grant, principal: Principal, named: bool) -> Result<serde_json::Value, Failure> {
        let scope = Scope::of(&grant);
        if let Some(c) = self.ipc.clients.iter_mut().find(|c| c.id == id) {
            if c.scope.as_ref().is_some_and(|kept| *kept != scope) {
                return Err(fail(ErrorCode::Forbidden, "a connection keeps the desk of its first hello"));
            }
            c.token = Some(grant.token.clone());
            c.scope = Some(scope);
            c.principal = principal;
        }
        let mut features: Vec<String> = self.spaces.dir.iter().map(|_| ede_protocol::FEATURE_SPACES.to_string()).collect();
        // #EXP-1514 (#EXP-2011, the flip): what offers a space to people.
        if self.says_space_readiness() {
            features.push(ede_protocol::FEATURE_SPACE_READINESS.to_string());
        }
        let id_of = grant.token.clone();
        let hello = Hello { version: PROTOCOL_VERSION, capabilities: grant.capabilities, session_id: grant.session_id, features, unknown_capabilities: vec![] };
        let mut hello = serde_json::to_value(hello).unwrap();
        // #EDE-172: a channel learns which grant it holds (a bound one's id).
        if !named {
            hello["grant"] = serde_json::Value::String(id_of.as_str().to_string());
        }
        Ok(hello)
    }

    /// #EDE-172 (P5'): what the `computer-use` principal never does (the
    /// app does none of it): `forbidden` before anything else is looked at.
    fn ipc_computer_use(&self, request: &Request) -> Result<(), Failure> {
        let refused = match request {
            Request::Lock | Request::Logout | Request::Unlock | Request::SetOutput { .. } | Request::ReloadConfig | Request::LaunchToken => true,
            Request::Subscribe | Request::BindShortcuts { .. } | Request::UnbindShortcuts { .. } => true,
            Request::Input { bindings, .. } => *bindings,
            _ => false,
        };
        if refused {
            return Err(fail(ErrorCode::Forbidden, "computer use does not do that: it is the shell's"));
        }
        Ok(())
    }

    /// #EDE-172 (P5'): the gate for a root hello on connection `id`:
    /// `Some(principal)` now, `None` = the reply comes later (a hash being
    /// read, a question to the person), or the refusal. `hash` = the
    /// executable's, read off the loop.
    fn ipc_gate(&mut self, id: u64, call_id: u64, hash: Option<[u8; 32]>) -> Result<Option<Principal>, Failure> {
        let own = [Some(std::process::id()), self.sealed_stand_in, self.shell_stand_in];
        let own: Vec<u32> = own.into_iter().flatten().collect();
        let policy = self.ipc.consent.policy();
        let Some(conn) = self.ipc.clients.iter().find(|c| c.id == id) else { return Err(fail(ErrorCode::Unauthorized, "gone")) };
        let peer = conn.peer.as_ref();
        let hash = hash.or_else(|| peer.and_then(|p| self.ipc.consent.cached_hash(p)));
        if hash.is_none() && peer.is_some_and(|p| self.ipc.consent.needs_hash(p, &self.installed, &policy)) {
            let peer = peer.unwrap();
            let (pid, exe) = (peer.pid, peer.exe.clone());
            let (Some(file), Some(exe)) = (crate::consent::file_key(peer), exe) else { return Err(fail(ErrorCode::Unauthorized, "its executable could not be read")) };
            // One hash per file: a hello for a file already being read waits for it.
            if let Some(waiting) = self.ipc.consent.hashing.get_mut(&file) {
                waiting.push((id, call_id));
                return Ok(None);
            }
            self.ipc.consent.hashing.insert(file, vec![(id, call_id)]);
            *self.ipc.consent.hashes_started.entry(file).or_default() += 1;
            let tx = self.consent_events();
            let spawned = std::thread::Builder::new().name("ede-consent-hash".into()).spawn(move || {
                let _ = tx.send(ConsentEvent::Hashed { file, hash: crate::consent::hash_exe(pid, exe.dev, exe.ino) });
            });
            if let Err(e) = spawned {
                self.ipc.consent.hashing.remove(&file);
                return Err(fail(ErrorCode::Internal, &e.to_string()));
            }
            return Ok(None);
        }
        let verdict = self.ipc.consent.verdict(peer, &self.installed, &own, &policy, hash.as_ref());
        let (pid, start) = peer.map_or((0, 0), |p| (p.pid, p.start));
        match verdict {
            Verdict::Grant(principal) => Ok(Some(principal)),
            Verdict::Refuse(why) => {
                tracing::warn!(pid, "the root token was refused: {why}");
                Err(fail(ErrorCode::Unauthorized, &why))
            }
            Verdict::Ask(asking) => {
                self.ipc.consent.waiting.push_back(Waiter { conn: id, call: call_id, pid, start, asking });
                self.consent_ask_next();
                Ok(None)
            }
        }
    }

    /// The gate's own events, on the loop: one channel for the compositor's life.
    fn consent_events(&mut self) -> smithay::reexports::calloop::channel::Sender<ConsentEvent> {
        if let Some(tx) = &self.ipc.consent_tx {
            return tx.clone();
        }
        let (tx, rx) = smithay::reexports::calloop::channel::channel::<ConsentEvent>();
        let inserted = self.loop_handle.insert_source(rx, |event, _, state| {
            if let smithay::reexports::calloop::channel::Event::Msg(event) = event {
                state.consent_event(event);
            }
        });
        if let Err(e) = inserted {
            tracing::error!("the consent gate's channel: {e}");
        }
        self.ipc.consent_tx = Some(tx.clone());
        tx
    }

    fn consent_event(&mut self, event: ConsentEvent) {
        match event {
            ConsentEvent::Hashed { file, hash } => {
                let waiting = self.ipc.consent.hashing.remove(&file).unwrap_or_default();
                if let Ok(hash) = hash {
                    self.ipc.consent.remember_hash(file, hash);
                }
                for (conn, call) in waiting {
                    let outcome = match &hash {
                        Ok(hash) => self.ipc_gate(conn, call, Some(*hash)),
                        Err(why) => Err(fail(ErrorCode::Unauthorized, &format!("its executable could not be read: {why}"))),
                    };
                    match outcome {
                        Ok(Some(principal)) => self.consent_finish(conn, call, principal),
                        Ok(None) => {}
                        Err(failure) => self.ipc_reply_later(conn, call, Err(failure)),
                    }
                }
            }
            ConsentEvent::Answered { key, allowed } => self.consent_answered(key, allowed),
        }
    }

    /// A waiting hello's end: bound as `principal`, answered.
    fn consent_finish(&mut self, conn: u64, call: u64, principal: Principal) {
        let root = self.ipc.root_grant();
        let reply = self.ipc_bind_hello(conn, root, principal, true);
        self.ipc_reply_later(conn, call, reply);
    }

    /// Opens the next question when none is open: the prompt (the sealed
    /// consent the compositor spawns), a test's stand-in, or
    /// `--consent-answer`.
    fn consent_ask_next(&mut self) {
        if self.ipc.consent.open.is_some() {
            return;
        }
        let Some(asking) = self.ipc.consent.waiting.front().map(|w| w.asking.clone()) else { return };
        let key = asking.key.clone();
        let (tx, rx) = smithay::reexports::calloop::channel::channel::<bool>();
        let events = self.consent_events();
        let answered = key.clone();
        let inserted = self.loop_handle.insert_source(rx, move |event, _, _| {
            if let smithay::reexports::calloop::channel::Event::Msg(allowed) = event {
                let _ = events.send(ConsentEvent::Answered { key: answered.clone(), allowed });
            }
        });
        if inserted.is_err() {
            self.consent_failed(&key, "the consent prompt could not be watched");
            return;
        }
        if let Some(allowed) = self.ipc.consent.answer_with {
            self.ipc.consent.open = Some((key, None));
            let _ = tx.send(allowed);
            return;
        }
        if let Some(stand_in) = self.ipc.consent.stand_in.clone() {
            self.ipc.consent.open = Some((key.clone(), None));
            if stand_in.send(crate::consent::StandIn { asking, answer: tx }).is_err() {
                self.ipc.consent.open = None;
                self.consent_failed(&key, "nobody answers for the person");
            }
            return;
        }
        let argv = vec![crate::sealed::sealed_binary(), "consent".into(), "--title".into(), asking.title.clone(), "--body".into(), asking.body.clone(), "--allow".into(), "Allow".into(), "--deny".into(), "Don't allow".into()];
        let child = self.client_command(&argv).and_then(|mut command| command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).spawn().map_err(|e| tracing::warn!("{}: {e}", argv[0])).ok());
        let Some(child) = child else {
            self.consent_failed(&key, "the consent prompt could not start");
            return;
        };
        let pid = child.id();
        tracing::info!(pid, "the consent prompt asks the person: {}", asking.body);
        self.ipc.consent.open = Some((key, Some(pid)));
        let spawned = std::thread::Builder::new().name("ede-consent-prompt".into()).spawn(move || {
            let allowed = child.wait_with_output().is_ok_and(|out| out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "allow");
            let _ = tx.send(allowed);
        });
        if spawned.is_err() {
            tracing::error!("the consent prompt's watcher did not start");
        }
    }

    /// The person answered the open question: every hello waiting for the
    /// same key gets it; then the next question.
    fn consent_answered(&mut self, key: Key, allowed: bool) {
        if self.ipc.consent.open.as_ref().is_some_and(|(open, _)| *open == key) {
            self.ipc.consent.open = None;
        }
        let waiting = std::mem::take(&mut self.ipc.consent.waiting);
        let (mine, rest): (Vec<Waiter>, Vec<Waiter>) = waiting.into_iter().partition(|w| w.asking.key == key);
        self.ipc.consent.waiting = rest.into();
        let askers: Vec<(u32, u64)> = mine.iter().map(|w| (w.pid, w.start)).collect();
        let files: Vec<(u64, u64)> = mine.iter().filter_map(|w| w.asking.file).collect();
        self.ipc.consent.answered(&key, allowed, &askers, &files);
        tracing::info!(allowed, "the person answered the consent prompt");
        for w in mine {
            if allowed {
                self.consent_finish(w.conn, w.call, w.asking.grants);
            } else {
                self.ipc_reply_later(w.conn, w.call, Err(fail(ErrorCode::Unauthorized, "the person did not allow it")));
            }
        }
        self.consent_ask_next();
    }

    /// No answer could be had (no prompt): the hellos of `key` are refused,
    /// nothing is remembered (the person said nothing).
    fn consent_failed(&mut self, key: &Key, why: &str) {
        tracing::warn!("{why}");
        let waiting = std::mem::take(&mut self.ipc.consent.waiting);
        let (mine, rest): (Vec<Waiter>, Vec<Waiter>) = waiting.into_iter().partition(|w| w.asking.key == *key);
        self.ipc.consent.waiting = rest.into();
        for w in mine {
            self.ipc_reply_later(w.conn, w.call, Err(fail(ErrorCode::Unavailable, why)));
        }
        self.consent_ask_next();
    }

    /// The consent prompt the compositor spawned, while it is open: its
    /// surface is sealed ([`crate::sealed::origin_of`]).
    pub fn consent_prompt_pid(&self) -> Option<u32> {
        self.ipc.consent.open.as_ref().and_then(|(_, pid)| *pid)
    }

    /// #EDE-172: connection `id` is anonymous from now on: its hello's reply
    /// holds no capability and names no grant.
    fn ipc_anonymous_hello(&mut self, id: u64) -> serde_json::Value {
        if let Some(c) = self.ipc.clients.iter_mut().find(|c| c.id == id) {
            c.anonymous = true;
            c.scope = Some(Scope::Person);
        }
        let mut features: Vec<String> = self.spaces.dir.iter().map(|_| ede_protocol::FEATURE_SPACES.to_string()).collect();
        if self.says_space_readiness() {
            features.push(ede_protocol::FEATURE_SPACE_READINESS.to_string());
        }
        let hello = Hello { version: PROTOCOL_VERSION, capabilities: vec![], session_id: SessionId::new(ANONYMOUS_SESSION).unwrap(), features, unknown_capabilities: vec![] };
        serde_json::to_value(hello).unwrap()
    }

    /// #EDE-172: the grant an `open_channel` call's connection holds: the
    /// caller's own (`session_id` none, exactly what it holds), or a bound
    /// child of it holding no more and expiring no later.
    fn ipc_open_channel(&mut self, grant: &Grant, session_id: Option<SessionId>, capabilities: Vec<Capability>, expires_at: Option<i64>, now: i64) -> Result<Grant, Failure> {
        let mut wanted = capabilities.clone();
        wanted.sort();
        wanted.dedup();
        let Some(session_id) = session_id else {
            let mut held = grant.capabilities.clone();
            held.sort();
            held.dedup();
            if wanted != held || expires_at.is_some() {
                return Err(fail(ErrorCode::Invalid, "a channel on the caller's grant holds it as it is; a narrower one names its session"));
            }
            return Ok(grant.clone());
        };
        if let Some(why) = crate::spaces::scope::mint_refused(&session_id, &wanted) {
            return Err(fail(ErrorCode::Invalid, why));
        }
        if wanted.contains(&Capability::Space) {
            return Err(fail(ErrorCode::Invalid, "a space grant is minted (mint_grant), never opened"));
        }
        if let Some(extra) = wanted.iter().find(|c| !grant.capabilities.contains(c)) {
            return Err(fail(ErrorCode::Forbidden, &format!("cannot open a channel holding {}: not held", extra.as_str())));
        }
        if let Some(limit) = grant.expires_at {
            if expires_at.is_none_or(|e| e > limit) {
                return Err(fail(ErrorCode::Forbidden, "cannot outlive the grant it is opened on"));
            }
        }
        if expires_at.is_some_and(|e| e <= now) {
            return Err(fail(ErrorCode::Invalid, "expired already"));
        }
        let child = Grant { token: new_token(), session_id, run_id: None, capabilities: wanted, expires_at };
        self.ipc.grants.push(child.clone());
        self.ipc.bound.insert(child.token.clone(), Bound { parent: grant.token.clone() });
        Ok(child)
    }

    /// `Request::Input` on `desk`, the desk the call resolved to, for the
    /// connection `conn` holding `grant`. `bindings` = the person's own
    /// input (the harness): only the shell's grant, only the person's desk.
    pub fn ipc_input(&mut self, conn: u64, desk: DeskId, grant: &Grant, event: InputEvent, bindings: bool, now: i64) -> Result<(), Failure> {
        if bindings && grant.session_id.as_str() != ROOT_SESSION {
            return Err(fail(ErrorCode::Forbidden, "only the shell's grant drives the bindings"));
        }
        // #EDE-136: the bindings arms below call the person's own paths
        // (their keyboard, their pointer), so they never take another desk:
        // a space's desk mixed with them would type into the person's focus.
        if bindings && desk != DeskId::PERSON {
            return Err(fail(ErrorCode::Forbidden, "the bindings drive the person's desk alone"));
        }
        // `bindings` = the person's own input (the harness): it
        // reaches the lock screen and protected surfaces like a
        // real keyboard; the agent path refuses those.
        let before = (!bindings).then(|| self.desk(desk).seat.get_keyboard().unwrap().pressed_keys());
        let refused = match (event, bindings) {
            (InputEvent::Key { key, pressed }, true) => {
                self.keyboard_key((key + 8).into(), if pressed { KeyState::Pressed } else { KeyState::Released }, now as u32);
                Ok(())
            }
            (InputEvent::PointerMotion { x, y }, true) => {
                self.person_pointer_motion_to((x, y).into(), now as u32);
                Ok(())
            }
            (InputEvent::PointerButton { button, pressed }, true) => {
                self.person_pointer_button(button, if pressed { ButtonState::Pressed } else { ButtonState::Released }, now as u32);
                Ok(())
            }
            (InputEvent::Key { key, pressed }, false) => {
                self.drive_key(desk, (key + 8).into(), if pressed { KeyState::Pressed } else { KeyState::Released }, now as u32, false)
            }
            (InputEvent::PointerMotionRelative { dx, dy }, bindings) => {
                let pos = self.desk(desk).seat.get_pointer().unwrap().current_location() + smithay::utils::Point::<f64, smithay::utils::Logical>::from((dx, dy));
                if bindings {
                    self.person_pointer_motion_to(pos, now as u32);
                    Ok(())
                } else {
                    self.drive_pointer_motion(desk, pos, now as u32)
                }
            }
            (InputEvent::Keysym { keysym, pressed }, bindings) => match self.drive_keysym(desk, keysym, pressed, bindings, now as u32) {
                Ok(false) => return Err(fail(ErrorCode::NotFound, &format!("no key makes keysym {keysym:#x} on this layout"))),
                other => other.map(|_| ()),
            },
            (InputEvent::PointerAxis { dx, dy }, _) => self.drive_pointer_axis(desk, (dx, dy), None, now as u32),
            (InputEvent::PointerAxisDiscrete { dx, dy }, _) => self.drive_pointer_axis(desk, (dx as f64 * 15.0, dy as f64 * 15.0), Some((dx * 120, dy * 120)), now as u32),
            (InputEvent::PointerMotion { x, y }, false) => self.drive_pointer_motion(desk, (x, y).into(), now as u32),
            (InputEvent::PointerButton { button, pressed }, false) => {
                let driven = self.drive_pointer_button(desk, button, if pressed { ButtonState::Pressed } else { ButtonState::Released }, now as u32);
                if driven.is_ok() {
                    self.desk_mut(desk).driven.buttons.retain(|(_, b)| *b != button);
                    if pressed {
                        self.desk_mut(desk).driven.buttons.push((conn, button));
                    }
                }
                driven
            }
        };
        if let Some(before) = before {
            self.note_driven(desk, conn, &before);
        }
        refused.map_err(|r| fail(ErrorCode::Forbidden, if r == crate::protected::Refused::Locked { "the session is locked" } else { "a protected surface is never driven" }))
    }

    fn window_by_id(&self, id: &WindowId) -> Result<Window, Failure> {
        self.desk.space.elements().find(|w| &window_id(w) == id).cloned().ok_or_else(|| fail(ErrorCode::NotFound, "no such window"))
    }

    /// On screen or parked on another workspace.
    fn any_window_by_id(&self, id: &WindowId) -> Result<Window, Failure> {
        self.all_windows().into_iter().find(|w| &window_id(w) == id).ok_or_else(|| fail(ErrorCode::NotFound, "no such window"))
    }

    pub fn window_infos(&self) -> Vec<WindowInfo> {
        let focused = self.focused_window(DeskId::PERSON);
        self.all_windows()
            .iter()
            .filter(|w| !x11_override_redirect(w))
            .filter_map(|w| {
                let st = crate::state::window_state(w);
                let g = self.desk.space.element_geometry(w).or_else(|| st.parked_at.map(|at| smithay::utils::Rectangle::new(at, w.geometry().size)))?;
                let protected = self.is_protected_window(w);
                let (title, app_id) = window_title(w);
                Some(WindowInfo {
                    id: window_id(w),
                    title: title.filter(|_| !protected),
                    app_id,
                    x: g.loc.x,
                    y: g.loc.y,
                    width: g.size.w,
                    height: g.size.h,
                    focused: focused.as_ref() == Some(w),
                    fullscreen: st.fullscreen.is_some(),
                    protected,
                    x11: is_x11(w),
                    workspace: st.workspace,
                    minimized: st.minimized,
                    // #EDE-91: every window is on the person's desk until spaces land.
                    space: None,
                })
            })
            .collect()
    }

    pub fn output_infos(&self) -> Vec<OutputInfo> {
        self.desk.space
            .outputs()
            .filter_map(|o| {
                let g = self.desk.space.output_geometry(o)?;
                let current = o.current_mode();
                let preferred = o.preferred_mode();
                let modes = o
                    .modes()
                    .into_iter()
                    .map(|m| ede_protocol::ModeInfo { width: m.size.w, height: m.size.h, refresh: m.refresh, preferred: Some(m) == preferred, current: Some(m) == current })
                    .collect();
                let p = o.physical_properties();
                Some(OutputInfo { name: o.name(), x: g.loc.x, y: g.loc.y, width: g.size.w, height: g.size.h, scale: o.current_scale().fractional_scale(), modes, make: p.make, model: p.model, physical_mm: [p.size.w, p.size.h], space: None })
            })
            .collect()
    }

    pub fn window_event(&mut self, kind: &str, window: &Window) {
        if self.ipc.clients.iter().any(|c| c.subscribed) {
            let (_, app_id) = window_title(window);
            let data = ede_protocol::WindowEvent { window: window_id(window), app_id, space: None };
            self.emit(kind, serde_json::to_value(data).unwrap());
        }
    }

    /// [`ede_protocol::events::OUTPUT_CHANGED`] for the output `name`.
    pub fn output_changed(&mut self, name: String) {
        self.emit(ede_protocol::events::OUTPUT_CHANGED, serde_json::to_value(ede_protocol::OutputEvent { output: name, space: None }).unwrap());
    }

    /// Re-reads the config file and applies what can change live (all but
    /// `xwayland`).
    pub fn reload_config(&mut self) -> Result<(), String> {
        let Some(path) = self.config_path.clone() else { return Err("started without a config file (safe mode)".into()) };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let mut config = crate::Config::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?.with_system_keyboard();
        config.xwayland = self.config.xwayland;
        // #EDE-172 (P4): who starts the helpers is decided at the start.
        config.session.helpers = self.config.session.helpers;
        let kb = config.keyboard.clone();
        self.config = config;
        let keyboard = self.desk.seat.get_keyboard().unwrap();
        let xkb = smithay::input::keyboard::XkbConfig { layout: &kb.layout, variant: &kb.variant, options: Some(kb.options.clone()).filter(|o| !o.is_empty()), ..Default::default() };
        self.desk.virtual_keymap = None;
        if let Err(e) = keyboard.set_xkb_config(self, xkb) {
            tracing::warn!("keyboard layout {:?} rejected on reload: {e:?}", kb.layout);
        }
        keyboard.change_repeat_info(kb.repeat_rate, kb.repeat_delay);
        self.apply_output_settings();
        self.emit(events::CONFIG_RELOADED, serde_json::Value::Null);
        Ok(())
    }
}

/// A window's stable id for the protocol (`w<n>`, never reused).
pub fn window_id(window: &Window) -> WindowId {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    window.user_data().get_or_insert(|| WindowId::new(format!("w{}", NEXT.fetch_add(1, Ordering::Relaxed))).unwrap()).clone()
}

/// An Xwayland window (none without the `xwayland` feature, #EDE-140).
pub(crate) fn is_x11(window: &Window) -> bool {
    #[cfg(feature = "xwayland")]
    return window.x11_surface().is_some();
    #[cfg(not(feature = "xwayland"))]
    {
        let _ = window;
        false
    }
}

/// An X11 override-redirect window (a menu, a tooltip): no window of the
/// list.
pub(crate) fn x11_override_redirect(window: &Window) -> bool {
    #[cfg(feature = "xwayland")]
    return window.x11_surface().is_some_and(|x| x.is_override_redirect());
    #[cfg(not(feature = "xwayland"))]
    {
        let _ = window;
        false
    }
}

/// The title (else the app id, else nothing) for a frame.
pub fn window_title_of(window: &Window) -> String {
    let (title, app_id) = window_title(window);
    title.filter(|t| !t.is_empty()).or(app_id).unwrap_or_default()
}

pub(crate) fn window_title(window: &Window) -> (Option<String>, Option<String>) {
    if let Some(t) = window.toplevel() {
        return smithay::wayland::compositor::with_states(t.wl_surface(), |s| {
            s.data_map
                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                .map(|d| {
                    let d = d.lock().unwrap();
                    (d.title.clone(), d.app_id.clone())
                })
                .unwrap_or_default()
        });
    }
    #[cfg(feature = "xwayland")]
    if let Some(x) = window.x11_surface() {
        return (Some(x.title()), Some(x.class()).filter(|c| !c.is_empty()));
    }
    (None, None)
}
