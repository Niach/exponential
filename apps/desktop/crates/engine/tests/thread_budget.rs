//! #PERF-12 — the threads a host spends per hosted session.
//!
//! Measured on mint (desktop 0.14.173): about 25 threads per hosted claude
//! session, and threads created and torn down all the time. Every thread
//! stack is an mmap under the process's mmap write lock, and the main
//! thread's page faults queued behind it. This test hosts N = 20 sessions of
//! the REAL claude adapter (against `fake-claude.sh`, a real child process)
//! through the REAL engine on ONE shared runtime, as a host does, and holds:
//!
//! - **at most 3 threads of its own per session** (the engine thread, the
//!   child's stdout reader and its waiter); everything else is a task on
//!   the shared runtime. Master spent 8 here (161 for 20: the ticker, 2
//!   outbox workers, the stderr reader, the exit forwarder on top; a
//!   publishing run adds 3 drains and the kill pump) and fails;
//! - **the shared runtime stays bounded** (its workers + its blocking pool);
//! - **no churn while idle**: over a few idle seconds no thread dies. A
//!   guard, not a master failure: tokio's default pool dropped an idle
//!   thread after 10 s, longer than this window; the 10 min keep-alive is
//!   what ends that churn.
//!
//! Linux only: it reads `/proc/self/task`, the same numbers the issue came
//! from. Its own test binary, so no other test's threads are counted.

#![cfg(target_os = "linux")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use engine::adapters::claude::ClaudeAgent;
use engine::adapters::{AdapterKind, AdapterSpec};
use engine::{ChildExitLink, EngineExit, EngineHost, EngineParts, EngineSession, EngineStart, EventSink, KillFeed, RecordingSink};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/claude");
const SESSIONS: usize = 20;
/// The budget: the threads a session may own.
const PER_SESSION: usize = 3;
/// Process-wide threads the first session starts once (a registry's
/// watcher, a lazy global), owned by no session.
const ONE_OFFS: usize = 8;
/// The shared runtime's thread name (its workers and its blocking pool).
const RUNTIME_THREAD: &str = "steer-ws";

struct TestHost;

impl EngineHost for TestHost {
    fn on_exit(&self, _exit: EngineExit) {}
}

/// The process's threads: id → name.
fn threads() -> BTreeMap<u32, String> {
    std::fs::read_dir("/proc/self/task")
        .expect("/proc/self/task is readable")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let tid: u32 = entry.file_name().to_str()?.parse().ok()?;
            let name = std::fs::read_to_string(entry.path().join("comm")).unwrap_or_default();
            Some((tid, name.trim().to_string()))
        })
        .collect()
}

fn fake_claude() -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let path = Path::new(FIXTURES).join("fake-claude.sh");
    if let Ok(metadata) = std::fs::metadata(&path) {
        let mut permissions = metadata.permissions();
        if permissions.mode() & 0o111 == 0 {
            permissions.set_mode(0o755);
            let _ = std::fs::set_permissions(&path, permissions);
        }
    }
    path
}

fn options() -> coding::LaunchOptions {
    coding::LaunchOptions {
        agent: coding::CodingAgent::Claude,
        model: "opus".to_string(),
        effort: String::new(),
        ultracode: false,
        plan_mode: false,
        subagent_model: String::new(),
        mcp_server_ids: Vec::new(),
        account: None,
        computer_use: None,
        computer_place: Default::default(),
    }
}

fn adapter_spec(work: &Path, session_id: &str, exit: ChildExitLink) -> AdapterSpec {
    let spawn = terminal::pty::SpawnSpec::new(fake_claude().display().to_string())
        .cwd(work)
        .env("CLAUDE_CONFIG_DIR", work.join("claude-config").display().to_string())
        .env("EXP_FAKE_CLAUDE_DIR", Path::new(FIXTURES).join("basic").display().to_string())
        .env(coding::MCP_TOKEN_ENV, "expu_test-key");
    AdapterSpec {
        kind: AdapterKind::Claude,
        agent: coding::CodingAgent::Claude,
        spawn,
        options: options(),
        mcp: coding::AgentMcp::ClaudeInline {
            url: "https://app.example/api/mcp".to_string(),
            session_id: Some(session_id.to_string()),
        },
        servers: Vec::new(),
        cwd: work.to_path_buf(),
        session_id: session_id.to_string(),
        prompt: None,
        resume: None,
        replay: false,
        personal_key: Some("expu_test-key".to_string()),
        reaper_settings_path: Some(work.join("claude-hooks/1/row.settings.json")),
        system_append: coding::skill::system_append(None),
        context_layers: coding::ContextLayers::default(),
        name_conversation: false,
        skills_dir: None,
        aside: false,
        exit,
    }
}

fn prepared(session_id: &str, worktree: &Path) -> coding::PreparedLaunch {
    // #EDE-97: no key holder here; a clear tree, as in the other engine tests.
    coding::sealing::note_clear_tree(session_id);
    coding::PreparedLaunch {
        session_id: session_id.to_string(),
        issue_identifier: "PERF-12".to_string(),
        worktree: worktree.to_path_buf(),
        clone: worktree.to_path_buf(),
        repository_id: None,
        account_pick: None,
        branch: "exp/PERF-12".to_string(),
        base_branch: None,
        base_ref: None,
        run_cleanup: None,
        mcp_warnings: Vec::new(),
        spawn: terminal::pty::SpawnSpec::new("claude"),
        acp: coding::AcpLaunch {
            prompt: None,
            options: options(),
            mcp: coding::AgentMcp::ClaudeFile,
            session_id: session_id.to_string(),
            resume: None,
            reaper_settings_path: None,
            system_append: coding::skill::system_append(None),
            context_layers: coding::ContextLayers::default(),
            servers: Vec::new(),
            mcp_secrets: Default::default(),
            skills_dir: None,
            computer_space: false,
        },
        tab_title: "claude · PERF-12".to_string(),
        tab_title_prefix: "PERF-12".to_string(),
        seed_message_id: None,
        start_note: None,
        bypass_permissions: true,
        plan_mode: false,
        agent: coding::CodingAgent::Claude,
        claude_session_id: None,
        codex_originator: None,
        codex_resume_id: None,
        launch_hold: None,
        mcp_pick: None,
        skill_pick: None,
        audience: None,
    }
}

struct Hosted {
    session: EngineSession,
    work: PathBuf,
    _hold: coding::LaunchHold,
}

fn host(runtime: &Arc<steer::SteerRuntime>, index: usize) -> Hosted {
    let work = std::env::temp_dir().join(format!("exp-perf12-{index}-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&work).expect("a scratch worktree");
    let hold = coding::launch_gate::hold(&work);
    let session_id = format!("perf12-{index}-{}", uuid::Uuid::new_v4().simple());
    let child_exit = ChildExitLink::new();
    let adapter =
        ClaudeAgent::new(adapter_spec(&work, &session_id, child_exit.clone())).expect("the adapter builds");
    let session = engine::start_with(
        EngineStart {
            prepared: prepared(&session_id, &work),
            // Unroutable: every server call fails at once.
            trpc: Arc::new(api::TrpcClient::new("http://127.0.0.1:1", Arc::new(|| None))),
            runtime: Arc::clone(runtime),
            data_dir: work.clone(),
            account_id: "acct-1".to_string(),
            own_user_id: Some("user-1".to_string()),
            personal_key: Some("expu_test-key".to_string()),
            persistent: false,
            foreign_host: false,
            rotation_host: false,
            publish: false,
            kill: KillFeed::inert(),
            local_sink: None,
        },
        Arc::new(TestHost),
        EngineParts {
            adapter,
            child_exit,
            sink: Some(RecordingSink::new() as Arc<dyn EventSink>),
            conversation: None,
            computer: None,
        },
    )
    .expect("the engine starts");
    Hosted { session, work, _hold: hold }
}

/// Threads in `now` and not in `base`, split into the shared runtime's and
/// everyone else's (the sessions' own).
fn new_threads(base: &BTreeMap<u32, String>, now: &BTreeMap<u32, String>) -> (Vec<String>, Vec<String>) {
    now.iter()
        .filter(|(tid, _)| !base.contains_key(tid))
        .map(|(_, name)| name.clone())
        .partition(|name| name == RUNTIME_THREAD)
}

#[test]
fn twenty_hosted_sessions_stay_within_three_threads_each_and_do_not_churn() {
    let runtime = steer::SteerRuntime::new().expect("a steer runtime");
    // Settle the runtime's workers before the baseline.
    runtime.handle().block_on(async { tokio::task::yield_now().await });
    let base = threads();

    let hosted: Vec<Hosted> = (0..SESSIONS).map(|index| host(&runtime, index)).collect();
    let deadline = Instant::now() + Duration::from_secs(30);
    while hosted.iter().any(|hosted| hosted.session.acp_session_id().is_none()) {
        assert!(Instant::now() < deadline, "every hosted session finishes its handshake");
        std::thread::sleep(Duration::from_millis(20));
    }
    // A few ticker rounds (one a second), so every per-run job has run.
    std::thread::sleep(Duration::from_secs(3));

    let live = threads();
    let (pool, own) = new_threads(&base, &live);
    let mut by_name: BTreeMap<String, usize> = BTreeMap::new();
    for name in &own {
        // `acp-lines-<pid>` and friends: one row per kind.
        let kind = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == '-').to_string();
        *by_name.entry(kind).or_default() += 1;
    }
    eprintln!(
        "PERF-12: {SESSIONS} sessions: {} own threads ({:.2}/session), {} shared runtime threads, {} total (base {})",
        own.len(),
        own.len() as f64 / SESSIONS as f64,
        pool.len(),
        live.len(),
        base.len()
    );
    eprintln!("PERF-12: own threads by kind: {by_name:?}");

    // Idle: no turn runs. Nothing may die (a kept pool) in this window.
    std::thread::sleep(Duration::from_secs(4));
    let idle = threads();
    let died: Vec<&String> = live.iter().filter(|(tid, _)| !idle.contains_key(tid)).map(|(_, name)| name).collect();
    let born: Vec<&String> = idle.iter().filter(|(tid, _)| !live.contains_key(tid)).map(|(_, name)| name).collect();
    eprintln!("PERF-12: idle 4 s: {} died {died:?}, {} born {born:?}", died.len(), born.len());

    for hosted in &hosted {
        hosted.session.kill("killed");
    }
    for hosted in &hosted {
        let _ = hosted.session.wait_timeout(Duration::from_secs(10));
        let _ = std::fs::remove_dir_all(&hosted.work);
    }

    assert!(
        own.len() <= PER_SESSION * SESSIONS + ONE_OFFS,
        "{SESSIONS} hosted sessions own {} threads, over the {PER_SESSION}-a-session budget ({} + {ONE_OFFS}): {by_name:?}",
        own.len(),
        PER_SESSION * SESSIONS
    );
    let pool_cap = eagent::engine::runtime::WORKER_THREADS + eagent::engine::runtime::MAX_BLOCKING_THREADS;
    assert!(pool.len() <= pool_cap, "the shared runtime grew to {} threads (cap {pool_cap})", pool.len());
    assert!(died.is_empty(), "threads died while every session was idle (churn): {died:?}");
}
