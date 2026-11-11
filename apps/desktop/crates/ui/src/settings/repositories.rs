//! Settings → Repositories (masterplan-v3 §4.2 + §7.9, SLOP-7 one flow).
//!
//! Web parity: `components/team/repositories-section.tsx`. This pane
//! shows the **live server truth** for GitHub connect state — never a local
//! guess (the old native app falsely said "not connected"):
//!
//! - the VIEWER's own GitHub connection off `integrations.github.status`
//!   (SLOP-7: not connected → Connect GitHub; connected as `login` with
//!   Disconnect; an expired token → Reconnect), then the accounts where the
//!   app is installed with a Configure link each and Install on another
//!   account. Nothing is per team: the connection is the person's, the
//!   repositories below are the team's;
//! - the team's connected repos + their board links off
//!   `repositories.list` (server-only tables — read via tRPC, never synced).
//!
//! Connect/Reconnect = the in-app link hop ([`crate::github_connect::connect_github`]:
//! a link ticket, the system browser, `exponential://oauth-return?linked=github`
//! back); Install the app = GitHub's install page in the browser (the App's
//! setup URL lands on the web page, which hands back through
//! `exponential://github-connected`). EXP-329 gave the pane the two mutations
//! the web card has had all along — "Add repository" (the
//! [`super::add_repository_dialog`] picker over `repositories.add`) and the
//! per-row remove. The shared status/repo fetches live in
//! [`crate::github_connect`].
//!
//! A browser hand-off finishes OUTSIDE the app, so the two deep links above
//! are the definitive refetch triggers and window activation the fallback:
//! coming back re-reads the connect state without flashing the skeleton over
//! a list that is already up.
//!
//! EXP-557 per-user sharing: the pane is MEMBER-visible — connecting a repo
//! shares it with the team, and per-row management (remove, branch pin) is
//! sharer-or-owner.
//!
//! #EXP-1629: the pane is a document on the native binding (projection at
//! `/pane`): the band with Add repository, the skeleton, the GitHub status
//! line in each of its states, the failed and empty states, the rows
//! (glyph, name, static branch chip, Private, remove, the access banner,
//! the board chips and the sharer) and the Local account's folders. Each
//! managed row's branch picker is a `pill` Select (#EXP-1765) whose `open`
//! kicks the lazy branch fetch; the dialogs and the alerts stay native.

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    div, px, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{button::ButtonVariant, ActiveTheme as _, Icon, Sizable as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sync::Store;

use eui_gpui::binding::Binding;
use eui_gpui::controls::CTL_MD_H;
use eui_gpui::host::HostPlugin;
use eui_gpui::view::SurfaceView;

use crate::github_connect::{copy, fetch_github_status, GithubStatus, Prerequisite};
use crate::native_dialog::{open_alert, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;
use crate::repo_resolver::links_snapshot;

use super::doc::{empty_state, eq, failed_state, hex, mount_with, not, path, IdeToasts, Ink, PillPaint, SM, VALUE_ALPHA, XS};
use super::open_url;
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
    /// picking it clears the pin server-side. `None` = unknown.
    #[serde(default)]
    pub github_default_branch: Option<String>,
    #[serde(default)]
    pub private: bool,
    /// The GitHub App installation the repo was connected through — joins
    /// the row to `integrations.github.status`'s installations for the
    /// per-repo suspended banner (EXP-774, web parity).
    #[serde(default)]
    pub installation_id: Option<i64>,
    /// Stamped when the App lost access to the repo (token mint failed);
    /// cleared on the next successful heal. Drives the "Re-grant" banner.
    #[serde(default)]
    pub inaccessible_at: Option<String>,
    /// The member who shared (connected) this repo with the team (EXP-557).
    /// Drives the "Shared by" line and the sharer-or-owner row gating.
    /// `None` on pre-sharing rows — owner-managed only.
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
    /// The fetch failed (a toast said so); the picker reads empty and the
    /// next open fetches again.
    Failed,
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
    /// FEED-79: the load in flight is the person's Retry press, so its
    /// failure toasts; every other load (on open, on focus, after a write)
    /// fails quietly into the empty state.
    retry_pressed: bool,
    /// #EXP-1727: a Local desktop's linked folders (`None` = not fetched).
    local: Option<Vec<api::repositories::LocalRepository>>,
    #[cfg(test)]
    shot: Option<RepoShot>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

/// A pinned state for the headless bench (the headless store has no team,
/// no account and no synced boards).
#[cfg(test)]
pub(crate) struct RepoShot {
    pub team: Option<String>,
    pub owner: bool,
    /// The viewer's user id (the sharer-or-owner gate).
    pub me: Option<String>,
    /// A Local desktop's account: its linked folders.
    pub local: Option<Vec<api::repositories::LocalRepository>>,
    /// The synced boards the chips read their glyph and tint off.
    pub boards: Vec<domain::rows::Board>,
    pub busy: bool,
    /// Branch lists already fetched, by repo id.
    pub branches: Vec<(String, Vec<String>)>,
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
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
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
                        this.connect_failed(crate::github_connect::connect_error_message(&code).into(), cx);
                    }
                }
            }),
            // SLOP-25: the Connect/Reconnect GitHub hop ends on
            // `exponential://oauth-return?linked=github` — refetch on success,
            // the web's link-error line otherwise.
            cx.observe_global::<crate::oauth::SignInLinkOutcome>(|this, cx| {
                match crate::github_connect::github_link_outcome(cx) {
                    Some(Ok(())) => {
                        if matches!(this.load, Load::Ready(_)) {
                            this.refetch_stale(cx);
                        } else {
                            this.load = Load::Idle;
                            cx.notify();
                        }
                    }
                    Some(Err(message)) => this.connect_failed(message.to_string(), cx),
                    None => {}
                }
            }),
        ];
        // The actions leave the binding for the pane: the dialogs, the
        // browser hand-offs and the reads.
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["add", "link_folder", "status_retry", "retry", "connect", "disconnect", "manage_suspended", "install", "configure", "regrant", "remove", "branches", "branch"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let key = event.context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
                    // A branch pick's Select value.
                    let value = event.payload.as_ref().and_then(|p| p.get("value")).and_then(Value::as_str).map(str::to_string);
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update and these write it.
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| pane.action(name, &key, value, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let theme = cx.theme();
        let tints = Tints {
            skeleton: hex(theme.muted_foreground.opacity(0.18)),
            danger: hex(theme.danger),
            warning: hex(theme::tokens::YELLOW.to_hsla()),
            live: hex(theme::tokens::GREEN.to_hsla()),
            link: hex(theme.link),
            list_hover: hex(theme.list_hover),
        };
        let plugin = Rc::new(RepoHost { ide: super::doc::ide_plugin() });
        let view = mount_with("settings-repositories", document(&Ink::of(cx), &tints), &binding, plugin, true, window, cx);
        Self {
            nav,
            load: Load::Idle,
            loaded_team: None,
            account_id: None,
            loaded_links: None,
            generation: 0,
            busy: false,
            branches: HashMap::new(),
            retry_pressed: false,
            local: None,
            #[cfg(test)]
            shot: None,
            binding,
            view,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(crate::navigation::nav_for_window(window, cx), window, cx)
    }

    /// The pane in a pinned state: `status` = `integrations.github.status`
    /// (`None` = the probe failed), `repos` = `None` loading, `Some(Err)`
    /// failed, `Some(Ok)` the `repositories.list` rows (JSON).
    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: RepoShot, status: Option<serde_json::Value>, repos: Option<Result<serde_json::Value, String>>, cx: &mut gpui::Context<Self>) {
        self.loaded_team = shot.team.clone();
        self.local = shot.local.clone();
        self.busy = shot.busy;
        self.branches = shot.branches.iter().map(|(id, names)| (id.clone(), BranchLoad::Ready(names.clone()))).collect();
        self.load = match repos {
            None => Load::Loading,
            Some(repos) => Load::Ready(Loaded {
                status: status.map(|status| serde_json::from_value(status).expect("a github status")),
                repos: repos.map(|rows| serde_json::from_value(rows).expect("repository rows")),
            }),
        };
        self.shot = Some(shot);
        cx.notify();
    }

    fn pinned(&self) -> bool {
        #[cfg(test)]
        if self.shot.is_some() {
            return true;
        }
        false
    }

    fn pane_local(&self, cx: &gpui::App) -> bool {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.local.is_some();
        }
        crate::queries::active_account(cx).is_some_and(|a| a.is_local())
    }

    fn pane_team(&self, cx: &gpui::App) -> Option<String> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.team.clone();
        }
        active_team_id(&self.nav, cx)
    }

    fn pane_owner(&self, team_id: &str, cx: &gpui::App) -> bool {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.owner;
        }
        super::is_owner(cx, team_id)
    }

    fn pane_me(&self, cx: &gpui::App) -> Option<String> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.me.clone();
        }
        queries::active_account(cx).map(|account| account.user_id)
    }

    /// The synced board a chip reads its glyph and tint off.
    fn synced_board(&self, id: &str, cx: &gpui::App) -> Option<domain::rows::Board> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.boards.iter().find(|board| board.id == id).cloned();
        }
        Store::global(cx).collections().boards.read(cx).get(id).cloned()
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
                        .map_err(|err| err.user_message());
                    Loaded { status, repos }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer fetch
                }
                // FEED-79: a failed list = the empty state + Retry; only a
                // Retry press that fails again toasts.
                let attended = std::mem::take(&mut this.retry_pressed);
                if let Err(message) = &result.repos {
                    crate::action_error::failed_load(
                        "the repositories",
                        attended,
                        Some(message.clone()),
                        cx,
                    );
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

    /// The failed list's Retry: the same load, attended.
    fn retry_load(&mut self, cx: &mut gpui::Context<Self>) {
        self.retry_pressed = true;
        self.refresh(cx);
    }

    /// Kick the branch-list fetch for one repo's picker if it isn't already
    /// cached/in flight (EXP-462). Called when the dropdown opens — the pane
    /// never fans out a GitHub read per row up front.
    fn ensure_branches(&mut self, repository_id: &str, cx: &mut gpui::Context<Self>) {
        // A failed list fetches again on the next open (FEED-79).
        if matches!(
            self.branches.get(repository_id),
            Some(BranchLoad::Loading | BranchLoad::Ready(_))
        ) {
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
                    // Loaded as the picker opens (FEED-79): quiet, the
                    // empty picker; the next open fetches again.
                    Err(err) => {
                        log::warn!("[ui] repositories.listBranches({repository_id}) failed: {err}");
                        BranchLoad::Failed
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
                // BAD_REQUEST/BAD_GATEWAY messages name the branch/repo —
                // user-presentable verbatim.
                let note = crate::action_error::failed_toast(
                    "set",
                    "the default branch",
                    crate::action_error::action_error_description(&error),
                );
                let _ = handle.update(cx, |_, window, cx| {
                    crate::toast::show(note, window, cx);
                });
            }
        })
        .detach();
    }

    /// The per-row remove confirm + `repositories.remove` → refetch. The
    /// server refuses (CONFLICT) while any board still points at the repo —
    /// including a trashed one, which no chip shows — so its message is
    /// user-presentable and surfaces verbatim.
    // dialog stays native: the shared native confirm alert.
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
        let spec = AlertSpec::from_prompt(
            "Remove repository",
            &domain::prompts::remove_repository(&full_name),
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
                    let note = crate::action_error::failed_toast(
                        "remove",
                        &full_name,
                        crate::action_error::action_error_description(&error),
                    );
                    let _ = handle.update(cx, |_, window, cx| {
                        crate::toast::show(note, window, cx);
                    });
                }
            })
            .detach();
            true
        });
        open_alert(window, cx, spec);
    }

    /// SLOP-7: confirm + `integrations.github.disconnect` → refetch. Unlinks
    /// the viewer's GitHub account; repositories already added keep working
    /// (their tokens mint off the App installation). The server's refusal
    /// (GitHub login on and no other way in) is user-presentable verbatim.
    // dialog stays native: the shared native confirm alert (Disconnect as the danger answer).
    fn confirm_disconnect(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let view = cx.entity().downgrade();
        let spec = AlertSpec::new(
            copy::DISCONNECT_CONFIRM_TITLE,
            copy::DISCONNECT_BODY,
            copy::DISCONNECT,
        )
        // The web's Prompt: Disconnect is the destructive answer, Cancel
        // takes Return (prompts.json roles + focus).
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
            cx.spawn(async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { crate::github_connect::github_disconnect(&trpc) })
                    .await;
                if let Err(error) = &result {
                    log::warn!("[ui] integrations.github.disconnect failed: {error}");
                }
                let _ = view.update(cx, |this, cx| {
                    this.busy = false;
                    match &result {
                        Ok(()) => this.refresh(cx),
                        // FEED-79: the server's refusal (a connected repo
                        // still rides the account) is the toast's sentence.
                        Err(error) => crate::action_error::failed_action_err(
                            "disconnect",
                            "GitHub",
                            error,
                            cx,
                        ),
                    }
                    cx.notify();
                });
            })
            .detach();
            true
        });
        open_alert(window, cx, spec);
    }

    /// Connect / Reconnect GitHub: the in-app link hop. The outcome lands as a
    /// [`crate::oauth::SignInLinkOutcome`] the constructor observes.
    fn connect_github(&mut self, cx: &mut gpui::Context<Self>) {
        cx.notify();
        crate::github_connect::connect_github(cx);
    }

    /// FEED-79 / EXP-368: the connect hand-back ended in an error — the
    /// error toast; Retry runs the hop again.
    fn connect_failed(&mut self, message: String, cx: &mut gpui::Context<Self>) {
        let pane = cx.entity().downgrade();
        crate::action_error::failed_action_with_retry(
            "connect",
            "GitHub",
            Some(message),
            move |_, cx| {
                let _ = pane.update(cx, |this, cx| this.connect_github(cx));
            },
            cx,
        );
        cx.notify();
    }
}

impl RepositoriesPane {
    /// #EXP-1727: a Local desktop's folders, read off its executor.
    fn load_local(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
        self.local = Some(api::repositories::list_local(&trpc).unwrap_or_default());
        cx.notify();
    }

    /// Link a folder on this machine (the native open panel) to the active board.
    fn link_folder(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let receiver = crate::file_picker::prompt_for_paths(cx, gpui::PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some(copy::LINK_FOLDER.into()) });
        let board = crate::navigation::active_board_id(&self.nav, cx);
        cx.spawn_in(window, async move |this, window| {
            let Ok(Ok(Some(paths))) = receiver.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            let _ = this.update_in(window, |this, _, cx| {
                let Some(trpc) = crate::queries::trpc_client(cx) else { return };
                match api::repositories::link_folder(&trpc, &path.to_string_lossy(), board.as_deref()) {
                    Ok(_) => this.load_local(cx),
                    Err(error) => crate::action_error::failed_action("link", "the folder", Some(error.to_string()), cx),
                }
            });
        })
        .detach();
    }
}

// ---------------------------------------------------------------------------
// The document (#EXP-1629)
// ---------------------------------------------------------------------------

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

/// An icon the projection names by its asset path (a board's own glyph).
const ASSET: &str = "asset:";

/// The pane's host: the IDE's plugin and the boards' glyphs by asset path.
struct RepoHost {
    ide: crate::exponential_ui_host::IdePlugin,
}

impl HostPlugin for RepoHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        match name.strip_prefix(ASSET) {
            Some(path) => Some(SharedString::from(path.to_string())),
            None => self.ide.icon(name),
        }
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.ide.on_unknown(node)
    }
}

/// The colours the document needs beyond [`Ink`].
struct Tints {
    skeleton: String,
    danger: String,
    warning: String,
    live: String,
    link: String,
    list_hover: String,
}

/// Where a repo's access banner hands off: its installation's page, else
/// the install page.
fn repo_manage_url(repo: &RepoRow, status: Option<&GithubStatus>) -> Option<String> {
    status
        .and_then(|status| status.installations.iter().find(|inst| Some(inst.installation_id) == repo.installation_id))
        .map(|inst| inst.manage_url.clone())
        .filter(|url| !url.is_empty())
        .or_else(|| status.and_then(|status| status.install_url.clone()))
}

/// The suspended line's Manage: the first suspended installation's page,
/// else the install page.
fn suspended_manage_url(status: &GithubStatus) -> Option<String> {
    status
        .installations
        .iter()
        .find(|inst| inst.suspended)
        .map(|inst| inst.manage_url.clone())
        .filter(|url| !url.is_empty())
        .or_else(|| status.install_url.clone())
}

impl RepositoriesPane {
    fn loaded(&self) -> Option<&Loaded> {
        match &self.load {
            Load::Ready(loaded) => Some(loaded),
            _ => None,
        }
    }

    /// A document action (on the row / installation `key` where it names
    /// one; a branch pick's `value`).
    fn action(&mut self, name: &str, key: &str, value: Option<String>, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let status = self.loaded().and_then(|loaded| loaded.status.clone());
        match name {
            // dialog stays native: `add_repository_dialog` is its own window (the searchable GitHub picker + by-name lookup).
            "add" => {
                if let Some(team) = self.pane_team(cx) {
                    let pane = cx.entity().downgrade();
                    super::add_repository_dialog::open(window, cx, team, pane);
                }
            }
            // dialog stays native: the system folder picker.
            "link_folder" => self.link_folder(window, cx),
            "status_retry" => self.refetch_stale(cx),
            "retry" => self.retry_load(cx),
            "connect" if !self.busy => self.connect_github(cx),
            "disconnect" if !self.busy => self.confirm_disconnect(window, cx),
            "manage_suspended" => {
                if let Some(url) = status.as_ref().and_then(suspended_manage_url) {
                    open_url(cx, url);
                }
            }
            "install" => {
                if let Some(url) = status.and_then(|status| status.install_url) {
                    open_url(cx, url);
                }
            }
            "configure" => {
                let url = status
                    .and_then(|status| status.installations.into_iter().find(|inst| inst.installation_id.to_string() == key))
                    .map(|inst| inst.manage_url)
                    .filter(|url| !url.is_empty());
                if let Some(url) = url {
                    open_url(cx, url);
                }
            }
            // A row's branch picker opened: the lazy fetch (EXP-462).
            "branches" => self.ensure_branches(key, cx),
            // A branch picked: pin it; GitHub's own default clears the pin
            // so the repo follows GitHub again.
            "branch" if !self.busy => {
                let Some(value) = value else { return };
                let Some(repo) = self.loaded().and_then(|loaded| loaded.repos.as_ref().ok()).and_then(|rows| rows.iter().find(|row| row.id == key)) else {
                    return;
                };
                if value == repo.default_branch {
                    return;
                }
                let pick = (repo.github_default_branch.as_deref() != Some(value.as_str())).then_some(value);
                self.set_default_branch(key.to_string(), pick, window.window_handle(), cx);
            }
            "regrant" | "remove" => {
                let Some(repo) = self.loaded().and_then(|loaded| loaded.repos.as_ref().ok()).and_then(|rows| rows.iter().find(|row| row.id == key)).cloned() else {
                    return;
                };
                if name == "regrant" {
                    if let Some(url) = repo_manage_url(&repo, status.as_ref()) {
                        open_url(cx, url);
                    }
                } else if !self.busy && repo.boards.is_empty() {
                    self.confirm_remove(&repo, window, cx);
                }
            }
            _ => {}
        }
    }

    /// The pane's state as the document reads it.
    fn project(&self, local: bool, team: Option<&str>, cx: &App) -> Value {
        let mut out = json!({"local": local, "noTeam": !local && team.is_none(), "list": !local && team.is_some(), "busy": self.busy,
            "localEmpty": false, "localHasRows": false, "localRows": [],
            "loading": false, "status": "", "failed": false, "empty": false, "hasRows": false, "rows": [],
            "installations": []});
        if local {
            let rows = self.local.clone().unwrap_or_default();
            out["localEmpty"] = json!(rows.is_empty());
            out["localHasRows"] = json!(!rows.is_empty());
            out["localRows"] = rows.iter().map(|row| json!({"key": row.id, "name": row.full_name, "path": row.path, "branch": row.default_branch})).collect();
        }
        if let (false, Some(team_id)) = (local, team) {
            match &self.load {
                Load::Idle | Load::Loading => out["loading"] = json!(true),
                Load::Ready(loaded) => {
                    let status = loaded.status.as_ref();
                    out["status"] = json!(match status.map(GithubStatus::prerequisite) {
                        None => "failed",
                        Some(Prerequisite::NotConfigured) => "notConfigured",
                        Some(Prerequisite::NotLinked) => "notLinked",
                        Some(Prerequisite::Expired) => "expired",
                        Some(Prerequisite::NotInstalled | Prerequisite::Ready) => "linked",
                    });
                    if let Some(status) = status {
                        out["connectedText"] = json!(match status.login.as_deref() {
                            Some(login) if !login.is_empty() => copy::connected_as(login),
                            _ => copy::CONNECTED.to_string(),
                        });
                        let suspended = status.suspended_labels();
                        out["hasSuspended"] = json!(!suspended.is_empty());
                        out["suspendedText"] = json!(copy::suspended_line(&suspended));
                        out["hasSuspendedManage"] = json!(suspended_manage_url(status).is_some());
                        let installed = !status.installations.is_empty();
                        out["notInstalled"] = json!(!installed);
                        out["hasInstallUrl"] = json!(status.install_url.is_some());
                        out["hasInstallations"] = json!(installed);
                        out["installAnother"] = json!(installed && status.install_url.is_some());
                        out["installations"] = status
                            .installations
                            .iter()
                            .map(|inst| {
                                let label = inst.label();
                                json!({"key": inst.installation_id.to_string(), "icon": if inst.is_organization() { "ui-organization" } else { "ui-avatar-placeholder" },
                                    "label": label, "hasConfigure": !inst.manage_url.is_empty(), "configureTip": copy::configure_title(&label)})
                            })
                            .collect();
                    }
                    match &loaded.repos {
                        Err(_) => out["failed"] = json!(true),
                        Ok(repos) if repos.is_empty() => out["empty"] = json!(true),
                        Ok(repos) => {
                            out["hasRows"] = json!(true);
                            let owner = self.pane_owner(team_id, cx);
                            let me = self.pane_me(cx);
                            let muted = hex(cx.theme().muted_foreground);
                            let rows: Vec<Value> = repos
                                .iter()
                                .map(|repo| {
                                    let manage = owner || repo.shared_by.as_ref().is_some_and(|shared| me.as_deref() == Some(shared.id.as_str()));
                                    let (branch_options, branch_loading) = if manage { self.branch_options(repo) } else { (Vec::new(), false) };
                                    let linked = repo.boards.len();
                                    let suspended = status
                                        .and_then(|status| status.installations.iter().find(|inst| Some(inst.installation_id) == repo.installation_id))
                                        .is_some_and(|inst| inst.suspended);
                                    let (banner, action) = if suspended {
                                        ("GitHub suspended the Exponential app for this repository's account. Unsuspend it on GitHub to code on this repo again.", "Unsuspend")
                                    } else {
                                        ("The GitHub App lost access to this repository. Re-grant it on GitHub.", "Re-grant")
                                    };
                                    let boards: Vec<Value> = repo
                                        .boards
                                        .iter()
                                        .map(|board| {
                                            let synced = self.synced_board(&board.id, cx);
                                            let icon = synced.as_ref().map(|row| format!("{ASSET}{}", gpui_component::IconNamed::path(crate::icons::board_glyph(row))));
                                            let color = synced.as_ref().and_then(|row| row.color.as_deref()).and_then(super::parse_hex_color).map_or(muted.clone(), hex);
                                            json!({"key": board.id, "name": board.name, "hasIcon": icon.is_some(), "icon": icon.unwrap_or_default(), "color": color})
                                        })
                                        .collect();
                                    let plural = if linked == 1 { "" } else { "s" };
                                    let sharer = repo.shared_by.as_ref().and_then(RepoSharedBy::label);
                                    json!({
                                        "key": repo.id,
                                        "name": repo.full_name,
                                        "manage": manage,
                                        "branch": repo.default_branch,
                                        "branchLive": manage && !self.busy,
                                        "branchBusy": manage && self.busy,
                                        "branchLabel": format!("Default branch for {}", repo.full_name),
                                        "branchOptions": branch_options,
                                        "branchLoading": branch_loading,
                                        "private": repo.private,
                                        "removeOn": manage && linked == 0 && !self.busy,
                                        "removeLinked": manage && linked > 0,
                                        "removeBusy": manage && linked == 0 && self.busy,
                                        "removeTip": format!("In use by {linked} board{plural}. Change their repository first."),
                                        "banner": repo.inaccessible_at.is_some() || suspended,
                                        "bannerText": banner,
                                        "bannerAction": action,
                                        "bannerManage": repo_manage_url(repo, status).is_some(),
                                        "noBoards": repo.boards.is_empty(),
                                        "hasBoards": !repo.boards.is_empty(),
                                        "boards": boards,
                                        "hasShared": sharer.is_some(),
                                        "sharedText": sharer.map(|sharer| format!("\u{b7} Shared by {sharer}")).unwrap_or_default(),
                                    })
                                })
                                .collect();
                            out["rows"] = Value::Array(rows);
                        }
                    }
                }
            }
        }
        out
    }

    /// A managed row's picker options + whether they still load: the
    /// branches in mono, "default" trailing GitHub's own; the effective
    /// branch always lists, even when GitHub no longer has it (a pinned
    /// branch deleted upstream) or the fetch failed (FEED-79: quiet).
    fn branch_options(&self, repo: &RepoRow) -> (Vec<Value>, bool) {
        let branches = match self.branches.get(&repo.id) {
            // Not fetched yet (the open fetches) or in flight: the skeleton.
            None | Some(BranchLoad::Loading) => return (Vec::new(), true),
            Some(BranchLoad::Failed) => Vec::new(),
            Some(BranchLoad::Ready(branches)) => branches.clone(),
        };
        let effective = &repo.default_branch;
        let mut names: Vec<String> = Vec::new();
        if !branches.contains(effective) {
            names.push(effective.clone());
        }
        names.extend(branches);
        let options = names
            .into_iter()
            .map(|name| {
                let mut option = json!({"label": name, "value": name, "mono": true});
                if repo.github_default_branch.as_deref() == Some(name.as_str()) {
                    option["trailing"] = json!("default");
                }
                option
            })
            .collect();
        (options, false)
    }
}

impl Render for RepositoriesPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "RepositoriesPane").entered();
        // #EXP-1727: a Local desktop links folders on this machine; nothing
        // clones and no GitHub install is involved.
        let local = self.pane_local(cx);
        if local && self.local.is_none() {
            self.load_local(cx);
        }
        let team = if local { None } else { self.pane_team(cx) };
        if let (false, Some(team_id), false) = (local, team.as_deref(), self.pinned()) {
            self.ensure_loaded(team_id, cx);
        }
        let projection = self.project(local, team.as_deref(), cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        // The bench's open branch picker: the headless clock stands still,
        // so its popup's enter fade runs out on the frames after the click.
        #[cfg(test)]
        if self.pinned() && !self.view.read(cx).layers().is_empty() {
            self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
        }
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// An `Icon` node at `size` in `color`.
fn icon(id: &str, name: Value, size: f32, color: &str) -> Value {
    json!({"id": id, "component": "Icon", "props": {"name": name}, "style": {"width": size, "height": size, "color": color, "flexShrink": 0}})
}

/// A text run that takes the row's free space (`flex_1 min_w_0`) and wraps.
fn grow(mut text: Value) -> Value {
    text["style"]["flexGrow"] = json!(1);
    text["style"]["flexShrink"] = json!(1);
    text["style"]["flexBasis"] = json!(0);
    text["style"]["minWidth"] = json!(0);
    text
}

/// [`grow`] on one ellipsised line.
fn grow_line(text: Value) -> Value {
    let mut text = grow(text);
    text["props"]["lines"] = json!(1);
    text
}

/// `h_flex().flex_wrap().gap_2().items_center()` (+ `pl` rems).
fn wrap_row(ink: &Ink, id: &str, pl: f32, visible: Value, children: Vec<Value>) -> Value {
    json!({"id": id, "component": "Box", "visible": visible, "style": {"display": "flex", "flexDirection": "row", "flexWrap": "wrap", "alignItems": "center", "width": "100%", "gap": ink.r(0.5), "paddingLeft": ink.r(pl)}, "children": children})
}

/// `controls::ghost_icon_button` (32px, the list hover wash) with a
/// `glyph`-px icon: the live face where `!disabled`, gpui-component's
/// disabled face (the glyph half-muted, no wash) where `disabled`; the
/// live face's `tip` (a literal or a binding) where given.
#[allow(clippy::too_many_arguments)]
fn ghost(ink: &Ink, tints: &Tints, id: &str, name: &str, glyph: f32, label: &str, event: Value, live: Value, tip: Option<Value>, dim: Value, dim_tip: Option<Value>) -> Vec<Value> {
    let face = |id: String, on: bool| {
        let (color, alpha) = if on { (&ink.foreground, VALUE_ALPHA) } else { (&ink.muted, 0.5) };
        let mut glyph_node = icon(&format!("{id}_icon"), json!(name), glyph, color);
        glyph_node["style"]["opacity"] = json!(alpha);
        let mut node = json!({"id": id, "component": "Box", "accessibility": {"label": label},
            "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "width": CTL_MD_H, "height": CTL_MD_H, "borderRadius": ink.radius_md},
            "children": [glyph_node]});
        if on {
            node["props"] = json!({"pressable": true});
            node["on"] = json!({"press": {"event": event.clone()}});
            node["style"]["cursor"] = json!("pointer");
            node["style"][":hover"] = json!({"backgroundColor": tints.list_hover});
        }
        node
    };
    let wrap = |id: &str, node: Value, visible: Value, tip: Option<Value>| match tip {
        Some(tip) => json!({"id": format!("{id}_tip"), "component": "Tooltip", "visible": visible, "props": {"content": tip}, "children": [node]}),
        None => {
            let mut node = node;
            node["visible"] = visible;
            node
        }
    };
    vec![wrap(id, face(id.to_string(), true), live, tip), wrap(&format!("{id}_dim"), face(format!("{id}_dim"), false), dim, dim_tip)]
}

/// The ✕ that unlinks the viewer's GitHub account (`xsmall`: a 12px glyph).
fn disconnect(ink: &Ink, tints: &Tints, id: &str) -> Vec<Value> {
    ghost(ink, tints, id, "ui-close", ink.r(0.75), copy::DISCONNECT, json!({"name": "disconnect"}), not(path("/pane/busy")), Some(json!(copy::DISCONNECT)), path("/pane/busy"), None)
}

/// A glass pill button with a TRAILING 12px glyph (Manage → GitHub).
fn pill_trailing(ink: &Ink, id: &str, label: &str, glyph: &str, event: Value, visible: Value) -> Value {
    let [mut pill, _] = ink.pill_button(id, None, json!(label), json!(false), event);
    let mut node = icon(&format!("{id}_glyph"), json!(glyph), 12., &ink.foreground);
    node["style"]["opacity"] = json!(VALUE_ALPHA);
    pill["children"].as_array_mut().expect("the pill's children").push(node);
    pill["visible"] = visible;
    pill
}

/// The one live face of a pill that never disables.
fn live(face: [Value; 2], visible: Value) -> Value {
    let [mut face, _] = face;
    face["visible"] = visible;
    face
}

/// The GitHub connection block (SLOP-7, web `GithubStatusLine`, ×4): ONE
/// precedence-ordered state (see the projection's `status`).
fn status_block(ink: &Ink, tints: &Tints) -> Vec<Value> {
    let is = |kind: &str| eq(path("/pane/status"), json!(kind));
    let xs = |id: &str, text: Value, color: &str| ink.text(id, text, XS, color);
    let glyph = ink.r(0.75);
    let github = |id: &str| icon(id, json!("ui-github"), glyph, &ink.muted);
    let primary = |id: &str, icon: &str, label: &str, disabled: Value| ink.pill_button_with(id, PillPaint::Primary, Some(json!(icon)), None, json!(label), disabled, None, json!({"name": "connect"})).to_vec();
    let failed = {
        let mut children = vec![github("failed_icon"), grow(xs("failed_text", json!(copy::STATUS_FAILED), &ink.muted))];
        children.push(live(ink.pill_button("status_retry", Some("ui-refresh"), json!(copy::RETRY), json!(false), json!({"name": "status_retry"})), json!(true)));
        wrap_row(ink, "status_failed", 0., is("failed"), children)
    };
    let not_configured = wrap_row(ink, "status_not_configured", 0., is("notConfigured"), vec![github("nc_icon"), xs("nc_text", json!(copy::NOT_CONFIGURED), &ink.muted)]);
    let mut not_linked = vec![github("nl_icon"), grow(xs("nl_text", json!(copy::NOT_LINKED), &ink.muted))];
    not_linked.extend(primary("connect", "ui-github", copy::CONNECT_GITHUB, path("/pane/busy")));
    let not_linked = wrap_row(ink, "status_not_linked", 0., is("notLinked"), not_linked);
    let mut expired = vec![icon("ex_icon", json!("ui-warning"), glyph, &tints.warning), grow(xs("ex_text", json!(copy::RECONNECT_NEEDED), &ink.muted))];
    expired.extend(primary("reconnect", "ui-refresh", copy::RECONNECT, path("/pane/busy")));
    expired.extend(disconnect(ink, tints, "ex_disconnect"));
    let expired = wrap_row(ink, "status_expired", 0., is("expired"), expired);

    // Linked: the live dot + "Connected as" + ✕, the suspended line, then
    // the installed accounts (or the not-installed line).
    let mut connection = vec![
        json!({"id": "dot", "component": "Box", "style": {"width": 8, "height": 8, "borderRadius": 4, "flexShrink": 0, "backgroundColor": tints.live}}),
        grow_line(xs("connected_text", path("/pane/connectedText"), &ink.muted)),
    ];
    connection.extend(disconnect(ink, tints, "linked_disconnect"));
    let connection = json!({"id": "connection", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "gap": ink.r(0.5)}, "children": connection});
    let suspended = wrap_row(ink, "suspended", 1.25, path("/pane/hasSuspended"), vec![
        icon("suspended_icon", json!("ui-warning"), glyph, &tints.danger),
        grow(xs("suspended_text", path("/pane/suspendedText"), &tints.danger)),
        pill_trailing(ink, "suspended_manage", copy::MANAGE, "ui-external-link", json!({"name": "manage_suspended"}), path("/pane/hasSuspendedManage")),
    ]);
    let [install, _] = ink.pill_button_with("install", PillPaint::Primary, Some(json!("ui-add")), None, json!(copy::INSTALL_APP), json!(false), None, json!({"name": "install"}));
    let mut install = install;
    install["visible"] = path("/pane/hasInstallUrl");
    let not_installed = wrap_row(ink, "not_installed", 1.25, path("/pane/notInstalled"), vec![grow(xs("not_installed_text", json!(copy::NOT_INSTALLED), &ink.muted)), install]);
    let padded = |id: &str, text: &str| json!({"id": id, "component": "Box", "visible": path("/pane/hasInstallations"), "style": {"display": "flex", "flexDirection": "column", "paddingLeft": ink.r(1.25)}, "children": [xs(&format!("{id}_text"), json!(text), &ink.muted)]});
    let mut configure_text = ink.text("configure_text", json!(copy::CONFIGURE), XS, &tints.link);
    configure_text["style"]["lineHeight"] = json!(ink.r(XS) * 1.25);
    configure_text["style"]["textDecoration"] = json!("underline");
    configure_text["style"]["flexShrink"] = json!(0);
    // gpui centres the label's line half a point lower than the flex centre.
    configure_text["style"]["paddingTop"] = json!(1);
    let configure = json!({"id": "configure", "component": "Box", "props": {"pressable": true}, "on": {"press": {"event": {"name": "configure", "context": {"key": path("key")}}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "gap": ink.r(0.25), "cursor": "pointer"},
        "children": [configure_text, icon("configure_glyph", json!("ui-external-link"), 12., &tints.link)]});
    let installation = json!({"id": "inst", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "gap": ink.r(0.5)},
        "children": [
            icon("inst_icon", path("icon"), glyph, &ink.muted),
            grow_line(ink.text("inst_label", path("label"), SM, &ink.foreground)),
            {"id": "configure_tip", "component": "Tooltip", "visible": path("hasConfigure"), "props": {"content": path("configureTip")}, "children": [configure]},
        ]});
    let installations = json!({"id": "installations", "component": "List", "visible": path("/pane/hasInstallations"), "props": {"gap": "none"},
        "style": {"gap": ink.r(0.25), "paddingLeft": ink.r(1.25)},
        "template": {"component": "inst_wrap", "path": "/pane/installations", "key": "key"},
        "children": [{"id": "inst_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [installation]}]});
    let another = wrap_row(ink, "another", 1.25, path("/pane/installAnother"), vec![live(ink.pill_button("install_another", Some("ui-add"), json!(copy::INSTALL_ANOTHER), json!(false), json!({"name": "install"})), json!(true))]);
    let linked = json!({"id": "linked", "component": "Box", "visible": is("linked"), "style": {"display": "flex", "flexDirection": "column", "width": "100%", "gap": ink.r(0.5)},
        "children": [connection, suspended, not_installed, padded("accounts", copy::ACCOUNTS_HEADER), installations, padded("caption", copy::INSTALLATION_CAPTION), another]});
    vec![failed, not_configured, not_linked, expired, linked]
}

/// One repo row (web `RepoRow`, `flat_row` `px_3` / `py_2`, `gap_1p5`):
/// the head (glyph, name, branch picker or static chip, Private, remove),
/// the access banner, then the "used by" line.
fn repo_row(ink: &Ink, tints: &Tints) -> Value {
    let mut name = grow_line(ink.text("row_name", path("name"), SM, &ink.foreground));
    name["style"]["fontWeight"] = json!(500);
    // The branch picker (EXP-462, web `DefaultBranchMenu`): a `pill` Select
    // on the effective branch over the repo's branches, fetched live from
    // GitHub on its first open behind a search (EXP-469); picking GitHub's
    // own ("default") clears the pin. The busy pane shows its disabled twin.
    let picker = |id: &str, disabled: bool, visible: Value| {
        json!({"id": id, "component": "Select", "visible": visible,
            "props": {"triggerVariant": "pill", "name": id, "value": path("branch"), "options": path("branchOptions"), "loading": path("branchLoading"),
                "searchable": true, "searchPlaceholder": "Search branches...", "emptyText": "No branches found.", "disabled": disabled},
            "accessibility": {"label": path("branchLabel")},
            "style": {"flexShrink": 0, "maxWidth": 240},
            "on": {"open": {"event": {"name": "branches", "context": {"key": path("key")}}}, "change": {"event": {"name": "branch", "context": {"key": path("key")}}}}})
    };
    let mut chip = ink.pill("row_branch_static", path("branch"), true, None);
    chip["visible"] = not(path("manage"));
    chip["children"][0]["style"]["fontFamily"] = json!("$type.family.mono");
    let mut private = ink.pill("row_private", json!("Private"), false, None);
    private["visible"] = path("private");
    let mut lock = icon("row_private_icon", json!("ui-private"), 12., &ink.foreground);
    lock["style"]["opacity"] = json!(VALUE_ALPHA);
    private["children"].as_array_mut().expect("the pill's children").insert(0, lock);
    let mut head = vec![icon("row_icon", json!("ui-github"), ink.r(0.875), &ink.muted), name, picker("branch", false, path("branchLive")), picker("branch_busy", true, path("branchBusy")), chip, private];
    head.extend(ghost(ink, tints, "row_remove", "ui-delete", ink.r(0.875), "Remove repository", json!({"name": "remove", "context": {"key": path("key")}}), path("removeOn"), None, path("removeLinked"), Some(path("removeTip"))));
    // `row_remove_dim` stands in for both disabled faces: the busy one has
    // no tooltip.
    let mut busy = ghost(ink, tints, "row_remove_busy", "ui-delete", ink.r(0.875), "Remove repository", Value::Null, json!(false), None, path("removeBusy"), None);
    head.push(busy.remove(1));
    let head = json!({"id": "row_head", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "gap": ink.r(0.5)}, "children": head});
    let banner = wrap_row(ink, "row_banner", 1.5, path("banner"), vec![
        icon("row_banner_icon", json!("ui-warning"), ink.r(0.75), &tints.danger),
        grow(ink.text("row_banner_text", path("bannerText"), XS, &tints.danger)),
        live(ink.pill_button("row_regrant", None, path("bannerAction"), json!(false), json!({"name": "regrant", "context": {"key": path("key")}})), path("bannerManage")),
    ]);
    let muted = |id: &str, text: Value, visible: Value| {
        let mut t = ink.text(id, text, XS, &ink.muted);
        t["visible"] = visible;
        t
    };
    let mut board = ink.pill("board", path("name"), false, None);
    let mut glyph = icon("board_icon", path("icon"), 12., "#00000000");
    glyph["style"]["color"] = path("color");
    glyph["visible"] = path("hasIcon");
    board["children"].as_array_mut().expect("the pill's children").insert(0, glyph);
    let boards = json!({"id": "boards", "component": "List", "visible": path("hasBoards"), "props": {"direction": "horizontal", "gap": "none"},
        "style": {"gap": ink.r(0.375), "alignItems": "center"},
        "template": {"component": "board", "path": "boards", "key": "key"},
        "children": [board]});
    let links = json!({"id": "row_links", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "flexWrap": "wrap", "alignItems": "center", "width": "100%", "gap": ink.r(0.375), "paddingLeft": ink.r(1.5)},
        "children": [
            muted("row_unused", json!("Not used by any board"), path("noBoards")),
            muted("row_used_by", json!("Used by"), path("hasBoards")),
            boards,
            muted("row_shared", path("sharedText"), path("hasShared")),
        ]});
    json!({"id": "row", "component": "Box",
        "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.375), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md},
        "children": [head, banner, links]})
}

/// A Local desktop's folder row (`px_2` / `py_1p5`, `gap_2`): the folder
/// glyph, the name over the path, the branch.
fn local_row(ink: &Ink) -> Value {
    let mut branch = ink.text("local_branch", path("branch"), XS, &ink.muted);
    branch["style"]["flexShrink"] = json!(0);
    let mut name = ink.text("local_name", path("name"), SM, &ink.foreground);
    name["props"]["lines"] = json!(1);
    let mut folder = ink.text("local_path", path("path"), XS, &ink.muted);
    folder["props"]["lines"] = json!(1);
    json!({"id": "local_row", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "gap": ink.r(0.5), "paddingHorizontal": ink.r(0.5), "paddingVertical": ink.r(0.375)},
        "children": [
            icon("local_icon", json!("ui-folder"), ink.r(1.), &ink.foreground),
            {"id": "local_text", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0}, "children": [name, folder]},
            branch,
        ]})
}

/// gpui centres a pill button's label line half a point lower than the
/// flex centre: every pill button's (`<id>_text` under a pressable or
/// dimmed capsule) label takes a 1px top inset.
fn nudge_pills(node: &mut Value) {
    let capsule = node["style"]["borderRadius"] == json!(9999) && node["style"]["height"] == json!(24) && node["style"]["justifyContent"] == json!("center");
    if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
        for child in children.iter_mut() {
            if capsule && child["component"] == json!("Text") {
                child["style"]["paddingTop"] = json!(1);
            }
            nudge_pills(child);
        }
    }
}

/// The pane's tree: the Local account's folders, or the band (Add
/// repository) over the skeleton, or the connection block and the failed /
/// empty states or the rows; no team = its empty state alone.
fn document(ink: &Ink, tints: &Tints) -> Value {
    let section = |id: &str, visible: Value, children: Vec<Value>| json!({"id": id, "component": "Box", "visible": visible, "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0}, "children": children});
    let slot = |id: &str, faces: [Value; 2]| {
        let face = live(faces, json!(true));
        json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0}, "children": [face]})
    };
    let local_rows = json!({"id": "local_rows", "component": "List", "visible": path("/pane/localHasRows"), "props": {"gap": "none"}, "style": {"gap": ink.r(0.5)},
        "template": {"component": "local_wrap", "path": "/pane/localRows", "key": "key"},
        "children": [{"id": "local_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [local_row(ink)]}]});
    let local = section("local", path("/pane/local"), vec![
        ink.band("local_band", copy::SECTION_TITLE, Some(slot("link_slot", ink.pill_button("link_folder", Some("ui-folder"), json!(copy::LINK_FOLDER), json!(false), json!({"name": "link_folder"}))))),
        empty_state(ink, "local_empty", "settings-repositories", copy::NO_LOCAL_FOLDERS, path("/pane/localEmpty")),
        local_rows,
    ]);
    let rows = json!({"id": "rows", "component": "List", "visible": path("/pane/hasRows"), "props": {"gap": "none"},
        "template": {"component": "row_wrap", "path": "/pane/rows", "key": "key"},
        "children": [{"id": "row_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [repo_row(ink, tints)]}]});
    let mut list = vec![
        ink.band("band", copy::SECTION_TITLE, Some(slot("add_slot", ink.pill_button("add", Some("ui-github"), json!(copy::ADD_REPOSITORY), json!(false), json!({"name": "add"}))))),
        ink.skeleton_rows("skeleton", &tints.skeleton, path("/pane/loading")),
    ];
    list.extend(status_block(ink, tints));
    list.push(failed_state(ink, "failed", "settings-repositories", copy::NO_REPOSITORIES, path("/pane/failed")));
    list.push(empty_state(ink, "empty", "settings-repositories", copy::NO_REPOSITORIES, path("/pane/empty")));
    list.push(rows);
    let mut root = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [local, section("list", path("/pane/list"), list), empty_state(ink, "no_team", "ui-team", crate::empty_state_copy::NO_TEAM_SELECTED, path("/pane/noTeam"))]});
    nudge_pills(&mut root);
    root
}

/// The TRAILING external-link glyph of a hand-off to GitHub (the add
/// dialog's Configure) — rendered after the label.
pub(super) fn external_glyph() -> Icon {
    Icon::new(registry::UI_EXTERNAL_LINK).with_size(px(crate::surface::PillSize::Sm.glyph()))
}
