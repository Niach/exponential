//! Files (#FEED-295): a file or a whole directory copied between two
//! nodes, streamed over QUIC (so end-to-end encrypted between the two
//! endpoint ids; a relay forwards ciphertext only), resumable, no size cap,
//! every file's BLAKE3 checked at its end.
//!
//! The SOURCE registers an [`XferOffer`] (a transfer id, the ONE peer that
//! may read it, a root path, the app's refusal rule); the RECEIVER dials it
//! on [`XFER_ALPN`] and pulls. Nothing else on the source is readable: a
//! read names a path of the offer's manifest, and a peer other than the
//! offer's gets the same "unknown transfer" as a wrong id.
//!
//! Wire (`exp/enet/xfer/1`), one bidirectional stream per request, control
//! frames = `[u32 BE len][JSON]` (util `write_frame`):
//! - `{op:"manifest", id}` → `{ok, manifest:{dir, dirs, files:[{path, size,
//!   mode}], skipped}}`: the walk of the root (symlinks, special files and
//!   paths the app refuses are SKIPPED and named, never followed);
//! - `{op:"read", id, path, offset}` → `{ok, size, from}` then the raw bytes
//!   `[from, size)` then the 32-byte BLAKE3 of the WHOLE file `[0, size)`
//!   (`from` = `offset`, or 0 when the file shrank under it);
//! - `{op:"end", id, ok, error?}` → `{ok}`: the receiver is finished (the
//!   source's [`Node::xfer_offer`] receiver sees [`XferEnd`]).
//!
//! The receiver writes ONLY under its destination: every manifest path is
//! relative with plain components (no `..`, no root, no empty part), every
//! directory on the way is checked to be a real directory (a symlink there
//! is refused, never followed), a file lands as `<name>.exp-part` and is
//! renamed into place once its checksum matches. A journal (one JSON line
//! per finished file) makes a retry skip what is done; an unfinished part
//! resumes from its length (its prefix is re-hashed from disk, so the
//! whole-file checksum still covers it). A fresh transfer never writes into
//! an existing destination.
//!
//! [`Node::xfer_offer`]: crate::Node::xfer_offer

use std::{
    collections::{HashMap, VecDeque},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use iroh::{
    endpoint::{Connection, RecvStream, SendStream},
    protocol::{AcceptError, ProtocolHandler},
    EndpointId,
};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{watch, OnceCell};

use crate::{
    error::{transport, NetError, Result},
    node::{classify, Shared},
    util::{read_frame, write_frame},
};

pub const XFER_ALPN: &[u8] = b"exp/enet/xfer/1";
/// The suffix of a file still being received.
pub const PART_SUFFIX: &str = ".exp-part";
/// Files fetched at once (one stream each).
pub const XFER_PARALLEL: usize = 4;
const CHUNK: usize = 256 * 1024;
const MAX_REQUEST: usize = 64 * 1024;
/// A manifest frame (a tree of a million files fits).
const MAX_MANIFEST: usize = 256 * 1024 * 1024;
const UNKNOWN: &str = "unknown transfer";

/// The app's refusal rule for a source path (true = skip it).
pub type PathFilter = Arc<dyn Fn(&Path) -> bool + Send + Sync>;

/// What a source serves to ONE peer.
#[derive(Clone)]
pub struct XferOffer {
    pub id: String,
    pub peer: EndpointId,
    /// The file or directory, as the app resolved it.
    pub root: PathBuf,
    pub refuse: PathFilter,
}

impl std::fmt::Debug for XferOffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XferOffer").field("id", &self.id).field("peer", &self.peer.fmt_short().to_string()).field("root", &self.root).finish()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct XferFile {
    /// `/`-separated, relative to the root ("" = the root is this file).
    pub path: String,
    pub size: u64,
    pub mode: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct XferManifest {
    /// The root is a directory (else exactly one file, path "").
    pub dir: bool,
    /// Every directory under the root, parents first.
    pub dirs: Vec<String>,
    pub files: Vec<XferFile>,
    /// Symlinks, special files and refused paths: named, never sent.
    pub skipped: Vec<String>,
}

impl XferManifest {
    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

/// How a source's offer ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum XferEnd {
    Done,
    Failed(String),
}

/// Live counters of a fetch (read them from anywhere while it runs).
#[derive(Debug, Default)]
pub struct XferProgress {
    pub files: AtomicU64,
    pub files_done: AtomicU64,
    pub bytes: AtomicU64,
    pub bytes_done: AtomicU64,
}

/// A finished fetch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XferSummary {
    pub dir: bool,
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    /// Files a journal already had (a resumed transfer).
    pub resumed_files: u64,
    pub skipped: Vec<String>,
}

// --- paths ------------------------------------------------------------------

/// A manifest path as a relative path of plain components, or `Invalid`.
pub fn safe_rel(path: &str) -> Result<PathBuf> {
    if path.is_empty() {
        return Ok(PathBuf::new());
    }
    let mut out = PathBuf::new();
    for part in path.split('/') {
        if part.is_empty() || part.contains('\\') || part.contains('\0') {
            return Err(NetError::Invalid(format!("unsafe path {path:?}")));
        }
        let mut comps = Path::new(part).components();
        match (comps.next(), comps.next()) {
            (Some(Component::Normal(c)), None) if c == part => out.push(c),
            _ => return Err(NetError::Invalid(format!("unsafe path {path:?}"))),
        }
    }
    Ok(out)
}

fn rel_string(rel: &Path) -> Option<String> {
    let parts: Option<Vec<&str>> = rel.components().map(|c| match c {
        Component::Normal(s) => s.to_str(),
        _ => None,
    }).collect();
    parts.map(|p| p.join("/"))
}

fn part_path(final_path: &Path) -> PathBuf {
    let mut name = final_path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(PART_SUFFIX);
    final_path.with_file_name(name)
}

/// Every directory from `dest` down to `dest/rel` is a real directory (a
/// symlink anywhere is refused); missing ones are created when `create`.
async fn ensure_dirs(dest: &Path, rel: &Path, create: bool) -> Result<()> {
    let mut at = dest.to_path_buf();
    check_dir(&at).await?;
    for comp in rel.components() {
        at.push(comp);
        match tokio::fs::symlink_metadata(&at).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => match tokio::fs::create_dir(&at).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(io(&at, e)),
            },
            Err(e) => return Err(io(&at, e)),
        }
        check_dir(&at).await?;
    }
    Ok(())
}

async fn check_dir(path: &Path) -> Result<()> {
    let meta = tokio::fs::symlink_metadata(path).await.map_err(|e| io(path, e))?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(NetError::Invalid(format!("{} is not a plain directory", path.display())));
    }
    Ok(())
}

fn io(path: &Path, e: std::io::Error) -> NetError {
    NetError::Invalid(format!("{}: {e}", path.display()))
}

#[cfg(unix)]
fn file_mode(meta: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o777
}
#[cfg(not(unix))]
fn file_mode(_: &std::fs::Metadata) -> u32 {
    0o644
}

#[cfg(unix)]
async fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    // Never setuid/setgid/sticky; the owner can always read and write it.
    let mode = (mode & 0o777) | 0o600;
    let _ = tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).await;
}
#[cfg(not(unix))]
async fn set_mode(_: &Path, _: u32) {}

/// Walks a source root into its manifest. Symlinks are never followed.
pub async fn build_manifest(root: &Path, refuse: &PathFilter) -> Result<XferManifest> {
    let meta = tokio::fs::symlink_metadata(root).await.map_err(|e| io(root, e))?;
    if meta.file_type().is_symlink() {
        return Err(NetError::Invalid(format!("{} is a symlink", root.display())));
    }
    if refuse(root) {
        return Err(NetError::Refused(format!("{} is a protected path", root.display())));
    }
    if meta.is_file() {
        return Ok(XferManifest { dir: false, files: vec![XferFile { path: String::new(), size: meta.len(), mode: file_mode(&meta) }], ..Default::default() });
    }
    if !meta.is_dir() {
        return Err(NetError::Invalid(format!("{} is neither a file nor a directory", root.display())));
    }
    let mut manifest = XferManifest { dir: true, ..Default::default() };
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let dir = root.join(&rel);
        let mut entries = tokio::fs::read_dir(&dir).await.map_err(|e| io(&dir, e))?;
        let mut children = Vec::new();
        while let Some(entry) = entries.next_entry().await.map_err(|e| io(&dir, e))? {
            children.push(entry);
        }
        children.sort_by_key(|e| e.file_name());
        let mut subdirs = Vec::new();
        for entry in children {
            let child_rel = rel.join(entry.file_name());
            let Some(name) = rel_string(&child_rel) else {
                manifest.skipped.push(child_rel.to_string_lossy().into_owned());
                continue;
            };
            let full = entry.path();
            let Ok(meta) = tokio::fs::symlink_metadata(&full).await else {
                manifest.skipped.push(name);
                continue;
            };
            if meta.file_type().is_symlink() || refuse(&full) || safe_rel(&name).is_err() {
                manifest.skipped.push(name);
            } else if meta.is_dir() {
                manifest.dirs.push(name);
                subdirs.push(child_rel);
            } else if meta.is_file() {
                manifest.files.push(XferFile { path: name, size: meta.len(), mode: file_mode(&meta) });
            } else {
                manifest.skipped.push(name);
            }
        }
        // Depth first, in name order.
        stack.extend(subdirs.into_iter().rev());
    }
    Ok(manifest)
}

// --- source -----------------------------------------------------------------

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "op", rename_all = "lowercase")]
enum Request {
    Manifest { id: String },
    Read { id: String, path: String, offset: u64 },
    End { id: String, ok: bool, #[serde(default)] error: Option<String> },
}

#[derive(Deserialize, Serialize, Debug, Default)]
struct Reply {
    ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    manifest: Option<XferManifest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    from: Option<u64>,
}

impl Reply {
    fn err(e: impl Into<String>) -> Self {
        Self { ok: false, error: Some(e.into()), ..Default::default() }
    }
}

struct Served {
    manifest: XferManifest,
    sizes: HashMap<String, u64>,
}

struct OfferSlot {
    offer: XferOffer,
    served: OnceCell<std::result::Result<Arc<Served>, String>>,
    end: watch::Sender<Option<XferEnd>>,
}

impl OfferSlot {
    async fn served(&self) -> std::result::Result<Arc<Served>, String> {
        self.served
            .get_or_init(|| async {
                let manifest = build_manifest(&self.offer.root, &self.offer.refuse).await.map_err(|e| e.to_string())?;
                let sizes = manifest.files.iter().map(|f| (f.path.clone(), f.size)).collect();
                Ok(Arc::new(Served { manifest, sizes }))
            })
            .await
            .clone()
    }
}

#[derive(Default)]
pub(crate) struct XferState {
    offers: Mutex<HashMap<String, Arc<OfferSlot>>>,
}

impl XferState {
    pub(crate) fn offer(&self, offer: XferOffer) -> watch::Receiver<Option<XferEnd>> {
        let (end, rx) = watch::channel(None);
        let id = offer.id.clone();
        self.offers.lock().expect("xfer lock").insert(id, Arc::new(OfferSlot { offer, served: OnceCell::new(), end }));
        rx
    }
    pub(crate) fn withdraw(&self, id: &str) -> bool {
        self.offers.lock().expect("xfer lock").remove(id).is_some()
    }
    fn get(&self, id: &str, peer: &EndpointId) -> Option<Arc<OfferSlot>> {
        self.offers.lock().expect("xfer lock").get(id).filter(|s| s.offer.peer == *peer).cloned()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct XferProtocol {
    pub(crate) shared: std::sync::Weak<Shared>,
}

impl ProtocolHandler for XferProtocol {
    async fn accept(&self, conn: Connection) -> std::result::Result<(), AcceptError> {
        let peer = conn.remote_id();
        loop {
            let (send, recv) = match conn.accept_bi().await {
                Ok(s) => s,
                Err(_) => return Ok(()),
            };
            let shared = self.shared.clone();
            n0_future::task::spawn(async move {
                let _ = serve(shared, peer, send, recv).await;
            });
        }
    }
}

async fn reply(send: &mut SendStream, reply: &Reply) -> Result<()> {
    write_frame(send, &serde_json::to_vec(reply).expect("reply serializes")).await
}

async fn serve(shared: std::sync::Weak<Shared>, peer: EndpointId, mut send: SendStream, mut recv: RecvStream) -> Result<()> {
    let Some(frame) = read_frame(&mut recv, MAX_REQUEST).await? else { return Ok(()) };
    let request: Request = serde_json::from_slice(&frame).map_err(|e| NetError::Invalid(e.to_string()))?;
    let id = match &request {
        Request::Manifest { id } | Request::Read { id, .. } | Request::End { id, .. } => id.clone(),
    };
    let slot = shared.upgrade().and_then(|s| s.xfer.get(&id, &peer));
    let Some(slot) = slot else {
        reply(&mut send, &Reply::err(UNKNOWN)).await?;
        let _ = send.finish();
        return Ok(());
    };
    match request {
        Request::Manifest { .. } => {
            let r = match slot.served().await {
                Ok(served) => Reply { ok: true, manifest: Some(served.manifest.clone()), ..Default::default() },
                Err(e) => Reply::err(e),
            };
            reply(&mut send, &r).await?;
        }
        Request::End { ok, error, .. } => {
            let _ = slot.end.send(Some(if ok { XferEnd::Done } else { XferEnd::Failed(error.unwrap_or_default()) }));
            reply(&mut send, &Reply { ok: true, ..Default::default() }).await?;
        }
        Request::Read { path, offset, .. } => {
            let served = match slot.served().await {
                Ok(s) => s,
                Err(e) => {
                    reply(&mut send, &Reply::err(e)).await?;
                    let _ = send.finish();
                    return Ok(());
                }
            };
            if !served.sizes.contains_key(&path) {
                reply(&mut send, &Reply::err("not part of this transfer")).await?;
            } else {
                serve_read(&slot.offer.root, &path, offset, &mut send).await?;
            }
        }
    }
    let _ = send.finish();
    Ok(())
}

async fn serve_read(root: &Path, path: &str, offset: u64, send: &mut SendStream) -> Result<()> {
    let rel = safe_rel(path)?;
    let full = if rel.as_os_str().is_empty() { root.to_path_buf() } else { root.join(rel) };
    let opened = async {
        let meta = tokio::fs::symlink_metadata(&full).await?;
        if !meta.is_file() {
            return Err(std::io::Error::other("no longer a plain file"));
        }
        let file = tokio::fs::File::open(&full).await?;
        let size = file.metadata().await?.len();
        Ok((file, size))
    }
    .await;
    let (mut file, size) = match opened {
        Ok(v) => v,
        Err(e) => return reply(send, &Reply::err(format!("{path}: {e}"))).await,
    };
    let from = if offset <= size { offset } else { 0 };
    reply(send, &Reply { ok: true, size: Some(size), from: Some(from), ..Default::default() }).await?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; CHUNK];
    let mut pos = 0u64;
    while pos < size {
        let want = ((size - pos) as usize).min(CHUNK);
        let n = file.read(&mut buf[..want]).await.map_err(|e| io(&full, e))?;
        if n == 0 {
            // Shrank under us: the receiver's checksum fails and it retries.
            send.reset(1u32.into()).ok();
            return Err(NetError::Invalid(format!("{path} shrank while being read")));
        }
        hasher.update(&buf[..n]);
        if pos >= from {
            send.write_all(&buf[..n]).await.map_err(transport)?;
        } else if pos + n as u64 > from {
            let skip = (from - pos) as usize;
            send.write_all(&buf[skip..n]).await.map_err(transport)?;
        }
        pos += n as u64;
    }
    send.write_all(hasher.finalize().as_bytes()).await.map_err(transport)?;
    Ok(())
}

// --- receiver ---------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct JournalHead {
    v: u32,
    id: String,
    dir: bool,
}

#[derive(Serialize, Deserialize)]
struct JournalLine {
    p: String,
    s: u64,
    h: String,
}

struct Journal {
    file: tokio::sync::Mutex<tokio::fs::File>,
    done: HashMap<String, u64>,
}

impl Journal {
    /// `(journal, fresh)`: fresh = no journal existed (a first attempt).
    async fn open(path: &Path, id: &str, dir: bool) -> Result<(Journal, bool)> {
        let mut done = HashMap::new();
        let fresh = match tokio::fs::read_to_string(path).await {
            Ok(text) => {
                let mut lines = text.lines();
                let head: Option<JournalHead> = lines.next().and_then(|l| serde_json::from_str(l).ok());
                match head {
                    Some(h) if h.id == id && h.dir == dir => {}
                    _ => return Err(NetError::Invalid(format!("{} belongs to another transfer", path.display()))),
                }
                // A torn last line (a crash mid-append) is simply not done.
                for line in lines {
                    if let Ok(l) = serde_json::from_str::<JournalLine>(line) {
                        done.insert(l.p, l.s);
                    }
                }
                false
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
            Err(e) => return Err(io(path, e)),
        };
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| io(parent, e))?;
        }
        let mut file = tokio::fs::OpenOptions::new().create(true).append(true).open(path).await.map_err(|e| io(path, e))?;
        if fresh {
            let head = serde_json::to_string(&JournalHead { v: 1, id: id.to_string(), dir }).expect("head serializes");
            file.write_all(format!("{head}\n").as_bytes()).await.map_err(|e| io(path, e))?;
            file.sync_data().await.map_err(|e| io(path, e))?;
        }
        Ok((Journal { file: tokio::sync::Mutex::new(file), done }, fresh))
    }

    async fn record(&self, path: &str, size: u64, hash: &blake3::Hash) -> Result<()> {
        let line = serde_json::to_string(&JournalLine { p: path.to_string(), s: size, h: hash.to_hex().to_string() }).expect("line serializes");
        let mut file = self.file.lock().await;
        file.write_all(format!("{line}\n").as_bytes()).await.map_err(transport)?;
        Ok(())
    }
}

async fn request(conn: &Connection, body: &Request, max: usize) -> Result<(Reply, RecvStream)> {
    let (mut send, mut recv) = conn.open_bi().await.map_err(|e| classify(conn, transport(e)))?;
    write_frame(&mut send, &serde_json::to_vec(body).expect("request serializes")).await.map_err(|e| classify(conn, e))?;
    let _ = send.finish();
    let frame = read_frame(&mut recv, max).await.map_err(|e| classify(conn, e))?.ok_or_else(|| classify(conn, NetError::Transport("stream closed".into())))?;
    let reply: Reply = serde_json::from_slice(&frame).map_err(|e| NetError::Invalid(e.to_string()))?;
    if !reply.ok {
        let error = reply.error.unwrap_or_default();
        return Err(if error == UNKNOWN { NetError::NotFound(error) } else { NetError::Refused(error) });
    }
    Ok((reply, recv))
}

/// Whether a fetch failure is worth another attempt (the link dropped, the
/// source has not offered yet) rather than final.
pub fn retryable(err: &NetError) -> bool {
    matches!(err, NetError::Transport(_) | NetError::Connect { .. } | NetError::Timeout(_) | NetError::NotFound(_))
}

/// Tells the source the transfer is over (best effort).
pub(crate) async fn end(shared: &Shared, peer: EndpointId, id: &str, error: Option<String>) -> Result<()> {
    let conn = shared.connect(peer, XFER_ALPN).await?;
    let r = request(&conn, &Request::End { id: id.to_string(), ok: error.is_none(), error }, MAX_REQUEST).await;
    conn.close(0u32.into(), b"done");
    r.map(|_| ())
}

pub(crate) async fn fetch(shared: &Shared, peer: EndpointId, id: &str, dest: &Path, journal: &Path, progress: Arc<XferProgress>) -> Result<XferSummary> {
    if !dest.is_absolute() {
        return Err(NetError::Invalid("the destination must be an absolute path".into()));
    }
    let conn = shared.connect(peer, XFER_ALPN).await?;
    let result = fetch_on(&conn, id, dest, journal, progress).await;
    if result.is_ok() {
        let _ = request(&conn, &Request::End { id: id.to_string(), ok: true, error: None }, MAX_REQUEST).await;
    }
    conn.close(0u32.into(), b"done");
    result
}

async fn fetch_on(conn: &Connection, id: &str, dest: &Path, journal_path: &Path, progress: Arc<XferProgress>) -> Result<XferSummary> {
    let (reply, _) = request(conn, &Request::Manifest { id: id.to_string() }, MAX_MANIFEST).await?;
    let manifest = reply.manifest.ok_or_else(|| NetError::Invalid("no manifest".into()))?;
    if !manifest.dir && (manifest.files.len() != 1 || !manifest.files[0].path.is_empty()) {
        return Err(NetError::Invalid("malformed manifest".into()));
    }
    let parent = dest.parent().ok_or_else(|| NetError::Invalid("the destination has no parent".into()))?;
    let (journal, fresh) = Journal::open(journal_path, id, manifest.dir).await?;
    if fresh {
        // A first attempt never writes into what is already there.
        if tokio::fs::symlink_metadata(dest).await.is_ok() {
            let _ = tokio::fs::remove_file(journal_path).await;
            return Err(NetError::Refused(format!("{} already exists", dest.display())));
        }
    }
    tokio::fs::create_dir_all(parent).await.map_err(|e| io(parent, e))?;
    check_dir(parent).await?;
    if manifest.dir {
        match tokio::fs::create_dir(dest).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && !fresh => {}
            Err(e) => return Err(io(dest, e)),
        }
        check_dir(dest).await?;
        for d in &manifest.dirs {
            ensure_dirs(dest, &safe_rel(d)?, true).await?;
        }
    }

    progress.files.store(manifest.files.len() as u64, Ordering::Relaxed);
    progress.bytes.store(manifest.bytes(), Ordering::Relaxed);
    let mut queue = VecDeque::new();
    let mut resumed = 0u64;
    let mut already = 0u64;
    for f in &manifest.files {
        safe_rel(&f.path)?;
        if journal.done.get(&f.path) == Some(&f.size) {
            resumed += 1;
            already += f.size;
        } else {
            queue.push_back(f.clone());
        }
    }
    progress.files_done.store(resumed, Ordering::Relaxed);
    progress.bytes_done.store(already, Ordering::Relaxed);

    let queue = Arc::new(Mutex::new(queue));
    let journal = Arc::new(journal);
    let stop = Arc::new(AtomicBool::new(false));
    let mut workers = Vec::new();
    for _ in 0..XFER_PARALLEL {
        let (conn, queue, journal, stop, progress) = (conn.clone(), queue.clone(), journal.clone(), stop.clone(), progress.clone());
        let (id, dest) = (id.to_string(), dest.to_path_buf());
        workers.push(n0_future::task::spawn(async move {
            loop {
                if stop.load(Ordering::SeqCst) {
                    return Ok(());
                }
                let Some(file) = queue.lock().expect("queue lock").pop_front() else { return Ok(()) };
                if let Err(e) = fetch_file(&conn, &id, &dest, &file, &journal, &progress).await {
                    stop.store(true, Ordering::SeqCst);
                    return Err(e);
                }
            }
        }));
    }
    let mut first_err = None;
    for w in workers {
        let r = w.await.map_err(|e| NetError::Transport(e.to_string())).and_then(|r| r);
        if let (Err(e), None) = (r, &first_err) {
            first_err = Some(e);
        }
    }
    if let Some(e) = first_err {
        return Err(e);
    }
    journal.file.lock().await.sync_all().await.map_err(transport)?;
    Ok(XferSummary {
        dir: manifest.dir,
        files: manifest.files.len() as u64,
        dirs: manifest.dirs.len() as u64,
        bytes: manifest.bytes(),
        resumed_files: resumed,
        skipped: manifest.skipped,
    })
}

async fn fetch_file(conn: &Connection, id: &str, dest: &Path, file: &XferFile, journal: &Journal, progress: &XferProgress) -> Result<()> {
    let rel = safe_rel(&file.path)?;
    let final_path = if rel.as_os_str().is_empty() { dest.to_path_buf() } else { dest.join(&rel) };
    if let Some(parent_rel) = rel.parent() {
        ensure_dirs(dest, parent_rel, true).await?;
    }
    if let Ok(meta) = tokio::fs::symlink_metadata(&final_path).await {
        if meta.is_dir() {
            return Err(NetError::Invalid(format!("{} is a directory", final_path.display())));
        }
    }
    let part = part_path(&final_path);
    for attempt in 0..2 {
        let mut offset = 0u64;
        match tokio::fs::symlink_metadata(&part).await {
            Ok(meta) if meta.is_file() && attempt == 0 => offset = meta.len(),
            Ok(meta) if meta.is_dir() => return Err(NetError::Invalid(format!("{} is a directory", part.display()))),
            Ok(_) => tokio::fs::remove_file(&part).await.map_err(|e| io(&part, e))?,
            Err(_) => {}
        }
        let (reply, mut recv) = request(conn, &Request::Read { id: id.to_string(), path: file.path.clone(), offset }, MAX_REQUEST).await?;
        let size = reply.size.unwrap_or(0);
        let from = reply.from.unwrap_or(0).min(offset);
        let mut out = tokio::fs::OpenOptions::new().create(true).write(true).read(true).truncate(false).open(&part).await.map_err(|e| io(&part, e))?;
        out.set_len(from).await.map_err(|e| io(&part, e))?;
        let mut hasher = blake3::Hasher::new();
        let mut buf = vec![0u8; CHUNK];
        // The resumed prefix counts toward the whole-file checksum.
        let mut left = from;
        while left > 0 {
            let n = out.read(&mut buf[..(left as usize).min(CHUNK)]).await.map_err(|e| io(&part, e))?;
            if n == 0 {
                return Err(NetError::Invalid(format!("{} shrank", part.display())));
            }
            hasher.update(&buf[..n]);
            left -= n as u64;
        }
        out.seek(std::io::SeekFrom::Start(from)).await.map_err(|e| io(&part, e))?;
        progress.bytes_done.fetch_add(from, Ordering::Relaxed);
        let mut counted = from;
        let mut remaining = size - from.min(size);
        let streamed = async {
            while remaining > 0 {
                let want = (remaining as usize).min(CHUNK);
                let n = recv.read(&mut buf[..want]).await.map_err(|e| classify(conn, transport(e)))?.ok_or_else(|| NetError::Transport("stream ended early".into()))?;
                out.write_all(&buf[..n]).await.map_err(|e| io(&part, e))?;
                hasher.update(&buf[..n]);
                remaining -= n as u64;
                counted += n as u64;
                progress.bytes_done.fetch_add(n as u64, Ordering::Relaxed);
            }
            let mut sum = [0u8; 32];
            recv.read_exact(&mut sum).await.map_err(|e| classify(conn, transport(e)))?;
            out.flush().await.map_err(|e| io(&part, e))?;
            out.sync_data().await.map_err(|e| io(&part, e))?;
            Ok::<_, NetError>(sum)
        }
        .await;
        let sum = match streamed {
            Ok(sum) => sum,
            Err(e) => {
                // What landed stays in the part; the counters go back.
                progress.bytes_done.fetch_sub(counted, Ordering::Relaxed);
                return Err(e);
            }
        };
        drop(out);
        let hash = hasher.finalize();
        if hash.as_bytes() == &sum {
            set_mode(&part, file.mode).await;
            tokio::fs::rename(&part, &final_path).await.map_err(|e| io(&final_path, e))?;
            journal.record(&file.path, size, &hash).await?;
            // The manifest's size may be stale (the file changed since).
            if size != file.size {
                progress.bytes.fetch_add(size, Ordering::Relaxed);
                progress.bytes.fetch_sub(file.size, Ordering::Relaxed);
            }
            progress.files_done.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
        progress.bytes_done.fetch_sub(counted, Ordering::Relaxed);
        let _ = tokio::fs::remove_file(&part).await;
    }
    Err(NetError::Invalid(format!("checksum mismatch on {} (twice)", if file.path.is_empty() { "the file" } else { &file.path })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_rel_takes_plain_components_only() {
        assert_eq!(safe_rel("a/b.txt").unwrap(), PathBuf::from("a/b.txt"));
        assert_eq!(safe_rel("").unwrap(), PathBuf::new());
        for bad in ["..", "a/../b", "/etc/passwd", "a//b", "./a", "a/.", "a\\..\\b", "a/\0", "a/"] {
            assert!(safe_rel(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn part_name_appends_the_suffix() {
        assert_eq!(part_path(Path::new("/d/a.ogg")), PathBuf::from("/d/a.ogg.exp-part"));
    }
}
