//! The shared trust-gated ACTION runner (EXP-253/EXP-257) — local dialog
//! launches AND relay remote starts both land here.
//!
//! The trust gate is the compensating control for executing server-stored
//! prompts locally: every run RE-FETCHES the action (`actions.get`), hashes
//! the FRESH body + inputs schema ([`api::actions::trust_hash`]), and
//! compares it against what this device last trusted ([`api::TrustStore`]).
//! Any mismatch — first run, an edited body, a changed inputs schema —
//! blocks behind the trust dialog; store errors read as untrusted (fail
//! CLOSED). The exceptions are the server-defined BUILTINS — "Create action"
//! (EXP-257) and "Fix merge conflicts" (EXP-259): their content is
//! server-shipped, never owner-authored, so they short-circuit BEFORE the
//! fetch (the server rejects `actions.get` for the builtin ids) and skip the
//! trust gate entirely.
//!
//! The fix-conflicts builtin additionally resolves its `pr` INPUT (the
//! representative issue id of an open PR) against the synced store up front:
//! the issue's branch + identifier feed [`coding::ActionRunKind::FixConflicts`]
//! and its board's repository pins the run's repo group (remote frames carry
//! the server-resolved group instead).

use gpui::{
    div, px, App, AppContext as _, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, Styled, Window,
};
use gpui_component::{
    dialog::DialogButtonProps, notification::Notification, ActiveTheme as _, WindowExt as _,
};
use serde::Deserialize;

use sync::Store;

use crate::coding_flow::{self, SessionSubject};
use crate::queries;
use crate::session::AuthContext;
use api::actions::{is_builtin_action_id, BUILTIN_FIX_CONFLICTS_ID};
use api::trust_store::{device_id, TrustStore};
use coding::{
    ActionInputValue, ActionLaunchRequest, ActionRunKind, LaunchOptions, LaunchOrigin, Prepared,
    PrepareRequest,
};

/// The fix-conflicts builtin's display-name snapshot (mirrors the server's
/// `BUILTIN_FIX_CONFLICTS_NAME` — the session row carries the server copy).
pub(crate) const FIX_CONFLICTS_ACTION_NAME: &str = "Fix merge conflicts";

/// The resolved `pr` input target of a fix-conflicts run (EXP-259), read
/// from the synced store before the gate spawns.
struct FixConflictsTarget {
    identifier: String,
    branch: String,
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
    /// The window for the dialog + the terminal tab (`None` = the first
    /// shell window — the relay path).
    pub target: Option<gpui::AnyWindowHandle>,
    /// Foreground the app first (remote starts must surface the trust
    /// dialog, not queue it behind other apps).
    pub activate_app: bool,
}

/// Start an action behind the trust gate (EXP-253): fetch FRESH → hash →
/// trusted? launch : dialog → trust + launch. The BUILTIN short-circuits
/// before the fetch and skips the gate (see the module docs).
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
    let Some(account) = queries::active_account(cx) else {
        return;
    };
    let data_dir = AuthContext::global(cx).data_dir.clone();
    let builtin = is_builtin_action_id(&action_id);
    // EXP-259: the fix-conflicts builtin's PR target comes from the synced
    // store — resolved up front (the gate closure has no gpui access).
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
    // The kind fields the post-gate request needs (the gate closure owns
    // `fix_target` for its repo resolution).
    let fix_kind = fix_target
        .as_ref()
        .map(|fix| (fix.branch.clone(), fix.identifier.clone()));

    cx.spawn(async move |cx| {
        // Background: fetch-fresh, hash, trust-check, resolve the repo. The
        // builtins construct their rows locally (the server REJECTS
        // `actions.get` for the builtin ids) and skip the TrustStore
        // entirely. The creator forces repo-less — its repo INPUT only pins
        // the authored action's binding, never this run's cwd; the
        // fix-conflicts run instead resolves the PR's repository.
        let gate = cx
            .background_executor()
            .spawn(async move {
                if builtin {
                    let fixing = action_id == BUILTIN_FIX_CONFLICTS_ID;
                    let action = api::actions::Action {
                        id: action_id,
                        team_id,
                        repository_id: None,
                        name: if fixing {
                            FIX_CONFLICTS_ACTION_NAME.to_string()
                        } else {
                            "Create action".to_string()
                        },
                        description: None,
                        body: String::new(),
                        builtin: true,
                        inputs: Vec::new(),
                        sort_order: 0.0,
                        created_at: None,
                        updated_at: None,
                    };
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
                    return Ok((action, String::new(), true, repo_group, data_dir, account.id));
                }
                let action = api::actions::get(&trpc, &action_id)
                    .map_err(|err| format!("Could not load the action: {err}"))?;
                let hash = api::actions::trust_hash(&action);
                let device = device_id(&data_dir);
                // Fail CLOSED: any store error reads as untrusted.
                let trusted = TrustStore::open(&TrustStore::default_path(&data_dir, &account.id))
                    .and_then(|store| store.is_trusted(&device, &action.id, &hash))
                    .unwrap_or(false);
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
                Ok::<_, String>((action, hash, trusted, repo_group, data_dir, account.id))
            })
            .await;

        let _ = cx.update(|cx| {
            let (action, hash, trusted, repo_group, data_dir, account_id) = match gate {
                Ok(gate) => gate,
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
                // A remote start must SURFACE the trust dialog — an
                // unattended desktop can't approve what it can't see.
                cx.activate(true);
            }

            let kind = match fix_kind {
                Some((branch, identifier)) => {
                    let default_branch = repo_group
                        .as_ref()
                        .map(|group| group.default_branch.clone())
                        .unwrap_or_default();
                    if default_branch.is_empty() {
                        // L30: never fabricate `main` — without a known
                        // default branch there is no rebase target.
                        let message =
                            "The repository has no known default branch — refresh it in \
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
                    }
                }
                None if builtin => ActionRunKind::CreateAction,
                None => ActionRunKind::Team,
            };
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

            if trusted {
                launch_action(request, window, cx);
                return;
            }

            // The trust dialog: the FULL instructions this device would
            // execute, scrollable — the compensating control must show
            // everything it approves (never a truncated preview) — plus the
            // inputs SCHEMA (EXP-257: the labels are owner-authored text
            // the prompt will carry). Confirm records the hash and launches.
            let title = SharedString::from(format!("Run \"{}\" on this device?", action.name));
            let inputs_schema: Vec<SharedString> = action
                .inputs
                .iter()
                .map(|input| {
                    SharedString::from(format!(
                        "{} — {}{}",
                        input.label,
                        input.input_type,
                        if input.required { "" } else { " — optional" }
                    ))
                })
                .collect();
            let _ = window.update(cx, |_, window, cx| {
                let body_view = cx.new(|_| TrustBodyView {
                    body: SharedString::from(action.body.clone()),
                    inputs_schema,
                    scroll: ScrollHandle::new(),
                });
                window.open_dialog(cx, move |dialog, _window, _cx| {
                    let request = request.clone();
                    let data_dir = data_dir.clone();
                    let account_id = account_id.clone();
                    let hash = hash.clone();
                    let action_id = request.action_id.clone();
                    dialog
                        .w(px(640.))
                        .title(title.clone())
                        .overlay_closable(true)
                        .button_props(
                            DialogButtonProps::default().ok_text("Trust & run on this device"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .child(
                                    "These instructions are new to this device (or changed \
since you last trusted them). They will run as YOU, with your local tools and sign-ins. \
Review them fully:",
                                ),
                        )
                        .child(body_view.clone())
                        .on_ok(move |_, window, cx| {
                            // Best-effort record: the human just approved
                            // THIS body — a failed write only re-asks later.
                            let device = device_id(&data_dir);
                            if let Err(err) =
                                TrustStore::open(&TrustStore::default_path(&data_dir, &account_id))
                                    .and_then(|store| store.trust(&device, &action_id, &hash))
                            {
                                log::warn!("actions: trust record failed: {err}");
                            }
                            let handle = window.window_handle();
                            launch_action(request.clone(), handle, cx);
                            true
                        })
                });
            });
        });
    })
    .detach();
}

/// The trust dialog's scrollable FULL-body pane (monospace, fixed height) —
/// what the human approves is exactly what will execute, so nothing may be
/// truncated away. EXP-257: the inputs SCHEMA renders above the body (the
/// labels are owner-authored text the prompt will carry).
struct TrustBodyView {
    body: SharedString,
    /// One `label — type[ — optional]` line per declared input.
    inputs_schema: Vec<SharedString>,
    scroll: ScrollHandle,
}

impl Render for TrustBodyView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let mut pane = div()
            .h(px(320.))
            .w_full()
            .flex()
            .flex_col()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .bg(theme.muted.opacity(0.3));
        if !self.inputs_schema.is_empty() {
            let mut schema = gpui_component::v_flex()
                .flex_shrink_0()
                .px_2()
                .py_1p5()
                .gap_0p5()
                .border_b_1()
                .border_color(theme.border)
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Asks for these inputs at run time:");
            for line in &self.inputs_schema {
                schema = schema.child(
                    div()
                        .text_color(theme.foreground)
                        .child(line.clone()),
                );
            }
            pane = pane.child(schema);
        }
        pane.child(crate::scroll_pane::v_scroll_pane(
            "trust-body",
            &self.scroll,
            div()
                .p_2()
                .text_xs()
                .font_family("monospace")
                .whitespace_normal()
                .text_color(theme.foreground)
                .child(self.body.clone()),
        ))
    }
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

/// The launch tail (post-gate): background `prepare(Action)` → foreground
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
