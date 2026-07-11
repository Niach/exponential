//! Mid-session installation-token refresh (EXP-56 P9): the JIT GitHub-App
//! token embedded in a clone's `origin` remote lives ~55 minutes, so any
//! session outliving it — a release orchestrator fanning out subagents, or a
//! long single-issue run — loses `git push` mid-flight. The fix is the same
//! call the launcher makes at step 3, repeated: mint a fresh token and re-set
//! the remote.
//!
//! Pure blocking half only (network + git; gpui-free). The scheduling —
//! per-clone ref-counted 40-minute loops — lives in the ui layer
//! (`ui::coding_flow::TokenRefreshers`), matching the crate's
//! background/foreground split. Because the remote is repo-level config
//! shared by every linked worktree, ONE refresh per clone covers the main
//! worktree and every subagent worktree at once.

use std::path::Path;

use api::repositories;
use api::trpc::TrpcClient;

use crate::git_worktree::{set_token_remote, TokenUrl};
use crate::launcher::CodingError;

/// Mint a fresh installation token for `repository_id` and re-embed it in
/// `clone`'s `origin` remote — exactly the launcher's step 2+3 remote reset,
/// callable mid-session. Blocking; run on a background thread/executor.
pub fn refresh_clone_token(
    trpc: &TrpcClient,
    repository_id: &str,
    clone: &Path,
) -> Result<(), CodingError> {
    let token = repositories::installation_token(trpc, repository_id)?;
    let url = TokenUrl::new(token.full_name, token.token);
    set_token_remote(clone, &url)?;
    Ok(())
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

    const TOKEN_OK: &str = r#"{"result":{"data":{"token":"ghs_fresh456","fullName":"acme/web","defaultBranch":"main","expiresAt":"2026-07-11T12:55:00.000Z"}}}"#;

    #[test]
    fn refresh_sets_a_fresh_token_remote_on_a_real_clone() {
        let dir = temp_dir("happy");
        let clone = seed_clone(&dir.0);
        let base = canned_server(vec![(200, TOKEN_OK.to_string())]);

        refresh_clone_token(&client(&base), "repo-1", &clone).unwrap();

        let remote = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(&clone)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&remote.stdout).trim(),
            "https://x-access-token:ghs_fresh456@github.com/acme/web.git"
        );
    }

    /// A denied mint surfaces as the api error (the ui loop logs + retries);
    /// the clone's remote is left untouched.
    #[test]
    fn denied_mint_leaves_the_remote_untouched() {
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

        match refresh_clone_token(&client(&base), "repo-1", &clone) {
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
    }
}
