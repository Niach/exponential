//! Shared test harness for the launcher tests: temp dirs, a canned one-shot
//! tRPC server, a fake [`WorktreeProvider`], ready-made [`CodingDeps`], and
//! the canned response bodies both the issue and batch prepare paths
//! consume. `#[cfg(test)]`-only — never compiled into the crate proper.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use api::token_store::{SecretKind, TokenStore};
use api::trpc::TrpcClient;
use api::StaticToken;

use crate::git_worktree::{GitError, TokenUrl};
use crate::launcher::{CodingDeps, IssueSeed, WorktreeProvider};
use crate::settings::Settings;

pub(crate) struct TempDir(pub PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn temp_dir(tag: &str) -> TempDir {
    let mut path = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    path.push(format!(
        "exp-coding-launch-{tag}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    TempDir(path)
}

/// Serve a fixed sequence of canned responses, one connection each
/// (`Connection: close`), in request order.
pub(crate) fn canned_server(responses: Vec<(u16, String)>) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for (status, body) in responses {
            let Ok((mut stream, _)) = listener.accept() else { return };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            // Drain head + any Content-Length body.
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

/// A fake §7.1-step-3 provider: hands back a pre-made temp worktree and
/// records the (full_name, default_branch, branch, expires_at) it was asked
/// for.
pub(crate) struct FakeWorktrees {
    pub worktree: PathBuf,
    pub seen: std::sync::Mutex<Vec<(String, String, String, Option<String>)>>,
}

impl WorktreeProvider for FakeWorktrees {
    fn prepare(
        &self,
        _repos_root: &Path,
        full_name: &str,
        default_branch: &str,
        branch: &str,
        _url: &TokenUrl,
        expires_at: Option<&str>,
    ) -> Result<PathBuf, GitError> {
        self.seen.lock().unwrap().push((
            full_name.to_string(),
            default_branch.to_string(),
            branch.to_string(),
            expires_at.map(str::to_string),
        ));
        Ok(self.worktree.clone())
    }
}

/// Deps with: doctor guaranteed green (claude_path = `git` — a real binary
/// answering `--version`; its version line never parses as a claude triple,
/// so the version gate stays open), key pre-seeded (no mint traffic), a fake
/// worktree provider, and a canned tRPC server.
pub(crate) fn make_deps(base: &str, data_dir: &Path, worktrees: Arc<FakeWorktrees>) -> CodingDeps {
    let store = TokenStore::file_only(data_dir.to_path_buf());
    store
        .set("acct", SecretKind::PersonalApiKey, "expu_seeded")
        .unwrap();
    CodingDeps {
        trpc: Arc::new(TrpcClient::new(base, Arc::new(StaticToken("tok".into())))),
        token_store: Arc::new(store),
        account_id: "acct".to_string(),
        settings: Settings {
            claude_path: "git".to_string(),
            repos_root: data_dir.join("repos").to_string_lossy().into_owned(),
            branch_prefix: "exp/".to_string(),
            ..Settings::default()
        },
        issue_seed: Arc::new(|_| {
            Some(IssueSeed {
                title: "Fix login flicker".to_string(),
                description: Some("Steps in the issue.".to_string()),
            })
        }),
        worktrees,
    }
}

pub(crate) const FOR_ISSUE_OK: &str = r#"{"result":{"data":{"repositoryId":"repo-1","fullName":"acme/web","defaultBranch":"main"}}}"#;
pub(crate) const TOKEN_OK: &str = r#"{"result":{"data":{"token":"ghs_secret123","fullName":"acme/web","defaultBranch":"main","expiresAt":"2026-07-03T12:55:00.000Z"}}}"#;
pub(crate) const START_OK: &str = r#"{"result":{"data":{"session":{"id":"sess-1","issueId":"issue-1","status":"running"}}}}"#;
pub(crate) const START_BATCH_OK: &str = r#"{"result":{"data":{"session":{"id":"sess-b","issueId":null,"teamId":"ws-1","status":"running"}}}}"#;
pub(crate) const MINT_OK: &str = r#"{"result":{"data":{"key":"expu_minted_runtime","id":"key-9","name":"Device: box","start":"expu_mi","prefix":"expu_","createdAt":"2026-07-03T10:00:00.000Z"}}}"#;
