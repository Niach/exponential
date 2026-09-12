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
    canned_server_recording(responses).0
}

/// [`canned_server`] that also records each request's raw head+body (the
/// tRPC procedure rides the request path, e.g. `POST /api/trpc/issues.update`)
/// so a test can assert WHICH calls the launcher made, in order.
pub(crate) fn canned_server_recording(
    responses: Vec<(u16, String)>,
) -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = Arc::clone(&requests);
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
            recorded
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf).into_owned());
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (base, requests)
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
        // Like `git worktree add`, the returned path exists afterwards — a
        // resume into a RECLAIMED worktree relies on that.
        let _ = fs::create_dir_all(&self.worktree);
        Ok(self.worktree.clone())
    }
}

/// Deps with: doctor guaranteed green (claude_path = `git` — a real binary
/// answering `--version`; its version line never parses as a claude triple,
/// so the version gate stays open), key pre-seeded (no mint traffic), a fake
/// worktree provider, and a canned tRPC server.
/// EXP-773: a stub agent CLI that answers `--version` with an ACP-READY
/// version and exits 0 for everything else. The engine is the only coding
/// transport now, so a launch is REFUSED unless the doctor reports
/// `acp: Some(true)` — every prepare fixture needs one of these rather than
/// the bare `git` the old PTY fallback tolerated. Unix-only (desktop tests
/// run on Linux, `.github/workflows/test.yml`).
#[cfg(unix)]
pub(crate) fn acp_ready_stub(data_dir: &Path, name: &str, version: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let stub = data_dir.join("bin").join(name);
    fs::create_dir_all(stub.parent().unwrap()).unwrap();
    // `--version` answers the doctor's version gate; anything else reads one
    // line and answers a line-delimited JSON handshake (harmless for the
    // claude/codex stubs, which are never probed that way).
    fs::write(
        &stub,
        format!(
            "#!/bin/sh\ncase \"$1\" in\n--version) echo '{version}';;\n*) read line\n\
echo '{{\"id\":\"1\",\"type\":\"response\",\"command\":\"get_state\",\"success\":true}}';;\nesac\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    wait_until_executable(&stub);
    stub.to_string_lossy().into_owned()
}

/// EXP-781: probe a freshly written stub until exec'ing it stops answering
/// `ETXTBSY`, then hand it out.
///
/// Every `make_deps` writes three of these, and ~30 tests run concurrently.
/// A sibling thread's `Command::spawn` forks between THIS thread's
/// `fs::write` opening the stub and closing it; the forked child inherits the
/// write fd and holds it until its own exec, and for that window the kernel
/// calls the file busy — so the exec our own test does next fails with
/// `ETXTBSY` (Linux; macOS does not enforce this).
///
/// A probe-and-retry NARROWS the window rather than closing it: the fix that
/// would close it is not forking with the fd open, which is `Command`'s
/// business, not ours. In practice one short backoff is enough — the sibling
/// exec is microseconds away.
#[cfg(unix)]
pub(crate) fn wait_until_executable(stub: &Path) {
    use std::process::{Command, Stdio};
    for attempt in 0..20 {
        let spawned = Command::new(stub)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match spawned {
            Ok(_) => return,
            Err(err) if err.raw_os_error() == Some(libc::ETXTBSY) => {
                std::thread::sleep(std::time::Duration::from_millis(5 * (attempt + 1)));
            }
            // Anything else is not this race; let the caller's own exec
            // report it with its real context.
            Err(_) => return,
        }
    }
}

#[cfg(not(unix))]
pub(crate) fn acp_ready_stub(_data_dir: &Path, _name: &str, _version: &str) -> String {
    "git".to_string()
}

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
            // EXP-773: ACP-ready stubs — the engine is the only transport, so
            // an agent the doctor cannot vouch for cannot start at all.
            claude_path: acp_ready_stub(data_dir, "claude", "9.9.9 (Claude Code)"),
            codex_path: acp_ready_stub(data_dir, "codex", "9.9.9"),
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
        // No codex rollouts in the fixture: resume tests either inject a
        // fixture root or exercise the no-recorded-session fallback.
        codex_sessions_root: None,
        claude_projects_root: None,
        device_id: None,
        // EXP-746: the tests' host CAN run the engine.
        acp_available: true,
        data_dir: data_dir.to_path_buf(),
    }
}

pub(crate) const FOR_ISSUE_OK: &str = r#"{"result":{"data":{"repositoryId":"repo-1","fullName":"acme/web","defaultBranch":"main"}}}"#;
pub(crate) const TOKEN_OK: &str = r#"{"result":{"data":{"token":"ghs_secret123","fullName":"acme/web","defaultBranch":"main","expiresAt":"2026-07-03T12:55:00.000Z"}}}"#;
pub(crate) const START_OK: &str = r#"{"result":{"data":{"session":{"id":"sess-1","issueId":"issue-1","status":"running"}}}}"#;
pub(crate) const START_BATCH_OK: &str = r#"{"result":{"data":{"session":{"id":"sess-b","issueId":null,"teamId":"ws-1","status":"running"}}}}"#;
pub(crate) const START_ACTION_OK: &str = r#"{"result":{"data":{"session":{"id":"sess-a","issueId":null,"teamId":"ws-1","actionId":"act-1","actionName":"Code review","status":"running"}}}}"#;
pub(crate) const MINT_OK: &str = r#"{"result":{"data":{"key":"expu_minted_runtime","id":"key-9","name":"Device: box","start":"expu_mi","prefix":"expu_","createdAt":"2026-07-03T10:00:00.000Z"}}}"#;
pub(crate) const UPDATE_OK: &str = r#"{"result":{"data":{"issue":{"id":"issue-1","identifier":"EXP-42","status":"in_progress"}}}}"#;
