//! #FEED-295: device-to-device file transfers between ONE person's own
//! machines, the machine half. The server queues `file_send` on the source
//! and `file_receive` on the destination (apps/web `lib/device-transfers.ts`);
//! both run here, on this install's eNet node (enet-core `xfer`): the
//! destination pulls, streamed, resumable, BLAKE3-checked per file, end to
//! end encrypted between the two endpoint ids.
//!
//! The machine's own rules (no prompt: the owner decided a transfer between
//! one person's machines just runs):
//! - every peer endpoint (`peers`: the other machine's live installs,
//!   newest first) must be an install of THIS install's own user, as the
//!   registry lists it (never a teammate's, whatever the command says); the
//!   source serves only those, the destination tries them in order;
//! - paths are absolute or `~/`, never with `..`; a protected path (keys,
//!   keyrings, credentials, the Exponential data dir, system dirs) is
//!   refused as a root and skipped inside a directory; symlinks are never
//!   followed inside a tree (enet-core skips them); the destination is
//!   resolved through its existing parent and must not exist yet;
//! - every transfer is logged (here and in the server's `device_transfers`).
//!
//! Reports (`devices.reportTransfer`): the destination's counters about
//! every 5 s, the source's check about every 30 s; `ok: false` = the
//! transfer already ended (the person cancelled it) and the machine stops.
//! Only the destination's `done` makes it done.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use api::devices::{PendingCommand, TransferReport};
use api::installs::InstallEntry;
use enet_core::{EndpointId, NetError, PathFilter, XferEnd, XferOffer, XferProgress};

use crate::enet::{endpoint_hex, parse_endpoint, InstallNode};

/// The cap a build with transfers advertises (the server refuses a machine
/// without it).
pub const CAP: &str = "file-transfer";
pub const SEND_KIND: &str = "file_send";
pub const RECEIVE_KIND: &str = "file_receive";

/// How often the destination reports its counters (and learns a cancel).
const PROGRESS_EVERY: Duration = Duration::from_secs(5);
/// How often the source checks the transfer is still on.
const SOURCE_CHECK_EVERY: Duration = Duration::from_secs(30);
/// A destination gives up after this long without a byte of progress (the
/// source offline, the link gone).
const STALL_LIMIT: Duration = Duration::from_secs(2 * 60 * 60);
const BACKOFF_MIN: Duration = Duration::from_secs(2);
const BACKOFF_MAX: Duration = Duration::from_secs(60);

/// Home-relative paths never read nor written.
const PROTECTED_HOME: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".pki",
    ".password-store",
    ".local/share/keyrings",
    ".local/share/kwalletd",
    ".local/share/exponential",
    ".config/exponential",
    ".aws",
    ".azure",
    ".kube",
    ".docker",
    ".config/gcloud",
    ".config/gh",
    ".netrc",
    ".git-credentials",
    ".claude",
    ".claude.json",
    ".codex",
    ".mozilla",
    ".config/google-chrome",
    ".config/chromium",
    ".config/BraveSoftware",
    "Library/Keychains",
    "Library/Application Support/Exponential",
];
/// Never read (kernel and device files).
const PROTECTED_READ: &[&str] = &["/proc", "/sys", "/dev"];
/// Never written (the system). macOS resolves `/etc` and `/var` into
/// `/private`; its `/private/tmp` stays writable like Linux's `/tmp`.
const PROTECTED_WRITE: &[&str] = &[
    "/proc", "/sys", "/dev", "/etc", "/boot", "/usr", "/bin", "/sbin", "/lib", "/lib32", "/lib64", "/var", "/System", "/Library",
    "/private/etc", "/private/var",
];

/// Where protected paths are anchored.
#[derive(Clone, Debug)]
pub struct Policy {
    pub home: Option<PathBuf>,
    /// The Exponential data dir(s) of this install.
    pub data_dirs: Vec<PathBuf>,
}

impl Policy {
    pub fn from_env(data_dir: &Path) -> Policy {
        let home = std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from);
        let mut data_dirs = vec![data_dir.to_path_buf()];
        if let Ok(canonical) = data_dir.canonicalize() {
            data_dirs.push(canonical);
        }
        Policy { home, data_dirs }
    }

    fn protected_roots(&self, write: bool) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = self.data_dirs.clone();
        if let Some(home) = &self.home {
            for rel in PROTECTED_HOME {
                roots.push(home.join(rel));
            }
            if let Ok(canonical) = home.canonicalize() {
                if &canonical != home {
                    for rel in PROTECTED_HOME {
                        roots.push(canonical.join(rel));
                    }
                }
            }
        }
        let system = if write { PROTECTED_WRITE } else { PROTECTED_READ };
        roots.extend(system.iter().map(PathBuf::from));
        roots
    }

    /// `Some(reason)` when `path` (absolute, resolved) is protected for a
    /// read (`write` false) or a write.
    pub fn refusal(&self, path: &Path, write: bool) -> Option<String> {
        if let Some(home) = &self.home {
            // The home dir itself may be read (its protected parts are
            // skipped) but never be a destination (it exists anyway).
            if write && path == home {
                return Some(format!("{} is your home directory", path.display()));
            }
        }
        if write && path.parent().is_none() {
            return Some("the file system root".into());
        }
        self.protected_roots(write)
            .iter()
            .find(|root| path.starts_with(root))
            .map(|root| format!("{} is protected ({})", path.display(), root.display()))
    }

    /// The source-side filter for entries inside a tree.
    pub fn read_filter(&self) -> PathFilter {
        let policy = self.clone();
        Arc::new(move |path: &Path| policy.refusal(path, false).is_some())
    }

    /// `~` expanded, absolute, no `..`/`.` parts.
    pub fn expand(&self, raw: &str) -> Result<PathBuf, String> {
        let path = if raw == "~" || raw.starts_with("~/") {
            let home = self.home.as_ref().ok_or("this machine has no home directory")?;
            let rest = raw.trim_start_matches('~').trim_start_matches('/');
            if rest.is_empty() {
                home.clone()
            } else {
                home.join(rest)
            }
        } else {
            PathBuf::from(raw)
        };
        if !path.is_absolute() {
            return Err(format!("{raw} is not an absolute path"));
        }
        // `Path::components` drops an inner `.`: read the raw parts too.
        if raw.split(['/', '\\']).any(|part| part == "." || part == "..")
            || path.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        {
            return Err(format!("{raw} contains . or .."));
        }
        Ok(path)
    }

    /// A source root: expanded, resolved (a symlinked root counts as its
    /// target), not protected.
    pub fn source(&self, raw: &str) -> Result<PathBuf, String> {
        let path = self.expand(raw)?;
        let resolved = path.canonicalize().map_err(|e| format!("{}: {e}", path.display()))?;
        for candidate in [&path, &resolved] {
            if let Some(reason) = self.refusal(candidate, false) {
                return Err(reason);
            }
        }
        Ok(resolved)
    }

    /// A destination: expanded, its nearest existing ancestor resolved (so a
    /// symlinked parent counts as its target), not protected, not existing.
    pub fn destination(&self, raw: &str) -> Result<PathBuf, String> {
        let path = self.expand(raw)?;
        let mut existing = path.as_path();
        let mut rest = Vec::new();
        loop {
            match std::fs::symlink_metadata(existing) {
                Ok(_) => break,
                Err(_) => {
                    rest.push(existing.file_name().ok_or("the destination has no existing parent")?.to_os_string());
                    existing = existing.parent().ok_or("the destination has no existing parent")?;
                }
            }
        }
        let mut resolved = existing.canonicalize().map_err(|e| format!("{}: {e}", existing.display()))?;
        for part in rest.iter().rev() {
            resolved.push(part);
        }
        for candidate in [&path, &resolved] {
            if let Some(reason) = self.refusal(candidate, true) {
                return Err(reason);
            }
        }
        Ok(resolved)
    }
}

/// The command's `peers`: the other machine's install endpoints (hex),
/// newest first. Takes the array, the same array JSON-encoded or
/// comma-separated in a string (a string-only payload column), and an
/// older build's single `peer`.
pub fn payload_peers(payload: &serde_json::Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |raw: &str| {
        let hex = raw.trim().to_ascii_lowercase();
        if !hex.is_empty() && !out.contains(&hex) {
            out.push(hex);
        }
    };
    match &payload["peers"] {
        serde_json::Value::Array(items) => items.iter().filter_map(|v| v.as_str()).for_each(&mut push),
        serde_json::Value::String(text) => match serde_json::from_str::<Vec<String>>(text) {
            Ok(items) => items.iter().for_each(|s| push(s)),
            Err(_) => text.split(',').for_each(&mut push),
        },
        _ => {}
    }
    if let Some(peer) = payload["peer"].as_str() {
        push(peer);
    }
    out
}

/// The `peers` that are installs of `own_user` per the registry `entries`
/// (in the given order, this install's own endpoint left out). A peer the
/// registry lists for anyone else, or not at all, is dropped.
pub fn own_peers(entries: &[InstallEntry], own_user: &str, peers: &[String], own_endpoint: Option<&str>) -> Vec<EndpointId> {
    peers
        .iter()
        .filter(|peer| own_endpoint.is_none_or(|own| !own.eq_ignore_ascii_case(peer)))
        .filter(|peer| entries.iter().any(|e| e.user_id == own_user && e.endpoint_id.eq_ignore_ascii_case(peer)))
        .filter_map(|peer| parse_endpoint(&peer.to_ascii_lowercase()))
        .collect()
}

/// Whether the registry list lacks one of `peers` entirely (a new install
/// the last allowlist pass has not seen: worth one refresh).
fn unknown_peer(entries: &[InstallEntry], peers: &[String]) -> bool {
    peers.iter().any(|peer| !entries.iter().any(|e| e.endpoint_id.eq_ignore_ascii_case(peer)))
}

pub const NOT_OWN: &str = "the other machine is not one of your own installs";

/// [`own_peers`] on the install's registry list, refreshed once when a peer
/// is not on it yet. `Err` = none of them is this user's.
async fn resolve_peers(install: &InstallNode, own_endpoint: &str, peers: &[String]) -> Result<Vec<EndpointId>, String> {
    if peers.is_empty() {
        return Err("Malformed command payload (no peers).".into());
    }
    if unknown_peer(&install.installs(), peers) {
        let passes = install.passes();
        install.refresh();
        let _ = tokio::time::timeout(Duration::from_secs(30), install.passes_at_least(passes + 1)).await;
    }
    let entries = install.installs();
    let own = own_peers(&entries, install.user_id(), peers, Some(own_endpoint));
    if own.len() < peers.len() {
        log::warn!("file transfer: {} of {} peers are not this user's installs (dropped)", peers.len() - own.len(), peers.len());
    }
    if own.is_empty() {
        return Err(NOT_OWN.into());
    }
    Ok(own)
}

/// Runs this install's transfer commands. Cheap to clone.
#[derive(Clone)]
pub struct FileTransfers {
    inner: Arc<Inner>,
}

struct Inner {
    runtime: tokio::runtime::Handle,
    install: InstallNode,
    trpc: Arc<api::TrpcClient>,
    data_dir: PathBuf,
    policy: Policy,
    /// Command ids running here: a redelivered command is a no-op.
    claimed: Mutex<HashSet<String>>,
}

/// The process's transfers: the running install's (the daemon's one node,
/// the desktop app's active account). Hosts set it when the node starts.
static CURRENT: Mutex<Option<FileTransfers>> = Mutex::new(None);

pub fn set_current(transfers: Option<FileTransfers>) {
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = transfers;
}

pub fn current() -> Option<FileTransfers> {
    CURRENT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// A host's `file_send` / `file_receive` arm: `Some((false, why))` when this
/// process cannot run it (complete it so); `None` = started or already
/// running (the task completes it itself).
pub fn dispatch(command: &PendingCommand) -> Option<(bool, String)> {
    match current() {
        Some(transfers) => {
            transfers.start(command);
            None
        }
        None => Some((false, "P2P is not running on this machine.".to_string())),
    }
}

fn field(command: &PendingCommand, key: &str) -> String {
    command.payload[key].as_str().unwrap_or_default().to_string()
}

impl FileTransfers {
    pub fn new(runtime: tokio::runtime::Handle, install: InstallNode, trpc: Arc<api::TrpcClient>, data_dir: PathBuf) -> FileTransfers {
        let policy = Policy::from_env(&data_dir);
        FileTransfers { inner: Arc::new(Inner { runtime, install, trpc, data_dir, policy, claimed: Mutex::new(HashSet::new()) }) }
    }

    /// Starts `command` (a `file_send` or `file_receive`) unless it already
    /// runs here. Returns at once; the task completes the command.
    pub fn start(&self, command: &PendingCommand) {
        {
            let mut claimed = self.inner.claimed.lock().unwrap_or_else(|e| e.into_inner());
            if !claimed.insert(command.id.clone()) {
                return;
            }
        }
        let inner = Arc::clone(&self.inner);
        let command = command.clone();
        self.inner.runtime.spawn(async move {
            let transfer_id = field(&command, "transferId");
            let (ok, message) = if transfer_id.is_empty() || transfer_id.contains(['/', '\\', '.']) {
                (false, "Malformed command payload.".to_string())
            } else {
                match command.kind.as_str() {
                    SEND_KIND => send(&inner, &command, &transfer_id).await,
                    RECEIVE_KIND => receive(&inner, &command, &transfer_id).await,
                    _ => (false, "Unsupported command.".to_string()),
                }
            };
            log::info!("file transfer {transfer_id} ({}): {} ({message})", command.kind, if ok { "ok" } else { "failed" });
            let (trpc, id) = (Arc::clone(&inner.trpc), command.id.clone());
            let _ = tokio::task::spawn_blocking(move || {
                if let Err(err) = api::devices::complete_command(&trpc, &id, ok, Some(&message)) {
                    log::debug!("completeCommand failed (redelivery will retry): {err}");
                }
            })
            .await;
            inner.claimed.lock().unwrap_or_else(|e| e.into_inner()).remove(&command.id);
        });
    }
}

async fn report(inner: &Inner, report: TransferReport) -> Option<api::devices::TransferAnswer> {
    let trpc = Arc::clone(&inner.trpc);
    match tokio::task::spawn_blocking(move || api::devices::report_transfer(&trpc, &report)).await {
        Ok(Ok(answer)) => Some(answer),
        Ok(Err(err)) => {
            log::debug!("file transfer: reportTransfer failed: {err}");
            None
        }
        Err(_) => None,
    }
}

fn base(transfer_id: &str, role: &str, status: &str) -> TransferReport {
    TransferReport { transfer_id: transfer_id.to_string(), role: role.to_string(), status: status.to_string(), ..Default::default() }
}

async fn fail(inner: &Inner, transfer_id: &str, role: &str, error: String) -> (bool, String) {
    report(inner, TransferReport { error: Some(error.clone()), ..base(transfer_id, role, "failed") }).await;
    (false, error)
}

async fn node(install: &InstallNode) -> Option<Arc<enet_core::Node>> {
    for _ in 0..60 {
        if let Some(node) = install.node() {
            return Some(node);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    None
}

async fn send(inner: &Arc<Inner>, command: &PendingCommand, transfer_id: &str) -> (bool, String) {
    let role = "send";
    let Some(node) = node(&inner.install).await else {
        return fail(inner, transfer_id, role, "P2P is not running on the source machine".into()).await;
    };
    let peers = match resolve_peers(&inner.install, &endpoint_hex(&node.endpoint_id()), &payload_peers(&command.payload)).await {
        Ok(p) => p,
        Err(e) => return fail(inner, transfer_id, role, e).await,
    };
    let root = match inner.policy.source(&field(command, "srcPath")) {
        Ok(r) => r,
        Err(e) => return fail(inner, transfer_id, role, format!("source: {e}")).await,
    };
    log::info!("file transfer {transfer_id}: serving {} to {} install(s)", root.display(), peers.len());
    let mut end = node.xfer_offer(XferOffer { id: transfer_id.to_string(), peers, root, refuse: inner.policy.read_filter() });
    if let Some(answer) = report(inner, base(transfer_id, role, "running")).await.filter(|a| !a.ok) {
        node.xfer_withdraw(transfer_id);
        return (answer.status == "done", format!("Transfer {}.", answer.status));
    }
    let outcome = loop {
        tokio::select! {
            changed = end.changed() => {
                if changed.is_err() { break (false, "P2P stopped on the source machine.".to_string()); }
                match end.borrow().clone() {
                    Some(XferEnd::Done) => break (true, "Sent.".to_string()),
                    Some(XferEnd::Failed(e)) => break (false, format!("The destination gave up: {e}")),
                    None => {}
                }
            }
            _ = tokio::time::sleep(SOURCE_CHECK_EVERY) => {
                if let Some(answer) = report(inner, base(transfer_id, role, "running")).await {
                    if !answer.ok {
                        break (answer.status == "done", format!("Transfer {}.", answer.status));
                    }
                }
            }
        }
    };
    node.xfer_withdraw(transfer_id);
    outcome
}

pub fn journal_path(data_dir: &Path, transfer_id: &str) -> PathBuf {
    data_dir.join("transfers").join(format!("{transfer_id}.journal"))
}

/// Whether a retryable error names a peer that cannot serve it now (not
/// this install's offer, not reachable): the next listed peer may.
fn try_next_peer(err: &NetError) -> bool {
    matches!(err, NetError::NotFound(_) | NetError::Connect { .. })
}

async fn receive(inner: &Arc<Inner>, command: &PendingCommand, transfer_id: &str) -> (bool, String) {
    let role = "receive";
    let Some(node) = node(&inner.install).await else {
        return fail(inner, transfer_id, role, "P2P is not running on the destination machine".into()).await;
    };
    let peers = match resolve_peers(&inner.install, &endpoint_hex(&node.endpoint_id()), &payload_peers(&command.payload)).await {
        Ok(p) => p,
        Err(e) => return fail(inner, transfer_id, role, e).await,
    };
    let journal = journal_path(&inner.data_dir, transfer_id);
    // A resumed transfer's destination exists already: resolve it as is
    // (enet-core refuses an existing one only on a fresh transfer).
    let dest = match inner.policy.destination(&field(command, "destPath")) {
        Ok(d) => d,
        Err(e) => return fail(inner, transfer_id, role, format!("destination: {e}")).await,
    };
    log::info!("file transfer {transfer_id}: receiving into {} ({} peer install(s))", dest.display(), peers.len());
    let progress = Arc::new(XferProgress::default());
    let mut backoff = BACKOFF_MIN;
    let mut last_progress = (Instant::now(), 0u64);
    // `at` = the peer tried next; `misses` = peers in a row that could not
    // serve it (a full round of them = wait before the next).
    let (mut at, mut misses) = (0usize, 0usize);
    loop {
        let peer = peers[at % peers.len()];
        let fetch = {
            let (node, dest, journal, progress, id) = (Arc::clone(&node), dest.clone(), journal.clone(), Arc::clone(&progress), transfer_id.to_string());
            tokio::spawn(async move { node.xfer_fetch(peer, &id, &dest, &journal, progress).await })
        };
        let abort = fetch.abort_handle();
        let mut fetch = fetch;
        let result = loop {
            tokio::select! {
                r = &mut fetch => break r.unwrap_or_else(|e| Err(NetError::Transport(e.to_string()))),
                _ = tokio::time::sleep(PROGRESS_EVERY) => {
                    let done = progress.bytes_done.load(Ordering::Relaxed);
                    if done > last_progress.1 { last_progress = (Instant::now(), done); }
                    let answer = report(inner, TransferReport {
                        files: Some(progress.files.load(Ordering::Relaxed)),
                        files_done: Some(progress.files_done.load(Ordering::Relaxed)),
                        bytes: Some(progress.bytes.load(Ordering::Relaxed)),
                        bytes_done: Some(done),
                        ..base(transfer_id, role, "running")
                    }).await;
                    if let Some(answer) = answer.filter(|a| !a.ok) {
                        // Wait for the abort to land: its file workers stop with it
                        // (no byte is written after the command completes).
                        abort.abort();
                        let _ = (&mut fetch).await;
                        let _ = node.xfer_fail(peer, transfer_id, answer.status.clone()).await;
                        // An ended transfer never resumes: its journal goes (what arrived stays).
                        let _ = tokio::fs::remove_file(&journal).await;
                        return (false, format!("Transfer {} on the server.", answer.status));
                    }
                }
            }
        };
        match result {
            Ok(summary) => {
                let _ = tokio::fs::remove_file(&journal).await;
                report(
                    inner,
                    TransferReport {
                        files: Some(summary.files),
                        files_done: Some(summary.files),
                        bytes: Some(summary.bytes),
                        bytes_done: Some(summary.bytes),
                        skipped: Some(summary.skipped.clone()),
                        ..base(transfer_id, role, "done")
                    },
                )
                .await;
                let resumed = if summary.resumed_files > 0 { format!(" ({} already there from an earlier attempt)", summary.resumed_files) } else { String::new() };
                let skipped = if summary.skipped.is_empty() { String::new() } else { format!(", {} skipped", summary.skipped.len()) };
                return (true, format!("Received {} files, {} bytes{resumed}{skipped}, every checksum verified.", summary.files, summary.bytes));
            }
            Err(e) if enet_core::xfer_retryable(&e) && last_progress.0.elapsed() < STALL_LIMIT => {
                // Progress since the last try resets the wait.
                let done = progress.bytes_done.load(Ordering::Relaxed);
                if done > last_progress.1 {
                    last_progress = (Instant::now(), done);
                    backoff = BACKOFF_MIN;
                    misses = 0;
                }
                let next = try_next_peer(&e);
                if next {
                    at += 1;
                    misses += 1;
                }
                if next && misses < peers.len() {
                    continue;
                }
                misses = 0;
                log::info!("file transfer {transfer_id}: {e}; retrying in {}s", backoff.as_secs());
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
                if let Some(answer) = report(inner, base(transfer_id, role, "running")).await {
                    if !answer.ok {
                        let _ = tokio::fs::remove_file(&journal).await;
                        return (false, format!("Transfer {} on the server.", answer.status));
                    }
                }
            }
            Err(e) => {
                let error = if enet_core::xfer_retryable(&e) { format!("no progress for {} min: {e}", STALL_LIMIT.as_secs() / 60) } else { e.to_string() };
                let _ = node.xfer_fail(peer, transfer_id, error.clone()).await;
                let _ = tokio::fs::remove_file(&journal).await;
                return fail(inner, transfer_id, role, error).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(home: &Path, data: &Path) -> Policy {
        Policy { home: Some(home.to_path_buf()), data_dirs: vec![data.to_path_buf()] }
    }

    #[test]
    fn protected_paths_are_refused() {
        let p = policy(Path::new("/home/u"), Path::new("/home/u/.local/share/exp-data"));
        for (path, write) in [
            ("/home/u/.ssh", false),
            ("/home/u/.ssh/id_ed25519", false),
            ("/home/u/.local/share/keyrings/login.keyring", false),
            ("/home/u/.local/share/exp-data/accounts", true),
            ("/home/u/.claude/.credentials.json", false),
            ("/proc/self/environ", false),
            ("/etc/x", true),
            ("/private/etc/hosts", true),
            ("/private/var/db/x", true),
            ("/home/u", true),
            ("/", true),
        ] {
            assert!(p.refusal(Path::new(path), write).is_some(), "{path} ({write}) must be refused");
        }
        for (path, write) in [("/home/u/Music", false), ("/home/u/Music/new", true), ("/home/u", false), ("/run/media/sd/wow", true), ("/etc/hostname", false), ("/private/tmp/x", true)] {
            assert!(p.refusal(Path::new(path), write).is_none(), "{path} ({write}) is allowed");
        }
        // Prefix by component, not by string.
        assert!(p.refusal(Path::new("/home/u/.sshkeys-backup"), false).is_none());
        // Inside a tree the same rule skips an entry.
        let skip = p.read_filter();
        assert!(skip(Path::new("/home/u/.ssh/id_ed25519")) && skip(Path::new("/home/u/.local/share/exp-data/x")));
        assert!(!skip(Path::new("/home/u/Music/a.ogg")));
    }

    #[test]
    fn expand_takes_absolute_or_home_paths_without_dots() {
        let p = policy(Path::new("/home/u"), Path::new("/d"));
        assert_eq!(p.expand("~/a/b").unwrap(), PathBuf::from("/home/u/a/b"));
        assert_eq!(p.expand("~").unwrap(), PathBuf::from("/home/u"));
        assert_eq!(p.expand("/x/y").unwrap(), PathBuf::from("/x/y"));
        for bad in ["a/b", "~/a/../.ssh", "/x/./y", "~user/x", "/x/.."] {
            assert!(p.expand(bad).is_err(), "{bad} must be refused");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_into_a_protected_dir_is_refused_on_both_sides() {
        // Under /tmp: macOS's per-user temp dir resolves into /private/var.
        let dir = PathBuf::from("/tmp").join(format!("steer-ft-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let home = dir.join("home");
        std::fs::create_dir_all(home.join(".ssh")).unwrap();
        std::fs::write(home.join(".ssh/id"), b"k").unwrap();
        std::fs::create_dir_all(home.join("music")).unwrap();
        std::os::unix::fs::symlink(home.join(".ssh"), home.join("innocent")).unwrap();
        let home = home.canonicalize().unwrap();
        let p = policy(&home, &dir.join("data"));
        assert!(p.source("~/innocent").unwrap_err().contains("protected"));
        assert!(p.destination("~/innocent/new").unwrap_err().contains("protected"));
        assert_eq!(p.source("~/music").unwrap(), home.join("music"));
        assert_eq!(p.destination("~/music/new/deeper").unwrap(), home.join("music/new/deeper"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn entry(user: &str, endpoint: &str) -> InstallEntry {
        InstallEntry {
            install_id: format!("inst-{}", &endpoint[..8]),
            user_id: user.to_string(),
            kind: "desktop".to_string(),
            label: None,
            platform: None,
            endpoint_id: endpoint.to_string(),
            x25519_public: String::new(),
            last_seen_at: None,
            device_id: Some("dev-1".to_string()),
        }
    }

    fn hex() -> String {
        endpoint_hex(&enet_core::Identity::generate().endpoint_id())
    }

    #[test]
    fn only_the_own_users_installs_pass_the_peer_check() {
        let me = "0b4e5a1c-7f2d-4c1e-9a3b-2f6d8e0c1a55";
        let teammate = "9c1d2e3f-4a5b-4c6d-8e7f-0a1b2c3d4e5f";
        let (mine, theirs, unknown, own) = (hex(), hex(), hex(), hex());
        let entries = vec![entry(me, &mine), entry(teammate, &theirs), entry(me, &own)];
        // Mine passes (any case); a teammate's and an unlisted one never do.
        assert_eq!(own_peers(&entries, me, &[mine.to_ascii_uppercase()], Some(own.as_str())), vec![parse_endpoint(&mine).unwrap()]);
        assert!(own_peers(&entries, me, std::slice::from_ref(&theirs), Some(own.as_str())).is_empty(), "another user's install is refused");
        assert!(own_peers(&entries, me, std::slice::from_ref(&unknown), Some(own.as_str())).is_empty());
        // The same endpoint listed for the teammate is refused even with my user id elsewhere.
        assert!(own_peers(&[entry(teammate, &mine)], me, std::slice::from_ref(&mine), None).is_empty());
        // A mixed list keeps only mine, in order; this install itself is never a peer.
        let list = vec![theirs.clone(), own.clone(), mine.clone(), "not-hex".to_string()];
        assert_eq!(own_peers(&entries, me, &list, Some(own.as_str())), vec![parse_endpoint(&mine).unwrap()]);
        assert!(unknown_peer(&entries, std::slice::from_ref(&unknown)));
        assert!(!unknown_peer(&entries, &[mine.clone(), theirs.clone()]));
    }

    #[test]
    fn payload_peers_reads_the_list_and_older_shapes() {
        let (a, b) = (hex(), hex());
        let list = serde_json::json!({ "peers": [a, b, a.to_ascii_uppercase()] });
        assert_eq!(payload_peers(&list), vec![a.clone(), b.clone()]);
        let encoded = serde_json::json!({ "peers": serde_json::to_string(&[&a, &b]).unwrap() });
        assert_eq!(payload_peers(&encoded), vec![a.clone(), b.clone()]);
        let comma = serde_json::json!({ "peers": format!("{a}, {b}") });
        assert_eq!(payload_peers(&comma), vec![a.clone(), b.clone()]);
        assert_eq!(payload_peers(&serde_json::json!({ "peer": a })), vec![a.clone()]);
        assert!(payload_peers(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn a_peer_that_cannot_serve_moves_to_the_next() {
        assert!(try_next_peer(&NetError::NotFound("unknown transfer".into())));
        assert!(try_next_peer(&NetError::Connect { peer: "p".into(), reason: "no route".into() }));
        assert!(!try_next_peer(&NetError::Transport("reset".into())));
    }

    #[test]
    fn the_journal_lives_in_the_data_dir() {
        assert_eq!(journal_path(Path::new("/d"), "t1"), PathBuf::from("/d/transfers/t1.journal"));
    }
}
