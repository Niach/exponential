//! One running instance per app id (Linux and Windows; macOS hands a running
//! app its links as `on_open_urls`). The first instance listens on a
//! per-app, per-user local endpoint: a Unix domain socket under
//! `$XDG_RUNTIME_DIR` (else the app's data directory), 0600, on Linux; a
//! named pipe with the default ACL (the user, SYSTEM, administrators) on
//! Windows. A later start [`claim`]s it: it forwards its deep link (or asks
//! the first to come to the front) and exits without a window or a store.
//!
//! The wire: one line per connection, `open <url>` or `activate`, answered
//! `ok`. An endpoint nobody answers (a crashed first instance's socket, a
//! hung one past [`TIMEOUT`]) is replaced, never waited on.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long a start waits on a running instance before it gives up on it.
pub const TIMEOUT: Duration = Duration::from_secs(2);

/// The longest line either side reads.
const MAX_LINE: u64 = 64 * 1024;

/// What a later start sends the running instance.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// A deep link: the same path as at launch (lifecycle.json `deepLink`).
    Open(String),
    /// A start without a link: come to the front.
    Activate,
}

impl Message {
    fn line(&self) -> String {
        match self {
            Message::Open(url) => format!("open {}\n", url.replace(['\r', '\n'], "")),
            Message::Activate => "activate\n".into(),
        }
    }

    fn parse(line: &str) -> Option<Message> {
        let line = line.strip_suffix('\n')?;
        if line == "activate" {
            return Some(Message::Activate);
        }
        line.strip_prefix("open ").filter(|url| !url.is_empty()).map(|url| Message::Open(url.into()))
    }
}

/// The local endpoint of one app id for this user.
#[derive(Clone, Debug, PartialEq)]
pub struct Endpoint(PathBuf);

impl Endpoint {
    /// A socket path (unix) or a pipe name `\\.\pipe\…` (Windows).
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Endpoint(path.into())
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The platform's endpoint for app `id` (`None` = there is nowhere to put one).
    #[cfg(unix)]
    pub fn for_app(id: &str) -> Option<Self> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        let data = crate::storage::platform_base().map(|base| crate::storage::app_dir(&base, id));
        socket_path(runtime.as_deref(), data.as_deref(), id).map(Endpoint)
    }

    #[cfg(windows)]
    pub fn for_app(id: &str) -> Option<Self> {
        Some(Endpoint(pipe_name(id, &std::env::var("USERNAME").unwrap_or_default()).into()))
    }
}

/// `$XDG_RUNTIME_DIR/<id>.sock` (an absolute one, as the spec requires), else `<app data>/<id>.sock`.
pub fn socket_path(runtime: Option<&Path>, app_data: Option<&Path>, id: &str) -> Option<PathBuf> {
    let dir = runtime.filter(|d| d.is_absolute()).or(app_data)?;
    Some(dir.join(format!("{id}.sock")))
}

/// `\\.\pipe\<id>.runner.<user>`: the pipe namespace is the machine's, so the user is in the name.
pub fn pipe_name(id: &str, user: &str) -> String {
    let user: String = user.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')).collect();
    format!(r"\\.\pipe\{id}.runner.{user}")
}

/// The outcome of a start.
pub enum Claim {
    /// This is the first instance: [`Primary::serve`] the endpoint, then run.
    First(Primary),
    /// A running instance took the message: exit 0.
    Forwarded,
}

/// The endpoint, bound, not yet served.
pub struct Primary {
    server: sys::Server,
    endpoint: Endpoint,
}

/// Hand `message` to the running instance of `endpoint`, or become it.
pub fn claim(endpoint: &Endpoint, message: &Message) -> io::Result<Claim> {
    // Two starts at once: one decides at a time (unix; a pipe's first instance is atomic).
    let _lock = sys::lock(endpoint)?;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match sys::connect(endpoint) {
            Ok(Some(stream)) => match timed(move |m| exchange(stream, &m), message.clone()) {
                Ok(()) => return Ok(Claim::Forwarded),
                // A socket nobody answers on is replaced; a live pipe cannot be.
                Err(e) if !sys::REPLACEABLE => return Err(e),
                Err(_) => {}
            },
            Ok(None) => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            Err(e) => return Err(e),
        }
        match sys::bind(endpoint)? {
            Some(server) => return Ok(Claim::First(Primary { server, endpoint: endpoint.clone() })),
            // Another start bound it between our connect and bind: forward to it.
            None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            None => return Err(io::ErrorKind::TimedOut.into()),
        }
    }
}

impl Primary {
    /// Accept on a thread; every message reaches `on` (off the main thread).
    pub fn serve(self, on: impl Fn(Message) + Send + 'static) -> Serving {
        let stop = Arc::new(AtomicBool::new(false));
        let Primary { mut server, endpoint } = self;
        let owner = sys::owner(&endpoint);
        let thread = {
            let stop = stop.clone();
            std::thread::spawn(move || loop {
                let accepted = server.accept();
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                if let Ok(stream) = accepted {
                    if let Some(message) = answer(stream) {
                        on(message);
                    }
                }
            })
        };
        Serving { stop, endpoint, owner, thread: Some(thread) }
    }
}

/// The served endpoint; dropping it stops the thread and removes the socket.
pub struct Serving {
    stop: Arc<AtomicBool>,
    endpoint: Endpoint,
    owner: Option<sys::Owner>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Serving {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the blocked accept.
        drop(sys::connect(&self.endpoint));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        sys::release(&self.endpoint, self.owner.take());
    }
}

/// The client side: send the line, wait for `ok`.
fn exchange<S: Read + Write>(mut stream: S, message: &Message) -> io::Result<()> {
    stream.write_all(message.line().as_bytes())?;
    stream.flush()?;
    let mut reply = String::new();
    BufReader::new(&mut stream).take(MAX_LINE).read_line(&mut reply)?;
    if reply == "ok\n" {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, "no answer"))
    }
}

/// The server side: read the line, answer `ok` before the message runs.
fn answer<S: Read + Write>(mut stream: S) -> Option<Message> {
    let mut line = String::new();
    BufReader::new(&mut stream).take(MAX_LINE).read_line(&mut line).ok()?;
    let message = Message::parse(&line)?;
    stream.write_all(b"ok\n").ok()?;
    stream.flush().ok()?;
    Some(message)
}

/// `f` on a thread, bounded by [`TIMEOUT`] (a pipe read has no timeout of its own).
fn timed<T: Send + 'static>(f: impl FnOnce(T) -> io::Result<()> + Send + 'static, arg: T) -> io::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f(arg));
    });
    rx.recv_timeout(TIMEOUT).unwrap_or_else(|_| Err(io::ErrorKind::TimedOut.into()))
}

#[cfg(unix)]
mod sys {
    use std::fs::{DirBuilder, File, OpenOptions};
    use std::io;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};

    use super::{Endpoint, TIMEOUT};

    /// A socket file nobody answers on can be removed and bound again.
    pub const REPLACEABLE: bool = true;

    pub struct Server(UnixListener);

    /// The socket file this instance bound (device, inode).
    pub type Owner = (u64, u64);

    /// `<socket>.lock`, held while a start decides.
    pub fn lock(endpoint: &Endpoint) -> io::Result<File> {
        let path = endpoint.path();
        if let Some(dir) = path.parent().filter(|d| !d.exists()) {
            DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        }
        let mut name = path.as_os_str().to_owned();
        name.push(".lock");
        let file = OpenOptions::new().create(true).truncate(false).write(true).mode(0o600).open(name)?;
        file.lock()?;
        Ok(file)
    }

    /// `None` = no socket, or a stale one (nobody listens).
    pub fn connect(endpoint: &Endpoint) -> io::Result<Option<UnixStream>> {
        match UnixStream::connect(endpoint.path()) {
            Ok(stream) => {
                stream.set_read_timeout(Some(TIMEOUT))?;
                stream.set_write_timeout(Some(TIMEOUT))?;
                Ok(Some(stream))
            }
            Err(e) if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Under the lock: whatever is at the path is stale, so it is replaced.
    pub fn bind(endpoint: &Endpoint) -> io::Result<Option<Server>> {
        let path = endpoint.path();
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Some(Server(listener)))
    }

    impl Server {
        pub fn accept(&mut self) -> io::Result<UnixStream> {
            let (stream, _) = self.0.accept()?;
            stream.set_read_timeout(Some(TIMEOUT))?;
            stream.set_write_timeout(Some(TIMEOUT))?;
            Ok(stream)
        }
    }

    pub fn owner(endpoint: &Endpoint) -> Option<Owner> {
        std::fs::symlink_metadata(endpoint.path()).ok().map(|m| (m.dev(), m.ino()))
    }

    /// Remove the socket if it is still the one this instance bound.
    pub fn release(endpoint: &Endpoint, owner: Option<Owner>) {
        if owner.is_some() && owner == self::owner(endpoint) {
            let _ = std::fs::remove_file(endpoint.path());
        }
    }
}

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;
    use std::fs::{File, OpenOptions};
    use std::io::{self, Read, Write};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};

    use super::Endpoint;

    /// A pipe lives only as long as its process: one that exists is live.
    pub const REPLACEABLE: bool = false;

    type Handle = *mut c_void;
    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    const PIPE_ACCESS_DUPLEX: u32 = 0x0000_0003;
    const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x0008_0000;
    const PIPE_TYPE_BYTE_READMODE_BYTE_WAIT: u32 = 0;
    const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x0000_0008;
    const PIPE_UNLIMITED_INSTANCES: u32 = 255;
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_PIPE_BUSY: i32 = 231;
    const ERROR_PIPE_CONNECTED: i32 = 535;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateNamedPipeW(name: *const u16, open_mode: u32, pipe_mode: u32, max_instances: u32, out_size: u32, in_size: u32, timeout: u32, security: *const c_void) -> Handle;
        fn ConnectNamedPipe(pipe: Handle, overlapped: *mut c_void) -> i32;
        fn DisconnectNamedPipe(pipe: Handle) -> i32;
        fn FlushFileBuffers(file: Handle) -> i32;
    }

    pub type Owner = ();

    /// The server always keeps one instance waiting, so the name never lapses while it runs.
    pub struct Server {
        name: Vec<u16>,
        waiting: Option<File>,
    }

    /// A connected instance: flushed and disconnected when dropped.
    pub struct Conn(File);

    impl Read for Conn {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.read(buf)
        }
    }

    impl Write for Conn {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.write(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.0.flush()
        }
    }

    impl Drop for Conn {
        fn drop(&mut self) {
            unsafe {
                FlushFileBuffers(self.0.as_raw_handle());
                DisconnectNamedPipe(self.0.as_raw_handle());
            }
        }
    }

    fn wide(endpoint: &Endpoint) -> Vec<u16> {
        endpoint.path().as_os_str().encode_wide().chain(Some(0)).collect()
    }

    /// `None` = another process already holds the name (`first` only).
    fn create(name: &[u16], first: bool) -> io::Result<Option<File>> {
        let mode = PIPE_ACCESS_DUPLEX | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
        let pipe = PIPE_TYPE_BYTE_READMODE_BYTE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
        // A null security descriptor = the default ACL: only this user (and SYSTEM, administrators) may write.
        let handle = unsafe { CreateNamedPipeW(name.as_ptr(), mode, pipe, PIPE_UNLIMITED_INSTANCES, 4096, 4096, 0, std::ptr::null()) };
        if handle == INVALID_HANDLE_VALUE {
            let e = io::Error::last_os_error();
            return if first && e.raw_os_error() == Some(ERROR_ACCESS_DENIED) { Ok(None) } else { Err(e) };
        }
        Ok(Some(unsafe { File::from_raw_handle(handle) }))
    }

    /// The pipe's first instance is atomic: nothing to lock.
    pub fn lock(_endpoint: &Endpoint) -> io::Result<()> {
        Ok(())
    }

    /// `None` = no instance runs; every instance busy = `WouldBlock`.
    pub fn connect(endpoint: &Endpoint) -> io::Result<Option<File>> {
        match OpenOptions::new().read(true).write(true).open(endpoint.path()) {
            Ok(file) => Ok(Some(file)),
            Err(e) if e.raw_os_error() == Some(ERROR_FILE_NOT_FOUND) => Ok(None),
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => Err(io::ErrorKind::WouldBlock.into()),
            Err(e) => Err(e),
        }
    }

    pub fn bind(endpoint: &Endpoint) -> io::Result<Option<Server>> {
        let name = wide(endpoint);
        Ok(create(&name, true)?.map(|first| Server { name, waiting: Some(first) }))
    }

    impl Server {
        pub fn accept(&mut self) -> io::Result<Conn> {
            let pipe = match self.waiting.take() {
                Some(pipe) => pipe,
                None => create(&self.name, false)?.ok_or(io::ErrorKind::AddrInUse)?,
            };
            let connected = unsafe { ConnectNamedPipe(pipe.as_raw_handle(), std::ptr::null_mut()) } != 0;
            let error = (!connected).then(io::Error::last_os_error);
            // The next instance waits before this one closes.
            self.waiting = create(&self.name, false)?;
            match error {
                Some(e) if e.raw_os_error() != Some(ERROR_PIPE_CONNECTED) => Err(e),
                _ => Ok(Conn(pipe)),
            }
        }
    }

    pub fn owner(_endpoint: &Endpoint) -> Option<Owner> {
        None
    }

    /// The pipe goes with its last handle.
    pub fn release(_endpoint: &Endpoint, _owner: Option<Owner>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn endpoint(dir: &Path, id: &str) -> Endpoint {
        #[cfg(unix)]
        return Endpoint::at(dir.join(format!("{id}.sock")));
        #[cfg(windows)]
        return {
            let _ = dir;
            Endpoint::at(pipe_name(&format!("{id}.{}", uuid::Uuid::new_v4().simple()), "test"))
        };
    }

    fn first(endpoint: &Endpoint) -> (Serving, mpsc::Receiver<Message>) {
        let Claim::First(primary) = claim(endpoint, &Message::Activate).unwrap() else { panic!("not the first") };
        let (tx, rx) = mpsc::channel();
        let serving = primary.serve(move |m| {
            let _ = tx.send(m);
        });
        (serving, rx)
    }

    #[test]
    fn the_wire() {
        for m in [Message::Open("at.acme.album://pages?album=a1".into()), Message::Activate] {
            assert_eq!(Message::parse(&m.line()), Some(m));
        }
        assert_eq!(Message::Open("a://b\nactivate".into()).line(), "open a://bactivate\n");
        assert_eq!(Message::parse("open a://b"), None, "a partial line is nothing");
        assert_eq!(Message::parse("open \n"), None);
        assert_eq!(Message::parse("quit\n"), None);
    }

    #[test]
    fn the_endpoint_per_app_and_user() {
        let id = "at.acme.album";
        #[cfg(unix)]
        let (runtime, data) = (Path::new("/run/user/1000"), Path::new("/home/a/.local/share/at.acme.album"));
        #[cfg(windows)]
        let (runtime, data) = (Path::new(r"C:\run"), Path::new(r"C:\data"));
        assert_eq!(socket_path(Some(runtime), Some(data), id), Some(runtime.join("at.acme.album.sock")));
        assert_eq!(socket_path(Some(Path::new("relative")), Some(data), id), Some(data.join("at.acme.album.sock")));
        assert_eq!(socket_path(None, Some(data), id), Some(data.join("at.acme.album.sock")));
        assert_eq!(socket_path(None, None, id), None);
        assert_eq!(pipe_name(id, r"ACME\jo doe"), r"\\.\pipe\at.acme.album.runner.ACMEjodoe");
    }

    #[test]
    fn a_second_start_forwards_its_link_and_a_bare_one_activates() {
        let dir = tempfile::tempdir().unwrap();
        let at = endpoint(dir.path(), "at.acme.album");
        let (_serving, rx) = first(&at);
        let link = Message::Open("at.acme.album://pages?album=a1".into());
        assert!(matches!(claim(&at, &link).unwrap(), Claim::Forwarded));
        assert_eq!(rx.recv_timeout(TIMEOUT).unwrap(), link);
        assert!(matches!(claim(&at, &Message::Activate).unwrap(), Claim::Forwarded));
        assert_eq!(rx.recv_timeout(TIMEOUT).unwrap(), Message::Activate);
        // Another app id is another endpoint.
        let other = endpoint(dir.path(), "at.acme.other");
        let (_other, _) = first(&other);
    }

    #[cfg(unix)]
    #[test]
    fn the_socket_is_the_users_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let at = endpoint(&dir.path().join("missing"), "at.acme.album");
        let (_serving, _) = first(&at);
        assert_eq!(std::fs::metadata(at.path()).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(dir.path().join("missing")).unwrap().permissions().mode() & 0o777, 0o700);
    }

    #[test]
    fn after_the_first_goes_the_next_start_is_first() {
        let dir = tempfile::tempdir().unwrap();
        let at = endpoint(dir.path(), "at.acme.album");
        drop(first(&at));
        let (_serving, rx) = first(&at);
        assert!(matches!(claim(&at, &Message::Activate).unwrap(), Claim::Forwarded));
        assert_eq!(rx.recv_timeout(TIMEOUT).unwrap(), Message::Activate);
    }

    #[cfg(unix)]
    #[test]
    fn a_crashed_first_instance_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let at = endpoint(dir.path(), "at.acme.album");
        // A crash leaves the socket file with nobody listening.
        drop(std::os::unix::net::UnixListener::bind(at.path()).unwrap());
        assert!(at.path().exists());
        let started = Instant::now();
        let (_serving, rx) = first(&at);
        assert!(started.elapsed() < TIMEOUT);
        assert!(matches!(claim(&at, &Message::Activate).unwrap(), Claim::Forwarded));
        assert_eq!(rx.recv_timeout(TIMEOUT).unwrap(), Message::Activate);
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_first_instance_is_replaced_after_the_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let at = endpoint(dir.path(), "at.acme.album");
        // Bound and listening, but it never answers.
        let _hung = std::os::unix::net::UnixListener::bind(at.path()).unwrap();
        let started = Instant::now();
        let (_serving, rx) = first(&at);
        assert!(started.elapsed() < TIMEOUT * 2);
        assert!(matches!(claim(&at, &Message::Activate).unwrap(), Claim::Forwarded));
        assert_eq!(rx.recv_timeout(TIMEOUT).unwrap(), Message::Activate);
    }

    #[test]
    fn a_silent_client_does_not_block_the_next() {
        let dir = tempfile::tempdir().unwrap();
        let at = endpoint(dir.path(), "at.acme.album");
        let (_serving, rx) = first(&at);
        let silent = sys::connect(&at).unwrap().unwrap();
        drop(silent);
        assert!(matches!(claim(&at, &Message::Open("at.acme.album://albums".into())).unwrap(), Claim::Forwarded));
        assert_eq!(rx.recv_timeout(TIMEOUT).unwrap(), Message::Open("at.acme.album://albums".into()));
    }
}
