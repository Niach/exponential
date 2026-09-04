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
    div, px, size, App, AppContext as _, Entity, InteractiveElement as _, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    scroll::{Scrollbar, ScrollbarAxis},
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};
use serde::{Deserialize, Serialize};
use sync::Store;

use crate::controls::{glass_input, WebControl as _};
use crate::actions::NewBoard;
use crate::github_connect::{fetch_github_repos, GithubRepo, GithubReposResult};
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::navigation::{active_team_id, nav_for_window};
use crate::queries;
use crate::settings::open_url;
use crate::icons::registry;

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
    /// The repo's effective default branch (team pin folded in server-side) —
    /// EXP-712's Branch dropdown shows it when the board pins nothing.
    #[serde(default)]
    default_branch: Option<String>,
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

/// Open the dialog for a team (a native window since EXP-284).
pub fn open(window: &mut Window, cx: &mut App, team_id: String) {
    // Web sm:max-w-[26rem] width; the form is tall (icon grid + swatches +
    // repo picker) — cap against the opener and scroll the body past the cap.
    // EXP-369: 560 → 640 so the whole form fits on a typical screen; the
    // pinned footer is what actually guarantees a reachable submit.
    let height = (window.viewport_size().height * 0.85).min(px(640.));
    let spec = DialogSpec::new("Create board", size(px(416.), height));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| CreateBoardDialogView::new(team_id, false, window, cx));
        let busy = view.clone();
        let submit = view.clone();
        DialogContent::new(view)
            // EXP-369: the view pins its own footer and scrolls only the form
            // — the shell's wrapper would scroll "Create board" out of reach.
            .self_scrolling()
            .can_close(move |cx| !busy.read(cx).submitting)
            .on_enter(move |window, cx| {
                submit.update(cx, |view, cx| view.submit(window, cx));
            })
    });
}

/// Emitted (embedded host only, EXP-367) once the created board is VISIBLE
/// in the synced collection — the onboarding wizard advances on it.
pub(crate) struct BoardCreated;
impl gpui::EventEmitter<BoardCreated> for CreateBoardDialogView {}

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
    /// EXP-712: the board's branch pin — `None` = follow the repo's default.
    /// Reset whenever the repository selection changes (the pin belongs to
    /// the repo it was picked on).
    branch: Option<String>,
    /// `repositories.listBranches` for the SELECTED registry repo, fetched
    /// when the Branch menu first opens.
    branches: Option<(String, crate::board_form::BranchLoad)>,
    /// EXP-712: the select's trailing "Connect …" action expands the GitHub
    /// picker + its connect affordances underneath.
    connect_open: bool,
    /// Connected-repo list for the picker, fetched from `repositories.list`.
    repos: RepoLoad,
    /// Installable GitHub-App repos, fetched from `integrations.github.repos`.
    github: GithubLoad,
    /// Monotonic guard so a slow refetch can't clobber a newer Refresh.
    fetch_generation: u64,
    submitting: bool,
    error: Option<SharedString>,
    /// Failure copy from the `exponential://github-connected?error=…` deep
    /// link (EXP-368) — the browser connect hand-off ended in an error.
    connect_error: Option<SharedString>,
    /// The last submit failed with the grant-model FORBIDDEN (stale/missing
    /// GitHub grant for the picked repo) — pair the error with a "Reconnect
    /// GitHub" hand-off.
    grant_reconnect: bool,
    /// EXP-367: hosted inside the onboarding wizard instead of a native
    /// dialog window — success EMITS [`BoardCreated`] instead of closing a
    /// dialog, and the plan-limit hand-off renders inline.
    embedded: bool,
    focused_once: bool,
    /// EXP-369: the scrolling form pane, so the footer can stay pinned.
    /// Unused in `embedded` mode (the wizard column scrolls instead).
    body_scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl CreateBoardDialogView {
    pub(crate) fn new(
        team_id: String,
        embedded: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
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
        // Web prefix input: uppercased, maxLength 4 (the server cap, REV-4).
        subscriptions.push(cx.subscribe_in(
            &prefix,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let value = this.prefix.read(cx).value().to_string();
                    let normalized: String = value.to_uppercase().chars().take(4).collect();
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
        // EXP-368: the browser GitHub-connect hand-off ends in an
        // `exponential://github-connected` deep link that lands on the App —
        // adopt it here so the picker refreshes itself (success) or explains
        // the failure without the user pressing "I've connected".
        subscriptions.push(
            cx.observe_global::<crate::github_connect::GithubConnectSignal>(|this, cx| {
                let Some(outcome) = cx
                    .try_global::<crate::github_connect::GithubConnectSignal>()
                    .and_then(|signal| signal.outcome.clone())
                else {
                    return;
                };
                match outcome {
                    crate::github_connect::GithubConnectOutcome::Connected => {
                        // Same as the manual refresh: bypass the server's
                        // per-team repo cache (spawn_fetches clears the
                        // notice).
                        this.spawn_fetches(true, cx);
                    }
                    crate::github_connect::GithubConnectOutcome::Failed(code) => {
                        // Nothing changed server-side — no refetch.
                        this.connect_error =
                            Some(crate::github_connect::connect_error_message(&code).into());
                        cx.notify();
                    }
                }
            }),
        );

        let mut this = Self {
            team_id,
            name,
            prefix,
            icon: DEFAULT_ICON,
            color: DEFAULT_COLOR.to_string(),
            repo_choice: None,
            branch: None,
            branches: None,
            connect_open: false,
            repos: RepoLoad::Loading,
            github: GithubLoad::Loading,
            fetch_generation: 0,
            submitting: false,
            error: None,
            connect_error: None,
            grant_reconnect: false,
            embedded,
            focused_once: false,
            body_scroll: ScrollHandle::new(),
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
        // A fresh fetch (manual refresh or a successful reconnect) retires
        // any earlier connect-failure notice.
        self.connect_error = None;
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
        // EXP-712: only meaningful with a repository (the server ignores it
        // otherwise) — and only when the board pins something OTHER than the
        // repo's own default.
        let default_branch = repository
            .is_some()
            .then(|| self.branch.clone())
            .flatten();
        let input = api::boards::BoardsCreateInput {
            team_id: self.team_id.clone(),
            name,
            prefix,
            icon: Some(self.icon.to_string()),
            color: Some(self.color.clone()),
            repository,
            default_branch,
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
                        let _ = api::onboarding::complete(&trpc);
                    }
                    created
                })
                .await;

            match result {
                Ok(output) => {
                    // Seed the row from the mutation response (EXP-470): a
                    // board created in an EXISTING team echoes fast, but the
                    // onboarding wizard creates one in a just-created team
                    // whose boards shape is still mid-rotation — the seed
                    // makes the close instant either way.
                    let board_id = output.board.id.clone();
                    let seeded = output.board.team_id.clone().map(|team_id| {
                        let mut board = domain::rows::Board::seeded(
                            board_id.clone(),
                            team_id,
                            output.board.name.clone().unwrap_or_else(|| "New board".to_string()),
                        );
                        board.slug = output.board.slug.clone();
                        board.prefix = output.board.prefix.clone();
                        board.color = output.board.color.clone();
                        board.repository_id = output.board.repository_id.clone();
                        board.default_branch = output.board.default_branch.clone();
                        board
                    });
                    let boards = window
                        .update(|_, cx| {
                            let store = Store::global(cx).clone();
                            if let Some(seeded) = seeded {
                                store.collections().seed_board(seeded, cx);
                            }
                            store.collections().boards.clone()
                        })
                        .ok();
                    if let Some(boards) = boards {
                        queries::await_row_visible(&boards, &board_id, window).await;
                    }
                    let _ = this.update_in(window, |view, window, cx| {
                        // The server just completed onboarding — mirror the
                        // stamp locally (EXP-367; warm starts never re-fetch
                        // the session).
                        crate::onboarding::stamp_local_onboarding(cx);
                        if view.embedded {
                            // The wizard host advances on the event; scope
                            // this window to the new board so the app lands
                            // on it once the wizard finishes.
                            view.submitting = false;
                            crate::navigation::set_active_board(window, cx, board_id.clone());
                            cx.emit(BoardCreated);
                            cx.notify();
                        } else {
                            native_dialog::close_then(window, cx, move |window, cx| {
                                // Scope the opener to the new board and surface
                                // its (empty) issue list in the sidebar.
                                crate::navigation::set_active_board(window, cx, board_id);
                                crate::sidebar::activate_tool(
                                    window,
                                    cx,
                                    crate::sidebar::ToolWindow::BoardIssues,
                                );
                            });
                        }
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, window, cx| {
                        // Grant-model FORBIDDEN checked first: its copy pairs
                        // with the reconnect hand-off, never the generic box.
                        if is_grant_forbidden(&err) {
                            this.error = Some(
                                "GitHub says you don't have access to this repository, or \
                                 your connection is stale. Reconnect GitHub and try again."
                                    .into(),
                            );
                            this.grant_reconnect = true;
                            this.submitting = false;
                            cx.notify();
                            return;
                        }
                        if is_plan_limit(&err) {
                            if this.embedded {
                                // Wizard host: no dialog to close — the same
                                // §4.9 neutral hand-off renders inline.
                                this.error = Some(
                                    "Board limit reached. Upgrade on the web to create \
                                     more."
                                        .into(),
                                );
                                this.submitting = false;
                                cx.notify();
                                return;
                            }
                            // §4.9: neutral hand-off, never an upgrade dialog.
                            // The notification lands on the OPENER — this
                            // window is about to be gone.
                            native_dialog::close_then(window, cx, |window, cx| {
                                window.push_notification(
                                    Notification::warning(
                                        "Board limit reached. Upgrade on the web to \
                                         create more.",
                                    ),
                                    cx,
                                );
                            });
                            return;
                        }
                        this.error = Some(err.user_message().into());
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
    /// The icon picker: the shared swatch-and-popover over the curated
    /// contract glyphs (EXP-575 — `crate::board_form`, also the per-board
    /// settings page).
    fn icon_picker(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let view = cx.entity().clone();
        crate::board_form::icon_picker(
            "create-board",
            Some(self.icon),
            false,
            move |name, _, cx| {
                let Some(name) = name else { return };
                view.update(cx, |this, cx| {
                    this.icon = name;
                    cx.notify();
                });
            },
            cx,
        )
    }

    /// "Name" = the icon picker LEFT of the name input, one row (EXP-584 —
    /// web `BoardNameField`, board settings and the natives share the shape).
    fn name_field(&self, window: &Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        v_flex().gap_2().child(field_label(cx, "Name")).child(
            h_flex()
                .gap_2()
                .items_center()
                .child(self.icon_picker(cx))
                .child(div().flex_1().child(glass_input(&self.name, window, cx).web_input_sm())),
        )
    }

    /// EXP-712 — the board's **repository + branch** block (web parity:
    /// `components/board-repo-field.tsx`).
    ///
    /// ONE select showing the current value: "No repository", the team's
    /// connected repos, then a trailing "Connect another repository…" action
    /// that expands the existing GitHub connect/picker flow
    /// underneath. Directly beneath it — only once a repository is selected —
    /// the **Branch** this board's coding sessions start from (the repo's
    /// default unless the board pins another). ONE caption line under both.
    fn repository_field(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let loading =
            matches!(self.repos, RepoLoad::Loading) && matches!(self.github, GithubLoad::Loading);
        let registry: Vec<RepoOption> = match &self.repos {
            RepoLoad::Ready(repos) => repos.clone(),
            _ => Vec::new(),
        };
        let github_result = match &self.github {
            GithubLoad::Ready(result) => Some(result),
            _ => None,
        };
        let has_repos = !registry.is_empty();

        let label: SharedString = match (&self.repo_choice, loading) {
            (Some(choice), _) => choice.full_name().to_string().into(),
            (None, true) => "Loading\u{2026}".into(),
            (None, false) => crate::board_form::NO_REPOSITORY.into(),
        };
        let selected_full = self
            .repo_choice
            .as_ref()
            .map(|choice| choice.full_name().to_string());
        // An inline pick has no registry row yet, so the select renders it
        // itself — otherwise choosing it would blank the trigger.
        let inline_choice = match &self.repo_choice {
            Some(RepoChoice::Inline(repo)) => Some(repo.clone()),
            _ => None,
        };
        let connect_label = crate::board_form::connect_repository_label(has_repos);
        let view = cx.entity().clone();
        let menu_repos = registry.clone();
        let select = Button::new("board-repo-picker")
            .outline()
            .cursor_pointer()
            .small()
            .w_full()
            .icon(registry::UI_GITHUB)
            .label(label)
            .dropdown_menu(move |menu, _window, _cx| {
                let mut menu = menu.scrollable(true).max_h(px(320.));
                {
                    let view = view.clone();
                    menu = menu.item(
                        PopupMenuItem::new(crate::board_form::NO_REPOSITORY)
                            .checked(selected_full.is_none())
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| this.pick_repo(None, cx));
                            }),
                    );
                }
                for repo in &menu_repos {
                    let view = view.clone();
                    let choice = RepoChoice::Registry {
                        id: repo.id.clone(),
                        full_name: repo.full_name.clone(),
                    };
                    let checked = selected_full.as_deref() == Some(repo.full_name.as_str());
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(repo.full_name.clone()))
                            .icon(Icon::new(registry::UI_GITHUB))
                            .checked(checked)
                            .on_click(move |_, _, cx| {
                                let choice = choice.clone();
                                view.update(cx, |this, cx| this.pick_repo(Some(choice), cx));
                            }),
                    );
                }
                if let Some(repo) = &inline_choice {
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(repo.full_name.clone()))
                            .icon(Icon::new(registry::UI_GITHUB))
                            .checked(true),
                    );
                }
                let view = view.clone();
                menu.item(
                    PopupMenuItem::new(connect_label)
                        .icon(Icon::new(registry::UI_ADD))
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.connect_open = true;
                                cx.notify();
                            });
                        }),
                )
            });

        let mut column = v_flex()
            .gap_2()
            .child(field_label(cx, "Repository"))
            .child(select);
        if self.connect_open {
            column = column.child(self.connect_section(github_result, cx));
        }

        let mut block = v_flex().gap_4().child(column);
        if let Some(branch) = self.branch_field(&registry, cx) {
            block = block.child(branch);
        }
        block
            .child(crate::board_form::board_repo_note(cx))
            .into_any_element()
    }

    /// EXP-712's **Branch** row — only once a repository is selected. A
    /// connected registry repo lists its branches live
    /// (`repositories.listBranches`); a repo picked for INLINE connect has no
    /// registry row to list against yet, so its row shows GitHub's default
    /// and unlocks after the board is created.
    fn branch_field(
        &self,
        registry: &[RepoOption],
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let (repository_id, repo_default) = match self.repo_choice.as_ref()? {
            RepoChoice::Registry { id, .. } => (
                Some(id.clone()),
                registry
                    .iter()
                    .find(|repo| &repo.id == id)
                    .and_then(|repo| repo.default_branch.clone()),
            ),
            RepoChoice::Inline(repo) => (
                None,
                Some(repo.default_branch.clone()).filter(|branch| !branch.is_empty()),
            ),
        };
        let value = crate::repo_resolver::board_base_branch(
            self.branch.as_deref(),
            repo_default.as_deref(),
        )?;
        let repository_id = match repository_id {
            Some(id) => id,
            // Inline: nothing to list yet — show the repo's default, locked.
            None => {
                return Some(
                    v_flex()
                        .gap_2()
                        .child(field_label(cx, "Branch"))
                        .child(crate::board_form::branch_menu::<Self>(
                            "board-branch-picker",
                            value,
                            repo_default,
                            true,
                            |_, _| None,
                            |_, _| {},
                            |_, _, _| {},
                            cx,
                        ))
                        .into_any_element(),
                )
            }
        };
        let fetch_id = repository_id.clone();
        let read_id = repository_id.clone();
        Some(
            v_flex()
                .gap_2()
                .child(field_label(cx, "Branch"))
                .child(crate::board_form::branch_menu::<Self>(
                    "board-branch-picker",
                    value,
                    repo_default,
                    false,
                    move |this: &Self, _| {
                        this.branches
                            .as_ref()
                            .filter(|(id, _)| id == &read_id)
                            .map(|(_, load)| load.clone())
                    },
                    move |this: &mut Self, cx| this.ensure_branches(fetch_id.clone(), cx),
                    |this: &mut Self, pick, cx| {
                        this.branch = pick;
                        cx.notify();
                    },
                    cx,
                ))
                .into_any_element(),
        )
    }

    /// Select a repository (or "No repository"). EXP-712: the branch pin
    /// belongs to the repo it was picked on, so every change resets it — the
    /// same rule `boards.setRepository` enforces server-side.
    fn pick_repo(&mut self, choice: Option<RepoChoice>, cx: &mut gpui::Context<Self>) {
        self.repo_choice = choice;
        self.branch = None;
        self.branches = None;
        self.connect_open = false;
        cx.notify();
    }

    /// Lazy `repositories.listBranches` for the selected repo (EXP-712) —
    /// kicked when the Branch menu opens, cached until the selection changes.
    fn ensure_branches(&mut self, repository_id: String, cx: &mut gpui::Context<Self>) {
        if self
            .branches
            .as_ref()
            .is_some_and(|(id, _)| id == &repository_id)
        {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.branches = Some((
            repository_id.clone(),
            crate::board_form::BranchLoad::Loading,
        ));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let fetch_id = repository_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::repositories::list_branches(&trpc, &fetch_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let load = match result {
                    Ok(out) => crate::board_form::BranchLoad::Ready(out.branches),
                    Err(err) => crate::board_form::BranchLoad::Failed(err.to_string().into()),
                };
                this.branches = Some((repository_id, load));
                cx.notify();
            });
        })
        .detach();
    }

    /// The connect/picker flow the select's trailing action expands: the
    /// user's installable GitHub repos (connected inline by `boards.create`),
    /// plus the browser hand-offs the GitHub App needs (connect, unsuspend,
    /// reconnect) and the manual refresh.
    fn connect_section(
        &self,
        github_result: Option<&GithubReposResult>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let mut column = v_flex().gap_2();
        let github_repos: Vec<GithubRepo> = github_result
            .map(|result| result.repos.clone())
            .unwrap_or_default();

        if !github_repos.is_empty() {
            let view = cx.entity().clone();
            column = column.child(
                Button::new("board-repo-github-picker")
                    .outline()
                    .cursor_pointer()
                    .small()
                    .w_full()
                    .icon(registry::UI_GITHUB)
                    .label("Select a GitHub repository\u{2026}")
                    .dropdown_menu(move |menu, _window, _cx| {
                        let mut menu = menu.scrollable(true).max_h(px(320.));
                        for repo in &github_repos {
                            let view = view.clone();
                            let repo = repo.clone();
                            let title = if repo.private {
                                format!("{} \u{00b7} private", repo.full_name)
                            } else {
                                repo.full_name.clone()
                            };
                            menu = menu.item(
                                PopupMenuItem::new(SharedString::from(title))
                                    .icon(Icon::new(registry::UI_GITHUB))
                                    .on_click(move |_, _, cx| {
                                        let choice = RepoChoice::Inline(repo.clone());
                                        view.update(cx, |this, cx| {
                                            this.pick_repo(Some(choice), cx)
                                        });
                                    }),
                            );
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
            if let Some(url) = connect_url {
                column = column.child(
                    h_flex().flex_wrap().gap_2().items_center().child(
                        Button::new("board-repo-connect-gh")
                            .outline()
                            .cursor_pointer()
                            .small()
                            .icon(registry::UI_GITHUB)
                            .label("Connect GitHub")
                            .on_click(move |_, _, cx| open_url(cx, url.clone())),
                    ),
                );
            }
        }

        // EXP-368: the browser connect hand-off deep-linked back with an
        // error — same dashed-danger notice shape as the suspension one
        // below. Cleared by any refetch (deep-link success or manual refresh).
        if let Some(message) = self.connect_error.clone() {
            column = column.child(notice_row(cx.theme().danger, message, cx));
        }

        // Suspension outranks reconnect (REV2-29, EXP-365): a suspended
        // installation lists no repos and mints no tokens, and a reconnect
        // CANNOT fix it — only unsuspending on GitHub can.
        let suspended_installs: Vec<&crate::github_connect::GithubInstallation> = github_result
            .map(|result| {
                result
                    .installations
                    .iter()
                    .filter(|inst| inst.suspended)
                    .collect()
            })
            .unwrap_or_default();
        if !suspended_installs.is_empty() {
            let names = suspended_installs
                .iter()
                .map(|inst| inst.label())
                .collect::<Vec<_>>()
                .join(", ");
            let manage_url = suspended_installs
                .first()
                .map(|inst| inst.manage_url.clone())
                .filter(|url| !url.is_empty());
            let mut notice = notice_row(
                cx.theme().danger,
                SharedString::from(format!(
                    "GitHub suspended the Exponential app for {names}. Its repositories \
                     can't be connected until you unsuspend it on GitHub."
                )),
                cx,
            );
            if let Some(url) = manage_url {
                notice = notice.child(
                    Button::new("board-repo-unsuspend-gh")
                        .outline()
                        .cursor_pointer()
                        .xsmall()
                        .label("Manage")
                        .on_click(move |_, _, cx| open_url(cx, url.clone())),
                );
            }
            column = column.child(notice);
        }

        // Grant-model reconnect: installed but the per-user grant snapshot is
        // missing/stale. Reconnect must run the OAuth connect (it re-captures
        // grants); the App install page does NOT (web parity).
        let github_repos_empty = github_result
            .map(|result| result.repos.is_empty())
            .unwrap_or(true);
        // EXP-557: STALE links are excluded — no reconnect can refresh them.
        let needs_reconnect = github_result
            .map(|result| {
                result.installed
                    && result
                        .installations
                        .iter()
                        .any(|inst| inst.needs_reconnect())
            })
            .unwrap_or(false);
        if needs_reconnect {
            let suffix = github_result
                .map(|result| {
                    crate::github_connect::reauth_account_suffix(
                        &result.installations,
                        if github_repos_empty { "from" } else { "for" },
                    )
                })
                .unwrap_or_default();
            let mut notice = notice_row(
                cx.theme().muted_foreground,
                SharedString::from(if github_repos_empty {
                    format!("Reconnect GitHub to load the repositories you can access{suffix}.")
                } else {
                    format!(
                        "Reconnect GitHub{suffix} to refresh. Repos created or shared \
                         with you since your last connect won't appear until you do."
                    )
                }),
                cx,
            );
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
                        .cursor_pointer()
                        .xsmall()
                        .icon(registry::UI_GITHUB)
                        .label("Reconnect GitHub")
                        .on_click(move |_, _, cx| open_url(cx, url.clone())),
                );
            }
            column = column.child(notice);
        }

        // A genuine fetch failure still has to say so — it is the difference
        // between "nothing to connect" and "we couldn't ask".
        let failure: Option<SharedString> = match (&self.repos, &self.github) {
            (RepoLoad::Failed(message), _) => Some(message.clone()),
            (_, GithubLoad::Failed(message)) => Some(message.clone()),
            (_, GithubLoad::Ready(result)) if !result.configured => Some(
                "GitHub isn't configured on this server, so repositories can't be connected."
                    .into(),
            ),
            _ => None,
        };
        if let Some(message) = failure {
            column = column.child(notice_row(cx.theme().muted_foreground, message, cx));
        }

        // Always offer a manual Refresh (re-detect after a browser install),
        // plus — once installed — a "Refresh from GitHub" re-auth and a
        // "manage on GitHub" link when the installed repo list was truncated.
        let mut actions = h_flex().gap_2().items_center().child(
            Button::new("board-repo-refresh")
                .ghost()
                .cursor_pointer()
                .xsmall()
                .label(if configured_not_installed {
                    "I've connected"
                } else {
                    "Refresh"
                })
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
                    .icon(registry::UI_EXTERNAL_LINK)
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
                    .icon(registry::UI_EXTERNAL_LINK)
                    .on_click(move |_, _, cx| open_url(cx, url.clone())),
            );
        }
        column.child(actions).into_any_element()
    }
}

/// The dashed inline notice shape the connect flow uses for every hand-off.
fn notice_row(color: gpui::Hsla, message: SharedString, cx: &App) -> gpui::Div {
    h_flex()
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
        .text_color(color)
        .child(Icon::new(registry::UI_WARNING).xsmall())
        .child(div().flex_1().min_w_0().child(message))
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
            .child(self.name_field(window, cx))
            .child(labeled(cx, "Prefix", glass_input(&self.prefix, window, cx).web_input_sm()))
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
                                .outline().cursor_pointer()
                                .xsmall()
                                .icon(registry::UI_GITHUB)
                                .label("Reconnect GitHub")
                                .on_click(move |_, _, cx| open_url(cx, url.clone())),
                        ),
                    );
                }
            }
            form = form.child(error_block);
        }

        let footer = h_flex().flex_shrink_0().justify_end().child(
            Button::new("create-board-submit")
                .primary().cursor_pointer()
                .web_sm()
                .label(if self.submitting {
                    "Creating..."
                } else {
                    "Create board"
                })
                .disabled(disabled)
                .loading(self.submitting)
                .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
        );

        // Embedded (onboarding wizard): the host column owns the scrolling and
        // has no definite height for `size_full` to resolve against — keep the
        // button in flow there.
        if self.embedded {
            return form.child(footer.pt_1()).into_any_element();
        }

        // EXP-369: full-height column — the FORM scrolls, "Create board" is
        // pinned to the dialog's bottom edge. Requires
        // [`DialogContent::self_scrolling`] (set in [`open`]), which hands
        // this root a definite-height box; without it the root collapses to
        // its content height and the footer scrolls away again.
        let body_scroll = self.body_scroll.clone();
        v_flex()
            .size_full()
            .gap_3()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        v_flex()
                            .id("create-board-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&body_scroll)
                            .child(form),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .child(Scrollbar::new(&body_scroll).axis(ScrollbarAxis::Vertical)),
                    ),
            )
            .child(
                footer
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border),
            )
            .into_any_element()
    }
}

/// Web `ColorSwatchGrid` — the shared grid (EXP-288: `crate::board_form`,
/// also the per-board settings page).
fn color_swatch_grid(
    selected: &str,
    view: Entity<CreateBoardDialogView>,
    cx: &App,
) -> impl IntoElement {
    crate::board_form::color_swatch_grid("create-board", selected, move |color, _, cx| {
        view.update(cx, |this, cx| {
            this.color = color.to_string();
            cx.notify();
        });
    }, cx)
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
/// space/dash/underscore-separated word, uppercased, max 4 (the server cap,
/// REV-4).
pub(crate) fn derive_prefix(name: &str) -> String {
    name.split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .filter_map(|word| word.chars().next())
        .collect::<String>()
        .to_uppercase()
        .chars()
        .take(4)
        .collect()
}

/// Every plan-cap throw in the server's lib/billing.ts is PRECONDITION_FAILED
/// (HTTP 412) with a message starting with this prefix — the client contract
/// of `apps/web/src/lib/plan-limit-error.ts`. Keep byte-identical.
pub(crate) const PLAN_LIMIT_MESSAGE_PREFIX: &str = "Your plan allows";

/// The web `isPlanLimitError` analog: HTTP 412 AND the plan-limit message
/// prefix. Matching the prefix — never the bare status — keeps the
/// GitHub-connect 412s ("No GitHub account is connected…", "GitHub suspended
/// the Exponential app…") out of the upsell path: they used to render as
/// "Repository limit reached — upgrade" (EXP-365).
pub(crate) fn is_plan_limit(err: &api::ApiError) -> bool {
    matches!(
        err,
        api::ApiError::Http { status: 412, message } if message.starts_with(PLAN_LIMIT_MESSAGE_PREFIX)
    )
}

/// The grant-model FORBIDDEN from `boards.create`'s inline `{fullName}`
/// arm: HTTP 403 whose message carries the server's "reconnect GitHub" hint
/// (`apps/web/src/lib/trpc/integrations.ts`). Check this BEFORE
/// [`is_plan_limit`] — that helper matches ANY 403, so this error would
/// otherwise be misread as a plan cap.
pub(crate) fn is_grant_forbidden(err: &api::ApiError) -> bool {
    matches!(
        err,
        api::ApiError::Http { status: 403, message }
            if message.to_lowercase().contains("reconnect github")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_forbidden_detected_and_plan_limit_needs_prefix() {
        let grant = api::ApiError::Http {
            status: 403,
            message: "You don't have access to acme/repo on GitHub, or your connection is \
                      stale. Reconnect GitHub in team settings → Repositories to \
                      refresh which repositories you can access."
                .into(),
        };
        assert!(is_grant_forbidden(&grant));
        assert!(!is_plan_limit(&grant));

        // The real plan-cap shape: 412 + the lib/plan-limit-error.ts prefix.
        let plan_cap = api::ApiError::Http {
            status: 412,
            message: "Your plan allows 3 seats. Upgrade to invite more members.".into(),
        };
        assert!(!is_grant_forbidden(&plan_cap));
        assert!(is_plan_limit(&plan_cap));

        // GitHub-connect 412s must NEVER read as an upsell (EXP-365): these
        // used to render "Repository limit reached — upgrade on the web".
        for message in [
            "No GitHub account is connected to this team. Connect one in team settings \
             → Repositories, then try again.",
            "GitHub suspended the Exponential app for acme. Unsuspend it on GitHub \
             (team settings → Repositories → Manage), then try again.",
        ] {
            let err = api::ApiError::Http {
                status: 412,
                message: message.into(),
            };
            assert!(!is_plan_limit(&err), "misclassified as plan cap: {message}");
        }
    }

    #[test]
    fn derive_prefix_matches_web() {
        assert_eq!(derive_prefix("My Board"), "MB");
        assert_eq!(derive_prefix("backend-api"), "BA");
        assert_eq!(derive_prefix("a_b_c_d_e_f_g"), "ABCD");
        assert_eq!(derive_prefix(""), "");
        assert_eq!(derive_prefix("  spaced   out  "), "SO");
    }
}
