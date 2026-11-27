//! #EDE-227: the session bus between eDE's shell and the login's holder
//! (`exponential-app`, the component the compositor starts; on any other
//! desktop the standalone app).
//!
//! - **Where:** a stream socket beside the app's link socket, `exp-desktop.bus`
//!   in its data directory (0600), bound by whoever holds `exp-desktop.lock`.
//!   The link socket (`exp-desktop.sock`, datagrams, one way) stays for the
//!   releases that only know it.
//! - **What:** request and reply, one JSON object per line each way. Every
//!   request gets exactly one reply, in order; a connection may carry many.
//!   An op this end does not know is answered [`Reply::Unsupported`], never
//!   dropped, so either end can be newer.
//! - **Who:** both ends read the other's peer credentials (`SO_PEERCRED`):
//!   a peer of another user is cut off before its first line is read, and
//!   a client talks to no holder of another user.
//! - **Raising a window:** [`Request::Open`] carries the launch token the
//!   shell minted for the press ([`ede_protocol`]'s `launch_token`); the
//!   holder hands it to xdg-activation, so its window takes the keyboard
//!   the way a launched app's does.
//!
//! - **Windows (#EDE-237):** in eDE mode every page of the app is a window
//!   of its own; [`Request::Windows`] long-polls what each one is (kind,
//!   concept icon, title, team: [`WindowMeta`]), so the shell's taskbar
//!   thumbnails and window list wear them. The holder keeps them on a
//!   [`WindowBoard`].
//!
//! Later phases ride the same socket (the shell's feeds): new ops, new
//! replies.

use std::{
    io::{self, BufRead, BufReader, Read as _, Write},
    os::unix::{
        fs::PermissionsExt as _,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};

/// The bus socket's name in the app's data directory.
pub const SOCKET: &str = "exp-desktop.bus";

/// This end's version of the bus (in [`Reply::Hello`]).
pub const VERSION: u32 = 2;

/// The longest line either end reads (a link, a command, a token).
pub const MAX_LINE: usize = 64 * 1024;

/// How long a client waits for the holder to take and answer a request.
pub const CALL_TIMEOUT: Duration = Duration::from_millis(1500);

/// How long the holder holds a [`Request::Windows`] that is up to date
/// before it answers with the same generation (a client's read timeout must
/// be longer: [`WINDOWS_TIMEOUT`]).
pub const WINDOWS_WAIT: Duration = Duration::from_secs(25);

/// A [`Request::Windows`] client's read timeout.
pub const WINDOWS_TIMEOUT: Duration = Duration::from_secs(30);

/// The bus socket of the app whose data directory is `data_dir`.
pub fn socket_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SOCKET)
}

/// What a client asks the holder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Request {
    /// Who answers: [`Reply::Hello`].
    Hello,
    /// Open `link` (an `exponential://` link; #EDE-237: in eDE mode also an
    /// in-app URL, `https://<instance>/t/<team>/…`, landing on its page's
    /// own window), or with none bring a window up; the window it lands in comes to the front with
    /// `activation_token` (an xdg-activation token, one-shot).
    Open {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        link: Option<String>,
        #[serde(default, rename = "activationToken", skip_serializing_if = "Option::is_none")]
        activation_token: Option<String>,
    },
    /// What the shell asks of the login's account and install
    /// ([`crate::holder::Command`]).
    Holder { command: crate::holder::Command },
    /// The component asks a standalone app to hand the login over: it
    /// replies [`Reply::Done`] and quits; the component then takes the lock.
    /// The component itself refuses it.
    Handover,
    /// #EDE-237: the holder's windows ([`Reply::Windows`]). With `after` (a
    /// generation the client has) the holder answers once the windows
    /// changed past it, or after [`WINDOWS_WAIT`] with the same generation.
    Windows {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<u64>,
    },
}

/// #EDE-237: what one window of the holder is. Its toplevel's title is
/// `title` and its app id the reply's `app_id`: that is how the shell finds
/// the window it describes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowMeta {
    /// The page the window holds (`issue:<id>`, `board:<id>`, `home`, …):
    /// one window per key.
    pub key: String,
    /// The page's kind (`issue`, `board`, `chat`, `device`, …).
    pub kind: String,
    /// The kind's concept icon (`packages/icons`, e.g. `ui-issue`).
    pub icon: String,
    pub title: String,
    /// The team the page is in, by name and id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
}

/// Who holds the bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    /// `exponential-app`, eDE's component.
    Component,
    /// The app a person installed (AppImage, a package, macOS, Windows).
    Standalone,
}

/// The holder's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "kebab-case")]
pub enum Reply {
    Hello { version: u32, pid: u32, role: Role },
    /// Taken (a link is opened on the holder's own time).
    Done,
    /// Not done, and why.
    Refused { message: String },
    /// An op this holder does not know.
    Unsupported,
    /// #EDE-237: the holder's windows at `generation`, oldest first; its
    /// toplevels carry the app id `app_id`.
    Windows { generation: u64, #[serde(rename = "appId")] app_id: String, windows: Vec<WindowMeta> },
}

/// #EDE-237: the holder's windows as the bus hands them out: the
/// foreground [`WindowBoard::set`]s them, a [`Request::Windows`] waits on
/// them from a bus thread.
#[derive(Default)]
pub struct WindowBoard {
    state: std::sync::Mutex<(u64, Vec<WindowMeta>)>,
    changed: std::sync::Condvar,
}

impl WindowBoard {
    /// The windows now; a change bumps the generation and wakes the waits.
    pub fn set(&self, windows: Vec<WindowMeta>) {
        let Ok(mut state) = self.state.lock() else { return };
        if state.1 == windows {
            return;
        }
        state.0 += 1;
        state.1 = windows;
        self.changed.notify_all();
    }

    /// The generation and the windows, once the generation is past `after`
    /// (`None` = at once) or `wait` ran out.
    pub fn wait(&self, after: Option<u64>, wait: Duration) -> (u64, Vec<WindowMeta>) {
        let Ok(state) = self.state.lock() else { return (0, Vec::new()) };
        let state = match after {
            Some(after) => match self.changed.wait_timeout_while(state, wait, |state| state.0 <= after) {
                Ok((state, _)) => state,
                Err(poisoned) => poisoned.into_inner().0,
            },
            None => state,
        };
        (state.0, state.1.clone())
    }
}

/// The other end of a bus connection, as the kernel says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Peer {
    pub pid: u32,
    pub uid: u32,
}

/// `stream`'s peer (`SO_PEERCRED`).
#[cfg(target_os = "linux")]
pub fn peer(stream: &UnixStream) -> io::Result<Peer> {
    use std::os::fd::AsRawFd as _;
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // Safety: a valid socket and an out buffer of the size we pass.
    let rc = unsafe { libc::getsockopt(stream.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, (&mut cred as *mut libc::ucred).cast(), &mut len) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Peer { pid: cred.pid as u32, uid: cred.uid })
}

/// `stream`'s peer (`getpeereid`; no pid here).
#[cfg(not(target_os = "linux"))]
pub fn peer(stream: &UnixStream) -> io::Result<Peer> {
    use std::os::fd::AsRawFd as _;
    let (mut uid, mut gid) = (0, 0);
    // Safety: a valid socket and two out values.
    if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Peer { pid: 0, uid })
}

fn own_uid() -> u32 {
    // Safety: no arguments, cannot fail.
    unsafe { libc::geteuid() }
}

/// Is `peer` this user's own process?
pub fn same_user(peer: &Peer) -> bool {
    peer.uid == own_uid()
}

/// Reads one request or reply line; `None` at the end of the stream.
fn read_line<T: for<'de> Deserialize<'de>>(reader: &mut impl BufRead) -> io::Result<Option<Result<T, String>>> {
    let mut line = Vec::new();
    let n = reader.take(MAX_LINE as u64 + 1).read_until(b'\n', &mut line)?;
    if n == 0 {
        return Ok(None);
    }
    if line.last() != Some(&b'\n') {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "a line past the bus's limit"));
    }
    Ok(Some(serde_json::from_slice(&line).map_err(|e| e.to_string())))
}

fn write_line(stream: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    let mut line = serde_json::to_vec(value).map_err(io::Error::other)?;
    line.push(b'\n');
    stream.write_all(&line)?;
    stream.flush()
}

/// Answers the requests on one connection with `handle` until it ends.
pub fn serve_connection(stream: UnixStream, handle: &(dyn Fn(&Peer, Request) -> Reply + Send + Sync)) -> io::Result<()> {
    let peer = peer(&stream)?;
    if !same_user(&peer) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("a bus peer of uid {} (pid {})", peer.uid, peer.pid)));
    }
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    while let Some(request) = read_line::<Request>(&mut reader)? {
        let reply = match request {
            Ok(request) => handle(&peer, request),
            Err(_) => Reply::Unsupported,
        };
        write_line(&mut writer, &reply)?;
    }
    Ok(())
}

/// Binds the bus at `path` (a dead holder's socket file goes first: call
/// it holding the lock) and answers every connection with `handle`, each on
/// a thread of its own.
pub fn serve(path: &Path, handle: impl Fn(&Peer, Request) -> Reply + Send + Sync + 'static) -> io::Result<()> {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    let handle: Arc<dyn Fn(&Peer, Request) -> Reply + Send + Sync> = Arc::new(handle);
    std::thread::Builder::new().name("exp-bus".into()).spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let handle = handle.clone();
            let _ = std::thread::Builder::new().name("exp-bus-conn".into()).spawn(move || {
                if let Err(err) = serve_connection(stream, &*handle) {
                    log::info!("[bus] a connection ended: {err}");
                }
            });
        }
    })?;
    Ok(())
}

/// A client's connection to the holder.
pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    /// The holder, as the kernel says.
    pub holder: Peer,
}

impl Client {
    /// Connects to the bus at `path`; refuses a holder of another user.
    pub fn connect(path: &Path, timeout: Duration) -> io::Result<Client> {
        let stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let holder = peer(&stream)?;
        if !same_user(&holder) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("the bus is held by uid {}", holder.uid)));
        }
        Ok(Client { writer: stream.try_clone()?, reader: BufReader::new(stream), holder })
    }

    /// One request, its reply.
    pub fn call(&mut self, request: &Request) -> io::Result<Reply> {
        write_line(&mut self.writer, request)?;
        match read_line::<Reply>(&mut self.reader)? {
            Some(Ok(reply)) => Ok(reply),
            // A reply this end does not know (a newer holder).
            Some(Err(_)) => Ok(Reply::Unsupported),
            None => Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the holder closed the bus")),
        }
    }
}

/// One request to the holder at `path` on a connection of its own.
pub fn call(path: &Path, request: &Request) -> io::Result<Reply> {
    Client::connect(path, CALL_TIMEOUT)?.call(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("exp-bus-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn requests_and_replies_keep_their_wire_shape() {
        let open = Request::Open { link: Some("exponential://issue/EXP-1".into()), activation_token: Some("t0k".into()) };
        assert_eq!(serde_json::to_value(&open).unwrap(), serde_json::json!({"op": "open", "link": "exponential://issue/EXP-1", "activationToken": "t0k"}));
        assert_eq!(serde_json::to_value(Request::Open { link: None, activation_token: None }).unwrap(), serde_json::json!({"op": "open"}));
        let holder = Request::Holder { command: crate::holder::Command::SignOut { account: "a".into() } };
        assert_eq!(serde_json::to_value(&holder).unwrap(), serde_json::json!({"op": "holder", "command": {"do": "sign-out", "account": "a"}}));
        assert_eq!(serde_json::to_value(Reply::Hello { version: 1, pid: 7, role: Role::Component }).unwrap(), serde_json::json!({"reply": "hello", "version": 1, "pid": 7, "role": "component"}));
        assert_eq!(serde_json::from_value::<Reply>(serde_json::json!({"reply": "done"})).unwrap(), Reply::Done);
    }

    #[test]
    fn every_request_gets_its_reply_and_an_unknown_op_says_so() {
        let dir = dir();
        let path = socket_path(&dir);
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        serve(&path, move |peer, request| {
            assert_eq!(peer.pid, std::process::id(), "the peer's pid is the kernel's");
            log.lock().unwrap().push(request.clone());
            match request {
                Request::Hello => Reply::Hello { version: VERSION, pid: std::process::id(), role: Role::Standalone },
                Request::Handover => Reply::Refused { message: "no".into() },
                _ => Reply::Done,
            }
        })
        .unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let mut client = Client::connect(&path, CALL_TIMEOUT).unwrap();
        assert_eq!(client.holder.pid, std::process::id());
        assert_eq!(client.call(&Request::Hello).unwrap(), Reply::Hello { version: VERSION, pid: std::process::id(), role: Role::Standalone });
        let open = Request::Open { link: None, activation_token: Some("tok".into()) };
        assert_eq!(client.call(&open).unwrap(), Reply::Done);
        assert_eq!(client.call(&Request::Handover).unwrap(), Reply::Refused { message: "no".into() });
        // A newer client's op: answered, the connection stays.
        write_line(&mut client.writer, &serde_json::json!({"op": "feeds"})).unwrap();
        assert_eq!(read_line::<Reply>(&mut client.reader).unwrap().unwrap().unwrap(), Reply::Unsupported);
        assert_eq!(client.call(&Request::Hello).unwrap(), Reply::Hello { version: VERSION, pid: std::process::id(), role: Role::Standalone });
        assert_eq!(seen.lock().unwrap().len(), 4, "the unknown op never reached the handler");
        // A one-shot call.
        assert_eq!(call(&path, &open).unwrap(), Reply::Done);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn meta(key: &str) -> WindowMeta {
        WindowMeta { key: key.into(), kind: "issue".into(), icon: "ui-issue".into(), title: key.into(), team: Some("Exponential".into()), team_id: Some("t1".into()) }
    }

    #[test]
    fn windows_keep_their_wire_shape() {
        assert_eq!(serde_json::to_value(Request::Windows { after: None }).unwrap(), serde_json::json!({"op": "windows"}));
        assert_eq!(serde_json::to_value(Request::Windows { after: Some(3) }).unwrap(), serde_json::json!({"op": "windows", "after": 3}));
        let reply = Reply::Windows { generation: 4, app_id: "at.exponential.Desktop".into(), windows: vec![meta("issue:i1")] };
        assert_eq!(
            serde_json::to_value(&reply).unwrap(),
            serde_json::json!({"reply": "windows", "generation": 4, "appId": "at.exponential.Desktop", "windows": [{"key": "issue:i1", "kind": "issue", "icon": "ui-issue", "title": "issue:i1", "team": "Exponential", "teamId": "t1"}]})
        );
    }

    #[test]
    fn a_windows_wait_answers_on_a_change_or_when_it_runs_out() {
        let board = Arc::new(WindowBoard::default());
        assert_eq!(board.wait(None, Duration::ZERO), (0, Vec::new()));
        board.set(vec![meta("a")]);
        board.set(vec![meta("a")]);
        assert_eq!(board.wait(None, Duration::ZERO).0, 1, "the same windows are no change");
        // Up to date: held until the wait runs out, same generation.
        let started = std::time::Instant::now();
        assert_eq!(board.wait(Some(1), Duration::from_millis(50)).0, 1);
        assert!(started.elapsed() >= Duration::from_millis(50));
        // A change wakes a held wait.
        let waiter = { let board = board.clone(); std::thread::spawn(move || board.wait(Some(1), Duration::from_secs(10))) };
        std::thread::sleep(Duration::from_millis(30));
        board.set(vec![meta("a"), meta("b")]);
        let (generation, windows) = waiter.join().unwrap();
        assert_eq!((generation, windows.len()), (2, 2));
    }

    #[test]
    fn a_line_past_the_limit_ends_the_connection() {
        let dir = dir();
        let path = socket_path(&dir);
        serve(&path, |_, _| Reply::Done).unwrap();
        let mut client = Client::connect(&path, CALL_TIMEOUT).unwrap();
        let long = Request::Open { link: Some("x".repeat(MAX_LINE + 10)), activation_token: None };
        assert!(client.call(&long).is_err(), "no reply to an endless line");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nobody_listening_is_an_error_not_a_wait() {
        let dir = dir();
        assert!(call(&socket_path(&dir), &Request::Hello).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
