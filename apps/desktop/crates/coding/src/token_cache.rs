//! Process-wide installation-token cache: every desktop consumer of a JIT
//! GitHub-App token (launcher prepare, GitBar sync worker, Source Control's
//! Commit & Push, the mid-session refresher) mints through here — one token
//! per repo at a time, re-minted only when its REAL remaining life (the
//! server returns GitHub's actual `expires_at` since EXP-73) falls under the
//! caller's margin ([`clone_manager::token_needs_remint_with_margin`]).
//!
//! In-memory only — the token is NEVER persisted or logged; it lives inside
//! [`TokenUrl`] (Display/Debug-redacted) plus this map, and reaches disk only
//! as the clone's credential file ([`crate::git_credentials`], 0600). The
//! token no longer rides `remote.origin.url` (EXP-73).
//!
//! EXP-712: the cached entry also carries the mint's `default_branch`, which
//! is BOARD-scoped when the launch named a board — so the key is
//! (repository, board), never the repository alone. Serving a repo-level
//! branch to a board launch would cut its worktree from the wrong base.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use api::error::ApiError;
use api::trpc::TrpcClient;

use crate::clone_manager::{token_needs_remint_with_margin, TOKEN_REMINT_MARGIN};
use crate::git_worktree::TokenUrl;

/// One minted installation token, keyed by `repository_id` in the cache.
/// Debug is safe: [`TokenUrl`]'s Debug is redacted, and the raw token lives
/// nowhere else in the struct.
#[derive(Clone, Debug)]
pub struct MintedToken {
    /// The token-embedded remote URL (redacted Display/Debug).
    pub url: TokenUrl,
    /// The base branch the mint resolved: the BOARD's branch when the mint
    /// named one (EXP-712), else the repo's live default.
    pub default_branch: String,
    /// ISO-8601 expiry from the server (`None` ⇒ treated as spent).
    pub expires_at: Option<String>,
}

/// The cache key: the repository, plus the board whose branch the entry's
/// `default_branch` answers for (EXP-712). `\u{1f}` (unit separator) can
/// appear in neither id, so the two halves can never collide.
fn cache_key(repository_id: &str, board_id: Option<&str>) -> String {
    match board_id {
        Some(board_id) => format!("{repository_id}\u{1f}{board_id}"),
        None => repository_id.to_string(),
    }
}

/// The in-memory token cache (see the module doc). Keyed by
/// [`cache_key`] — one entry per (repository, board).
#[derive(Default)]
pub struct TokenCache(Mutex<HashMap<String, MintedToken>>);

impl TokenCache {
    /// The cached token for `repository_id` when it does not need a re-mint
    /// (per-op margin [`TOKEN_REMINT_MARGIN`]); else mint a fresh one via
    /// `repositories.installationToken`, cache, and return it. Blocking
    /// (network) — run on a background executor.
    pub fn get_or_mint(
        &self,
        trpc: &TrpcClient,
        repository_id: &str,
        board_id: Option<&str>,
    ) -> Result<MintedToken, ApiError> {
        self.get_or_mint_with_margin(trpc, repository_id, board_id, TOKEN_REMINT_MARGIN)
    }

    /// [`TokenCache::get_or_mint`] with an explicit freshness margin: a hit
    /// must have MORE than `margin` of real life left. The refresher passes
    /// its longer lead so a token it is about to install outlives the gap to
    /// its next scheduled run.
    pub fn get_or_mint_with_margin(
        &self,
        trpc: &TrpcClient,
        repository_id: &str,
        board_id: Option<&str>,
        margin: Duration,
    ) -> Result<MintedToken, ApiError> {
        let key = cache_key(repository_id, board_id);
        if let Some(hit) = self
            .0
            .lock()
            .expect("token cache lock")
            .get(&key)
            .filter(|entry| {
                !token_needs_remint_with_margin(
                    entry.expires_at.as_deref(),
                    SystemTime::now(),
                    margin,
                )
            })
        {
            return Ok(hit.clone());
        }
        let token = api::repositories::installation_token(trpc, repository_id, board_id)?;
        let minted = MintedToken {
            url: TokenUrl::new(token.full_name, token.token),
            default_branch: token.default_branch,
            expires_at: token.expires_at,
        };
        self.0
            .lock()
            .expect("token cache lock")
            .insert(key, minted.clone());
        Ok(minted)
    }
}

/// The process-wide cache (one per app — tokens are account-agnostic here
/// because `repository_id`s are instance-unique).
pub fn token_cache() -> &'static TokenCache {
    static CACHE: OnceLock<TokenCache> = OnceLock::new();
    CACHE.get_or_init(TokenCache::default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::StaticToken;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::time::Duration;

    // Canned tRPC server (mirrors token_refresh.rs's harness): serves the
    // given responses in order, one connection each.
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

    fn token_json(token: &str, expires_at: &str) -> String {
        format!(
            r#"{{"result":{{"data":{{"token":"{token}","fullName":"acme/web","defaultBranch":"main","expiresAt":"{expires_at}"}}}}}}"#
        )
    }

    #[test]
    fn fresh_hit_skips_the_second_mint() {
        // ONE canned response: a second network round-trip would hang/fail,
        // so two successful calls prove the cache hit.
        let base = canned_server(vec![(200, token_json("ghs_one", "2099-01-01T00:00:00.000Z"))]);
        let cache = TokenCache::default();

        let first = cache.get_or_mint(&client(&base), "repo-cache-hit", None).unwrap();
        assert_eq!(first.default_branch, "main");
        assert_eq!(first.url.redacted(), "https://x-access-token:***@github.com/acme/web.git");

        let second = cache.get_or_mint(&client(&base), "repo-cache-hit", None).unwrap();
        assert_eq!(second.expires_at.as_deref(), Some("2099-01-01T00:00:00.000Z"));
    }

    #[test]
    fn expired_entry_re_mints() {
        let base = canned_server(vec![
            (200, token_json("ghs_stale", "2020-01-01T00:00:00.000Z")), // already expired
            (200, token_json("ghs_fresh", "2099-01-01T00:00:00.000Z")),
        ]);
        let cache = TokenCache::default();

        let stale = cache.get_or_mint(&client(&base), "repo-expiring", None).unwrap();
        assert_eq!(stale.expires_at.as_deref(), Some("2020-01-01T00:00:00.000Z"));

        // The stale entry fails token_needs_remint → second call mints anew.
        let fresh = cache.get_or_mint(&client(&base), "repo-expiring", None).unwrap();
        assert_eq!(fresh.expires_at.as_deref(), Some("2099-01-01T00:00:00.000Z"));
    }

    #[test]
    fn a_wider_margin_re_mints_what_the_default_margin_would_serve() {
        let base = canned_server(vec![
            (200, token_json("ghs_first", "2099-01-01T00:00:00.000Z")),
            (200, token_json("ghs_second", "2099-06-01T00:00:00.000Z")),
        ]);
        let cache = TokenCache::default();

        // Comfortably fresh under the default per-op margin → cache hit.
        let first = cache.get_or_mint(&client(&base), "repo-margin", None).unwrap();
        let hit = cache.get_or_mint(&client(&base), "repo-margin", None).unwrap();
        assert_eq!(first.expires_at, hit.expires_at);

        // A margin wider than the token's remaining life forces a re-mint —
        // the refresher's stricter freshness demand.
        let wide = Duration::from_secs(60 * 60 * 24 * 365 * 200); // ≫ 2099
        let fresh = cache
            .get_or_mint_with_margin(&client(&base), "repo-margin", None, wide)
            .unwrap();
        assert_eq!(fresh.expires_at.as_deref(), Some("2099-06-01T00:00:00.000Z"));
    }

    /// EXP-712: a repo-level entry must NEVER answer a board launch — its
    /// `default_branch` is the repo's, and cutting the board's worktree from
    /// it is the whole bug. Same repo, different board ⇒ separate entries.
    #[test]
    fn a_board_launch_never_inherits_the_repo_level_branch() {
        let base = canned_server(vec![
            (
                200,
                format!(
                    r#"{{"result":{{"data":{{"token":"ghs_repo","fullName":"acme/web","defaultBranch":"master","expiresAt":"2099-01-01T00:00:00.000Z"}}}}}}"#
                ),
            ),
            (
                200,
                format!(
                    r#"{{"result":{{"data":{{"token":"ghs_board","fullName":"acme/web","defaultBranch":"develop","expiresAt":"2099-01-01T00:00:00.000Z"}}}}}}"#
                ),
            ),
        ]);
        let cache = TokenCache::default();
        let client = client(&base);

        // The repo-level mint (an action/chat run) seeds the cache.
        let repo_level = cache.get_or_mint(&client, "repo-boards", None).unwrap();
        assert_eq!(repo_level.default_branch, "master");

        // A board launch on the SAME repo misses it and mints its own.
        let board = cache
            .get_or_mint(&client, "repo-boards", Some("board-1"))
            .unwrap();
        assert_eq!(board.default_branch, "develop");

        // Both entries now serve their own key without another round trip
        // (the canned server has no third response — a mint would fail).
        assert_eq!(
            cache
                .get_or_mint(&client, "repo-boards", Some("board-1"))
                .unwrap()
                .default_branch,
            "develop"
        );
        assert_eq!(
            cache
                .get_or_mint(&client, "repo-boards", None)
                .unwrap()
                .default_branch,
            "master"
        );
    }

    #[test]
    fn denied_mint_surfaces_the_api_error_and_caches_nothing() {
        let base = canned_server(vec![
            (403, r#"{"error":{"message":"You are not a member of this team","code":-32003,"data":{"code":"FORBIDDEN","httpStatus":403}}}"#.to_string()),
        ]);
        let cache = TokenCache::default();
        let err = cache.get_or_mint(&client(&base), "repo-denied", None).unwrap_err();
        assert!(err.to_string().contains("not a member"), "{err}");
        assert!(cache.0.lock().unwrap().is_empty());
    }
}
