//! The Actions tool window (EXP-253): the team's reusable markdown prompts —
//! list + ▶ Run (per-device body-hash TRUST GATE), owner-only Edit/Delete,
//! and the two creators: "Describe with Claude" (the ONE MCP-enabled claude
//! task — L24's direct descendant) and the raw editor.
//!
//! The trust gate is the compensating control for executing server-stored
//! prompts locally: every run RE-FETCHES the action (`actions.get`), hashes
//! THAT body ([`api::actions::body_hash`]), and compares it against what this
//! device last trusted ([`api::TrustStore`]). Any mismatch — first run, an
//! edited body, another author's change — blocks behind the trust dialog;
//! store errors read as untrusted (fail CLOSED). Remote starts
//! ([`crate::steer_wiring`]) run the same gate with the dialog foregrounded.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, AppContext as _, ClickEvent, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    dialog::DialogButtonProps,
    input::{Input, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _,
};
use serde::Deserialize;

use crate::coding_flow::{self, SessionSubject};
use crate::icons::ExpIcon;
use crate::navigation::{active_team_id, nav_for_window, Navigation};
use crate::queries;
use crate::session::AuthContext;
use api::trust_store::{device_id, TrustStore};
use coding::{ActionLaunchRequest, LaunchOptions, LaunchOrigin, Prepared, PrepareRequest};
use terminal::TabKind;

// ---------------------------------------------------------------------------
// Default templates (masterplan §15 D8: templates in the New-action flow —
// NEVER seeded rows). Seeds for the describe-with-Claude prompt and the raw
// editor's prefill; the web creator ships the same three.
// ---------------------------------------------------------------------------

pub(crate) struct ActionTemplate {
    pub name: &'static str,
    pub body: &'static str,
}

pub(crate) const ACTION_TEMPLATES: [ActionTemplate; 3] = [
    ActionTemplate {
        name: "Code review → file issues",
        body: "# Code review\n\nReview recent changes on this repository's default branch and \
file issues for real problems.\n\n1. Scan the last ~20 commits (`git log --oneline -20`) and \
inspect the substantive ones (`git show <sha>`).\n2. Look for actual defects: broken error \
handling, races, security problems, dead code, risky logic without tests. Skip style nits.\n\
3. Find the right board with `exponential_teams_list` + `exponential_boards_list`, and check \
`exponential_issues_list` first so you never file a duplicate.\n4. For each finding, file one \
issue with `exponential_issues_create`: short title; description with file/line, why it's a \
problem, and a suggested fix; sensible priority.\n\nFinish with a summary: issues filed \
(identifiers + one-liners) and what you reviewed but left alone.",
    },
    ActionTemplate {
        name: "Label + prioritize all issues",
        body: "# Label + prioritize the backlog\n\nTidy every open issue in this team.\n\n1. \
`exponential_teams_list` for the team, then `exponential_boards_list` and \
`exponential_labels_list` to learn the boards and the existing label vocabulary.\n2. Page \
through open issues with `exponential_issues_list` (skip done/cancelled/duplicate).\n3. For \
each issue: add fitting existing labels with `exponential_issue_labels_add` (only create a \
new label with `exponential_labels_create` when several issues clearly need it). Set \
priority with `exponential_issues_update`: urgent = breakage/data loss, high = user-visible \
bugs, medium = solid improvements, low = nice-to-haves. Leave issues that already look right \
alone.\n\nFinish with a summary: issues touched, labels added, priorities changed.",
    },
    ActionTemplate {
        name: "Draft changelog from recent merges",
        body: "# Draft a changelog\n\nDraft a user-facing changelog entry from what shipped \
recently.\n\n1. Collect recently completed work with `exponential_issues_list` (status done, \
roughly the last two weeks, across the team's boards).\n2. Group into a few user-visible \
themes; drop internal refactors unless users feel them.\n3. Write a short GFM entry: one \
summary line plus 3–6 bold-led bullets (\"**Faster search** — …\"), plain language, no issue \
identifiers.\n\nPost the draft as your final message — this action only drafts, it changes \
nothing.",
    },
];

// ---------------------------------------------------------------------------
// The shared trust-gated runner — local Run clicks AND relay remote starts
// ---------------------------------------------------------------------------

/// How the run resolves its repo group.
pub(crate) enum ActionRepo {
    /// Remote start: the frame's server-resolved group (`None` = repo-less).
    Provided(Option<coding::RepoGroup>),
    /// Local start: resolve `action.repository_id` via `repositories.list`
    /// on the background executor.
    Resolve,
}

/// Slim `repositories.list` row for the local repo resolution + the editor's
/// picker.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActionRepoRow {
    pub id: String,
    pub full_name: String,
    #[serde(default)]
    pub default_branch: Option<String>,
}

fn fetch_repositories(
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

/// Start `action_id` behind the trust gate (EXP-253): fetch FRESH →
/// hash → trusted? launch : dialog → trust + launch. `target` picks the
/// window for the dialog + the terminal tab (`None` = the first shell
/// window — the relay path); `activate_app` foregrounds the app first
/// (remote starts must surface the dialog, not queue it behind other apps).
pub(crate) fn start_action_run(
    action_id: String,
    repo: ActionRepo,
    options: LaunchOptions,
    origin: LaunchOrigin,
    target: Option<gpui::AnyWindowHandle>,
    activate_app: bool,
    cx: &mut App,
) {
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("actions: run ignored — not signed in");
        return;
    };
    let Some(account) = queries::active_account(cx) else {
        return;
    };
    let data_dir = AuthContext::global(cx).data_dir.clone();

    cx.spawn(async move |cx| {
        // Background: fetch-fresh, hash, trust-check, resolve the repo.
        let gate = cx
            .background_executor()
            .spawn(async move {
                let action = api::actions::get(&trpc, &action_id)
                    .map_err(|err| format!("Could not load the action: {err}"))?;
                let hash = api::actions::body_hash(&action.body);
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

            let request = ActionLaunchRequest {
                action_id: action.id.clone(),
                action_name: action.name.clone(),
                body: action.body.clone(),
                repo: repo_group,
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
            // everything it approves (never a truncated preview). Confirm
            // records the hash and launches.
            let title = SharedString::from(format!("Run \"{}\" on this device?", action.name));
            let _ = window.update(cx, |_, window, cx| {
                let body_view = cx.new(|_| TrustBodyView {
                    body: SharedString::from(action.body.clone()),
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
/// truncated away.
struct TrustBodyView {
    body: SharedString,
    scroll: ScrollHandle,
}

impl Render for TrustBodyView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .h(px(320.))
            .w_full()
            .flex()
            .flex_col()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .bg(theme.muted.opacity(0.3))
            .child(crate::scroll_pane::v_scroll_pane(
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

// ---------------------------------------------------------------------------
// ActionsPanel — the tool-window list
// ---------------------------------------------------------------------------

/// Fetch lifecycle (the settings/run-bar load-gate pattern).
enum Load {
    Idle,
    Loading,
    Ready,
}

pub struct ActionsPanel {
    nav: Entity<Navigation>,
    /// The team the loaded list belongs to (scope-change reset key).
    team_id: Option<String>,
    load: Load,
    actions: Vec<api::actions::Action>,
    error: Option<SharedString>,
    /// Bumped per fetch — a stale response checks it before landing.
    generation: u64,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl ActionsPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        Self {
            nav,
            team_id: None,
            load: Load::Idle,
            actions: Vec::new(),
            error: None,
            generation: 0,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Render-time load gate: reset on team change, fetch once while Idle.
    fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        let team = active_team_id(&self.nav, cx);
        if team != self.team_id {
            self.team_id = team;
            self.actions.clear();
            self.error = None;
            self.load = Load::Idle;
            self.generation += 1;
        }
        if !matches!(self.load, Load::Idle) {
            return;
        }
        self.refetch(cx);
    }

    /// (Re)fetch the active team's actions.
    pub(crate) fn refetch(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.team_id.clone() else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.load = Load::Loading;
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::actions::list(&trpc, &team_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.load = Load::Ready;
                match result {
                    Ok(actions) => {
                        this.actions = actions;
                        this.error = None;
                    }
                    Err(err) => {
                        this.error = Some(SharedString::from(format!(
                            "Could not load actions: {err}"
                        )));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// ▶ Run — the trust-gated local start, in THIS window.
    fn run(&mut self, action_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let settings = coding_flow::CodingHub::global(cx).read(cx).settings.clone();
        // Local runs take the desktop's own claude defaults (model/effort);
        // Claude-only v1 (the launcher clamps too).
        let options = LaunchOptions::remote(&settings, Some("claude"), None, None, None, None, None);
        let handle = window.window_handle();
        start_action_run(
            action_id,
            ActionRepo::Resolve,
            options,
            LaunchOrigin::Local,
            Some(handle),
            false,
            cx,
        );
    }

    /// Owner Delete, behind a confirm (destructive native actions confirm
    /// first — the client contract).
    fn prompt_delete(
        &mut self,
        action_id: String,
        name: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let panel = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _window, _cx| {
            let panel = panel.clone();
            let action_id = action_id.clone();
            alert
                .confirm()
                .overlay_closable(true)
                .close_button(true)
                .width(px(416.))
                .title(SharedString::from(format!("Delete \"{name}\"?")))
                .description(
                    "Team members will no longer be able to run this action. \
                     A live run keeps going and keeps its label.",
                )
                .button_props(DialogButtonProps::default().ok_text("Delete action"))
                .on_ok(move |_, _, cx| {
                    let Some(trpc) = queries::trpc_client(cx) else {
                        return true;
                    };
                    let panel = panel.clone();
                    let action_id = action_id.clone();
                    cx.spawn(async move |cx| {
                        let result = cx
                            .background_executor()
                            .spawn(async move { api::actions::delete(&trpc, &action_id) })
                            .await;
                        let _ = cx.update(|cx| {
                            if let Err(err) = result {
                                log::warn!("actions: delete failed: {err}");
                            }
                            if let Some(panel) = panel.upgrade() {
                                panel.update(cx, |panel, cx| panel.refetch(cx));
                            }
                        });
                    })
                    .detach();
                    true
                })
        });
    }

    /// "Describe with Claude" — the ONE MCP-enabled claude task (L24): a
    /// scratch `.exp-mcp.json` + [`coding::create_action_prompt`] in a
    /// `ClaudeTask` tab; the exit hook refetches the list.
    fn describe_with_claude(
        &mut self,
        description: String,
        template: Option<&'static str>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(team_id) = self.team_id.clone() else {
            return;
        };
        let Some(account) = queries::active_account(cx) else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let Some(manager) = coding_flow::window_terminal_manager(window, cx) else {
            window.push_notification(Notification::error("Terminal dock is not available."), cx);
            return;
        };
        let data_dir = AuthContext::global(cx).data_dir.clone();
        let settings = coding_flow::CodingHub::global(cx).read(cx).settings.clone();
        // The creator's scratch cwd: no repo needed — the task only calls
        // MCP tools.
        let creator_dir = data_dir.join("actions").join("_creator");

        cx.spawn_in(window, async move |this, cx| {
            let prep_dir = creator_dir.clone();
            let prep = cx
                .background_executor()
                .spawn(async move {
                    std::fs::create_dir_all(&prep_dir)
                        .map_err(|err| format!("create creator dir: {err}"))?;
                    let store = api::token_store::TokenStore::new(data_dir);
                    let key = api::users::ensure_personal_key(&trpc, &store, &account.id)
                        .map_err(|err| err.to_string())?;
                    coding::write_mcp_json(&prep_dir, trpc.base_url(), &key)
                        .map_err(|err| format!("write .exp-mcp.json: {err}"))?;
                    Ok::<(), String>(())
                })
                .await;
            let _ = this.update_in(cx, |_this, window, cx| {
                if let Err(message) = prep {
                    window.push_notification(
                        Notification::error(SharedString::from(message)),
                        cx,
                    );
                    return;
                }
                let prompt = coding::create_action_prompt(&team_id, &description, template);
                let task =
                    coding::claude_task_with_mcp(&settings, &creator_dir, &prompt, "New action");
                let panel = cx.entity().downgrade();
                let on_exit: terminal::ExitHook = Box::new(move |_id, _exit, cx| {
                    if let Some(panel) = panel.upgrade() {
                        panel.update(cx, |panel, cx| panel.refetch(cx));
                    }
                });
                let result = manager.update(cx, |manager, cx| {
                    manager.open_tab(
                        TabKind::ClaudeTask,
                        task.tab_title.clone(),
                        None,
                        &task.spawn,
                        Some(on_exit),
                        cx,
                    )
                });
                if let Err(err) = result {
                    window.push_notification(
                        Notification::error(SharedString::from(format!(
                            "Could not start Claude: {err}"
                        ))),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    // -- render -------------------------------------------------------------

    fn render_row(
        &self,
        index: usize,
        action: &api::actions::Action,
        owner: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let run_id = action.id.clone();
        let edit_action = action.clone();
        let repo_backed = action.repository_id.is_some();

        gpui_component::v_flex()
            .id(SharedString::from(format!("action-{}", action.id)))
            .w_full()
            .gap_0p5()
            .px_2()
            .py_1p5()
            .rounded(theme.radius)
            .hover(|this| this.bg(theme.accent.opacity(0.3)))
            .child(
                gpui_component::h_flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Icon::from(ExpIcon::Zap)
                            .xsmall()
                            .text_color(theme.muted_foreground),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .text_color(theme.foreground)
                            .child(SharedString::from(action.name.clone())),
                    )
                    .when(repo_backed, |this| {
                        this.child(
                            Icon::from(ExpIcon::GitMerge)
                                .xsmall()
                                .text_color(theme.muted_foreground),
                        )
                    })
                    .when(owner, |this| {
                        let panel = cx.entity().downgrade();
                        this.child(
                            Button::new(("action-menu", index))
                                .ghost()
                                .xsmall()
                                .icon(IconName::Ellipsis)
                                .dropdown_menu(move |menu, _window, _cx| {
                                    // Direct closures (the members-menu
                                    // pattern) — never App-global dispatch
                                    // from an overlay into an unfocused view.
                                    let edit = edit_action.clone();
                                    let edit_panel = panel.clone();
                                    let delete = edit_action.clone();
                                    let delete_panel = panel.clone();
                                    menu.item(
                                        PopupMenuItem::new("Edit…").on_click(
                                            move |_, window, cx| {
                                                let Some(panel) = edit_panel.upgrade() else {
                                                    return;
                                                };
                                                let Some(team_id) = panel
                                                    .read(cx)
                                                    .team_id
                                                    .clone()
                                                else {
                                                    return;
                                                };
                                                open_action_editor(
                                                    window,
                                                    cx,
                                                    team_id,
                                                    Some(edit.clone()),
                                                    edit_panel.clone(),
                                                );
                                            },
                                        ),
                                    )
                                    .separator()
                                    .item(
                                        PopupMenuItem::new("Delete…").on_click(
                                            move |_, window, cx| {
                                                let Some(panel) = delete_panel.upgrade()
                                                else {
                                                    return;
                                                };
                                                let id = delete.id.clone();
                                                let name = delete.name.clone();
                                                panel.update(cx, |panel, cx| {
                                                    panel.prompt_delete(id, name, window, cx);
                                                });
                                            },
                                        ),
                                    )
                                }),
                        )
                    })
                    .child(
                        Button::new(("action-run", index))
                            .primary()
                            .xsmall()
                            .icon(Icon::from(ExpIcon::Play))
                            .tooltip("Run on this device")
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.run(run_id.clone(), window, cx);
                            })),
                    ),
            )
            .when_some(action.description.clone(), |this, description| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(SharedString::from(description)),
                )
            })
            .into_any_element()
    }
}

impl Render for ActionsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_loaded(cx);
        // Copied out (Hsla is Copy) — the theme borrow must not overlap the
        // row-render closures' mutable cx borrow.
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let sidebar_border = cx.theme().sidebar_border;
        let owner = self
            .team_id
            .as_deref()
            .is_some_and(|team_id| crate::settings::is_owner(cx, team_id));
        let loading = matches!(self.load, Load::Loading) && self.actions.is_empty();

        let rows: Vec<gpui::AnyElement> = self
            .actions
            .clone()
            .iter()
            .enumerate()
            .map(|(index, action)| self.render_row(index, action, owner, cx))
            .collect();

        gpui_component::v_flex()
            .size_full()
            .min_h_0()
            // New-action affordances (owner-only writes; the buttons hide for
            // members — server enforces regardless).
            .when(owner, |this| {
                this.child(
                    gpui_component::h_flex()
                        .flex_shrink_0()
                        .gap_1()
                        .px_2()
                        .py_1p5()
                        .border_b_1()
                        .border_color(sidebar_border)
                        .child(
                            Button::new("action-new-claude")
                                .primary()
                                .xsmall()
                                .label("Describe with Claude…")
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    if this.team_id.is_none() {
                                        return;
                                    }
                                    let panel = cx.entity().downgrade();
                                    open_describe_dialog(window, cx, panel);
                                })),
                        )
                        .child(
                            Button::new("action-new-manual")
                                .ghost()
                                .xsmall()
                                .label("Write manually…")
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    let Some(team_id) = this.team_id.clone() else {
                                        return;
                                    };
                                    let panel = cx.entity().downgrade();
                                    open_action_editor(window, cx, team_id, None, panel);
                                })),
                        ),
                )
            })
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .text_color(danger)
                        .child(error),
                )
            })
            .child(crate::scroll_pane::v_scroll_pane(
                "actions-scroll",
                &self.scroll,
                gpui_component::v_flex()
                    .p_1()
                    .gap_0p5()
                    .children(rows)
                    .when(loading, |this| {
                        this.child(
                            div()
                                .p_2()
                                .text_xs()
                                .text_color(muted)
                                .child("Loading actions…"),
                        )
                    })
                    .when(
                        !loading && self.actions.is_empty() && self.error.is_none(),
                        |this| {
                            this.child(
                                div().p_2().text_xs().text_color(muted).child(if owner {
                                    "No actions yet — describe one and Claude writes it, \
                                     or write it manually."
                                } else {
                                    "No actions yet — a team owner can create one."
                                }),
                            )
                        },
                    ),
            ))
    }
}

// ---------------------------------------------------------------------------
// The describe-with-Claude dialog (one-line description + template seed)
// ---------------------------------------------------------------------------

struct DescribeDialogView {
    description: Entity<InputState>,
    /// Index into [`ACTION_TEMPLATES`], `None` = blank.
    template: Option<usize>,
    panel: gpui::WeakEntity<ActionsPanel>,
}

impl DescribeDialogView {
    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let description = self.description.read(cx).value().trim().to_string();
        if description.is_empty() {
            return;
        }
        let template = self.template.map(|index| ACTION_TEMPLATES[index].body);
        if let Some(panel) = self.panel.upgrade() {
            panel.update(cx, |panel, cx| {
                panel.describe_with_claude(description, template, window, cx);
            });
        }
        window.close_dialog(cx);
    }
}

impl Render for DescribeDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let template_label: SharedString = match self.template {
            Some(index) => ACTION_TEMPLATES[index].name.into(),
            None => "Blank".into(),
        };
        gpui_component::v_flex()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(
                        "One line on what the action should do — Claude inspects the team \
                         and writes it via the actions MCP tools.",
                    ),
            )
            .child(Input::new(&self.description).small())
            .child(
                gpui_component::h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Template"),
                    )
                    .child(
                        Button::new("describe-template")
                            .ghost()
                            .xsmall()
                            .label(template_label)
                            .dropdown_menu({
                                let view = cx.entity().downgrade();
                                move |mut menu, _window, _cx| {
                                    for pick in
                                        std::iter::once(None).chain((0..ACTION_TEMPLATES.len()).map(Some))
                                    {
                                        let label: SharedString = match pick {
                                            Some(index) => ACTION_TEMPLATES[index].name.into(),
                                            None => "Blank".into(),
                                        };
                                        let view = view.clone();
                                        menu = menu.item(PopupMenuItem::new(label).on_click(
                                            move |_, _, cx| {
                                                if let Some(view) = view.upgrade() {
                                                    view.update(cx, |view, cx| {
                                                        view.template = pick;
                                                        cx.notify();
                                                    });
                                                }
                                            },
                                        ));
                                    }
                                    menu
                                }
                            }),
                    ),
            )
    }
}

fn open_describe_dialog(
    window: &mut Window,
    cx: &mut App,
    panel: gpui::WeakEntity<ActionsPanel>,
) {
    let view = cx.new(|cx| DescribeDialogView {
        description: cx.new(|cx| {
            InputState::new(window, cx).placeholder("e.g. Review new PRs and file issues")
        }),
        template: None,
        panel,
    });
    window.open_dialog(cx, move |dialog, _window, _cx| {
        dialog
            .w(px(460.))
            .title("New action — describe it")
            .overlay_closable(true)
            .on_ok({
                let view = view.clone();
                move |_, window, cx| {
                    view.update(cx, |view, cx| view.submit(window, cx));
                    false
                }
            })
            .child(view.clone())
    });
}

// ---------------------------------------------------------------------------
// The raw editor dialog (create + edit)
// ---------------------------------------------------------------------------

struct ActionEditorView {
    team_id: String,
    /// `Some(id)` = editing; `None` = creating.
    editing: Option<String>,
    name: Entity<InputState>,
    description: Entity<InputState>,
    body: Entity<InputState>,
    /// The chosen repo (`None` = repo-less).
    repository: Option<ActionRepoRow>,
    /// The edited action's repo binding as loaded — an update only sends
    /// `repositoryId` when the picker actually CHANGED it (a failed or
    /// still-loading repos fetch must never silently strip the binding).
    initial_repository_id: Option<String>,
    repos: Vec<ActionRepoRow>,
    /// The picker rows landed (fetch succeeded) — the gate for trusting the
    /// picker state on save.
    repos_loaded: bool,
    submitting: bool,
    error: Option<SharedString>,
    panel: gpui::WeakEntity<ActionsPanel>,
}

impl ActionEditorView {
    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.submitting {
            return;
        }
        let name = self.name.read(cx).value().trim().to_string();
        let description = self.description.read(cx).value().trim().to_string();
        let body = self.body.read(cx).value().to_string();
        if name.is_empty() || body.trim().is_empty() {
            self.error = Some("Name and body are required.".into());
            cx.notify();
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };
        self.submitting = true;
        self.error = None;
        cx.notify();

        let team_id = self.team_id.clone();
        let editing = self.editing.clone();
        let repository_id = self.repository.as_ref().map(|repo| repo.id.clone());
        // Only a REAL picker change rides the update; an unloaded picker
        // (fetch failed / still in flight) must not clobber the binding.
        let repository_patch = if !self.repos_loaded {
            api::Patch::Omit
        } else if repository_id == self.initial_repository_id {
            api::Patch::Omit
        } else {
            api::Patch::set_or_null(repository_id.clone())
        };
        let panel = self.panel.clone();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    match editing {
                        Some(id) => {
                            let mut input = api::actions::ActionUpdate::new(id);
                            input.name = Some(name);
                            input.description = Some(description);
                            input.repository_id = repository_patch;
                            input.body = Some(body);
                            api::actions::update(&trpc, &input).map(|_| ())
                        }
                        None => api::actions::create(
                            &trpc,
                            &team_id,
                            &name,
                            (!description.is_empty()).then_some(description.as_str()),
                            repository_id.as_deref(),
                            &body,
                        )
                        .map(|_| ()),
                    }
                })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                this.submitting = false;
                match result {
                    Ok(()) => {
                        if let Some(panel) = panel.upgrade() {
                            panel.update(cx, |panel, cx| panel.refetch(cx));
                        }
                        window.close_dialog(cx);
                    }
                    Err(err) => {
                        this.error = Some(SharedString::from(format!("{err}")));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}

impl Render for ActionEditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let repo_label: SharedString = match &self.repository {
            Some(repo) => repo.full_name.clone().into(),
            None => "No repository".into(),
        };
        let repos = self.repos.clone();
        gpui_component::v_flex()
            .gap_2()
            .child(field_label("Name", cx))
            .child(Input::new(&self.name).small())
            .child(field_label("Description (optional)", cx))
            .child(Input::new(&self.description).small())
            .child(field_label("Repository", cx))
            .child(
                Button::new("action-repo")
                    .ghost()
                    .xsmall()
                    .label(repo_label)
                    .dropdown_menu({
                        let view = cx.entity().downgrade();
                        move |mut menu, _window, _cx| {
                            for pick in std::iter::once(None)
                                .chain((0..repos.len()).map(Some))
                            {
                                let label: SharedString = match pick {
                                    Some(index) => repos[index].full_name.clone().into(),
                                    None => "No repository".into(),
                                };
                                let view = view.clone();
                                menu = menu.item(PopupMenuItem::new(label).on_click(
                                    move |_, _, cx| {
                                        if let Some(view) = view.upgrade() {
                                            view.update(cx, |view, cx| {
                                                view.repository = pick.and_then(|index| {
                                                    view.repos.get(index).cloned()
                                                });
                                                cx.notify();
                                            });
                                        }
                                    },
                                ));
                            }
                            menu
                        }
                    }),
            )
            .child(field_label("Instructions (markdown)", cx))
            .child(Input::new(&self.body))
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.danger)
                        .child(error),
                )
            })
    }
}

fn field_label(text: &'static str, cx: &App) -> gpui::Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

fn open_action_editor(
    window: &mut Window,
    cx: &mut App,
    team_id: String,
    existing: Option<api::actions::Action>,
    panel: gpui::WeakEntity<ActionsPanel>,
) {
    let editing = existing.as_ref().map(|action| action.id.clone());
    let title: SharedString = if editing.is_some() {
        "Edit action".into()
    } else {
        "New action".into()
    };
    let existing_repo = existing.as_ref().and_then(|action| action.repository_id.clone());
    let view = cx.new(|cx| ActionEditorView {
        team_id: team_id.clone(),
        editing,
        name: cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("e.g. Code review");
            if let Some(action) = &existing {
                state.set_value(action.name.clone(), window, cx);
            }
            state
        }),
        description: cx.new(|cx| {
            let mut state =
                InputState::new(window, cx).placeholder("One line on what this action does");
            if let Some(description) = existing.as_ref().and_then(|action| {
                action.description.clone()
            }) {
                state.set_value(description, window, cx);
            }
            state
        }),
        body: cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .rows(12)
                .placeholder("# What to do\n\nStep-by-step markdown instructions…");
            if let Some(action) = &existing {
                state.set_value(action.body.clone(), window, cx);
            }
            state
        }),
        repository: None,
        initial_repository_id: existing_repo.clone(),
        repos: Vec::new(),
        repos_loaded: false,
        submitting: false,
        error: None,
        panel,
    });

    // Fetch the repo picker's rows off the foreground; pre-select the
    // edited action's repo once they land.
    if let Some(trpc) = queries::trpc_client(cx) {
        let view_for_fetch = view.downgrade();
        cx.spawn(async move |cx| {
            let rows = cx
                .background_executor()
                .spawn(async move { fetch_repositories(&trpc, &team_id) })
                .await;
            let _ = cx.update(|cx| {
                if let Some(view) = view_for_fetch.upgrade() {
                    view.update(cx, |view, cx| match rows {
                        Ok(rows) => {
                            view.repository = existing_repo
                                .as_deref()
                                .and_then(|id| rows.iter().find(|row| row.id == id).cloned());
                            view.repos = rows;
                            view.repos_loaded = true;
                            cx.notify();
                        }
                        Err(err) => {
                            // Leave repos_loaded=false — save then omits the
                            // binding instead of stripping it.
                            log::warn!("actions: repositories.list failed: {err}");
                            cx.notify();
                        }
                    });
                }
            });
        })
        .detach();
    }

    window.open_dialog(cx, move |dialog, _window, cx| {
        let busy = view.read(cx).submitting;
        dialog
            .w(px(560.))
            .title(title.clone())
            .overlay_closable(!busy)
            .keyboard(!busy)
            .on_ok({
                let view = view.clone();
                move |_, window, cx| {
                    view.update(cx, |view, cx| view.submit(window, cx));
                    false
                }
            })
            .child(view.clone())
    });
}
