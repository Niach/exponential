//! The RELEASE-run launcher (EXP-56) — "start coding on a whole release":
//! ONE Claude orchestrator session per (release, repo group). Mirrors
//! [`crate::launcher::prepare_launch`]'s background/foreground split and
//! reuses its [`Prepared`]/[`DisabledReason`]/[`CodingError`] surfaces and
//! the [`WorktreeProvider`] git path, so the ui layer spawns the result
//! through the SAME [`crate::launcher::spawn_prepared_with`].
//!
//! Differences from the single-issue sequence:
//! - Claude-ONLY, regardless of the `codingAgent` setting (the orchestration
//!   depends on Claude subagents + MCP; the dialog gates, this enforces);
//! - the repo is resolved by the DIALOG (issues grouped by repo, one launch
//!   per group) — no `repositories.forIssue` here;
//! - the session worktree holds the pushed integration branch
//!   `exp/rel-<slug>` (lowercase ⇒ the webhook's issue-identifier parse can
//!   never mis-link it — guarded by a test below);
//! - `PROMPT.md` is the wave/integration orchestration template
//!   ([`crate::release_prompt`]), and per-issue subagents ride `--agents`
//!   ([`crate::agents_json`]) when the CLI supports it;
//! - the `coding_sessions` row is RELEASE-scoped
//!   ([`api::coding_sessions::start_release`]).

use std::collections::BTreeMap;
use std::sync::Arc;

use api::error::ApiError;
use api::{coding_sessions, repositories, users};
use terminal::pty::SpawnSpec;

use crate::agent::Agent;
use crate::agents_json::{build_agents_json, SubagentDefaults};
use crate::doctor::{run_doctor_for, ClaudeFlagSupport};
use crate::git_worktree::{branch_name, clone_path, worktree_path, TokenUrl};
use crate::launcher::{CodingDeps, CodingError, DisabledReason, LaunchOrigin, Prepared, PreparedLaunch};
use crate::mcp_json::write_mcp_json;
use crate::prompt::{write_rendered_prompt, SEED_LINE};
use crate::release_prompt::{render_release_prompt, ReleasePromptArgs, ReleasePromptIssue};

/// The repo group this session works (a workspace-level release may span
/// repos; the dialog launches one orchestrator per group).
#[derive(Clone, Debug)]
pub struct RepoGroup {
    pub repository_id: String,
    /// `owner/name`.
    pub full_name: String,
    pub default_branch: String,
}

/// One selected issue, snapshotted by the dialog from the sync store.
#[derive(Clone, Debug)]
pub struct ReleaseIssueSpec {
    pub issue_id: String,
    /// `EXP-42`.
    pub issue_identifier: String,
    pub title: String,
    pub description: Option<String>,
    /// Dialog per-issue subagent overrides (None = inherit the dialog's
    /// subagent defaults).
    pub model_override: Option<String>,
    pub effort_override: Option<String>,
}

/// The dialog's model/effort/mode choices.
#[derive(Clone, Debug)]
pub struct ReleaseLaunchOptions {
    /// Orchestrator model — pinned to `opus` when `ultracode` engages.
    pub main_model: String,
    /// Orchestrator effort; blank/None = omit. Ignored while ultracode is on
    /// (ultracode IS the effort mode).
    pub main_effort: Option<String>,
    /// Subagent defaults (per-issue overrides win).
    pub subagent_model: String,
    pub subagent_effort: Option<String>,
    /// Dynamic workflows. Engages only when the CLI supports `--settings`
    /// (doctor probe) — degrades to a plain run otherwise.
    pub ultracode: bool,
    /// false = one decomposition gate in the prompt.
    pub autonomous: bool,
}

/// The dialog's launch input for ONE repo group.
#[derive(Clone, Debug)]
pub struct ReleaseLaunchRequest {
    pub release_id: String,
    pub release_name: String,
    pub repo: RepoGroup,
    /// Only issues resolving to `repo`, already filtered to launchable ones.
    pub issues: Vec<ReleaseIssueSpec>,
    pub device_label: String,
    pub origin: LaunchOrigin,
    pub options: ReleaseLaunchOptions,
}

/// Lowercase, webhook-safe slug for the integration branch: `<name-slug>-<id8>`.
/// The name part keeps `[a-z0-9]` runs joined by single dashes; the first 8
/// chars of the release UUID make the branch unique per release even for
/// same-named releases. EVERY char is lowercase alnum or `-`, so the branch's
/// last path segment can never match the webhook's `[A-Z0-9]+-\d+$` issue
/// parse (locked by a test below against the ported regex).
pub fn release_slug(release_name: &str, release_id: &str) -> String {
    let mut name_part = String::new();
    let mut last_dash = true; // suppress leading dashes
    for ch in release_name.chars() {
        if ch.is_ascii_alphanumeric() {
            name_part.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            name_part.push('-');
            last_dash = true;
        }
    }
    let name_part = name_part.trim_end_matches('-');
    let id_part: String = release_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase();
    if name_part.is_empty() {
        format!("release-{id_part}")
    } else {
        format!("{name_part}-{id_part}")
    }
}

/// `exp/rel-<slug>` — the pushed integration branch. Deliberately NOT the
/// user's branch prefix: the `rel-` marker + lowercase slug are the webhook
/// safety guarantee, independent of prefix configuration.
pub fn release_branch_name(slug: &str) -> String {
    format!("exp/rel-{slug}")
}

/// The orchestrator argv. `--model` explicit-always (pinned to opus under
/// ultracode); `--effort` only without ultracode, when set AND supported;
/// `--settings {"ultracode":true}` when requested AND supported; `--agents`
/// carries the per-issue subagent defs when supported; the skip flag and the
/// positional seed line close it out, exactly like the single-issue argv.
pub fn release_args(
    opts: &ReleaseLaunchOptions,
    agents_json: Option<&str>,
    flags: &ClaudeFlagSupport,
) -> Vec<String> {
    let ultracode = opts.ultracode && flags.settings;
    let mut args = vec![
        "--model".to_string(),
        if ultracode { "opus".to_string() } else { opts.main_model.clone() },
    ];
    if !ultracode {
        if let Some(effort) = opts.main_effort.as_deref().map(str::trim) {
            if !effort.is_empty() && flags.effort {
                args.push("--effort".to_string());
                args.push(effort.to_string());
            }
        }
    }
    if ultracode {
        args.push("--settings".to_string());
        args.push(r#"{"ultracode":true}"#.to_string());
    }
    if let Some(json) = agents_json {
        args.push("--agents".to_string());
        args.push(json.to_string());
    }
    args.push("--dangerously-skip-permissions".to_string());
    args.push(SEED_LINE.to_string());
    args
}

/// Steps 0–6 of the release sequence (blocking; background executor).
/// Foreground follow-up is the shared [`crate::launcher::spawn_prepared_with`].
pub fn prepare_release_launch(
    req: &ReleaseLaunchRequest,
    deps: &CodingDeps,
) -> Result<Prepared, CodingError> {
    // Step 0 — doctor CLAUDE explicitly (release runs are Claude-only, the
    // `codingAgent` setting does not apply) + the launch-flag probe.
    let report = run_doctor_for(Agent::Claude, &deps.settings);
    if let Some(failed) = report.first_failure() {
        return Ok(Prepared::Disabled(DisabledReason::DoctorFailed(failed.clone())));
    }
    let flags = report.claude_flags;

    // Step 1 — the dialog already resolved the repo group; trust it.
    // Step 2 — JIT installation token (same mapping as the issue launcher).
    let token = match repositories::installation_token(&deps.trpc, &req.repo.repository_id) {
        Ok(token) => token,
        Err(ApiError::Http { status: 412, message }) => {
            return Ok(Prepared::Disabled(DisabledReason::GithubAppMissing {
                full_name: req.repo.full_name.clone(),
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

    // Personal-key mint races the git prep (always MCP for release runs).
    let key_handle = {
        let trpc = Arc::clone(&deps.trpc);
        let store = Arc::clone(&deps.token_store);
        let account_id = deps.account_id.clone();
        std::thread::spawn(move || users::ensure_personal_key(&trpc, &store, &account_id))
    };

    // Step 3 — the session worktree on the integration branch, cut from
    // origin/<default> exactly like an issue worktree.
    let slug = release_slug(&req.release_name, &req.release_id);
    let integration_branch = release_branch_name(&slug);
    let repos_root = deps.settings.repos_root_path();
    let url = TokenUrl::new(token.full_name.clone(), token.token.clone());
    let worktree = deps.worktrees.prepare(
        &repos_root,
        &token.full_name,
        &token.default_branch,
        &integration_branch,
        &url,
    )?;
    let clone = clone_path(&repos_root, &token.full_name);

    // Step 4 — .mcp.json (subagents inherit the session's MCP servers).
    let personal_key = key_handle
        .join()
        .map_err(|_| CodingError::Io("personal-key thread panicked".to_string()))??;
    write_mcp_json(&worktree, deps.trpc.base_url(), &personal_key)
        .map_err(|e| CodingError::Io(format!("write .mcp.json: {e}")))?;

    // Step 5 — the orchestration PROMPT.md + the per-issue subagent defs.
    // Issue branches/worktrees use the SAME prefix + layout as single-issue
    // launches, so a partially-coded issue reuses its worktree idempotently.
    let prompt_issues: Vec<ReleasePromptIssue> = req
        .issues
        .iter()
        .map(|issue| ReleasePromptIssue {
            identifier: issue.issue_identifier.clone(),
            title: issue.title.clone(),
            description: issue.description.clone(),
            branch: branch_name(&deps.settings.branch_prefix, &issue.issue_identifier),
            worktree: worktree_path(
                &clone,
                &branch_name(&deps.settings.branch_prefix, &issue.issue_identifier),
            )
            .to_string_lossy()
            .into_owned(),
            agent_name: issue.issue_identifier.to_ascii_lowercase(),
        })
        .collect();

    let agents_json = flags.agents.then(|| {
        let overrides: BTreeMap<String, (Option<String>, Option<String>)> = req
            .issues
            .iter()
            .map(|issue| {
                (
                    issue.issue_identifier.clone(),
                    (issue.model_override.clone(), issue.effort_override.clone()),
                )
            })
            .collect();
        build_agents_json(
            &prompt_issues,
            &SubagentDefaults {
                model: &req.options.subagent_model,
                effort: req.options.subagent_effort.as_deref(),
                effort_supported: flags.effort,
            },
            &overrides,
        )
    });

    let prompt = render_release_prompt(&ReleasePromptArgs {
        release_id: &req.release_id,
        release_name: &req.release_name,
        repository_id: &req.repo.repository_id,
        default_branch: &token.default_branch,
        integration_branch: &integration_branch,
        autonomous: req.options.autonomous,
        agents_predefined: agents_json.is_some(),
        issues: &prompt_issues,
    });
    write_rendered_prompt(&worktree, &prompt)
        .map_err(|e| CodingError::Io(format!("write PROMPT.md: {e}")))?;

    // Step 6 — the RELEASE-scoped session row, BEFORE spawn.
    let session = match coding_sessions::start_release(
        &deps.trpc,
        &req.release_id,
        Some(&req.device_label),
    ) {
        Ok(session) => session,
        Err(ApiError::Http { status: 412, message }) => {
            return Ok(Prepared::Disabled(DisabledReason::SessionLimit { message }))
        }
        Err(err) => return Err(err.into()),
    };

    // Step 7's spawn spec — always the resolved CLAUDE program.
    let spawn = SpawnSpec::new(&deps.settings.resolved_claude_path())
        .args(release_args(&req.options, agents_json.as_deref(), &flags))
        .cwd(&worktree);

    Ok(Prepared::Ready(PreparedLaunch {
        session_id: session.id,
        issue_identifier: req.release_name.clone(),
        worktree,
        clone,
        repository_id: req.repo.repository_id.clone(),
        branch: integration_branch,
        spawn,
        tab_title: format!("claude · release {}", req.release_name),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::WorktreeProvider;
    use crate::settings::Settings;
    use api::token_store::{SecretKind, TokenStore};
    use api::trpc::TrpcClient;
    use api::StaticToken;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    // ---- harness (mirrors launcher.rs's) ----

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
            "exp-coding-release-{tag}-{}-{nanos}",
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
        ) -> Result<PathBuf, crate::git_worktree::GitError> {
            self.seen.lock().unwrap().push((
                full_name.to_string(),
                default_branch.to_string(),
                branch.to_string(),
            ));
            Ok(self.worktree.clone())
        }
    }

    fn make_deps(base: &str, data_dir: &Path, worktrees: Arc<FakeWorktrees>) -> CodingDeps {
        let store = TokenStore::file_only(data_dir.to_path_buf());
        store
            .set("acct", SecretKind::PersonalApiKey, "expu_seeded")
            .unwrap();
        CodingDeps {
            trpc: Arc::new(TrpcClient::new(base, Arc::new(StaticToken("tok".into())))),
            token_store: Arc::new(store),
            account_id: "acct".to_string(),
            settings: Settings {
                claude_path: "git".to_string(), // doctor-green stand-in
                repos_root: data_dir.join("repos").to_string_lossy().into_owned(),
                branch_prefix: "exp/".to_string(),
                claude_model: "opus".to_string(),
                ..Settings::default()
            },
            issue_seed: Arc::new(|_| None),
            worktrees,
        }
    }

    fn options() -> ReleaseLaunchOptions {
        ReleaseLaunchOptions {
            main_model: "opus".to_string(),
            main_effort: Some("high".to_string()),
            subagent_model: "opus".to_string(),
            subagent_effort: Some("high".to_string()),
            ultracode: true,
            autonomous: true,
        }
    }

    fn request() -> ReleaseLaunchRequest {
        ReleaseLaunchRequest {
            release_id: "1dc5fb4a-8923-471c-a940-53094cd33b76".to_string(),
            release_name: "0.4".to_string(),
            repo: RepoGroup {
                repository_id: "repo-1".to_string(),
                full_name: "acme/web".to_string(),
                default_branch: "main".to_string(),
            },
            issues: vec![
                ReleaseIssueSpec {
                    issue_id: "issue-1".to_string(),
                    issue_identifier: "EXP-42".to_string(),
                    title: "Fix login flicker".to_string(),
                    description: Some("Steps.".to_string()),
                    model_override: None,
                    effort_override: None,
                },
                ReleaseIssueSpec {
                    issue_id: "issue-2".to_string(),
                    issue_identifier: "EXP-43".to_string(),
                    title: "Add badge".to_string(),
                    description: None,
                    model_override: Some("sonnet".to_string()),
                    effort_override: None,
                },
            ],
            device_label: "testbox".to_string(),
            origin: LaunchOrigin::Local,
            options: options(),
        }
    }

    const TOKEN_OK: &str = r#"{"result":{"data":{"token":"ghs_secret123","fullName":"acme/web","defaultBranch":"main","expiresAt":"2026-07-11T12:55:00.000Z"}}}"#;
    const START_RELEASE_OK: &str = r#"{"result":{"data":{"session":{"id":"sess-rel","releaseId":"1dc5fb4a-8923-471c-a940-53094cd33b76","status":"running"}}}}"#;

    // ---- slug / branch webhook safety ----

    /// The server's `parseIssueIdentifierFromBranch` regex, ported: the last
    /// `/`-segment must be ENTIRELY `[A-Z0-9]+-\d+` to link an issue (see
    /// apps/web/src/lib/integrations/pr-sync.ts — its test suite locks the
    /// same release-branch cases). The integration branch must NEVER satisfy
    /// it.
    fn parses_as_issue_branch(branch: &str) -> bool {
        let tail = branch.rsplit('/').next().unwrap_or(branch);
        let Some(dash) = tail.rfind('-') else { return false };
        let (head, digits) = (&tail[..dash], &tail[dash + 1..]);
        !head.is_empty()
            && !digits.is_empty()
            && digits.chars().all(|c| c.is_ascii_digit())
            && head
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    }

    #[test]
    fn release_branches_can_never_match_the_issue_webhook_parse() {
        for (name, id) in [
            ("0.4", "1dc5fb4a-8923-471c-a940-53094cd33b76"),
            ("July Wave!", "abcdef01-2222-3333-4444-555555555555"),
            ("V2", "12345678-9999-0000-1111-222222222222"),
            ("", "deadbeef-1111-2222-3333-444444444444"),
            ("ÜBER release 7", "00112233-4455-6677-8899-aabbccddeeff"),
        ] {
            let branch = release_branch_name(&release_slug(name, id));
            assert!(
                branch.starts_with("exp/rel-"),
                "branch {branch:?} lost its rel- marker"
            );
            let tail = branch.rsplit('/').next().unwrap();
            assert!(
                tail.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "branch tail must be lowercase-safe: {branch:?}"
            );
            assert!(
                !parses_as_issue_branch(&branch),
                "release branch {branch:?} would link as an issue!"
            );
        }
        // Sanity: the checker itself recognizes real issue branches.
        assert!(parses_as_issue_branch("exp/EXP-42"));
        assert!(!parses_as_issue_branch("exp/rel-0-4-1dc5fb4a"));
    }

    #[test]
    fn slug_is_stable_lowercase_and_unique_per_release() {
        assert_eq!(
            release_slug("0.4", "1dc5fb4a-8923-471c-a940-53094cd33b76"),
            "0-4-1dc5fb4a"
        );
        assert_eq!(
            release_slug("July  Wave!", "abcdef01-1111-2222-3333-444444444444"),
            "july-wave-abcdef01"
        );
        // Same name, different release ⇒ different branch.
        assert_ne!(
            release_slug("0.4", "1dc5fb4a-8923-471c-a940-53094cd33b76"),
            release_slug("0.4", "9dc5fb4a-8923-471c-a940-53094cd33b76")
        );
        // Nothing usable in the name ⇒ id-only fallback.
        assert_eq!(
            release_slug("!!!", "deadbeef-0000-1111-2222-333333333333"),
            "release-deadbeef"
        );
    }

    // ---- argv matrix ----

    #[test]
    fn release_args_matrix() {
        let all = ClaudeFlagSupport { effort: true, agents: true, settings: true };
        // Ultracode: opus pinned, no --effort, --settings, --agents.
        let args = release_args(&options(), Some(r#"{"exp-42":{}}"#), &all);
        assert_eq!(
            args,
            vec![
                "--model",
                "opus",
                "--settings",
                r#"{"ultracode":true}"#,
                "--agents",
                r#"{"exp-42":{}}"#,
                "--dangerously-skip-permissions",
                SEED_LINE,
            ]
        );

        // Ultracode requested but CLI lacks --settings ⇒ degrade to a plain
        // run: main model + effort apply, no --settings.
        let degraded = ClaudeFlagSupport { effort: true, agents: true, settings: false };
        let mut opts = options();
        opts.main_model = "fable".to_string();
        let args = release_args(&opts, None, &degraded);
        assert_eq!(
            args,
            vec![
                "--model",
                "fable",
                "--effort",
                "high",
                "--dangerously-skip-permissions",
                SEED_LINE,
            ]
        );

        // Ultracode OFF: model + effort; effort dropped when unsupported.
        let mut opts = options();
        opts.ultracode = false;
        let args = release_args(&opts, None, &all);
        assert!(args.windows(2).any(|w| w == ["--effort", "high"]));
        let none = ClaudeFlagSupport::default();
        let args = release_args(&opts, None, &none);
        assert!(!args.iter().any(|a| a == "--effort"));
        assert!(!args.iter().any(|a| a == "--settings"));
        assert!(!args.iter().any(|a| a == "--agents"));
    }

    // ---- the full prepare sequence ----

    #[test]
    fn prepare_release_launch_full_sequence() {
        let dir = temp_dir("happy");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, TOKEN_OK.to_string()),
            (200, START_RELEASE_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees.clone());

        let prepared = match prepare_release_launch(&request(), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            Prepared::Disabled(reason) => panic!("unexpectedly disabled: {reason:?}"),
        };

        assert_eq!(prepared.session_id, "sess-rel");
        assert_eq!(prepared.branch, "exp/rel-0-4-1dc5fb4a");
        assert_eq!(prepared.tab_title, "claude · release 0.4");
        // P9 refresher inputs ride along (repo id from the request's group,
        // clone path under the repos root).
        assert_eq!(prepared.repository_id, "repo-1");
        assert_eq!(prepared.clone, dir.0.join("repos").join("acme").join("web"));

        // Git prepared the INTEGRATION branch from the server-confirmed repo.
        let seen = worktrees.seen.lock().unwrap();
        assert_eq!(
            seen.as_slice(),
            &[(
                "acme/web".to_string(),
                "main".to_string(),
                "exp/rel-0-4-1dc5fb4a".to_string()
            )]
        );

        // .mcp.json (subagents inherit it).
        let mcp = fs::read_to_string(worktree.join(".mcp.json")).unwrap();
        assert!(mcp.contains("Bearer expu_seeded"));

        // PROMPT.md: the orchestration template with both issues, the
        // integration branch, and the release-PR tool inputs.
        let prompt = fs::read_to_string(worktree.join("PROMPT.md")).unwrap();
        assert!(prompt.contains("RELEASE ORCHESTRATOR"));
        assert!(prompt.contains("### EXP-42: Fix login flicker"));
        assert!(prompt.contains("### EXP-43: Add badge"));
        assert!(prompt.contains("exp/rel-0-4-1dc5fb4a"));
        assert!(prompt.contains("releaseId `1dc5fb4a-8923-471c-a940-53094cd33b76`"));
        assert!(prompt.contains("repositoryId `repo-1`"));
        // Issue worktrees ride the single-issue layout under the clone.
        assert!(prompt.contains("web.worktrees"));

        // The spawn args: git (the test claude stand-in) has no claude flags,
        // so the probe strips --settings/--agents/--effort and the argv
        // degrades to the plain orchestrator invocation.
        assert_eq!(prepared.spawn.program, "git");
        assert_eq!(
            prepared.spawn.args,
            vec![
                "--model".to_string(),
                "opus".to_string(),
                "--dangerously-skip-permissions".to_string(),
                SEED_LINE.to_string(),
            ]
        );
        assert_eq!(prepared.spawn.cwd.as_deref(), Some(worktree.as_path()));
        // Degraded ⇒ the prompt itself must carry the generic-subagent spawn
        // instructions instead of referencing pre-defined agents.
        assert!(prompt.contains("general-purpose subagent"));
    }

    #[test]
    fn session_limit_and_token_denied_map_like_the_issue_launcher() {
        let dir = temp_dir("limit");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, TOKEN_OK.to_string()),
            (412, r#"{"error":{"message":"Concurrent coding session limit reached — upgrade to run more.","code":-32012,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees { worktree, seen: Default::default() });
        let deps = make_deps(&base, &dir.0, worktrees);
        match prepare_release_launch(&request(), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::SessionLimit { message }) => {
                assert!(message.contains("upgrade"));
            }
            other => panic!("expected SessionLimit, got {other:?}"),
        }

        let dir = temp_dir("denied");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![(
            403,
            r#"{"error":{"message":"You are not a member of this workspace","code":-32003,"data":{"code":"FORBIDDEN","httpStatus":403}}}"#.to_string(),
        )]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree,
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        match prepare_release_launch(&request(), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::TokenDenied { message }) => {
                assert!(message.contains("not a member"));
            }
            other => panic!("expected TokenDenied, got {other:?}"),
        }
    }

    /// Release runs are Claude-only: even with the codex opt-in active, the
    /// doctor targets claude and the spawn resolves the claude program.
    #[test]
    fn codex_setting_does_not_apply_to_release_runs() {
        let dir = temp_dir("claude-only");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, TOKEN_OK.to_string()),
            (200, START_RELEASE_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees);
        deps.settings.coding_agent = "codex".to_string();
        deps.settings.codex_path = "definitely-not-a-real-binary-exp".to_string();

        // codex path is dead but claude (the `git` stand-in) is green — a
        // codex-agnostic release run must still launch.
        match prepare_release_launch(&request(), &deps).unwrap() {
            Prepared::Ready(prepared) => {
                assert_eq!(prepared.spawn.program, "git");
                assert!(worktree.join(".mcp.json").exists());
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }
}
