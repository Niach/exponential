//! Settings → Repositories → "Add repository" (EXP-329).
//!
//! Web parity: the `GithubRepoPicker` half of
//! `components/team/repositories-section.tsx`. Until EXP-329 the desktop's
//! Repositories pane was READ-ONLY — connecting a repo was only possible as a
//! side effect of creating a board — so this dialog is the desktop's direct
//! connect surface: pick one of the repos the signed-in user proved access to
//! at OAuth-connect time, and `repositories.add` registers it for the team.
//!
//! Everything here reads the live server truth through
//! [`crate::github_connect::fetch_github_repos`]; the App connect/install
//! itself stays a browser hand-off. The one signal that a hand-off finished is
//! this window regaining focus, so an activation while the list is unusable
//! (not installed / grantless / empty) re-fetches with `refresh` — the server
//! caches the repo list per team, and a just-completed install would otherwise
//! stay invisible for a minute.

use gpui::{
    div, prelude::FluentBuilder as _, px, size, App, AppContext as _, Entity,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    spinner::Spinner,
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};

use crate::github_connect::{fetch_github_repos, GithubRepo, GithubReposResult};
use crate::icons::registry;
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;

use super::repositories::RepositoriesPane;
use super::{error_notice, open_url, row_stroke, upgrade_notice};

enum Load {
    Loading,
    Ready(GithubReposResult),
    Failed(SharedString),
}

/// Open the dialog over the Repositories pane. `pane` is refreshed after a
/// successful add — `repositories.list` is a server read with no Electric
/// echo, so the new row only appears on a refetch.
pub(super) fn open(
    window: &mut Window,
    cx: &mut App,
    team_id: String,
    pane: WeakEntity<RepositoriesPane>,
) {
    // Same width as the other settings dialogs; the list owns the height, so
    // cap against the opener's viewport and let it scroll inside.
    let height = (window.viewport_size().height * 0.85).min(px(480.));
    let spec = DialogSpec::new("Add repository", size(px(416.), height));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| AddRepositoryDialogView::new(team_id, pane, window, cx));
        let busy = view.clone();
        DialogContent::new(view)
            // The search input stays pinned while only the list scrolls.
            .self_scrolling()
            .can_close(move |cx| !busy.read(cx).adding)
    });
}

pub struct AddRepositoryDialogView {
    team_id: String,
    /// The pane to refetch once a repo is connected.
    pane: WeakEntity<RepositoriesPane>,
    query: Entity<InputState>,
    load: Load,
    /// Monotonic guard: a stale in-flight fetch must not clobber a newer one.
    generation: u64,
    adding: bool,
    error: Option<SharedString>,
    /// The add failed a plan cap — render the neutral upgrade notice (§4.9).
    plan_limited: bool,
    /// The add failed the grant-model FORBIDDEN — pair the error with the
    /// OAuth reconnect hand-off.
    grant_reconnect: bool,
    focused_once: bool,
    _subscriptions: Vec<Subscription>,
}

impl AddRepositoryDialogView {
    fn new(
        team_id: String,
        pane: WeakEntity<RepositoriesPane>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let query =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search repositories\u{2026}"));
        let subscriptions = vec![
            cx.subscribe(&query, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            // The connect/install hand-off completes in the browser — coming
            // back to this window is the only signal it happened.
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.refetch_after_connect(cx);
                }
            }),
        ];
        let mut this = Self {
            team_id,
            pane,
            query,
            load: Load::Loading,
            generation: 0,
            adding: false,
            error: None,
            plan_limited: false,
            grant_reconnect: false,
            focused_once: false,
            _subscriptions: subscriptions,
        };
        this.fetch(false, true, cx);
        this
    }

    /// Re-detect after a browser hand-off: only when the current list is
    /// unusable, and always with `refresh` so the server's per-team repo cache
    /// can't hide a just-completed install.
    fn refetch_after_connect(&mut self, cx: &mut gpui::Context<Self>) {
        let Load::Ready(result) = &self.load else {
            return;
        };
        let repos_empty = result.repos.is_empty();
        let stale = !result.installed
            || repos_empty
            || result.installations.iter().any(|inst| inst.needs_reauth);
        if stale {
            self.fetch(true, repos_empty, cx);
        }
    }

    /// `show_loading` keeps a usable list on screen while a background
    /// re-detect runs (the reconnect-with-repos case) instead of flashing the
    /// spinner over it.
    fn fetch(&mut self, refresh: bool, show_loading: bool, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            self.load = Load::Failed("Not signed in.".into());
            cx.notify();
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        if show_loading {
            self.load = Load::Loading;
        }
        cx.notify();
        let team_id = self.team_id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    fetch_github_repos(&trpc, &team_id, refresh).map_err(|err| err.to_string())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer fetch
                }
                this.load = match result {
                    Ok(result) => Load::Ready(result),
                    Err(message) => Load::Failed(message.into()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn add(&mut self, repo: &GithubRepo, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.adding {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };
        self.adding = true;
        self.error = None;
        self.plan_limited = false;
        self.grant_reconnect = false;
        cx.notify();

        let team_id = self.team_id.clone();
        let pane = self.pane.clone();
        let full_name = repo.full_name.clone();
        let default_branch =
            (!repo.default_branch.is_empty()).then(|| repo.default_branch.clone());
        let private = repo.private;

        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    api::repositories::add(
                        &trpc,
                        &team_id,
                        &full_name,
                        default_branch.as_deref(),
                        Some(private),
                    )
                })
                .await;
            let _ = this.update_in(window, |this, window, cx| match result {
                Ok(_) => {
                    // `repositories.list` is a server read — the pane only
                    // shows the new row after a refetch.
                    native_dialog::close_then(window, cx, move |_, cx| {
                        let _ = pane.update(cx, |pane, cx| pane.refresh(cx));
                    });
                }
                Err(err) => {
                    this.adding = false;
                    // Grant-model FORBIDDEN first: `is_plan_limit` matches any
                    // 412/403-shaped cap, and a stale GitHub grant must not be
                    // misread as an upsell (create_board_dialog precedent).
                    if crate::create_board_dialog::is_grant_forbidden(&err) {
                        this.error = Some(
                            "GitHub says you don't have access to this repository, or your \
                             connection is stale — reconnect GitHub and try again."
                                .into(),
                        );
                        this.grant_reconnect = true;
                    } else if super::is_plan_limit(&err) {
                        this.plan_limited = true;
                    } else {
                        this.error = Some(format!("{err}").into());
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The OAuth connect target — it re-captures the per-user repo grants; the
    /// App install page does NOT (web parity: `github-repo-picker.tsx`).
    fn connect_url(&self) -> Option<String> {
        match &self.load {
            Load::Ready(result) => result
                .connect_url
                .clone()
                .or_else(|| result.install_url.clone()),
            _ => None,
        }
    }

    fn connect_button(&self, id: &'static str, label: &'static str) -> Option<impl IntoElement> {
        self.connect_url().map(|url| {
            Button::new(id)
                .outline()
                .small()
                .icon(registry::UI_GITHUB)
                .label(label)
                .on_click(move |_, _, cx| open_url(cx, url.clone()))
        })
    }

    fn message(&self, message: &'static str, cx: &gpui::App) -> impl IntoElement {
        div()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(message)
    }

    /// The amber "your grant snapshot is stale" banner above a list that still
    /// has rows — the repos are pickable, they're just possibly incomplete.
    fn reconnect_banner(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_wrap()
            .gap_2()
            .items_center()
            .px_3()
            .py_2()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(row_stroke(cx))
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(
                Icon::new(registry::UI_WARNING)
                    .xsmall()
                    .text_color(theme::tokens::YELLOW.to_hsla()),
            )
            .child(div().flex_1().min_w_0().child(
                "Reconnect GitHub to refresh — repos created or shared with you since your \
                 last connect won't appear until you do.",
            ))
            .children(self.connect_button("add-repo-reconnect", "Reconnect GitHub"))
    }

    /// One pickable repo row. The name ellipsizes, so the whole definite-width
    /// chain has to hold: `w_full` row → `flex_1 min_w_0` on the truncating
    /// div itself.
    fn render_repo_row(
        &self,
        index: usize,
        repo: &GithubRepo,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let repo_for_click = repo.clone();
        h_flex()
            .id(("add-repo-row", index))
            .w_full()
            .items_center()
            .gap_2()
            .px_2()
            .py_1p5()
            .rounded(cx.theme().radius)
            .cursor_pointer()
            .hover(|this| this.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
            .child(
                Icon::new(registry::UI_GITHUB)
                    .xsmall()
                    .flex_shrink_0()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::from(repo.full_name.clone())),
            )
            .when(repo.private, |row| {
                row.child(
                    Icon::new(registry::UI_PRIVATE)
                        .xsmall()
                        .flex_shrink_0()
                        .text_color(cx.theme().muted_foreground),
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.add(&repo_for_click, window, cx);
            }))
    }
}

impl Render for AddRepositoryDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            self.query.update(cx, |state, cx| state.focus(window, cx));
        }

        let mut body = v_flex().size_full().min_h_0().gap_3();

        match &self.load {
            Load::Loading => {
                body = body.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(Spinner::new().icon(registry::UI_LOADING).xsmall())
                        .child("Loading your GitHub repositories\u{2026}"),
                );
            }
            Load::Failed(message) => {
                body = body.child(error_notice(message.clone(), cx));
            }
            Load::Ready(result) if !result.configured => {
                body = body.child(self.message(
                    "GitHub isn't configured on this server, so repositories can't be connected.",
                    cx,
                ));
            }
            Load::Ready(result) if !result.installed => {
                body = body
                    .child(self.message(
                        "Connect the Exponential GitHub App to pick a repository. You'll come \
                         right back here.",
                        cx,
                    ))
                    .child(
                        h_flex()
                            .flex_wrap()
                            .gap_2()
                            .items_center()
                            .children(self.connect_button("add-repo-connect", "Connect GitHub"))
                            .child(
                                Button::new("add-repo-connected-refresh")
                                    .ghost()
                                    .small()
                                    .label("I've connected — refresh")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.fetch(true, true, cx)
                                    })),
                            ),
                    );
            }
            Load::Ready(result) if result.repos.is_empty() => {
                // Installed but the per-user grant snapshot is missing/stale —
                // the OAuth connect is what re-captures it.
                body = body
                    .child(
                        self.message("Reconnect GitHub to load the repositories you can access.", cx),
                    )
                    .child(h_flex().children(
                        self.connect_button("add-repo-reconnect-empty", "Reconnect GitHub"),
                    ));
            }
            Load::Ready(result) => {
                if result.installations.iter().any(|inst| inst.needs_reauth) {
                    body = body.child(self.reconnect_banner(cx));
                }
                let filter = self.query.read(cx).value().trim().to_lowercase();
                let visible: Vec<GithubRepo> = result
                    .repos
                    .iter()
                    .filter(|repo| {
                        filter.is_empty() || repo.full_name.to_lowercase().contains(&filter)
                    })
                    .cloned()
                    .collect();

                body = body.child(Input::new(&self.query).small().cleanable(true));
                if visible.is_empty() {
                    body = body.child(self.message("No repositories found.", cx));
                } else {
                    let mut list = v_flex()
                        .id("add-repo-list")
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .overflow_y_scroll();
                    for (index, repo) in visible.iter().enumerate() {
                        list = list.child(self.render_repo_row(index, repo, cx));
                    }
                    body = body.child(list);
                }
            }
        }

        if self.plan_limited {
            body = body.child(upgrade_notice(
                "Repository limit reached — upgrade on the web to add more.".into(),
                cx,
            ));
        }
        if let Some(error) = &self.error {
            body = body.child(error_notice(error.clone(), cx));
            if self.grant_reconnect {
                body = body.child(h_flex().children(
                    self.connect_button("add-repo-grant-reconnect", "Reconnect GitHub"),
                ));
            }
        }

        body
    }
}
