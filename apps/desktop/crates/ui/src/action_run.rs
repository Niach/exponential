//! The shared ACTION runner (EXP-253/EXP-257) — local dialog launches AND
//! relay remote starts both land here.
//!
//! Every run RE-FETCHES the action (`actions.get`) for its body — the synced
//! `actions` shape rows are body-less (EXP-268) — then resolves the repo and
//! launches. The per-device sha256 trust prompt that used to gate runs was
//! removed in EXP-268: actions are team-owner-authored content and run
//! without a local approval step. The BUILTINS — "Create action" (EXP-257)
//! and "Fix merge conflicts" (EXP-259) — construct their rows locally BEFORE
//! the fetch (the server rejects `actions.get` for the builtin ids).
//!
//! The fix-conflicts builtin additionally resolves its `pr` INPUT (the
//! representative issue id of an open PR) against the synced store up front:
//! the issue's branch + identifier feed [`coding::ActionRunKind::FixConflicts`]
//! and its board's repository pins the run's repo group (remote frames carry
//! the server-resolved group instead).

use gpui::{App, SharedString};
use gpui_component::{notification::Notification, WindowExt as _};
use serde::Deserialize;

use sync::Store;

use crate::coding_flow::{self, SessionSubject};
use crate::queries;
use api::actions::{is_builtin_action_id, BUILTIN_FIX_CONFLICTS_ID};
use coding::{
    ActionInputValue, ActionLaunchRequest, ActionRunKind, LaunchOptions, LaunchOrigin, Prepared,
    PrepareRequest,
};

/// The resolved `pr` input target of a fix-conflicts run (EXP-259), read
/// from the synced store before the gate spawns.
struct FixConflictsTarget {
    identifier: String,
    branch: String,
    /// The representative issue's UUID — the launcher resolves the PR's LIVE
    /// rebase target from it via `issues.prepareConflictFix` (EXP-324).
    issue_id: String,
    /// The PR issue's board repository — pins the run's repo group on LOCAL
    /// starts (remote frames carry the server-resolved group).
    repository_id: Option<String>,
}

/// Resolve the fix-conflicts `pr` input (a representative issue id) against
/// the synced issues/boards. Errors are user-facing.
fn resolve_fix_conflicts_target(
    inputs: &[ActionInputValue],
    cx: &App,
) -> Result<FixConflictsTarget, String> {
    let issue_id = inputs
        .iter()
        .find(|input| input.key == "pr")
        .map(|input| input.value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Pick a pull request to fix.".to_string())?;
    let collections = Store::global(cx).collections().clone();
    let issues = collections.issues.read(cx);
    let issue = issues
        .get(&issue_id)
        .ok_or_else(|| "That pull request's issue is not synced on this device.".to_string())?;
    if issue.pr_state.as_deref() != Some(domain::contract::PR_STATE_OPEN) {
        return Err("That pull request is no longer open.".to_string());
    }
    let branch = issue
        .branch
        .clone()
        .filter(|branch| !branch.is_empty())
        .ok_or_else(|| "That pull request has no recorded branch.".to_string())?;
    let identifier = issue.identifier.clone();
    let board_id = issue.board_id.clone();
    let repository_id = collections
        .boards
        .read(cx)
        .get(&board_id)
        .and_then(|board| board.repository_id.clone());
    Ok(FixConflictsTarget {
        identifier,
        branch,
        issue_id,
        repository_id,
    })
}

/// How the run resolves its repo group.
pub(crate) enum ActionRepo {
    /// Remote start: the frame's server-resolved group (`None` = repo-less).
    Provided(Option<coding::RepoGroup>),
    /// Local start: resolve `action.repository_id` via `repositories.list`
    /// on the background executor.
    Resolve,
}

/// Slim `repositories.list` row for the local repo resolution + the repo
/// pickers (action editor, launch-dialog repo inputs).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActionRepoRow {
    pub id: String,
    pub full_name: String,
    #[serde(default)]
    pub default_branch: Option<String>,
}

pub(crate) fn fetch_repositories(
    trpc: &api::TrpcClient,
    team_id: &str,
) -> Result<Vec<ActionRepoRow>, api::ApiError> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.query_with_input("repositories.list", &Input { team_id })
}

/// One [`start_action_run`] request (EXP-257 — grew past positional args).
pub(crate) struct StartActionArgs {
    pub action_id: String,
    /// The action's team — the builtin has no fetchable row to resolve it
    /// from, so every caller supplies it.
    pub team_id: String,
    pub repo: ActionRepo,
    pub options: LaunchOptions,
    pub origin: LaunchOrigin,
    /// Resolved input values, definition-ordered (empty = input-less run).
    pub inputs: Vec<ActionInputValue>,
    /// The window for the terminal tab (`None` = the first shell window —
    /// the relay path).
    pub target: Option<gpui::AnyWindowHandle>,
    /// Foreground the app first (remote starts surface the new tab).
    pub activate_app: bool,
}

/// Start an action: fetch FRESH body (`actions.get`) → resolve repo →
/// launch. The BUILTIN short-circuits before the fetch (see module docs).
pub(crate) fn start_action_run(args: StartActionArgs, cx: &mut App) {
    let StartActionArgs {
        action_id,
        team_id,
        repo,
        options,
        origin,
        inputs,
        target,
        activate_app,
    } = args;
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("actions: run ignored — not signed in");
        return;
    };
    let builtin = is_builtin_action_id(&action_id);
    // EXP-259: the fix-conflicts builtin's PR target comes from the synced
    // store — resolved up front (the background closure has no gpui access).
    let fix_target = if action_id == BUILTIN_FIX_CONFLICTS_ID {
        match resolve_fix_conflicts_target(&inputs, cx) {
            Ok(resolved) => Some(resolved),
            Err(message) => {
                log::warn!("actions: fix-conflicts start refused — {message}");
                notify_target_error(target, &message, cx);
                return;
            }
        }
    } else {
        None
    };
    // The kind fields the post-fetch request needs (the background closure
    // owns `fix_target` for its repo resolution).
    let fix_kind = fix_target
        .as_ref()
        .map(|fix| (fix.branch.clone(), fix.identifier.clone(), fix.issue_id.clone()));

    cx.spawn(async move |cx| {
        // Background: fetch-fresh + resolve the repo. The builtins construct
        // their rows locally (the server REJECTS `actions.get` for the
        // builtin ids). The creator forces repo-less — its repo INPUT only
        // pins the authored action's binding, never this run's cwd; the
        // fix-conflicts run instead resolves the PR's repository.
        let fetched = cx
            .background_executor()
            .spawn(async move {
                if builtin {
                    let fixing = action_id == BUILTIN_FIX_CONFLICTS_ID;
                    let mut action = if fixing {
                        api::actions::builtin_fix_conflicts_action(&team_id)
                    } else {
                        api::actions::builtin_create_action(&team_id)
                    };
                    // The runner composes the builtin prompts itself — the
                    // input schema is a dialog-side concern only.
                    action.inputs = Vec::new();
                    let repo_group = if fixing {
                        match repo {
                            // Remote start: the frame's server-resolved group.
                            ActionRepo::Provided(group) => group,
                            // Local start: the PR issue's board repository.
                            ActionRepo::Resolve => {
                                let Some(repository_id) = fix_target
                                    .as_ref()
                                    .and_then(|fix| fix.repository_id.clone())
                                else {
                                    return Err(
                                        "That pull request's board has no linked repository."
                                            .to_string(),
                                    );
                                };
                                let rows = fetch_repositories(&trpc, &action.team_id).map_err(
                                    |err| format!("Could not resolve the repository: {err}"),
                                )?;
                                let Some(row) =
                                    rows.into_iter().find(|row| row.id == repository_id)
                                else {
                                    return Err(
                                        "That pull request's repository is no longer connected."
                                            .to_string(),
                                    );
                                };
                                Some(coding::RepoGroup {
                                    repository_id: row.id,
                                    full_name: row.full_name,
                                    default_branch: row.default_branch.unwrap_or_default(),
                                })
                            }
                        }
                    } else {
                        None
                    };
                    return Ok((action, repo_group));
                }
                let action = api::actions::get(&trpc, &action_id)
                    .map_err(|err| format!("Could not load the action: {err}"))?;
                let repo_group = match repo {
                    ActionRepo::Provided(group) => group,
                    ActionRepo::Resolve => match &action.repository_id {
                        None => None,
                        Some(repository_id) => {
                            let rows = fetch_repositories(&trpc, &action.team_id)
                                .map_err(|err| format!("Could not resolve the repository: {err}"))?;
                            match rows.into_iter().find(|row| &row.id == repository_id) {
                                Some(row) => Some(coding::RepoGroup {
                                    repository_id: row.id,
                                    full_name: row.full_name,
                                    default_branch: row.default_branch.unwrap_or_default(),
                                }),
                                // The registry row vanished (disconnected
                                // meanwhile) — degrade to repo-less rather
                                // than failing the run.
                                None => None,
                            }
                        }
                    },
                };
                Ok::<_, String>((action, repo_group))
            })
            .await;

        let _ = cx.update(|cx| {
            let (action, repo_group) = match fetched {
                Ok(fetched) => fetched,
                Err(message) => {
                    log::warn!("actions: {message}");
                    notify_target_error(target, &message, cx);
                    return;
                }
            };
            let Some(window) = target.or_else(|| crate::steer_wiring::find_team_window(cx))
            else {
                log::warn!("actions: run for {} — no shell window open", action.name);
                return;
            };
            if activate_app {
                // A remote start surfaces the freshly spawned tab.
                cx.activate(true);
            }

            let fix_branch = fix_kind.as_ref().map(|(branch, ..)| branch.clone());
            let kind = match fix_kind {
                Some((branch, identifier, issue_id)) => {
                    let default_branch = repo_group
                        .as_ref()
                        .map(|group| group.default_branch.clone())
                        .unwrap_or_default();
                    if default_branch.is_empty() {
                        // L30: never fabricate `main` — without a known
                        // default branch there is no rebase target.
                        let message =
                            "The repository has no known default branch. Refresh it in \
team settings → Repositories.";
                        log::warn!("actions: fix-conflicts start refused — {message}");
                        let _ = window.update(cx, |_, window, cx| {
                            window.push_notification(
                                Notification::error(SharedString::from(message)),
                                cx,
                            );
                        });
                        return;
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
            // LAST gate before the launch: take the PR branch over from the
            // sessions still holding its worktree. Deliberately after the
            // fetch/repo/default-branch validation above — this KILLS live
            // sessions, and a run that then fails validation would have torn
            // them down for nothing.
            if let Some(branch) = fix_branch {
                if !take_over_branch(&branch, window, cx) {
                    return;
                }
            }
            let request = ActionLaunchRequest {
                action_id: action.id.clone(),
                action_name: action.name.clone(),
                team_id: action.team_id.clone(),
                body: action.body.clone(),
                repo: repo_group,
                inputs,
                kind,
                device_label: coding::default_device_label(),
                origin,
                options,
            };
            launch_action(request, window, cx);
        });
    })
    .detach();
}

/// Take a fix-conflicts run's branch over from the local sessions still
/// holding its worktree. Sessions outlive their PR (EXP-358 keeps them alive
/// through in_review/merged, so the session that opened the PR commonly
/// survives a failed merge) and the fix run supersedes them: close their tabs
/// now — `close_tab` kills and joins the PTY, and the `TabClosed` watcher
/// fires the idempotent `codingSessions.end` — so the rebase never runs under
/// a live PTY's cwd.
///
/// Returns `false` when the launch must be REFUSED: another fix run is
/// already working the branch, and two agents rebasing + force-pushing one
/// worktree is the failure this guard exists for. ALL holders are considered
/// (and closed) — a branch is routinely co-held by an issue session and an
/// action session, and inspecting only one of them both missed the live fix
/// run and left the other holder's PTY sitting on the tree.
fn take_over_branch(branch: &str, window: gpui::AnyWindowHandle, cx: &mut App) -> bool {
    let Some(sessions) = coding_flow::LocalSessions::global_ref(cx) else {
        return true;
    };
    let claims: Vec<_> = sessions
        .read(cx)
        .sessions_on_branch(branch)
        .map(|session| coding_flow::BranchClaim {
            is_fix_run: coding_flow::is_fix_conflicts_run(session.action_id.as_deref()),
            handle: (session.manager.clone(), session.tab),
        })
        .collect();
    match coding_flow::plan_branch_takeover(claims) {
        coding_flow::BranchTakeover::Refuse => {
            let message = "A fix-conflicts run is already working this pull request.";
            log::warn!("actions: fix-conflicts start refused — {message}");
            let _ = window.update(cx, |_, window, cx| {
                window.push_notification(Notification::error(SharedString::from(message)), cx);
            });
            false
        }
        coding_flow::BranchTakeover::Close(holders) => {
            for (manager, tab) in holders {
                if let Some(manager) = manager.upgrade() {
                    manager.update(cx, |manager, cx| manager.close_tab(tab, cx));
                }
            }
            true
        }
    }
}

/// Surface a runner failure on the target window (best-effort). A RELAY start
/// carries no target (EXP-357) — it used to swallow the refusal into the log,
/// so a remote "Fix merge conflicts" that resolved to nothing looked, from the
/// phone that sent it, exactly like a start that was never picked up. Fall
/// back to the shell window every other refusal in this file already uses.
fn notify_target_error(target: Option<gpui::AnyWindowHandle>, message: &str, cx: &mut App) {
    if let Some(window) = target.or_else(|| crate::steer_wiring::find_team_window(cx)) {
        let message = SharedString::from(message.to_string());
        let _ = window.update(cx, |_, window, cx| {
            window.push_notification(Notification::error(message), cx);
        });
    }
}

/// The launch tail: background `prepare(Action)` → foreground
/// `spawn_into_window` — the exact remote-issue-start shape.
fn launch_action(request: ActionLaunchRequest, target: gpui::AnyWindowHandle, cx: &mut App) {
    let Some(deps) = coding_flow::build_action_deps(cx) else {
        log::warn!("actions: launch ignored — not signed in");
        return;
    };
    let hooks = crate::steer_wiring::hook_setup(cx);
    let observer = crate::steer_wiring::observer_setup(cx);
    cx.spawn(async move |cx| {
        let prepared = cx
            .background_executor()
            .spawn(async move {
                coding::prepare_with_hooks(
                    &PrepareRequest::Action(request),
                    &deps,
                    hooks.as_ref(),
                    observer.as_ref(),
                )
            })
            .await;
        let _ = target.update(cx, |_, window, cx| match prepared {
            Ok(Prepared::Ready(prepared)) => {
                // Subject = the SESSION row id (concurrent runs of one
                // action must not share a registry key).
                let subject = SessionSubject::Action(prepared.session_id.clone());
                if let Err(message) = coding_flow::spawn_into_window(
                    prepared,
                    subject,
                    window,
                    cx,
                ) {
                    log::warn!("actions: spawn failed: {message}");
                    window.push_notification(
                        Notification::error(SharedString::from(message)),
                        cx,
                    );
                }
            }
            Ok(Prepared::Disabled(reason)) => {
                log::warn!("actions: run disabled — {}", reason.message());
                window.push_notification(
                    Notification::error(SharedString::from(reason.message())),
                    cx,
                );
            }
            Err(err) => {
                log::warn!("actions: prepare failed: {err}");
                window.push_notification(
                    Notification::error(SharedString::from(format!(
                        "Could not start the action: {err}"
                    ))),
                    cx,
                );
            }
        });
    })
    .detach();
}
