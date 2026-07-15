//! Create-project dialog (masterplan-v3 §4.2 — mirror of
//! `apps/web/src/components/create-project-dialog.tsx`).
//!
//! Name `Input` + an **auto-derived-but-editable prefix** `Input`
//! (`derivePrefix`, uppercased, max 10) + the `ColorSwatchGrid` — **no slug
//! field** (server-derived) + the **required backing repository** picker (v4
//! §3.1). The repo picker mirrors the web `GithubRepoPicker`: it offers the
//! workspace's already-connected registry repos AND, once the GitHub App is
//! installed, the user's installable GitHub repos to connect inline in the
//! same `projects.create` call; when the App is configured but not installed a
//! "Connect GitHub" button opens the browser install and an explicit Refresh
//! re-detects. Submit → `projects.create` (then a fire-and-forget
//! `onboarding.complete`); the close is gated on the new project appearing in
//! the synced collection (§4.1 create flows), so the sidebar row is there the
//! moment the dialog is gone. A plan-cap FORBIDDEN surfaces as the neutral
//! "Upgrade on the web" notification (§4.9) — never an in-app purchase UI;
//! the grant-model FORBIDDEN (403 + the server's "reconnect GitHub" hint) is
//! detected first and surfaces a reconnect prompt instead.
//!
//! Opened by the sidebar's Projects `+` via the [`NewProject`]
//! action; [`init`] owns the handler.

use gpui::{
    div, px, App, AppContext as _, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription,
    Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    switch::Switch,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, WindowExt as _,
};
use serde::{Deserialize, Serialize};
use sync::Store;

use domain::contract::{PROJECT_TYPE_DEV, PROJECT_TYPE_FEEDBACK, PROJECT_TYPE_TASKS};

use crate::actions::NewProject;
use crate::create_issue_dialog::parse_hex_color;
use crate::github_connect::{fetch_github_repos, GithubRepo, GithubReposResult};
use crate::navigation::{active_workspace_id, nav_for_window};
use crate::queries;
use crate::settings::open_url;

/// Web `LABEL_COLORS` (`lib/label-colors.ts`) — the swatch palette shared by
/// project + label colors (fixed hex literals on web too).
pub(crate) const SWATCH_COLORS: [&str; 20] = [
    "#ef4444", "#dc2626", "#f97316", "#f59e0b", "#eab308", "#84cc16", "#22c55e", "#10b981",
    "#14b8a6", "#06b6d4", "#0ea5e9", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7", "#ec4899",
    "#f43f5e", "#78716c", "#64748b", "#a3a3a3",
];

/// Web default project color (`create-project-dialog.tsx`).
const DEFAULT_COLOR: &str = "#6366f1";

/// A quickstart template: `(key, title, subtitle, icon, is_public, repo_leads)`.
/// The key reuses the legacy `project_type` string purely for the card's glyph
/// + identity; it is never sent to the server. Picking one seeds `is_public`,
/// `icon`, and whether the repository picker leads the optional fields.
struct Template {
    key: &'static str,
    title: &'static str,
    subtitle: &'static str,
    icon: &'static str,
    is_public: bool,
    repo_leads: bool,
}

const TEMPLATES: [Template; 3] = [
    Template {
        key: PROJECT_TYPE_DEV,
        title: "Dev board",
        subtitle: "Code with Claude on a connected repository.",
        icon: "code",
        is_public: false,
        repo_leads: true,
    },
    Template {
        key: PROJECT_TYPE_TASKS,
        title: "Task board",
        subtitle: "Plain issue tracking — a repository is optional.",
        icon: "square-kanban",
        is_public: false,
        repo_leads: false,
    },
    Template {
        key: PROJECT_TYPE_FEEDBACK,
        title: "Feedback board",
        subtitle: "Public board — anyone with the link can read it.",
        icon: "megaphone",
        is_public: true,
        repo_leads: false,
    },
];

/// A registry repo the new project can target (v4 §3.1 — every project is
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

    /// The `projects.create` repository union arm this choice submits.
    fn to_input(&self) -> api::projects::ProjectRepositoryInput {
        match self {
            RepoChoice::Registry { id, .. } => api::projects::ProjectRepositoryInput::Registry {
                repository_id: id.clone(),
            },
            RepoChoice::Inline(repo) => api::projects::ProjectRepositoryInput::Inline {
                full_name: repo.full_name.clone(),
                default_branch: (!repo.default_branch.is_empty())
                    .then(|| repo.default_branch.clone()),
                private: Some(repo.private),
            },
        }
    }
}

/// `repositories.list({workspaceId})` — the workspace's already-connected repos.
fn fetch_repositories(
    trpc: &api::TrpcClient,
    workspace_id: &str,
) -> Result<Vec<RepoOption>, api::ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        workspace_id: &'a str,
    }
    trpc.query_with_input("repositories.list", &Input { workspace_id })
}

/// Register the App-global [`NewProject`] handler (call once from `ui::init`).
pub fn init(cx: &mut App) {
    cx.on_action(|_: &NewProject, cx| {
        crate::navigation::on_active_window(cx, |window, cx| {
            let nav = nav_for_window(window, cx);
            let Some(workspace_id) = active_workspace_id(&nav, cx) else {
                return;
            };
            open(window, cx, workspace_id);
        });
    });
}

/// Open the dialog for a workspace.
pub fn open(window: &mut Window, cx: &mut App, workspace_id: String) {
    let view = cx.new(|cx| CreateProjectDialogView::new(workspace_id, window, cx));
    window.open_dialog(cx, move |dialog, _window, cx| {
        let busy = view.read(cx).submitting;
        dialog
            .w(px(416.)) // web sm:max-w-[26rem]
            .title("Create project")
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

pub struct CreateProjectDialogView {
    workspace_id: String,
    name: Entity<InputState>,
    prefix: Entity<InputState>,
    /// The selected quickstart template (`dev` / `tasks` / `feedback`) — drives
    /// the card highlight and the default `is_public`/`icon`/`repo_leads`. It is
    /// NOT sent to the server (the create sends `is_public` + `icon`); picking a
    /// template just seeds those.
    template: &'static str,
    /// Whether the new board is public — seeded by the template, then owned by
    /// the "Public" toggle. Sent as `isPublic`.
    is_public: bool,
    /// Curated icon name seeded by the template. Sent as `icon`.
    icon: &'static str,
    /// Whether the repository picker leads the optional fields (dev template).
    repo_leads: bool,
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

impl CreateProjectDialogView {
    fn new(workspace_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
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

        // Default to the Dev quickstart (repo picker leads).
        let default = &TEMPLATES[0];
        let mut this = Self {
            workspace_id,
            name,
            prefix,
            template: default.key,
            is_public: default.is_public,
            icon: default.icon,
            repo_leads: default.repo_leads,
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
        // Load the workspace's connected repos AND the installable GitHub-App
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
        let workspace_id = self.workspace_id.clone();
        cx.spawn(async move |this, cx| {
            let (registry, github) = cx
                .background_executor()
                .spawn(async move {
                    let registry = fetch_repositories(&trpc, &workspace_id)
                        .map_err(|err| err.to_string());
                    let github = fetch_github_repos(&trpc, &workspace_id, refresh)
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

        // A repository is optional on every board now — send whatever was
        // picked, or nothing.
        let repository = self.repo_choice.as_ref().map(RepoChoice::to_input);
        let input = api::projects::ProjectsCreateInput {
            workspace_id: self.workspace_id.clone(),
            name,
            prefix,
            is_public: self.is_public,
            icon: Some(self.icon.to_string()),
            color: Some(self.color.clone()),
            repository,
        };

        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    let created = api::projects::projects_create(&trpc, &input);
                    // First project completes onboarding (web fires this after
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
                    // sidebar shows the project the moment the dialog closes,
                    // then open its (empty) board.
                    let project_id = output.project.id.clone();
                    let projects = window
                        .update(|_, cx| Store::global(cx).collections().projects.clone())
                        .ok();
                    if let Some(projects) = projects {
                        queries::await_row_visible(&projects, &project_id, window).await;
                    }
                    let _ = this.update_in(window, |_, window, cx| {
                        window.close_dialog(cx);
                        // Scope the window to the new project and surface its
                        // (empty) issue list in the sidebar.
                        crate::navigation::set_active_project(window, cx, project_id);
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
                                    "Project limit reached — upgrade on the web to create more.",
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

impl CreateProjectDialogView {
    /// The quickstart-template picker: three selectable cards — Dev / Task /
    /// Feedback board — each with a glyph, title, and one-liner. Picking one
    /// seeds `is_public` + `icon` + `repo_leads`; the repository stays optional
    /// on every template and the "Public" toggle can still override the seed.
    fn template_selector(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let mut grid = v_flex().gap_2();
        for template in &TEMPLATES {
            let key = template.key;
            let icon = template.icon;
            let is_public = template.is_public;
            let repo_leads = template.repo_leads;
            let selected = self.template == key;
            let view = cx.entity().clone();
            grid = grid.child(
                h_flex()
                    .id(SharedString::from(format!("project-template-{key}")))
                    .w_full()
                    .gap_3()
                    .items_center()
                    .px_3()
                    .py_2()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(if selected {
                        cx.theme().primary
                    } else {
                        cx.theme().border
                    })
                    .cursor_pointer()
                    .child(
                        crate::icons::project_icon_name_glyph(template.icon, key)
                            .small()
                            .flex_shrink_0()
                            .text_color(if selected {
                                cx.theme().primary
                            } else {
                                cx.theme().muted_foreground
                            }),
                    )
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(template.title),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(template.subtitle),
                            ),
                    )
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            if this.template != key {
                                this.template = key;
                                this.icon = icon;
                                this.is_public = is_public;
                                this.repo_leads = repo_leads;
                                cx.notify();
                            }
                        });
                    }),
            );
        }

        v_flex()
            .gap_2()
            .child(field_label(cx, "Template"))
            .child(grid)
    }

    /// The "Public" toggle: a public board is readable by anyone with the link.
    /// Seeded by the template, but the user can flip it independently.
    fn public_toggle(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let view = cx.entity().clone();
        v_flex().gap_2().child(field_label(cx, "Visibility")).child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    Switch::new("project-public")
                        .small()
                        .checked(self.is_public)
                        .disabled(self.submitting)
                        .on_click(move |checked: &bool, _, cx| {
                            let checked = *checked;
                            view.update(cx, |this, cx| {
                                this.is_public = checked;
                                cx.notify();
                            });
                        }),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Public — anyone with the link can read it"),
                ),
        )
    }

    /// The "Repository" field: a dropdown offering the workspace's connected
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
                    Button::new("project-repo-picker")
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
                Button::new("project-repo-picker")
                    .outline()
                    .small()
                    .w_full()
                    .icon(IconName::Github)
                    .label(label)
                    .dropdown_menu(move |menu, _window, _cx| {
                        // A workspace can hold many repos — cap + scroll
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
            // Connect claims the account for the workspace: prefer the
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
                    Button::new("project-repo-connect-gh")
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
                    Button::new("project-repo-reconnect-gh")
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
            Button::new("project-repo-refresh")
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
                Button::new("project-repo-refresh-gh")
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
                Button::new("project-repo-manage-gh")
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

impl Render for CreateProjectDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            self.name.update(cx, |state, cx| state.focus(window, cx));
        }

        let name_empty = self.name.read(cx).value().trim().is_empty();
        let prefix_empty = self.prefix.read(cx).value().trim().is_empty();
        // A repository is optional on every board now — only name + prefix gate
        // submit.
        let disabled = name_empty || prefix_empty || self.submitting;

        let mut form = v_flex()
            .gap_4()
            .child(self.template_selector(cx))
            .child(labeled(cx, "Name", Input::new(&self.name).small()))
            .child(labeled(cx, "Prefix", Input::new(&self.prefix).small()));
        // Repository (always optional) + public toggle. The dev template leads
        // with the repo picker; the others surface the toggle first.
        if self.repo_leads {
            form = form
                .child(self.repository_field(cx))
                .child(self.public_toggle(cx));
        } else {
            form = form
                .child(self.public_toggle(cx))
                .child(self.repository_field(cx));
        }
        form = form.child(
            v_flex()
                .gap_2()
                .child(field_label(cx, "Color"))
                .child(color_swatch_grid(&self.color, cx.entity().clone(), cx)),
        );

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
                            Button::new("project-grant-reconnect-gh")
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
                Button::new("create-project-submit")
                    .primary()
                    .small()
                    .label(if self.submitting {
                        "Creating..."
                    } else {
                        "Create project"
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
    view: Entity<CreateProjectDialogView>,
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

/// Web `derivePrefix` (`lib/project.ts`): first letter of each
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

/// The grant-model FORBIDDEN from `projects.create`'s inline `{fullName}`
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
                      stale — reconnect GitHub in workspace settings → Repositories to \
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
        assert_eq!(derive_prefix("My Project"), "MP");
        assert_eq!(derive_prefix("backend-api"), "BA");
        assert_eq!(derive_prefix("a_b_c_d_e_f_g"), "ABCDE");
        assert_eq!(derive_prefix(""), "");
        assert_eq!(derive_prefix("  spaced   out  "), "SO");
    }
}
