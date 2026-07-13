//! Mid-session installation-token refresh (EXP-56 P9, reworked in EXP-73):
//! the JIT GitHub-App token in a clone's credential file lives ≤1 h, so any
//! session outliving it — a release orchestrator fanning out subagents, or a
//! long single-issue run — loses `git push` mid-flight. The refresh is the
//! same ambient-auth install the launcher runs at step 3, repeated:
//! cached-or-fresh mint + [`crate::git_credentials::ensure`] on the clone
//! (downgrade-guarded, so it composes with the git-bar's own writes).
//!
//! Pure blocking half only (network + git; gpui-free). The scheduling —
//! per-clone ref-counted loops paced by [`next_refresh_delay`] from the
//! token's REAL expiry (the server returns GitHub's actual `expires_at`;
//! a fixed 40-minute cadence used to outlive cache-served tokens) — lives in
//! the ui layer (`ui::coding_flow::TokenRefreshers`), matching the crate's
//! background/foreground split. The credential file sits in the clone's
//! shared `.git`, so ONE refresh per clone covers the main worktree and
//! every subagent worktree at once.

use std::path::Path;
use std::time::{Duration, SystemTime};

use api::trpc::TrpcClient;

use crate::clone_manager::parse_iso8601_utc;
use crate::git_credentials;
use crate::launcher::CodingError;
use crate::token_cache::MintedToken;

/// How much real token life the refresher demands, and how far before the
/// expiry the next refresh is scheduled. Strictly under the server's 10-min
/// cache serve margin (so a refresh-triggered mint is guaranteed genuinely
/// fresh) and strictly over the per-op
/// [`crate::clone_manager::TOKEN_REMINT_MARGIN`] (so a token the refresher
/// installs always satisfies the transport ops' own freshness check).
pub const REFRESH_LEAD: Duration = Duration::from_secs(8 * 60);

/// Backoff before retrying a failed refresh (also [`next_refresh_delay`]'s
/// answer for an absent/unparseable expiry).
pub const TOKEN_REFRESH_RETRY: Duration = Duration::from_secs(5 * 60);

/// Floor/ceiling for [`next_refresh_delay`]: never spin faster than once a
/// minute, never trust a claimed expiry further than 40 minutes out.
const REFRESH_DELAY_MIN: Duration = Duration::from_secs(60);
const REFRESH_DELAY_MAX: Duration = Duration::from_secs(40 * 60);

/// Ensure `clone` holds a token with at least [`REFRESH_LEAD`] of real life —
/// the launcher's step 2+3 ambient-auth install, callable mid-session.
/// Returns the minted token so the caller can schedule the next refresh from
/// its expiry. Blocking; run on a background thread/executor.
pub fn refresh_clone_token(
    trpc: &TrpcClient,
    repository_id: &str,
    clone: &Path,
) -> Result<MintedToken, CodingError> {
    git_credentials::ensure_repo_auth_with_margin(trpc, repository_id, clone, REFRESH_LEAD)
}

/// When to refresh next: `expires_at − REFRESH_LEAD` from `now`, clamped to
/// [1 min, 40 min]. An absent/unparseable expiry answers the retry backoff;
/// one already inside the lead (or past) clamps to the 1-min floor — refresh
/// soon, the token's real life is nearly spent. Pure (injected `now`) for
/// testability.
pub fn next_refresh_delay(expires_at: Option<&str>, now: SystemTime) -> Duration {
    let Some(expiry) = expires_at.and_then(parse_iso8601_utc) else {
        return TOKEN_REFRESH_RETRY;
    };
    match expiry.duration_since(now) {
        Ok(remaining) => remaining
            .saturating_sub(REFRESH_LEAD)
            .clamp(REFRESH_DELAY_MIN, REFRESH_DELAY_MAX),
        Err(_) => REFRESH_DELAY_MIN, // already expired — refresh now-ish
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::StaticToken;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::Arc;
    use std::time::Duration;

    // ---- harness (mirrors release_launcher.rs's canned server +
    //      git_worktree.rs's seed_origin) ----

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
        path.push(format!(
            "exp-coding-refresh-{tag}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn canned_server(responses: Vec<(u16, String)>) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        std::thread::spawn(move || {
            for (status, body) in responses {
                let Ok((mut stream, _)) = listener.accept() else { return };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let (mut head_end, mut content_length) = (None::<usize>, 0usize);
                while let Ok(n) = stream.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if head_end.is_none() {
                        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            head_end = Some(pos + 4);
                            let head = String::from_utf8_lossy(&buf[..pos]);
                            content_length = head
                                .lines()
                                .find_map(|line| {
                                    let (name, value) = line.split_once(':')?;
                                    name.eq_ignore_ascii_case("content-length")
                                        .then(|| value.trim().parse().ok())?
                                })
                                .unwrap_or(0);
                        }
                    }
                    if let Some(pos) = head_end {
                        if buf.len() >= pos + content_length {
                            break;
                        }
                    }
                }
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        base
    }

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
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
        assert!(output.status.success(), "git {args:?} failed");
    }

    /// A local "origin" repo with one commit, cloned to `clone` — a real repo
    /// with a real `origin` remote for `set_token_remote` to rewrite.
    fn seed_clone(dir: &Path) -> PathBuf {
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
        clone
    }

    // NOTE: a PAST expiry — refresh_clone_token mints through the
    // process-GLOBAL token cache, so fixtures must never be cache-servable
    // (and each test uses a unique repository id).
    const TOKEN_OK: &str = r#"{"result":{"data":{"token":"ghs_fresh456","fullName":"acme/web","defaultBranch":"main","expiresAt":"2026-07-11T12:55:00.000Z"}}}"#;

    #[test]
    fn refresh_installs_ambient_auth_on_a_real_clone() {
        let dir = temp_dir("happy");
        let clone = seed_clone(&dir.0);
        let base = canned_server(vec![(200, TOKEN_OK.to_string())]);

        let minted =
            refresh_clone_token(&client(&base), "repo-refresh-happy", &clone).unwrap();
        assert_eq!(minted.expires_at.as_deref(), Some("2026-07-11T12:55:00.000Z"));

        // EXP-73: origin stays BARE; the token lands in the credential file.
        let remote = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(&clone)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&remote.stdout).trim(),
            "https://github.com/acme/web.git"
        );
        let credentials =
            fs::read_to_string(git_credentials::credential_file(&clone)).unwrap();
        assert_eq!(credentials, "username=x-access-token\npassword=ghs_fresh456\n");
    }

    /// A denied mint surfaces as the api error (the ui loop logs + retries);
    /// the clone's remote and credentials are left untouched.
    #[test]
    fn denied_mint_leaves_the_clone_untouched() {
        let dir = temp_dir("denied");
        let clone = seed_clone(&dir.0);
        let before = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(&clone)
            .output()
            .unwrap();
        let base = canned_server(vec![(
            403,
            r#"{"error":{"message":"You are not a member of this workspace","code":-32003,"data":{"code":"FORBIDDEN","httpStatus":403}}}"#.to_string(),
        )]);

        match refresh_clone_token(&client(&base), "repo-refresh-denied", &clone) {
            Err(CodingError::Api(err)) => {
                assert!(err.to_string().contains("not a member"), "{err}");
            }
            other => panic!("expected Api error, got {other:?}"),
        }
        let after = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(&clone)
            .output()
            .unwrap();
        assert_eq!(before.stdout, after.stdout);
        assert!(!git_credentials::credential_file(&clone).exists());
    }

    // ---- next_refresh_delay (pure) ----

    #[test]
    fn next_refresh_delay_leads_the_real_expiry() {
        // `now` == 2026-07-03T12:00:00Z.
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_783_080_000);

        // A full-life token (60 min out): refresh at expiry − lead = 52 min,
        // clamped to the 40-min ceiling.
        assert_eq!(
            next_refresh_delay(Some("2026-07-03T13:00:00Z"), now),
            REFRESH_DELAY_MAX
        );
        // 30 min out → 22 min.
        assert_eq!(
            next_refresh_delay(Some("2026-07-03T12:30:00Z"), now),
            Duration::from_secs(22 * 60)
        );
        // Inside the lead (5 min out) → the 1-min floor.
        assert_eq!(
            next_refresh_delay(Some("2026-07-03T12:05:00Z"), now),
            REFRESH_DELAY_MIN
        );
        // Already expired → the floor.
        assert_eq!(
            next_refresh_delay(Some("2026-07-03T11:00:00Z"), now),
            REFRESH_DELAY_MIN
        );
        // Absent/unparseable → the retry backoff.
        assert_eq!(next_refresh_delay(None, now), TOKEN_REFRESH_RETRY);
        assert_eq!(next_refresh_delay(Some("whenever"), now), TOKEN_REFRESH_RETRY);
    }
}
