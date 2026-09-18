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
    input::{InputEvent, InputState},
    spinner::Spinner,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use crate::controls::{glass_input, search_field, SearchFieldSize, WebControl as _};
use crate::github_connect::{
    copy, fetch_github_repos, is_repo_full_name, lookup_repo, reauth_logins, GithubRepo,
    GithubReposResult,
};
use crate::icons::registry;
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;

use super::repositories::RepositoriesPane;
use super::{error_notice, open_url, row_stroke};

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
    /// The add failed a plan cap — the server's own message, rendered with
    /// the neutral "Upgrade on the web" pointer (§4.9).
    plan_limited: Option<SharedString>,
    /// The add failed the grant-model FORBIDDEN — pair the error with the
    /// OAuth reconnect hand-off.
    grant_reconnect: bool,
    focused_once: bool,
    /// FEED-30: the footer's "Add by name" escape hatch — `owner/name`, looked
    /// up through `integrations.github.lookupRepo` (the connect path's own
    /// checks) and, on a hit, added exactly like a row pick.
    lookup: Entity<InputState>,
    lookup_busy: bool,
    lookup_error: Option<SharedString>,
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
            cx.new(|cx| InputState::new(window, cx).placeholder(copy::SEARCH_PLACEHOLDER));
        let lookup = cx.new(|cx| InputState::new(window, cx).placeholder(copy::LOOKUP_PLACEHOLDER));
        let subscriptions = vec![
            cx.subscribe(&query, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            // Typing clears a previous lookup failure (web parity); Enter
            // looks the name up.
            cx.subscribe_in(&lookup, window, |this, _, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::Change => {
                        this.lookup_error = None;
                        cx.notify();
                    }
                    InputEvent::PressEnter { .. } => this.lookup(window, cx),
                    _ => {}
                }
            }),
            // The connect/install hand-off completes in the browser — coming
            // back to this window refetches as a heuristic fallback; since
            // EXP-368 the definitive signal is the github-connected deep
            // link below.
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.refetch_after_connect(cx);
                }
            }),
            // EXP-368: the `exponential://github-connected` hand-back —
            // unconditional refresh on success (unlike the heuristic above,
            // the deep link is definitive), failure copy otherwise.
            cx.observe_global::<crate::github_connect::GithubConnectSignal>(|this, cx| {
                let Some(outcome) = cx
                    .try_global::<crate::github_connect::GithubConnectSignal>()
                    .and_then(|signal| signal.outcome.clone())
                else {
                    return;
                };
                match outcome {
                    crate::github_connect::GithubConnectOutcome::Connected => {
                        this.error = None;
                        // Spinner only when there's no usable list on screen
                        // (the `show_loading` convention below).
                        let show_loading = !matches!(this.load, Load::Ready(_));
                        this.fetch(true, show_loading, cx);
                    }
                    crate::github_connect::GithubConnectOutcome::Failed(code) => {
                        this.error =
                            Some(crate::github_connect::connect_error_message(&code).into());
                        cx.notify();
                    }
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
            plan_limited: None,
            grant_reconnect: false,
            focused_once: false,
            lookup,
            lookup_busy: false,
            lookup_error: None,
            _subscriptions: subscriptions,
        };
        this.fetch(false, true, cx);
        this
    }

    /// FEED-30: on OAuth instances the list IS the viewer's grant snapshot,
    /// which only the OAuth re-auth (or the installation_repositories webhook)
    /// rewrites — a bare cache refresh can't surface a repo granted since. So
    /// "Refresh" runs the re-auth hop there (the deep link / window activation
    /// re-lists on return) and a plain forced re-list where there is no OAuth.
    fn refresh_access(&mut self, cx: &mut gpui::Context<Self>) {
        let connect_url = match &self.load {
            Load::Ready(result) => result.connect_url.clone(),
            _ => None,
        };
        match connect_url {
            Some(url) => open_url(cx, url),
            None => self.fetch(true, false, cx),
        }
    }

    /// FEED-30: `integrations.github.lookupRepo` for the typed `owner/name`;
    /// a hit is added exactly like a row pick, a miss shows the server's
    /// message inline (it names the real reason — grant it, connect that
    /// account, or reconnect).
    fn lookup(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let full_name = self.lookup.read(cx).value().trim().to_string();
        if self.lookup_busy || !is_repo_full_name(&full_name) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.lookup_error = Some("Not signed in.".into());
            cx.notify();
            return;
        };
        self.lookup_busy = true;
        self.lookup_error = None;
        cx.notify();
        let team_id = self.team_id.clone();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { lookup_repo(&trpc, &team_id, &full_name) })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                this.lookup_busy = false;
                match result {
                    Ok(repo) => {
                        this.lookup
                            .update(cx, |state, cx| state.set_value("", window, cx));
                        this.add(&repo, window, cx);
                    }
                    Err(err) => {
                        this.lookup_error = Some(format!("{err}").into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// FEED-30: the list explains itself. A missing repo is (almost) always an
    /// installation whose repo selection doesn't include it, or a repo on an
    /// account that isn't installed at all — say so, link the exact GitHub
    /// page per account, offer the two fixes, and the by-name escape hatch.
    /// Rendered in EVERY installed state, the empty one included.
    fn footer(
        &self,
        result: &GithubReposResult,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let manage_links: Vec<(String, String)> = result
            .installations
            .iter()
            .filter(|inst| !inst.manage_url.is_empty())
            .map(|inst| (inst.label(), inst.manage_url.clone()))
            .collect();
        let install_url = result.install_url.clone();
        let has_more = result.has_more;
        let loading = matches!(self.load, Load::Loading);
        let lookup_valid = is_repo_full_name(self.lookup.read(cx).value().trim());

        // The sentence, then one Configure link per account (`accountLogin`
        // or "installation {id}") with a trailing external glyph, comma-
        // separated (FEED-42 ×4).
        let mut sentence = h_flex()
            .flex_wrap()
            .items_center()
            .gap_x_1()
            .child(copy::FOOTER_SENTENCE);
        let link_count = manage_links.len();
        for (index, (label, url)) in manage_links.into_iter().enumerate() {
            sentence = sentence.child(
                h_flex()
                    .items_center()
                    .child(
                        Button::new(("add-repo-manage", index))
                            .link()
                            .cursor_pointer()
                            .xsmall()
                            .label(SharedString::from(label))
                            .child(super::repositories::external_glyph())
                            .on_click(move |_, _, cx| open_url(cx, url.clone())),
                    )
                    .when(index + 1 < link_count, |link| link.child(",")),
            );
        }

        let mut lookup_button = pill("add-repo-lookup", cx)
            .label(copy::LOOK_UP)
            .disabled(!lookup_valid || self.lookup_busy)
            .on_click(cx.listener(|this, _, window, cx| this.lookup(window, cx)));
        if self.lookup_busy {
            // A pill has no glyph at rest — the spinner rides the icon slot.
            lookup_button = lookup_button.icon(registry::UI_LOADING).loading(true);
        }

        v_flex()
            .w_full()
            .gap_2()
            .px_3()
            .py_2()
            .rounded(cx.theme().radius)
            .border_1()
            .border_dashed()
            .border_color(row_stroke(cx))
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(sentence)
            .when(has_more, |column| column.child(copy::HAS_MORE))
            .child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        pill("add-repo-refresh", cx)
                            .icon(registry::UI_REFRESH)
                            .label(copy::REFRESH)
                            .disabled(loading)
                            .on_click(cx.listener(|this, _, _, cx| this.refresh_access(cx))),
                    )
                    .children(install_url.map(|url| {
                        pill("add-repo-install-another", cx)
                            .icon(registry::UI_ADD)
                            .label(copy::INSTALL_ON_ANOTHER_ACCOUNT)
                            .on_click(move |_, _, cx| open_url(cx, url.clone()))
                    })),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(glass_input(&self.lookup, window, cx).web_input_sm()),
                    )
                    .child(lookup_button),
            )
            .children(
                self.lookup_error
                    .clone()
                    .map(|message| div().text_color(cx.theme().danger).child(message)),
            )
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
            || result.installations.iter().any(|inst| inst.needs_reconnect());
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
        self.plan_limited = None;
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
                    // The dialog stays open on every failure (FEED-42).
                    if crate::create_board_dialog::is_grant_forbidden(&err) {
                        this.error = Some(copy::ADD_FORBIDDEN.into());
                        this.grant_reconnect = true;
                    } else if super::is_plan_limit(&err) {
                        this.plan_limited = Some(super::form_error(&err, &err.to_string()).into());
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

    fn message(&self, message: &'static str, cx: &gpui::App) -> impl IntoElement {
        div()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(message)
    }

    /// The inline "Reconnect GitHub" pill (re-auth banner + FORBIDDEN add).
    fn reconnect_pill(&self, id: &'static str, cx: &gpui::App) -> Option<impl IntoElement> {
        self.connect_url().map(|url| {
            pill(id, cx)
                .icon(registry::UI_REFRESH)
                .label(copy::RECONNECT_GITHUB)
                .on_click(move |_, _, cx| open_url(cx, url.clone()))
        })
    }

    /// The re-auth banner — shown INDEPENDENTLY of the suspended one (FEED-42),
    /// over an empty list (the load-your-repos variant) or a list that still
    /// has rows (the possibly-incomplete variant). Names the reconnectable
    /// accounts when known (EXP-365); STALE links are excluded (EXP-557).
    fn reconnect_banner(
        &self,
        result: &GithubReposResult,
        cx: &gpui::App,
    ) -> impl IntoElement {
        let logins = reauth_logins(&result.installations);
        let message = if result.repos.is_empty() {
            copy::picker_reauth_empty(&logins)
        } else {
            copy::picker_reauth(&logins)
        };
        banner(cx)
            .text_color(cx.theme().muted_foreground)
            .child(
                Icon::new(registry::UI_WARNING)
                    .xsmall()
                    .flex_shrink_0()
                    .text_color(theme::tokens::YELLOW.to_hsla()),
            )
            .child(div().flex_1().min_w_0().child(SharedString::from(message)))
            .children(self.reconnect_pill("add-repo-reconnect", cx))
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

/// Every inline action of the picker is an `sm action` pill (FEED-42).
fn pill(id: impl Into<gpui::ElementId>, cx: &gpui::App) -> Button {
    crate::surface::glass_pill_button(id, crate::surface::PillSize::Sm, cx)
}

/// The bordered notice row both banners share.
fn banner(cx: &gpui::App) -> gpui::Div {
    h_flex()
        .w_full()
        .flex_wrap()
        .gap_2()
        .items_center()
        .px_3()
        .py_2()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(row_stroke(cx))
        .text_xs()
}

/// A full-state box (loading / not configured / not installed): the glyph,
/// then the copy.
fn state_box(glyph: gpui::AnyElement, message: &'static str, cx: &gpui::App) -> impl IntoElement {
    h_flex()
        .w_full()
        .items_start()
        .gap_2()
        .px_3()
        .py_3()
        .rounded(cx.theme().radius)
        .border_1()
        .border_dashed()
        .border_color(row_stroke(cx))
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(div().flex_shrink_0().pt_0p5().child(glyph))
        .child(div().flex_1().min_w_0().child(message))
}

/// GitHub-side App suspension (REV2-29): the installation lists no repos and
/// mints no tokens until it's unsuspended ON GITHUB. Explanation only — no
/// button (FEED-42 ×4); the Repositories pane carries Manage. An account
/// without a login reads "a connected account".
fn suspended_notice(
    installations: &[crate::github_connect::GithubInstallation],
    cx: &gpui::App,
) -> impl IntoElement {
    let names = installations
        .iter()
        .filter(|inst| inst.suspended)
        .map(|inst| {
            inst.account_login
                .clone()
                .filter(|login| !login.is_empty())
                .unwrap_or_else(|| copy::SUSPENDED_FALLBACK.to_string())
        })
        .collect::<Vec<_>>()
        .join(", ");
    banner(cx)
        .border_color(cx.theme().danger.opacity(0.5))
        .bg(cx.theme().danger.opacity(0.1))
        .text_color(cx.theme().danger)
        .child(Icon::new(registry::UI_GITHUB).xsmall().flex_shrink_0())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(SharedString::from(copy::picker_suspended(&names))),
        )
}

impl Render for AddRepositoryDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            self.query.update(cx, |state, cx| state.focus(window, cx));
        }

        let mut body = v_flex().size_full().min_h_0().gap_3();
        let github_glyph = || Icon::new(registry::UI_GITHUB).small().into_any_element();

        match &self.load {
            Load::Loading => {
                body = body.child(state_box(
                    Spinner::new()
                        .icon(registry::UI_LOADING)
                        .small()
                        .into_any_element(),
                    copy::LOADING_REPOS,
                    cx,
                ));
            }
            Load::Failed(message) => {
                // A fetch that failed with no data reads like not configured
                // (web parity), plus the inline error.
                body = body
                    .child(state_box(github_glyph(), copy::PICKER_NOT_CONFIGURED, cx))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().danger)
                            .child(message.clone()),
                    );
            }
            Load::Ready(result) if !result.configured => {
                body = body.child(state_box(github_glyph(), copy::PICKER_NOT_CONFIGURED, cx));
            }
            Load::Ready(result) if !result.installed => {
                body = body
                    .child(state_box(github_glyph(), copy::PICKER_NOT_INSTALLED, cx))
                    .child(
                        h_flex()
                            .flex_wrap()
                            .gap_2()
                            .items_center()
                            .children(self.connect_url().map(|url| {
                                Button::new("add-repo-connect")
                                    .primary()
                                    .cursor_pointer()
                                    .web_sm()
                                    .icon(registry::UI_GITHUB)
                                    .label(copy::CONNECT_GITHUB)
                                    .on_click(move |_, _, cx| open_url(cx, url.clone()))
                            }))
                            .child(
                                pill("add-repo-connected-refresh", cx)
                                    .icon(registry::UI_REFRESH)
                                    .label(copy::I_HAVE_CONNECTED)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.fetch(true, true, cx)
                                    })),
                            ),
                    );
            }
            Load::Ready(result) => {
                // Installed. The suspended and re-auth banners are
                // INDEPENDENT (FEED-42): a suspended installation needs an
                // UNSUSPEND on GitHub (a reconnect cannot fix it), an
                // uncaptured grant snapshot on another account still needs
                // the OAuth reconnect (EXP-557: STALE links excluded).
                let any_suspended = result.installations.iter().any(|inst| inst.suspended);
                let needs_reauth = result
                    .installations
                    .iter()
                    .any(|inst| inst.needs_reconnect());
                if any_suspended {
                    body = body.child(suspended_notice(&result.installations, cx));
                }
                if needs_reauth {
                    body = body.child(self.reconnect_banner(result, cx));
                }

                if result.repos.is_empty() {
                    if !needs_reauth && !any_suspended {
                        body = body.child(self.message(copy::NO_GRANTS, cx));
                    }
                } else {
                    let filter = self.query.read(cx).value().trim().to_lowercase();
                    let visible: Vec<GithubRepo> = result
                        .repos
                        .iter()
                        .filter(|repo| {
                            filter.is_empty() || repo.full_name.to_lowercase().contains(&filter)
                        })
                        .cloned()
                        .collect();

                    body = body.child(search_field(&self.query, SearchFieldSize::Sm, window, cx));
                    if visible.is_empty() {
                        body = body.child(self.message(copy::NO_REPOS_FOUND, cx));
                    } else {
                        // Tap adds (FEED-42 ×4) — no select step, no check.
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
                // FEED-30: the footer explains every installed state, the
                // empty one included.
                body = body.child(self.footer(result, window, cx));
            }
        }

        // Add errors land INSIDE the still-open picker (FEED-42).
        if let Some(message) = self.plan_limited.clone() {
            body = body.child(
                v_flex()
                    .gap_1()
                    .px_3()
                    .py_2()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().primary.opacity(0.4))
                    .bg(cx.theme().primary.opacity(0.05))
                    .text_sm()
                    .child(message)
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(copy::UPGRADE_ON_THE_WEB),
                    ),
            );
        }
        if let Some(error) = &self.error {
            body = body.child(error_notice(error.clone(), cx));
            if self.grant_reconnect {
                body = body.child(
                    h_flex().children(self.reconnect_pill("add-repo-grant-reconnect", cx)),
                );
            }
        }

        body
    }
}
