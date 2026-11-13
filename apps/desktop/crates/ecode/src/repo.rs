//! `LocalRepo`: the local git on gitoxide (feature `repo`). open/init,
//! commits from per-path changes, log, diff, revert, refs with
//! compare-and-swap, and the path-wise three-way merge the spec's
//! mergeability is defined by (`forge.json`).
//!
//! What gix does not cover yet runs on git argv behind the same API
//! (the coding crate's "argv only" rule): push and fetch ([`crate::Remote`]),
//! the squash merge (`merge-tree --write-tree` + `commit-tree`),
//! `rebase --onto` (in a throwaway worktree) and bundles (gix 0.90 has no
//! bundle reader or writer).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ecode_core::{diff_trees, three_way, Changes, Mergeability, Tree};
use gix::hash::ObjectId;
use gix::object::tree::EntryKind;
use gix::refs::transaction::{Change as RefChange, LogChange, PreviousValue, RefEdit, RefLog};
use gix::refs::Target;

use crate::{Change, Error, FileChange, GitCommand, MergeCheck, Result, StoreError};

/// Who commits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub email: String,
}

impl Default for Identity {
    fn default() -> Self {
        Self { name: "eCode".into(), email: "ecode@localhost".into() }
    }
}

/// What a ref must hold before it moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expected<'a> {
    Any,
    /// The ref must not exist.
    Absent,
    /// The ref must point at this sha.
    Is(&'a str),
}

impl<'a> From<Option<&'a str>> for Expected<'a> {
    /// `Some(sha)` = must be `sha`, `None` = must not exist.
    fn from(value: Option<&'a str>) -> Self {
        value.map_or(Expected::Absent, Expected::Is)
    }
}

/// One commit of a log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    pub sha: String,
    pub parents: Vec<String>,
    pub message: String,
    pub author: String,
    /// Unix seconds.
    pub time: i64,
}

/// A revert's outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reverted {
    /// The new commit on the branch.
    Commit(String),
    /// The paths a later commit changed too; nothing was written.
    Conflicts(Vec<String>),
}

/// What a bundle carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundleInfo {
    /// `(ref name, sha)`.
    pub refs: Vec<(String, String)>,
    /// Commits the receiver must have already.
    pub prerequisites: Vec<String>,
}

/// A file's mode and object, as the model's tree value: `"<mode> <oid>"`.
fn entry_value(kind: EntryKind, id: &ObjectId) -> String {
    format!("{:o} {id}", kind as u16)
}

fn parse_entry(value: &str) -> Option<(EntryKind, ObjectId)> {
    let (mode, id) = value.split_once(' ')?;
    let kind = match u16::from_str_radix(mode, 8).ok()? {
        0o100755 => EntryKind::BlobExecutable,
        0o120000 => EntryKind::Link,
        0o160000 => EntryKind::Commit,
        0o040000 => EntryKind::Tree,
        _ => EntryKind::Blob,
    };
    Some((kind, ObjectId::from_hex(id.as_bytes()).ok()?))
}

fn kind_of(mode: u16) -> Option<EntryKind> {
    Some(match mode {
        0o040000 => return None,
        0o100755 => EntryKind::BlobExecutable,
        0o120000 => EntryKind::Link,
        0o160000 => EntryKind::Commit,
        _ => EntryKind::Blob,
    })
}

fn err(op: &'static str) -> impl Fn(gix::Error) -> Error {
    move |e| Error::git(op, e)
}

fn now_seconds() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// The local repository.
pub struct LocalRepo {
    repo: gix::ThreadSafeRepository,
    identity: Identity,
}

impl LocalRepo {
    /// Opens the repository at `path` (a work tree or a git dir).
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let repo = gix::open(path).map_err(err("open"))?;
        Ok(Self::wrap(repo))
    }

    /// Creates a repository with a work tree at `path`.
    pub fn init(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::wrap(gix::init(path).map_err(err("init"))?))
    }

    /// Creates a bare repository at `path` (a forge's).
    pub fn init_bare(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::wrap(gix::init_bare(path).map_err(err("init"))?))
    }

    fn wrap(repo: gix::Repository) -> Self {
        let identity = repo
            .committer()
            .and_then(|s| s.ok())
            .map(|s| Identity { name: s.name.to_string(), email: s.email.to_string() })
            .unwrap_or_default();
        Self { repo: repo.into_sync(), identity }
    }

    /// Commits as `identity` from now on.
    pub fn with_identity(mut self, identity: Identity) -> Self {
        self.identity = identity;
        self
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    fn repo(&self) -> gix::Repository {
        self.repo.to_thread_local()
    }

    /// The git dir.
    pub fn git_dir(&self) -> PathBuf {
        self.repo.path().to_path_buf()
    }

    /// The work tree, `None` for a bare repository.
    pub fn work_dir(&self) -> Option<PathBuf> {
        self.repo().workdir().map(Path::to_path_buf)
    }

    // ── reading ─────────────────────────────────────────────────────────

    fn oid(sha: &str) -> Option<ObjectId> {
        (sha.len() == 40).then(|| ObjectId::from_hex(sha.as_bytes()).ok()).flatten()
    }

    fn commit_id(&self, sha: &str) -> Result<ObjectId> {
        self.resolve(sha)?.and_then(|s| Self::oid(&s)).ok_or_else(|| Error::Invalid(format!("unknown commit {sha}")))
    }

    /// A commit's sha from a full sha, a branch name or a full ref name;
    /// `None` = no such commit.
    pub fn resolve(&self, rev: &str) -> Result<Option<String>> {
        let repo = self.repo();
        if let Some(id) = Self::oid(rev) {
            let found = repo.try_find_object(id).map_err(err("find object"))?;
            return Ok(found.filter(|o| o.kind == gix::object::Kind::Commit).map(|_| id.to_string()));
        }
        let name = if rev.starts_with("refs/") { rev.to_string() } else { format!("refs/heads/{rev}") };
        let Ok(Some(mut reference)) = repo.try_find_reference(name.as_str()) else { return Ok(None) };
        let id = reference.peel_to_id().map_err(err("peel ref"))?;
        Ok(Some(id.to_string()))
    }

    /// A branch's tip, `None` = no such branch.
    pub fn branch(&self, name: &str) -> Result<Option<String>> {
        let repo = self.repo();
        let Ok(Some(mut reference)) = repo.try_find_reference(format!("refs/heads/{name}").as_str()) else {
            return Ok(None);
        };
        Ok(Some(reference.peel_to_id().map_err(err("peel ref"))?.to_string()))
    }

    /// Every local branch and its tip, by name.
    pub fn branches(&self) -> Result<BTreeMap<String, String>> {
        let repo = self.repo();
        let platform = repo.references().map_err(err("list refs"))?;
        let mut out = BTreeMap::new();
        for reference in platform.local_branches().map_err(err("list refs"))? {
            let mut reference = reference.map_err(|e| Error::git("list refs", e))?;
            let name = reference.name().shorten().to_string();
            let id = reference.peel_to_id().map_err(err("peel ref"))?;
            out.insert(name, id.to_string());
        }
        Ok(out)
    }

    /// Whether `ancestor` is `of` or one of its ancestors.
    pub fn is_ancestor(&self, ancestor: &str, of: &str) -> Result<bool> {
        if ancestor == of {
            return Ok(true);
        }
        Ok(self.merge_base(ancestor, of)?.is_some_and(|base| base == ancestor))
    }

    /// The best common ancestor, `None` = unrelated histories.
    pub fn merge_base(&self, a: &str, b: &str) -> Result<Option<String>> {
        let (a, b) = (self.commit_id(a)?, self.commit_id(b)?);
        if a == b {
            return Ok(Some(a.to_string()));
        }
        Ok(self.repo().merge_base(a, b).ok().flatten().map(|id| id.to_string()))
    }

    /// The files of a commit as the model's tree (`path → "<mode> <oid>"`);
    /// `None` = the empty tree.
    pub fn tree(&self, commit: Option<&str>) -> Result<Tree> {
        let Some(commit) = commit else { return Ok(Tree::new()) };
        let repo = self.repo();
        let commit = repo.find_commit(self.commit_id(commit)?).map_err(err("find commit"))?;
        let tree = commit.tree().map_err(err("find tree"))?;
        let files = tree.traverse().breadthfirst.files().map_err(err("walk tree"))?;
        Ok(files
            .into_iter()
            .filter_map(|e| {
                let kind = kind_of(e.mode.value())?;
                Some((String::from_utf8_lossy(&e.filepath).into_owned(), entry_value(kind, &e.oid)))
            })
            .collect())
    }

    /// A file's bytes at a commit; `None` = no such file.
    pub fn read_file(&self, commit: &str, path: &str) -> Result<Option<Vec<u8>>> {
        let Some(value) = self.tree(Some(commit))?.remove(path) else { return Ok(None) };
        let (_, id) = parse_entry(&value).ok_or_else(|| Error::Invalid(value.clone()))?;
        let repo = self.repo();
        let blob = repo.find_blob(id).map_err(err("find blob"))?;
        Ok(Some(blob.data.clone()))
    }

    /// The commits reachable from `rev`, newest first, at most `limit`.
    pub fn log(&self, rev: &str, limit: usize) -> Result<Vec<LogEntry>> {
        let repo = self.repo();
        let tip = self.commit_id(rev)?;
        let walk = repo.rev_walk([tip]).all().map_err(err("log"))?;
        let mut out = vec![];
        for info in walk.take(limit) {
            let info = info.map_err(|e| Error::git("log", e))?;
            let commit = info.object().map_err(err("log"))?;
            let decoded = commit.decode().map_err(|e| Error::git("log", e))?;
            out.push(LogEntry {
                sha: info.id.to_string(),
                parents: decoded.parents().map(|p| p.to_string()).collect(),
                message: decoded.message.to_string(),
                author: decoded.author().map(|a| a.name.to_string()).unwrap_or_default(),
                time: decoded.committer().ok().and_then(|c| c.time().ok()).map_or(0, |t| t.seconds),
            });
        }
        Ok(out)
    }

    /// What changed from `from` (`None` = the empty tree) to `to`, in path
    /// order (the classification of `pr.files`).
    pub fn diff(&self, from: Option<&str>, to: &str) -> Result<Vec<FileChange>> {
        let before = self.tree(from)?;
        let after = self.tree(Some(to))?;
        Ok(classify(&before, &diff_trees(&before, &after)))
    }

    /// The spec's mergeability of `head` into `base` (`forge.json`): a
    /// path-wise three-way merge over the merge base. Also returns the
    /// merged tree.
    pub fn mergeability(&self, base: &str, head: &str) -> Result<(MergeCheck, Tree)> {
        let origin = self.merge_base(base, head)?;
        let (tree, conflicts) = three_way(&self.tree(origin.as_deref())?, &self.tree(Some(base))?, &self.tree(Some(head))?);
        let mergeability = if conflicts.is_empty() { Mergeability::Clean } else { Mergeability::Dirty };
        Ok((MergeCheck { mergeability, conflicts }, tree))
    }

    // ── writing ─────────────────────────────────────────────────────────

    fn signature(&self) -> gix::actor::Signature {
        gix::actor::Signature {
            name: self.identity.name.as_str().into(),
            email: self.identity.email.as_str().into(),
            time: gix::date::Time::new(now_seconds(), 0),
        }
    }

    /// Writes a tree: `parent`'s with the model changes applied
    /// (`"<mode> <oid>"` or `None` = removed).
    fn write_model_tree(&self, parent: Option<ObjectId>, changes: &Changes) -> Result<ObjectId> {
        let repo = self.repo();
        let mut editor = match parent {
            Some(commit) => {
                let tree = repo.find_commit(commit).map_err(err("find commit"))?.tree_id().map_err(err("find tree"))?;
                repo.edit_tree(tree).map_err(err("edit tree"))?
            }
            None => repo.empty_tree().edit().map_err(err("edit tree"))?,
        };
        for (path, value) in changes {
            match value {
                Some(value) => {
                    let (kind, id) = parse_entry(value).ok_or_else(|| Error::Invalid(value.clone()))?;
                    editor.upsert(path.as_str(), kind, id).map_err(err("edit tree"))?;
                }
                None => {
                    editor.remove(path.as_str()).map_err(err("edit tree"))?;
                }
            }
        }
        Ok(editor.write().map_err(err("write tree"))?.detach())
    }

    fn write_commit(&self, tree: ObjectId, parents: Vec<ObjectId>, message: &str) -> Result<String> {
        let signature = self.signature();
        let commit = gix::objs::Commit {
            tree,
            parents: parents.into_iter().collect(),
            author: signature.clone(),
            committer: signature,
            encoding: None,
            message: message.into(),
            extra_headers: vec![],
        };
        Ok(self.repo().write_object(&commit).map_err(err("write commit"))?.to_string())
    }

    /// Writes a commit on `parent` (`None` = a root) with per-path changes
    /// (bytes, or `None` = removed). Moves no ref.
    pub fn commit(&self, parent: Option<&str>, changes: &BTreeMap<String, Option<Vec<u8>>>, message: &str) -> Result<String> {
        let parent = parent.map(|p| self.commit_id(p)).transpose()?;
        let repo = self.repo();
        let mut model = Changes::new();
        for (path, content) in changes {
            let value = match content {
                Some(bytes) => Some(entry_value(EntryKind::Blob, &repo.write_blob(bytes).map_err(err("write blob"))?.detach())),
                None => None,
            };
            model.insert(path.clone(), value);
        }
        let tree = self.write_model_tree(parent, &model)?;
        self.write_commit(tree, parent.into_iter().collect(), message)
    }

    /// Commits the changes on top of `branch` and moves it (created when
    /// absent); `Conflict` when the branch moved meanwhile.
    pub fn commit_on(&self, branch: &str, changes: &BTreeMap<String, Option<Vec<u8>>>, message: &str) -> Result<String> {
        let tip = self.branch(branch)?;
        let sha = self.commit(tip.as_deref(), changes, message)?;
        self.set_branch(branch, &sha, tip.as_deref().into())?;
        Ok(sha)
    }

    /// Reverts `commit` on top of `branch` (eDE's undo): the inverse of its
    /// changes, three-way against what came after it. Conflicting paths
    /// write nothing.
    pub fn revert(&self, branch: &str, commit: &str, message: &str) -> Result<Reverted> {
        let tip = self.branch(branch)?.ok_or_else(|| Error::Invalid(format!("no branch {branch}")))?;
        let target = self.commit_id(commit)?;
        let parent = self.repo().find_commit(target).map_err(err("find commit"))?.parent_ids().next().map(|p| p.to_string());
        let ours = self.tree(Some(&tip))?;
        let (merged, conflicts) = three_way(&self.tree(Some(commit))?, &ours, &self.tree(parent.as_deref())?);
        if !conflicts.is_empty() {
            return Ok(Reverted::Conflicts(conflicts));
        }
        let tip_id = self.commit_id(&tip)?;
        let tree = self.write_model_tree(Some(tip_id), &diff_trees(&ours, &merged))?;
        let sha = self.write_commit(tree, vec![tip_id], message)?;
        self.set_branch(branch, &sha, Expected::Is(&tip))?;
        Ok(Reverted::Commit(sha))
    }

    /// Moves (or creates) a branch, compare-and-swap against `expected`.
    pub fn set_branch(&self, name: &str, sha: &str, expected: Expected<'_>) -> Result<()> {
        let new = self.commit_id(sha)?;
        self.edit_branch(name, RefChange::Update {
            log: LogChange { mode: RefLog::AndReference, force_create_reflog: false, message: "ecode".into() },
            expected: self.previous(expected)?,
            new: Target::Object(new),
        }, expected)
    }

    /// Deletes a branch, compare-and-swap against `expected`.
    pub fn delete_branch(&self, name: &str, expected: Expected<'_>) -> Result<()> {
        self.edit_branch(name, RefChange::Delete { expected: self.previous(expected)?, log: RefLog::AndReference }, expected)
    }

    fn previous(&self, expected: Expected<'_>) -> Result<PreviousValue> {
        Ok(match expected {
            Expected::Any => PreviousValue::Any,
            Expected::Absent => PreviousValue::MustNotExist,
            Expected::Is(sha) => PreviousValue::MustExistAndMatch(Target::Object(
                Self::oid(sha).ok_or_else(|| Error::Invalid(format!("not a sha: {sha}")))?,
            )),
        })
    }

    fn edit_branch(&self, name: &str, change: RefChange, expected: Expected<'_>) -> Result<()> {
        let full = format!("refs/heads/{name}");
        let full_name: gix::refs::FullName =
            full.as_str().try_into().map_err(|_| Error::Invalid(format!("invalid branch name {name}")))?;
        let edit = RefEdit { change, name: full_name, deref: false };
        // The reflog names the committer: ours, never the host's git config
        // (a CI runner or a fresh device has none).
        let signature = self.signature();
        let mut time = gix::date::parse::TimeBuf::default();
        match self.repo().edit_references_as([edit], Some(signature.to_ref(&mut time))) {
            Ok(_) => Ok(()),
            Err(e) => {
                let actual = self.branch(name)?;
                let matches = match expected {
                    Expected::Any => true,
                    Expected::Absent => actual.is_none(),
                    Expected::Is(sha) => actual.as_deref() == Some(sha),
                };
                if matches {
                    Err(Error::git("update ref", e))
                } else {
                    Err(StoreError::Conflict { name: full, actual }.into())
                }
            }
        }
    }

    // ── git argv ────────────────────────────────────────────────────────

    /// A git command on this repository, committing as its identity.
    pub fn git(&self) -> GitCommand {
        GitCommand::new(None)
            .arg(format!("--git-dir={}", self.git_dir().display()))
            .env("GIT_AUTHOR_NAME", &self.identity.name)
            .env("GIT_AUTHOR_EMAIL", &self.identity.email)
            .env("GIT_COMMITTER_NAME", &self.identity.name)
            .env("GIT_COMMITTER_EMAIL", &self.identity.email)
    }

    /// The squash commit of `head` onto `base` (`git merge-tree --write-tree`
    /// + `commit-tree`): one commit whose parent is `base`. Moves no ref;
    ///   refuses `dirty` when the trees conflict.
    pub fn squash_commit(&self, base: &str, head: &str, message: &str) -> Result<String> {
        self.squash_commit_by(base, head, message, None)
    }

    /// [`Self::squash_commit`] authored by `author` (the PR's author, as the
    /// merge train's squash does); `None` = this repo's identity. The
    /// committer stays this repo's identity.
    pub fn squash_commit_by(&self, base: &str, head: &str, message: &str, author: Option<&Identity>) -> Result<String> {
        let (base, head) = (self.commit_id(base)?.to_string(), self.commit_id(head)?.to_string());
        let mut merge = self.git().args(["merge-tree", "--write-tree", "--no-messages"]);
        merge = match self.merge_base(&base, &head)? {
            Some(origin) => merge.arg(format!("--merge-base={origin}")),
            None => merge.arg("--allow-unrelated-histories"),
        };
        let out = merge.args([&base, &head]).output()?;
        if !out.success {
            return Err(Error::refused(ecode_core::ForgeReason::Dirty));
        }
        let tree = out.stdout.lines().next().unwrap_or_default().trim().to_string();
        let mut commit = self.git();
        if let Some(author) = author {
            commit = commit.env("GIT_AUTHOR_NAME", &author.name).env("GIT_AUTHOR_EMAIL", &author.email);
        }
        let sha = commit.args(["commit-tree", &tree, "-p", &base, "-m", message]).run()?;
        Ok(sha.trim().to_string())
    }

    /// `git rebase --onto <onto> <upstream>` of `tip`, in a throwaway
    /// worktree: every commit kept (also ones that end up empty). Moves no
    /// ref; refuses `dirty` on a conflict.
    pub fn rebase_commits(&self, tip: &str, onto: &str, upstream: &str) -> Result<String> {
        let (tip, onto, upstream) = (self.commit_id(tip)?, self.commit_id(onto)?, self.commit_id(upstream)?);
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ecode-rebase-{}-{seq}", std::process::id()));
        let dir_arg = dir.display().to_string();
        self.git().args(["worktree", "add", "--detach", "--quiet", &dir_arg, &tip.to_string()]).run()?;
        // Inside the worktree: its `.git` file names the repository.
        let in_tree = || GitCommand::new(Some(&dir))
            .env("GIT_AUTHOR_NAME", &self.identity.name)
            .env("GIT_AUTHOR_EMAIL", &self.identity.email)
            .env("GIT_COMMITTER_NAME", &self.identity.name)
            .env("GIT_COMMITTER_EMAIL", &self.identity.email)
            .args(["-c", "rebase.autoStash=false", "-c", "rebase.updateRefs=false"]);
        let rebased = in_tree()
            .args(["rebase", "--quiet", "--empty=keep", "--reapply-cherry-picks", "--no-autosquash", "--onto"])
            .args([onto.to_string(), upstream.to_string()])
            .output();
        let result = match rebased {
            Ok(out) if out.success => in_tree().args(["rev-parse", "HEAD"]).run().map(|s| s.trim().to_string()),
            Ok(_) => {
                let _ = in_tree().args(["rebase", "--abort"]).output();
                Err(Error::refused(ecode_core::ForgeReason::Dirty))
            }
            Err(e) => Err(e),
        };
        let _ = self.git().args(["worktree", "remove", "--force", &dir_arg]).output();
        let _ = std::fs::remove_dir_all(&dir);
        let _ = self.git().args(["worktree", "prune"]).output();
        result
    }

    /// `git bundle create`: `refs` (branch names or full ref names) minus
    /// everything reachable from `basis` (shas the receiver has).
    pub fn bundle_create(&self, path: &Path, refs: &[&str], basis: &[&str]) -> Result<BundleInfo> {
        let mut command = self.git().args(["bundle", "create", "--quiet"]).arg(path.as_os_str());
        for name in refs {
            command = command.arg(if name.starts_with("refs/") { name.to_string() } else { format!("refs/heads/{name}") });
        }
        for sha in basis {
            command = command.arg(format!("^{}", self.commit_id(sha)?));
        }
        command.run()?;
        read_bundle_header(path)
    }

    /// `git bundle verify` against this repository (its prerequisites must
    /// be here), then the bundle's refs.
    pub fn bundle_verify(&self, path: &Path) -> Result<BundleInfo> {
        self.git().args(["bundle", "verify", "--quiet"]).arg(path.as_os_str()).run()?;
        read_bundle_header(path)
    }
}

/// The added/modified/removed classification of model changes.
pub(crate) fn classify(before: &Tree, changes: &Changes) -> Vec<FileChange> {
    changes
        .iter()
        .map(|(path, content)| FileChange {
            path: path.clone(),
            change: match content {
                None => Change::Removed,
                Some(_) if before.contains_key(path) => Change::Modified,
                Some(_) => Change::Added,
            },
        })
        .collect()
}

/// A v2/v3 bundle header: `-<sha>` prerequisites, `<sha> <ref>` lines, a
/// blank line, then the pack.
fn read_bundle_header(path: &Path) -> Result<BundleInfo> {
    use std::io::BufRead;
    let file = std::fs::File::open(path).map_err(|e| Error::git("read bundle", e))?;
    let mut info = BundleInfo { refs: vec![], prerequisites: vec![] };
    let mut lines = std::io::BufReader::new(file).split(b'\n');
    let first = lines.next().transpose().map_err(|e| Error::git("read bundle", e))?.unwrap_or_default();
    if !first.starts_with(b"# v2 git bundle") && !first.starts_with(b"# v3 git bundle") {
        return Err(Error::git("read bundle", "not a git bundle"));
    }
    for line in lines {
        let line = line.map_err(|e| Error::git("read bundle", e))?;
        if line.is_empty() {
            break;
        }
        let line = String::from_utf8_lossy(&line);
        if line.starts_with('@') {
            continue; // v3 capabilities
        }
        if let Some(rest) = line.strip_prefix('-') {
            info.prerequisites.push(rest.split(' ').next().unwrap_or_default().to_string());
        } else if let Some((sha, name)) = line.split_once(' ') {
            info.refs.push((name.to_string(), sha.to_string()));
        }
    }
    Ok(info)
}
