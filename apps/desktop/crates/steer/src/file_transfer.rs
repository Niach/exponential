//! #FEED-295: device-to-device file transfers between ONE person's own
//! machines, the machine half. The server queues `file_send` on the source
//! and `file_receive` on the destination (apps/web `lib/device-transfers.ts`);
//! both run here, on this install's eNet node (enet-core `xfer`): the
//! destination pulls, streamed, resumable, BLAKE3-checked per file, end to
//! end encrypted between the two endpoint ids.
//!
//! The machine's own rules (no prompt: the owner decided a transfer between
//! one person's machines just runs):
//! - the peer must be an install of THIS install's own user, as the
//!   registry lists it (never a teammate's, whatever the command says);
//! - paths are absolute or `~/`, never with `..`; a protected path (keys,
//!   keyrings, credentials, the Exponential data dir, system dirs) is
//!   refused as a root and skipped inside a directory; symlinks are never
//!   followed inside a tree (enet-core skips them); the destination is
//!   resolved through its existing parent and must not exist yet;
//! - every transfer is logged (here and in the server's `device_transfers`).

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use api::devices::{PendingCommand, TransferReport};
use enet_core::{EndpointId, NetError, PathFilter, XferEnd, XferOffer, XferProgress};

use crate::enet::{parse_endpoint, InstallNode};

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
/// process cannot run it (complete it so); `None` = started (it completes
/// itself).
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
            let (ok, message) = match command.kind.as_str() {
                SEND_KIND => send(&inner, &command).await,
                RECEIVE_KIND => receive(&inner, &command).await,
                _ => (false, "Unsupported command.".to_string()),
            };
            log::info!("file transfer {transfer_id} ({}): {} — {message}", command.kind, if ok { "ok" } else { "failed" });
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

/// The peer's endpoint when it is an install of THIS install's own user
/// (one refresh of the registry list if it is not there yet).
async fn own_peer(install: &InstallNode, peer: &str) -> Result<EndpointId, String> {
    let endpoint = parse_endpoint(peer).ok_or("malformed peer endpoint")?;
    for attempt in 0..2 {
        let own = install.installs().iter().any(|e| e.user_id == install.user_id() && e.endpoint_id.eq_ignore_ascii_case(peer));
        if own {
            return Ok(endpoint);
        }
        if attempt == 0 {
            let passes = install.passes();
            install.refresh();
            let _ = tokio::time::timeout(Duration::from_secs(30), install.passes_at_least(passes + 1)).await;
        }
    }
    Err("the other machine is not one of your own installs".into())
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

async fn send(inner: &Arc<Inner>, command: &PendingCommand) -> (bool, String) {
    let transfer_id = field(command, "transferId");
    let role = "send";
    let Some(node) = node(&inner.install).await else {
        return fail(inner, &transfer_id, role, "P2P is not running on the source machine".into()).await;
    };
    let peer = match own_peer(&inner.install, &field(command, "peer")).await {
        Ok(p) => p,
        Err(e) => return fail(inner, &transfer_id, role, e).await,
    };
    let root = match inner.policy.source(&field(command, "srcPath")) {
        Ok(r) => r,
        Err(e) => return fail(inner, &transfer_id, role, format!("source: {e}")).await,
    };
    log::info!("file transfer {transfer_id}: serving {} to {}", root.display(), peer.fmt_short());
    let mut end = node.xfer_offer(XferOffer { id: transfer_id.clone(), peer, root, refuse: inner.policy.read_filter() });
    let outcome = loop {
        tokio::select! {
            changed = end.changed() => {
                if changed.is_err() { break (false, "the node stopped".to_string()); }
                match end.borrow().clone() {
                    Some(XferEnd::Done) => break (true, "Sent.".to_string()),
                    Some(XferEnd::Failed(e)) => break (false, format!("The destination gave up: {e}")),
                    None => {}
                }
            }
            _ = tokio::time::sleep(SOURCE_CHECK_EVERY) => {
                if let Some(answer) = report(inner, base(&transfer_id, role, "running")).await {
                    if !answer.ok {
                        break (answer.status == "done", format!("Transfer {}.", answer.status));
                    }
                }
            }
        }
    };
    node.xfer_withdraw(&transfer_id);
    outcome
}

fn journal_path(data_dir: &Path, transfer_id: &str) -> PathBuf {
    data_dir.join("transfers").join(format!("{transfer_id}.journal"))
}

async fn receive(inner: &Arc<Inner>, command: &PendingCommand) -> (bool, String) {
    let transfer_id = field(command, "transferId");
    let role = "receive";
    if transfer_id.is_empty() || transfer_id.contains(['/', '\\', '.']) {
        return (false, "Malformed command payload.".into());
    }
    let Some(node) = node(&inner.install).await else {
        return fail(inner, &transfer_id, role, "P2P is not running on the destination machine".into()).await;
    };
    let peer = match own_peer(&inner.install, &field(command, "peer")).await {
        Ok(p) => p,
        Err(e) => return fail(inner, &transfer_id, role, e).await,
    };
    let journal = journal_path(&inner.data_dir, &transfer_id);
    // A resumed transfer's destination exists already: resolve it as is.
    let dest = match inner.policy.destination(&field(command, "destPath")) {
        Ok(d) => d,
        Err(e) => return fail(inner, &transfer_id, role, format!("destination: {e}")).await,
    };
    log::info!("file transfer {transfer_id}: receiving from {} into {}", peer.fmt_short(), dest.display());
    let progress = Arc::new(XferProgress::default());
    let mut backoff = Duration::from_secs(2);
    let mut last_progress = (Instant::now(), 0u64);
    loop {
        let fetch = {
            let (node, dest, journal, progress, id) = (Arc::clone(&node), dest.clone(), journal.clone(), Arc::clone(&progress), transfer_id.clone());
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
                        ..base(&transfer_id, role, "running")
                    }).await;
                    if answer.is_some_and(|a| !a.ok) {
                        abort.abort();
                        let _ = node.xfer_fail(peer, &transfer_id, "cancelled".into()).await;
                        return (false, "Transfer ended on the server (cancelled).".into());
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
                        ..base(&transfer_id, role, "done")
                    },
                )
                .await;
                let resumed = if summary.resumed_files > 0 { format!(" ({} already there from an earlier attempt)", summary.resumed_files) } else { String::new() };
                return (true, format!("Received {} files, {} bytes{resumed}, every checksum verified.", summary.files, summary.bytes));
            }
            Err(e) if enet_core::xfer_retryable(&e) && last_progress.0.elapsed() < STALL_LIMIT => {
                log::info!("file transfer {transfer_id}: {e}; retrying in {}s", backoff.as_secs());
                // Progress since the last try resets the wait.
                let done = progress.bytes_done.load(Ordering::Relaxed);
                if done > last_progress.1 {
                    last_progress = (Instant::now(), done);
                    backoff = Duration::from_secs(2);
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
                if let Some(answer) = report(inner, base(&transfer_id, role, "running")).await {
                    if !answer.ok {
                        return (false, format!("Transfer {}.", answer.status));
                    }
                }
            }
            Err(e) => {
                let error = e.to_string();
                let _ = node.xfer_fail(peer, &transfer_id, error.clone()).await;
                return fail(inner, &transfer_id, role, error).await;
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
}
