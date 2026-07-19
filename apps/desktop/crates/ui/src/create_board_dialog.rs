//! Create-board dialog (masterplan-v3 §4.2 — mirror of
//! `apps/web/src/components/create-board-dialog.tsx`).
//!
//! A plain name/prefix/icon/color/optional-repo form: name `Input` + an
//! **auto-derived-but-editable prefix** `Input` (`derivePrefix`, uppercased,
//! max 10) + an icon picker over the curated contract glyphs + the
//! `ColorSwatchGrid` — **no slug field** (server-derived) + the **optional
//! backing repository** picker (nullable `repository_id`). The repo picker
//! mirrors the web `GithubRepoPicker`: it offers the team's
//! already-connected registry repos AND, once the GitHub App is
//! installed, the user's installable GitHub repos to connect inline in the
//! same `boards.create` call; when the App is configured but not installed a
//! "Connect GitHub" button opens the browser install and an explicit Refresh
//! re-detects. Submit → `boards.create` (then a fire-and-forget
//! `onboarding.complete`); the close is gated on the new board appearing in
//! the synced collection (§4.1 create flows), so the sidebar row is there the
//! moment the dialog is gone. A plan-cap FORBIDDEN surfaces as the neutral
//! "Upgrade on the web" notification (§4.9) — never an in-app purchase UI;
//! the grant-model FORBIDDEN (403 + the server's "reconnect GitHub" hint) is
//! detected first and surfaces a reconnect prompt instead.
//!
//! Opened by the sidebar's Boards `+` via the [`NewBoard`]
//! action; [`init`] owns the handler.

use gpui::{
    div, px, App, AppContext as _, Entity, InteractiveElement as _, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, WindowExt as _,
};
use serde::{Deserialize, Serialize};
use sync::Store;

use crate::actions::NewBoard;
use crate::create_issue_dialog::parse_hex_color;
use crate::github_connect::{fetch_github_repos, GithubRepo, GithubReposResult};
use crate::navigation::{active_team_id, nav_for_window};
use crate::queries;
use crate::settings::open_url;

/// Web `LABEL_COLORS` (`lib/label-colors.ts`) — the swatch palette shared by
/// board + label colors (fixed hex literals on web too).
pub(crate) const SWATCH_COLORS: [&str; 20] = [
    "#ef4444", "#dc2626", "#f97316", "#f59e0b", "#eab308", "#84cc16", "#22c55e", "#10b981",
    "#14b8a6", "#06b6d4", "#0ea5e9", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7", "#ec4899",
    "#f43f5e", "#78716c", "#64748b", "#a3a3a3",
];

/// Web default board color (`create-board-dialog.tsx`).
const DEFAULT_COLOR: &str = "#6366f1";

/// Default curated icon for a new board (the plain kanban board glyph).
const DEFAULT_ICON: &str = "square-kanban";

/// A registry repo the new board can target (v4 §3.1 — every board is
/// backed by exactly one repository). Slim mirror of a `repositories.list`
/// row (`apps/web/src/lib/trpc/repositories.ts`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoOption {
    id: String,
    full_name: String,
}

/// Server fetch state for the registry repo picker.
enum RepoLoad {
    Loading,
    /// The connected-repo list (possibly empty → fall through to the inline
    /// GitHub picker).
    Ready(Vec<RepoOption>),
    Failed(SharedString),
}

/// Server fetch state for the inline GitHub-App repo picker
/// (`integrations.github.repos`).
enum GithubLoad {
    Loading,
    Ready(GithubReposResult),
    Failed(SharedString),
}

/// The chosen backing repository — either an already-connected registry repo
/// (`{repositoryId}`) or a GitHub-App repo connected inline (`{fullName, …}`).
/// Both carry `full_name` so the trigger renders without a lookup.
#[derive(Clone)]
enum RepoChoice {
    Registry { id: String, full_name: String },
    Inline(GithubRepo),
}

impl RepoChoice {
    fn full_name(&self) -> &str {
        match self {
            RepoChoice::Registry { full_name, .. } => full_name,
            RepoChoice::Inline(repo) => &repo.full_name,
        }
    }

    /// The `boards.create` repository union arm this choice submits.
    fn to_input(&self) -> api::boards::BoardRepositoryInput {
        match self {
            RepoChoice::Registry { id, .. } => api::boards::BoardRepositoryInput::Registry {
                repository_id: id.clone(),
            },
            RepoChoice::Inline(repo) => api::boards::BoardRepositoryInput::Inline {
                full_name: repo.full_name.clone(),
                default_branch: (!repo.default_branch.is_empty())
                    .then(|| repo.default_branch.clone()),
                private: Some(repo.private),
            },
        }
    }
}

/// `repositories.list({teamId})` — the team's already-connected repos.
fn fetch_repositories(
    trpc: &api::TrpcClient,
    team_id: &str,
) -> Result<Vec<RepoOption>, api::ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.query_with_input("repositories.list", &Input { team_id })
}

/// Register the App-global [`NewBoard`] handler (call once from `ui::init`).
pub fn init(cx: &mut App) {
    cx.on_action(|_: &NewBoard, cx| {
        crate::navigation::on_active_window(cx, |window, cx| {
            let nav = nav_for_window(window, cx);
            let Some(team_id) = active_team_id(&nav, cx) else {
                return;
            };
            open(window, cx, team_id);
        });
    });
}

/// Open the dialog for a team.
pub fn open(window: &mut Window, cx: &mut App, team_id: String) {
    let view = cx.new(|cx| CreateBoardDialogView::new(team_id, window, cx));
    window.open_dialog(cx, move |dialog, _window, cx| {
        let busy = view.read(cx).submitting;
        dialog
            .w(px(416.)) // web sm:max-w-[26rem]
            .title("Create board")
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

pub struct CreateBoardDialogView {
    team_id: String,
    name: Entity<InputState>,
    prefix: Entity<InputState>,
    /// Curated icon name (`domain::contract::BOARD_ICON_VALUES`) chosen in
    /// the icon picker. Sent as `icon`.
    icon: &'static str,
    color: String,
    /// The chosen backing repository (v4 §3.1 — required to submit).
    repo_choice: Option<RepoChoice>,
    /// Connected-repo list for the picker, fetched from `repositories.list`.
    repos: RepoLoad,
    /// Installable GitHub-App repos, fetched from `integrations.github.repos`.
    github: GithubLoad,
    /// Monotonic guard so a slow refetch can't clobber a newer Refresh.
    fetch_generation: u64,
    submitting: bool,
    error: Option<SharedString>,
    /// The last submit failed with the grant-model FORBIDDEN (stale/missing
    /// GitHub grant for the picked repo) — pair the error with a "Reconnect
    /// GitHub" hand-off.
    grant_reconnect: bool,
    focused_once: bool,
    _subscriptions: Vec<Subscription>,
}

impl CreateBoardDialogView {
    fn new(team_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Backend API"));
        let prefix = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. API"));

        let mut subscriptions = Vec::new();
        // Web `handleNameChange`: every name edit re-derives the prefix.
        subscriptions.push(cx.subscribe_in(
            &name,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let derived = derive_prefix(&this.name.read(cx).value());
                    this.prefix
                        .update(cx, |state, cx| state.set_value(derived, window, cx));
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                _ => {}
            },
        ));
        // Web prefix input: uppercased, maxLength 10.
        subscriptions.push(cx.subscribe_in(
            &prefix,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let value = this.prefix.read(cx).value().to_string();
                    let normalized: String = value.to_uppercase().chars().take(10).collect();
                    if normalized != value {
                        this.prefix
                            .update(cx, |state, cx| state.set_value(normalized, window, cx));
                    }
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                _ => {}
            },
        ));

        let mut this = Self {
            team_id,
            name,
            prefix,
            icon: DEFAULT_ICON,
            color: DEFAULT_COLOR.to_string(),
            repo_choice: None,
            repos: RepoLoad::Loading,
            github: GithubLoad::Loading,
            fetch_generation: 0,
            submitting: false,
            error: None,
            grant_reconnect: false,
            focused_once: false,
            _subscriptions: subscriptions,
        };
        // Load the team's connected repos AND the installable GitHub-App
        // repos so the picker can offer both an existing registry repo and an
        // inline connect (a backing repo is required by the server, v4 §3.1).
        this.spawn_fetches(false, cx);
        this
    }

    /// (Re)fetch both the registry repos and the installable GitHub-App repos.
    /// `refresh` forces the server past its per-user repo cache — used by the
    /// explicit Refresh after the user connects the App in the browser.
    fn spawn_fetches(&mut self, refresh: bool, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.fetch_generation += 1;
        let generation = self.fetch_generation;
        self.repos = RepoLoad::Loading;
        self.github = GithubLoad::Loading;
        let team_id = self.team_id.clone();
        cx.spawn(async move |this, cx| {
            let (registry, github) = cx
                .background_executor()
                .spawn(async move {
                    let registry = fetch_repositories(&trpc, &team_id)
                        .map_err(|err| err.to_string());
                    let github = fetch_github_repos(&trpc, &team_id, refresh)
                        .map_err(|err| err.to_string());
                    (registry, github)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.fetch_generation != generation {
                    return; // superseded by a newer fetch
                }
                this.repos = match registry {
                    Ok(repos) => RepoLoad::Ready(repos),
                    Err(message) => RepoLoad::Failed(message.into()),
                };
                this.github = match github {
                    Ok(result) => GithubLoad::Ready(result),
                    Err(message) => GithubLoad::Failed(message.into()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let name = self.name.read(cx).value().trim().to_string();
        let prefix = self.prefix.read(cx).value().trim().to_string();
        if name.is_empty() || prefix.is_empty() || self.submitting {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };

        self.error = None;
        self.grant_reconnect = false;
        self.submitting = true;
        cx.notify();

        // A repository is optional on every board — send whatever was
        // picked, or nothing.
        let repository = self.repo_choice.as_ref().map(RepoChoice::to_input);
        let input = api::boards::BoardsCreateInput {
            team_id: self.team_id.clone(),
            name,
            prefix,
            icon: Some(self.icon.to_string()),
            color: Some(self.color.clone()),
            repository,
        };

        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    let created = api::boards::boards_create(&trpc, &input);
                    // First board completes onboarding (web fires this after
                    // the onboarding create). Fire-and-forget: a repeat call on
                    // an already-onboarded user just no-ops, and a failure here
                    // must never block the create.
                    if created.is_ok() {
                        let _ = trpc.mutation_no_input::<serde_json::Value>(
                            "onboarding.complete",
                        );
                    }
                    created
                })
                .await;

            match result {
                Ok(output) => {
                    // Gate the close on the Electric echo (§4.1) so the
                    // sidebar shows the board the moment the dialog closes,
                    // then open its (empty) board.
                    let board_id = output.board.id.clone();
                    let boards = window
                        .update(|_, cx| Store::global(cx).collections().boards.clone())
                        .ok();
                    if let Some(boards) = boards {
                        queries::await_row_visible(&boards, &board_id, window).await;
                    }
                    let _ = this.update_in(window, |_, window, cx| {
                        window.close_dialog(cx);
                        // Scope the window to the new board and surface its
                        // (empty) issue list in the sidebar.
                        crate::navigation::set_active_board(window, cx, board_id);
                        crate::sidebar::activate_tool(
                            window,
                            cx,
                            crate::sidebar::ToolWindow::AllIssues,
                        );
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, window, cx| {
                        // Grant-model FORBIDDEN must be checked BEFORE the
                        // plan-limit fallback — `is_plan_limit` matches any
                        // 403, which would misread a stale GitHub grant as a
                        // plan cap and tell the user to upgrade.
                        if is_grant_forbidden(&err) {
                            this.error = Some(
                                "GitHub says you don't have access to this repository, or \
                                 your connection is stale — reconnect GitHub and try again."
                                    .into(),
                            );
                            this.grant_reconnect = true;
                            this.submitting = false;
                            cx.notify();
                            return;
                        }
                        if is_plan_limit(&err) {
                            // §4.9: neutral hand-off, never an upgrade dialog.
                            window.close_dialog(cx);
                            window.push_notification(
                                Notification::warning(
                                    "Board limit reached — upgrade on the web to create more.",
                                ),
                                cx,
                            );
                            return;
                        }
                        this.error = Some(format!("{err}").into());
                        this.submitting = false;
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }
}

impl CreateBoardDialogView {
    /// The icon picker: a wrapping grid of the curated contract glyphs
    /// (`domain::contract::BOARD_ICON_VALUES`), one clickable cell per icon
    /// name; the selected one carries the primary ring (same selection style
    /// as the color swatch grid below).
    fn icon_picker(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let mut grid = h_flex().flex_wrap().gap_1p5();
        for &name in domain::contract::BOARD_ICON_VALUES {
            let selected = name == self.icon;
            let view = cx.entity().clone();
            grid = grid.child(
                div()
                    .id(SharedString::from(format!("board-icon-{name}")))
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(if selected {
                        cx.theme().primary
                    } else {
                        cx.theme().border
                    })
                    .cursor_pointer()
                    .child(
                        crate::icons::board_icon_name_glyph(name)
                            .small()
                            .text_color(if selected {
                                cx.theme().primary
                            } else {
                                cx.theme().muted_foreground
                            }),
                    )
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.icon = name;
                            cx.notify();
                        });
                    }),
            );
        }

        v_flex().gap_2().child(field_label(cx, "Icon")).child(grid)
    }

    /// The "Repository" field: a dropdown offering the team's connected
    /// registry repos AND (once the GitHub App is installed) the user's
    /// installable GitHub repos to connect inline. When the App is configured
    /// but not installed, a "Connect GitHub" button opens the install URL in
    /// the browser; an explicit Refresh re-runs both fetches after the user
    /// returns. Failures/empties fall through to a nudge.
    fn repository_field(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let mut column = v_flex().gap_2();

        // Both sources still in flight → a single disabled placeholder.
        if matches!(self.repos, RepoLoad::Loading) && matches!(self.github, GithubLoad::Loading) {
            return v_flex()
                .gap_2()
                .child(field_label(cx, "Repository"))
                .child(
                    Button::new("board-repo-picker")
                        .outline()
                        .small()
                        .w_full()
                        .label("Loading repositories\u{2026}")
                        .disabled(true),
                );
        }

        let registry: Vec<RepoOption> = match &self.repos {
            RepoLoad::Ready(repos) => repos.clone(),
            _ => Vec::new(),
        };
        let github_result = match &self.github {
            GithubLoad::Ready(result) => Some(result),
            _ => None,
        };
        let github_repos: Vec<GithubRepo> = github_result
            .map(|result| result.repos.clone())
            .unwrap_or_default();
        let has_options = !registry.is_empty() || !github_repos.is_empty();

        if has_options {
            let label: SharedString = self
                .repo_choice
                .as_ref()
                .map(|choice| SharedString::from(choice.full_name().to_string()))
                .unwrap_or_else(|| "Select a repository".into());
            let selected_full = self
                .repo_choice
                .as_ref()
                .map(|choice| choice.full_name().to_string());
            let view = cx.entity().clone();
            column = column.child(
                Button::new("board-repo-picker")
                    .outline()
                    .small()
                    .w_full()
                    .icon(IconName::Github)
                    .label(label)
                    .dropdown_menu(move |menu, _window, _cx| {
                        // A team can hold many repos — cap + scroll
                        // (EXP-46a; mirror of create_issue_dialog's pickers).
                        // No submenus here (unsupported inside scrollable
                        // menus at the pinned gpui-component rev).
                        let mut menu = menu.scrollable(true).max_h(px(320.));
                        if !registry.is_empty() {
                            menu = menu.label("Connected");
                            for repo in &registry {
                                let view = view.clone();
                                let id = repo.id.clone();
                                let full_name = repo.full_name.clone();
                                let checked = selected_full.as_deref() == Some(repo.full_name.as_str());
                                menu = menu.item(
                                    PopupMenuItem::new(SharedString::from(repo.full_name.clone()))
                                        .icon(Icon::new(IconName::Github))
                                        .checked(checked)
                                        .on_click(move |_, _, cx| {
                                            let choice = RepoChoice::Registry {
                                                id: id.clone(),
                                                full_name: full_name.clone(),
                                            };
                                            view.update(cx, |this, cx| {
                                                this.repo_choice = Some(choice.clone());
                                                cx.notify();
                                            });
                                        }),
                                );
                            }
                        }
                        if !github_repos.is_empty() {
                            if !registry.is_empty() {
                                menu = menu.separator();
                            }
                            menu = menu.label("GitHub");
                            for repo in &github_repos {
                                let view = view.clone();
                                let repo = repo.clone();
                                let checked = selected_full.as_deref() == Some(repo.full_name.as_str());
                                let title = if repo.private {
                                    format!("{} \u{00b7} private", repo.full_name)
                                } else {
                                    repo.full_name.clone()
                                };
                                menu = menu.item(
                                    PopupMenuItem::new(SharedString::from(title))
                                        .icon(Icon::new(IconName::Github))
                                        .checked(checked)
                                        .on_click(move |_, _, cx| {
                                            let choice = RepoChoice::Inline(repo.clone());
                                            view.update(cx, |this, cx| {
                                                this.repo_choice = Some(choice.clone());
                                                cx.notify();
                                            });
                                        }),
                                );
                            }
                        }
                        menu
                    }),
            );
        }

        // Connect-GitHub affordance: the App is configured on the server but
        // not installed for this user. Install is a browser hand-off; Refresh
        // re-runs both fetches once the user returns.
        let configured_not_installed = github_result
            .map(|result| result.configured && !result.installed)
            .unwrap_or(false);
        if configured_not_installed {
            // Connect claims the account for the team: prefer the
            // single-consent connect URL, fall back to the App install page.
            let connect_url = github_result.and_then(|result| {
                result
                    .connect_url
                    .clone()
                    .or_else(|| result.install_url.clone())
            });
            let mut row = h_flex().flex_wrap().gap_2().items_center();
            if let Some(url) = connect_url {
                row = row.child(
                    Button::new("board-repo-connect-gh")
                        .outline()
                        .small()
                        .icon(IconName::Github)
                        .label("Connect GitHub")
                        .on_click(move |_, _, cx| open_url(cx, url.clone())),
                );
            }
            column = column.child(row);
        }

        // Grant-model reconnect: installed but the per-user grant snapshot is
        // missing/stale — a pre-grant link comes back `installed: true` with
        // an empty `repos[]` and `needs_reauth` on the linked installation(s).
        // Reconnect must run the OAuth connect (it re-captures grants); the
        // App install page does NOT (web parity: `github-repo-picker.tsx`).
        let github_repos_empty = github_result
            .map(|result| result.repos.is_empty())
            .unwrap_or(true);
        let needs_reconnect = github_result
            .map(|result| {
                result.installed
                    && (result.repos.is_empty()
                        || result.installations.iter().any(|inst| inst.needs_reauth))
            })
            .unwrap_or(false);
        if needs_reconnect {
            let mut notice = h_flex()
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
                .child(
                    Icon::new(IconName::TriangleAlert)
                        .xsmall()
                        .text_color(theme::tokens::YELLOW.to_hsla()),
                )
                .child(div().flex_1().min_w_0().child(if github_repos_empty {
                    "Reconnect GitHub to load the repositories you can access."
                } else {
                    "Reconnect GitHub to refresh — repos created or shared with you since \
                     your last connect won't appear until you do."
                }));
            let reconnect_url = github_result.and_then(|result| {
                result
                    .connect_url
                    .clone()
                    .or_else(|| result.install_url.clone())
            });
            if let Some(url) = reconnect_url {
                notice = notice.child(
                    Button::new("board-repo-reconnect-gh")
                        .outline()
                        .xsmall()
                        .icon(IconName::Github)
                        .label("Reconnect GitHub")
                        .on_click(move |_, _, cx| open_url(cx, url.clone())),
                );
            }
            column = column.child(notice);
        }

        // Empty/failure messaging when there is nothing to pick (the
        // installed-but-grantless empty case is the reconnect notice above).
        if !has_options && !configured_not_installed && !needs_reconnect {
            let message: SharedString = match (&self.repos, &self.github) {
                (RepoLoad::Failed(message), _) => message.clone(),
                (_, GithubLoad::Failed(message)) => message.clone(),
                (_, GithubLoad::Ready(result)) if !result.configured => {
                    "GitHub isn't configured on this server, so repositories can't be connected."
                        .into()
                }
                _ => "No repositories available yet — connect one on GitHub.".into(),
            };
            column = column.child(
                div()
                    .px_3()
                    .py_2()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_dashed()
                    .border_color(cx.theme().border)
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(message),
            );
        }

        // Always offer a manual Refresh (re-detect after a browser install),
        // plus — once installed — a "Refresh from GitHub" re-auth (the repo
        // list is a grant snapshot: repos created or shared since the last
        // OAuth connect only appear after reconnecting) and a "manage on
        // GitHub" link when the installed repo list was truncated (the
        // target repo may need granting on GitHub first).
        let mut actions = h_flex().gap_2().items_center().child(
            Button::new("board-repo-refresh")
                .ghost()
                .xsmall()
                .label("Refresh")
                .on_click(cx.listener(|this, _, _, cx| this.spawn_fetches(true, cx))),
        );
        if let Some(url) = github_result.and_then(|result| {
            result
                .installed
                .then(|| {
                    result
                        .connect_url
                        .clone()
                        .or_else(|| result.install_url.clone())
                })
                .flatten()
        }) {
            actions = actions.child(
                Button::new("board-repo-refresh-gh")
                    .link()
                    .xsmall()
                    .label("Refresh from GitHub")
                    .icon(IconName::ExternalLink)
                    .on_click(move |_, _, cx| open_url(cx, url.clone())),
            );
        }
        if let Some(url) = github_result.and_then(|result| {
            (result.installed && result.has_more)
                .then(|| result.install_url.clone())
                .flatten()
        }) {
            actions = actions.child(
                Button::new("board-repo-manage-gh")
                    .link()
                    .xsmall()
                    .label("Add more on GitHub")
                    .icon(IconName::ExternalLink)
                    .on_click(move |_, _, cx| open_url(cx, url.clone())),
            );
        }
        column = column.child(actions);

        v_flex()
            .gap_2()
            .child(field_label(cx, "Repository"))
            .child(column)
    }
}

impl Render for CreateBoardDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            self.name.update(cx, |state, cx| state.focus(window, cx));
        }

        let name_empty = self.name.read(cx).value().trim().is_empty();
        let prefix_empty = self.prefix.read(cx).value().trim().is_empty();
        // A repository is optional — only name + prefix gate submit.
        let disabled = name_empty || prefix_empty || self.submitting;

        let mut form = v_flex()
            .gap_4()
            .child(labeled(cx, "Name", Input::new(&self.name).small()))
            .child(labeled(cx, "Prefix", Input::new(&self.prefix).small()))
            .child(self.icon_picker(cx))
            .child(
                v_flex()
                    .gap_2()
                    .child(field_label(cx, "Color"))
                    .child(color_swatch_grid(&self.color, cx.entity().clone(), cx)),
            )
            .child(self.repository_field(cx));

        if let Some(error) = &self.error {
            let mut error_block = v_flex().gap_2().child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
            // Grant-model FORBIDDEN: pair the message with the OAuth
            // reconnect hand-off (`connect_url` re-captures grants; the App
            // install page does not).
            if self.grant_reconnect {
                let url = match &self.github {
                    GithubLoad::Ready(result) => result
                        .connect_url
                        .clone()
                        .or_else(|| result.install_url.clone()),
                    _ => None,
                };
                if let Some(url) = url {
                    error_block = error_block.child(
                        h_flex().child(
                            Button::new("board-grant-reconnect-gh")
                                .outline()
                                .xsmall()
                                .icon(IconName::Github)
                                .label("Reconnect GitHub")
                                .on_click(move |_, _, cx| open_url(cx, url.clone())),
                        ),
                    );
                }
            }
            form = form.child(error_block);
        }

        form.child(
            h_flex().justify_end().child(
                Button::new("create-board-submit")
                    .primary()
                    .small()
                    .label(if self.submitting {
                        "Creating..."
                    } else {
                        "Create board"
                    })
                    .disabled(disabled)
                    .loading(self.submitting)
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            ),
        )
    }
}

/// Web `ColorSwatchGrid`: a wrapping row of h-5 w-5 rounded-full swatches;
/// the selected one carries a ring (approximated as a padded border ring).
fn color_swatch_grid(
    selected: &str,
    view: Entity<CreateBoardDialogView>,
    cx: &App,
) -> impl IntoElement {
    let mut grid = h_flex().flex_wrap().gap_1p5();
    for color in SWATCH_COLORS {
        let fill = parse_hex_color(color).unwrap_or(cx.theme().muted_foreground);
        let is_selected = color == selected;
        let view = view.clone();
        grid = grid.child(
            div()
                .id(SharedString::from(format!("swatch-{color}")))
                .size(px(24.))
                .rounded_full()
                .p(px(2.))
                .border_1()
                .border_color(if is_selected {
                    cx.theme().foreground
                } else {
                    gpui::transparent_black()
                })
                .cursor_pointer()
                .child(div().size_full().rounded_full().bg(fill))
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.color = color.to_string();
                        cx.notify();
                    });
                }),
        );
    }
    grid
}

fn field_label(cx: &App, label: &'static str) -> impl IntoElement {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(label)
}

fn labeled(cx: &App, label: &'static str, input: Input) -> impl IntoElement {
    v_flex().gap_2().child(field_label(cx, label)).child(input)
}

/// Web `derivePrefix` (`lib/board.ts`): first letter of each
/// space/dash/underscore-separated word, uppercased, max 5.
pub(crate) fn derive_prefix(name: &str) -> String {
    name.split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .filter_map(|word| word.chars().next())
        .collect::<String>()
        .to_uppercase()
        .chars()
        .take(5)
        .collect()
}

/// The web `isPlanLimitError` analog: plan caps surface as tRPC FORBIDDEN
/// (HTTP 403).
pub(crate) fn is_plan_limit(err: &api::ApiError) -> bool {
    matches!(err, api::ApiError::Http { status: 403, .. })
}

/// The grant-model FORBIDDEN from `boards.create`'s inline `{fullName}`
/// arm: HTTP 403 whose message carries the server's "reconnect GitHub" hint
/// (`apps/web/src/lib/trpc/integrations.ts`). Check this BEFORE
/// [`is_plan_limit`] — that helper matches ANY 403, so this error would
/// otherwise be misread as a plan cap.
fn is_grant_forbidden(err: &api::ApiError) -> bool {
    matches!(
        err,
        api::ApiError::Http { status: 403, message } if message.contains("reconnect GitHub")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_forbidden_detected_before_plan_limit() {
        let grant = api::ApiError::Http {
            status: 403,
            message: "You don't have access to acme/repo on GitHub, or your connection is \
                      stale — reconnect GitHub in team settings → Repositories to \
                      refresh which repositories you can access."
                .into(),
        };
        // A grant FORBIDDEN also satisfies `is_plan_limit` (any 403) — the
        // submit handler's ordering is what keeps it out of the upsell path.
        assert!(is_grant_forbidden(&grant));
        assert!(is_plan_limit(&grant));

        let plan_cap = api::ApiError::Http {
            status: 403,
            message: "Plan limit reached".into(),
        };
        assert!(!is_grant_forbidden(&plan_cap));
        assert!(is_plan_limit(&plan_cap));
    }

    #[test]
    fn derive_prefix_matches_web() {
        assert_eq!(derive_prefix("My Board"), "MB");
        assert_eq!(derive_prefix("backend-api"), "BA");
        assert_eq!(derive_prefix("a_b_c_d_e_f_g"), "ABCDE");
        assert_eq!(derive_prefix(""), "");
        assert_eq!(derive_prefix("  spaced   out  "), "SO");
    }
}
