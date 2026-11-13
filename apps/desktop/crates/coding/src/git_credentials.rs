//! Ambient git auth for launcher-managed clones (EXP-73) — the JIT
//! installation token no longer rides `remote.origin.url`.
//!
//! The old scheme (`git remote set-url origin
//! https://x-access-token:<token>@github.com/…`) had two writers of ONE piece
//! of shared repo-level config — the mid-session refresher and the git-bar
//! sync worker — and whichever wrote last won. During the Release 1 run the
//! git-bar re-asserted a stale cached token mid-run, breaking `git push` for
//! the orchestrator and all eight subagent worktrees at once (and silently
//! reverting a manual `git remote set-url` fix within 120 s).
//!
//! The replacement keeps `origin` at the BARE URL forever and supplies the
//! token through a repo-local credential helper scoped to EXACTLY this repo's
//! URL (path-matched via `useHttpPath`):
//!
//! ```text
//! [credential "https://<host>"]
//!     useHttpPath = true
//! [credential "https://<host>/<path>.git"]
//!     helper =                                       # reset inherited helpers
//!     helper = !f() { test "$1" = get && cat '<clone>/.git/exp-git-credentials' 2>/dev/null; :; }; f
//! ```
//!
//! * The empty first entry resets git's helper list — deliberate, not
//!   defensive: helpers run system → global → local, so a user's global
//!   keychain (their personal PAT) would otherwise answer BEFORE ours and
//!   win. Inside a launcher-managed clone the App token must deterministically
//!   be the credential for THIS repo (a member without personal push rights
//!   has nothing else; and personal-PAT pushes would bypass the App's repo
//!   scoping).
//! * The reset is PATH-SCOPED so it only claims this repo's URL: the JIT
//!   token can't reach any other repo anyway, and a Claude child touching a
//!   different github.com remote from inside a worktree (a fork, a private
//!   submodule) must keep falling through to the user's own helpers instead
//!   of being fed a 403-ing token. `useHttpPath` is what makes git consider
//!   the path when matching the scoped subsection.
//! * The helper is READ-ONLY on purpose — `git credential-store` was rejected
//!   because git broadcasts `erase` to helpers after any 401 and the store
//!   helper would delete our line, leaving the next `git push` inside a
//!   Claude PTY facing an interactive credential prompt. This helper answers
//!   `get` and ignores `store`/`erase` (always exiting 0); only [`ensure`]
//!   ever writes the file.
//! * Config + credential file live in the clone's shared `.git`, so every
//!   linked worktree AND every spawned `claude` child gets working auth from
//!   one write — the same sharing that made the old clobber catastrophic now
//!   works for us, because writes are downgrade-guarded ([`ensure`] step 3).
//! * Helper values run through `sh` (git-for-windows ships one); the path is
//!   SINGLE-quoted so spaces (macOS `Application Support`) survive and `$`,
//!   backticks, and double quotes are never shell-expanded.
//!
//! Concurrency: the refresher and the git-bar worker may still race, but every
//! step of [`ensure`] is individually safe — `set-url`/`config` take git's own
//! config lock, the credential write is an atomic rename, and the
//! no-downgrade guard makes writer ORDER irrelevant (a token with an earlier
//! real expiry never overwrites a later one). Worst case is a transiently
//! mismatched file/sidecar pair, corrected by the next writer.
//!
//! Freshness (EXP-447): the helper is DELIBERATELY dumb — it never checks the
//! sidecar expiry or re-mints on `get`. The helper line is persisted per-clone
//! git config shared by both apps and every session, so a smarter helper
//! would have to bake in either the `exponential` binary's path (not
//! guaranteed present for desktop-only installs, moves across updates) or a
//! loopback port (stale after every app restart), and it must survive
//! git-for-windows' `sh`. Keeping tokens alive is instead owned by the
//! proactive per-clone refreshers — `ui::coding_flow::TokenRefreshers`
//! (desktop) and [`crate::token_refresh_host`] (CLI/daemon) — plus the
//! per-op [`ensure_repo_auth`] writers (trunk sync, clone reuse).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use api::error::ApiError;
use api::trpc::TrpcClient;

use crate::clone_manager::{parse_iso8601_utc, TOKEN_REMINT_MARGIN};
use crate::git_worktree::{run_git, GitError, TokenUrl};
use crate::launcher::CodingError;
use crate::token_cache::{token_cache, MintedToken};

/// The credential file inside the clone's shared git dir — resolved by every
/// worktree and every child process through the repo-local helper config.
/// Holds the CURRENT token in git-credential protocol form
/// (`username=…\npassword=…\n`, 0600 on unix), rewritten atomically on every
/// refresh; the helper simply `cat`s it.
pub fn credential_file(clone: &Path) -> PathBuf {
    clone.join(".git").join("exp-git-credentials")
}

///`<credential_file>.expires-at` — the ISO-8601 expiry of the token currently
/// in the file (no secret material). The no-downgrade guard compares against
/// it; kept OUTSIDE the credential file so that file stays exactly the two
/// protocol lines the helper emits.
fn expiry_sidecar(clone: &Path) -> PathBuf {
    clone.join(".git").join("exp-git-credentials.expires-at")
}

/// Install/refresh ambient auth on the shared clone. Idempotent; safe to call
/// from every writer (launcher prepare, refresher, git-bar sync) — the
/// no-downgrade guard makes concurrent callers commutative.
///
/// 1. `git remote set-url origin <bare url>` when the current URL differs —
///    heals a token-embedded origin from the pre-EXP-73 scheme AND any manual
///    edit. Read-first so the steady state is write-free: `git config`
///    writers fail fast on a held `config.lock` (no wait), and this runs from
///    several writers (launcher, refresher, git-bar every 120 s / on focus).
/// 2. Ensure the repo-local, repo-URL-scoped helper pair (reset + ours) and
///    the `useHttpPath` switch — also read-first.
/// 3. Write the credential file + expiry sidecar, UNLESS the file still holds
///    a token with a later-or-equal (or unknown-but-unexpired) real expiry —
///    never downgrade. A file with no `password=` line always rewrites — a
///    fresh-looking sidecar must never shield a deleted/truncated file.
pub fn ensure(
    clone: &Path,
    url: &TokenUrl,
    expires_at: Option<&str>,
) -> Result<(), GitError> {
    // Lock-free read; the output is compared and dropped, never logged (a
    // pre-migration URL can still embed a dead token).
    let current = run_git(
        Some(clone),
        &["config", "--local", "--get", "remote.origin.url"],
        Some(url),
        "git config --get remote.origin.url",
    )
    .map(|out| out.trim().to_string())
    .unwrap_or_default();
    if current != url.bare() {
        run_git(
            Some(clone),
            &["remote", "set-url", "origin", &url.bare()],
            Some(url),
            "git remote set-url origin",
        )?;
    }
    ensure_helper_config(clone, url)?;
    write_credential(clone, url, expires_at)
}

/// One-stop working auth for a network op: cached-or-fresh token mint +
/// [`ensure`] on the clone. Every call site that is about to run a transport
/// op against an EXISTING clone goes through here (git-bar sync worker,
/// Commit & Push) — the margin is the per-op [`TOKEN_REMINT_MARGIN`].
pub fn ensure_repo_auth(
    trpc: &TrpcClient,
    repository_id: &str,
    clone: &Path,
) -> Result<MintedToken, CodingError> {
    ensure_repo_auth_with_margin(trpc, repository_id, clone, TOKEN_REMINT_MARGIN)
}

/// [`ensure_repo_auth`] with an explicit freshness margin — the refresher
/// passes its longer lead so a token it installs always outlives the gap to
/// the next scheduled refresh.
pub fn ensure_repo_auth_with_margin(
    trpc: &TrpcClient,
    repository_id: &str,
    clone: &Path,
    margin: Duration,
) -> Result<MintedToken, CodingError> {
    let minted = token_cache()
        // Repo-level: these are maintenance ops on an EXISTING clone (git-bar
        // sync, Commit & Push, the refresher) — only the token matters, and
        // the entry's branch is nobody's base (EXP-712).
        .get_or_mint_with_margin(trpc, repository_id, None, margin)
        .map_err(|err: ApiError| CodingError::Api(err))?;
    ensure(clone, &minted.url, minted.expires_at.as_deref())?;
    Ok(minted)
}

/// The exact helper list the repo-local config must hold for THIS repo's URL:
/// the reset entry, then the read-only cat helper. The file path is absolute
/// with forward slashes and single-quoted — `!`-prefixed helper values run
/// through `sh`, and single quotes survive spaces AND suppress `$`/backtick/
/// double-quote expansion (a repos root is user-configurable). The trailing
/// `:` pins exit 0 for the ignored `store`/`erase` actions (and a missing
/// file), so git never warns about a failing helper.
fn desired_helpers(clone: &Path) -> Vec<String> {
    let file = credential_file(clone);
    let path = file.to_string_lossy().replace('\\', "/");
    let quoted = format!("'{}'", path.replace('\'', "'\\''"));
    vec![
        String::new(),
        format!("!f() {{ test \"$1\" = get && cat {quoted} 2>/dev/null; :; }}; f"),
    ]
}

/// The scoped helper key: `credential.https://<host>/<path>.git.helper`
/// (eCode's `credential_helper_key`, `remote.json`). Path-scoped (not
/// host-scoped) so the reset entry only suppresses inherited helpers for
/// THIS repo — see the module doc.
fn helper_key(url: &TokenUrl) -> String {
    ecode::core::credential_helper_key(&url.bare()).unwrap_or_else(|| format!("credential.{}.helper", url.bare()))
}

/// `credential.https://<host>.useHttpPath`: makes git consider the URL path
/// when matching the scoped subsection above, on the remote's own host.
fn use_http_path_key(url: &TokenUrl) -> String {
    url.credentials().use_http_path_key().unwrap_or_else(|| "credential.https://github.com.useHttpPath".into())
}

fn ensure_helper_config(clone: &Path, url: &TokenUrl) -> Result<(), GitError> {
    let key = helper_key(url);
    let desired = desired_helpers(clone);
    // A missing key exits 1 → Err → treat as unconfigured (the write below
    // fails loudly if the repo itself is broken). Reads are lock-free.
    let read_all = |k: &str| {
        run_git(
            Some(clone),
            &["config", "--local", "--get-all", k],
            Some(url),
            "git config --get-all",
        )
        .map(|out| {
            out.lines()
                .map(|line| line.trim_end().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
    };
    let use_http_path = use_http_path_key(url);
    if read_all(&use_http_path) != ["true"] {
        run_git(
            Some(clone),
            &["config", "--local", &use_http_path, "true"],
            Some(url),
            "git config credential.useHttpPath",
        )?;
    }
    if read_all(&key) == desired {
        return Ok(());
    }
    // --replace-all collapses whatever is there to the single reset entry;
    // --add appends ours. Both take git's own config lock.
    run_git(
        Some(clone),
        &["config", "--local", "--replace-all", &key, ""],
        Some(url),
        "git config --replace-all credential helper",
    )?;
    run_git(
        Some(clone),
        &["config", "--local", "--add", &key, &desired[1]],
        Some(url),
        "git config --add credential helper",
    )?;
    Ok(())
}

/// Step 3: the downgrade-guarded credential write (see [`ensure`]).
fn write_credential(
    clone: &Path,
    url: &TokenUrl,
    expires_at: Option<&str>,
) -> Result<(), GitError> {
    let file = credential_file(clone);
    let sidecar = expiry_sidecar(clone);

    let new_expiry = expires_at.and_then(parse_iso8601_utc);
    let stored_expiry = std::fs::read_to_string(&sidecar)
        .ok()
        .and_then(|raw| parse_iso8601_utc(raw.trim()));
    let keep_stored = match (new_expiry, stored_expiry) {
        // Never downgrade a fresher (or equally fresh) token.
        (Some(new), Some(stored)) => stored >= new,
        // A token of UNKNOWN freshness must not clobber one that is provably
        // still alive — an absent expiry bypassing the guard would re-open
        // the exact stale-over-fresh overwrite this module exists to prevent.
        // A stored-but-expired token loses to it, though: the unknown one was
        // at least just minted.
        (None, Some(stored)) => stored > std::time::SystemTime::now(),
        _ => false,
    };
    if keep_stored && file_has_credential(&file) {
        return Ok(());
    }

    let io_err = |what: &str, e: std::io::Error| GitError {
        op: "install git credentials".to_string(),
        detail: format!("{what}: {e}"),
    };
    let git_dir = clone.join(".git");
    if !git_dir.is_dir() {
        return Err(GitError {
            op: "install git credentials".to_string(),
            detail: format!("{} is not a git clone", clone.display()),
        });
    }
    write_atomic(&git_dir, &file, url.credential_file_contents().as_bytes())
        .map_err(|e| io_err("write credentials", e))?;
    // Sidecar AFTER the file: a crash between the two leaves the sidecar
    // older than the file — the next writer just rewrites (safe), whereas the
    // reverse order could make a fresh sidecar shield a stale file.
    match expires_at {
        Some(raw) => write_atomic(&git_dir, &sidecar, raw.as_bytes())
            .map_err(|e| io_err("write expiry", e))?,
        None => {
            let _ = std::fs::remove_file(&sidecar);
        }
    }
    Ok(())
}

/// Prefix of [`write_atomic`]'s temp files (`<name>.tmp-<pid>-<seq>-<nanos>`).
const TMP_MARKER: &str = ".tmp-";

/// A temp file older than this is a crashed writer's leftover, swept by the
/// next [`write_atomic`] (a live writer holds its own for milliseconds).
const STALE_TMP_AGE: Duration = Duration::from_secs(10 * 60);

/// Process-wide sequence: two writers of ONE process (several sessions
/// resumed at once, FEED-183) never pick the same temp name, whatever the
/// clock's resolution (macOS: microseconds).
static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// #FEED-183: replace `target` (in `dir`) with `contents` atomically and
/// idempotently: written to a fresh owner-only (0600) temp file, then
/// renamed over whatever is there (a leftover from an interrupted run
/// included). Every writer has its own temp name (pid + process sequence +
/// clock); a name that still exists (a crashed run's leftover, a recycled
/// pid) is skipped for the next one, never an error. Concurrent writers each
/// land a complete file; the last rename wins.
fn write_atomic(dir: &Path, target: &Path, contents: &[u8]) -> std::io::Result<()> {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    sweep_stale_tmps(dir, &name);
    let mut last = None;
    for _ in 0..16 {
        let seq = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = dir.join(format!("{name}{TMP_MARKER}{}-{seq}-{nanos}", std::process::id()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600); // token material — owner-only from birth
        }
        let mut handle = match options.open(&tmp) {
            Ok(handle) => handle,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                last = Some(e);
                continue;
            }
            Err(e) => return Err(e),
        };
        let written = handle.write_all(contents).and_then(|()| handle.sync_all());
        drop(handle);
        let landed = written.and_then(|()| std::fs::rename(&tmp, target));
        if landed.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        return landed;
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("no free temp name")))
}

/// Best-effort: remove `<name>.tmp-*` files in `dir` older than
/// [`STALE_TMP_AGE`] — what an interrupted writer left behind.
fn sweep_stale_tmps(dir: &Path, name: &str) {
    let prefix = format!("{name}{TMP_MARKER}");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(&prefix) {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > STALE_TMP_AGE);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn file_has_credential(file: &Path) -> bool {
    std::fs::read_to_string(file)
        .map(|content| content.lines().any(|line| line.starts_with("password=")))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::{Command, Stdio};

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(tag: &str) -> TempDir {
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("exp-git-creds-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// A local origin cloned to `<dir>/clone`, with the PRE-EXP-73 token URL
    /// embedded as origin — the migration/healing starting state.
    fn seed_clone_with_token_remote(dir: &Path) -> PathBuf {
        let origin = dir.join("origin-src");
        fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]);
        fs::write(origin.join("README.md"), "seed\n").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "--quiet", "-m", "seed"]);
        let clone = dir.join("clone");
        git(
            dir,
            &["clone", "--quiet", origin.to_str().unwrap(), clone.to_str().unwrap()],
        );
        git(
            &clone,
            &[
                "remote",
                "set-url",
                "origin",
                "https://x-access-token:ghs_old_dead@github.com/acme/web.git",
            ],
        );
        clone
    }

    fn origin_url(clone: &Path) -> String {
        let out = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(clone)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn ensure_heals_a_token_embedded_origin_to_the_bare_url() {
        let dir = temp_dir("heal");
        let clone = seed_clone_with_token_remote(&dir.0);
        let url = TokenUrl::new("acme/web", "ghs_fresh456");

        ensure(&clone, &url, Some("2099-01-01T00:00:00.000Z")).unwrap();

        assert_eq!(origin_url(&clone), "https://github.com/acme/web.git");
    }

    #[test]
    fn ensure_writes_the_exact_helper_pair_and_is_idempotent() {
        let dir = temp_dir("helpers");
        let clone = seed_clone_with_token_remote(&dir.0);
        let url = TokenUrl::new("acme/web", "ghs_fresh456");

        ensure(&clone, &url, Some("2099-01-01T00:00:00.000Z")).unwrap();
        ensure(&clone, &url, Some("2099-01-01T00:00:00.000Z")).unwrap();

        let out = Command::new("git")
            .args(["config", "--local", "--get-all", &helper_key(&url)])
            .current_dir(&clone)
            .output()
            .unwrap();
        let lines: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect();
        assert_eq!(lines, desired_helpers(&clone), "no duplicates, reset first");

        let use_path = Command::new("git")
            .args(["config", "--local", "--get", "credential.https://github.com.useHttpPath"])
            .current_dir(&clone)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&use_path.stdout).trim(), "true");
    }

    #[test]
    fn credential_file_holds_the_token_with_private_perms_and_sidecar() {
        let dir = temp_dir("file");
        let clone = seed_clone_with_token_remote(&dir.0);
        let url = TokenUrl::new("acme/web", "ghs_fresh456");

        ensure(&clone, &url, Some("2099-01-01T00:00:00.000Z")).unwrap();

        let content = fs::read_to_string(credential_file(&clone)).unwrap();
        assert_eq!(content, "username=x-access-token\npassword=ghs_fresh456\n");
        assert_eq!(
            fs::read_to_string(expiry_sidecar(&clone)).unwrap(),
            "2099-01-01T00:00:00.000Z"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(credential_file(&clone))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "token material must be owner-only");
        }
    }

    #[test]
    fn a_staler_token_never_overwrites_a_fresher_one() {
        let dir = temp_dir("downgrade");
        let clone = seed_clone_with_token_remote(&dir.0);

        ensure(
            &clone,
            &TokenUrl::new("acme/web", "ghs_fresh"),
            Some("2099-06-01T00:00:00.000Z"),
        )
        .unwrap();
        // The postmortem failure: a writer holding an OLDER token re-asserts
        // it — the file must stay byte-identical.
        ensure(
            &clone,
            &TokenUrl::new("acme/web", "ghs_stale"),
            Some("2099-01-01T00:00:00.000Z"),
        )
        .unwrap();
        let content = fs::read_to_string(credential_file(&clone)).unwrap();
        assert!(content.contains("password=ghs_fresh"), "downgraded: {content}");

        // A genuinely newer token replaces.
        ensure(
            &clone,
            &TokenUrl::new("acme/web", "ghs_newer"),
            Some("2099-12-01T00:00:00.000Z"),
        )
        .unwrap();
        let content = fs::read_to_string(credential_file(&clone)).unwrap();
        assert!(content.contains("password=ghs_newer"), "not upgraded: {content}");
    }

    #[test]
    fn an_unknown_expiry_never_clobbers_a_live_token_but_replaces_a_dead_one() {
        let dir = temp_dir("unknown-expiry");
        let clone = seed_clone_with_token_remote(&dir.0);

        ensure(
            &clone,
            &TokenUrl::new("acme/web", "ghs_live"),
            Some("2099-06-01T00:00:00.000Z"),
        )
        .unwrap();
        // No expiry at all must not bypass the guard — that would re-open the
        // stale-over-fresh overwrite.
        ensure(&clone, &TokenUrl::new("acme/web", "ghs_mystery"), None).unwrap();
        let content = fs::read_to_string(credential_file(&clone)).unwrap();
        assert!(content.contains("password=ghs_live"), "clobbered: {content}");

        // …but a stored token that is provably EXPIRED loses to a fresh mint
        // of unknown expiry.
        fs::write(expiry_sidecar(&clone), "2020-01-01T00:00:00.000Z").unwrap();
        ensure(&clone, &TokenUrl::new("acme/web", "ghs_mystery"), None).unwrap();
        let content = fs::read_to_string(credential_file(&clone)).unwrap();
        assert!(content.contains("password=ghs_mystery"), "not replaced: {content}");
        assert!(!expiry_sidecar(&clone).exists(), "stale sidecar must go");
    }

    #[test]
    fn an_erased_credential_file_is_rewritten_despite_a_fresh_sidecar() {
        let dir = temp_dir("erase");
        let clone = seed_clone_with_token_remote(&dir.0);
        let url = TokenUrl::new("acme/web", "ghs_fresh456");
        ensure(&clone, &url, Some("2099-06-01T00:00:00.000Z")).unwrap();

        // A truncated/emptied credential file (crash mid-write, manual
        // cleanup) while the sidecar still claims a fresh token — recovery
        // must rewrite anyway (same token, same expiry → the expiry guard
        // alone would skip).
        fs::write(credential_file(&clone), "").unwrap();
        ensure(&clone, &url, Some("2099-06-01T00:00:00.000Z")).unwrap();

        let content = fs::read_to_string(credential_file(&clone)).unwrap();
        assert!(content.contains("password=ghs_fresh456"), "not restored: {content}");
    }

    /// #FEED-183: an interrupted install leaves a credential file (wrong
    /// perms, half-written) and temp files behind; the resume's install
    /// replaces the file atomically (0600), sweeps the stale temps and
    /// never fails on them.
    #[test]
    fn a_resume_after_an_interrupted_install_succeeds_over_its_leftovers() {
        let dir = temp_dir("leftover");
        let clone = seed_clone_with_token_remote(&dir.0);
        let git_dir = clone.join(".git");
        fs::write(credential_file(&clone), "username=x-access-token\npass").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(credential_file(&clone), fs::Permissions::from_mode(0o644)).unwrap();
        }
        let stale_tmps = [
            git_dir.join(format!("exp-git-credentials.tmp-{}-1", std::process::id())),
            git_dir.join(format!("exp-git-credentials.tmp-{}-0-1", std::process::id())),
            git_dir.join("exp-git-credentials.expires-at.tmp-1-0-1"),
        ];
        let old = std::time::SystemTime::now() - Duration::from_secs(3600);
        for tmp in &stale_tmps {
            fs::write(tmp, "password=ghs_half").unwrap();
            fs::File::options().write(true).open(tmp).unwrap().set_modified(old).unwrap();
        }
        // A concurrent writer's fresh temp is never touched.
        let live_tmp = git_dir.join("exp-git-credentials.tmp-1-0-2");
        fs::write(&live_tmp, "x").unwrap();

        let url = TokenUrl::new("acme/web", "ghs_resumed");
        ensure(&clone, &url, Some("2099-01-01T00:00:00.000Z")).unwrap();
        // Idempotent: again, same token, and once more with a newer one.
        ensure(&clone, &url, Some("2099-01-01T00:00:00.000Z")).unwrap();
        ensure(&clone, &TokenUrl::new("acme/web", "ghs_newer"), Some("2099-02-01T00:00:00.000Z")).unwrap();

        let content = fs::read_to_string(credential_file(&clone)).unwrap();
        assert_eq!(content, "username=x-access-token\npassword=ghs_newer\n");
        assert_eq!(fs::read_to_string(expiry_sidecar(&clone)).unwrap(), "2099-02-01T00:00:00.000Z");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(credential_file(&clone)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "a replaced leftover is owner-only");
        }
        for tmp in &stale_tmps {
            assert!(!tmp.exists(), "stale temp not swept: {}", tmp.display());
        }
        assert!(live_tmp.exists(), "a live writer's temp was swept");
    }

    /// #FEED-183: several sessions of one process resuming at once (the
    /// reporter's three interrupted children) install the same clone's
    /// credentials concurrently: every install succeeds, the file is one
    /// complete token, and no temp file is left behind.
    #[test]
    fn concurrent_installs_on_one_clone_all_succeed() {
        let dir = temp_dir("concurrent");
        let clone = seed_clone_with_token_remote(&dir.0);
        let git_dir = clone.join(".git");
        let results: Vec<Result<(), GitError>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..16)
                .map(|i| {
                    let clone = &clone;
                    scope.spawn(move || {
                        write_credential(
                            clone,
                            &TokenUrl::new("acme/web", &format!("ghs_tok{i}")),
                            Some(&format!("2099-01-{:02}T00:00:00.000Z", i + 1)),
                        )
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        for result in results {
            result.unwrap();
        }
        let content = fs::read_to_string(credential_file(&clone)).unwrap();
        assert!(
            content.starts_with("username=x-access-token\npassword=ghs_tok") && content.ends_with('\n'),
            "{content}"
        );
        let leftovers: Vec<_> = fs::read_dir(&git_dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(TMP_MARKER))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    /// `git credential fill` for `url` run inside `cwd`, with `home` as the
    /// fake $HOME (so a test-controlled "global" ~/.gitconfig applies and the
    /// real user's never does). Returns stdout.
    fn credential_fill(cwd: &Path, home: &Path, url: &str) -> String {
        let mut child = Command::new("git")
            .args(["credential", "fill"])
            .current_dir(cwd)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("HOME", home)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "true") // a helperless miss answers empty, not a prompt
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("url={url}\n\n").as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "credential fill failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// The end-to-end proof: `git credential fill` run from a WORKTREE of a
    /// clone under a path with a SPACE and a `$` resolves our helper and
    /// yields the token — single-quoting, sh invocation, `useHttpPath`
    /// matching, and shared-`.git` config in one. This is the hermetic
    /// stand-in for "the spawned claude's `git push` authenticates".
    #[test]
    fn credential_fill_resolves_from_a_worktree_under_a_hostile_path() {
        let base = temp_dir("fill");
        let dir = base.0.join("dir with $pace");
        fs::create_dir_all(&dir).unwrap();
        let clone = seed_clone_with_token_remote(&dir);
        let url = TokenUrl::new("acme/web", "ghs_fill_proof");
        ensure(&clone, &url, Some("2099-01-01T00:00:00.000Z")).unwrap();

        let worktree = dir.join("wt");
        git(
            &clone,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "exp/EXP-1",
                worktree.to_str().unwrap(),
            ],
        );

        let stdout =
            credential_fill(&worktree, &base.0, "https://github.com/acme/web.git");
        assert!(stdout.contains("username=x-access-token"), "{stdout}");
        assert!(stdout.contains("password=ghs_fill_proof"), "{stdout}");
    }

    /// The scoping contract: inside the clone, OUR helper deterministically
    /// beats a configured global helper for THIS repo's URL (the reset entry
    /// — a member's personal PAT must not shadow the App token), while any
    /// OTHER github.com repo still falls through to the global helper (a
    /// Claude child pushing a fork / fetching a submodule keeps the user's
    /// own auth instead of a 403-ing repo-scoped token).
    #[test]
    fn scoped_reset_beats_global_helper_for_this_repo_only() {
        let base = temp_dir("scoped");
        let clone = seed_clone_with_token_remote(&base.0);
        let url = TokenUrl::new("acme/web", "ghs_app_token");
        ensure(&clone, &url, Some("2099-01-01T00:00:00.000Z")).unwrap();

        // A fake "global" helper standing in for the user's keychain/PAT.
        fs::write(
            base.0.join(".gitconfig"),
            "[credential]\n\thelper = \"!f() { echo username=global-user; echo password=global_pat; }; f\"\n",
        )
        .unwrap();

        let ours = credential_fill(&clone, &base.0, "https://github.com/acme/web.git");
        assert!(ours.contains("password=ghs_app_token"), "{ours}");
        assert!(!ours.contains("global_pat"), "global helper leaked in: {ours}");

        let other =
            credential_fill(&clone, &base.0, "https://github.com/other/repo.git");
        assert!(other.contains("password=global_pat"), "{other}");
        assert!(!other.contains("ghs_app_token"), "token leaked cross-repo: {other}");
    }

    /// #EXP-1615: the helper is scoped to the remote's OWN host (eCode's
    /// `credential_helper_key`), not a hard-coded github.com.
    #[test]
    fn any_host_gets_its_own_scoped_helper() {
        let base = temp_dir("anyhost");
        let clone = seed_clone_with_token_remote(&base.0);
        let credentials =
            ecode::Credentials::new("https://git.example.com/team/app", "exp-bot", "tok_any_host", None).unwrap();
        let url = TokenUrl::from_credentials(credentials);
        assert_eq!(url.full_name(), "team/app");
        ensure(&clone, &url, None).unwrap();
        assert_eq!(origin_url(&clone), "https://git.example.com/team/app.git");
        let ours = credential_fill(&clone, &base.0, "https://git.example.com/team/app.git");
        assert!(ours.contains("username=exp-bot") && ours.contains("password=tok_any_host"), "{ours}");
        assert_eq!(url.redacted(), "https://exp-bot:***@git.example.com/team/app.git");
    }
}
