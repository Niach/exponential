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
//! "Upgrade on the web" notification (§4.9) — never an in-app purchase UI.
//!
//! Opened by the sidebar's Projects `+` via the [`NewProject`]
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

        let mut this = Self {
            workspace_id,
            name,
            prefix,
            color: DEFAULT_COLOR.to_string(),
            repo_choice: None,
            repos: RepoLoad::Loading,
            github: GithubLoad::Loading,
            fetch_generation: 0,
            submitting: false,
            error: None,
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
                    let github = fetch_github_repos(&trpc, refresh)
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
        let Some(repo_choice) = self.repo_choice.clone() else {
            return;
        };
        if name.is_empty() || prefix.is_empty() || self.submitting {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };

        self.error = None;
        self.submitting = true;
        cx.notify();

        let input = api::projects::ProjectsCreateInput {
            workspace_id: self.workspace_id.clone(),
            name,
            prefix,
            color: Some(self.color.clone()),
            repository: repo_choice.to_input(),
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
                    .dropdown_menu(move |mut menu, _window, _cx| {
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
            let install_url = github_result.and_then(|result| result.install_url.clone());
            let mut row = h_flex().flex_wrap().gap_2().items_center();
            if let Some(url) = install_url {
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

        // Empty/failure messaging when there is nothing to pick.
        if !has_options && !configured_not_installed {
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
        // plus a "manage on GitHub" link when the installed repo list was
        // truncated (the target repo may need granting on GitHub first).
        let mut actions = h_flex().gap_2().items_center().child(
            Button::new("project-repo-refresh")
                .ghost()
                .xsmall()
                .label("Refresh")
                .on_click(cx.listener(|this, _, _, cx| this.spawn_fetches(true, cx))),
        );
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
        // v4 §3.1: a project must be backed by a repository — block submit
        // until one is picked (the server would otherwise reject the create).
        let disabled =
            name_empty || prefix_empty || self.repo_choice.is_none() || self.submitting;

        let mut form = v_flex()
            .gap_4()
            .child(labeled(cx, "Name", Input::new(&self.name).small()))
            .child(labeled(cx, "Prefix", Input::new(&self.prefix).small()))
            .child(self.repository_field(cx))
            .child(
                v_flex()
                    .gap_2()
                    .child(field_label(cx, "Color"))
                    .child(color_swatch_grid(&self.color, cx.entity().clone(), cx)),
            );

        if let Some(error) = &self.error {
            form = form.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_prefix_matches_web() {
        assert_eq!(derive_prefix("My Project"), "MP");
        assert_eq!(derive_prefix("backend-api"), "BA");
        assert_eq!(derive_prefix("a_b_c_d_e_f_g"), "ABCDE");
        assert_eq!(derive_prefix(""), "");
        assert_eq!(derive_prefix("  spaced   out  "), "SO");
    }
}
