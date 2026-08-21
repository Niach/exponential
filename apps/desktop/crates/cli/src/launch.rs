//! Building the three `coding::PrepareRequest` shapes from CLI/daemon
//! inputs — the headless port of the desktop's `coding_flow::build_launch`
//! + `action_run::start_action_run` resolution, backed by tRPC instead of
//! the Electric sync store (`issues.get`, `actions.get`,
//! `repositories.list`/`forIssue`).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context as _};
use api::actions::BUILTIN_FIX_CONFLICTS_ID;
use api::issues::FetchedIssue;
use coding::{
    ActionInputValue, ActionLaunchRequest, ActionRunKind, CodingDeps, GitWorktrees, IssueSeed,
    LaunchOptions, LaunchOrigin, LaunchRequest, RepoGroup,
};
use domain::IssueStatus;
use serde::Deserialize;

use crate::context::Ctx;

/// Agent-related flags shared by `code` and `run`.
#[derive(Debug, Default)]
pub struct AgentFlags {
    pub agent: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub plan: bool,
    pub skip_permissions: bool,
}

/// Resolve flags over the settings defaults with the desktop's capability
/// masking. Detached (non-tty) runs default plan mode OFF — the F7 rule:
/// an unattended session must never park at the plan-approval TUI unless
/// explicitly asked to (`--plan`; it stays steerable from the web).
pub fn agent_options(
    settings: &coding::Settings,
    flags: &AgentFlags,
    interactive: bool,
) -> anyhow::Result<LaunchOptions> {
    let agent = match flags.agent.as_deref() {
        None => settings.default_agent,
        Some(raw) => coding::CodingAgent::parse(raw)
            .ok_or_else(|| anyhow!("unknown agent `{raw}` (claude, codex or pi)"))?,
    };
    let mut options = LaunchOptions::defaults_for(settings, agent);
    if let Some(model) = &flags.model {
        options.model = model.clone();
    }
    if let Some(effort) = &flags.effort {
        options.effort = effort.clone();
    }
    if !interactive {
        options.plan_mode = false;
    }
    if flags.plan {
        if !agent.supports_plan_mode() {
            bail!("--plan is not available for {}", agent.id());
        }
        options.plan_mode = true;
    }
    if flags.skip_permissions {
        if !agent.supports_skip_permissions() {
            bail!("--skip-permissions is not available for {}", agent.id());
        }
        options.skip_permissions = true;
    }
    Ok(options)
}

/// Who hosts a launch — the ONE thing that decides whether the session row
/// may carry this machine's `device_id` (EXP-550).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchHost {
    /// `exponential daemon`: it heartbeats the `devices` row, so a session
    /// stamped with this device resolves ONLINE on every client.
    Daemon,
    /// A foreground `exponential code` / `run`: nothing keeps the `devices`
    /// row fresh. Stamping the id there makes a perfectly live session read
    /// as "Paused, device offline" the moment a past daemon's row goes
    /// stale, so these runs send NO device id — the server falls back to the
    /// hostname label (`resolveSessionDevice` → `online: null`, never
    /// paused).
    Foreground,
}

impl LaunchHost {
    fn device_id(self, ctx: &Ctx) -> Option<String> {
        match self {
            LaunchHost::Daemon => Some(ctx.device_id()),
            LaunchHost::Foreground => None,
        }
    }
}

/// The injected launcher collaborators, seeded with the issues the caller
/// already fetched (the seed fn runs off-thread inside `prepare`).
pub fn coding_deps(ctx: &Ctx, seeds: HashMap<String, IssueSeed>, host: LaunchHost) -> CodingDeps {
    CodingDeps {
        trpc: Arc::clone(&ctx.trpc),
        token_store: Arc::clone(&ctx.token_store),
        account_id: ctx.account.id.clone(),
        settings: ctx.settings.clone(),
        issue_seed: Arc::new(move |issue_id: &str| seeds.get(issue_id).cloned()),
        worktrees: Arc::new(GitWorktrees),
        codex_sessions_root: None,
        // A relay-origin start still stamps the frame's own deviceId
        // (`attribution` prefers it) — this is only the fallback for starts
        // that name no device.
        device_id: host.device_id(ctx),
        data_dir: ctx.data_dir.clone(),
    }
}

pub fn issue_seed(issue: &FetchedIssue) -> IssueSeed {
    IssueSeed {
        title: issue.title.clone(),
        description: issue.description.clone(),
    }
}

/// `resume` (EXP-481): honor a remote start's resume flag — the launcher's
/// `.exp-agents` marker gate degrades a mismatched/missing worktree to a
/// fresh session seeded with the resume prompt, so an optimistic flag is
/// always safe. Local `code` starts stay fresh (no `--resume` flag yet).
pub fn issue_launch_request(
    issue: &FetchedIssue,
    options: LaunchOptions,
    origin: LaunchOrigin,
    resume: bool,
) -> LaunchRequest {
    LaunchRequest {
        issue_id: issue.id.clone(),
        issue_identifier: issue.identifier.clone(),
        issue_status: IssueStatus::from_wire(issue.status.as_deref().unwrap_or("")),
        device_label: coding::default_device_label(),
        origin,
        options,
        resume,
    }
}

/// Slim `repositories.list` row (the ui crate's `ActionRepoRow` twin).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoRow {
    pub id: String,
    pub full_name: String,
    #[serde(default)]
    pub default_branch: Option<String>,
}

pub fn fetch_repositories(
    trpc: &api::trpc::TrpcClient,
    team_id: &str,
) -> Result<Vec<RepoRow>, api::ApiError> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.query_with_input("repositories.list", &Input { team_id })
}

/// `teams.getDefault` — the non-creating default-team resolver (oldest
/// membership or null); the CLI's team fallback for builtin action runs.
pub fn default_team_id(trpc: &api::trpc::TrpcClient) -> anyhow::Result<String> {
    #[derive(Deserialize)]
    struct Output {
        #[serde(default)]
        team: Option<TeamRow>,
    }
    #[derive(Deserialize)]
    struct TeamRow {
        id: String,
    }
    let output: Output = trpc
        .query("teams.getDefault")
        .context("resolve the default team")?;
    output
        .team
        .map(|team| team.id)
        .ok_or_else(|| anyhow!("You are not a member of any team yet."))
}

/// How an action run's repo group arrives (ui `ActionRepo` twin).
pub enum ActionRepo {
    /// Remote start: the frame's server-resolved group (`None` = repo-less).
    Provided(Option<RepoGroup>),
    /// Local start: resolve `action.repository_id` via `repositories.list`.
    Resolve,
}

/// The `action_run::start_action_run` resolution, headless: builtins are
/// constructed locally (the server rejects `actions.get` for their ids),
/// team actions fetch the FRESH body, fix-conflicts resolves its `pr` input
/// (issue UUID or identifier) to the PR branch + repository via tRPC.
/// `trigger` (EXP-530) is set only by the daemon's automation host — it feeds
/// the prompt's `## Trigger` section and the row's `started_reason`; every
/// user-initiated start passes `None`.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)] // the one action-request builder
pub fn resolve_action_request(
    ctx: &Ctx,
    action_id: &str,
    team_id: &str,
    repo: ActionRepo,
    inputs: Vec<ActionInputValue>,
    options: LaunchOptions,
    origin: LaunchOrigin,
    trigger: Option<coding::TriggerNote>,
    // EXP-583: the `automations` row that fired this run — stamped on the
    // session beside `startedReason`. `None` on every person-started run.
    automation_id: Option<String>,
) -> anyhow::Result<ActionLaunchRequest> {
    let builtin = api::actions::is_builtin_action_id(action_id);
    let fixing = action_id == BUILTIN_FIX_CONFLICTS_ID;

    // EXP-259: the fix-conflicts PR target — resolved from the `pr` input
    // BEFORE anything else so a bad pick fails fast.
    let fix_target = if fixing {
        let value = inputs
            .iter()
            .find(|input| input.key == "pr")
            .map(|input| input.value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("Pick a pull request to fix (--input pr=<issue>)."))?;
        let fetched = api::issues::issues_get(&ctx.trpc, &value)
            .context("resolve the pull request's issue")?;
        let issue = fetched.issue;
        if issue.pr_state.as_deref() != Some("open") {
            bail!("That pull request is no longer open.");
        }
        let branch = issue
            .branch
            .clone()
            .filter(|branch| !branch.is_empty())
            .ok_or_else(|| anyhow!("That pull request has no recorded branch."))?;
        Some((issue.identifier.clone(), branch, issue.id.clone()))
    } else {
        None
    };

    let (action, repo_group) = if builtin {
        let mut action = if fixing {
            api::actions::builtin_fix_conflicts_action(team_id)
        } else {
            api::actions::builtin_create_action(team_id)
        };
        // The runner composes the builtin prompts itself — the input schema
        // is a dialog-side concern only (desktop parity).
        action.inputs = Vec::new();
        let repo_group = if fixing {
            match repo {
                ActionRepo::Provided(group) => group,
                ActionRepo::Resolve => {
                    let (_, _, issue_id) = fix_target
                        .as_ref()
                        .expect("fix target resolved above");
                    let repository = api::repositories::for_issue(&ctx.trpc, issue_id)
                        .context("resolve the pull request's repository")?
                        .ok_or_else(|| {
                            anyhow!("That pull request's board has no linked repository.")
                        })?;
                    Some(RepoGroup {
                        repository_id: repository.repository_id,
                        full_name: repository.full_name,
                        default_branch: repository.default_branch,
                    })
                }
            }
        } else {
            // The creator's repo INPUT only pins the authored action's
            // binding — never this run's cwd.
            None
        };
        (action, repo_group)
    } else {
        let action = api::actions::get(&ctx.trpc, action_id).context("load the action")?;
        let repo_group = match repo {
            ActionRepo::Provided(group) => group,
            ActionRepo::Resolve => match &action.repository_id {
                None => None,
                Some(repository_id) => {
                    let rows = fetch_repositories(&ctx.trpc, &action.team_id)
                        .context("resolve the repository")?;
                    rows.into_iter()
                        .find(|row| &row.id == repository_id)
                        .map(|row| RepoGroup {
                            repository_id: row.id,
                            full_name: row.full_name,
                            default_branch: row.default_branch.unwrap_or_default(),
                        })
                    // The registry row vanished — degrade to repo-less
                    // rather than failing the run (desktop parity).
                }
            },
        };
        (action, repo_group)
    };

    let kind = match fix_target {
        Some((identifier, branch, issue_id)) => {
            let default_branch = repo_group
                .as_ref()
                .map(|group| group.default_branch.clone())
                .unwrap_or_default();
            if default_branch.is_empty() {
                // L30: never fabricate `main`.
                bail!(
                    "The repository has no known default branch. Refresh it in team settings → Repositories."
                );
            }
            ActionRunKind::FixConflicts {
                branch,
                default_branch,
                identifier,
                issue_id,
            }
        }
        None if builtin => ActionRunKind::CreateAction,
        None => ActionRunKind::Team,
    };

    Ok(ActionLaunchRequest {
        action_id: action.id.clone(),
        action_name: action.name.clone(),
        team_id: action.team_id.clone(),
        body: action.body.clone(),
        repo: repo_group,
        inputs,
        kind,
        trigger,
        automation_id,
        device_label: coding::default_device_label(),
        origin,
        options,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(std::path::PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn temp_ctx(name: &str) -> (TempDir, Ctx) {
        let dir = std::env::temp_dir().join(format!("exp-cli-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let auth = api::accounts::AuthStore::load(dir.clone());
        let account = api::accounts::Account {
            id: "acct-1".to_string(),
            instance_url: "https://example.test".to_string(),
            user_id: "user-1".to_string(),
            email: "dev@example.test".to_string(),
            name: None,
            is_admin: false,
            onboarding_completed_at: None,
            getting_started_dismissed_at: None,
        };
        let trpc = Arc::new(api::trpc::TrpcClient::new(
            &account.instance_url,
            auth.token_provider(&account.id),
        ));
        let ctx = Ctx {
            data_dir: dir.clone(),
            auth,
            account,
            trpc,
            token_store: Arc::new(api::token_store::TokenStore::new(dir.clone())),
            settings: coding::Settings::default(),
        };
        (TempDir(dir), ctx)
    }

    /// EXP-550: only the daemon heartbeats the `devices` row, so only a
    /// daemon-hosted run may stamp its session with this machine's device id.
    /// A foreground `code`/`run` that stamped it would render as "Paused,
    /// device offline" everywhere the moment a past daemon's row went stale.
    #[test]
    fn only_a_daemon_hosted_launch_stamps_the_device_id() {
        let (_dir, ctx) = temp_ctx("launch-host");

        let hosted = coding_deps(&ctx, HashMap::new(), LaunchHost::Daemon);
        assert_eq!(hosted.device_id.as_deref(), Some(ctx.device_id().as_str()));

        let foreground = coding_deps(&ctx, HashMap::new(), LaunchHost::Foreground);
        assert_eq!(foreground.device_id, None);
    }
}
