//! Trunk clone lifecycle: auto-clone on board open, background auto-sync,
//! and push/publish — all against the **trunk** clone
//! (`<repos_root>/<owner>/<name>`), reusing [`crate::git_worktree`]'s
//! `clone_path` / [`TokenUrl`] redaction. This module is the ONE transport
//! layer — every network git op on the trunk routes through here.
//!
//! Contract:
//! * **Auto-clone on board open** when `<clone>/.git` is missing: mint a JIT
//!   token, clone, then a fetch. Progress surfaces via [`CloneEvent`] (parsed
//!   from `git clone --progress` stderr → the git-bar chip). `git clone`
//!   streams progress on stderr with `\r`-overwritten phase lines, so [`ensure`]
//!   spawns git itself (rather than reusing `git_worktree::ensure_clone`, which
//!   blocks on `.output()` and cannot surface a percentage) and parses each
//!   segment via [`parse_clone_progress`].
//! * **Auto-sync**: the GitBar runs [`auto_sync`] on a [`AUTO_SYNC_INTERVAL`]
//!   timer and on window focus, coalesced through [`should_fetch`]
//!   ([`FETCH_DEBOUNCE`]). [`auto_sync`] = fetch → read the trunk state → and
//!   ONLY when `TrunkState::ff_eligible()` (clean + behind-only + upstream +
//!   real branch) run [`ff_update`] (`git merge --ff-only`). Everything else
//!   returns a [`AutoSyncOutcome`] Skipped variant — auto-sync structurally
//!   cannot checkout, rebase, or touch `<clone>.worktrees/`.
//! * **Token re-mint**: the ~55-min installation token is disposable. The
//!   caller obtains working ambient auth before each network op
//!   ([`crate::git_credentials::ensure_repo_auth`] — cached-or-fresh mint +
//!   downgrade-guarded credential install); the wrappers here are pure
//!   transport over that ambient auth (EXP-73: they no longer rewrite
//!   `remote.origin.url`, which is how a stale cached token used to clobber
//!   a fresh one mid-run).
//! * **Ahead/behind** = `git rev-list --left-right --count <branch>...origin/<branch>`
//!   after a fetch — no network for the counts themselves ([`ahead_behind`]).
//! * **Push** = fetch → auto-rebase if behind → push, always targeting the
//!   CHECKED-OUT branch the caller read from disk. **Publish** = `push -u`
//!   for a branch with no upstream yet. There is no pull op: "Get latest" is
//!   [`ff_update`], integrating divergence is [`push`]'s rebase. Conflicts
//!   never auto-abort (no `--abort` is ever issued here): git's markers are
//!   left in place and the trunk enters conflict mode, re-derived from disk
//!   by `crate::scm::detect_conflict`.
//!
//! Redaction (§7.1 step 2 / L5): every git op is `std::process::Command("git")`
//! with explicit argv — never `gh`, never a git library, never a shell. The
//! only op that carries the token in argv is the clone (via `TokenUrl::raw`);
//! its captured stderr — and any error output that quotes the remote URL — is
//! scrubbed (`raw` → `redacted`) before it can reach a [`GitError`].

use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use crate::git_credentials;
use crate::git_worktree::{clone_path, run_git, GitError, TokenUrl};
use crate::trunk_state;

/// Background auto-sync cadence: the GitBar's timer runs [`auto_sync`] this
/// often while a board with a cloned trunk is open.
pub const AUTO_SYNC_INTERVAL: Duration = Duration::from_secs(120);

/// Freshness debounce: focus- and timer-triggered auto-syncs coalesce to at
/// most one per minute ([`should_fetch`]). Board-open and post-transport
/// fetches are not debounced.
pub const FETCH_DEBOUNCE: Duration = Duration::from_secs(60);

/// Token re-mint margin (v4 §4.1): a cached installation token within five
/// minutes of expiry is treated as spent — re-mint before the next network op
/// (the token lives ~55 min; the remote URL that carries it is disposable).
pub const TOKEN_REMINT_MARGIN: Duration = Duration::from_secs(5 * 60);

/// Progress of a clone/fetch job (v4 §4.1) — the git-bar chip renders these
/// (`Cloning <name>… 42%`, error + retry). Emitted through a caller-supplied
/// callback so the network job stays off the gpui foreground.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloneEvent {
    /// The job started (chip → spinner).
    Started,
    /// `git clone --progress` percentage (0–100), parsed from stderr.
    Progress(u8),
    /// Clone (and initial fetch) finished — the trunk is ready.
    Done,
    /// Failed with a token-scrubbed reason (chip → error state + retry).
    Failed(String),
}

/// A sink for [`CloneEvent`]s. The background clone job calls this per
/// progress line; the caller marshals to the UI thread.
pub type CloneProgress<'a> = &'a mut dyn FnMut(CloneEvent);

// ---------------------------------------------------------------------------
// Pure freshness / token-expiry decisions (unit-tested, no git, no clock I/O)
// ---------------------------------------------------------------------------

/// Whether a timer-/focus-triggered auto-sync is due: at least
/// [`FETCH_DEBOUNCE`] has elapsed since the last successful sync. The caller
/// holds `last` (the `Instant` of its previous sync) and calls this from the
/// [`AUTO_SYNC_INTERVAL`] timer tick and the window-focus observer.
pub fn should_fetch(last: Instant) -> bool {
    last.elapsed() >= FETCH_DEBOUNCE
}

/// Whether the cached installation token must be re-minted before the next
/// network op: within [`TOKEN_REMINT_MARGIN`] of `expires_at`, already past
/// it, or an absent/unparseable expiry (v4 §4.1 — re-mint on any doubt, the
/// remote URL is disposable). `expires_at` is the ISO-8601 UTC timestamp from
/// `repositories.installationToken`; `now` is injected so the decision is pure
/// and testable.
pub fn token_needs_remint(expires_at: Option<&str>, now: SystemTime) -> bool {
    token_needs_remint_with_margin(expires_at, now, TOKEN_REMINT_MARGIN)
}

/// [`token_needs_remint`] with an explicit margin — the mid-session refresher
/// demands a longer remaining life ([`crate::token_refresh::REFRESH_LEAD`])
/// than a one-shot network op does.
pub fn token_needs_remint_with_margin(
    expires_at: Option<&str>,
    now: SystemTime,
    margin: Duration,
) -> bool {
    match expires_at.and_then(parse_iso8601_utc) {
        Some(expiry) => match expiry.duration_since(now) {
            Ok(remaining) => remaining < margin,
            Err(_) => true, // now >= expiry → already expired
        },
        None => true, // unknown/unparseable expiry → re-mint to be safe
    }
}

/// Parse the server's ISO-8601 UTC form (`2026-07-03T12:55:00.000Z`, with or
/// without fractional seconds) into a [`SystemTime`]. `None` on any deviation
/// from that shape — callers treat `None` as "re-mint" (fail safe). Pure: no
/// external time crate (chrono is not a desktop dependency), uses Howard
/// Hinnant's `days_from_civil`. Crate-visible: [`crate::git_credentials`]'s
/// no-downgrade guard and [`crate::token_refresh`]'s scheduling compare the
/// same server timestamps.
pub(crate) fn parse_iso8601_utc(raw: &str) -> Option<SystemTime> {
    let raw = raw.trim();
    let (date, time_part) = raw.split_once('T')?;
    // Tolerate a trailing `Z` and drop any fractional seconds.
    let time = time_part.trim_end_matches('Z').split('.').next()?;

    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let second: i64 = time_parts.next().unwrap_or("0").parse().ok()?;
    if !(0..24).contains(&hour) || !(0..60).contains(&minute) || !(0..=60).contains(&second) {
        return None;
    }

    // days_from_civil (Hinnant): civil date → days since the Unix epoch.
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let year_of_era = y - era * 400; // [0, 399]
    let month_index = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_index + 2) / 5 + day - 1; // [0, 365]
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146097 + day_of_era - 719468;

    let total = days * 86400 + hour * 3600 + minute * 60 + second;
    if total < 0 {
        return None; // pre-epoch: never a valid token expiry
    }
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(total as u64))
}

/// Parse a `git clone --progress` stderr segment into a percentage. Only the
/// transfer phases (`Receiving objects:` and the trailing `Resolving deltas:`)
/// are surfaced — the server-side `Counting`/`Compressing` phases flash to
/// 100% instantly and would make the chip jump. `None` for any other line.
pub fn parse_clone_progress(line: &str) -> Option<u8> {
    let line = line.trim();
    if !(line.starts_with("Receiving objects:") || line.starts_with("Resolving deltas:")) {
        return None;
    }
    let percent_at = line.find('%')?;
    let digits: String = line[..percent_at]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    Some(digits.parse::<u8>().ok()?.min(100))
}

/// Parse `git rev-list --left-right --count <branch>...origin/<branch>` output
/// (`<ahead>\t<behind>`) into `(ahead, behind)`.
fn parse_ahead_behind(raw: &str) -> Option<(u32, u32)> {
    let mut parts = raw.split_whitespace();
    let ahead = parts.next()?.parse().ok()?;
    let behind = parts.next()?.parse().ok()?;
    Some((ahead, behind))
}

// ---------------------------------------------------------------------------
// Git ops (argv only, trunk clone)
// ---------------------------------------------------------------------------

/// Ensure the trunk clone exists, emitting [`CloneEvent`]s as it goes
/// (v4 §4.1 auto-clone). Idempotent: an existing `<clone>/.git` short-circuits
/// to [`CloneEvent::Done`] with no network. Otherwise: `git clone --progress`
/// (streaming percentage), then a best-effort fetch (§4.1 "clone, then a
/// fetch"), then [`CloneEvent::Done`]. On failure, emits
/// [`CloneEvent::Failed`] with a token-scrubbed reason and returns the error.
pub fn ensure(
    repos_root: &Path,
    full_name: &str,
    url: &TokenUrl,
    expires_at: Option<&str>,
    on_event: CloneProgress<'_>,
) -> Result<PathBuf, GitError> {
    let clone = clone_path(repos_root, full_name);
    if clone.join(".git").exists() {
        // Reuse — §7.1 idempotent relaunch. Best-effort ambient-auth install
        // (EXP-73): heals a pre-existing token-embedded origin at board
        // open; a failure here must not fail the reuse (the next network op
        // installs auth itself).
        let _ = git_credentials::ensure(&clone, url, expires_at);
        on_event(CloneEvent::Done);
        return Ok(clone);
    }

    on_event(CloneEvent::Started);

    if let Some(parent) = clone.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            let err = GitError {
                op: format!("prepare clone dir for {full_name}"),
                detail: e.to_string(),
            };
            on_event(CloneEvent::Failed(err.detail.clone()));
            return Err(err);
        }
    }

    let clone_str = clone.to_string_lossy().into_owned();
    match run_clone_streaming(url, &clone_str, full_name, &mut *on_event) {
        Ok(()) => {
            // Ambient auth right after the clone exists (the credential file
            // needs `.git` on disk; the clone itself still rides the token
            // URL in argv) — the follow-up fetch already exercises it, and
            // `origin` is reset to the bare URL here (EXP-73).
            if let Err(err) = git_credentials::ensure(&clone, url, expires_at) {
                on_event(CloneEvent::Failed(err.detail.clone()));
                return Err(err);
            }
            // §4.1: clone, then a fetch. A fresh clone is already up to date,
            // so this is best-effort — a transient fetch failure must not fail
            // the clone (the trunk is on disk and usable).
            let _ = fetch(&clone, url);
            on_event(CloneEvent::Done);
            Ok(clone)
        }
        Err(err) => {
            on_event(CloneEvent::Failed(err.detail.clone()));
            Err(err)
        }
    }
}

/// `git fetch origin` against the trunk clone (freshness). Pure transport:
/// auth is the clone's ambient credential helper, installed by the caller
/// via [`git_credentials::ensure_repo_auth`] (EXP-73 — no more per-op
/// `remote set-url`). `url` remains for error scrubbing only.
pub fn fetch(clone: &Path, url: &TokenUrl) -> Result<(), GitError> {
    run_git(Some(clone), &["fetch", "origin"], Some(url), "git fetch origin")?;
    Ok(())
}

/// Push the trunk's CHECKED-OUT branch: fetch → auto-rebase onto
/// `origin/<branch>` if behind → push. The caller reads `branch` fresh from
/// disk (`trunk_state::read`) so the transport always targets what is
/// actually checked out. A rebase conflict leaves markers in place and
/// returns the error (no auto-abort).
pub fn push(clone: &Path, branch: &str, url: &TokenUrl) -> Result<(), GitError> {
    fetch(clone, url)?;
    // Local counts only (the fetch above already refreshed origin/<branch>);
    // a missing upstream ref just means "not behind" → push creates it.
    let (_ahead, behind) = ahead_behind(clone, branch).unwrap_or((0, 0));
    if behind > 0 {
        run_git(
            Some(clone),
            &["rebase", "--autostash", &format!("origin/{branch}")],
            Some(url),
            "git rebase origin",
        )?;
    }
    run_git(Some(clone), &["push", "origin", branch], Some(url), "git push")?;
    Ok(())
}

/// Publish an unpublished branch: `git push -u origin <branch>` (creates the
/// upstream the git bar's counts and [`auto_sync`] need). Pure transport over
/// ambient auth, like [`fetch`].
pub fn publish(clone: &Path, branch: &str, url: &TokenUrl) -> Result<(), GitError> {
    run_git(Some(clone), &["push", "-u", "origin", branch], Some(url), "git push -u")?;
    Ok(())
}

/// Fast-forward the checked-out `branch` to `origin/<branch>`:
/// `git merge --ff-only origin/<branch>`. Local + tokenless (run after a
/// [`fetch`]), and the ONLY auto-mutation primitive — `--ff-only` is the
/// TOCTOU guard: if the tree diverged between the eligibility check and the
/// merge, git refuses instead of creating a merge commit.
pub fn ff_update(clone: &Path, branch: &str) -> Result<(), GitError> {
    run_git(
        Some(clone),
        &["merge", "--ff-only", &format!("origin/{branch}")],
        None,
        "git merge --ff-only",
    )?;
    Ok(())
}

/// What one [`auto_sync`] pass did (or why it deliberately did nothing).
/// Every variant after a successful fetch counts as a successful sync — the
/// Skipped* variants are the safety gates, not failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoSyncOutcome {
    /// Clean + behind-only → fast-forwarded to `origin/<branch>`.
    FastForwarded,
    /// Nothing behind (local-only commits included) — nothing to do.
    UpToDate,
    /// Behind, but the working tree has local changes.
    SkippedDirty,
    /// Behind AND ahead — integrating is the explicit "Sync" push path.
    SkippedDiverged,
    /// A rebase/merge sits paused with conflicts.
    SkippedConflict,
    /// The checked-out branch tracks no upstream (unpublished).
    SkippedNoUpstream,
    /// Detached HEAD (e.g. mid-rebase inspection) — no branch to move.
    SkippedDetached,
}

/// One background auto-sync pass: [`fetch`] → read the trunk state from disk
/// → fast-forward ONLY when `TrunkState::ff_eligible()`. Structurally cannot
/// switch branches or touch `<clone>.worktrees/`: no checkout/switch/worktree
/// argv exists here, the branch is derived from HEAD, and the only mutation
/// is [`ff_update`]'s `merge --ff-only` at the clone root. A run config
/// launched mid-sync therefore always runs the same working copy on the same
/// branch (at worst one ff newer — identical to the user having pulled).
pub fn auto_sync(clone: &Path, url: &TokenUrl) -> Result<AutoSyncOutcome, GitError> {
    fetch(clone, url)?;
    let state = trunk_state::read(clone)?;
    if state.conflict.is_some() {
        return Ok(AutoSyncOutcome::SkippedConflict);
    }
    if state.branch.is_empty() || state.branch.starts_with('(') {
        return Ok(AutoSyncOutcome::SkippedDetached);
    }
    if !state.has_upstream {
        return Ok(AutoSyncOutcome::SkippedNoUpstream);
    }
    if state.behind == 0 {
        return Ok(AutoSyncOutcome::UpToDate);
    }
    if state.dirty {
        return Ok(AutoSyncOutcome::SkippedDirty);
    }
    if state.ahead > 0 {
        return Ok(AutoSyncOutcome::SkippedDiverged);
    }
    debug_assert!(state.ff_eligible());
    ff_update(clone, &state.branch)?;
    Ok(AutoSyncOutcome::FastForwarded)
}

/// Ahead/behind of `<branch>` vs `origin/<branch>` via
/// `git rev-list --left-right --count <branch>...origin/<branch>` — no network
/// (run after a [`fetch`], v4 §4.1). Returns `(ahead, behind)`.
pub fn ahead_behind(clone: &Path, branch: &str) -> Result<(u32, u32), GitError> {
    let op = "git rev-list --left-right --count";
    let out = run_git(
        Some(clone),
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("{branch}...origin/{branch}"),
        ],
        None,
        op,
    )?;
    parse_ahead_behind(&out).ok_or_else(|| GitError {
        op: op.to_string(),
        detail: format!("unexpected rev-list output: {}", out.trim()),
    })
}

// ---------------------------------------------------------------------------
// argv plumbing (non-streaming ops route through the shared
// `git_worktree::run_git`; only the progress-streaming clone stays local)
// ---------------------------------------------------------------------------

/// `git clone --progress <token-url> <path>`, streaming stderr into
/// [`CloneEvent::Progress`] events while accumulating the full (scrubbed on
/// failure) output for the error detail. The token rides in argv here — the
/// captured stderr is scrubbed before it can reach a [`GitError`].
fn run_clone_streaming(
    url: &TokenUrl,
    clone_str: &str,
    full_name: &str,
    on_event: CloneProgress<'_>,
) -> Result<(), GitError> {
    let op = format!("git clone {full_name}"); // never the URL
    let mut child = base_command_no_cwd(&["clone", "--progress", &url.raw(), clone_str])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| GitError {
            op: op.clone(),
            detail: if e.kind() == std::io::ErrorKind::NotFound {
                "git not found on PATH".to_string()
            } else {
                url.scrub(&e.to_string())
            },
        })?;

    let stderr = child
        .stderr
        .take()
        .expect("stderr was requested via Stdio::piped");
    let captured = stream_progress(stderr, on_event);

    let status = child.wait().map_err(|e| GitError {
        op: op.clone(),
        detail: url.scrub(&e.to_string()),
    })?;
    if status.success() {
        Ok(())
    } else {
        let mut detail = url.scrub(captured.trim());
        if detail.is_empty() {
            detail = format!("exit code {}", status.code().unwrap_or(-1));
        }
        Err(GitError { op, detail })
    }
}

/// Read a child's stderr, splitting on `\r` OR `\n` (git overwrites the
/// progress line with `\r` and only terminates a phase with `\n`), emitting a
/// [`CloneEvent::Progress`] per parseable segment and returning the full text
/// (for the failure detail). Byte-buffered so multibyte paths in error output
/// stay valid UTF-8.
fn stream_progress<R: Read>(reader: R, on_event: CloneProgress<'_>) -> String {
    let mut reader = BufReader::new(reader);
    let mut byte = [0u8; 1];
    let mut segment: Vec<u8> = Vec::new();
    let mut captured = String::new();

    let flush = |segment: &mut Vec<u8>, captured: &mut String, on_event: &mut dyn FnMut(CloneEvent)| {
        if segment.is_empty() {
            return;
        }
        let line = String::from_utf8_lossy(segment);
        if let Some(pct) = parse_clone_progress(&line) {
            on_event(CloneEvent::Progress(pct));
        }
        captured.push_str(&line);
        captured.push('\n');
        segment.clear();
    };

    while let Ok(1) = reader.read(&mut byte) {
        if byte[0] == b'\r' || byte[0] == b'\n' {
            flush(&mut segment, &mut captured, on_event);
        } else {
            segment.push(byte[0]);
        }
    }
    flush(&mut segment, &mut captured, on_event);
    captured
}

/// A bare `git` command with the shared hardening (explicit argv, no shell, no
/// interactive credential prompt — a dead token must FAIL, never park a GUI app
/// behind an invisible username/password prompt). Only the progress-streaming
/// clone needs to build the child itself; every other op goes through
/// [`git_worktree::run_git`].
fn base_command_no_cwd(args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command.args(args);
    command.env("GIT_TERMINAL_PROMPT", "0");
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    // ---- pure: freshness debounce ----

    #[test]
    fn should_fetch_only_after_the_debounce_window() {
        // A just-now fetch is not due; a fetch older than the window is.
        assert!(!should_fetch(Instant::now()));
        let stale = Instant::now() - (FETCH_DEBOUNCE + Duration::from_secs(1));
        assert!(should_fetch(stale));
        let fresh = Instant::now() - (FETCH_DEBOUNCE - Duration::from_secs(30));
        assert!(!should_fetch(fresh));
    }

    // ---- pure: token expiry / re-mint ----

    #[test]
    fn parse_iso8601_utc_matches_the_server_form() {
        // 2026-07-03T12:55:00Z == 1_783_083_300 unix seconds.
        let expected = SystemTime::UNIX_EPOCH + Duration::from_secs(1_783_083_300);
        assert_eq!(parse_iso8601_utc("2026-07-03T12:55:00.000Z"), Some(expected));
        assert_eq!(parse_iso8601_utc("2026-07-03T12:55:00Z"), Some(expected));
        // The epoch itself round-trips.
        assert_eq!(
            parse_iso8601_utc("1970-01-01T00:00:00Z"),
            Some(SystemTime::UNIX_EPOCH)
        );
        // Garbage / partial / pre-epoch → None (caller re-mints).
        assert_eq!(parse_iso8601_utc("not-a-date"), None);
        assert_eq!(parse_iso8601_utc("2026-07-03"), None);
        assert_eq!(parse_iso8601_utc("2026-13-03T00:00:00Z"), None);
        assert_eq!(parse_iso8601_utc("1969-12-31T23:59:59Z"), None);
    }

    #[test]
    fn token_needs_remint_within_five_minutes_or_on_doubt() {
        // `now` == 2026-07-03T12:55:00Z.
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_783_083_300);

        // Expiry 10 minutes out → still fresh.
        assert!(!token_needs_remint(Some("2026-07-03T13:05:00Z"), now));
        // Exactly 5 minutes out → the margin is `<`, so NOT yet re-minted.
        assert!(!token_needs_remint(Some("2026-07-03T13:00:00Z"), now));
        // 4m59s out → inside the margin → re-mint.
        assert!(token_needs_remint(Some("2026-07-03T12:59:59Z"), now));
        // Already expired → re-mint.
        assert!(token_needs_remint(Some("2026-07-03T12:50:00Z"), now));
        // Absent / unparseable expiry → re-mint (fail safe).
        assert!(token_needs_remint(None, now));
        assert!(token_needs_remint(Some("whenever"), now));
    }

    // ---- pure: clone progress parsing ----

    #[test]
    fn parse_clone_progress_reads_transfer_phases_only() {
        assert_eq!(
            parse_clone_progress("Receiving objects:  42% (42/100), 1.00 MiB | 2.00 MiB/s"),
            Some(42)
        );
        assert_eq!(
            parse_clone_progress("Receiving objects: 100% (100/100), 5.00 MiB, done."),
            Some(100)
        );
        assert_eq!(
            parse_clone_progress("Resolving deltas:   7% (2/30)"),
            Some(7)
        );
        // Server-side phases and chatter are ignored (they flash to 100%).
        assert_eq!(parse_clone_progress("remote: Counting objects: 100% (100/100)"), None);
        assert_eq!(parse_clone_progress("Compressing objects: 100% (50/50)"), None);
        assert_eq!(parse_clone_progress("Cloning into '/tmp/x'..."), None);
        assert_eq!(parse_clone_progress(""), None);
    }

    #[test]
    fn parse_ahead_behind_reads_tab_separated_counts() {
        assert_eq!(parse_ahead_behind("1\t2\n"), Some((1, 2)));
        assert_eq!(parse_ahead_behind("0\t0"), Some((0, 0)));
        assert_eq!(parse_ahead_behind("  3   5 "), Some((3, 5)));
        assert_eq!(parse_ahead_behind(""), None);
        assert_eq!(parse_ahead_behind("x\ty"), None);
    }

    // ---- stderr streaming (hermetic: an in-memory `\r`-overwritten stream) ----

    #[test]
    fn stream_progress_emits_per_carriage_return_segment_and_captures_all() {
        // Mimics git --progress: `\r`-overwritten receiving line, then a `\n`
        // phase terminator, then a trailing fatal (no newline).
        let raw = b"Cloning into '/tmp/x'...\nReceiving objects:  10% (1/10)\rReceiving objects:  55% (6/10)\rReceiving objects: 100% (10/10), done.\nfatal: boom".to_vec();
        let mut events = Vec::new();
        let captured = {
            let mut sink = |e: CloneEvent| events.push(e);
            stream_progress(std::io::Cursor::new(raw), &mut sink)
        };
        assert_eq!(
            events,
            vec![
                CloneEvent::Progress(10),
                CloneEvent::Progress(55),
                CloneEvent::Progress(100),
            ]
        );
        // The full text is retained for a failure detail.
        assert!(captured.contains("fatal: boom"), "captured: {captured}");
        assert!(captured.contains("Cloning into"), "captured: {captured}");
    }

    // ---- real-git integration (hermetic: local file:// remote, no network) ----

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
        path.push(format!("exp-clone-mgr-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn git(cwd: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn seed_origin(dir: &Path) -> PathBuf {
        let origin = dir.join("origin-src");
        fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]);
        fs::write(origin.join("README.md"), "seed\n").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "--quiet", "-m", "seed"]);
        origin
    }

    #[test]
    fn ahead_behind_counts_local_vs_upstream() {
        let dir = temp_dir("aheadbehind");
        let origin = seed_origin(&dir.0);
        let work = dir.0.join("work");
        git(&dir.0, &["clone", "--quiet", origin.to_str().unwrap(), work.to_str().unwrap()]);

        // Even with origin/main tracked: zero divergence right after clone.
        assert_eq!(ahead_behind(&work, "main").unwrap(), (0, 0));

        // One local commit → ahead 1, behind 0.
        fs::write(work.join("local.txt"), "x\n").unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "--quiet", "-m", "local"]);
        assert_eq!(ahead_behind(&work, "main").unwrap(), (1, 0));

        // A new upstream commit that `work` fetches → ahead 1, behind 1.
        fs::write(origin.join("upstream.txt"), "y\n").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "--quiet", "-m", "upstream"]);
        git(&work, &["fetch", "--quiet", "origin"]);
        assert_eq!(ahead_behind(&work, "main").unwrap(), (1, 1));
    }

    // ---- transport against a local bare origin (hermetic, no network) ----
    //
    // EXP-73: the wrappers are pure transport over whatever `origin` is
    // configured (ambient credential-helper auth in production), so the
    // fixture's plain local-bare origin exercises the exact production argv.
    // `dummy_url` survives purely as the scrub input.

    fn dummy_url() -> TokenUrl {
        TokenUrl::new("acme/web", "ghs_dead")
    }

    fn init_repo(path: &Path) {
        fs::create_dir_all(path).unwrap();
        git(path, &["init", "--quiet", "-b", "main"]);
        git(path, &["config", "user.email", "t@example.com"]);
        git(path, &["config", "user.name", "t"]);
        git(path, &["config", "commit.gpgsign", "false"]);
    }

    fn write(path: &Path, rel: &str, content: &str) {
        fs::write(path.join(rel), content).unwrap();
    }

    fn commit_all(path: &Path, msg: &str) {
        git(path, &["add", "-A"]);
        git(path, &["commit", "--quiet", "-m", msg]);
    }

    /// A bare origin with one `main` commit, a `work` clone whose token
    /// remote rewrites to it, and a `consumer` clone used to advance the
    /// origin out-of-band. Returns (`work`, `consumer`, `bare`).
    fn seed_remote(d: &Path) -> (PathBuf, PathBuf, PathBuf) {
        let bare = d.join("origin.git");
        git(d, &["init", "--quiet", "--bare", "-b", "main", bare.to_str().unwrap()]);

        let work = d.join("work");
        init_repo(&work);
        write(&work, "base.txt", "base\n");
        commit_all(&work, "base");
        git(&work, &["remote", "add", "origin", bare.to_str().unwrap()]);
        git(&work, &["push", "--quiet", "-u", "origin", "main"]);

        let consumer = d.join("consumer");
        git(d, &["clone", "--quiet", bare.to_str().unwrap(), consumer.to_str().unwrap()]);
        git(&consumer, &["config", "user.email", "c@example.com"]);
        git(&consumer, &["config", "user.name", "c"]);

        (work, consumer, bare)
    }

    fn head_ref(repo: &Path) -> String {
        let out = Command::new("git")
            .args(["symbolic-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn head_commit(repo: &Path) -> String {
        let out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn push_pushes_a_fast_forward_of_the_given_branch() {
        let d = temp_dir("push");
        let (work, _consumer, bare) = seed_remote(&d.0);
        write(&work, "w.txt", "w\n");
        commit_all(&work, "work commit");

        push(&work, "main", &dummy_url()).unwrap();

        // A fresh clone of the bare must see the pushed commit.
        let verify = d.0.join("verify");
        git(&d.0, &["clone", "--quiet", bare.to_str().unwrap(), verify.to_str().unwrap()]);
        assert!(verify.join("w.txt").exists());
    }

    #[test]
    fn push_auto_rebases_when_behind() {
        let d = temp_dir("pushrebase");
        let (work, consumer, bare) = seed_remote(&d.0);

        // Consumer advances origin (non-conflicting file).
        write(&consumer, "c.txt", "c\n");
        commit_all(&consumer, "consumer commit");
        git(&consumer, &["push", "--quiet", "origin", "main"]);

        // Work commits on the stale base ⇒ diverged; push must fetch, rebase,
        // then push.
        write(&work, "w.txt", "w\n");
        commit_all(&work, "work commit");

        push(&work, "main", &dummy_url()).unwrap();

        let verify = d.0.join("verify");
        git(&d.0, &["clone", "--quiet", bare.to_str().unwrap(), verify.to_str().unwrap()]);
        assert!(verify.join("w.txt").exists());
        assert!(verify.join("c.txt").exists());
        // Linear (rebased) history, not a merge: both commits + base.
        assert_eq!(crate::scm::log_branch(&verify, None, 0, 10).unwrap().len(), 3);
    }

    #[test]
    fn push_conflict_leaves_markers_for_detect() {
        let d = temp_dir("pushconflict");
        let (work, consumer, _bare) = seed_remote(&d.0);

        // Both edit the same file ⇒ the pre-push rebase conflicts.
        write(&consumer, "base.txt", "consumer\n");
        commit_all(&consumer, "consumer edit");
        git(&consumer, &["push", "--quiet", "origin", "main"]);
        write(&work, "base.txt", "work\n");
        commit_all(&work, "work edit");

        let err = push(&work, "main", &dummy_url()).unwrap_err();
        assert!(!format!("{err}").contains("ghs_dead"), "token leaked: {err}");

        let conflict = crate::scm::detect_conflict(&work).expect("rebase should be paused");
        assert_eq!(conflict.kind, crate::scm::ConflictKind::Rebase);
        assert!(conflict.files.contains(&"base.txt".to_string()));

        crate::scm::abort_conflict(&work, crate::scm::ConflictKind::Rebase).unwrap();
        assert!(crate::scm::detect_conflict(&work).is_none());
    }

    #[test]
    fn publish_creates_the_upstream() {
        let d = temp_dir("publish");
        let (work, _consumer, _bare) = seed_remote(&d.0);
        git(&work, &["checkout", "--quiet", "-b", "feature/x"]);
        write(&work, "f.txt", "f\n");
        commit_all(&work, "feature commit");

        let before = crate::scm::status(&work).unwrap();
        assert_eq!(before.upstream, None);

        publish(&work, "feature/x", &dummy_url()).unwrap();

        let after = crate::scm::status(&work).unwrap();
        assert_eq!(after.upstream.as_deref(), Some("origin/feature/x"));
        assert_eq!(ahead_behind(&work, "feature/x").unwrap(), (0, 0));
    }

    #[test]
    fn ff_update_fast_forwards_and_refuses_diverged() {
        let d = temp_dir("ffupdate");
        let (work, consumer, _bare) = seed_remote(&d.0);

        // Behind-only → ff succeeds.
        write(&consumer, "c.txt", "c\n");
        commit_all(&consumer, "consumer commit");
        git(&consumer, &["push", "--quiet", "origin", "main"]);
        fetch(&work, &dummy_url()).unwrap();
        ff_update(&work, "main").unwrap();
        assert!(work.join("c.txt").exists());

        // Diverged → --ff-only refuses (the TOCTOU guard).
        write(&consumer, "c2.txt", "c2\n");
        commit_all(&consumer, "consumer 2");
        git(&consumer, &["push", "--quiet", "origin", "main"]);
        write(&work, "w.txt", "w\n");
        commit_all(&work, "local");
        fetch(&work, &dummy_url()).unwrap();
        assert!(ff_update(&work, "main").is_err());
    }

    // ---- auto_sync outcome matrix (scope F: the run-config safety
    // invariant — a skipped sync must leave HEAD untouched) ----

    #[test]
    fn auto_sync_fast_forwards_when_clean_and_behind_only() {
        let d = temp_dir("autosync-ff");
        let (work, consumer, _bare) = seed_remote(&d.0);
        write(&consumer, "c.txt", "c\n");
        commit_all(&consumer, "consumer commit");
        git(&consumer, &["push", "--quiet", "origin", "main"]);

        let head_before = head_ref(&work);
        let outcome = auto_sync(&work, &dummy_url()).unwrap();
        assert_eq!(outcome, AutoSyncOutcome::FastForwarded);
        assert!(work.join("c.txt").exists());
        // Fast-forward moves the commit, never the checked-out branch.
        assert_eq!(head_ref(&work), head_before);
    }

    #[test]
    fn auto_sync_up_to_date_when_nothing_behind() {
        let d = temp_dir("autosync-utd");
        let (work, _consumer, _bare) = seed_remote(&d.0);
        assert_eq!(auto_sync(&work, &dummy_url()).unwrap(), AutoSyncOutcome::UpToDate);

        // Ahead-only is also "nothing to pull".
        write(&work, "w.txt", "w\n");
        commit_all(&work, "local");
        assert_eq!(auto_sync(&work, &dummy_url()).unwrap(), AutoSyncOutcome::UpToDate);
    }

    #[test]
    fn auto_sync_skips_dirty_and_leaves_head_and_tree_untouched() {
        let d = temp_dir("autosync-dirty");
        let (work, consumer, _bare) = seed_remote(&d.0);
        write(&consumer, "c.txt", "c\n");
        commit_all(&consumer, "consumer commit");
        git(&consumer, &["push", "--quiet", "origin", "main"]);

        write(&work, "base.txt", "local dirt\n");
        let (head_before, commit_before) = (head_ref(&work), head_commit(&work));

        assert_eq!(auto_sync(&work, &dummy_url()).unwrap(), AutoSyncOutcome::SkippedDirty);
        assert_eq!(head_ref(&work), head_before);
        assert_eq!(head_commit(&work), commit_before);
        assert_eq!(fs::read_to_string(work.join("base.txt")).unwrap(), "local dirt\n");
        assert!(!work.join("c.txt").exists());
    }

    #[test]
    fn auto_sync_skips_diverged_and_leaves_head_untouched() {
        let d = temp_dir("autosync-diverged");
        let (work, consumer, _bare) = seed_remote(&d.0);
        write(&consumer, "c.txt", "c\n");
        commit_all(&consumer, "consumer commit");
        git(&consumer, &["push", "--quiet", "origin", "main"]);
        write(&work, "w.txt", "w\n");
        commit_all(&work, "local commit");

        let (head_before, commit_before) = (head_ref(&work), head_commit(&work));
        assert_eq!(
            auto_sync(&work, &dummy_url()).unwrap(),
            AutoSyncOutcome::SkippedDiverged
        );
        assert_eq!(head_ref(&work), head_before);
        assert_eq!(head_commit(&work), commit_before);
        assert!(!work.join("c.txt").exists());
    }

    #[test]
    fn auto_sync_skips_a_paused_conflict_and_leaves_it_paused() {
        let d = temp_dir("autosync-conflict");
        let (work, consumer, _bare) = seed_remote(&d.0);

        // Engage a real rebase conflict (both edit base.txt), via push's
        // integrate path.
        write(&consumer, "base.txt", "consumer\n");
        commit_all(&consumer, "consumer edit");
        git(&consumer, &["push", "--quiet", "origin", "main"]);
        write(&work, "base.txt", "work\n");
        commit_all(&work, "work edit");
        assert!(push(&work, "main", &dummy_url()).is_err());
        assert!(crate::scm::detect_conflict(&work).is_some());

        assert_eq!(
            auto_sync(&work, &dummy_url()).unwrap(),
            AutoSyncOutcome::SkippedConflict
        );
        // Still paused — auto-sync never aborts or resolves.
        assert!(crate::scm::detect_conflict(&work).is_some());
        crate::scm::abort_conflict(&work, crate::scm::ConflictKind::Rebase).unwrap();
    }

    #[test]
    fn auto_sync_skips_unpublished_and_detached_heads() {
        let d = temp_dir("autosync-heads");
        let (work, _consumer, _bare) = seed_remote(&d.0);

        // A local branch with no upstream.
        git(&work, &["checkout", "--quiet", "-b", "lonely"]);
        assert_eq!(
            auto_sync(&work, &dummy_url()).unwrap(),
            AutoSyncOutcome::SkippedNoUpstream
        );

        // Detached HEAD.
        git(&work, &["checkout", "--quiet", "--detach"]);
        let commit_before = head_commit(&work);
        assert_eq!(
            auto_sync(&work, &dummy_url()).unwrap(),
            AutoSyncOutcome::SkippedDetached
        );
        assert_eq!(head_commit(&work), commit_before);
    }
}
