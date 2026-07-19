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
//! (the App's install OAuth flow can't run in-process). Inline repo *connect*
//! now happens in the create-board dialog once the App is installed — the
//! shared status/repo fetches live in [`crate::github_connect`]. This pane is
//! the read-only connected-repo surface + the install/manage hand-off.

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

use crate::github_connect::{fetch_github_status, GithubStatus};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;
use crate::repo_resolver::links_snapshot;

use super::{card, card_header, error_notice, open_url};

// ---------------------------------------------------------------------------
// Server-only reads (typed mirrors of the web loader results)
// ---------------------------------------------------------------------------

/// `repositories.list` — one connected repo + the boards it backs (v4
/// `boards.repositoryId`; board names now resolved server-side).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RepoRow {
    /// Consumed by the Boards pane's repository picker
    /// (`boards.setRepository`); this read-only pane doesn't render it.
    pub id: String,
    pub full_name: String,
    #[serde(default)]
    pub default_branch: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub boards: Vec<RepoBoardRef>,
}

/// A board this repo backs (`{ id, name, slug }` from `repositories.list`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RepoBoardRef {
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
    _subscriptions: Vec<Subscription>,
}

impl RepositoriesPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
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
        ];
        Self {
            nav,
            load: Load::Idle,
            loaded_team: None,
            account_id: None,
            loaded_links: None,
            generation: 0,
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
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };

        self.load = Load::Loading;
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

    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.load = Load::Idle;
        cx.notify();
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
            Load::Ready(loaded) => loaded.repos.as_ref().map(|repos| repos.len()).unwrap_or(0),
            _ => 0,
        };

        let mut body = card(cx).child(card_header(
            format!("Repositories · {repo_count}"),
            "Connect GitHub repos so issues in this team can be coded on. Link a repo \
             to a board to make it the clone target for \u{201c}Start coding\u{201d}.",
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
                        // Grant-model reconnect (web parity:
                        // `repositories-section.tsx`): a linked installation
                        // whose per-user repo grants were never captured
                        // (`needs_reauth`) lists no repos until the user
                        // re-runs the OAuth connect — `connect_url`, NOT the
                        // install page (installing does not re-capture
                        // grants).
                        if status.installations.iter().any(|inst| inst.needs_reauth) {
                            let mut notice = h_flex()
                                .flex_wrap()
                                .gap_2()
                                .items_center()
                                .px_3()
                                .py_2()
                                .rounded(cx.theme().radius)
                                .border_1()
                                .border_color(cx.theme().border)
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    Icon::new(IconName::TriangleAlert)
                                        .xsmall()
                                        .text_color(theme::tokens::YELLOW.to_hsla()),
                                )
                                .child(div().flex_1().min_w_0().child(
                                    "Reconnect GitHub to refresh which repositories you can \
                                     access — repos created or shared with you since your \
                                     last connect won't appear until you do.",
                                ));
                            let reconnect_url = status
                                .connect_url
                                .clone()
                                .or_else(|| status.install_url.clone());
                            if let Some(url) = reconnect_url {
                                notice = notice.child(
                                    Button::new("gh-reconnect")
                                        .outline()
                                        .xsmall()
                                        .label("Reconnect GitHub")
                                        .icon(IconName::Github)
                                        .on_click(move |_, _, cx| open_url(cx, url.clone())),
                                );
                            }
                            body = body.child(notice);
                        }
                    }
                    Some(status) => {
                        // Configured but not installed → the install nudge.
                        // Install is a browser hand-off: open the install URL,
                        // never carry the App's OAuth flow in-app.
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
                                    "The Exponential GitHub App isn't connected for your \
                                     account yet — connect it here to add repositories.",
                                ),
                            );
                        // Connect claims the account for the team: prefer
                        // the single-consent connect URL, fall back to the App
                        // install page.
                        let connect_url = status
                            .connect_url
                            .clone()
                            .or_else(|| status.install_url.clone());
                        if let Some(url) = connect_url {
                            banner = banner.child(
                                Button::new("gh-install")
                                    .outline()
                                    .xsmall()
                                    .label("Connect GitHub")
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
/// "used by" line — one chip per board the repo backs (v4 one-repo-per-
/// board; names resolved server-side).
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
    if repo.boards.is_empty() {
        links = links.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("No boards linked"),
        );
    } else {
        for board in &repo.boards {
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
                    .child(SharedString::from(board.name.clone())),
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
