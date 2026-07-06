//! The Start-coding launcher (masterplan-v3 §7.1, DC-1) — the eight-step
//! sequence, run **identically** for a local button press and a relay
//! `start_session` frame (§08 calls this same entry point; there is no
//! second "remote start" implementation).
//!
//! Split to match gpui's threading model while keeping one code path:
//!
//! 1. [`prepare_launch`] — steps 1–6 (repo resolve → JIT token → git →
//!    `.mcp.json` → `PROMPT.md` → `codingSessions.start`). **Blocking
//!    network and git I/O, gpui-free** — run it on the background executor.
//!    Returns either a [`PreparedLaunch`] (the spawn spec for
//!    `claude --dangerously-skip-permissions`) or a [`DisabledReason`] (never
//!    falsely block, always explain — none of these are errors/panics).
//! 2. [`spawn_prepared`] — steps 7–8 on the foreground: opens the Claude tab
//!    through the §06 `TerminalManager` (keyed by the `coding_sessions` id)
//!    and installs the one-shot exit hook that ends the session row
//!    (idempotent server-side) when the child dies. The seed line rides the
//!    spawn spec as claude's positional prompt (never PTY stdin).
//!
//! The launcher never touches PTYs (§06 owns them) and never talks to the
//! steer relay (§3.1: `coding` does not depend on `steer`) — the app/ui layer
//! takes `LaunchOutcome::Spawned { session_id, .. }` and hands the same PTY
//! tee + session id to the steer publisher (§08).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use api::error::ApiError;
use api::token_store::TokenStore;
use api::trpc::TrpcClient;
use api::{coding_sessions, repositories, users};
use gpui::App;
use terminal::pty::SpawnSpec;
use terminal::tab::{TabId, TabKind};
use terminal::TerminalManager;

use crate::doctor::{run_doctor, ToolCheck};
use crate::git_worktree::{
    branch_name, create_worktree, ensure_clone, fetch_base, set_token_remote, GitError, TokenUrl,
};
use crate::mcp_json::write_mcp_json;
use crate::prompt::{write_prompt, SEED_LINE};
use crate::settings::Settings;

/// Where the launch came from (§7.1). Both origins run the SAME sequence —
/// the variant exists for the session's audit surface, not for branching.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchOrigin {
    /// The Start-coding button on the issue-detail header.
    Local,
    /// A relay `start_session` frame (§08's control channel).
    Relay { device_id: String, claimant: String },
}

/// The machine's hostname — §7.1's `device_label` (also the server-side
/// `coding_sessions.device_label`). Env vars first (cheap), then the
/// ubiquitous `hostname` binary; never fails (falls back to a placeholder).
pub fn default_device_label() -> String {
    for var in ["HOSTNAME", "COMPUTERNAME", "HOST"] {
        if let Ok(value) = std::env::var(var) {
            let trimmed = value.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
    }
    if let Ok(output) = std::process::Command::new("hostname").output() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }
    "unknown-host".to_string()
}

/// §7.1's public entry-point input.
#[derive(Clone, Debug)]
pub struct LaunchRequest {
    pub issue_id: String,
    /// e.g. `EXP-42` — becomes the branch name (`<prefix><IDENTIFIER>`).
    pub issue_identifier: String,
    /// Hostname; also `coding_sessions.device_label`.
    pub device_label: String,
    pub origin: LaunchOrigin,
}

/// Issue text for `PROMPT.md`, fetched by the caller from the sync store
/// (`coding` cannot depend on `sync` — §3.1 dependency direction).
#[derive(Clone, Debug)]
pub struct IssueSeed {
    pub title: String,
    pub description: Option<String>,
}

/// §7.1 step 3, injectable for tests: turn (repos_root, repo, branch, token)
/// into a ready worktree. The real impl is [`GitWorktrees`] (argv git).
pub trait WorktreeProvider: Send + Sync {
    fn prepare(
        &self,
        repos_root: &Path,
        full_name: &str,
        default_branch: &str,
        branch: &str,
        url: &TokenUrl,
    ) -> Result<PathBuf, GitError>;
}

/// The real git path: `ensure_clone` → `set_token_remote` (re-set EVERY
/// launch — the previous embedded token is dead) → best-effort fetch of the
/// base branch → `create_worktree` (idempotent reuse) → repo-local excludes
/// for the credential-bearing seed files.
pub struct GitWorktrees;

impl WorktreeProvider for GitWorktrees {
    fn prepare(
        &self,
        repos_root: &Path,
        full_name: &str,
        default_branch: &str,
        branch: &str,
        url: &TokenUrl,
    ) -> Result<PathBuf, GitError> {
        let clone = ensure_clone(repos_root, full_name, url)?;
        set_token_remote(&clone, url)?;
        // Best-effort: a stale-but-present origin/<default> still yields a
        // valid worktree; only a truly missing base ref fails below.
        let _ = fetch_base(&clone, default_branch, url);
        let worktree =
            create_worktree(&clone, branch, &format!("origin/{default_branch}"), url)?;
        // .mcp.json carries the raw expu_ key and claude is told to commit +
        // push — keep both seed files out of `git add -A` via the shared,
        // never-committed `.git/info/exclude` (best-effort by design).
        let _ = crate::git_worktree::ensure_local_excludes(
            &clone,
            &[crate::mcp_json::MCP_JSON_FILE, crate::prompt::PROMPT_FILE],
        );
        Ok(worktree)
    }
}

/// Issue title/description lookup for `PROMPT.md` (sync-store backed; the
/// caller owns the store — §3.1: `coding` cannot depend on `sync`).
pub type IssueSeedFn = Arc<dyn Fn(&str) -> Option<IssueSeed> + Send + Sync>;

/// The injected collaborators (§7.1) — everything the sequence needs, so the
/// crate stays testable and both launch origins share one code path.
pub struct CodingDeps {
    /// Mutation client bound to the signed-in account (bearer at call time).
    pub trpc: Arc<TrpcClient>,
    /// The file-based secret store holding the hidden `expu_` key (§7.2).
    pub token_store: Arc<TokenStore>,
    /// The account the key/session belong to ([`api::accounts`] id form).
    pub account_id: String,
    /// Resolved coding settings (claude path, repos root, branch prefix).
    pub settings: Settings,
    /// Issue title/description lookup for `PROMPT.md` (sync-store backed).
    pub issue_seed: IssueSeedFn,
    /// Git ops ([`GitWorktrees`] in production).
    pub worktrees: Arc<dyn WorktreeProvider>,
}

/// §7.1's non-fatal "why Start coding can't run" set — each renders as a
/// small inline error with a remediation link (never falsely block,
/// always explain). None of these panic and none are transport errors.
#[derive(Clone, Debug)]
pub enum DisabledReason {
    /// `repositories.forIssue` returned null.
    NoRepositoryLinked,
    /// The GitHub App is not installed on the repo (server 412) — install is
    /// web-only (§7.9); link out.
    GithubAppMissing { full_name: String, message: String },
    /// `claude` or `git` failed the doctor (§7.7) — names which tool.
    DoctorFailed(ToolCheck),
    /// The plan's concurrent-session cap (server 412 on start) — carries the
    /// server's upgrade copy.
    SessionLimit { message: String },
    /// The server refused to mint the installation token (401/403).
    TokenDenied { message: String },
}

impl DisabledReason {
    /// User-facing copy (§7.1: inline error / disabled-button helper text).
    pub fn message(&self) -> String {
        match self {
            DisabledReason::NoRepositoryLinked => {
                "Link a repository to this project in workspace settings.".to_string()
            }
            DisabledReason::GithubAppMissing { message, .. } => message.clone(),
            DisabledReason::DoctorFailed(check) => check
                .error
                .clone()
                .unwrap_or_else(|| format!("{} is not available", check.tool)),
            DisabledReason::SessionLimit { message } => message.clone(),
            DisabledReason::TokenDenied { message } => message.clone(),
        }
    }
}

/// A hard failure in the sequence (network/git/filesystem) — distinct from
/// [`DisabledReason`], which is the expected "can't run, here's why" surface.
#[derive(Debug)]
pub enum CodingError {
    Api(ApiError),
    Git(GitError),
    Io(String),
    Terminal(String),
}

impl std::fmt::Display for CodingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodingError::Api(err) => write!(f, "api: {err}"),
            CodingError::Git(err) => write!(f, "git: {err}"),
            CodingError::Io(msg) => write!(f, "io: {msg}"),
            CodingError::Terminal(msg) => write!(f, "terminal: {msg}"),
        }
    }
}

impl std::error::Error for CodingError {}

impl From<ApiError> for CodingError {
    fn from(err: ApiError) -> Self {
        CodingError::Api(err)
    }
}

impl From<GitError> for CodingError {
    fn from(err: GitError) -> Self {
        CodingError::Git(err)
    }
}

/// Steps 1–6 done: everything the foreground needs to open the Claude tab.
#[derive(Debug)]
pub struct PreparedLaunch {
    /// The `coding_sessions` row id — keys the terminal tab (§06) and the
    /// steer session room (§08).
    pub session_id: String,
    pub issue_identifier: String,
    pub worktree: PathBuf,
    /// The real git branch (keeps its `/`), e.g. `exp/EXP-42`.
    pub branch: String,
    /// `claude --dangerously-skip-permissions` in the worktree (§7.1 step 7).
    pub spawn: SpawnSpec,
    /// Tab strip default title (`claude · EXP-42`).
    pub tab_title: String,
}

/// [`prepare_launch`]'s outcome: ready to spawn, or disabled-with-reason.
#[derive(Debug)]
pub enum Prepared {
    Ready(PreparedLaunch),
    Disabled(DisabledReason),
}

/// §7.1's `LaunchOutcome`, produced by [`spawn_prepared`] (or directly by
/// the caller when [`prepare_launch`] returned [`Prepared::Disabled`]).
#[derive(Debug)]
pub enum LaunchOutcome {
    Spawned {
        session_id: String,
        terminal_tab: TabId,
        worktree: PathBuf,
        branch: String,
    },
    Disabled {
        reason: DisabledReason,
    },
}

/// Steps 1–6 of §7.1 (blocking; run on the background executor):
///
/// 0. doctor — both `claude` AND `git` must resolve (§7.7: a machine
///    with git missing is blocked here, not allowed to crash at clone);
/// 1. `repositories.forIssue` — null ⇒ [`DisabledReason::NoRepositoryLinked`];
/// 2. `repositories.installationToken` — JIT, session-gated, never persisted;
/// 3. git: clone/worktree/`exp/<IDENTIFIER>` branch + token remote re-set
///    (the personal-key read/mint races this on a side thread, §7.2);
/// 4. `.mcp.json` (the ONLY place the raw `expu_` key lands on disk);
/// 5. `PROMPT.md` (plan-first seed, templated from the sync store);
/// 6. `codingSessions.start` — BEFORE spawn; its id keys tab + steer room.
pub fn prepare_launch(req: &LaunchRequest, deps: &CodingDeps) -> Result<Prepared, CodingError> {
    // Step 0/1a — the doctor gate. Cheap relative to clone/mint and
    // structural: the relay origin has no button whose disabled state could
    // have gated this.
    let report = run_doctor(&deps.settings);
    if let Some(failed) = report.first_failure() {
        return Ok(Prepared::Disabled(DisabledReason::DoctorFailed(
            failed.clone(),
        )));
    }

    // Step 1 — resolve the repository (the coding-first gate).
    let Some(repo) = repositories::for_issue(&deps.trpc, &req.issue_id)? else {
        return Ok(Prepared::Disabled(DisabledReason::NoRepositoryLinked));
    };

    // Step 2 — mint the JIT installation token (session-gated, ~55 min TTL,
    // never persisted/logged — TokenUrl + scrubbed git errors enforce that).
    let token = match repositories::installation_token(&deps.trpc, &repo.repository_id) {
        Ok(token) => token,
        Err(ApiError::Http { status: 412, message }) => {
            return Ok(Prepared::Disabled(DisabledReason::GithubAppMissing {
                full_name: repo.full_name,
                message,
            }))
        }
        Err(ApiError::Http { status: status @ (401 | 403), message }) => {
            return Ok(Prepared::Disabled(DisabledReason::TokenDenied {
                message: format!("{message} (HTTP {status})"),
            }))
        }
        Err(err) => return Err(err.into()),
    };

    // §7.2 — the personal-key read/mint races the git prep on a side thread;
    // only step 4 (.mcp.json) needs the result.
    let key_handle = {
        let trpc = Arc::clone(&deps.trpc);
        let store = Arc::clone(&deps.token_store);
        let account_id = deps.account_id.clone();
        std::thread::spawn(move || users::ensure_personal_key(&trpc, &store, &account_id))
    };

    // Step 3 — git via argv (never gh): clone → token remote → worktree.
    let branch = branch_name(&deps.settings.branch_prefix, &req.issue_identifier);
    let url = TokenUrl::new(token.full_name.clone(), token.token.clone());
    let worktree = deps.worktrees.prepare(
        &deps.settings.repos_root_path(),
        &token.full_name,
        &token.default_branch,
        &branch,
        &url,
    )?;

    let personal_key = key_handle
        .join()
        .map_err(|_| CodingError::Io("personal-key thread panicked".to_string()))??;

    // Step 4 — .mcp.json (authenticates the spawned claude as the real user).
    write_mcp_json(&worktree, deps.trpc.base_url(), &personal_key)
        .map_err(|e| CodingError::Io(format!("write .mcp.json: {e}")))?;

    // Step 5 — PROMPT.md (plan-first; title/description from the sync store).
    let seed = (deps.issue_seed)(&req.issue_id);
    let (title, description) = match &seed {
        Some(seed) => (seed.title.as_str(), seed.description.as_deref()),
        None => (req.issue_identifier.as_str(), None),
    };
    write_prompt(&worktree, &req.issue_identifier, title, description)
        .map_err(|e| CodingError::Io(format!("write PROMPT.md: {e}")))?;

    // Step 6 — codingSessions.start BEFORE spawn (the id keys everything).
    let session =
        match coding_sessions::start(&deps.trpc, &req.issue_id, Some(&req.device_label)) {
            Ok(session) => session,
            Err(ApiError::Http { status: 412, message }) => {
                return Ok(Prepared::Disabled(DisabledReason::SessionLimit { message }))
            }
            Err(err) => return Err(err.into()),
        };

    // Step 7's spawn spec — program from settings (§7.7), argv-direct. The
    // model is passed explicitly-ALWAYS (never the user's `claude` CLI default,
    // which may be a scarcer model like Fable) so coding sessions never silently
    // consume it (§7.7, locked 2026-07-03).
    // The seed line rides argv as the positional prompt: bytes typed into
    // the PTY before claude's TUI enters raw mode get swallowed during
    // startup, so the prompt must never be delivered via stdin.
    let spawn = SpawnSpec::new(&deps.settings.resolved_claude_path())
        .args([
            "--model",
            deps.settings.claude_model.as_str(),
            "--dangerously-skip-permissions",
            SEED_LINE,
        ])
        .cwd(&worktree);

    Ok(Prepared::Ready(PreparedLaunch {
        session_id: session.id,
        issue_identifier: req.issue_identifier.clone(),
        worktree,
        branch,
        spawn,
        tab_title: format!("claude · {}", req.issue_identifier),
    }))
}

/// Foreground follow-up to the child-exit edge (§7.5): the ui layer passes
/// one of these into [`spawn_prepared_with`] to flip its play↔stop state /
/// clear its local-session registry when the Claude child dies. Runs on the
/// gpui foreground AFTER the idempotent `codingSessions.end` fire-and-forget
/// thread is spawned. The `coding` crate itself never needs it —
/// [`spawn_prepared`] passes `None`.
pub type ExitNotify = Box<dyn FnOnce(&mut App) + 'static>;

/// Steps 7–8 of §7.1 (foreground; needs `&mut App`):
///
/// 7. open a Claude tab keyed by the `coding_sessions` id via the §06
///    `TerminalManager` — the seed line ("Read PROMPT.md in this directory,
///    then follow it.") already rides the spawn spec as claude's positional
///    prompt (stdin written before the TUI's raw mode is swallowed);
/// 8. install the one-shot exit hook: when the child dies,
///    `codingSessions.end` fires from a plain thread (idempotent server-side,
///    so a relay-side kill that already ended the row is safe). The tab
///    itself stays open with the final scrollback + exit-code strip (§7.5).
pub fn spawn_prepared(
    prepared: PreparedLaunch,
    manager: &gpui::Entity<TerminalManager>,
    cx: &mut App,
    trpc: Arc<TrpcClient>,
) -> Result<LaunchOutcome, CodingError> {
    spawn_prepared_with(prepared, manager, cx, trpc, None)
}

/// [`spawn_prepared`] with the optional foreground [`ExitNotify`] — the seam
/// the §7.5 play/stop UI consumes (the hook itself stays owned here so both
/// entry points share ONE exit path).
pub fn spawn_prepared_with(
    prepared: PreparedLaunch,
    manager: &gpui::Entity<TerminalManager>,
    cx: &mut App,
    trpc: Arc<TrpcClient>,
    exit_notify: Option<ExitNotify>,
) -> Result<LaunchOutcome, CodingError> {
    let PreparedLaunch { session_id, worktree, branch, spawn, tab_title, .. } = prepared;

    let end_session_id = session_id.clone();
    let exit_trpc = Arc::clone(&trpc);
    let on_exit: terminal::ExitHook = Box::new(move |_tab, _exit, cx| {
        // Blocking HTTP off the foreground; best-effort — the server also
        // reconciles (idempotent end), and a dead network here must never
        // take the exit-strip rendering down with it.
        let trpc = Arc::clone(&exit_trpc);
        let end_session_id = end_session_id.clone();
        std::thread::spawn(move || {
            let _ = coding_sessions::end(&trpc, &end_session_id);
        });
        if let Some(notify) = exit_notify {
            notify(cx);
        }
    });

    let tab_id = manager
        .update(cx, |manager, cx| -> Result<TabId, CodingError> {
            let tab_id = manager
                .open_tab(TabKind::Claude, tab_title, &spawn, Some(on_exit), cx)
                .map_err(|e| CodingError::Terminal(format!("spawn claude tab: {e}")))?;
            Ok(tab_id)
        })
        .inspect_err(|_| {
            // Step 6 already created a `running` row; a spawn failure means no
            // child and therefore no exit hook will EVER fire — end the row
            // now (idempotent server-side) or the "coding now" badge ghosts
            // on every client forever.
            let trpc = Arc::clone(&trpc);
            let session_id = session_id.clone();
            std::thread::spawn(move || {
                let _ = coding_sessions::end(&trpc, &session_id);
            });
        })?;

    Ok(LaunchOutcome::Spawned {
        session_id,
        terminal_tab: tab_id,
        worktree,
        branch,
    })
}

/// Best-effort session end for teardown paths that bypass the exit hook
/// (app quit with a live child, relay kill). Idempotent server-side.
pub fn end_session_best_effort(trpc: &TrpcClient, session_id: &str) {
    let _ = coding_sessions::end(trpc, session_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::token_store::SecretKind;
    use api::StaticToken;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    // ---- harness ----

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
            "exp-coding-launch-{tag}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    /// Serve a fixed sequence of canned responses, one connection each
    /// (`Connection: close`), in request order.
    fn canned_server(responses: Vec<(u16, String)>) -> String {
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
    /// records the branch/base it was asked for.
    struct FakeWorktrees {
        worktree: PathBuf,
        seen: std::sync::Mutex<Vec<(String, String, String)>>,
    }

    impl WorktreeProvider for FakeWorktrees {
        fn prepare(
            &self,
            _repos_root: &Path,
            full_name: &str,
            default_branch: &str,
            branch: &str,
            _url: &TokenUrl,
        ) -> Result<PathBuf, GitError> {
            self.seen.lock().unwrap().push((
                full_name.to_string(),
                default_branch.to_string(),
                branch.to_string(),
            ));
            Ok(self.worktree.clone())
        }
    }

    fn request(identifier: &str) -> LaunchRequest {
        LaunchRequest {
            issue_id: "issue-1".to_string(),
            issue_identifier: identifier.to_string(),
            device_label: "testbox".to_string(),
            origin: LaunchOrigin::Local,
        }
    }

    /// Deps with: doctor guaranteed green (claude_path = `git` — a real
    /// binary answering `--version`), key pre-seeded (no mint traffic), a
    /// fake worktree provider, and a canned tRPC server.
    fn deps(base: &str, data_dir: &Path, worktree: Arc<FakeWorktrees>) -> CodingDeps {
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
                claude_model: "opus".to_string(),
            },
            issue_seed: Arc::new(|_| {
                Some(IssueSeed {
                    title: "Fix login flicker".to_string(),
                    description: Some("Steps in the issue.".to_string()),
                })
            }),
            worktrees: worktree,
        }
    }

    const FOR_ISSUE_OK: &str = r#"{"result":{"data":{"repositoryId":"repo-1","fullName":"acme/web","defaultBranch":"main"}}}"#;
    const TOKEN_OK: &str = r#"{"result":{"data":{"token":"ghs_secret123","fullName":"acme/web","defaultBranch":"main","expiresAt":"2026-07-03T12:55:00.000Z"}}}"#;
    const START_OK: &str = r#"{"result":{"data":{"session":{"id":"sess-1","issueId":"issue-1","status":"running"}}}}"#;

    // ---- the disabled surfaces (explain, never crash) ----

    #[test]
    fn doctor_failure_disables_without_any_network_call() {
        let dir = temp_dir("doctor");
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("wt"),
            seen: Default::default(),
        });
        // Unroutable base: any network call would error the launch — proving
        // the doctor gate fires first.
        let mut deps = deps("http://127.0.0.1:1", &dir.0, worktrees);
        deps.settings.claude_path = "definitely-not-a-real-binary-exp".to_string();

        match prepare_launch(&request("EXP-42"), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::DoctorFailed(check)) => {
                assert_eq!(check.tool, crate::doctor::Tool::Claude);
                assert_eq!(
                    check.error.as_deref(),
                    Some("claude not found on PATH — set an absolute path")
                );
            }
            other => panic!("expected DoctorFailed, got {other:?}"),
        }
    }

    #[test]
    fn null_repo_is_no_repository_linked() {
        let dir = temp_dir("no-repo");
        let base = canned_server(vec![(200, r#"{"result":{"data":null}}"#.to_string())]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("wt"),
            seen: Default::default(),
        });
        let deps = deps(&base, &dir.0, worktrees.clone());
        match prepare_launch(&request("EXP-42"), &deps).unwrap() {
            Prepared::Disabled(reason @ DisabledReason::NoRepositoryLinked) => {
                // §7.1: the exact helper copy for the disabled button.
                assert_eq!(
                    reason.message(),
                    "Link a repository to this project in workspace settings."
                );
            }
            other => panic!("expected NoRepositoryLinked, got {other:?}"),
        }
        assert!(worktrees.seen.lock().unwrap().is_empty(), "no git on null repo");
    }

    #[test]
    fn app_missing_412_maps_to_github_app_missing() {
        let dir = temp_dir("app-missing");
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (412, r#"{"error":{"message":"The Exponential GitHub App is not installed on acme/web. Reconnect it in workspace settings.","code":-32012,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("wt"),
            seen: Default::default(),
        });
        let deps = deps(&base, &dir.0, worktrees);
        match prepare_launch(&request("EXP-42"), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::GithubAppMissing { full_name, message }) => {
                assert_eq!(full_name, "acme/web");
                assert!(message.contains("not installed"));
            }
            other => panic!("expected GithubAppMissing, got {other:?}"),
        }
    }

    #[test]
    fn token_403_maps_to_token_denied() {
        let dir = temp_dir("denied");
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (403, r#"{"error":{"message":"You are not a member of this workspace","code":-32003,"data":{"code":"FORBIDDEN","httpStatus":403}}}"#.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("wt"),
            seen: Default::default(),
        });
        let deps = deps(&base, &dir.0, worktrees);
        match prepare_launch(&request("EXP-42"), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::TokenDenied { message }) => {
                assert!(message.contains("not a member"));
            }
            other => panic!("expected TokenDenied, got {other:?}"),
        }
    }

    #[test]
    fn session_limit_412_maps_to_session_limit_with_upgrade_copy() {
        let dir = temp_dir("limit");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (412, r#"{"error":{"message":"Concurrent coding session limit reached — upgrade to run more.","code":-32012,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree,
            seen: Default::default(),
        });
        let deps = deps(&base, &dir.0, worktrees);
        match prepare_launch(&request("EXP-42"), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::SessionLimit { message }) => {
                assert!(message.contains("upgrade"));
            }
            other => panic!("expected SessionLimit, got {other:?}"),
        }
    }

    // ---- the happy path through steps 1–6 ----

    #[test]
    fn prepare_launch_full_sequence() {
        let dir = temp_dir("happy");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let deps = deps(&base, &dir.0, worktrees.clone());

        let prepared = match prepare_launch(&request("EXP-42"), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            Prepared::Disabled(reason) => panic!("unexpectedly disabled: {reason:?}"),
        };

        // Session id from codingSessions.start keys everything downstream.
        assert_eq!(prepared.session_id, "sess-1");
        assert_eq!(prepared.branch, "exp/EXP-42");
        assert_eq!(prepared.worktree, worktree);
        assert_eq!(prepared.tab_title, "claude · EXP-42");

        // Step 7's spawn spec: configured program, explicit --model, the skip
        // flag, the seed line as the positional prompt (never typed into the
        // PTY), worktree cwd. Model is ALWAYS passed (§7.7).
        assert_eq!(prepared.spawn.program, "git"); // test claude_path
        assert_eq!(
            prepared.spawn.args,
            vec![
                "--model",
                "opus",
                "--dangerously-skip-permissions",
                "Read PROMPT.md in this directory, then follow it.",
            ]
        );
        assert_eq!(prepared.spawn.cwd.as_deref(), Some(worktree.as_path()));

        // Step 3 got the server-confirmed repo + §7.1 branch name.
        let seen = worktrees.seen.lock().unwrap();
        assert_eq!(
            seen.as_slice(),
            &[(
                "acme/web".to_string(),
                "main".to_string(),
                "exp/EXP-42".to_string()
            )]
        );

        // Step 4: .mcp.json carries the stored key + the instance /api/mcp.
        let mcp = fs::read_to_string(worktree.join(".mcp.json")).unwrap();
        assert!(mcp.contains(&format!("{base}/api/mcp")));
        assert!(mcp.contains("Bearer expu_seeded"));

        // Step 5: PROMPT.md templated from the sync-store seed.
        let prompt = fs::read_to_string(worktree.join("PROMPT.md")).unwrap();
        assert!(prompt.contains("**EXP-42: Fix login flicker**"));
        assert!(prompt.contains("Steps in the issue."));
        assert!(prompt.contains("`exponential_pr_open`"));
    }

    // ---- The hidden key auto-mints on the FIRST coding session ----

    const MINT_OK: &str = r#"{"result":{"data":{"key":"expu_minted_runtime","id":"key-9","name":"Device: box","start":"expu_mi","prefix":"expu_","createdAt":"2026-07-03T10:00:00.000Z"}}}"#;

    /// §7.2 runtime path: an EMPTY token store at launch time silently mints
    /// via `users.mintPersonalApiKey` (request 3 — the key race lands between
    /// `installationToken` and `codingSessions.start`), stores the raw key +
    /// row id, and `.mcp.json` carries the fresh key. No manual key UI exists
    /// anywhere; this is the only way the key ever comes to be.
    #[test]
    fn first_session_auto_mints_the_hidden_personal_key() {
        let dir = temp_dir("auto-mint");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, MINT_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let deps = deps(&base, &dir.0, worktrees);
        // The launcher finds NO stored key — the runtime auto-mint must fire.
        deps.token_store.delete("acct", SecretKind::PersonalApiKey);
        assert_eq!(deps.token_store.get("acct", SecretKind::PersonalApiKey), None);

        match prepare_launch(&request("EXP-42"), &deps).unwrap() {
            Prepared::Ready(prepared) => assert_eq!(prepared.session_id, "sess-1"),
            other => panic!("expected Ready, got {other:?}"),
        }

        // Minted key + row id stashed for later sessions / Regenerate (§7.2).
        assert_eq!(
            deps.token_store
                .get("acct", SecretKind::PersonalApiKey)
                .as_deref(),
            Some("expu_minted_runtime")
        );
        assert_eq!(
            deps.token_store
                .get("acct", SecretKind::PersonalApiKeyId)
                .as_deref(),
            Some("key-9")
        );
        // .mcp.json is the ONLY on-disk consumer of the raw key (§7.1 step 4).
        let mcp = fs::read_to_string(worktree.join(".mcp.json")).unwrap();
        assert!(mcp.contains("Bearer expu_minted_runtime"), "mcp: {mcp}");
    }

    // ---- §7.6: N issues = N worktrees/branches/sessions (client never
    //      self-throttles; per-launch state never bleeds across issues) ----

    #[test]
    fn two_issues_prepare_into_isolated_worktrees_and_sessions() {
        let dir = temp_dir("concurrent-prep");
        let wt_a = dir.0.join("wt-a");
        let wt_b = dir.0.join("wt-b");
        fs::create_dir_all(&wt_a).unwrap();
        fs::create_dir_all(&wt_b).unwrap();

        let launch = |identifier: &str, issue_id: &str, session: &str, worktree: &PathBuf| {
            let base = canned_server(vec![
                (200, FOR_ISSUE_OK.to_string()),
                (200, TOKEN_OK.to_string()),
                (
                    200,
                    format!(
                        r#"{{"result":{{"data":{{"session":{{"id":"{session}","issueId":"{issue_id}","status":"running"}}}}}}}}"#
                    ),
                ),
            ]);
            let worktrees = Arc::new(FakeWorktrees {
                worktree: worktree.clone(),
                seen: Default::default(),
            });
            let deps = deps(&base, &dir.0, worktrees);
            let mut req = request(identifier);
            req.issue_id = issue_id.to_string();
            match prepare_launch(&req, &deps).unwrap() {
                Prepared::Ready(prepared) => prepared,
                other => panic!("expected Ready, got {other:?}"),
            }
        };

        let a = launch("EXP-1", "issue-a", "sess-a", &wt_a);
        let b = launch("EXP-2", "issue-b", "sess-b", &wt_b);

        // Distinct branches, worktrees, session ids, tab titles — the §7.6
        // "no collision" invariant at the prepare layer (the manager-side
        // PTY/tab isolation is tests/concurrent.rs).
        assert_eq!(a.branch, "exp/EXP-1");
        assert_eq!(b.branch, "exp/EXP-2");
        assert_ne!(a.worktree, b.worktree);
        assert_ne!(a.session_id, b.session_id);
        assert_eq!(a.tab_title, "claude · EXP-1");
        assert_eq!(b.tab_title, "claude · EXP-2");
        // Both spawn specs are cwd-bound to their OWN worktree.
        assert_eq!(a.spawn.cwd.as_deref(), Some(wt_a.as_path()));
        assert_eq!(b.spawn.cwd.as_deref(), Some(wt_b.as_path()));
    }

    #[test]
    fn default_device_label_is_never_empty() {
        assert!(!default_device_label().trim().is_empty());
    }

    #[test]
    fn prompt_falls_back_to_identifier_when_sync_store_misses() {
        let dir = temp_dir("fallback");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let mut deps = deps(&base, &dir.0, worktrees);
        deps.issue_seed = Arc::new(|_| None);

        match prepare_launch(&request("EXP-7"), &deps).unwrap() {
            Prepared::Ready(_) => {}
            other => panic!("expected Ready, got {other:?}"),
        }
        let prompt = fs::read_to_string(worktree.join("PROMPT.md")).unwrap();
        assert!(prompt.contains("**EXP-7: EXP-7**"));
        assert!(prompt.contains("(no description)"));
    }
}
