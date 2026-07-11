//! Process-wide installation-token cache: the GitBar sync worker and Source
//! Control's Commit & Push both need a JIT GitHub-App token per network op,
//! and unconditionally re-minting one per click/timer tick is wasteful (the
//! token lives ~55 minutes). [`TokenCache::get_or_mint`] returns the cached
//! token while it is comfortably fresh ([`clone_manager::token_needs_remint`])
//! and re-mints through `repositories.installationToken` otherwise.
//!
//! In-memory only — the token is NEVER persisted or logged; it lives inside
//! [`TokenUrl`] (Display/Debug-redacted) plus this map. The mid-session
//! 40-minute remote-refresh loop stays on [`crate::token_refresh`] — that one
//! must re-embed a FRESH token into a clone's remote unconditionally.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use api::error::ApiError;
use api::trpc::TrpcClient;

use crate::clone_manager::token_needs_remint;
use crate::git_worktree::TokenUrl;

/// One minted installation token, keyed by `repository_id` in the cache.
/// Debug is safe: [`TokenUrl`]'s Debug is redacted, and the raw token lives
/// nowhere else in the struct.
#[derive(Clone, Debug)]
pub struct MintedToken {
    /// The token-embedded remote URL (redacted Display/Debug).
    pub url: TokenUrl,
    /// The repo's live default branch, as the mint resolved it.
    pub default_branch: String,
    /// ISO-8601 expiry from the server (`None` ⇒ treated as spent).
    pub expires_at: Option<String>,
}

/// The in-memory token cache (see the module doc).
#[derive(Default)]
pub struct TokenCache(Mutex<HashMap<String, MintedToken>>);

impl TokenCache {
    /// The cached token for `repository_id` when it does not need a re-mint;
    /// else mint a fresh one via `repositories.installationToken`, cache, and
    /// return it. Blocking (network) — run on a background executor.
    pub fn get_or_mint(
        &self,
        trpc: &TrpcClient,
        repository_id: &str,
    ) -> Result<MintedToken, ApiError> {
        if let Some(hit) = self
            .0
            .lock()
            .expect("token cache lock")
            .get(repository_id)
            .filter(|entry| !token_needs_remint(entry.expires_at.as_deref(), SystemTime::now()))
        {
            return Ok(hit.clone());
        }
        let token = api::repositories::installation_token(trpc, repository_id)?;
        let minted = MintedToken {
            url: TokenUrl::new(token.full_name, token.token),
            default_branch: token.default_branch,
            expires_at: token.expires_at,
        };
        self.0
            .lock()
            .expect("token cache lock")
            .insert(repository_id.to_string(), minted.clone());
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

        let first = cache.get_or_mint(&client(&base), "repo-cache-hit").unwrap();
        assert_eq!(first.default_branch, "main");
        assert_eq!(first.url.redacted(), "https://x-access-token:***@github.com/acme/web.git");

        let second = cache.get_or_mint(&client(&base), "repo-cache-hit").unwrap();
        assert_eq!(second.expires_at.as_deref(), Some("2099-01-01T00:00:00.000Z"));
    }

    #[test]
    fn expired_entry_re_mints() {
        let base = canned_server(vec![
            (200, token_json("ghs_stale", "2020-01-01T00:00:00.000Z")), // already expired
            (200, token_json("ghs_fresh", "2099-01-01T00:00:00.000Z")),
        ]);
        let cache = TokenCache::default();

        let stale = cache.get_or_mint(&client(&base), "repo-expiring").unwrap();
        assert_eq!(stale.expires_at.as_deref(), Some("2020-01-01T00:00:00.000Z"));

        // The stale entry fails token_needs_remint → second call mints anew.
        let fresh = cache.get_or_mint(&client(&base), "repo-expiring").unwrap();
        assert_eq!(fresh.expires_at.as_deref(), Some("2099-01-01T00:00:00.000Z"));
    }

    #[test]
    fn denied_mint_surfaces_the_api_error_and_caches_nothing() {
        let base = canned_server(vec![
            (403, r#"{"error":{"message":"You are not a member of this workspace","code":-32003,"data":{"code":"FORBIDDEN","httpStatus":403}}}"#.to_string()),
        ]);
        let cache = TokenCache::default();
        let err = cache.get_or_mint(&client(&base), "repo-denied").unwrap_err();
        assert!(err.to_string().contains("not a member"), "{err}");
        assert!(cache.0.lock().unwrap().is_empty());
    }
}
