//! #EXP-2059: a worktree whose files still stand at the state BEFORE `HEAD`.
//!
//! Seen twice (`exp/leftover-14bdcb10`: 92 files, −2,757 lines;
//! `exp/leftover-af47e75e`): a merge of `origin/master` landed on a branch
//! while its ended session's worktree kept the pre-merge files, so the
//! finish's WIP commit was the exact inverse of that merge: "recovering" it
//! would have reverted master's changes.
//!
//! [`bring_up_to_head`] runs before every WIP save ([`crate::wip_save`]).
//! For a parent `P` of `HEAD`, the worktree is stale when NONE of the paths
//! `P..HEAD` changed is at `HEAD`'s version and at least one is still at
//! `P`'s. Then:
//! - a path still at `P`'s version carries no work: it is checked out at
//!   `HEAD` (a file `HEAD` deleted goes);
//! - a path that differs from both was edited on top of `P`: the edit is
//!   merged onto `HEAD`'s version (`git merge-file`, base `P`);
//! - an edit that cannot be merged (a conflict, a binary, a file `HEAD`
//!   added or deleted) changes nothing: under a merge commit the save is
//!   refused and the worktree stays on disk, else the save goes on as before.
//!
//! Every other change is outside `P..HEAD` and saved as it is. Nothing is
//! touched until every merge has succeeded.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::auto_push::{git_dir, op_in_progress};

/// Paths per git argv (well under any OS argv limit).
const PATH_CHUNK: usize = 200;

/// What [`bring_up_to_head`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stale {
    pub head: String,
    /// The parent of `head` the worktree stood at.
    pub parent: String,
    /// Paths that were at `parent`'s version: checked out at `head`.
    pub dropped: usize,
    /// Paths edited on top of `parent`: the edit now sits on `head`'s version.
    pub merged: usize,
}

impl Stale {
    /// One line for the log and the WIP commit's body.
    pub fn note(&self) -> String {
        let short = |sha: &str| sha.chars().take(11).collect::<String>();
        let (head, parent) = (short(&self.head), short(&self.parent));
        let merged = match self.merged {
            0 => String::new(),
            n => format!(" {n} edited path(s) were merged onto {head}."),
        };
        format!(
            "Not saved: {} path(s) stood at {parent}, the state before {head}; saved, they would have reverted it.{merged}",
            self.dropped
        )
    }
}

/// One path `parent..head` changed.
struct Delta {
    path: String,
    /// `--name-status`: `A` added by `head`, `D` deleted by it, `M`/`T`.
    status: char,
}

fn git_bytes(worktree: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut command = terminal::process::background_command("git");
    command.args(args).current_dir(worktree).env("GIT_TERMINAL_PROMPT", "0").env("LC_ALL", "C");
    let output = command.output().map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn git(worktree: &Path, args: &[&str]) -> Result<String, String> {
    git_bytes(worktree, args).map(|out| String::from_utf8_lossy(&out).into_owned())
}

fn nul_fields(out: &str) -> Vec<&str> {
    out.split('\0').filter(|field| !field.is_empty()).collect()
}

/// `git <args> -- <paths>` in chunks, the outputs joined.
fn chunked(worktree: &Path, args: &[&str], paths: &[&str]) -> Result<String, String> {
    let mut out = String::new();
    for chunk in paths.chunks(PATH_CHUNK) {
        let mut argv = vec!["--literal-pathspecs"];
        argv.extend_from_slice(args);
        argv.push("--");
        argv.extend_from_slice(chunk);
        out.push_str(&git(worktree, &argv)?);
    }
    Ok(out)
}

/// `path → (mode, blob)` of `commit` for `paths` (the ones it has).
fn tree_entries(worktree: &Path, commit: &str, paths: &[&str]) -> Result<BTreeMap<String, (String, String)>, String> {
    let out = chunked(worktree, &["ls-tree", "-z", commit], paths)?;
    let mut entries = BTreeMap::new();
    for field in nul_fields(&out) {
        let Some((meta, path)) = field.split_once('\t') else { continue };
        let mut meta = meta.split(' ');
        if let (Some(mode), Some(_), Some(oid)) = (meta.next(), meta.next(), meta.next()) {
            entries.insert(path.to_string(), (mode.to_string(), oid.to_string()));
        }
    }
    Ok(entries)
}

/// The blob id each of `paths` has in the worktree (clean filters applied);
/// a missing file has none. A symlink hashes its target path, like git.
fn worktree_blobs(worktree: &Path, paths: &[&str]) -> Result<BTreeMap<String, String>, String> {
    let mut blobs = BTreeMap::new();
    let mut files = Vec::new();
    for path in paths {
        match std::fs::symlink_metadata(worktree.join(path)) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let target = std::fs::read_link(worktree.join(path)).map_err(|err| err.to_string())?;
                let scratch = scratch_dir(worktree)?.join("link");
                std::fs::write(&scratch, target.to_string_lossy().as_bytes()).map_err(|err| err.to_string())?;
                let oid = git(worktree, &["hash-object", "--no-filters", "--", &scratch.to_string_lossy()])?;
                blobs.insert(path.to_string(), oid.trim().to_string());
            }
            Ok(meta) if meta.is_file() => files.push(*path),
            // A directory where a file was: neither version.
            Ok(_) => {
                blobs.insert(path.to_string(), String::new());
            }
            Err(_) => {}
        }
    }
    for chunk in files.chunks(PATH_CHUNK) {
        let mut argv = vec!["hash-object", "--"];
        argv.extend_from_slice(chunk);
        let out = git(worktree, &argv)?;
        let oids: Vec<&str> = out.lines().collect();
        if oids.len() != chunk.len() {
            return Err("git hash-object: unexpected output".to_string());
        }
        for (path, oid) in chunk.iter().zip(oids) {
            blobs.insert(path.to_string(), oid.trim().to_string());
        }
    }
    Ok(blobs)
}

/// A private scratch dir in the worktree's own git dir.
fn scratch_dir(worktree: &Path) -> Result<PathBuf, String> {
    let dir = git_dir(worktree).ok_or_else(|| "no git dir".to_string())?.join("exp-stale-worktree");
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

/// The worktree's edit of `path` (made on `parent`'s version) on top of
/// `head`'s version; `Err` on a conflict or a binary.
fn merge_edit(worktree: &Path, scratch: &Path, parent: &str, head: &str, path: &str) -> Result<Vec<u8>, String> {
    let side = |name: &str, commit: &str| -> Result<PathBuf, String> {
        let bytes = git_bytes(worktree, &["cat-file", "--filters", &format!("{commit}:{path}")])?;
        let file = scratch.join(name);
        std::fs::write(&file, bytes).map_err(|err| err.to_string())?;
        Ok(file)
    };
    let base = side("base", parent)?;
    let theirs = side("head", head)?;
    let ours = scratch.join("edit");
    std::fs::copy(worktree.join(path), &ours).map_err(|err| err.to_string())?;
    git(
        worktree,
        &["merge-file", "-q", &ours.to_string_lossy(), &base.to_string_lossy(), &theirs.to_string_lossy()],
    )
    .map_err(|detail| format!("{path} does not merge onto {head}: {detail}"))?;
    std::fs::read(&ours).map_err(|err| err.to_string())
}

/// Bring a worktree that still stands at a parent of `HEAD` up to `HEAD`
/// (module docs). `Ok(None)` = it is not stale; `Err` = stale under a merge
/// commit with an edit that does not merge: nothing was touched.
pub fn bring_up_to_head(worktree: &Path) -> Result<Option<Stale>, String> {
    let Some(dir) = git_dir(worktree) else {
        return Ok(None);
    };
    if op_in_progress(&dir) {
        return Ok(None);
    }
    let Ok(line) = git(worktree, &["rev-list", "--parents", "-n", "1", "HEAD"]) else {
        return Ok(None);
    };
    let mut ids = line.split_whitespace().map(str::to_string);
    let Some(head) = ids.next() else {
        return Ok(None);
    };
    let parents: Vec<String> = ids.collect();
    let Ok(changed) = git(worktree, &["diff", "--name-only", "-z", "--no-renames", "HEAD"]) else {
        return Ok(None);
    };
    let changed: BTreeSet<&str> = nul_fields(&changed).into_iter().collect();
    if changed.is_empty() {
        return Ok(None);
    }
    let scratch = scratch_dir(worktree)?;
    let result = parents.iter().find_map(|parent| match stale_at(worktree, &scratch, &head, parent, &changed, parents.len() > 1) {
        Ok(None) => None,
        Ok(Some(stale)) => Some(Ok(stale)),
        Err(detail) => Some(Err(detail)),
    });
    let _ = std::fs::remove_dir_all(&scratch);
    result.transpose()
}

fn stale_at(
    worktree: &Path,
    scratch: &Path,
    head: &str,
    parent: &str,
    changed: &BTreeSet<&str>,
    merge_commit: bool,
) -> Result<Option<Stale>, String> {
    let Ok(out) = git(worktree, &["diff", "--name-status", "-z", "--no-renames", parent, head]) else {
        return Ok(None);
    };
    let fields = nul_fields(&out);
    let delta: Vec<Delta> = fields
        .chunks(2)
        .filter(|pair| pair.len() == 2)
        .map(|pair| Delta { path: pair[1].to_string(), status: pair[0].chars().next().unwrap_or('M') })
        .collect();
    // The cheap way out: an ordinary WIP rarely touches what `HEAD` changed.
    if !delta.iter().any(|entry| changed.contains(entry.path.as_str())) {
        return Ok(None);
    }
    let paths: Vec<&str> = delta.iter().map(|entry| entry.path.as_str()).collect();
    let (Ok(before), Ok(after), Ok(now)) = (
        tree_entries(worktree, parent, &paths),
        tree_entries(worktree, head, &paths),
        worktree_blobs(worktree, &paths),
    ) else {
        return Ok(None);
    };
    let mut dropped: Vec<&Delta> = Vec::new();
    let mut edited: Vec<&Delta> = Vec::new();
    for entry in &delta {
        let blob = |entries: &BTreeMap<String, (String, String)>| entries.get(&entry.path).map(|(_, oid)| oid.clone());
        let current = now.get(&entry.path).cloned();
        if current == blob(&after) {
            // One path at `HEAD`'s version: the worktree has seen `HEAD`.
            return Ok(None);
        }
        if current == blob(&before) {
            dropped.push(entry);
        } else {
            edited.push(entry);
        }
    }
    if dropped.is_empty() {
        return Ok(None);
    }
    let mut merges: Vec<(&str, Vec<u8>)> = Vec::new();
    for entry in &edited {
        let regular = |entries: &BTreeMap<String, (String, String)>| {
            entries.get(&entry.path).is_some_and(|(mode, _)| mode.starts_with("100"))
        };
        let merged = if entry.status == 'M' && regular(&before) && regular(&after) && worktree.join(&entry.path).is_file() {
            merge_edit(worktree, scratch, parent, head, &entry.path)
        } else {
            Err(format!("{} was added, deleted or retyped by {head}", entry.path))
        };
        match merged {
            Ok(bytes) => merges.push((entry.path.as_str(), bytes)),
            // A merge commit never came from this worktree: saving would
            // revert it, so everything stays on disk.
            Err(detail) if merge_commit => {
                return Err(format!(
                    "the worktree stands at {parent}, before the merge {head}, with an edit that does not merge ({detail}); the uncommitted work stays in the worktree"
                ));
            }
            Err(detail) => {
                log::info!("WIP save in {}: not brought up to {head}: {detail}", worktree.display());
                return Ok(None);
            }
        }
    }
    for (path, bytes) in &merges {
        std::fs::write(worktree.join(path), bytes).map_err(|err| format!("write {path}: {err}"))?;
    }
    let kept: Vec<&str> = dropped.iter().filter(|entry| entry.status != 'D').map(|entry| entry.path.as_str()).collect();
    let gone: Vec<&str> = dropped.iter().filter(|entry| entry.status == 'D').map(|entry| entry.path.as_str()).collect();
    chunked(worktree, &["restore", "--source=HEAD", "--staged", "--worktree"], &kept)?;
    chunked(worktree, &["rm", "-q", "-f", "--cached", "--ignore-unmatch"], &gone)?;
    for path in &gone {
        // `parent`'s own blob, byte for byte: history has it.
        let _ = std::fs::remove_file(worktree.join(path));
    }
    Ok(Some(Stale { head: head.to_string(), parent: parent.to_string(), dropped: dropped.len(), merged: merges.len() }))
}
