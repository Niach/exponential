//! VAPP-167: the daemon PROXIES a CLI's steer viewer over a local socket.
//!
//! The CLI and the daemon of one install share one eNet identity
//! (`cli-enet-identity`), and two processes on one endpoint id knock each
//! other off the relay — so while the daemon runs, `exponential sessions log
//! --follow` cannot start a node of its own. Instead it asks the daemon: one
//! connection to the daemon's Unix socket, one [`FollowRequest`] line; the
//! daemon runs a [`crate::spawn_viewer`] over ITS node and writes every
//! [`ViewerEvent`] back as one JSON line, until the phase is terminal or the
//! CLI hangs up (which shuts the viewer down). The socket is the user's own
//! (mode 0600 in the user's data dir): it serves what the daemon's
//! account may view, which is what the CLI's account may view.

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::Shutdown;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::viewer::{ViewerEvent, ViewerHandle};

/// How long a fresh connection may take to send its request line.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// The one line a CLI sends after connecting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowRequest {
    pub session_id: String,
    /// The synced row already says ended
    /// ([`ViewerHandle::note_session_ended`]).
    #[serde(default)]
    pub ended: bool,
}

/// Starts one viewer for a request (production: [`crate::spawn_viewer`] over
/// the daemon's install node).
pub type SpawnViewer = Arc<dyn Fn(String, flume::Sender<ViewerEvent>) -> ViewerHandle + Send + Sync>;

/// The serving end, owned by the daemon. Dropping it stops it.
pub struct ViewerProxy {
    path: PathBuf,
    stopped: Arc<AtomicBool>,
}

/// Bind `path` (a stale socket file from an unclean exit is replaced) and
/// serve each connection on its own thread.
pub fn serve(path: &Path, spawn: SpawnViewer) -> std::io::Result<ViewerProxy> {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    let stopped = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stopped);
    std::thread::Builder::new().name("viewer-proxy".into()).spawn(move || {
        for stream in listener.incoming() {
            if flag.load(Ordering::SeqCst) {
                break;
            }
            let Ok(stream) = stream else { continue };
            let spawn = Arc::clone(&spawn);
            let _ = std::thread::Builder::new()
                .name("viewer-proxy-conn".into())
                .spawn(move || serve_one(stream, spawn));
        }
    })?;
    Ok(ViewerProxy { path: path.to_path_buf(), stopped })
}

impl ViewerProxy {
    /// Stop accepting and remove the socket file. Idempotent; connections
    /// already being served run to their end.
    pub fn stop(&self) {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        // Wake the blocking accept so the thread sees the flag.
        let _ = UnixStream::connect(&self.path);
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Drop for ViewerProxy {
    fn drop(&mut self) {
        self.stop();
    }
}

fn serve_one(stream: UnixStream, spawn: SpawnViewer) {
    let _ = stream.set_read_timeout(Some(REQUEST_TIMEOUT));
    let Ok(reader) = stream.try_clone() else { return };
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let Ok(request) = serde_json::from_str::<FollowRequest>(line.trim()) else {
        log::debug!("viewer proxy: unreadable request");
        return;
    };
    let _ = stream.set_read_timeout(None);
    let (events_tx, events_rx) = flume::unbounded();
    let handle = spawn(request.session_id.clone(), events_tx);
    if request.ended {
        handle.note_session_ended();
    }
    log::info!("viewer proxy: following {} for a local CLI", request.session_id);
    // The CLI says nothing more: EOF (or an error) is its hang-up.
    let gone = Arc::new(AtomicBool::new(false));
    let hung_up = Arc::clone(&gone);
    std::thread::spawn(move || {
        let mut sink = Vec::new();
        let _ = std::io::Read::read_to_end(&mut reader, &mut sink);
        hung_up.store(true, Ordering::SeqCst);
    });
    let mut writer = &stream;
    while !gone.load(Ordering::SeqCst) {
        let event = match events_rx.recv_timeout(Duration::from_millis(500)) {
            Ok(event) => event,
            Err(flume::RecvTimeoutError::Timeout) => continue,
            Err(flume::RecvTimeoutError::Disconnected) => break,
        };
        let terminal = matches!(&event, ViewerEvent::Phase(phase) if phase.is_terminal());
        let Ok(mut json) = serde_json::to_string(&event) else { continue };
        json.push('\n');
        if writer.write_all(json.as_bytes()).is_err() || terminal {
            break;
        }
    }
    handle.shutdown();
    let _ = stream.shutdown(Shutdown::Both);
}

/// The CLI's end: a connection to the daemon's proxy. Events arrive on the
/// channel given to [`follow`]; it disconnects when the daemon closes the
/// stream (a terminal phase, or the daemon went away).
pub struct ProxiedViewer {
    stream: UnixStream,
}

impl ProxiedViewer {
    /// Hang up: the daemon shuts its viewer down. Idempotent.
    pub fn shutdown(&self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

impl Drop for ProxiedViewer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Ask the daemon at `path` to follow `session_id`. `Err` = no proxy there
/// (no daemon, or one too old to serve viewers).
pub fn follow(
    path: &Path,
    session_id: &str,
    ended: bool,
    events_tx: flume::Sender<ViewerEvent>,
) -> std::io::Result<ProxiedViewer> {
    let mut stream = UnixStream::connect(path)?;
    let mut request = serde_json::to_string(&FollowRequest { session_id: session_id.to_string(), ended })?;
    request.push('\n');
    stream.write_all(request.as_bytes())?;
    let reader = stream.try_clone()?;
    std::thread::Builder::new().name("viewer-proxy-client".into()).spawn(move || {
        for line in BufReader::new(reader).lines() {
            let Ok(line) = line else { break };
            match serde_json::from_str::<ViewerEvent>(&line) {
                Ok(event) => {
                    if events_tx.send(event).is_err() {
                        break;
                    }
                }
                Err(err) => log::debug!("viewer proxy: unreadable event: {err}"),
            }
        }
    })?;
    Ok(ProxiedViewer { stream })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::{ViewerPhase, DEVICE_OFFLINE};
    use crate::SteerRuntime;

    fn socket(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("vp-{name}-{}.sock", std::process::id()))
    }

    fn next(events: &flume::Receiver<ViewerEvent>) -> Option<ViewerEvent> {
        events.recv_timeout(Duration::from_secs(10)).ok()
    }

    #[test]
    fn events_round_trip_as_json_lines() {
        for event in [
            ViewerEvent::Phase(ViewerPhase::Ended { outcome: Some("exit:0".into()) }),
            ViewerEvent::Phase(ViewerPhase::Live),
            ViewerEvent::Activity(7, crate::ActivityEvent::narration("hi")),
            ViewerEvent::Synced { first_seq: 0, truncated: false },
            ViewerEvent::Keepalive,
        ] {
            let json = serde_json::to_string(&event).unwrap();
            assert!(!json.contains('\n'));
            assert_eq!(serde_json::from_str::<ViewerEvent>(&json).unwrap(), event);
        }
    }

    /// The daemon's viewer runs for the CLI: the request's session id and
    /// `ended` reach it, its events stream back, and a terminal phase closes
    /// the stream (the CLI's channel disconnects).
    #[test]
    fn the_proxy_streams_the_daemons_viewer_until_a_terminal_phase() {
        let runtime = SteerRuntime::new().unwrap();
        let path = socket("stream");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let record = Arc::clone(&seen);
        let rt = Arc::clone(&runtime);
        let proxy = serve(
            &path,
            Arc::new(move |session_id, events_tx: flume::Sender<ViewerEvent>| {
                record.lock().unwrap().push(session_id.clone());
                events_tx.send(ViewerEvent::Activity(3, crate::ActivityEvent::narration("from the daemon"))).unwrap();
                // No eNet source: an ended row ends `device_offline`.
                crate::spawn_viewer(&rt, None, session_id, events_tx)
            }),
        )
        .unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);

        let (events_tx, events) = flume::unbounded();
        let viewer = follow(&path, "sess-1", true, events_tx).unwrap();
        let mut got = Vec::new();
        while let Some(event) = next(&events) {
            got.push(event);
        }
        assert_eq!(seen.lock().unwrap().as_slice(), ["sess-1"]);
        assert_eq!(got.first(), Some(&ViewerEvent::Activity(3, crate::ActivityEvent::narration("from the daemon"))));
        assert_eq!(
            got.last(),
            Some(&ViewerEvent::Phase(ViewerPhase::Ended { outcome: Some(DEVICE_OFFLINE.into()) }))
        );
        assert!(events.is_disconnected());
        viewer.shutdown();

        proxy.stop();
        assert!(!path.exists());
        let (events_tx, _events) = flume::unbounded();
        assert!(follow(&path, "sess-1", false, events_tx).is_err());
    }

    /// A CLI that hangs up shuts the daemon's viewer down.
    #[test]
    fn a_hang_up_stops_the_daemons_viewer() {
        let runtime = SteerRuntime::new().unwrap();
        let path = socket("hangup");
        let (spawned_tx, spawned) = flume::unbounded::<flume::Sender<ViewerEvent>>();
        let rt = Arc::clone(&runtime);
        let _proxy = serve(
            &path,
            Arc::new(move |session_id, events_tx: flume::Sender<ViewerEvent>| {
                spawned_tx.send(events_tx.clone()).unwrap();
                // A live row with no source waits in `Starting` for good.
                crate::spawn_viewer(&rt, None, session_id, events_tx)
            }),
        )
        .unwrap();
        let (events_tx, events) = flume::unbounded();
        let viewer = follow(&path, "sess-live", false, events_tx).unwrap();
        let daemon_side = spawned.recv_timeout(Duration::from_secs(10)).unwrap();
        loop {
            if let Some(ViewerEvent::Phase(ViewerPhase::Starting)) = next(&events) {
                break;
            }
        }
        viewer.shutdown();
        // The serving thread drops its receiver once it saw the hang-up.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !daemon_side.is_disconnected() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(daemon_side.is_disconnected(), "the proxy kept serving after the hang-up");
    }
}
