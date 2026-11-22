//! #EDE-172 (P8): the host's LOCAL SOCKET, the way a run's agent reaches this
//! host's MCP servers with no token anywhere a process of the person could
//! read it (rule L-32: no readable value grants the person's desk). The
//! loopback HTTP listeners stay as they are; this is a second door to the
//! same [`crate::Server`]s.
//!
//! **One socket per host process** (the Exponential app, the CLI daemon):
//! `$XDG_RUNTIME_DIR/exponential/mcp-<host pid>.sock` on Linux (without a
//! runtime directory, a private `exponential-<uid>` directory under the temp
//! directory) and `$TMPDIR/exponential/mcp-<host pid>.sock` on macOS; the
//! directory is 0700 and the socket 0600. Every [`crate::Server`] of the
//! process is reachable on it by its [`crate::ServerInfo::name`].
//!
//! **A run is its agent's process tree.** The host binds a run to the agent
//! process it spawned, right after the spawn ([`bind_process`]; a resume or
//! a relaunch binds the new process). On accept the host takes the peer
//! from the kernel (Linux `SO_PEERCRED` + `SO_PEERPIDFD`, else `pidfd_open`;
//! macOS `LOCAL_PEERPID` + `getpeereid`) and walks its parents, comparing
//! start times, until it meets the agent of a bound run (admitted as that
//! run) or init or the host itself (refused): see [`ancestry`]. Nothing the
//! peer sends decides which run it is, and a run nobody bound admits nobody.
//!
//! **The protocol.** The peer's first line names the server (`computer\n`).
//! The host answers one line, `ok` or `refused: <why>`, and then speaks MCP
//! (newline-delimited JSON-RPC, MCP's stdio framing) as that run until the
//! connection closes. The run must hold a grant on that server
//! ([`crate::Server::grant`]); one revoked while connected reads why on its
//! next call, as over HTTP (#EXP-1918).
//!
//! What it leaves open (`ede-172-design.md` 8.3): the run's tree IS the run
//! (a program its agent starts may call as it while it lives); a process
//! that can ptrace a member of the tree; Windows, where no socket is served
//! (the hosts keep the HTTP bearer there, its own issue).
//!
//! Dark until a host asks: nothing listens before [`listen`].

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod ancestry;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod peer;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use ecore::SessionId;

use crate::server::LocalEntry;

/// What a host that cannot serve the socket hears.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
const UNSUPPORTED: &str = "The local MCP socket is served on Linux and macOS only.";

/// The servers by name ([`crate::Server`] registers itself at serve; a later
/// server of the same name replaces an earlier one here).
fn servers() -> &'static Mutex<HashMap<String, LocalEntry>> {
    static SERVERS: OnceLock<Mutex<HashMap<String, LocalEntry>>> = OnceLock::new();
    SERVERS.get_or_init(Default::default)
}

pub(crate) fn register(name: &str, entry: LocalEntry) {
    servers().lock().unwrap().insert(name.to_string(), entry);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn entry(name: &str) -> Option<LocalEntry> {
    servers().lock().unwrap().get(name).cloned()
}

/// The runs bound to their agent processes.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn bound() -> &'static Mutex<Vec<ancestry::Bound>> {
    static BOUND: OnceLock<Mutex<Vec<ancestry::Bound>>> = OnceLock::new();
    BOUND.get_or_init(Default::default)
}

/// Bind the run `session_id` to its agent process `pid`: every process
/// descending from it (the agent CLI's relay among them) is admitted as the
/// run. An earlier binding of the run (a relaunch) or of the pid ends.
/// Call it right after the spawn, while the child is unreaped: its pid
/// cannot name another process yet. The host itself is never a run's.
pub fn bind_process(session_id: &SessionId, pid: u32) -> Result<(), String> {
    if pid == std::process::id() {
        return Err("The host itself is never a run's process.".to_string());
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use ancestry::Procs as _;
        let stat = ancestry::Os.stat(pid).ok_or_else(|| format!("Process {pid} is gone."))?;
        let mut bound = bound().lock().unwrap();
        bound.retain(|bound| bound.run != *session_id && bound.pid != pid);
        bound.push(ancestry::Bound { run: session_id.clone(), pid, start: stat.start });
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = session_id;
        Err(UNSUPPORTED.to_string())
    }
}

/// The run ended: no process is its any more.
pub fn unbind_process(session_id: &SessionId) {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    bound().lock().unwrap().retain(|bound| bound.run != *session_id);
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let _ = session_id;
}

/// The listening socket, once [`listen`] opened it.
struct Listening {
    path: PathBuf,
    /// `<pid>:<start>`, what a relay is told its host is.
    identity: String,
}

static LISTENING: OnceLock<Result<Listening, String>> = OnceLock::new();

/// Open the host's socket (once per process; a failure is remembered, it
/// will not change under a running process) and serve it: its path.
pub fn listen() -> Result<PathBuf, String> {
    LISTENING.get_or_init(start).as_ref().map(|listening| listening.path.clone()).map_err(Clone::clone)
}

/// The socket's path, once listening.
pub fn socket_path() -> Option<PathBuf> {
    LISTENING.get()?.as_ref().ok().map(|listening| listening.path.clone())
}

/// `<pid>:<start>` of this host once listening: the relay checks that the
/// process behind the socket is this one, start time included, so a
/// process that took the socket's path cannot play the host.
pub fn host_identity() -> Option<String> {
    LISTENING.get()?.as_ref().ok().map(|listening| listening.identity.clone())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn start() -> Result<Listening, String> {
    Err(UNSUPPORTED.to_string())
}

/// Where the socket lives (see the module docs).
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn socket_dir() -> PathBuf {
    #[cfg(target_os = "linux")]
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|dir| dir.is_absolute() && dir.is_dir()) {
        return runtime.join("exponential");
    }
    #[cfg(target_os = "linux")]
    return std::env::temp_dir().join(format!("exponential-{}", peer::own_uid()));
    #[cfg(target_os = "macos")]
    return std::env::temp_dir().join("exponential");
}

/// `dir` exists, is a directory of this user (never a link) and is 0700.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn private_dir(dir: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _, PermissionsExt as _};
    let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir);
    let meta = std::fs::symlink_metadata(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    if !meta.is_dir() || meta.uid() != peer::own_uid() {
        return Err(format!("{} is not a directory of this user.", dir.display()));
    }
    if meta.mode() & 0o077 != 0 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    Ok(())
}

/// Sockets of hosts that are gone (`mcp-<pid>.sock` of a dead pid).
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn sweep(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|name| name.strip_prefix("mcp-")?.strip_suffix(".sock")?.parse::<libc::pid_t>().ok()) else {
            continue;
        };
        // SAFETY: signal 0 checks existence only.
        if pid > 0 && unsafe { libc::kill(pid, 0) } != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn start() -> Result<Listening, String> {
    use std::os::unix::fs::PermissionsExt as _;
    use std::sync::Arc;

    use ancestry::Procs as _;

    let dir = socket_dir();
    private_dir(&dir)?;
    sweep(&dir);
    let pid = std::process::id();
    let path = dir.join(format!("mcp-{pid}.sock"));
    // A socket left by a dead host of the same pid.
    let _ = std::fs::remove_file(&path);
    let bind = path.clone();
    let listener = crate::runtime::block_on(async move { tokio::net::UnixListener::bind(bind) })
        .map_err(|err| format!("Could not open the local MCP socket {}: {err}", path.display()))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|err| format!("Could not make {} private: {err}", path.display()))?;
    let start = ancestry::Os.stat(pid).ok_or("This process has no start time.")?.start;
    let listener = Arc::new(listener);
    crate::runtime::runtime().spawn(crate::server::accept_loop(
        "local socket".to_string(),
        move || {
            let listener = listener.clone();
            async move { listener.accept().await.map(|(stream, _)| stream) }
        },
        |stream| {
            tokio::spawn(serve(stream));
        },
    ));
    log::info!("[emcp] local MCP socket on {}", path.display());
    Ok(Listening { path, identity: format!("{pid}:{start}") })
}

/// The longest server-name line a peer may send.
#[cfg(any(target_os = "linux", target_os = "macos"))]
const NAME_LINE_MAX: u64 = 128;

/// How long a peer has for its name line.
#[cfg(any(target_os = "linux", target_os = "macos"))]
const NAME_LINE_WITHIN: std::time::Duration = std::time::Duration::from_secs(10);

/// The run `stream`'s peer belongs to.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn admit(fd: std::os::fd::RawFd) -> Result<SessionId, String> {
    use ancestry::Procs as _;

    let peer = peer::of(fd).map_err(|err| format!("no peer credentials ({err})"))?;
    let refused = |why: &dyn std::fmt::Display| {
        log::info!("[emcp] local socket: refused process {} ({}): {why}", peer.pid, peer::exe_of(peer.pid));
        why.to_string()
    };
    if peer.uid != peer::own_uid() {
        return Err(refused(&"another user's process"));
    }
    let first = ancestry::Os.stat(peer.pid).ok_or_else(|| refused(&ancestry::Refused::Gone))?;
    let bound = {
        let mut bound = bound().lock().unwrap();
        // A bound agent that is gone admits nobody (its pid may be taken).
        bound.retain(|bound| ancestry::Os.stat(bound.pid).is_some_and(|stat| stat.start == bound.start));
        bound.clone()
    };
    let run = ancestry::walk(&ancestry::Os, peer.pid, std::process::id(), &bound).map_err(|why| refused(&why))?;
    // What the walk read was the peer's: it is still there, on its pid.
    if !peer.alive() || ancestry::Os.stat(peer.pid) != Some(first) {
        return Err(refused(&ancestry::Refused::Gone));
    }
    Ok(run)
}

/// One accepted connection: admit, read the name line, answer, serve.
#[cfg(any(target_os = "linux", target_os = "macos"))]
async fn serve(stream: tokio::net::UnixStream) {
    use std::os::fd::AsRawFd as _;
    use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _};

    let admitted = admit(stream.as_raw_fd());
    let (read, mut write) = stream.into_split();
    let run = match admitted {
        Ok(run) => run,
        Err(why) => {
            let _ = write.write_all(format!("refused: {why}\n").as_bytes()).await;
            return;
        }
    };
    let mut read = tokio::io::BufReader::new(read);
    let mut line = Vec::new();
    let named = tokio::time::timeout(NAME_LINE_WITHIN, (&mut read).take(NAME_LINE_MAX).read_until(b'\n', &mut line)).await;
    if !matches!(named, Ok(Ok(_))) || line.last() != Some(&b'\n') {
        let _ = write.write_all(b"refused: the first line names the server\n").await;
        return;
    }
    let name = String::from_utf8_lossy(&line).trim().to_string();
    let Some(entry) = entry(&name).filter(|entry| entry.admits(&run)) else {
        log::info!("[emcp] local socket: {run} holds no grant on {name:?}");
        let _ = write.write_all(format!("refused: this run holds no grant on {name:?}\n").as_bytes()).await;
        return;
    };
    if write.write_all(b"ok\n").await.is_err() {
        return;
    }
    log::info!("[emcp] local socket: {name} for {run}");
    entry.serve(run, read, write).await;
}
