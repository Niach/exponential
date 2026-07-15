//! §7.6 / §11.4 Phase-5 gate #6: **two concurrent coding sessions** — the
//! manager-side isolation invariants, end-to-end and hermetic (no network,
//! no real GitHub, no real `claude`):
//!
//! Two issues (`CC-1`, `CC-2`) on ONE repo launch into TWO
//! [`terminal::TerminalManager`]s (the per-window managers of §7.6's
//! two-window scenario). Proven invariants:
//!
//! - **N issues = N worktrees**: two `exp/<ID>` branches → two distinct
//!   `.worktrees/` dirs under the same clone, no collision;
//! - **N tabs = N PTYs = N children**: distinct `TabId`s, both stub children
//!   ALIVE at the same time (each writes `alive.txt` into its own worktree
//!   cwd), no desktop-side concurrency gate anywhere;
//! - **exit edges never cross**: releasing child A fires
//!   `codingSessions.end(sess-cc-1)` while B is still running (no `end` for
//!   B in the request log), then releasing B fires its own end — each tab's
//!   one-shot [`terminal::ExitHook`] is bound to ITS `coding_sessions.id`.
//!
//! Like `dry_run.rs`, this needs the process main thread for the platform
//! run loop (`harness = false`) and self-skips unless
//! `EXP_CODING_CONCURRENT_E2E=1`:
//!
//! ```sh
//! EXP_CODING_CONCURRENT_E2E=1 cargo test -p coding --test concurrent
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
    LaunchOptions, LaunchOrigin, LaunchRequest, LaunchOutcome, Prepared, PrepareRequest,
    Settings,
};
use gpui::AppContext as _;
use terminal::TerminalManager;

// ---- harness (same shape as dry_run.rs) ----

fn temp_dir(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    path.push(format!("exp-coding-concurrent-{tag}-{}-{nanos}", std::process::id()));
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

/// Serve canned responses in request order (`Connection: close`), recording
/// each request so the watchdog can assert WHICH end fired WHEN.
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

fn end_seen(log: &Arc<Mutex<Vec<String>>>, session_id: &str) -> bool {
    log.lock().unwrap().iter().any(|request| {
        request.starts_with("POST /api/trpc/codingSessions.end")
            && request.contains(&format!(r#"{{"id":"{session_id}"}}"#))
    })
}

fn main() {
    if std::env::var("EXP_CODING_CONCURRENT_E2E").as_deref() != Ok("1") {
        eprintln!(
            "concurrent e2e: skipped (set EXP_CODING_CONCURRENT_E2E=1 to run — needs the \
             process main thread + a live platform run loop)"
        );
        return;
    }

    let root = temp_dir("root");

    // ---- ONE local "GitHub" repo; both issues land on it (§7.6: the branch,
    //      not the repo, is the isolation unit) ----
    let src = root.join("origin-src");
    fs::create_dir_all(&src).unwrap();
    git(&src, &["init", "--quiet", "-b", "main"]);
    fs::write(src.join("README.md"), "seed\n").unwrap();
    git(&src, &["add", "."]);
    git(&src, &["commit", "--quiet", "-m", "seed"]);
    let bare = root.join("origin.git");
    git(&root, &["clone", "--quiet", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);

    let repos_root = root.join("repos");
    let clone = clone_path(&repos_root, "acme/web");
    fs::create_dir_all(clone.parent().unwrap()).unwrap();
    git(&root, &["clone", "--quiet", bare.to_str().unwrap(), clone.to_str().unwrap()]);

    // ---- the stub `claude`: stays ALIVE until its worktree gets release.txt,
    //      so both children provably run at the same time ----
    let stub = root.join("bin").join("claude-stub");
    fs::create_dir_all(stub.parent().unwrap()).unwrap();
    fs::write(
        &stub,
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then echo '9.9.9 (Claude Code stub)'; exit 0; fi\n\
         [ \"$1\" = \"--model\" ] || exit 7\n\
         [ \"$3\" = \"--mcp-config\" ] || exit 6\n\
         [ \"$6\" = \"--dangerously-skip-permissions\" ] || exit 8\n\
         echo alive > alive.txt\n\
         while [ ! -f release.txt ]; do sleep 0.05; done\n\
         exit 0\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    }

    // ---- canned tRPC: (forIssue → token → start) ×2, then the two ends in
    //      the release order the watchdog drives (A first, then B) ----
    let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let repo_json = r#"{"result":{"data":{"repositoryId":"repo-1","fullName":"acme/web","defaultBranch":"main"}}}"#;
    let token_json = r#"{"result":{"data":{"token":"ghs_concurrent_dead","fullName":"acme/web","defaultBranch":"main","expiresAt":"2026-07-03T12:55:00.000Z"}}}"#;
    let base = canned_server(
        vec![
            (200, repo_json.to_string()),
            (200, token_json.to_string()),
            (200, r#"{"result":{"data":{"session":{"id":"sess-cc-1","issueId":"issue-cc-1","status":"running"}}}}"#.to_string()),
            (200, repo_json.to_string()),
            (200, token_json.to_string()),
            (200, r#"{"result":{"data":{"session":{"id":"sess-cc-2","issueId":"issue-cc-2","status":"running"}}}}"#.to_string()),
            (200, r#"{"result":{"data":{"session":{"id":"sess-cc-1","status":"ended"}}}}"#.to_string()),
            (200, r#"{"result":{"data":{"session":{"id":"sess-cc-2","status":"ended"}}}}"#.to_string()),
        ],
        Arc::clone(&requests),
    );

    // ---- deps: real GitWorktrees, pre-seeded key (mint path is launcher.rs
    //      unit-tested; this e2e isolates the §7.6 invariants) ----
    let data_dir = root.join("data");
    fs::create_dir_all(&data_dir).unwrap();
    let store = TokenStore::file_only(data_dir.clone());
    store.set("acct", SecretKind::PersonalApiKey, "expu_concurrent_key").unwrap();
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
                title: "Concurrent session".to_string(),
                description: None,
            })
        }),
        worktrees: Arc::new(GitWorktrees),
    };

    // plan_mode OFF so the stub's `$6 = --dangerously-skip-permissions`
    // check holds (the prompt rides argv as $7 — direct delivery).
    let request_for = |issue_id: &str, identifier: &str| {
        PrepareRequest::Issue(LaunchRequest {
            issue_id: issue_id.to_string(),
            issue_identifier: identifier.to_string(),
            device_label: "concurrentbox".to_string(),
            origin: LaunchOrigin::Local,
            options: LaunchOptions {
                model: "opus".to_string(),
                effort: "".to_string(),
                ultracode: false,
                plan_mode: false,
            },
        })
    };

    // ---- steps 1–6 for BOTH issues (sequential prep; the CHILDREN overlap) ----
    let prepared_a = match prepare(&request_for("issue-cc-1", "CC-1"), &deps).unwrap() {
        Prepared::Ready(prepared) => prepared,
        Prepared::Disabled(reason) => panic!("A unexpectedly disabled: {reason:?}"),
    };
    let prepared_b = match prepare(&request_for("issue-cc-2", "CC-2"), &deps).unwrap() {
        Prepared::Ready(prepared) => prepared,
        Prepared::Disabled(reason) => panic!("B unexpectedly disabled: {reason:?}"),
    };

    // N issues = N worktrees: distinct branch dirs under ONE clone.
    let wt_a = worktree_path(&clone, "exp/CC-1");
    let wt_b = worktree_path(&clone, "exp/CC-2");
    assert_eq!(prepared_a.worktree, wt_a);
    assert_eq!(prepared_b.worktree, wt_b);
    assert_ne!(wt_a, wt_b, "worktrees must not collide");
    assert_eq!(prepared_a.session_id, "sess-cc-1");
    assert_eq!(prepared_b.session_id, "sess-cc-2");
    eprintln!("concurrent e2e: two worktrees cut ({} | {})", wt_a.display(), wt_b.display());

    // ---- the watchdog drives the release sequence + owns pass/fail ----
    let alive_a = wt_a.join("alive.txt");
    let alive_b = wt_b.join("alive.txt");
    let release_a = wt_a.join("release.txt");
    let release_b = wt_b.join("release.txt");
    let log = Arc::clone(&requests);
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(60);
        let wait_for = |what: &str, pred: &dyn Fn() -> bool| {
            while !pred() {
                if Instant::now() >= deadline {
                    eprintln!("concurrent e2e: FAIL — timeout waiting for {what}");
                    std::process::exit(3);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        };

        // 1. BOTH children alive at the same time (two PTYs, two processes).
        wait_for("both stubs alive", &|| alive_a.is_file() && alive_b.is_file());
        assert!(
            !end_seen(&log, "sess-cc-1") && !end_seen(&log, "sess-cc-2"),
            "no end may fire while both children run"
        );
        eprintln!("concurrent e2e: both children alive simultaneously");

        // 2. Release A → ITS end fires; B keeps running, ITS end must not.
        fs::write(&release_a, "go\n").unwrap();
        wait_for("codingSessions.end(sess-cc-1)", &|| end_seen(&log, "sess-cc-1"));
        assert!(
            !end_seen(&log, "sess-cc-2"),
            "B's end fired although B is still running — exit hooks crossed"
        );
        eprintln!("concurrent e2e: A ended alone (B still running)");

        // 3. Release B → its own end fires.
        fs::write(&release_b, "go\n").unwrap();
        wait_for("codingSessions.end(sess-cc-2)", &|| end_seen(&log, "sess-cc-2"));
        eprintln!(
            "concurrent e2e: PASS — two isolated sessions (worktrees, tabs, PTYs, \
             per-session end calls)"
        );
        std::process::exit(0);
    });

    // ---- steps 7–8 into TWO managers = the two windows of §7.6 ----
    gpui_platform::headless().run(move |cx| {
        let manager_a = cx.new(|_| TerminalManager::new());
        let manager_b = cx.new(|_| TerminalManager::new());

        let tab_a = match spawn_prepared(prepared_a, &manager_a, cx, Arc::clone(&trpc)) {
            Ok(LaunchOutcome::Spawned { terminal_tab, session_id, .. }) => {
                assert_eq!(session_id, "sess-cc-1");
                terminal_tab
            }
            other => panic!("A: expected Spawned, got {other:?}"),
        };
        let tab_b = match spawn_prepared(prepared_b, &manager_b, cx, Arc::clone(&trpc)) {
            Ok(LaunchOutcome::Spawned { terminal_tab, session_id, .. }) => {
                assert_eq!(session_id, "sess-cc-2");
                terminal_tab
            }
            other => panic!("B: expected Spawned, got {other:?}"),
        };

        // N tabs: process-unique ids; each manager owns exactly ITS tab.
        assert_ne!(tab_a, tab_b, "tab ids must be process-unique");
        assert_eq!(manager_a.read(cx).len(), 1);
        assert_eq!(manager_b.read(cx).len(), 1);
        // N PTYs / N children: two live children with distinct pids.
        let pid_a = manager_a.read(cx).tab(tab_a).and_then(|tab| {
            tab.view.read(cx).session().borrow().process_id()
        });
        let pid_b = manager_b.read(cx).tab(tab_b).and_then(|tab| {
            tab.view.read(cx).session().borrow().process_id()
        });
        assert!(pid_a.is_some() && pid_b.is_some(), "both children must have pids");
        assert_ne!(pid_a, pid_b, "children must be distinct processes");
        eprintln!("concurrent e2e: two tabs ({tab_a:?}, {tab_b:?}), two children ({pid_a:?}, {pid_b:?})");

        // Keep both managers (and their exit subscriptions) alive; the
        // watchdog owns pass/fail and exits the process.
        std::mem::forget(manager_a);
        std::mem::forget(manager_b);
    });
}
