//! EXP-1252 — moving a live session to another device: the handoff package.
//!
//! A person asks for a move (`sessions.send{sessionId, deviceId}`); the
//! server sets the synced `move_to_device_id` and the SOURCE host does the
//! rest ([`hand_off`], planned as [`crate::pickup::PickupAction::Handoff`]):
//!
//! 1. it stops delivering, resuming and reviving the session (messages wait
//!    on the server) and waits for a TRUE idle — no turn, no backgrounded
//!    subagents, no open question/plan card — at most [`IDLE_TIMEOUT`];
//! 2. it suspends the engine BEFORE packaging (`engine::RELEASED_OUTCOME`:
//!    the end sequence writes nothing to the row);
//! 3. it packs a tar.gz ([`build_package`]): [`MANIFEST`], the steer
//!    [`JOURNAL`], claude's [`NATIVE`] transcript and the workspace — a
//!    repo worktree as a `git bundle` of `HEAD` past `origin/<base>`
//!    ([`BUNDLE`]), `git diff --binary HEAD` ([`DIFF`]), the staged
//!    `git diff --cached` as a manifest field (`Manifest::index_diff`: a
//!    0.14.73 target, whose whitelist refuses any entry it does not know,
//!    skips the field and still restores the rest) and the untracked,
//!    non-ignored files ([`UNTRACKED`]); a repo-less scratch dir whole
//!    ([`SCRATCH`]). Over [`api::session_device::HANDOFF_MAX_BYTES`] the
//!    transcript goes first, then the journal; a workspace that alone is too
//!    big fails the move (work is never dropped);
//! 4. it uploads the package, then `sessions.handoff{moved}` re-stamps the
//!    row onto the target (`queued`). Any failure before that is
//!    `handoff{failed, reason}` and the session stays here untouched (its
//!    engine revives on the next message as usual);
//! 5. [`cleanup`] settles the local copy in two steps, off a pending-cleanup
//!    list in `{data_dir}/handoffs/` that survives restarts. Once the row
//!    shows the target CLAIMED it (`device_id ≠ me`, status past `queued`)
//!    only the run record goes (the entry is marked released). EXP-1349:
//!    the worktree, the local branch, the scratch dir and a transcript that
//!    travelled go only once the target RESTORED the package — the synced
//!    `sessions.handoff_restored_at` (`sessions.handoffDone`). The target
//!    claims before it restores and a failed restore falls back to a fresh
//!    launch, so until the marker the source's copy is the recovery point;
//!    a restore that never lands leaves it on disk.
//!
//! The TARGET's `launch_queued` sees `sessions.brief.handoff`, downloads the
//! package into `{data_dir}/handoffs/<id>/` ([`extract_package`]: whitelisted
//! names only), builds the run record ([`target_record`]) and resumes the
//! SAME session natively: the launcher restores the worktree
//! ([`restore_repo`] / [`restore_scratch`]) and imports the transcript into
//! the target profile's `projects/<munge(cwd)>/<id>.jsonl` with its `cwd`
//! fields rewritten ([`import_native`]). Codex, or any import failure, falls
//! back to a fresh conversation in the restored worktree with the handoff
//! note ([`moved_note`]). Credentials never move.

use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::agent::CodingAgent;
use crate::run_registry::{RunKind, RunRecord};

/// The package's entries — the ONLY names a target extracts.
pub const MANIFEST: &str = "manifest.json";
pub const JOURNAL: &str = "journal.jsonl";
pub const NATIVE: &str = "native.jsonl";
pub const BUNDLE: &str = "worktree.bundle";
pub const DIFF: &str = "worktree.diff";
pub const UNTRACKED: &str = "untracked.tar";
pub const SCRATCH: &str = "scratch.tar";
const ENTRIES: [&str; 7] = [MANIFEST, JOURNAL, NATIVE, BUNDLE, DIFF, UNTRACKED, SCRATCH];
/// EXP-1355: where [`restore_repo`] writes the staged patch it decodes from
/// `Manifest::index_diff` (inside the extracted package dir; never a tar
/// entry: a 0.14.73 target's whitelist would refuse the whole package).
const INDEX_DIFF_FILE: &str = "index.diff";

/// `{data_dir}/handoffs/` — downloaded packages and the pending-cleanup list.
pub const HANDOFFS_DIR: &str = "handoffs";
/// The source's pending cleanups (`{data_dir}/handoffs/moved-away.json`).
const MOVED_AWAY: &str = "moved-away.json";

pub const MANIFEST_VERSION: u32 = 1;

/// How long a move waits for the session's true idle before it fails.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Claude names a project dir by munging the cwd; past this many chars it
/// appends a hash we cannot reproduce — the import then falls back.
pub const MUNGE_MAX: usize = 200;

/// The most a package may unpack to (every entry together): a hostile or
/// corrupt package never fills the disk.
const MAX_UNPACKED: u64 = 4 * 1024 * 1024 * 1024;

/// The raw workspace a source reads into memory before compressing: past
/// this the move fails without trying.
const MAX_RAW_WORKSPACE: u64 = 1024 * 1024 * 1024;

/// What the package carries for the workspace.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceKind {
    /// A repo worktree: bundle + diff + untracked.
    Repo,
    /// A repo-less scratch dir, packed whole.
    Scratch,
    /// Nothing (a dispatcher's shared checkout, a session that never ran).
    #[default]
    None,
}

/// `manifest.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub version: u32,
    pub session_id: String,
    /// `claude` / `codex` (`CodingAgent::id`); empty = no run record.
    #[serde(default)]
    pub agent: String,
    /// The agent's own conversation id (claude's session uuid).
    #[serde(default)]
    pub native_session_id: Option<String>,
    /// The source's cwd as the source spells it (its OS's separators).
    #[serde(default)]
    pub source_cwd: String,
    /// `std::env::consts::OS` of the source.
    #[serde(default)]
    pub source_os: String,
    #[serde(default)]
    pub workspace: WorkspaceKind,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub base_branch: Option<String>,
    /// `origin/<base>` at the source when packed (the bundle's prerequisite).
    #[serde(default)]
    pub base_sha: Option<String>,
    /// The worktree's `HEAD` at the source.
    #[serde(default)]
    pub head_sha: Option<String>,
    /// [`BUNDLE`] is in the package (`HEAD` was ahead of the base).
    #[serde(default)]
    pub bundle: bool,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub effort: String,
    #[serde(default)]
    pub ultracode: bool,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub team_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub issues: Vec<String>,
    /// [`JOURNAL`] is in the package.
    #[serde(default)]
    pub journal: bool,
    /// [`NATIVE`] is in the package.
    #[serde(default)]
    pub native: bool,
    /// A dispatcher root's run: its workspace is the clone's shared,
    /// detached checkout, re-made on the target (nothing of it travels).
    #[serde(default)]
    pub dispatch: bool,
    /// EXP-1355: person messages the source's engine was handed but could
    /// not report consumed (`RunRecord::pending_consumes`): the target
    /// consumes them, never delivers them again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_consumes: Vec<String>,
    /// EXP-1355: the staged half, `git diff --cached --binary HEAD`,
    /// base64 ([`base64_encode`]), applied to the index only after [`DIFF`]
    /// restored the files. A manifest field rather than a tar entry so a
    /// target that predates it ignores it (the [`DIFF`] carries the staged
    /// content too: only the index split is lost there).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_diff: Option<String>,
}

/// Why a package was not made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageError {
    /// The workspace alone is over the cap: the move fails, nothing is lost.
    TooBig,
    Failed(String),
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooBig => f.write_str(REASON_TOO_BIG),
            Self::Failed(reason) => f.write_str(reason),
        }
    }
}

/// The reasons a source posts with `handoff{failed}` (the server shows them
/// in the conversation).
pub const REASON_TOO_BIG: &str =
    "Couldn't move the session: its worktree is larger than 64 MB. Commit and push, then start it on the other device.";
pub const REASON_NOT_IDLE: &str =
    "Couldn't move the session: it did not reach a quiet moment within 30 minutes.";

/// What [`build_package`] packs: the run record (if any) and the local
/// transcript files.
#[derive(Debug, Clone, Default)]
pub struct PackageSource {
    pub session_id: String,
    pub record: Option<RunRecord>,
    /// `{data_dir}/journal/<id>.jsonl`, when it exists.
    pub journal: Option<PathBuf>,
    /// Claude's transcript file, when found.
    pub native: Option<PathBuf>,
}

/// A built package and what it dropped to fit.
#[derive(Debug, Clone)]
pub struct Package {
    pub bytes: Vec<u8>,
    pub manifest: Manifest,
}

/// Build the tar.gz for `source` within `cap` bytes: everything, else
/// without the native transcript, else without the journal too; the
/// workspace alone over the cap is [`PackageError::TooBig`].
pub fn build_package(source: &PackageSource, cap: u64) -> Result<Package, PackageError> {
    let mut manifest = Manifest {
        version: MANIFEST_VERSION,
        session_id: source.session_id.clone(),
        source_os: std::env::consts::OS.to_string(),
        ..Manifest::default()
    };
    let mut parts: Vec<(&'static str, Vec<u8>)> = Vec::new();
    if let Some(record) = &source.record {
        manifest.agent = record.agent.id().to_string();
        manifest.native_session_id = record
            .claude_session_id
            .clone()
            .or_else(|| record.agent_native_session_id.clone())
            .filter(|id| plain_segment(id));
        manifest.source_cwd = record.cwd.to_string_lossy().into_owned();
        manifest.repo = record.repo.clone();
        manifest.repository_id = record.repository_id.clone();
        manifest.branch = record.branch.clone();
        manifest.base_branch = record.base_branch.clone();
        manifest.model = record.model.clone();
        manifest.effort = record.effort.clone();
        manifest.ultracode = record.ultracode;
        manifest.kind = record.kind.as_str().to_string();
        manifest.team_id = record.team_id.clone();
        manifest.title = record.title.clone();
        manifest.issues = record.issues.clone();
        manifest.dispatch = record.is_dispatch();
        manifest.pending_consumes = record.pending_consumes();
        if record.clone.is_some() && record.branch.is_some() && !record.is_dispatch() {
            if record.cwd.join(".git").exists() {
                manifest.workspace = WorkspaceKind::Repo;
                pack_repo(&record.cwd, record.base_branch.as_deref(), &mut manifest, &mut parts)?;
            }
        } else if record.clone.is_none() && record.cwd.is_dir() {
            manifest.workspace = WorkspaceKind::Scratch;
            let tar = tar_dir(&record.cwd)?;
            parts.push((SCRATCH, tar));
        }
    }
    let read = |path: &Option<PathBuf>| path.as_ref().and_then(|path| std::fs::read(path).ok());
    let journal = read(&source.journal);
    // Only claude's transcript travels (codex's lives in the login's rollout
    // store and resumes fresh on the target).
    let native = read(&source.native).filter(|_| manifest.agent == CodingAgent::Claude.id());
    let attempts = [(true, true), (false, true), (false, false)];
    for (with_native, with_journal) in attempts {
        let mut manifest = manifest.clone();
        manifest.native = with_native && native.is_some();
        manifest.journal = with_journal && journal.is_some();
        let mut entries: Vec<(&str, &[u8])> = parts.iter().map(|(name, bytes)| (*name, bytes.as_slice())).collect();
        if manifest.native {
            entries.push((NATIVE, native.as_deref().unwrap_or_default()));
        }
        if manifest.journal {
            entries.push((JOURNAL, journal.as_deref().unwrap_or_default()));
        }
        let manifest_json = serde_json::to_vec_pretty(&manifest)
            .map_err(|err| PackageError::Failed(format!("manifest: {err}")))?;
        entries.insert(0, (MANIFEST, &manifest_json));
        let bytes = gzip_tar(&entries)?;
        if (bytes.len() as u64) <= cap {
            return Ok(Package { bytes, manifest });
        }
    }
    Err(PackageError::TooBig)
}

/// The repo half of a package: `HEAD` (bundled past `origin/<base>` when
/// ahead of it), the uncommitted diff, its staged half and the untracked,
/// non-ignored files.
fn pack_repo(
    cwd: &Path,
    base_branch: Option<&str>,
    manifest: &mut Manifest,
    parts: &mut Vec<(&'static str, Vec<u8>)>,
) -> Result<(), PackageError> {
    let git = |args: &[&str]| -> Result<Vec<u8>, PackageError> {
        let output = crate::git_worktree::git_output(Some(cwd), args, None, "handoff git")
            .map_err(|err| PackageError::Failed(format!("git {}: {}", args.join(" "), err.detail)))?;
        if !output.status.success() {
            return Err(PackageError::Failed(format!(
                "git {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(output.stdout)
    };
    let text = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).trim().to_string();
    let head = text(git(&["rev-parse", "HEAD"])?);
    manifest.head_sha = Some(head.clone());
    let base_sha = base_branch
        .filter(|base| !base.is_empty() && !base.starts_with('-'))
        .and_then(|base| git(&["rev-parse", "--verify", "--quiet", &format!("origin/{base}^{{commit}}")]).ok())
        .map(text)
        .filter(|sha| !sha.is_empty());
    manifest.base_sha = base_sha.clone();
    let ahead: u64 = match &base_sha {
        Some(base) => text(git(&["rev-list", "--count", "HEAD", &format!("^{base}")])?).parse().unwrap_or(0),
        None => 1,
    };
    if ahead > 0 {
        let tmp = temp_path("bundle");
        let tmp_str = tmp.to_string_lossy().into_owned();
        let mut args = vec!["bundle", "create", tmp_str.as_str(), "HEAD"];
        let exclude = base_sha.as_ref().map(|base| format!("^{base}"));
        if let Some(exclude) = &exclude {
            args.push(exclude);
        }
        let made = git(&args);
        let bundle = made
            .and_then(|_| std::fs::read(&tmp).map_err(|err| PackageError::Failed(format!("read the bundle: {err}"))));
        let _ = std::fs::remove_file(&tmp);
        parts.push((BUNDLE, bundle?));
        manifest.bundle = true;
    }
    let diff = git(&["diff", "--binary", "HEAD"])?;
    if !diff.is_empty() {
        parts.push((DIFF, diff));
    }
    let staged = git(&["diff", "--cached", "--binary", "HEAD"])?;
    if !staged.is_empty() {
        manifest.index_diff = Some(base64_encode(&staged));
    }
    let listed = git(&["ls-files", "--others", "--exclude-standard", "-z"])?;
    let files: Vec<String> = listed
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).into_owned())
        .collect();
    if !files.is_empty() {
        parts.push((UNTRACKED, tar_files(cwd, &files)?));
    }
    Ok(())
}

fn temp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "exp-handoff-{tag}-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ))
}

/// An uncompressed tar of `files` (paths relative to `root`), symlinks kept
/// as links.
fn tar_files(root: &Path, files: &[String]) -> Result<Vec<u8>, PackageError> {
    let mut builder = tar::Builder::new(Vec::new());
    builder.follow_symlinks(false);
    let mut total: u64 = 0;
    for file in files {
        let path = root.join(file);
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        total += meta.len();
        if total > MAX_RAW_WORKSPACE {
            return Err(PackageError::TooBig);
        }
        builder
            .append_path_with_name(&path, file)
            .map_err(|err| PackageError::Failed(format!("pack {file}: {err}")))?;
    }
    builder.into_inner().map_err(|err| PackageError::Failed(format!("pack: {err}")))
}

/// An uncompressed tar of everything under `root` (relative names).
fn tar_dir(root: &Path) -> Result<Vec<u8>, PackageError> {
    let mut files = Vec::new();
    collect_files(root, Path::new(""), &mut files)?;
    tar_files(root, &files)
}

fn collect_files(root: &Path, rel: &Path, out: &mut Vec<String>) -> Result<(), PackageError> {
    let dir = root.join(rel);
    let entries = std::fs::read_dir(&dir)
        .map_err(|err| PackageError::Failed(format!("read {}: {err}", dir.display())))?;
    let mut names: Vec<_> = entries.flatten().collect();
    names.sort_by_key(|entry| entry.file_name());
    for entry in names {
        let rel = rel.join(entry.file_name());
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            collect_files(root, &rel, out)?;
        } else {
            // Tar names are `/`-separated whatever the OS.
            let name: Vec<String> = rel
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.push(name.join("/"));
        }
    }
    Ok(())
}

fn gzip_tar(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, PackageError> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (name, bytes) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        builder
            .append_data(&mut header, name, *bytes)
            .map_err(|err| PackageError::Failed(format!("pack {name}: {err}")))?;
    }
    let encoder = builder.into_inner().map_err(|err| PackageError::Failed(format!("pack: {err}")))?;
    encoder.finish().map_err(|err| PackageError::Failed(format!("compress: {err}")))
}

/// Unpack a downloaded package into `dir` (created): ONLY the whitelisted
/// regular-file entries at the top level, every other entry refused; the
/// manifest is required. Returns it.
pub fn extract_package(bytes: &[u8], dir: &Path) -> Result<Manifest, String> {
    std::fs::create_dir_all(dir).map_err(|err| format!("create {}: {err}", dir.display()))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut total: u64 = 0;
    let mut seen = Vec::new();
    for entry in archive.entries().map_err(|err| format!("read the package: {err}"))? {
        let mut entry = entry.map_err(|err| format!("read the package: {err}"))?;
        let path = entry.path().map_err(|err| format!("package entry: {err}"))?.into_owned();
        let name = whitelisted(&path).ok_or_else(|| format!("unexpected package entry {}", path.display()))?;
        if entry.header().entry_type() != tar::EntryType::Regular {
            return Err(format!("package entry {name} is not a file"));
        }
        if seen.contains(&name) {
            return Err(format!("package entry {name} appears twice"));
        }
        seen.push(name);
        total = total.saturating_add(entry.size());
        if total > MAX_UNPACKED {
            return Err("the package unpacks too large".to_string());
        }
        let mut file = std::fs::File::create(dir.join(name)).map_err(|err| format!("write {name}: {err}"))?;
        std::io::copy(&mut (&mut entry).take(MAX_UNPACKED), &mut file).map_err(|err| format!("write {name}: {err}"))?;
    }
    let manifest = std::fs::read(dir.join(MANIFEST)).map_err(|_| "the package has no manifest".to_string())?;
    let manifest: Manifest =
        serde_json::from_slice(&manifest).map_err(|err| format!("the package manifest: {err}"))?;
    if manifest.version != MANIFEST_VERSION {
        return Err(format!("unknown package version {}", manifest.version));
    }
    Ok(manifest)
}

/// `path` is exactly one of [`ENTRIES`] (a single plain component).
fn whitelisted(path: &Path) -> Option<&'static str> {
    let mut components = path.components();
    let first = components.next()?;
    if components.next().is_some() {
        return None;
    }
    let Component::Normal(name) = first else { return None };
    ENTRIES.iter().copied().find(|entry| name == std::ffi::OsStr::new(entry))
}

/// Unpack an inner workspace tar ([`UNTRACKED`], [`SCRATCH`]) into `dst`:
/// relative paths only (no `..`, no root), files, dirs and symlinks whose
/// target stays relative and inside; anything else is skipped. `unpack_in`
/// re-checks that nothing lands outside `dst`.
pub fn unpack_safely(tar_bytes: &[u8], dst: &Path) -> Result<usize, String> {
    std::fs::create_dir_all(dst).map_err(|err| format!("create {}: {err}", dst.display()))?;
    let mut archive = tar::Archive::new(tar_bytes);
    archive.set_overwrite(true);
    let mut count = 0;
    for entry in archive.entries().map_err(|err| format!("read the workspace: {err}"))? {
        let mut entry = entry.map_err(|err| format!("read the workspace: {err}"))?;
        let path = entry.path().map_err(|err| format!("workspace entry: {err}"))?.into_owned();
        if !relative_inside(&path) {
            return Err(format!("refusing workspace entry {}", path.display()));
        }
        match entry.header().entry_type() {
            tar::EntryType::Regular | tar::EntryType::Directory => {}
            tar::EntryType::Symlink => {
                let target = entry
                    .link_name()
                    .map_err(|err| format!("workspace link: {err}"))?
                    .map(|target| target.into_owned());
                if !target.as_deref().is_some_and(relative_inside) {
                    log::warn!("handoff: skipped the link {} (it points outside)", path.display());
                    continue;
                }
            }
            _ => continue,
        }
        let unpacked = entry
            .unpack_in(dst)
            .map_err(|err| format!("unpack {}: {err}", path.display()))?;
        if !unpacked {
            return Err(format!("refusing workspace entry {}", path.display()));
        }
        count += 1;
    }
    Ok(count)
}

fn relative_inside(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.components().all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

/// A plain id usable as one path segment (uuids in practice).
fn plain_segment(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 (RFC 4648, padded) of `bytes`: how a binary patch rides
/// in the JSON manifest (`Manifest::index_diff`).
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        out.push(BASE64[(n >> 18) as usize & 63] as char);
        out.push(BASE64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { BASE64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { BASE64[n as usize & 63] as char } else { '=' });
    }
    out
}

/// The inverse of [`base64_encode`] (whitespace skipped, padding required).
pub fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    fn digit(byte: u8) -> Option<u32> {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        Some(u32::from(value))
    }
    let digits: Vec<u8> = text.bytes().filter(|byte| !byte.is_ascii_whitespace()).collect();
    if digits.len() % 4 != 0 {
        return Err("base64 of a length not a multiple of 4".to_string());
    }
    let mut out = Vec::with_capacity(digits.len() / 4 * 3);
    for quad in digits.chunks(4) {
        let pad = quad.iter().rev().take_while(|byte| **byte == b'=').count();
        if pad > 2 {
            return Err("base64 padding".to_string());
        }
        let mut n = 0u32;
        for byte in &quad[..4 - pad] {
            n = (n << 6) | digit(*byte).ok_or_else(|| format!("base64 digit {:?}", *byte as char))?;
        }
        n <<= 6 * pad as u32;
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

// ── The target side ───────────────────────────────────────────────────────

/// What the launcher restores on a resume that came with a package.
#[derive(Clone, Debug)]
pub struct HandoffImport {
    /// `{data_dir}/handoffs/<id>/` (the extracted package).
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// The source device's name, for the note.
    pub from_device: String,
}

/// `{data_dir}/handoffs/<session id>/`.
pub fn package_dir(data_dir: &Path, session_id: &str) -> Option<PathBuf> {
    plain_segment(session_id).then(|| data_dir.join(HANDOFFS_DIR).join(session_id))
}

/// Restore a repo worktree at `cwd` (already created on the branch by the
/// launcher, from `origin/<base>`): fetch the bundle's `HEAD` (after a
/// best-effort fetch of the base, its prerequisite) and move the branch to
/// it, apply the uncommitted diff, stage its staged half (EXP-1355: the
/// index as the source left it), unpack the untracked files. With no
/// bundle the branch moves to the recorded `HEAD` when this clone has it.
pub fn restore_repo(cwd: &Path, import: &HandoffImport) -> Result<(), String> {
    let manifest = &import.manifest;
    let git = |args: &[&str]| -> Result<(), String> {
        crate::git_worktree::run_git(Some(cwd), args, None, "handoff restore")
            .map(|_| ())
            .map_err(|err| format!("git {}: {}", args.join(" "), err.detail))
    };
    let has_commit = |sha: &str| git(&["cat-file", "-e", &format!("{sha}^{{commit}}")]).is_ok();
    if let Some(base) = manifest.base_branch.as_deref().filter(|base| !base.is_empty() && !base.starts_with('-')) {
        if !manifest.base_sha.as_deref().is_some_and(has_commit) {
            // The repo-local credential helper is installed: plain origin.
            if let Err(err) = git(&["fetch", "origin", &format!("refs/heads/{base}:refs/remotes/origin/{base}")]) {
                log::warn!("handoff: base {base} not fetched: {err}");
            }
        }
    }
    let bundle = import.dir.join(BUNDLE);
    if manifest.bundle {
        let bundle = bundle.to_string_lossy().into_owned();
        git(&["fetch", "--no-tags", &bundle, "HEAD"])?;
        git(&["reset", "--hard", "FETCH_HEAD"])?;
    } else if let Some(head) = manifest.head_sha.as_deref().filter(|head| has_commit(head)) {
        git(&["reset", "--hard", head])?;
    }
    let diff = import.dir.join(DIFF);
    if diff.is_file() {
        let diff = diff.to_string_lossy().into_owned();
        git(&["apply", "--binary", "--whitespace=nowarn", &diff])?;
    }
    if let Some(encoded) = manifest.index_diff.as_deref() {
        let patch = base64_decode(encoded).map_err(|err| format!("the staged patch: {err}"))?;
        let staged = import.dir.join(INDEX_DIFF_FILE);
        std::fs::write(&staged, patch).map_err(|err| format!("write {INDEX_DIFF_FILE}: {err}"))?;
        let staged = staged.to_string_lossy().into_owned();
        git(&["apply", "--cached", "--binary", "--whitespace=nowarn", &staged])?;
    }
    let untracked = import.dir.join(UNTRACKED);
    if untracked.is_file() {
        let bytes = std::fs::read(&untracked).map_err(|err| format!("read {UNTRACKED}: {err}"))?;
        unpack_safely(&bytes, cwd)?;
    }
    Ok(())
}

/// Restore a repo-less scratch dir at `cwd` (created) from [`SCRATCH`].
pub fn restore_scratch(cwd: &Path, import: &HandoffImport) -> Result<(), String> {
    let scratch = import.dir.join(SCRATCH);
    if !scratch.is_file() {
        return Ok(());
    }
    let bytes = std::fs::read(&scratch).map_err(|err| format!("read {SCRATCH}: {err}"))?;
    unpack_safely(&bytes, cwd).map(|_| ())
}

/// Restore the steer journal (`{data_dir}/journal/<id>.jsonl`) — the
/// session view's history here. Replaces a stale one (a move back).
pub fn restore_journal(data_dir: &Path, import: &HandoffImport) -> Result<(), String> {
    let source = import.dir.join(JOURNAL);
    if !import.manifest.journal || !source.is_file() {
        return Ok(());
    }
    let Some(target) = journal_path(data_dir, &import.manifest.session_id) else {
        return Err("the session id is not a plain path segment".to_string());
    };
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("create {}: {err}", parent.display()))?;
    }
    std::fs::copy(&source, &target).map(|_| ()).map_err(|err| format!("restore the journal: {err}"))
}

/// `{data_dir}/journal/<id>.jsonl` (`steer::history::journal_path`'s rule —
/// this crate cannot depend on steer).
pub fn journal_path(data_dir: &Path, session_id: &str) -> Option<PathBuf> {
    plain_segment(session_id).then(|| data_dir.join("journal").join(format!("{session_id}.jsonl")))
}

/// Claude's project-dir name for `cwd`: every char that is not an ASCII
/// letter or digit becomes `-` (one per UTF-16 unit, as the CLI's JS
/// replace does).
pub fn munge(cwd: &str) -> String {
    let mut out = String::with_capacity(cwd.len());
    for c in cwd.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else {
            for _ in 0..c.len_utf16() {
                out.push('-');
            }
        }
    }
    out
}

/// Import the package's claude transcript into `projects_root` (the target
/// profile's `projects/`) as `<munge(cwd)>/<native id>.jsonl`, every `cwd`
/// field under the source cwd rewritten onto `cwd`. Refused when the munged
/// name would pass [`MUNGE_MAX`] (claude hash-suffixes those).
pub fn import_native(projects_root: &Path, cwd: &Path, import: &HandoffImport) -> Result<PathBuf, String> {
    let manifest = &import.manifest;
    let id = manifest
        .native_session_id
        .as_deref()
        .filter(|id| plain_segment(id))
        .ok_or_else(|| "no native session id".to_string())?;
    if !manifest.native {
        return Err("the package carries no transcript".to_string());
    }
    let target_cwd = cwd.to_string_lossy().into_owned();
    let project = munge(&target_cwd);
    if project.len() > MUNGE_MAX {
        return Err(format!("the worktree path is too long for claude's project dir ({} chars)", project.len()));
    }
    let source = std::fs::read(import.dir.join(NATIVE)).map_err(|err| format!("read {NATIVE}: {err}"))?;
    let rewritten = rewrite_transcript(&source, &manifest.source_cwd, &target_cwd, std::path::MAIN_SEPARATOR);
    let dir = projects_root.join(project);
    std::fs::create_dir_all(&dir).map_err(|err| format!("create {}: {err}", dir.display()))?;
    let path = dir.join(format!("{id}.jsonl"));
    let mut file = std::fs::File::create(&path).map_err(|err| format!("write {}: {err}", path.display()))?;
    file.write_all(&rewritten).map_err(|err| format!("write {}: {err}", path.display()))?;
    Ok(path)
}

/// Rewrite every JSONL line's `cwd` fields (at any depth) from under
/// `source_cwd` onto `target_cwd` with `target_sep`. Lines that are not JSON
/// objects pass through byte for byte.
pub fn rewrite_transcript(bytes: &[u8], source_cwd: &str, target_cwd: &str, target_sep: char) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for (index, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
        if index > 0 {
            out.push(b'\n');
        }
        let rewritten = serde_json::from_slice::<serde_json::Value>(line).ok().and_then(|mut value| {
            rewrite_cwd_fields(&mut value, source_cwd, target_cwd, target_sep)
                .then(|| serde_json::to_vec(&value).ok())
                .flatten()
        });
        match rewritten {
            Some(line) => out.extend_from_slice(&line),
            None => out.extend_from_slice(line),
        }
    }
    out
}

fn rewrite_cwd_fields(value: &mut serde_json::Value, source: &str, target: &str, sep: char) -> bool {
    let mut changed = false;
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if key == "cwd" {
                    if let Some(moved) = value.as_str().and_then(|path| rewrite_path(path, source, target, sep)) {
                        *value = serde_json::Value::String(moved);
                        changed = true;
                        continue;
                    }
                }
                changed |= rewrite_cwd_fields(value, source, target, sep);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                changed |= rewrite_cwd_fields(item, source, target, sep);
            }
        }
        _ => {}
    }
    changed
}

/// `path` under `source` (the source's spelling: `\` counts as a separator
/// and the compare ignores case when the source is a Windows path) mapped
/// onto `target` with `sep`; `None` = not under it.
pub fn rewrite_path(path: &str, source: &str, target: &str, sep: char) -> Option<String> {
    if source.is_empty() {
        return None;
    }
    let windows = is_windows_path(source);
    let split = |text: &str| -> Vec<String> {
        text.split(|c| c == '/' || (windows && c == '\\'))
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect()
    };
    let source_parts = split(source);
    let parts = split(path);
    if parts.len() < source_parts.len() {
        return None;
    }
    let same = |a: &str, b: &str| {
        if windows {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };
    if !source_parts.iter().zip(&parts).all(|(a, b)| same(a, b)) {
        return None;
    }
    // A POSIX source keeps its leading `/`; a bare relative never matches it.
    if !windows && source.starts_with('/') != path.starts_with('/') {
        return None;
    }
    let mut out = target.trim_end_matches(['/', '\\']).to_string();
    if out.is_empty() {
        out.push(sep);
    }
    for part in &parts[source_parts.len()..] {
        if !out.ends_with(sep) {
            out.push(sep);
        }
        out.push_str(part);
    }
    Some(out)
}

/// `C:\…`, `C:/…` or a `\\server\share` path.
fn is_windows_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':') || path.starts_with("\\\\")
}

/// The run record a TARGET resumes a moved session from: the row's pickers,
/// the package's workspace placed where THIS machine keeps it (a repo's
/// worktree at its layout path under this repos root, a scratch dir under
/// this data dir), claude's conversation id when its transcript came along.
pub fn target_record(
    row: &domain::rows::SessionRow,
    manifest: &Manifest,
    data_dir: &Path,
    repos_root: &Path,
    account_id: &str,
) -> Option<RunRecord> {
    let agent = CodingAgent::parse(&manifest.agent)
        .or_else(|| row.cli.as_deref().and_then(CodingAgent::parse))?;
    let picked = |value: &Option<String>| value.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
    let dispatch_clone = manifest
        .repo
        .as_deref()
        .filter(|_| manifest.dispatch)
        .map(|repo| crate::git_worktree::clone_path(repos_root, repo));
    let (cwd, clone) = match (manifest.workspace, dispatch_clone) {
        (_, Some(clone)) => {
            (crate::git_worktree::worktrees_dir(&clone).join(crate::git_worktree::DISPATCH_CHECKOUT_DIR), Some(clone))
        }
        (WorkspaceKind::Repo, None) => {
            let repo = manifest.repo.as_deref()?;
            let branch = manifest.branch.as_deref()?;
            let clone = crate::git_worktree::clone_path(repos_root, repo);
            (crate::git_worktree::worktree_path(&clone, branch), Some(clone))
        }
        _ => {
            let root = row.root_id_or_self();
            if !plain_segment(root) || !plain_segment(&row.id) {
                return None;
            }
            (crate::scratch::scratch_root(data_dir).join(root).join(&row.id), None)
        }
    };
    let native = manifest
        .native_session_id
        .clone()
        .filter(|_| manifest.native && agent == CodingAgent::Claude);
    let mut record = RunRecord {
        session_id: row.id.clone(),
        account_id: account_id.to_string(),
        agent,
        kind: RunKind::of(row),
        team_id: row.team_id.clone().unwrap_or_else(|| manifest.team_id.clone()),
        issues: manifest.issues.clone(),
        title: row.title.clone().or_else(|| manifest.title.clone()),
        repo: manifest.repo.clone().filter(|_| clone.is_some()),
        repository_id: manifest.repository_id.clone().or_else(|| row.repository_id.clone()),
        branch: manifest.branch.clone().filter(|_| manifest.workspace == WorkspaceKind::Repo),
        base_branch: manifest.base_branch.clone(),
        claude_session_id: native.clone(),
        codex_originator: None,
        model: picked(&row.model).unwrap_or_else(|| manifest.model.clone()),
        effort: picked(&row.effort).unwrap_or_else(|| manifest.effort.clone()),
        ultracode: manifest.ultracode,
        transport: Some(crate::launcher::ACP_TRANSPORT.to_string()),
        acp_session_id: None,
        agent_native_session_id: native,
        acp_child_pid: None,
        host_pid: None,
        recorded_at: crate::run_registry::now_secs(),
        extra: Default::default(),
        cwd,
        clone,
    };
    if manifest.dispatch {
        record
            .extra
            .insert(crate::run_registry::DISPATCH_KEY.to_string(), serde_json::Value::Bool(true));
    }
    record.set_pending_consumes(&manifest.pending_consumes);
    record.set_account(row.account.as_deref());
    record.set_honored_row_account(row.account.as_deref());
    Some(record)
}

/// The line a moved session's agent reads (system append; on a fresh
/// conversation also after the brief).
pub fn moved_note(from_device: &str, native: bool) -> String {
    let from = from_device.trim();
    let from = if from.is_empty() { "another device" } else { from };
    let conversation = if native {
        "your conversation came with it"
    } else {
        "your earlier conversation could not come along, so pick the work up from the worktree and the session's messages"
    };
    format!(
        "This session moved here from {from}. Its worktree (commits, staged, unstaged and untracked files) was restored on this machine and {conversation}. Paths in earlier messages may differ: work in the current directory."
    )
}

// ── The source side ───────────────────────────────────────────────────────

/// One session handed off from here whose local copy waits for [`cleanup`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovedAway {
    pub session_id: String,
    pub cwd: PathBuf,
    #[serde(default)]
    pub clone: Option<PathBuf>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub native: Option<PathBuf>,
    /// EXP-1349: the transcript at `native` travelled in the package
    /// (`manifest.native`), so the target holds a copy. Absent (an entry
    /// written before the field) = unknown, and the transcript stays.
    #[serde(default)]
    pub native_packaged: bool,
    /// EXP-1349: the target claimed the row and the run record here is gone;
    /// the workspace waits for [`MovedAway::restored`].
    #[serde(default)]
    pub released: bool,
    /// EXP-1349: the row carried `handoff_restored_at` off this device: the
    /// target holds the work, the workspace here may go.
    #[serde(default)]
    pub restored: bool,
    /// The device THIS move went to (`move_to_device_id` when packed). A
    /// restore marker counts only while the row sits on it: a later move
    /// elsewhere (a target whose restore failed moving on fresh) restores
    /// THAT package, not ours, and the copy here stays. Absent (an entry
    /// written before the field) = any marker counts.
    #[serde(default)]
    pub target: Option<String>,
}

fn moved_away_path(data_dir: &Path) -> PathBuf {
    data_dir.join(HANDOFFS_DIR).join(MOVED_AWAY)
}

/// The pending cleanups.
pub fn moved_away(data_dir: &Path) -> Vec<MovedAway> {
    std::fs::read(moved_away_path(data_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_moved_away(data_dir: &Path, entries: &[MovedAway]) {
    let path = moved_away_path(data_dir);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(json) = serde_json::to_string_pretty(entries) else { return };
    if let Err(err) = crate::atomic_config::write_atomic(&path, &json) {
        log::warn!("handoff: the pending cleanups were not saved: {err}");
    }
}

/// Remember `entry` for [`cleanup`] (replacing an older one for the session).
pub fn note_moved_away(data_dir: &Path, entry: MovedAway) {
    let mut entries = moved_away(data_dir);
    entries.retain(|other| other.session_id != entry.session_id);
    entries.push(entry);
    write_moved_away(data_dir, &entries);
}

fn forget_moved_away(data_dir: &Path, session_id: &str) {
    let mut entries = moved_away(data_dir);
    let before = entries.len();
    entries.retain(|other| other.session_id != session_id);
    if entries.len() != before {
        write_moved_away(data_dir, &entries);
    }
}

/// Patch the pending entry of `session_id` (none = nothing).
fn update_moved_away(data_dir: &Path, session_id: &str, patch: impl FnOnce(&mut MovedAway)) {
    let mut entries = moved_away(data_dir);
    let Some(entry) = entries.iter_mut().find(|entry| entry.session_id == session_id) else { return };
    patch(entry);
    write_moved_away(data_dir, &entries);
}

/// The sessions whose local copy has a [`cleanup`] step due now, for rows
/// handed off from here that name another device: the target RESTORED it
/// (`handoff_restored_at` while the row is on [`MovedAway::target`],
/// recorded on the entry — the workspace goes), or it CLAIMED it (status
/// past `queued`, not yet released — the run record goes). A row back on
/// THIS device (moved back before the target took it) drops the pending
/// cleanup — the copy is the live one again.
pub fn cleanups_due(data_dir: &Path, device_id: &str, sessions: &[domain::rows::SessionRow]) -> Vec<String> {
    let entries = moved_away(data_dir);
    if entries.is_empty() {
        return Vec::new();
    }
    let mut due = Vec::new();
    for entry in &entries {
        let Some(row) = sessions.iter().find(|row| row.id == entry.session_id) else { continue };
        if row.on_device(device_id) {
            forget_moved_away(data_dir, &entry.session_id);
            continue;
        }
        // The marker is ours only on our target: set once the row moved on
        // (B → C restored B's package) it says nothing about the work here.
        let restored_by_target = row.handoff_restored()
            && entry.target.as_deref().is_none_or(|target| row.device_id.as_deref() == Some(target));
        if entry.restored || restored_by_target {
            if !entry.restored {
                update_moved_away(data_dir, &entry.session_id, |entry| entry.restored = true);
            }
            due.push(entry.session_id.clone());
        } else if !entry.released && !row.status_is(domain::contract::SESSION_STATUS_QUEUED) {
            due.push(entry.session_id.clone());
        }
    }
    due
}

/// Settle a handed-off session's local copy, one step per [`cleanups_due`]:
///
/// - claimed, not yet restored: the run record goes (nothing here resumes or
///   revives it) and the entry is marked released. The worktree, the local
///   branch, the scratch dir and the transcript STAY — EXP-1349: the target
///   claims the row before it restores and a failed restore falls back to a
///   fresh launch, so the claim is no proof the moved work (commits past
///   `origin/<base>`, the diff, untracked files) landed anywhere;
/// - restored ([`MovedAway::restored`]): the worktree and its local branch
///   (unless another run record here shares the worktree) or the scratch
///   dir, the transcript when it travelled, the run record and the entry.
pub fn cleanup(data_dir: &Path, session_id: &str) {
    let Some(entry) = moved_away(data_dir).into_iter().find(|entry| entry.session_id == session_id) else {
        return;
    };
    crate::run_registry::remove(data_dir, session_id);
    if !entry.restored {
        update_moved_away(data_dir, session_id, |entry| entry.released = true);
        log::info!("handoff: session {session_id} was claimed elsewhere; its workspace stays until the target restores it");
        return;
    }
    let shared = crate::run_registry::all(data_dir)
        .iter()
        .any(|record| record.session_id != session_id && record.cwd == entry.cwd);
    match (&entry.clone, shared) {
        (_, true) => log::info!("handoff: {} stays (another session here uses it)", entry.cwd.display()),
        (Some(clone), false) if entry.branch.is_some() => {
            let cwd = entry.cwd.to_string_lossy().into_owned();
            if entry.cwd.exists() {
                if let Err(err) =
                    crate::git_worktree::run_git(Some(clone), &["worktree", "remove", "--force", &cwd], None, "handoff cleanup")
                {
                    log::warn!("handoff: worktree {cwd} not removed: {}", err.detail);
                }
            }
            let _ = crate::git_worktree::run_git(Some(clone), &["worktree", "prune"], None, "handoff cleanup");
            if let Some(branch) = entry.branch.as_deref().filter(|branch| !branch.starts_with('-')) {
                if let Err(err) = crate::git_worktree::run_git(Some(clone), &["branch", "-D", branch], None, "handoff cleanup") {
                    log::info!("handoff: branch {branch} kept: {}", err.detail);
                }
            }
        }
        (Some(_), false) => {}
        (None, false) => {
            if crate::scratch::is_scratch_dir(data_dir, &entry.cwd) {
                if let Err(err) = std::fs::remove_dir_all(&entry.cwd) {
                    log::warn!("handoff: scratch dir {} not removed: {err}", entry.cwd.display());
                }
            }
        }
    }
    match &entry.native {
        Some(native) if entry.native_packaged => {
            let _ = std::fs::remove_file(native);
        }
        Some(native) => log::info!("handoff: transcript {} stays (it did not travel)", native.display()),
        None => {}
    }
    forget_moved_away(data_dir, session_id);
    log::info!("handoff: session {session_id} was restored elsewhere; its local copy is gone");
}

/// Read a package source for `session_id` off this machine.
pub fn package_source(deps: &crate::CodingDeps, session_id: &str) -> PackageSource {
    let record = crate::run_registry::get(&deps.data_dir, session_id);
    let native = record.as_ref().and_then(|record| {
        let id = record.claude_session_id.clone().or_else(|| record.agent_native_session_id.clone())?;
        let profile = crate::agent_profiles::account_dir(&deps.data_dir, Some(record.agent), record.account().as_deref());
        crate::launcher::claude_projects_root_in(deps, profile.as_deref())
            .and_then(|root| crate::launcher::locate_claude_transcript(&root, &id))
    });
    PackageSource {
        session_id: session_id.to_string(),
        record,
        journal: journal_path(&deps.data_dir, session_id).filter(|path| path.is_file()),
        native,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("exp1252-{tag}-{}-{}", std::process::id(), uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git(cwd: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// An origin with `main`, a source clone + worktree on `exp/s1` and a
    /// target clone of the same origin.
    struct Repos {
        root: PathBuf,
        source_wt: PathBuf,
        target_clone: PathBuf,
    }

    fn repos() -> Repos {
        let root = temp("repo");
        let origin = root.join("origin.git");
        git(&root, &["init", "--bare", "-b", "main", origin.to_str().unwrap()]);
        let seed = root.join("seed");
        git(&root, &["clone", origin.to_str().unwrap(), seed.to_str().unwrap()]);
        std::fs::write(seed.join("a.txt"), "one\n").unwrap();
        std::fs::write(seed.join(".gitignore"), "ignored.log\n").unwrap();
        git(&seed, &["add", "."]);
        git(&seed, &["commit", "-m", "base"]);
        git(&seed, &["push", "origin", "HEAD:main"]);
        let source = root.join("source");
        git(&root, &["clone", origin.to_str().unwrap(), source.to_str().unwrap()]);
        let source_wt = root.join("source.worktrees").join("exp-s1");
        git(&source, &["worktree", "add", "-b", "exp/s1", source_wt.to_str().unwrap(), "origin/main"]);
        let target_clone = root.join("target");
        git(&root, &["clone", origin.to_str().unwrap(), target_clone.to_str().unwrap()]);
        Repos { root, source_wt, target_clone }
    }

    fn record(cwd: &Path, clone: Option<PathBuf>, branch: Option<&str>) -> RunRecord {
        RunRecord {
            session_id: "s1".to_string(),
            account_id: "acct".to_string(),
            agent: CodingAgent::Claude,
            kind: RunKind::Thread,
            team_id: "team".to_string(),
            issues: Vec::new(),
            title: None,
            cwd: cwd.to_path_buf(),
            clone,
            repo: Some("acme/web".to_string()),
            repository_id: Some("repo-1".to_string()),
            branch: branch.map(str::to_string),
            base_branch: Some("main".to_string()),
            claude_session_id: Some("native-1".to_string()),
            codex_originator: None,
            model: "opus".to_string(),
            effort: String::new(),
            ultracode: false,
            transport: None,
            acp_session_id: None,
            agent_native_session_id: None,
            acp_child_pid: None,
            host_pid: None,
            recorded_at: 0,
            extra: Default::default(),
        }
    }

    /// The target half for a test: a fresh worktree of the target clone on
    /// the branch, cut from origin/main, then [`restore_repo`].
    fn restore_on_target(repos: &Repos, package: &Package) -> PathBuf {
        let dir = repos.root.join("pkg");
        let manifest = extract_package(&package.bytes, &dir).unwrap();
        let wt = repos.root.join("target.worktrees").join("exp-s1");
        git(&repos.target_clone, &["worktree", "add", "-b", "exp/s1", wt.to_str().unwrap(), "origin/main"]);
        let import = HandoffImport { dir, manifest, from_device: "Laptop".to_string() };
        restore_repo(&wt, &import).unwrap();
        wt
    }

    #[test]
    fn a_repo_worktree_round_trips_exactly() {
        let repos = repos();
        let wt = &repos.source_wt;
        // An unpushed commit, a staged change, an unstaged change, a binary
        // file, an untracked file in a new dir, and an ignored one.
        std::fs::write(wt.join("b.txt"), "committed\n").unwrap();
        git(wt, &["add", "b.txt"]);
        git(wt, &["commit", "-m", "unpushed"]);
        std::fs::write(wt.join("a.txt"), "one\nstaged\n").unwrap();
        git(wt, &["add", "a.txt"]);
        // EXP-1355: staged AND unstaged on one file.
        std::fs::write(wt.join("a.txt"), "one\nstaged\nunstaged\n").unwrap();
        std::fs::write(wt.join("b.txt"), "committed\nunstaged\n").unwrap();
        std::fs::write(wt.join("bin.dat"), [0u8, 159, 146, 150, 255]).unwrap();
        git(wt, &["add", "bin.dat"]);
        std::fs::create_dir_all(wt.join("new/dir")).unwrap();
        std::fs::write(wt.join("new/dir/c.txt"), "untracked\n").unwrap();
        std::fs::write(wt.join("ignored.log"), "never\n").unwrap();
        let mut owed = record(wt, Some(repos.root.join("source")), Some("exp/s1"));
        owed.set_pending_consumes(&["m1".to_string()]);
        let source = PackageSource { session_id: "s1".to_string(), record: Some(owed), ..PackageSource::default() };
        let package = build_package(&source, 64 * 1024 * 1024).unwrap();
        assert_eq!(package.manifest.workspace, WorkspaceKind::Repo);
        assert!(package.manifest.bundle);
        assert_eq!(package.manifest.pending_consumes, vec!["m1".to_string()]);
        assert_eq!(package.manifest.native_session_id.as_deref(), Some("native-1"));
        // EXP-1355: the staged half rides in the MANIFEST (a 0.14.73 target
        // refuses any tar entry it does not know; it skips a field).
        let staged = base64_decode(package.manifest.index_diff.as_deref().expect("staged patch")).unwrap();
        assert_eq!(String::from_utf8_lossy(&staged).trim_end(), git(wt, &["diff", "--cached", "--binary", "HEAD"]));
        let names = entry_names(&package.bytes);
        assert!(!names.iter().any(|name| name == "index.diff"), "{names:?}");
        assert!(names.iter().all(|name| ENTRIES.contains(&name.as_str())), "{names:?}");
        let json: serde_json::Value = serde_json::from_slice(&manifest_of(&package.bytes)).unwrap();
        assert!(json.get("indexDiff").is_some_and(serde_json::Value::is_string));

        let target = restore_on_target(&repos, &package);
        assert_eq!(git(&target, &["rev-parse", "HEAD"]), git(wt, &["rev-parse", "HEAD"]));
        assert_eq!(std::fs::read_to_string(target.join("a.txt")).unwrap(), "one\nstaged\nunstaged\n");
        // EXP-1355: the index comes back as the source left it.
        assert_eq!(git(&target, &["status", "--porcelain"]), git(wt, &["status", "--porcelain"]));
        assert_eq!(git(&target, &["diff", "--cached", "--binary"]), git(wt, &["diff", "--cached", "--binary"]));
        assert_eq!(git(&target, &["show", ":a.txt"]), "one\nstaged");
        assert_eq!(std::fs::read_to_string(target.join("b.txt")).unwrap(), "committed\nunstaged\n");
        assert_eq!(std::fs::read(target.join("bin.dat")).unwrap(), vec![0u8, 159, 146, 150, 255]);
        assert_eq!(std::fs::read_to_string(target.join("new/dir/c.txt")).unwrap(), "untracked\n");
        assert!(!target.join("ignored.log").exists(), "ignored files never move");
        assert_eq!(git(&target, &["log", "-1", "--format=%s"]), "unpushed");
        let _ = std::fs::remove_dir_all(&repos.root);
    }

    #[test]
    fn a_branch_with_nothing_beyond_its_base_moves_without_a_bundle() {
        let repos = repos();
        let wt = &repos.source_wt;
        std::fs::write(wt.join("a.txt"), "dirty\n").unwrap();
        let source = PackageSource {
            session_id: "s1".to_string(),
            record: Some(record(wt, Some(repos.root.join("source")), Some("exp/s1"))),
            ..PackageSource::default()
        };
        let package = build_package(&source, 64 * 1024 * 1024).unwrap();
        assert!(!package.manifest.bundle);
        assert!(package.manifest.index_diff.is_none(), "nothing staged: no field");
        assert!(package.manifest.base_sha.is_some());
        assert_eq!(package.manifest.head_sha, package.manifest.base_sha);
        let target = restore_on_target(&repos, &package);
        assert_eq!(git(&target, &["rev-parse", "HEAD"]), git(wt, &["rev-parse", "HEAD"]));
        assert_eq!(std::fs::read_to_string(target.join("a.txt")).unwrap(), "dirty\n");
        let _ = std::fs::remove_dir_all(&repos.root);
    }

    #[test]
    fn a_scratch_dir_moves_whole() {
        let src = temp("scratch-src");
        std::fs::create_dir_all(src.join("notes/deep")).unwrap();
        std::fs::write(src.join("plan.md"), "# plan\n").unwrap();
        std::fs::write(src.join("notes/deep/x.bin"), [1u8, 2, 3]).unwrap();
        let mut rec = record(&src, None, None);
        rec.repo = None;
        let source = PackageSource { session_id: "s1".to_string(), record: Some(rec), ..PackageSource::default() };
        let package = build_package(&source, 64 * 1024 * 1024).unwrap();
        assert_eq!(package.manifest.workspace, WorkspaceKind::Scratch);
        let dir = temp("scratch-pkg");
        let manifest = extract_package(&package.bytes, &dir).unwrap();
        let dst = temp("scratch-dst");
        restore_scratch(&dst, &HandoffImport { dir: dir.clone(), manifest, from_device: String::new() }).unwrap();
        assert_eq!(std::fs::read_to_string(dst.join("plan.md")).unwrap(), "# plan\n");
        assert_eq!(std::fs::read(dst.join("notes/deep/x.bin")).unwrap(), vec![1u8, 2, 3]);
        for dir in [src, dir, dst] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// Pseudo-random bytes gzip cannot shrink.
    fn noise(len: usize, seed: u64) -> Vec<u8> {
        let mut state = seed;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect()
    }

    #[test]
    fn over_the_cap_the_transcript_goes_first_then_the_journal_then_the_move_fails() {
        let src = temp("cap");
        std::fs::write(src.join("small.txt"), "x").unwrap();
        let native = src.join("native-file");
        let journal = src.join("journal-file");
        std::fs::write(&native, noise(40_000, 7)).unwrap();
        std::fs::write(&journal, noise(20_000, 11)).unwrap();
        let ws = temp("cap-ws");
        std::fs::write(ws.join("f.txt"), "tiny").unwrap();
        let mut rec = record(&ws, None, None);
        rec.repo = None;
        let source = PackageSource {
            session_id: "s1".to_string(),
            record: Some(rec.clone()),
            journal: Some(journal.clone()),
            native: Some(native.clone()),
        };
        let all = build_package(&source, 1 << 20).unwrap();
        assert!(all.manifest.native && all.manifest.journal);
        let no_native = build_package(&source, 35_000).unwrap();
        assert!(!no_native.manifest.native && no_native.manifest.journal);
        let bare = build_package(&source, 10_000).unwrap();
        assert!(!bare.manifest.native && !bare.manifest.journal);
        // The workspace alone over the cap: never dropped, the move fails.
        std::fs::write(ws.join("big.bin"), noise(30_000, 3)).unwrap();
        assert_eq!(build_package(&source, 10_000).unwrap_err(), PackageError::TooBig);
        // A codex run's transcript never travels.
        let mut codex = rec;
        codex.agent = CodingAgent::Codex;
        let codex = build_package(&PackageSource { record: Some(codex), ..source }, 1 << 20).unwrap();
        assert!(!codex.manifest.native && codex.manifest.journal);
        for dir in [src, ws] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    fn raw_tar(entries: &[(&str, tar::EntryType, &[u8], Option<&str>)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, kind, bytes, link) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(*kind);
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            // `set_path` refuses `..`; write the raw name like a hostile tool.
            let raw = header.as_old_mut();
            raw.name[..name.len()].copy_from_slice(name.as_bytes());
            if let Some(link) = link {
                header.set_link_name(link).unwrap();
            }
            header.set_cksum();
            builder.append(&header, *bytes).unwrap();
        }
        builder.into_inner().unwrap()
    }

    fn gz(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn a_package_extracts_only_whitelisted_files() {
        let manifest = serde_json::to_vec(&Manifest { version: MANIFEST_VERSION, ..Manifest::default() }).unwrap();
        let ok = gz(&raw_tar(&[(MANIFEST, tar::EntryType::Regular, &manifest, None), (DIFF, tar::EntryType::Regular, b"d", None)]));
        let dir = temp("wl");
        extract_package(&ok, &dir).unwrap();
        assert!(dir.join(DIFF).is_file());
        for hostile in [
            raw_tar(&[(MANIFEST, tar::EntryType::Regular, &manifest, None), ("../evil", tar::EntryType::Regular, b"x", None)]),
            raw_tar(&[(MANIFEST, tar::EntryType::Regular, &manifest, None), ("sub/manifest.json", tar::EntryType::Regular, b"x", None)]),
            raw_tar(&[(MANIFEST, tar::EntryType::Regular, &manifest, None), ("other.txt", tar::EntryType::Regular, b"x", None)]),
            raw_tar(&[(MANIFEST, tar::EntryType::Regular, &manifest, None), (JOURNAL, tar::EntryType::Symlink, b"", Some("/etc/passwd"))]),
            raw_tar(&[(DIFF, tar::EntryType::Regular, b"no manifest", None)]),
        ] {
            let dir = temp("wl-bad");
            assert!(extract_package(&gz(&hostile), &dir).is_err());
            assert!(!dir.parent().unwrap().join("evil").exists());
            let _ = std::fs::remove_dir_all(dir);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The top-level entry names of a package.
    fn entry_names(package: &[u8]) -> Vec<String> {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(package));
        archive
            .entries()
            .unwrap()
            .map(|entry| entry.unwrap().path().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    /// The raw `manifest.json` bytes of a package.
    fn manifest_of(package: &[u8]) -> Vec<u8> {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(package));
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.path().unwrap() == Path::new(MANIFEST) {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).unwrap();
                return bytes;
            }
        }
        panic!("no manifest")
    }

    #[test]
    fn base64_round_trips_every_byte_and_refuses_junk() {
        let all: Vec<u8> = (0..=255u8).collect();
        for len in [0usize, 1, 2, 3, 4, 5, 255, 256] {
            let bytes = &all[..len];
            let text = base64_encode(bytes);
            assert_eq!(text.len() % 4, 0);
            assert_eq!(base64_decode(&text).unwrap(), bytes, "len {len}");
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_encode(b"M"), "TQ==");
        assert_eq!(base64_decode("TW\nFu").unwrap(), b"Man");
        assert!(base64_decode("TWF").is_err());
        assert!(base64_decode("TW!u").is_err());
        assert!(base64_decode("T===").is_err());
    }

    /// The 0.14.73 target's `Manifest` (same serde shape, no
    /// `deny_unknown_fields`) reads a manifest with fields it predates.
    #[test]
    fn a_manifest_with_unknown_fields_still_parses() {
        let json = r#"{"version":1,"sessionId":"s1","indexDiff":"TWFu","somethingNewer":{"x":1}}"#;
        let manifest: Manifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.index_diff.as_deref(), Some("TWFu"));
        let without: Manifest = serde_json::from_str(r#"{"version":1,"sessionId":"s1"}"#).unwrap();
        assert!(without.index_diff.is_none());
        assert!(!serde_json::to_string(&without).unwrap().contains("indexDiff"), "absent when nothing is staged");
    }

    #[test]
    fn a_workspace_tar_never_writes_outside() {
        let dst = temp("unpack");
        let escape = raw_tar(&[("../escaped.txt", tar::EntryType::Regular, b"x", None)]);
        assert!(unpack_safely(&escape, &dst).is_err());
        assert!(!dst.parent().unwrap().join("escaped.txt").exists());
        let absolute = raw_tar(&[("/tmp/abs.txt", tar::EntryType::Regular, b"x", None)]);
        assert!(unpack_safely(&absolute, &dst).is_err());
        // A link out is skipped; one inside is kept.
        let links = raw_tar(&[
            ("out", tar::EntryType::Symlink, b"", Some("/etc")),
            ("up", tar::EntryType::Symlink, b"", Some("../..")),
            ("file.txt", tar::EntryType::Regular, b"hi", None),
            ("in", tar::EntryType::Symlink, b"", Some("file.txt")),
        ]);
        assert_eq!(unpack_safely(&links, &dst).unwrap(), 2);
        assert!(std::fs::symlink_metadata(dst.join("out")).is_err());
        assert!(std::fs::symlink_metadata(dst.join("up")).is_err());
        assert_eq!(std::fs::read_to_string(dst.join("in")).unwrap(), "hi");
        let _ = std::fs::remove_dir_all(dst);
    }

    #[test]
    fn cwd_fields_rewrite_onto_the_target_including_windows_to_posix() {
        assert_eq!(
            rewrite_path("/Users/a/wt/sub/x", "/Users/a/wt", "/home/b/repo.worktrees/exp-1", '/').as_deref(),
            Some("/home/b/repo.worktrees/exp-1/sub/x")
        );
        assert_eq!(rewrite_path("/Users/a/wt", "/Users/a/wt", "/t", '/').as_deref(), Some("/t"));
        assert_eq!(rewrite_path("/Users/a/wtx", "/Users/a/wt", "/t", '/'), None);
        assert_eq!(rewrite_path("/elsewhere", "/Users/a/wt", "/t", '/'), None);
        // Windows → POSIX: either separator, any case.
        assert_eq!(
            rewrite_path("c:\\Users\\A\\wt\\src\\main.rs", "C:\\Users\\a\\wt", "/home/b/wt", '/').as_deref(),
            Some("/home/b/wt/src/main.rs")
        );
        assert_eq!(rewrite_path("C:/Users/a/wt/x", "C:\\Users\\a\\wt", "/t", '/').as_deref(), Some("/t/x"));
        // POSIX → Windows.
        assert_eq!(
            rewrite_path("/Users/a/wt/src/x.rs", "/Users/a/wt", "C:\\Users\\b\\wt", '\\').as_deref(),
            Some("C:\\Users\\b\\wt\\src\\x.rs")
        );
        let transcript = concat!(
            r#"{"type":"user","cwd":"C:\\Users\\a\\wt","message":{"content":"hi"}}"#,
            "\n",
            r#"{"type":"assistant","nested":{"cwd":"C:\\Users\\a\\wt\\sub"},"cwdx":"C:\\Users\\a\\wt"}"#,
            "\n",
            "not json at all\n",
        );
        let out = rewrite_transcript(transcript.as_bytes(), "C:\\Users\\a\\wt", "/home/b/wt", '/');
        let out = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = out.split('\n').collect();
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["cwd"], "/home/b/wt");
        assert_eq!(first["message"]["content"], "hi");
        let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(second["nested"]["cwd"], "/home/b/wt/sub");
        assert_eq!(second["cwdx"], "C:\\Users\\a\\wt", "only `cwd` keys move");
        assert_eq!(lines[2], "not json at all");
        assert_eq!(lines[3], "");
    }

    #[test]
    fn munge_matches_claude_and_guards_its_length() {
        assert_eq!(
            munge("/Users/niach/Exponential/repos/Niach/exponential.worktrees/exp-session-2959e62e"),
            "-Users-niach-Exponential-repos-Niach-exponential-worktrees-exp-session-2959e62e"
        );
        assert_eq!(munge("C:\\Users\\a b"), "C--Users-a-b");
        assert_eq!(munge("/ü"), "--");
        let dir = temp("munge");
        let pkg = temp("munge-pkg");
        std::fs::write(pkg.join(NATIVE), r#"{"cwd":"/src/wt"}"#).unwrap();
        let import = HandoffImport {
            dir: pkg.clone(),
            manifest: Manifest {
                version: MANIFEST_VERSION,
                session_id: "s1".to_string(),
                native_session_id: Some("native-1".to_string()),
                source_cwd: "/src/wt".to_string(),
                native: true,
                ..Manifest::default()
            },
            from_device: String::new(),
        };
        let long = PathBuf::from(format!("/{}", "a".repeat(MUNGE_MAX + 5)));
        assert!(import_native(&dir, &long, &import).is_err());
        let short = PathBuf::from("/home/b/wt");
        let path = import_native(&dir, &short, &import).unwrap();
        assert_eq!(path, dir.join("-home-b-wt").join("native-1.jsonl"));
        let line: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(line["cwd"], short.to_string_lossy().as_ref());
        for dir in [dir, pkg] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn the_workspace_waits_for_the_targets_restore_marker_and_survives_restarts() {
        let data = temp("cleanup");
        let scratch = crate::scratch::scratch_root(&data).join("root-1").join("s1");
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::write(scratch.join("f"), "x").unwrap();
        let native = data.join("native.jsonl");
        std::fs::write(&native, "{}").unwrap();
        let mut rec = record(&scratch, None, None);
        rec.repo = None;
        crate::run_registry::record(&data, rec);
        note_moved_away(
            &data,
            MovedAway {
                session_id: "s1".to_string(),
                cwd: scratch.clone(),
                native: Some(native.clone()),
                native_packaged: true,
                target: Some("other".to_string()),
                ..MovedAway::default()
            },
        );
        let row = |device: &str, status: &str, restored: bool| domain::rows::SessionRow {
            id: "s1".to_string(),
            device_id: Some(device.to_string()),
            status: Some(status.to_string()),
            handoff_restored_at: restored.then(|| "2026-10-19T10:00:00Z".to_string()),
            ..Default::default()
        };
        assert!(cleanups_due(&data, "me", &[row("other", "queued", false)]).is_empty());
        assert!(cleanups_due(&data, "me", &[]).is_empty());
        assert_eq!(moved_away(&data).len(), 1, "persisted until due");

        // The CLAIM: only the run record goes.
        assert_eq!(cleanups_due(&data, "me", &[row("other", "running", false)]), vec!["s1".to_string()]);
        cleanup(&data, "s1");
        assert!(crate::run_registry::get(&data, "s1").is_none());
        assert!(scratch.join("f").is_file(), "no restore yet: the scratch dir is kept");
        assert!(native.exists(), "no restore yet: the transcript is kept");
        assert!(moved_away(&data)[0].released);
        // Released, not restored: nothing more is due — a restore that
        // failed (or never comes) leaves the source intact.
        assert!(cleanups_due(&data, "me", &[row("other", "running", false)]).is_empty());
        assert!(cleanups_due(&data, "me", &[row("other", "idle", false)]).is_empty());
        cleanup(&data, "s1");
        assert!(scratch.join("f").is_file());

        // The RESTORE marker: the workspace and the travelled transcript go.
        assert_eq!(cleanups_due(&data, "me", &[row("other", "running", true)]), vec!["s1".to_string()]);
        assert!(moved_away(&data)[0].restored, "the marker is remembered across restarts");
        // Even a row no longer carrying the marker (a later move) settles.
        assert_eq!(cleanups_due(&data, "me", &[row("third", "queued", false)]), vec!["s1".to_string()]);
        cleanup(&data, "s1");
        assert!(!scratch.exists());
        assert!(!native.exists());
        assert!(moved_away(&data).is_empty());

        // A transcript that did not travel stays even after the restore.
        std::fs::write(&native, "{}").unwrap();
        note_moved_away(
            &data,
            MovedAway { session_id: "s1".to_string(), native: Some(native.clone()), ..MovedAway::default() },
        );
        assert_eq!(cleanups_due(&data, "me", &[row("other", "queued", true)]), vec!["s1".to_string()]);
        cleanup(&data, "s1");
        assert!(native.exists(), "the transcript did not travel: kept");

        // A row back on this device drops the pending cleanup.
        note_moved_away(&data, MovedAway { session_id: "s2".to_string(), ..MovedAway::default() });
        let back = domain::rows::SessionRow { id: "s2".to_string(), device_id: Some("me".to_string()), status: Some("running".to_string()), ..Default::default() };
        assert!(cleanups_due(&data, "me", &[back]).is_empty());
        assert!(moved_away(&data).is_empty());
        let _ = std::fs::remove_dir_all(data);
    }

    /// A → B: B claims, its restore fails (fresh launch, no marker). Later
    /// B → C: C restores B's partial package and sets the marker with the
    /// row on C. A's copy is the only one of the work B never restored: the
    /// marker on another device than A's target never settles it.
    #[test]
    fn a_later_moves_restore_marker_never_settles_the_first_sources_copy() {
        let data = temp("cleanup-double");
        let scratch = crate::scratch::scratch_root(&data).join("root-1").join("s1");
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::write(scratch.join("f"), "x").unwrap();
        let mut rec = record(&scratch, None, None);
        rec.repo = None;
        crate::run_registry::record(&data, rec);
        note_moved_away(
            &data,
            MovedAway { session_id: "s1".to_string(), cwd: scratch.clone(), target: Some("b".to_string()), ..MovedAway::default() },
        );
        let row = |device: &str, restored: bool| domain::rows::SessionRow {
            id: "s1".to_string(),
            device_id: Some(device.to_string()),
            status: Some("running".to_string()),
            handoff_restored_at: restored.then(|| "2026-10-19T10:00:00Z".to_string()),
            ..Default::default()
        };
        // B claimed: the run record goes, the workspace waits.
        assert_eq!(cleanups_due(&data, "a", &[row("b", false)]), vec!["s1".to_string()]);
        cleanup(&data, "s1");
        assert!(moved_away(&data)[0].released);
        assert!(scratch.join("f").is_file());
        // C restored B's package: not our marker.
        assert!(cleanups_due(&data, "a", &[row("c", true)]).is_empty());
        assert!(!moved_away(&data)[0].restored, "a later move's marker is not remembered as ours");
        cleanup(&data, "s1");
        assert!(scratch.join("f").is_file(), "the only copy of what B never restored stays");
        // The marker on OUR target settles it.
        assert_eq!(cleanups_due(&data, "a", &[row("b", true)]), vec!["s1".to_string()]);
        assert!(moved_away(&data)[0].restored);
        cleanup(&data, "s1");
        assert!(!scratch.exists());
        assert!(moved_away(&data).is_empty());

        // An entry written before the field (no target) keeps today's rule:
        // any marker off this device counts.
        std::fs::create_dir_all(&scratch).unwrap();
        note_moved_away(&data, MovedAway { session_id: "s1".to_string(), cwd: scratch.clone(), ..MovedAway::default() });
        assert_eq!(cleanups_due(&data, "a", &[row("c", true)]), vec!["s1".to_string()]);
        cleanup(&data, "s1");
        assert!(!scratch.exists());
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn a_restored_repo_move_removes_the_worktree_and_branch() {
        let repos = repos();
        let clone = repos.root.join("source");
        let cwd = repos.source_wt.clone();
        std::fs::write(cwd.join("work.txt"), "unpushed").unwrap();
        let data = repos.root.join("data");
        note_moved_away(
            &data,
            MovedAway {
                session_id: "s1".to_string(),
                cwd: cwd.clone(),
                clone: Some(clone.clone()),
                branch: Some("exp/s1".to_string()),
                ..MovedAway::default()
            },
        );
        let claimed = domain::rows::SessionRow {
            id: "s1".to_string(),
            device_id: Some("other".to_string()),
            status: Some("running".to_string()),
            ..Default::default()
        };
        assert_eq!(cleanups_due(&data, "me", std::slice::from_ref(&claimed)), vec!["s1".to_string()]);
        cleanup(&data, "s1");
        assert!(cwd.join("work.txt").is_file(), "the claim alone keeps the worktree");
        assert!(!git(&clone, &["branch", "--list", "exp/s1"]).is_empty());
        let restored = domain::rows::SessionRow {
            handoff_restored_at: Some("2026-10-19T10:00:00Z".to_string()),
            ..claimed
        };
        assert_eq!(cleanups_due(&data, "me", &[restored]), vec!["s1".to_string()]);
        cleanup(&data, "s1");
        assert!(!cwd.exists());
        assert!(git(&clone, &["branch", "--list", "exp/s1"]).is_empty());
        let _ = std::fs::remove_dir_all(&repos.root);
    }

    #[test]
    fn the_target_record_places_the_workspace_here() {
        let row = domain::rows::SessionRow {
            id: "s1".to_string(),
            root_id: Some("r1".to_string()),
            cli: Some("claude".to_string()),
            account: Some("prof-b".to_string()),
            model: Some("sonnet".to_string()),
            ..Default::default()
        };
        let manifest = Manifest {
            version: MANIFEST_VERSION,
            agent: "claude".to_string(),
            native_session_id: Some("native-1".to_string()),
            workspace: WorkspaceKind::Repo,
            repo: Some("acme/web".to_string()),
            branch: Some("exp/s1".to_string()),
            base_branch: Some("main".to_string()),
            native: true,
            ..Manifest::default()
        };
        let record = target_record(&row, &manifest, Path::new("/data"), Path::new("/repos"), "acct").unwrap();
        assert_eq!(record.cwd, PathBuf::from("/repos/acme/web.worktrees/exp-s1"));
        assert_eq!(record.clone, Some(PathBuf::from("/repos/acme/web")));
        assert_eq!(record.claude_session_id.as_deref(), Some("native-1"));
        assert_eq!(record.account().as_deref(), Some("prof-b"));
        assert_eq!(record.model, "sonnet");
        assert!(record.workspace_reclaimed());
        assert!(record.pending_consumes().is_empty());
        // EXP-1355: the source's owed consumes travel with the record.
        let owed = Manifest { pending_consumes: vec!["m1".to_string()], ..manifest.clone() };
        let record = target_record(&row, &owed, Path::new("/data"), Path::new("/repos"), "acct").unwrap();
        assert_eq!(record.pending_consumes(), vec!["m1".to_string()]);
        let scratch = target_record(
            &row,
            &Manifest { workspace: WorkspaceKind::Scratch, native: false, ..manifest },
            Path::new("/data"),
            Path::new("/repos"),
            "acct",
        )
        .unwrap();
        assert_eq!(scratch.cwd, PathBuf::from("/data/scratch/r1/s1"));
        assert_eq!(scratch.claude_session_id, None);
    }
}
