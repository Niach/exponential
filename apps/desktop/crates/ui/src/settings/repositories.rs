//! Settings → Repositories (masterplan-v3 §4.2 + §7.9).
//!
//! Web parity: `components/team/repositories-section.tsx`. This pane
//! shows the **live server truth** for GitHub connect state — never a local
//! guess (the old native app falsely said "not connected"):
//!
//! - the GitHub-App install banner off `integrations.github.status`
//!   (`{configured, installed, installUrl, accounts[]}`) — "Installed as
//!   @acme" vs. the install nudge;
//! - the team's connected repos + their board links off
//!   `repositories.list` (server-only tables — read via tRPC, never synced).
//!
//! The GitHub-App **install** is a browser hand-off: the buttons open the
//! install/manage URL in the system browser through the robust opener chain
//! (the App's install OAuth flow can't run in-process). EXP-329 gave the pane
//! the two mutations the web card has had all along — "Add repository"
//! (the [`super::add_repository_dialog`] picker over `repositories.add`) and
//! the per-row remove — so connecting a repo no longer requires creating a
//! board to hang it off. The shared status/repo fetches live in
//! [`crate::github_connect`].
//!
//! A browser hand-off finishes OUTSIDE the app, so window activation is the
//! refetch trigger: coming back re-reads the connect state without flashing
//! the skeleton over a list that is already up.
//!
//! EXP-557 per-user sharing: the pane is MEMBER-visible — the status line and
//! the add dialog show the VIEWER's own GitHub connections/repos (the server
//! scopes them), connecting a repo shares it with the team, and per-row
//! management (remove, branch pin) is sharer-or-owner. STALE linked accounts
//! (zero grants from anyone — no reconnect can ever refresh them) get a
//! Disconnect affordance via `integrations.github.unlink`.

use std::collections::HashMap;

use gpui::{
    div, px, AppContext as _, Entity, Focusable as _, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    notification::Notification,
    popover::Popover,
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};
use serde::{Deserialize, Serialize};
use sync::Store;

use crate::controls::WebControl as _;
use crate::github_connect::{fetch_github_status, GithubStatus};
use crate::native_dialog::{open_alert, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;
use crate::repo_resolver::links_snapshot;

use super::{card_title, error_notice, open_url, section};
use crate::icons::registry;

// ---------------------------------------------------------------------------
// Server-only reads (typed mirrors of the web loader results)
// ---------------------------------------------------------------------------

/// `repositories.list` — one connected repo + the boards it backs (v4
/// `boards.repositoryId`; board names now resolved server-side).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RepoRow {
    /// Consumed by the Boards pane's repository picker
    /// (`boards.setRepository`) and this pane's per-row remove.
    pub id: String,
    pub full_name: String,
    /// The EFFECTIVE default branch (EXP-462): the team's pinned branch when
    /// set, GitHub's default otherwise — the value every consumer acts on.
    #[serde(default)]
    pub default_branch: String,
    /// GitHub's own default branch, for the picker's "(default)" entry —
    /// picking it clears the pin server-side. Absent on older servers.
    #[serde(default)]
    pub github_default_branch: Option<String>,
    #[serde(default)]
    pub private: bool,
    /// The member who shared (connected) this repo with the team (EXP-557).
    /// Drives the "Shared by" line and the sharer-or-owner row gating.
    /// `None` on pre-sharing rows and older servers — owner-managed only.
    #[serde(default)]
    pub shared_by: Option<RepoSharedBy>,
    #[serde(default)]
    pub boards: Vec<RepoBoardRef>,
}

/// `repositories.list`'s `sharedBy` — the sharer's display identity.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RepoSharedBy {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

impl RepoSharedBy {
    /// Name, else email (web parity: `sharedBy.name || sharedBy.email`).
    fn label(&self) -> Option<String> {
        self.name
            .clone()
            .filter(|name| !name.is_empty())
            .or_else(|| self.email.clone().filter(|email| !email.is_empty()))
    }
}

/// A board this repo backs (`{ id, name, slug }` from `repositories.list`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RepoBoardRef {
    /// EXP-698: kept off the struct until the board chips became stateful
    /// pills, which need a STABLE element id — two boards may share a name.
    pub id: String,
    pub name: String,
}

/// Shared with the Boards pane's repository picker (same server read).
pub(super) fn fetch_repositories(
    trpc: &api::TrpcClient,
    team_id: &str,
) -> Result<Vec<RepoRow>, api::ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.query_with_input("repositories.list", &Input { team_id })
}

// ---------------------------------------------------------------------------
// Pane
// ---------------------------------------------------------------------------

struct Loaded {
    /// `None` = the status probe failed (banner is best-effort, web parity).
    status: Option<GithubStatus>,
    repos: Result<Vec<RepoRow>, String>,
}

enum Load {
    Idle,
    Loading,
    Ready(Loaded),
}

/// One repo's lazily-fetched branch list for the default-branch picker
/// (EXP-462). Fetched on first menu open, cached until the pane reloads.
#[derive(Clone)]
enum BranchLoad {
    Loading,
    Failed(String),
    Ready(Vec<String>),
}

pub struct RepositoriesPane {
    nav: Entity<Navigation>,
    load: Load,
    /// The team the current `load` belongs to; a switch re-fetches.
    loaded_team: Option<String>,
    /// The account it was fetched as — a re-login must re-fetch (the GitHub
    /// install state is per-user).
    account_id: Option<String>,
    /// The synced (board → repository) links the current `load` was fetched
    /// under (EXP-139) — a link change on any client re-fetches so the
    /// "used by" chips stay live.
    loaded_links: Option<Vec<(String, String)>>,
    /// Monotonic guard: a stale in-flight fetch must not clobber a newer one.
    generation: u64,
    /// A remove or branch-pin mutation is in flight — disables the rows'
    /// buttons.
    busy: bool,
    /// Branch lists for the per-row default-branch pickers, keyed by repo id
    /// (EXP-462). Dropped with the repo list so a reload refetches.
    branches: HashMap<String, BranchLoad>,
    /// Search query of the default-branch pickers (EXP-469 — the searchable
    /// popover recipe of `crate::pickers`). ONE shared state — only one
    /// popover is ever open, and every open resets it.
    branch_query: Entity<InputState>,
    /// Failure copy from the `exponential://github-connected?error=…` deep
    /// link (EXP-368) — the browser connect hand-off ended in an error.
    connect_error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl RepositoriesPane {
    pub fn new(
        nav: Entity<Navigation>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        // The GitHub-App install state + repo list (incl. board names) come
        // straight from the server; navigation (team switch) and — since
        // the per-repo board chips mirror `boards.repository_id` — a
        // synced repo-link change drive the re-render/re-fetch (EXP-139).
        let boards = Store::global(cx).collections().boards.clone();
        let branch_query =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search branches..."));
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // Typing in a branch picker re-filters its rows.
            cx.observe(&branch_query, |_, _, cx| cx.notify()),
            cx.observe(&boards, |this: &mut Self, _, cx| {
                this.refresh_if_links_changed(cx);
            }),
            // The GitHub connect/install hand-off completes in the browser —
            // regaining focus refetches as a fallback (EXP-329); since
            // EXP-368 the definitive signal is the github-connected deep
            // link below.
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.refetch_stale(cx);
                }
            }),
            // EXP-368: the `exponential://github-connected` hand-back —
            // refetch on success, surface the failure copy otherwise.
            cx.observe_global::<crate::github_connect::GithubConnectSignal>(|this, cx| {
                let Some(outcome) = cx
                    .try_global::<crate::github_connect::GithubConnectSignal>()
                    .and_then(|signal| signal.outcome.clone())
                else {
                    return;
                };
                match outcome {
                    crate::github_connect::GithubConnectOutcome::Connected => {
                        this.connect_error = None;
                        if matches!(this.load, Load::Ready(_)) {
                            this.refetch_stale(cx);
                        } else {
                            // Render-time ensure_loaded picks the fetch up
                            // when the pane is (next) visible.
                            this.load = Load::Idle;
                            cx.notify();
                        }
                    }
                    crate::github_connect::GithubConnectOutcome::Failed(code) => {
                        this.connect_error =
                            Some(crate::github_connect::connect_error_message(&code).into());
                        cx.notify();
                    }
                }
            }),
        ];
        Self {
            nav,
            load: Load::Idle,
            loaded_team: None,
            account_id: None,
            loaded_links: None,
            generation: 0,
            busy: false,
            branches: HashMap::new(),
            branch_query,
            connect_error: None,
            _subscriptions: subscriptions,
        }
    }

    /// Drop the cached list when a board's repo link changed under it — the
    /// next render re-fetches (a hidden pane stays idle until reopened).
    fn refresh_if_links_changed(&mut self, cx: &mut gpui::Context<Self>) {
        if !matches!(self.load, Load::Ready(_)) {
            return;
        }
        let Some(team_id) = self.loaded_team.clone() else {
            return;
        };
        if self.loaded_links.as_ref() != Some(&links_snapshot(&team_id, cx)) {
            self.load = Load::Idle;
            cx.notify();
        }
    }

    /// Kick the server fetch when the pane is first shown or the team /
    /// account changed. Runs at render time so a hidden pane never fetches.
    fn ensure_loaded(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        let account_id = Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        if account_id != self.account_id {
            self.account_id = account_id;
            self.load = Load::Idle;
        }
        let same_team = self.loaded_team.as_deref() == Some(team_id);
        if same_team && !matches!(self.load, Load::Idle) {
            return;
        }
        self.start_fetch(team_id, true, cx);
    }

    /// Window activation refetch (EXP-329): re-read the connect state a
    /// browser hand-off may have changed, WITHOUT flipping back to the
    /// skeleton — an alt-tab must not blank a list that is already up.
    fn refetch_stale(&mut self, cx: &mut gpui::Context<Self>) {
        if !matches!(self.load, Load::Ready(_)) {
            return;
        }
        let Some(team_id) = self.loaded_team.clone() else {
            return;
        };
        self.start_fetch(&team_id, false, cx);
    }

    fn start_fetch(&mut self, team_id: &str, show_loading: bool, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };

        if show_loading {
            self.load = Load::Loading;
            self.branches.clear();
            // A full (re)load is a fresh context (pane opened, team switch) —
            // retire any connect-failure notice. The non-loading
            // refetch_stale path deliberately keeps it: the window-activation
            // refetch races the error deep link and must not wipe the copy
            // before the user reads it.
            self.connect_error = None;
        }
        self.loaded_team = Some(team_id.to_string());
        self.loaded_links = Some(links_snapshot(team_id, cx));
        self.generation += 1;
        let generation = self.generation;
        let team_id = team_id.to_string();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    // Banner is best-effort (web: status errors are ignored);
                    // the repo list carries its own error state. Status is
                    // scoped to the team's claimed installations.
                    let status = fetch_github_status(&trpc, &team_id).ok();
                    let repos = fetch_repositories(&trpc, &team_id)
                        .map_err(|err| err.to_string());
                    Loaded { status, repos }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer fetch
                }
                this.load = Load::Ready(result);
                cx.notify();
                // A repo link that changed while this fetch was in flight
                // still lands: compare once more now that the load settled.
                this.refresh_if_links_changed(cx);
            });
        })
        .detach();
    }

    /// Hard refetch after a mutation (add/remove/branch pin): drop the cached
    /// list; the next render re-fetches with the skeleton. The branch-list
    /// cache goes with it — stale branches would resurrect a just-deleted
    /// pick.
    pub(super) fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.load = Load::Idle;
        self.branches.clear();
        cx.notify();
    }

    /// Kick the branch-list fetch for one repo's picker if it isn't already
    /// cached/in flight (EXP-462). Called when the dropdown opens — the pane
    /// never fans out a GitHub read per row up front.
    fn ensure_branches(&mut self, repository_id: &str, cx: &mut gpui::Context<Self>) {
        if self.branches.contains_key(repository_id) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.branches
            .insert(repository_id.to_string(), BranchLoad::Loading);
        let repository_id = repository_id.to_string();
        cx.spawn(async move |this, cx| {
            let fetch_id = repository_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::repositories::list_branches(&trpc, &fetch_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let load = match result {
                    Ok(out) => BranchLoad::Ready(out.branches),
                    Err(err) => {
                        log::warn!("[ui] repositories.listBranches({repository_id}) failed: {err}");
                        BranchLoad::Failed(err.to_string())
                    }
                };
                this.branches.insert(repository_id, load);
                cx.notify();
            });
        })
        .detach();
    }

    /// `repositories.setDefaultBranch` → refetch (EXP-462). `None` = follow
    /// GitHub again. Failures land as a notification on the settings window,
    /// mirroring the remove flow.
    fn set_default_branch(
        &mut self,
        repository_id: String,
        branch: Option<String>,
        handle: gpui::AnyWindowHandle,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let mutate_id = repository_id.clone();
            let mutate_branch = branch.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::repositories::set_default_branch(
                        &trpc,
                        &mutate_id,
                        mutate_branch.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                if result.is_ok() {
                    this.refresh(cx);
                }
                cx.notify();
            });
            if let Err(error) = result {
                log::warn!("[ui] repositories.setDefaultBranch({repository_id}) failed: {error}");
                let message = match &error {
                    // BAD_REQUEST/BAD_GATEWAY messages name the branch/repo —
                    // user-presentable verbatim.
                    api::ApiError::Http { message, .. } => SharedString::from(message.clone()),
                    other => SharedString::from(format!("Could not set the default branch: {other}")),
                };
                let _ = handle.update(cx, |_, window, cx| {
                    window.push_notification(Notification::error(message), cx);
                });
            }
        })
        .detach();
    }

    /// The per-row remove confirm + `repositories.remove` → refetch. The
    /// server refuses (CONFLICT) while any board still points at the repo —
    /// including a trashed one, which no chip shows — so its message is
    /// user-presentable and surfaces verbatim.
    fn confirm_remove(
        &mut self,
        repo: &RepoRow,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        // The OPENER's window — the alert closes on confirm, so a failure
        // notification has to land back on the settings window.
        let handle = window.window_handle();
        let repository_id = repo.id.clone();
        let full_name = repo.full_name.clone();
        let spec = AlertSpec::new(
            "Remove repository",
            format!("This disconnects {full_name} from the team."),
            "Remove",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            let Some(trpc) = queries::trpc_client(cx) else {
                return true;
            };
            let _ = view.update(cx, |this, cx| {
                this.busy = true;
                cx.notify();
            });
            let view = view.clone();
            let repository_id = repository_id.clone();
            let full_name = full_name.clone();
            cx.spawn(async move |cx| {
                let removed_id = repository_id.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { api::repositories::remove(&trpc, &removed_id) })
                    .await;
                let _ = view.update(cx, |this, cx| {
                    this.busy = false;
                    if result.is_ok() {
                        this.refresh(cx);
                    }
                    cx.notify();
                });
                if let Err(error) = result {
                    log::warn!("[ui] repositories.remove({repository_id}) failed: {error}");
                    let message = match &error {
                        api::ApiError::Http { message, .. } => SharedString::from(message.clone()),
                        other => SharedString::from(format!(
                            "Could not remove {full_name}: {other}"
                        )),
                    };
                    let _ = handle.update(cx, |_, window, cx| {
                        window.push_notification(Notification::error(message), cx);
                    });
                }
            })
            .detach();
            true
        });
        open_alert(window, cx, spec);
    }

    /// EXP-557: confirm + `integrations.github.unlink` → refetch. Offered on
    /// STALE links only (zero grants from anyone — a reconnect can never
    /// refresh them, so disconnecting is the one real fix). The server
    /// enforces link-creator-or-owner and refuses (CONFLICT) while a
    /// connected repo still rides the installation — both messages are
    /// user-presentable and surface verbatim.
    fn confirm_disconnect(
        &mut self,
        installation_id: i64,
        label: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(team_id) = self.loaded_team.clone() else {
            return;
        };
        let view = cx.entity().downgrade();
        // The OPENER's window — the alert closes on confirm, so a failure
        // notification has to land back on the settings window.
        let handle = window.window_handle();
        let spec = AlertSpec::new(
            "Disconnect GitHub account",
            format!(
                "This removes {label} from the team. Nobody\u{2019}s GitHub connection \
                 covers it, so no repositories are lost."
            ),
            "Disconnect",
        )
        .on_ok(move |_, cx| {
            let Some(trpc) = queries::trpc_client(cx) else {
                return true;
            };
            let _ = view.update(cx, |this, cx| {
                this.busy = true;
                cx.notify();
            });
            let view = view.clone();
            let team_id = team_id.clone();
            let label = label.clone();
            cx.spawn(async move |cx| {
                let unlink_team = team_id.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        crate::github_connect::github_unlink(&trpc, &unlink_team, installation_id)
                    })
                    .await;
                let _ = view.update(cx, |this, cx| {
                    this.busy = false;
                    if result.is_ok() {
                        this.refresh(cx);
                    }
                    cx.notify();
                });
                if let Err(error) = result {
                    log::warn!(
                        "[ui] integrations.github.unlink({installation_id}) failed: {error}"
                    );
                    let message = match &error {
                        api::ApiError::Http { message, .. } => SharedString::from(message.clone()),
                        other => SharedString::from(format!(
                            "Could not disconnect {label}: {other}"
                        )),
                    };
                    let _ = handle.update(cx, |_, window, cx| {
                        window.push_notification(Notification::error(message), cx);
                    });
                }
            })
            .detach();
            true
        });
        open_alert(window, cx, spec);
    }
}

impl Render for RepositoriesPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return v_flex().child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No team selected."),
            );
        };
        self.ensure_loaded(&team_id, cx);

        let repo_count = match &self.load {
            Load::Ready(loaded) => loaded.repos.as_ref().ok().map(|repos| repos.len()),
            _ => None,
        };
        let dialog_team = team_id.clone();

        let mut body = section(cx)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(card_title("Repositories"))
                    .children(
                        repo_count.map(|count| {
                            chip("repos-count", SharedString::from(count.to_string()), cx)
                        }),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("repos-add")
                            .primary()
                            .web_sm()
                            .icon(registry::UI_ADD)
                            .label("Add repository")
                            .on_click(cx.listener(move |_, _, window, cx| {
                                let pane = cx.entity().downgrade();
                                super::add_repository_dialog::open(
                                    window,
                                    cx,
                                    dialog_team.clone(),
                                    pane,
                                );
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "Connect your GitHub repos to share them with the team \u{2014} \
                         everyone can code on a shared repo. Point a board at one to make \
                         it the clone target for \u{201c}Start coding\u{201d}.",
                    ),
            );

        match &self.load {
            Load::Idle | Load::Loading => {
                body = body.child(
                    v_flex()
                        .gap_2()
                        .child(Skeleton::new().h_4().w_64())
                        .child(Skeleton::new().h_8().w_full())
                        .child(Skeleton::new().h_8().w_full()),
                );
            }
            Load::Ready(loaded) => {
                // GitHub connect state: live server truth — the precedence-
                // ordered primary line plus one line per STALE linked account
                // (EXP-557).
                body = body.child(self.github_status_line(loaded.status.as_ref(), cx));

                // EXP-368: the browser connect hand-off deep-linked back
                // with an error.
                if let Some(message) = self.connect_error.clone() {
                    body = body.child(error_notice(message, cx));
                }

                match &loaded.repos {
                    Err(message) => {
                        body = body.child(error_notice(
                            SharedString::from(message.clone()),
                            cx,
                        ));
                    }
                    Ok(repos) if repos.is_empty() => {
                        // EXP-698: the shared glass row card, not a bespoke
                        // bordered-but-unfilled box.
                        body = body.child(
                            crate::surface::glass_row_card()
                                .px_3()
                                .py_2()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("No repositories connected yet."),
                        );
                    }
                    Ok(repos) => {
                        // Sharer-or-owner row gating (EXP-557): everyone
                        // else gets a read-only row — they can still code on
                        // the shared repo.
                        let owner = super::is_owner(cx, &team_id);
                        let me = queries::active_account(cx).map(|account| account.user_id);
                        let mut list = v_flex().gap_2();
                        for (index, repo) in repos.iter().enumerate() {
                            let can_manage = owner
                                || repo.shared_by.as_ref().is_some_and(|shared| {
                                    me.as_deref() == Some(shared.id.as_str())
                                });
                            list =
                                list.child(self.render_repo_row(index, repo, can_manage, cx));
                        }
                        body = body.child(list);
                    }
                }
            }
        }

        v_flex().child(body)
    }
}

impl RepositoriesPane {
    /// The GitHub connect state: the ONE precedence-ordered muted line
    /// (EXP-329 replaced the stack of boxed banners), plus one line per
    /// STALE linked account with a Disconnect affordance (EXP-557 — a
    /// reconnect can never refresh those, so they must not feed the
    /// reconnect nag). A method (not the old free function) because the
    /// Disconnect button needs `cx.listener`.
    fn github_status_line(
        &self,
        status: Option<&GithubStatus>,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let mut column = v_flex()
            .gap_2()
            .child(primary_status_line(status, cx));
        if let Some(status) = status {
            for (index, inst) in status
                .installations
                .iter()
                .enumerate()
                .filter(|(_, inst)| inst.is_stale())
            {
                column = column.child(self.stale_account_line(index, inst, cx));
            }
        }
        column
    }

    /// One stale linked account (EXP-557): name the dead link and offer the
    /// only real fix — disconnecting it. The server shows foreign stale
    /// links to owners only; a member sees just their own.
    fn stale_account_line(
        &self,
        index: usize,
        inst: &crate::github_connect::GithubInstallation,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let installation_id = inst.installation_id;
        let label = inst.label();
        let label_for_click = label.clone();
        h_flex()
            .w_full()
            .flex_wrap()
            .gap_2()
            .items_center()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(
                Icon::new(registry::UI_WARNING)
                    .xsmall()
                    .flex_shrink_0()
                    .text_color(theme::tokens::YELLOW.to_hsla()),
            )
            .child(div().flex_1().min_w_0().child(SharedString::from(format!(
                "No one\u{2019}s GitHub connection covers {label} anymore \u{2014} \
                 reconnecting can\u{2019}t refresh it."
            ))))
            .child(
                crate::surface::glass_pill_button(("gh-disconnect-stale", index), crate::surface::PillSize::Sm, cx)
                    .label("Disconnect account")
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm_disconnect(
                            installation_id,
                            label_for_click.clone(),
                            window,
                            cx,
                        );
                    })),
            )
    }
}

/// The precedence-ordered primary connect-state line — each arm carries at
/// most one hand-off button. Reconnect/Manage open `connect_url` — the OAuth
/// authorize is what re-captures the per-user repo grants; the App install
/// page does NOT.
fn primary_status_line(status: Option<&GithubStatus>, cx: &gpui::App) -> impl IntoElement {
    let row = h_flex()
        .w_full()
        .flex_wrap()
        .gap_2()
        .items_center()
        .text_xs()
        .text_color(cx.theme().muted_foreground);

    let Some(status) = status else {
        // The probe is best-effort — say nothing definite rather than a false
        // "not connected".
        return row
            .child(Icon::new(registry::UI_GITHUB).xsmall())
            .child("Couldn't reach GitHub connect state.");
    };
    if !status.configured {
        return row
            .child(Icon::new(registry::UI_GITHUB).xsmall())
            .child("GitHub isn't configured on this server.");
    }

    let connect_url = status
        .connect_url
        .clone()
        .or_else(|| status.install_url.clone());

    if !status.installed {
        return row
            .child(Icon::new(registry::UI_GITHUB).xsmall())
            .child("No GitHub account connected")
            .children(connect_url.map(|url| {
                crate::surface::glass_pill_button("gh-connect", crate::surface::PillSize::Sm, cx)
                    .label("Connect GitHub")
                    .on_click(move |_, _, cx| open_url(cx, url.clone()))
            }));
    }

    // Suspension outranks reconnect (REV2-29): a suspended installation mints
    // no tokens and lists no repos, and a reconnect CANNOT fix it — only
    // unsuspending on GitHub can. Never nudge the wrong fix (EXP-365).
    let suspended: Vec<&crate::github_connect::GithubInstallation> = status
        .installations
        .iter()
        .filter(|inst| inst.suspended)
        .collect();
    if !suspended.is_empty() {
        let names = suspended
            .iter()
            .map(|inst| inst.label())
            .collect::<Vec<_>>()
            .join(", ");
        let manage_url = suspended
            .first()
            .map(|inst| inst.manage_url.clone())
            .filter(|url| !url.is_empty())
            .or_else(|| status.install_url.clone());
        return row
            .child(
                Icon::new(registry::UI_WARNING)
                    .xsmall()
                    .text_color(cx.theme().danger),
            )
            .child(format!(
                "GitHub suspended the Exponential app for {names}. Unsuspend it on GitHub."
            ))
            .children(manage_url.map(|url| {
                crate::surface::glass_pill_button("gh-unsuspend", crate::surface::PillSize::Sm, cx)
                    .label("Manage")
                    .on_click(move |_, _, cx| open_url(cx, url.clone()))
            }));
    }

    // A linked installation whose per-user repo grants were never captured
    // lists no repos until the user re-runs the OAuth connect. Name the
    // accounts so the fix is actionable with several linked (EXP-365).
    // STALE accounts are excluded (EXP-557): a reconnect can't refresh them,
    // so they get their own Disconnect line instead of this nag.
    if status
        .installations
        .iter()
        .any(|inst| inst.needs_reconnect())
    {
        let suffix =
            crate::github_connect::reauth_account_suffix(&status.installations, "from");
        return row
            .child(
                Icon::new(registry::UI_WARNING)
                    .xsmall()
                    .text_color(theme::tokens::YELLOW.to_hsla()),
            )
            .child(format!(
                "Reconnect GitHub to refresh which repositories you can access{suffix}."
            ))
            .children(connect_url.map(|url| {
                crate::surface::glass_pill_button("gh-reconnect", crate::surface::PillSize::Sm, cx)
                    .label("Reconnect")
                    .on_click(move |_, _, cx| open_url(cx, url.clone()))
            }));
    }

    // EXP-558: the account names come off `installations[]`, which carries
    // the same logins — the flat `accounts` mirror the server used to send
    // alongside is gone.
    let accounts: Vec<String> = status
        .installations
        .iter()
        .map(|installation| installation.label())
        .collect();
    let label: SharedString = if accounts.is_empty() {
        "GitHub connected".into()
    } else {
        format!("GitHub: {}", accounts.join(", ")).into()
    };
    row.child(
        div()
            .size_2()
            .flex_shrink_0()
            .rounded_full()
            .bg(theme::tokens::GREEN.to_hsla()),
    )
    .child(Icon::new(registry::UI_GITHUB).xsmall())
    .child(label)
    .children(connect_url.map(|url| {
        crate::surface::glass_pill_button("gh-manage", crate::surface::PillSize::Sm, cx)
            .label("Manage")
            .on_click(move |_, _, cx| open_url(cx, url.clone()))
    }))
}

impl RepositoriesPane {
    /// Web `RepoRow`: name + branch/private chips + remove, then the "used by"
    /// line — one chip per board the repo backs (v4 one-repo-per-board; names
    /// resolved server-side). The server refuses a remove while any board
    /// still points here (FK restrict), so a linked repo's button says so up
    /// front instead of firing a doomed mutation. `can_manage` is the
    /// sharer-or-owner gate (EXP-557): without it the row is read-only — a
    /// static branch chip, no remove.
    fn render_repo_row(
        &self,
        index: usize,
        repo: &RepoRow,
        can_manage: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let mut head = h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(
                Icon::new(registry::UI_GITHUB)
                    .small()
                    .flex_shrink_0()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::from(repo.full_name.clone())),
            );
        head = if can_manage {
            head.child(self.branch_picker(index, repo, cx))
        } else {
            head.child(chip(
                ("repo-branch-static", index),
                SharedString::from(repo.default_branch.clone()),
                cx,
            ))
        };
        if repo.private {
            head = head.child(private_chip(index, cx));
        }

        if can_manage {
            // EXP-698: the one 32px glass chrome every trailing row action wears.
            let remove = crate::controls::glass_icon_button(
                ("repo-remove", index),
                Icon::new(registry::UI_DELETE),
                cx,
            );
            let linked = repo.boards.len();
            head = head.child(if linked > 0 {
                let plural = if linked == 1 { "" } else { "s" };
                remove.disabled(true).tooltip(SharedString::from(format!(
                    "In use by {linked} board{plural}. Change their repository first."
                )))
            } else {
                let repo_for_remove = repo.clone();
                remove
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm_remove(&repo_for_remove, window, cx);
                    }))
            });
        }

        let mut links = h_flex().flex_wrap().gap_1p5().pl_6().items_center();
        if repo.boards.is_empty() {
            links = links.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Not used by any board"),
            );
        } else {
            // The chips alone read as loose board names — the label says what
            // the relationship is.
            links = links.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Used by"),
            );
            for board in &repo.boards {
                // EXP-698: the shared non-interactive glass chip.
                links = links.child(
                    crate::surface::glass_pill(
                        SharedString::from(format!("repo-board-{}", board.id)),
                        crate::surface::PillSize::Sm,
                        crate::surface::PillMode::Readonly,
                        cx,
                    )
                    .child(SharedString::from(board.name.clone())),
                );
            }
        }
        // EXP-557: name the sharer so read-only rows explain themselves.
        if let Some(sharer) = repo.shared_by.as_ref().and_then(RepoSharedBy::label) {
            links = links.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(format!("\u{b7} Shared by {sharer}"))),
            );
        }

        // EXP-698: the shared glass ROW CARD — the repo list around it is
        // gapped, so each repo is its own object.
        crate::surface::glass_row_card()
            .flex()
            .flex_col()
            .gap_1p5()
            .px_3()
            .py_2()
            .child(head)
            .child(links)
    }

    /// The row's branch chip as a picker (EXP-462, web parity:
    /// `DefaultBranchMenu`): shows the effective default branch; the popover
    /// lists the repo's branches (fetched live from GitHub on first open)
    /// behind a search input (EXP-469 — repos routinely carry more branches
    /// than a scrollable menu can be skimmed for), "default" marks GitHub's
    /// own — picking it clears the pin so the repo follows GitHub again.
    fn branch_picker(
        &self,
        index: usize,
        repo: &RepoRow,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        use gpui::{ElementId, InteractiveElement as _, StatefulInteractiveElement as _};

        let button = crate::surface::glass_pill_button(("repo-branch", index), crate::surface::PillSize::Sm, cx)
            .max_w(px(240.))
            .label(SharedString::from(repo.default_branch.clone()))
            .disabled(self.busy);

        let pane = cx.entity().clone();
        let repo_id = repo.id.clone();
        let effective = repo.default_branch.clone();
        let github_default = repo.github_default_branch.clone();
        let query = self.branch_query.clone();
        let query_for_open = query.clone();
        let pane_for_open = pane.clone();
        let repo_id_for_open = repo_id.clone();
        Popover::new(("repo-branch-picker", index))
            .p_1()
            .w(px(crate::pickers::PICKER_SEARCH_WIDTH))
            .trigger(button)
            .on_open_change(move |open, window, cx| {
                // Fresh search per open; the input takes focus like the web
                // CommandInput. Opening also kicks the lazy branch fetch —
                // the popover content re-renders on pane notify, so the list
                // appears in place once it lands.
                query_for_open.update(cx, |input, cx| input.set_value("", window, cx));
                if *open {
                    query_for_open.read(cx).focus_handle(cx).focus(window, cx);
                    pane_for_open.update(cx, |this, cx| {
                        this.ensure_branches(&repo_id_for_open, cx);
                    });
                }
            })
            .content(move |_, window, cx| {
                let popover_state = cx.entity();
                let load = pane.read(cx).branches.get(&repo_id).cloned();
                let filter = query.read(cx).value().trim().to_lowercase();

                let mut column = v_flex()
                    .w_full()
                    .child(Input::new(&query).web_input_sm().appearance(false).cleanable(true))
                    .child(
                        div()
                            .h(px(1.))
                            .w_full()
                            .my_1()
                            .bg(cx.theme().border.opacity(0.5)),
                    );
                match load {
                    None | Some(BranchLoad::Loading) => {
                        column = column.child(
                            h_flex()
                                .gap_2()
                                .px_2()
                                .py_1()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("Loading branches\u{2026}"),
                        );
                    }
                    Some(BranchLoad::Failed(message)) => {
                        column = column.child(
                            div()
                                .px_2()
                                .py_1()
                                .text_sm()
                                .text_color(cx.theme().danger)
                                .child(SharedString::from(format!(
                                    "Couldn't load branches: {message}"
                                ))),
                        );
                        let pane = pane.clone();
                        let repo_id = repo_id.clone();
                        column = column.child(
                            crate::pickers::picker_row(("repo-branch-retry", index), cx)
                                .child("Retry")
                                .on_click(move |_, _, cx| {
                                    pane.update(cx, |this, cx| {
                                        this.branches.remove(&repo_id);
                                        this.ensure_branches(&repo_id, cx);
                                    });
                                }),
                        );
                    }
                    Some(BranchLoad::Ready(branches)) => {
                        // The effective branch always renders even when GitHub
                        // no longer has it (a pinned branch deleted upstream) —
                        // the list must show what the row is set to.
                        let mut names: Vec<String> = Vec::new();
                        if !branches.contains(&effective) {
                            names.push(effective.clone());
                        }
                        names.extend(branches);
                        names.retain(|name| {
                            filter.is_empty() || name.to_lowercase().contains(&filter)
                        });
                        if names.is_empty() {
                            return column
                                .child(crate::pickers::empty_picker_row("No branches found.", cx));
                        }

                        let handle = window.window_handle();
                        let mut rows = v_flex()
                            .id(("repo-branch-rows", index))
                            .w_full()
                            .max_h(px(240.))
                            .overflow_y_scroll();
                        for name in names {
                            let is_github_default = github_default.as_deref() == Some(&name);
                            let checked = name == effective;
                            let mut row = crate::pickers::picker_row(
                                ElementId::Name(SharedString::from(format!(
                                    "repo-branch-{repo_id}-{name}"
                                ))),
                                cx,
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .font_family(theme::terminal::FONT_FAMILY)
                                    .text_xs()
                                    .child(SharedString::from(name.clone())),
                            );
                            if is_github_default {
                                row = row.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .flex_shrink_0()
                                        .child("default"),
                                );
                            }
                            if checked {
                                row = row.child(
                                    Icon::new(registry::UI_CHECK)
                                        .size_3()
                                        .flex_shrink_0()
                                        .text_color(cx.theme().muted_foreground),
                                );
                            } else {
                                let pane = pane.clone();
                                let repo_id = repo_id.clone();
                                let popover_state = popover_state.clone();
                                row = row.on_click(move |_, window, cx| {
                                    let pick = if is_github_default {
                                        None
                                    } else {
                                        Some(name.clone())
                                    };
                                    pane.update(cx, |this, cx| {
                                        this.set_default_branch(repo_id.clone(), pick, handle, cx);
                                    });
                                    popover_state.update(cx, |state, cx| state.dismiss(window, cx));
                                });
                            }
                            rows = rows.child(row);
                        }
                        column = column.child(rows);
                    }
                }
                column
            })
            .into_any_element()
    }
}

/// The "Private" chip — same outline chip with the lock glyph, so private
/// repos read at a glance instead of by word.
fn private_chip(index: usize, cx: &gpui::App) -> impl IntoElement {
    use crate::surface::{PillMode, PillSize};
    crate::surface::glass_pill(("repo-private", index), PillSize::Sm, PillMode::Readonly, cx)
        .child(Icon::new(registry::UI_PRIVATE).with_size(gpui::px(PillSize::Sm.glyph())))
        .child("Private")
}

/// Outline chip (web `Badge variant="outline"` at compact density).
fn chip(id: impl Into<gpui::ElementId>, label: SharedString, cx: &gpui::App) -> impl IntoElement {
    use crate::surface::{PillMode, PillSize};
    crate::surface::glass_pill(id, PillSize::Sm, PillMode::Readonly, cx)
    .font_family(theme::terminal::FONT_FAMILY)
    .child(label)
}
