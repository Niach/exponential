//! FEED-63: save a STOPPED run's uncommitted work as a local WIP commit.
//!
//! A run ended by a person (an explicit End, MCP `exponential_sessions_end`)
//! used to leave its worktree dirty and nothing else: the next start or the
//! prune could lose it, and nobody was told. The host now commits whatever
//! the run left behind (tracked changes and untracked files alike) on the
//! run's own branch, so the work survives as history. Never pushed: the
//! commit is a local safety net, the branch owner decides what to do with it.

use std::path::Path;

use crate::git_worktree::run_git;
use crate::prune::{worktree_dirty_state, DirtyState};

/// The message every saved WIP commit carries.
pub const WIP_COMMIT_MESSAGE: &str =
    "WIP: uncommitted changes saved when the session was stopped (Exponential)";

/// NEW-5: the message of the WIP commit [`save_tracked_wip_commit`] makes
/// when a FINISHED session's worktree is cleaned up
/// ([`crate::run_cleanup::finish_worktree`]).
pub const WIP_FINISH_COMMIT_MESSAGE: &str =
    "WIP: uncommitted changes saved when the session ended (Exponential)";

/// Paths per `git add` argv (well under any OS argv limit).
const ADD_CHUNK: usize = 200;

/// The identity a commit falls back to when the worktree has none configured.
const FALLBACK_NAME: &str = "user.name=Exponential";
const FALLBACK_EMAIL: &str = "user.email=noreply@exponential.local";

/// What one [`save_wip_commit`] pass did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WipSave {
    /// No git work tree here (a repo-less scratch dir, or the worktree is
    /// already gone): nothing to save.
    NotARepo,
    /// Nothing uncommitted.
    Clean,
    /// Committed on `branch` (`HEAD` when detached).
    Committed { branch: String },
    /// `git add` or `git commit` failed; the detail is for the log.
    Failed(String),
}

/// Commit everything uncommitted in `worktree` as one WIP commit, with the
/// worktree's own identity, or the Exponential fallback identity when the
/// first attempt fails for a missing one. Hooks and signing are skipped: a
/// save at stop time must not hang on a prompt or be vetoed by a linter.
///
/// #EXP-2039: by the auto-push snapshot's rules
/// ([`crate::auto_push::stop_save_paths`]): a flood of untracked files
/// (build output nobody ignored) and secret-looking or oversized files stay
/// out, uncommitted; the end cleanup later pushes this commit.
pub fn save_wip_commit(worktree: &Path) -> WipSave {
    save(worktree, &["add", "-A"], WIP_COMMIT_MESSAGE)
}

/// NEW-5: [`save_wip_commit`] for TRACKED changes only (`git add -u`), with
/// [`WIP_FINISH_COMMIT_MESSAGE`] — the finished-session cleanup commits real
/// work but never the untracked debris (build artifacts, the launcher's seed
/// files) a removal discards anyway. An untracked-only tree reads `Clean`.
pub fn save_tracked_wip_commit(worktree: &Path) -> WipSave {
    save(worktree, &["add", "-u"], WIP_FINISH_COMMIT_MESSAGE)
}

fn save(worktree: &Path, add: &[&str], message: &str) -> WipSave {
    // A worktree has a `.git` FILE, a clone a `.git` dir; a scratch dir has
    // neither (and may sit inside some unrelated repo, which is not ours).
    if !worktree.join(".git").exists() {
        return WipSave::NotARepo;
    }
    let tracked_only = add.contains(&"-u");
    let nothing_to_save = |state: DirtyState| match state {
        DirtyState::Clean => true,
        DirtyState::UntrackedOnly => tracked_only,
        DirtyState::TrackedChanges => false,
    };
    if nothing_to_save(worktree_dirty_state(worktree)) {
        return WipSave::Clean;
    }
    // FEED-67: a Stop mid-rebase leaves HEAD detached. A commit there lands
    // on no branch, the tree then reads clean and run cleanup removes the
    // worktree — the commit surviving only in the reflog. The DIRTY tree is
    // what cleanup keeps, so leave it be.
    let branch = current_branch(worktree);
    if branch.as_deref() == Some("HEAD") {
        return WipSave::Failed(
            "detached HEAD (a rebase or checkout in progress) — the uncommitted work stays in the worktree"
                .to_string(),
        );
    }
    // #EXP-2059: files still at the state BEFORE `HEAD` are no work; saved,
    // they would be the revert of `HEAD`.
    let stale = match crate::stale_worktree::bring_up_to_head(worktree) {
        Ok(stale) => stale,
        Err(detail) => return WipSave::Failed(detail),
    };
    let mut message = message.to_string();
    if let Some(stale) = &stale {
        log::warn!("WIP save in {}: {}", worktree.display(), stale.note());
        if nothing_to_save(worktree_dirty_state(worktree)) {
            return WipSave::Clean;
        }
        message = format!("{message}\n\n{}\n", stale.note());
    }
    let message = message.as_str();
    if tracked_only {
        if let Err(err) = run_git(Some(worktree), add, None, "git add") {
            return WipSave::Failed(err.detail);
        }
    } else {
        let (paths, skipped) = match crate::auto_push::stop_save_paths(worktree) {
            Ok(selected) => selected,
            Err(detail) => return WipSave::Failed(detail),
        };
        if !skipped.is_empty() {
            log::info!(
                "WIP save in {}: {} path(s) left out (secret-looking, too big or untracked build output)",
                worktree.display(),
                skipped.len()
            );
        }
        if paths.is_empty() {
            return WipSave::Clean;
        }
        for chunk in paths.chunks(ADD_CHUNK) {
            let mut args = vec!["--literal-pathspecs", "add", "-A", "--"];
            args.extend(chunk.iter().map(String::as_str));
            if let Err(err) = run_git(Some(worktree), &args, None, "git add (WIP)") {
                return WipSave::Failed(err.detail);
            }
        }
    }
    let commit = |identity: &[&str]| {
        let mut args: Vec<&str> = identity.to_vec();
        args.extend([
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--no-verify",
            "-m",
            message,
        ]);
        run_git(Some(worktree), &args, None, "git commit (WIP)")
    };
    if let Err(err) = commit(&[]) {
        if !is_missing_identity(&err.detail) {
            return WipSave::Failed(err.detail);
        }
        if let Err(err) = commit(&["-c", FALLBACK_NAME, "-c", FALLBACK_EMAIL]) {
            return WipSave::Failed(err.detail);
        }
    }
    WipSave::Committed {
        branch: branch.unwrap_or_else(|| "HEAD".to_string()),
    }
}

/// `git rev-parse --abbrev-ref HEAD`: the branch name, `HEAD` when detached,
/// `None` when git cannot say (an unborn branch).
fn current_branch(worktree: &Path) -> Option<String> {
    run_git(
        Some(worktree),
        &["rev-parse", "--abbrev-ref", "HEAD"],
        None,
        "git rev-parse",
    )
    .ok()
    .map(|out| out.trim().to_string())
    .filter(|branch| !branch.is_empty())
}

/// Whether a failed `git commit` failed for want of an author/committer
/// identity (unset, empty, or `user.useConfigOnly` with none configured).
fn is_missing_identity(detail: &str) -> bool {
    let detail = detail.to_ascii_lowercase();
    ["tell me who you are", "identity unknown", "empty ident", "unable to auto-detect email", "no name was given"]
        .iter()
        .any(|needle| detail.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-wip-save-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A repo with one commit on `exp/APP-1` and a configured identity.
    fn repo(tag: &str) -> PathBuf {
        let dir = temp_dir(tag);
        git(&dir, &["init", "-q", "--initial-branch=exp/APP-1", "."]);
        git(&dir, &["config", "user.name", "Run Owner"]);
        git(&dir, &["config", "user.email", "owner@example.com"]);
        git(&dir, &["config", "commit.gpgsign", "false"]);
        std::fs::write(dir.join("README.md"), "hello\n").unwrap();
        git(&dir, &["add", "README.md"]);
        git(&dir, &["commit", "-q", "-m", "init"]);
        dir
    }

    #[test]
    fn a_dirty_worktree_is_committed_with_its_own_identity() {
        let dir = repo("dirty");
        std::fs::write(dir.join("README.md"), "changed\n").unwrap();
        std::fs::write(dir.join("new.txt"), "untracked\n").unwrap();
        assert_eq!(
            save_wip_commit(&dir),
            WipSave::Committed { branch: "exp/APP-1".to_string() }
        );
        assert_eq!(worktree_dirty_state(&dir), DirtyState::Clean);
        let log = git(&dir, &["log", "-1", "--format=%s|%an"]);
        assert_eq!(log.trim(), format!("{WIP_COMMIT_MESSAGE}|Run Owner"));
        let files = git(&dir, &["show", "--name-only", "--format=", "HEAD"]);
        assert!(files.contains("README.md") && files.contains("new.txt"), "{files}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn untracked_only_is_saved_too() {
        let dir = repo("untracked");
        std::fs::write(dir.join("notes.md"), "draft\n").unwrap();
        assert!(matches!(save_wip_commit(&dir), WipSave::Committed { .. }));
        assert_eq!(worktree_dirty_state(&dir), DirtyState::Clean);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// #EXP-2039 (`exp/leftover-0119c15e`: 7,681 DerivedData files): a
    /// stop-time save takes the real work and leaves a flood of untracked
    /// build output and secret-looking files out.
    #[test]
    fn a_stop_save_leaves_build_output_floods_and_secrets_out() {
        let dir = repo("flood");
        std::fs::write(dir.join("README.md"), "changed\n").unwrap();
        let derived = dir.join("apps/ios/.dd-exp1958/Build");
        std::fs::create_dir_all(&derived).unwrap();
        for n in 0..=crate::auto_push::MAX_WIP_PATHS {
            std::fs::write(derived.join(format!("obj-{n}.o")), "x").unwrap();
        }
        assert!(matches!(save_wip_commit(&dir), WipSave::Committed { .. }));
        let files = git(&dir, &["show", "--name-only", "--format=", "HEAD"]);
        assert_eq!(files.trim(), "README.md", "only the tracked change: {}", files.lines().count());
        assert!(derived.join("obj-0.o").exists(), "the build output stays on disk, uncommitted");
        let _ = std::fs::remove_dir_all(&dir);

        let dir = repo("secret");
        std::fs::write(dir.join("notes.md"), "draft\n").unwrap();
        std::fs::write(dir.join(".env"), "TOKEN=1\n").unwrap();
        assert!(matches!(save_wip_commit(&dir), WipSave::Committed { .. }));
        let files = git(&dir, &["show", "--name-only", "--format=", "HEAD"]);
        assert_eq!(files.trim(), "notes.md");
        // Only a secret left: nothing to save.
        assert_eq!(save_wip_commit(&dir), WipSave::Clean);
        assert!(dir.join(".env").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_clean_worktree_and_a_scratch_dir_are_left_alone() {
        let dir = repo("clean");
        let head = git(&dir, &["rev-parse", "HEAD"]);
        assert_eq!(save_wip_commit(&dir), WipSave::Clean);
        assert_eq!(git(&dir, &["rev-parse", "HEAD"]), head);
        let _ = std::fs::remove_dir_all(&dir);

        let scratch = temp_dir("scratch");
        std::fs::write(scratch.join("file.txt"), "x\n").unwrap();
        assert_eq!(save_wip_commit(&scratch), WipSave::NotARepo);
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// FEED-67: a detached HEAD (Stop mid-rebase) is refused without a
    /// commit — the tree stays dirty, which is what run cleanup keeps.
    #[test]
    fn a_detached_head_is_refused_and_the_tree_stays_dirty() {
        let dir = repo("detached");
        git(&dir, &["checkout", "-q", "--detach"]);
        let head = git(&dir, &["rev-parse", "HEAD"]);
        std::fs::write(dir.join("README.md"), "mid-rebase\n").unwrap();
        match save_wip_commit(&dir) {
            WipSave::Failed(detail) => assert!(detail.contains("detached HEAD"), "{detail}"),
            other => panic!("expected Failed, got {other:?}"),
        }
        assert_eq!(worktree_dirty_state(&dir), DirtyState::TrackedChanges);
        assert_eq!(git(&dir, &["rev-parse", "HEAD"]), head);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// NEW-5: the finish variant commits tracked changes only; untracked
    /// debris stays untracked, and untracked-only reads clean.
    #[test]
    fn the_tracked_variant_leaves_untracked_debris_alone() {
        let dir = repo("tracked-only");
        std::fs::write(dir.join("debris.log"), "noise\n").unwrap();
        assert_eq!(save_tracked_wip_commit(&dir), WipSave::Clean);
        std::fs::write(dir.join("README.md"), "changed\n").unwrap();
        assert_eq!(
            save_tracked_wip_commit(&dir),
            WipSave::Committed { branch: "exp/APP-1".to_string() }
        );
        assert_eq!(worktree_dirty_state(&dir), DirtyState::UntrackedOnly);
        let log = git(&dir, &["log", "-1", "--format=%s"]);
        assert_eq!(log.trim(), WIP_FINISH_COMMIT_MESSAGE);
        let files = git(&dir, &["show", "--name-only", "--format=", "HEAD"]);
        assert!(files.contains("README.md") && !files.contains("debris.log"), "{files}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// #EXP-2059: `exp/APP-1` gets a merge of `main` made in ANOTHER
    /// checkout of it (`worktree add -f`), so the returned worktree's files
    /// and index stay at the pre-merge commit while its `HEAD` is the merge.
    /// `main` rewrote line 1 of `a.txt` and `README.md`, added `new.txt`
    /// and deleted `gone.txt`; the branch's own commit added `own.txt`.
    fn merged_under(tag: &str) -> (PathBuf, PathBuf) {
        let dir = repo(tag);
        std::fs::write(dir.join("a.txt"), "one\ntwo\nthree\nfour\nfive\n").unwrap();
        std::fs::write(dir.join("gone.txt"), "old\n").unwrap();
        git(&dir, &["add", "a.txt", "gone.txt"]);
        git(&dir, &["commit", "-q", "-m", "files"]);
        git(&dir, &["branch", "main"]);
        let side = temp_dir(&format!("{tag}-side"));
        let main = side.join("main");
        git(&dir, &["worktree", "add", "-q", main.to_str().unwrap(), "main"]);
        std::fs::write(main.join("a.txt"), "ONE\ntwo\nthree\nfour\nfive\n").unwrap();
        std::fs::write(main.join("README.md"), "hello from main\n").unwrap();
        std::fs::write(main.join("new.txt"), "new\n").unwrap();
        git(&main, &["rm", "-q", "gone.txt"]);
        git(&main, &["add", "-A"]);
        git(&main, &["commit", "-q", "-m", "main moves on"]);
        std::fs::write(dir.join("own.txt"), "own\n").unwrap();
        git(&dir, &["add", "own.txt"]);
        git(&dir, &["commit", "-q", "-m", "own work"]);
        let twin = side.join("twin");
        git(&dir, &["worktree", "add", "-q", "-f", twin.to_str().unwrap(), "exp/APP-1"]);
        git(&twin, &["merge", "-q", "--no-ff", "-m", "Merge main into exp/APP-1", "main"]);
        (dir, side)
    }

    /// #EXP-2059: a worktree still at the pre-merge state is no work: no
    /// WIP commit (it would be the merge's inverse), the files come up to
    /// `HEAD`. Whether the index stayed behind too or already is `HEAD`'s.
    #[test]
    fn the_pure_inverse_of_a_merge_is_never_saved() {
        for index_at_head in [false, true] {
            let (dir, side) = merged_under(if index_at_head { "inverse-index" } else { "inverse" });
            if index_at_head {
                git(&dir, &["reset", "-q"]);
            }
            let head = git(&dir, &["rev-parse", "HEAD"]);
            assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "one\ntwo\nthree\nfour\nfive\n");
            assert_eq!(save_tracked_wip_commit(&dir), WipSave::Clean, "index at HEAD: {index_at_head}");
            assert_eq!(git(&dir, &["rev-parse", "HEAD"]), head);
            assert_eq!(worktree_dirty_state(&dir), DirtyState::Clean);
            assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "ONE\ntwo\nthree\nfour\nfive\n");
            assert_eq!(std::fs::read_to_string(dir.join("README.md")).unwrap(), "hello from main\n");
            assert!(dir.join("new.txt").exists() && !dir.join("gone.txt").exists());
            let _ = std::fs::remove_dir_all(&dir);
            let _ = std::fs::remove_dir_all(&side);
        }
    }

    /// #EXP-2059: real edits in a pre-merge worktree are saved ON the
    /// merge: one outside what it changed, one in a file it changed too.
    /// Nothing of the merge is reverted. The stop-time save does the same.
    #[test]
    fn real_edits_in_a_pre_merge_worktree_survive_and_revert_nothing() {
        for stop in [false, true] {
            let (dir, side) = merged_under(if stop { "edits-stop" } else { "edits" });
            let merge = git(&dir, &["rev-parse", "HEAD"]);
            std::fs::write(dir.join("own.txt"), "own, edited\n").unwrap();
            std::fs::write(dir.join("a.txt"), "one\ntwo\nthree\nfour\nFIVE\n").unwrap();
            let saved = if stop { save_wip_commit(&dir) } else { save_tracked_wip_commit(&dir) };
            assert_eq!(saved, WipSave::Committed { branch: "exp/APP-1".to_string() });
            assert_eq!(git(&dir, &["rev-parse", "HEAD~1"]), merge);
            assert_eq!(worktree_dirty_state(&dir), DirtyState::Clean);
            let files = git(&dir, &["show", "--name-only", "--format=", "HEAD"]);
            assert_eq!(files.lines().collect::<Vec<_>>(), ["a.txt", "own.txt"]);
            assert_eq!(git(&dir, &["show", "HEAD:a.txt"]), "ONE\ntwo\nthree\nfour\nFIVE\n");
            assert_eq!(git(&dir, &["show", "HEAD:own.txt"]), "own, edited\n");
            assert_eq!(git(&dir, &["show", "HEAD:README.md"]), "hello from main\n");
            assert_eq!(git(&dir, &["show", "HEAD:new.txt"]), "new\n");
            let body = git(&dir, &["log", "-1", "--format=%b"]);
            assert!(body.contains("Not saved: 3 path(s)") && body.contains("1 edited path(s)"), "{body}");
            let _ = std::fs::remove_dir_all(&dir);
            let _ = std::fs::remove_dir_all(&side);
        }
    }

    /// #EXP-2059: an edit that collides with the merge is neither saved as
    /// a revert nor dropped: the save is refused, the worktree untouched.
    #[test]
    fn an_edit_that_collides_with_the_merge_keeps_the_worktree() {
        let (dir, side) = merged_under("collide");
        let merge = git(&dir, &["rev-parse", "HEAD"]);
        std::fs::write(dir.join("a.txt"), "uno\ntwo\nthree\nfour\nfive\n").unwrap();
        match save_tracked_wip_commit(&dir) {
            WipSave::Failed(detail) => assert!(detail.contains("stays in the worktree"), "{detail}"),
            other => panic!("expected Failed, got {other:?}"),
        }
        assert_eq!(git(&dir, &["rev-parse", "HEAD"]), merge);
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "uno\ntwo\nthree\nfour\nfive\n");
        assert_eq!(std::fs::read_to_string(dir.join("README.md")).unwrap(), "hello\n");
        assert!(dir.join("gone.txt").exists() && !dir.join("new.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&side);
    }

    /// #EXP-2059: a fast-forward that reached the branch from another
    /// checkout (one commit, then several: `@{1}` names where it stood).
    #[test]
    fn the_pure_inverse_of_a_fast_forward_is_never_saved() {
        let dir = repo("ff");
        let side = temp_dir("ff-side");
        let twin = side.join("twin");
        git(&dir, &["worktree", "add", "-q", "-f", twin.to_str().unwrap(), "exp/APP-1"]);
        std::fs::write(twin.join("README.md"), "second\n").unwrap();
        git(&twin, &["commit", "-q", "-am", "second"]);
        assert_eq!(worktree_dirty_state(&dir), DirtyState::TrackedChanges);
        assert_eq!(save_tracked_wip_commit(&dir), WipSave::Clean);
        assert_eq!(std::fs::read_to_string(dir.join("README.md")).unwrap(), "second\n");
        // Two commits in one move, and an edit of this worktree's own.
        git(&twin, &["checkout", "-q", "--detach"]);
        std::fs::write(twin.join("README.md"), "third\n").unwrap();
        git(&twin, &["commit", "-q", "-am", "third"]);
        std::fs::write(twin.join("b.txt"), "b\n").unwrap();
        git(&twin, &["add", "b.txt"]);
        git(&twin, &["commit", "-q", "-m", "fourth"]);
        let tip = git(&twin, &["rev-parse", "HEAD"]);
        git(&twin, &["update-ref", "-m", "moved from elsewhere", "refs/heads/exp/APP-1", tip.trim()]);
        std::fs::write(dir.join("own.txt"), "own\n").unwrap();
        git(&dir, &["add", "-N", "own.txt"]);
        assert!(matches!(save_tracked_wip_commit(&dir), WipSave::Committed { .. }));
        assert_eq!(git(&dir, &["rev-parse", "HEAD~1"]), tip);
        let files = git(&dir, &["show", "--name-only", "--format=", "HEAD"]);
        assert_eq!(files.trim(), "own.txt");
        assert_eq!(git(&dir, &["show", "HEAD:README.md"]), "third\n");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&side);
    }

    /// #EXP-2059: ordinary work is parked as before: in the very files the
    /// last commit changed, and an undo of the whole last commit made HERE
    /// (index at `HEAD`) is work too.
    #[test]
    fn ordinary_work_and_an_own_undo_are_parked_as_before() {
        let dir = repo("ordinary");
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        std::fs::write(dir.join("README.md"), "second\n").unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", "second"]);
        std::fs::write(dir.join("README.md"), "third\n").unwrap();
        assert!(matches!(save_tracked_wip_commit(&dir), WipSave::Committed { .. }));
        assert_eq!(git(&dir, &["show", "HEAD:README.md"]), "third\n");
        assert_eq!(git(&dir, &["log", "-1", "--format=%b"]).trim(), "");
        std::fs::write(dir.join("README.md"), "fourth\n").unwrap();
        git(&dir, &["commit", "-q", "-am", "fourth"]);
        std::fs::write(dir.join("README.md"), "third\n").unwrap();
        assert!(matches!(save_tracked_wip_commit(&dir), WipSave::Committed { .. }));
        assert_eq!(git(&dir, &["show", "HEAD:README.md"]), "third\n");
        assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), WIP_FINISH_COMMIT_MESSAGE);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_identity_falls_back_to_exponential() {
        let dir = repo("no-identity");
        // An EMPTY local name outranks any global one, so the first commit
        // fails for the identity on every machine; `-c` outranks it again.
        git(&dir, &["config", "user.name", ""]);
        std::fs::write(dir.join("README.md"), "changed\n").unwrap();
        assert!(matches!(save_wip_commit(&dir), WipSave::Committed { .. }));
        let log = git(&dir, &["log", "-1", "--format=%an <%ae>"]);
        assert_eq!(log.trim(), "Exponential <noreply@exponential.local>");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
