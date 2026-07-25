//! The shared ACTION runner (EXP-253/EXP-257) — local dialog launches AND
//! relay remote starts both land here.
//!
//! Every run RE-FETCHES the action (`actions.get`) for its body — the synced
//! `actions` shape rows are body-less (EXP-268) — then resolves the repo and
//! launches. The per-device sha256 trust prompt that used to gate runs was
//! removed in EXP-268: actions are team-owner-authored content and run
//! without a local approval step. The BUILTIN "Create action" (EXP-257)
//! constructs its row locally BEFORE the fetch (the server rejects
//! `actions.get` for the builtin id).

use gpui::{App, SharedString};
use gpui_component::{notification::Notification, WindowExt as _};
use serde::Deserialize;

use crate::coding_flow::{self, SessionSubject};
use crate::queries;
use api::actions::BUILTIN_CREATE_ACTION_ID;
use coding::{ActionInputValue, ActionLaunchRequest, LaunchOptions, LaunchOrigin, Prepared, PrepareRequest};

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
    let builtin = action_id == BUILTIN_CREATE_ACTION_ID;

    cx.spawn(async move |cx| {
        // Background: fetch-fresh + resolve the repo. The builtin constructs
        // its row locally (the server REJECTS `actions.get` for the builtin
        // id) and forces repo-less — its repo INPUT only pins the authored
        // action's binding, never this run's cwd.
        let fetched = cx
            .background_executor()
            .spawn(async move {
                if builtin {
                    let mut action = api::actions::builtin_create_action(&team_id);
                    // The runner composes the creator prompt itself — the
                    // input schema is dialog-side concern only.
                    action.inputs = Vec::new();
                    return Ok((action, None));
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

            let request = ActionLaunchRequest {
                action_id: action.id.clone(),
                action_name: action.name.clone(),
                team_id: action.team_id.clone(),
                body: action.body.clone(),
                repo: repo_group,
                inputs,
                builtin,
                device_label: coding::default_device_label(),
                origin,
                options,
            };
            launch_action(request, window, cx);
        });
    })
    .detach();
}

/// Surface a runner failure on the target window (best-effort).
fn notify_target_error(target: Option<gpui::AnyWindowHandle>, message: &str, cx: &mut App) {
    if let Some(window) = target {
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
    cx.spawn(async move |cx| {
        let prepared = cx
            .background_executor()
            .spawn(async move { coding::prepare(&PrepareRequest::Action(request), &deps) })
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
