//! Settings → Repositories (masterplan-v3 §4.2 + §7.9).
//!
//! Web parity: `components/workspace/repositories-section.tsx`. This pane
//! shows the **live server truth** for GitHub connect state — never a local
//! guess (the old native app falsely said "not connected"):
//!
//! - the GitHub-App install banner off `integrations.github.status`
//!   (`{configured, installed, installUrl, accounts[]}`) — "Installed as
//!   @acme" vs. the install nudge;
//! - the workspace's connected repos + their project links off
//!   `repositories.list` (server-only tables — read via tRPC, never synced).
//!
//! The GitHub-App **install** is a web-only hand-off (§7.9): the buttons
//! open the install/manage URL in the system browser through the robust
//! opener chain. Repo add/link/unlink/set-primary mutations are the §7
//! IDE-track's step on top of this pane; v1 here is the read-only state
//! surface + the browser hand-off.

use gpui::{
    div, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Icon, IconName, Sizable as _,
};
use serde::{Deserialize, Serialize};
use sync::Store;

use crate::navigation::{active_workspace_id, Navigation};
use crate::queries;

use super::{card, card_header, error_notice, open_url};

// ---------------------------------------------------------------------------
// Server-only reads (typed mirrors of the web loader results)
// ---------------------------------------------------------------------------

/// `integrations.github.status` (per-user App install state). Shared with
/// the Account → Integrations pane.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GithubStatus {
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub install_url: Option<String>,
    #[serde(default)]
    pub accounts: Vec<String>,
}

/// `repositories.list` — one connected repo + the projects it backs (v4
/// `projects.repositoryId`; project names now resolved server-side).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RepoRow {
    /// Consumed by the §7.9 repo mutations (remove) when the IDE track lands
    /// them; the read-only pane doesn't render it.
    #[allow(dead_code)]
    pub id: String,
    pub full_name: String,
    #[serde(default)]
    pub default_branch: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub projects: Vec<RepoProjectRef>,
}

/// A project this repo backs (`{ id, name, slug }` from `repositories.list`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RepoProjectRef {
    pub name: String,
}

pub(super) fn fetch_github_status(
    trpc: &api::TrpcClient,
) -> Result<GithubStatus, api::ApiError> {
    trpc.query("integrations.github.status")
}

fn fetch_repositories(
    trpc: &api::TrpcClient,
    workspace_id: &str,
) -> Result<Vec<RepoRow>, api::ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        workspace_id: &'a str,
    }
    trpc.query_with_input("repositories.list", &Input { workspace_id })
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

pub struct RepositoriesPane {
    nav: Entity<Navigation>,
    load: Load,
    /// The workspace the current `load` belongs to; a switch re-fetches.
    loaded_workspace: Option<String>,
    /// The account it was fetched as — a re-login must re-fetch (the GitHub
    /// install state is per-user).
    account_id: Option<String>,
    /// Monotonic guard: a stale in-flight fetch must not clobber a newer one.
    generation: u64,
    _subscriptions: Vec<Subscription>,
}

impl RepositoriesPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        // The GitHub-App install state + repo list (incl. project names) come
        // straight from the server; only navigation (workspace switch) drives a
        // re-render/re-fetch.
        let subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        Self {
            nav,
            load: Load::Idle,
            loaded_workspace: None,
            account_id: None,
            generation: 0,
            _subscriptions: subscriptions,
        }
    }

    /// Kick the server fetch when the pane is first shown or the workspace /
    /// account changed. Runs at render time so a hidden pane never fetches.
    fn ensure_loaded(&mut self, workspace_id: &str, cx: &mut gpui::Context<Self>) {
        let account_id = Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        if account_id != self.account_id {
            self.account_id = account_id;
            self.load = Load::Idle;
        }
        let same_workspace = self.loaded_workspace.as_deref() == Some(workspace_id);
        if same_workspace && !matches!(self.load, Load::Idle) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };

        self.load = Load::Loading;
        self.loaded_workspace = Some(workspace_id.to_string());
        self.generation += 1;
        let generation = self.generation;
        let workspace_id = workspace_id.to_string();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    // Banner is best-effort (web: status errors are ignored);
                    // the repo list carries its own error state.
                    let status = fetch_github_status(&trpc).ok();
                    let repos = fetch_repositories(&trpc, &workspace_id)
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
            });
        })
        .detach();
    }

    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.load = Load::Idle;
        cx.notify();
    }
}

impl Render for RepositoriesPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(workspace_id) = active_workspace_id(&self.nav, cx) else {
            return v_flex().child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No workspace selected."),
            );
        };
        self.ensure_loaded(&workspace_id, cx);

        let repo_count = match &self.load {
            Load::Ready(loaded) => loaded.repos.as_ref().map(|repos| repos.len()).unwrap_or(0),
            _ => 0,
        };

        let mut body = card(cx).child(card_header(
            format!("Repositories · {repo_count}"),
            "Connect GitHub repos so issues in this workspace can be coded on. Link a repo \
             to a project to make it the clone target for \u{201c}Start coding\u{201d}.",
            cx,
        ));

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
                // GitHub-App banner: live server truth.
                match &loaded.status {
                    Some(status) if !status.configured => {
                        body = body.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    "GitHub is not configured on this server. Set GITHUB_APP_ID \
                                     and GITHUB_APP_PRIVATE_KEY to enable it.",
                                ),
                        );
                    }
                    Some(status) if status.installed => {
                        let label: SharedString = if status.accounts.is_empty() {
                            "GitHub App installed".into()
                        } else {
                            format!("GitHub App installed as {}", status.accounts.join(", "))
                                .into()
                        };
                        let mut banner = h_flex()
                            .gap_1p5()
                            .items_center()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(div().size_2().rounded_full().bg(theme::tokens::GREEN.to_hsla()))
                            .child(Icon::new(IconName::Github).xsmall())
                            .child(label);
                        if let Some(url) = status.install_url.clone() {
                            banner = banner.child(
                                Button::new("gh-manage")
                                    .link()
                                    .xsmall()
                                    .label("Manage on GitHub")
                                    .icon(IconName::ExternalLink)
                                    .on_click(move |_, _, cx| open_url(cx, url.clone())),
                            );
                        }
                        body = body.child(banner);
                    }
                    Some(status) => {
                        // Configured but not installed → the install nudge.
                        // Install is a WEB-ONLY hand-off (§7.9): open the
                        // browser, never carry the flow in-app.
                        let mut banner = h_flex()
                            .flex_wrap()
                            .gap_2()
                            .items_center()
                            .px_3()
                            .py_2()
                            .rounded(cx.theme().radius)
                            .border_1()
                            .border_dashed()
                            .border_color(cx.theme().border)
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(Icon::new(IconName::Github).xsmall())
                            .child(
                                div().flex_1().min_w_0().child(
                                    "The Exponential GitHub App isn't installed for your \
                                     account yet. Install it on the web to connect \
                                     repositories here.",
                                ),
                            );
                        if let Some(url) = status.install_url.clone() {
                            banner = banner.child(
                                Button::new("gh-install")
                                    .outline()
                                    .xsmall()
                                    .label("Install GitHub App")
                                    .icon(IconName::ExternalLink)
                                    .on_click(move |_, _, cx| open_url(cx, url.clone())),
                            );
                        }
                        body = body.child(banner);
                    }
                    None => {
                        // Status probe failed — banner is best-effort; say
                        // nothing definite rather than a false "not
                        // connected".
                        body = body.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Couldn't reach GitHub install state — refresh to retry."),
                        );
                    }
                }

                match &loaded.repos {
                    Err(message) => {
                        body = body.child(error_notice(
                            SharedString::from(message.clone()),
                            cx,
                        ));
                    }
                    Ok(repos) if repos.is_empty() => {
                        body = body.child(
                            div()
                                .px_3()
                                .py_2()
                                .rounded(cx.theme().radius)
                                .border_1()
                                .border_color(cx.theme().border)
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("No repositories connected yet."),
                        );
                    }
                    Ok(repos) => {
                        let mut list = v_flex().gap_2();
                        for repo in repos {
                            list = list.child(render_repo_row(repo, cx));
                        }
                        body = body.child(list);
                    }
                }
            }
        }

        body = body.child(
            h_flex().gap_2().child(
                Button::new("repos-refresh")
                    .ghost()
                    .xsmall()
                    .label("Refresh")
                    .loading(matches!(self.load, Load::Loading))
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            ),
        );

        v_flex().child(body)
    }
}

/// Web `RepoRow` (read-only v1): name + branch/private chips, then the
/// "used by" line — one chip per project the repo backs (v4 one-repo-per-
/// project; names resolved server-side).
fn render_repo_row(repo: &RepoRow, cx: &gpui::App) -> impl IntoElement {
    let mut head = h_flex()
        .gap_2()
        .items_center()
        .child(
            Icon::new(IconName::Github)
                .small()
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
        )
        .child(chip(SharedString::from(repo.default_branch.clone()), cx));
    if repo.private {
        head = head.child(chip("Private".into(), cx));
    }

    let mut links = h_flex().flex_wrap().gap_1p5().pl_6().items_center();
    if repo.projects.is_empty() {
        links = links.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("No projects linked"),
        );
    } else {
        for project in &repo.projects {
            links = links.child(
                h_flex()
                    .gap_1()
                    .px_1p5()
                    .py_0p5()
                    .items_center()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().border)
                    .text_xs()
                    .child(SharedString::from(project.name.clone())),
            );
        }
    }

    v_flex()
        .gap_1p5()
        .px_3()
        .py_2()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().border)
        .child(head)
        .child(links)
}

/// Outline chip (web `Badge variant="outline"` at compact density).
fn chip(label: SharedString, cx: &gpui::App) -> impl IntoElement {
    div()
        .px_1p5()
        .py_0p5()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().border)
        .text_xs()
        .font_family(theme::terminal::FONT_FAMILY)
        .text_color(cx.theme().muted_foreground)
        .flex_shrink_0()
        .child(label)
}
