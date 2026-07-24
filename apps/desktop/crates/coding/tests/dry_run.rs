//! §11.4 Phase-5-core gate: the launcher dry-run — the FULL local Start-coding
//! sequence against a LOCAL bare git repo standing in for GitHub, a canned
//! tRPC server, and a stub `claude` (a shell script that reads its positional
//! prompt and exits 0). Proves, end-to-end and hermetically (no network, no
//! real GitHub App):
//!
//! 1. `prepare` with the REAL [`coding::GitWorktrees`] provider:
//!    `ensure_clone` (reuse — the one GitHub-bound step, pre-seeded from the
//!    local bare origin per the crate's hermetic test design) →
//!    `git_credentials::ensure` (bare origin + repo-local helper + token
//!    file, EXP-73) → `create_worktree` cutting `exp/GATE-99` from
//!    `origin/main` → `.git/info/exclude` covering `.exp-mcp.json`;
//! 2. `.exp-mcp.json` written into the worktree with the exact §7.1 contents
//!    (instance `/api/mcp` URL + `Bearer expu_…`) and the rendered issue
//!    prompt riding argv DIRECTLY (small prompt ⇒ no `PROMPT.md` on disk);
//! 3. the composed spawn spec (`<claude> --dangerously-skip-permissions`,
//!    cwd = worktree);
//! 4. `spawn_prepared` through a REAL headless-gpui `TerminalManager` tab:
//!    the stub runs in the worktree, echoes its positional prompt, exits 0 —
//!    and the launcher's one-shot exit hook fires `codingSessions.end` with
//!    the session id minted by `codingSessions.start`.
//!
//! Like `terminal/tests/exit_hook.rs`, this needs the process main thread for
//! the platform run loop (`harness = false`) and self-skips unless
//! `EXP_CODING_DRYRUN_E2E=1`:
//!
//! ```sh
//! EXP_CODING_DRYRUN_E2E=1 cargo test -p coding --test dry_run
//! ```

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use api::token_store::{SecretKind, TokenStore};
use api::trpc::TrpcClient;
use api::StaticToken;
use coding::{
    clone_path, prepare, spawn_prepared, worktree_path, CodingDeps, GitWorktrees, IssueSeed,
    LaunchOptions, LaunchOrigin, LaunchOutcome, LaunchRequest, Prepared, PrepareRequest,
    Settings,
};
use gpui::AppContext as _;
use terminal::TerminalManager;

// ---- harness ----

fn temp_dir(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    path.push(format!("exp-coding-dryrun-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
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

fn git_stdout(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git").args(args).current_dir(cwd).output().unwrap();
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Serve canned responses in request order (`Connection: close`), recording
/// each request's head + body so the watchdog can assert the end call.
fn canned_server(responses: Vec<(u16, String)>, log: Arc<Mutex<Vec<String>>>) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for (status, body) in responses {
            let Ok((mut stream, _)) = listener.accept() else { return };
            stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
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
            log.lock().unwrap().push(String::from_utf8_lossy(&buf).into_owned());
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    base
}

fn main() {
    if std::env::var("EXP_CODING_DRYRUN_E2E").as_deref() != Ok("1") {
        eprintln!(
            "dry_run e2e: skipped (set EXP_CODING_DRYRUN_E2E=1 to run — needs the process \
             main thread + a live platform run loop)"
        );
        return;
    }

    let root = temp_dir("root");

    // ---- the LOCAL "GitHub": a bare origin with one commit on main ----
    let src = root.join("origin-src");
    fs::create_dir_all(&src).unwrap();
    git(&src, &["init", "--quiet", "-b", "main"]);
    fs::write(src.join("README.md"), "seed\n").unwrap();
    git(&src, &["add", "."]);
    git(&src, &["commit", "--quiet", "-m", "seed"]);
    let bare = root.join("origin.git");
    git(&root, &["clone", "--quiet", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);

    // Pre-seed the clone at the §7.1 layout path from the bare origin —
    // the ONE GitHub-bound step (`git clone <token-url>`) substituted per the
    // crate's hermetic design; ensure_clone then takes its reuse path and
    // every later git op (remote reset, worktree cut, excludes) runs REAL.
    let repos_root = root.join("repos");
    let clone = clone_path(&repos_root, "acme/web");
    fs::create_dir_all(clone.parent().unwrap()).unwrap();
    git(&root, &["clone", "--quiet", bare.to_str().unwrap(), clone.to_str().unwrap()]);

    // ---- the stub `claude`: answers --version, echoes the first line of
    //      its positional prompt ($7 — direct delivery), exits 0 ----
    let stub = root.join("bin").join("claude-stub");
    fs::create_dir_all(stub.parent().unwrap()).unwrap();
    fs::write(
        &stub,
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then echo '9.9.9 (Claude Code stub)'; exit 0; fi\n\
         [ \"$1\" = \"--model\" ] || exit 7\n\
         [ \"$3\" = \"--mcp-config\" ] || exit 6\n\
         [ \"$6\" = \"--dangerously-skip-permissions\" ] || exit 8\n\
         printf '%s\\n' \"$7\" | head -n 1 > claude-ran.txt || exit 9\n\
         exit 0\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    }

    // ---- canned tRPC server: forIssue → installationToken → start → end ----
    let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let base = canned_server(
        vec![
            (200, r#"{"result":{"data":{"repositoryId":"repo-1","fullName":"acme/web","defaultBranch":"main"}}}"#.to_string()),
            (200, r#"{"result":{"data":{"token":"ghs_dryrun_dead","fullName":"acme/web","defaultBranch":"main","expiresAt":"2026-07-03T12:55:00.000Z"}}}"#.to_string()),
            (200, r#"{"result":{"data":{"session":{"id":"sess-dryrun-1","issueId":"issue-1","status":"running"}}}}"#.to_string()),
            (200, r#"{"result":{"data":{"session":{"id":"sess-dryrun-1","status":"ended"}}}}"#.to_string()),
        ],
        Arc::clone(&requests),
    );

    // ---- deps: real GitWorktrees, pre-seeded personal key (no mint) ----
    let data_dir = root.join("data");
    fs::create_dir_all(&data_dir).unwrap();
    let store = TokenStore::file_only(data_dir.clone());
    store
        .set("acct", SecretKind::PersonalApiKey, "expu_dryrun_key")
        .unwrap();
    let trpc = Arc::new(TrpcClient::new(&base, Arc::new(StaticToken("tok".into()))));
    let deps = CodingDeps {
        trpc: Arc::clone(&trpc),
        token_store: Arc::new(store),
        account_id: "acct".to_string(),
        settings: Settings {
            claude_path: stub.to_string_lossy().into_owned(),
            repos_root: repos_root.to_string_lossy().into_owned(),
            branch_prefix: "exp/".to_string(),
            ..Settings::default()
        },
        issue_seed: Arc::new(|_| {
            Some(IssueSeed {
                title: "Dry-run the launcher".to_string(),
                description: Some("Prove the full local sequence minus GitHub.".to_string()),
            })
        }),
        worktrees: Arc::new(GitWorktrees),
        codex_sessions_root: None,
        data_dir: data_dir.to_path_buf(),
    };
    // plan_mode OFF so the stub's `$6 = --dangerously-skip-permissions`
    // check holds; the prompt rides argv as $7 (direct delivery).
    let req = PrepareRequest::Issue(LaunchRequest {
        issue_id: "issue-1".to_string(),
        issue_identifier: "GATE-99".to_string(),
        // in_progress ⇒ no step-6.5 flip: the canned sequence stays exact.
        issue_status: domain::IssueStatus::InProgress,
        device_label: "dryrunbox".to_string(),
        origin: LaunchOrigin::Local,
        options: LaunchOptions {
            agent: coding::CodingAgent::Claude,
            model: "opus".to_string(),
            effort: "".to_string(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: true,
        },
        resume: false,
    });

    // ---- steps 0–6 (blocking, gpui-free) ----
    let prepared = match prepare(&req, &deps).expect("prepare") {
        Prepared::Ready(prepared) => prepared,
        Prepared::Disabled(reason) => panic!("unexpectedly disabled: {reason:?}"),
    };

    // Worktree at the §7.1 layout path, on the real branch, seeded from main.
    let expected_worktree = worktree_path(&clone, "exp/GATE-99");
    assert_eq!(prepared.session_id, "sess-dryrun-1");
    assert_eq!(prepared.branch, "exp/GATE-99");
    assert_eq!(prepared.worktree, expected_worktree);
    assert!(expected_worktree.join(".git").exists(), "worktree .git missing");
    assert!(expected_worktree.join("README.md").exists(), "seed file missing");
    assert_eq!(
        git_stdout(&expected_worktree, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "exp/GATE-99"
    );

    // git_credentials::ensure ran (EXP-73): origin is the BARE URL, the
    // repo-local helper pair is configured, and the token sits in the
    // credential file (0600) — never in the remote.
    assert_eq!(
        git_stdout(&clone, &["remote", "get-url", "origin"]),
        "https://github.com/acme/web.git"
    );
    let helpers = git_stdout(
        &clone,
        &[
            "config",
            "--local",
            "--get-all",
            "credential.https://github.com/acme/web.git.helper",
        ],
    );
    let helper_lines: Vec<&str> = helpers.lines().collect();
    assert_eq!(helper_lines.len(), 2, "reset + ours: {helpers}");
    assert_eq!(helper_lines[0], "");
    assert!(helper_lines[1].contains("exp-git-credentials"), "{helpers}");
    let credentials =
        fs::read_to_string(coding::git_credentials::credential_file(&clone)).unwrap();
    assert_eq!(
        credentials,
        "username=x-access-token\npassword=ghs_dryrun_dead\n"
    );

    // .exp-mcp.json: exact §7.1 step-4 contents, private perms.
    let mcp = fs::read_to_string(expected_worktree.join(".exp-mcp.json")).unwrap();
    assert!(mcp.contains(&format!("\"url\": \"{base}/api/mcp\"")), "mcp url: {mcp}");
    assert!(mcp.contains("\"Authorization\": \"Bearer expu_dryrun_key\""), "mcp key: {mcp}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(expected_worktree.join(".exp-mcp.json")).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, ".exp-mcp.json must be private");
    }

    // Direct delivery: the FULL rendered prompt is the positional argv and
    // there is NO PROMPT.md indirection on disk.
    assert!(!expected_worktree.join("PROMPT.md").exists(), "PROMPT.md must not exist");
    let prompt = prepared.spawn.args.last().unwrap().clone();
    assert!(prompt.contains("**GATE-99: Dry-run the launcher**"), "prompt: {prompt}");
    assert!(prompt.contains("Prove the full local sequence minus GitHub."));
    assert!(prompt.contains("`exponential_pr_open`"));

    // The credential seed file is git-invisible (token-leak guard).
    let status = git_stdout(&expected_worktree, &["status", "--porcelain"]);
    assert!(!status.contains("mcp.json"), "seed file not excluded: {status}");

    // The composed spawn spec: stub program, explicit --model, the
    // explicit+strict MCP config (EXP-83: no project-discovery trust dialog),
    // the skip flag, the prompt positional-last, worktree cwd. Model is
    // ALWAYS passed (§7.7).
    assert_eq!(prepared.spawn.program, stub.to_string_lossy());
    assert_eq!(
        prepared.spawn.args,
        vec![
            "--model".to_string(),
            "opus".to_string(),
            "--mcp-config".to_string(),
            ".exp-mcp.json".to_string(),
            "--strict-mcp-config".to_string(),
            "--dangerously-skip-permissions".to_string(),
            prompt.clone(),
        ]
    );
    assert_eq!(prepared.spawn.cwd.as_deref(), Some(expected_worktree.as_path()));
    assert_eq!(prepared.tab_title, "claude · GATE-99");
    // EXP-145: the identifier rides along so live OSC titles keep it.
    assert_eq!(prepared.tab_title_prefix, "GATE-99");
    eprintln!("dry_run e2e: steps 0–6 verified (worktree, remote, .exp-mcp.json, direct prompt, spawn spec)");

    // ---- steps 7–8: spawn the stub through a real headless TerminalManager;
    //      the watchdog (outside gpui) asserts the observable effects ----
    let marker = expected_worktree.join("claude-ran.txt");
    let prompt_first_line = prompt.lines().next().unwrap().to_string();
    let watchdog_requests = Arc::clone(&requests);
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let marker_ok = fs::read_to_string(&marker)
                .map(|content| content.trim() == prompt_first_line)
                .unwrap_or(false);
            let end_seen = watchdog_requests.lock().unwrap().iter().any(|request| {
                request.starts_with("POST /api/trpc/codingSessions.end")
                    && request.contains(r#"{"id":"sess-dryrun-1"}"#)
            });
            if marker_ok && end_seen {
                eprintln!(
                    "dry_run e2e: PASS — stub claude received the direct positional prompt in \
                     the worktree, exited 0, and the exit hook fired \
                     codingSessions.end(sess-dryrun-1)"
                );
                std::process::exit(0);
            }
            if Instant::now() >= deadline {
                eprintln!(
                    "dry_run e2e: FAIL — timeout (marker_ok={marker_ok}, end_seen={end_seen})"
                );
                std::process::exit(3);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });

    gpui_platform::headless().run(move |cx| {
        let manager = cx.new(|_| TerminalManager::new());
        match spawn_prepared(prepared, &manager, cx, Arc::clone(&trpc)) {
            Ok(LaunchOutcome::Spawned { session_id, worktree, branch, .. }) => {
                assert_eq!(session_id, "sess-dryrun-1");
                assert_eq!(branch, "exp/GATE-99");
                assert!(worktree.ends_with("web.worktrees/exp-GATE-99"));
            }
            Ok(other) => panic!("expected Spawned, got {other:?}"),
            Err(err) => panic!("spawn_prepared failed: {err}"),
        }
        // Keep the manager (and its exit subscription) alive; the watchdog
        // owns pass/fail and exits the process.
        std::mem::forget(manager);
    });
}
