//! Trunk clone lifecycle (masterplan v4 §4.1, L12): auto-clone on project
//! open, freshness fetches, and pull/push — all against the **trunk** clone
//! (`<repos_root>/<owner>/<name>`), reusing [`crate::git_worktree`]'s
//! `clone_path` / `set_token_remote` / [`TokenUrl`] redaction.
//!
//! Contract (v4 §4.1):
//! * **Auto-clone on project open** when `<clone>/.git` is missing: mint a JIT
//!   token, clone, then a fetch. Progress surfaces via [`CloneEvent`] (parsed
//!   from `git clone --progress` stderr → the git-bar chip). `git clone`
//!   streams progress on stderr with `\r`-overwritten phase lines, so [`ensure`]
//!   spawns git itself (rather than reusing `git_worktree::ensure_clone`, which
//!   blocks on `.output()` and cannot surface a percentage) and parses each
//!   segment via [`parse_clone_progress`].
//! * **Freshness**: fetch on project open, after every pull/push, and on
//!   window focus with a ≥5-minute debounce ([`should_fetch`]). The debounce
//!   and the actual re-mint live in the caller (gpui foreground owns the timer
//!   + the trpc client); this module is the git side.
//! * **Token re-mint**: the ~55-min installation token is disposable. The
//!   caller checks [`token_needs_remint`] before each network op and, when it
//!   is within [`TOKEN_REMINT_MARGIN`] of expiry, re-mints
//!   (`repositories.installationToken`) and passes the fresh [`TokenUrl`]; the
//!   network wrappers here always [`set_token_remote`] first so the freshly
//!   minted token is installed before git touches the remote.
//! * **Ahead/behind** = `git rev-list --left-right --count <branch>...origin/<branch>`
//!   after a fetch — no network for the counts themselves ([`ahead_behind`]).
//! * **Pull** = fetch + `git pull --rebase --autostash`, respecting an explicit
//!   `pull.rebase=false` (→ merge). **Push** = fetch → auto-rebase if behind →
//!   push. Conflicts never auto-abort (no `--abort` is ever issued here): git's
//!   markers are left in place and the trunk enters conflict mode (§4.4),
//!   re-derived from disk by `crate::scm::detect_conflict`.
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

use crate::git_worktree::{clone_path, run_git, set_token_remote, GitError, TokenUrl};

/// Freshness debounce (v4 §4.1): focus-triggered fetches coalesce to at most
/// one per five minutes. Project-open and post-transport fetches are not
/// debounced — only the focus path calls [`should_fetch`].
pub const FETCH_DEBOUNCE: Duration = Duration::from_secs(5 * 60);

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

/// Whether a focus-triggered fetch is due: at least [`FETCH_DEBOUNCE`] has
/// elapsed since the last fetch (v4 §4.1). The caller holds `last` (the
/// `Instant` of its previous fetch) and calls this on window focus.
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
    match expires_at.and_then(parse_iso8601_utc) {
        Some(expiry) => match expiry.duration_since(now) {
            Ok(remaining) => remaining < TOKEN_REMINT_MARGIN,
            Err(_) => true, // now >= expiry → already expired
        },
        None => true, // unknown/unparseable expiry → re-mint to be safe
    }
}

/// Parse the server's ISO-8601 UTC form (`2026-07-03T12:55:00.000Z`, with or
/// without fractional seconds) into a [`SystemTime`]. `None` on any deviation
/// from that shape — callers treat `None` as "re-mint" (fail safe). Pure: no
/// external time crate (chrono is not a desktop dependency), uses Howard
/// Hinnant's `days_from_civil`.
fn parse_iso8601_utc(raw: &str) -> Option<SystemTime> {
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
    on_event: CloneProgress<'_>,
) -> Result<PathBuf, GitError> {
    let clone = clone_path(repos_root, full_name);
    if clone.join(".git").exists() {
        on_event(CloneEvent::Done); // reuse — §7.1 idempotent relaunch
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

/// `git fetch origin` against the trunk clone (freshness). Re-sets the token
/// remote first (v4 §4.1: the caller may have re-minted; the ~55-min token is
/// disposable).
pub fn fetch(clone: &Path, url: &TokenUrl) -> Result<(), GitError> {
    set_token_remote(clone, url)?;
    run_git(Some(clone), &["fetch", "origin"], Some(url), "git fetch origin")?;
    Ok(())
}

/// Pull the trunk's default branch: `git pull --rebase --autostash`, unless the
/// user set `pull.rebase=false` (→ `--no-rebase`, a merge). A conflict leaves
/// git's markers in place and returns the error — the caller re-derives
/// conflict mode from disk (v4 §4.1/§4.4); nothing here aborts.
///
/// Re-sets the token remote first (the ~55-min token is disposable) but does
/// NOT pre-fetch: `git pull` fetches as its first step, so an explicit
/// [`fetch`] here would be a redundant second network round-trip.
pub fn pull(clone: &Path, default_branch: &str, url: &TokenUrl) -> Result<(), GitError> {
    set_token_remote(clone, url)?;
    let rebase_arg = if pull_rebase_disabled(clone) {
        "--no-rebase" // respect an explicit `pull.rebase=false` → merge
    } else {
        "--rebase"
    };
    run_git(
        Some(clone),
        &["pull", rebase_arg, "--autostash", "origin", default_branch],
        Some(url),
        "git pull",
    )?;
    Ok(())
}

/// Push the trunk's default branch: fetch → auto-rebase onto
/// `origin/<default_branch>` if behind → push (v4 §4.1). A rebase conflict
/// leaves markers in place and returns the error (no auto-abort).
pub fn push(clone: &Path, default_branch: &str, url: &TokenUrl) -> Result<(), GitError> {
    fetch(clone, url)?;
    // Local counts only (the fetch above already refreshed origin/<branch>);
    // a missing upstream ref just means "not behind" → push creates it.
    let (_ahead, behind) = ahead_behind(clone, default_branch).unwrap_or((0, 0));
    if behind > 0 {
        run_git(
            Some(clone),
            &["rebase", "--autostash", &format!("origin/{default_branch}")],
            Some(url),
            "git rebase origin",
        )?;
    }
    run_git(Some(clone), &["push", "origin", default_branch], Some(url), "git push")?;
    Ok(())
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

/// Whether `pull.rebase` is explicitly `false` (→ merge). Unset or `true` →
/// rebase. `--bool` normalizes the value; a non-zero exit means unset.
fn pull_rebase_disabled(clone: &Path) -> bool {
    let output = Command::new("git")
        .args(["config", "--bool", "--get", "pull.rebase"])
        .current_dir(clone)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output();
    matches!(
        output,
        Ok(out) if out.status.success()
            && String::from_utf8_lossy(&out.stdout).trim() == "false"
    )
}

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
}
