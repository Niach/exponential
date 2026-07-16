//! Settings → Projects (masterplan-v3 §4.2).
//!
//! Web parity: `components/workspace/projects-section.tsx` — the visible
//! (non-archived) project list with color dot + name + prefix badge, a
//! per-project **repository picker** (`projects.setRepository`, the web
//! "Change repository" dialog — EXP-139: repo linking lives in the IDE too),
//! and a per-project **Delete** behind a confirm dialog (`projects.delete`).
//!
//! The picker offers the workspace's already-connected registry repos
//! (`repositories.list`, shared with the Repositories pane); connecting a
//! brand-new GitHub repo stays with the create-project dialog / the web. The
//! current link renders live from the SYNCED `projects.repository_id` (the
//! mutation's Electric echo updates the row), so this pane needs no
//! optimistic state; a rejected retarget surfaces as an inline error.
//!
//! The §7.3 run-targets editor (DB `run_configs`) plugs into this pane when
//! the IDE track lands it — its CRUD + Trust gate is §07-owned; this file
//! deliberately does not stub it.

use gpui::{
    div, px, ElementId, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogButtonProps,
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, WindowExt as _,
};
use sync::Store;

use crate::navigation::{active_workspace_id, Navigation};
use crate::queries;
use crate::repo_resolver::links_snapshot;

use super::repositories::{fetch_repositories, RepoRow};
use super::{card, card_header, error_notice, parse_hex_color, spawn_trpc};

/// Server fetch state for the per-project repository picker.
enum RepoLoad {
    Idle,
    Loading,
    Ready(Vec<RepoRow>),
    Failed(SharedString),
}

pub struct ProjectsPane {
    nav: Entity<Navigation>,
    repos: RepoLoad,
    /// The workspace the current `repos` belongs to; a switch re-fetches.
    loaded_workspace: Option<String>,
    /// The account it was fetched as — a re-login must re-fetch.
    account_id: Option<String>,
    /// The synced (project → repository) links the current `repos` was
    /// fetched under (EXP-139) — a link to a repo this cache doesn't know yet
    /// (connected on another client) re-fetches exactly once per change.
    loaded_links: Option<Vec<(String, String)>>,
    /// The last `projects.setRepository` rejection, rendered inline under the
    /// list (web parity: the ChangeRepositoryDialog error). Cleared on the
    /// next attempt / workspace switch.
    link_error: Option<SharedString>,
    /// Monotonic guard: a stale in-flight fetch must not clobber a newer one.
    generation: u64,
    _subscriptions: Vec<Subscription>,
}

impl ProjectsPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&collections.projects, |this: &mut Self, _, cx| {
                this.refresh_if_links_changed(cx);
                cx.notify();
            }),
        ];
        Self {
            nav,
            repos: RepoLoad::Idle,
            loaded_workspace: None,
            account_id: None,
            loaded_links: None,
            link_error: None,
            generation: 0,
            _subscriptions: subscriptions,
        }
    }

    /// Kick the `repositories.list` fetch backing the picker when the pane is
    /// first shown or the workspace / account changed (render-time — a hidden
    /// pane never fetches; mirror of the Repositories pane).
    fn ensure_repos(&mut self, workspace_id: &str, cx: &mut gpui::Context<Self>) {
        let account_id = Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        if account_id != self.account_id {
            self.account_id = account_id;
            self.repos = RepoLoad::Idle;
        }
        let same_workspace = self.loaded_workspace.as_deref() == Some(workspace_id);
        if same_workspace && !matches!(self.repos, RepoLoad::Idle) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };

        if !same_workspace {
            self.link_error = None;
        }
        self.repos = RepoLoad::Loading;
        self.loaded_workspace = Some(workspace_id.to_string());
        self.loaded_links = Some(links_snapshot(workspace_id, cx));
        self.generation += 1;
        let generation = self.generation;
        let workspace_id = workspace_id.to_string();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    fetch_repositories(&trpc, &workspace_id).map_err(|err| err.to_string())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer fetch
                }
                this.repos = match result {
                    Ok(repos) => RepoLoad::Ready(repos),
                    Err(message) => RepoLoad::Failed(message.into()),
                };
                cx.notify();
                // A link that changed while this fetch was in flight still
                // lands: compare once more now that the load settled.
                this.refresh_if_links_changed(cx);
            });
        })
        .detach();
    }

    /// Re-fetch the repo list only when a link points at a repo this cache
    /// doesn't know — a repo connected + linked on another client must
    /// resolve to its name here without a restart (EXP-139). Retargets among
    /// already-known repos re-label from the synced row with no fetch (and no
    /// label flicker). Snapshot-keyed so a repo the server genuinely doesn't
    /// list can't loop the fetch; a Failed load heals on any link change.
    fn refresh_if_links_changed(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(workspace_id) = self.loaded_workspace.clone() else {
            return;
        };
        let links = links_snapshot(&workspace_id, cx);
        if self.loaded_links.as_ref() == Some(&links) {
            return;
        }
        let refetch = match &self.repos {
            // Idle refetches on the next render; Loading re-checks on
            // completion (which restores the loaded_links comparison base).
            RepoLoad::Idle | RepoLoad::Loading => return,
            RepoLoad::Failed(_) => true,
            RepoLoad::Ready(repos) => links
                .iter()
                .any(|(_, repo_id)| !repos.iter().any(|repo| &repo.id == repo_id)),
        };
        self.loaded_links = Some(links);
        if refetch {
            self.repos = RepoLoad::Idle;
            cx.notify();
        }
    }

    fn open_delete_dialog(
        project_id: String,
        project_name: String,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        window.open_dialog(cx, move |dialog, _, _| {
            let name = project_name.clone();
            let project_id = project_id.clone();
            dialog
                .title("Delete project")
                .content(move |content, _, cx| {
                    content.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(format!(
                                "This will permanently delete {name} and all its issues. \
                                 This cannot be undone."
                            ))),
                    )
                })
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("Delete project")
                        .ok_variant(ButtonVariant::Danger)
                        .show_cancel(true)
                        .on_ok({
                            let project_id = project_id.clone();
                            move |_, _, cx| {
                                let project_id = project_id.clone();
                                spawn_trpc(cx, "projects.delete", move |trpc| {
                                    api::projects::projects_delete(trpc, &project_id)
                                });
                                true
                            }
                        }),
                )
        });
    }
}

impl ProjectsPane {
    /// `projects.setRepository` off the foreground; success needs nothing
    /// (the synced project row's Electric echo re-labels every surface, incl.
    /// the trunk resolver), a rejection surfaces inline (web parity — a
    /// silently-dropped retarget would leave the user coding on the old repo).
    fn set_repository(
        &mut self,
        project_id: String,
        repository_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.link_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::projects::projects_set_repository(&trpc, &project_id, &repository_id)
                })
                .await;
            if let Err(err) = result {
                log::warn!("[ui] projects.setRepository failed: {err}");
                let _ = this.update(cx, |this, cx| {
                    this.link_error = Some(format!("{err}").into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// The per-project repository picker (EXP-139 — web parity:
    /// `ChangeRepositoryDialog`): a dropdown of the workspace's connected
    /// registry repos, labeled with the project's CURRENT link from the
    /// synced row. Protected projects keep their repo (server-guarded) — the
    /// affordance is disabled like Delete.
    fn repo_picker(
        &self,
        project: &domain::rows::Project,
        protected: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let label: SharedString = match (&project.repository_id, &self.repos) {
            (Some(repo_id), RepoLoad::Ready(repos)) => repos
                .iter()
                .find(|repo| &repo.id == repo_id)
                .map(|repo| SharedString::from(repo.full_name.clone()))
                // Linked, but the list doesn't know it (yet) — never claim
                // "No repository" for a linked project.
                .unwrap_or_else(|| "Repository".into()),
            (Some(_), _) => "Repository".into(),
            (None, _) => "No repository".into(),
        };

        let button = Button::new(row_id("project-repo", &project.id))
            .ghost()
            .xsmall()
            .max_w(px(224.))
            .icon(IconName::Github)
            .label(label);
        if protected {
            return button.disabled(true).into_any_element();
        }

        let project_id = project.id.clone();
        let pane = cx.entity().clone();
        button
            .dropdown_menu(move |menu, _window, cx| {
                // Read LIVE state at open time — the component caches the
                // built menu until dismiss, so a render-time snapshot would
                // pin whatever the list looked like when the row last drew.
                let mut menu = menu.scrollable(true).max_h(px(320.));
                let current = Store::global(cx)
                    .collections()
                    .projects
                    .read(cx)
                    .get(&project_id)
                    .and_then(|project| project.repository_id.clone());
                match &pane.read(cx).repos {
                    RepoLoad::Idle | RepoLoad::Loading => {
                        menu = menu.label("Loading repositories\u{2026}");
                    }
                    RepoLoad::Failed(message) => {
                        menu = menu.label(SharedString::from(format!(
                            "Couldn't load repositories: {message}"
                        )));
                    }
                    RepoLoad::Ready(repos) if repos.is_empty() => {
                        menu = menu.label("No repositories connected yet.");
                    }
                    RepoLoad::Ready(repos) => {
                        for repo in repos {
                            let pane = pane.clone();
                            let project_id = project_id.clone();
                            let repo_id = repo.id.clone();
                            menu = menu.item(
                                PopupMenuItem::new(SharedString::from(repo.full_name.clone()))
                                    .icon(Icon::new(IconName::Github))
                                    .checked(current.as_deref() == Some(repo.id.as_str()))
                                    .on_click(move |_, _, cx| {
                                        pane.update(cx, |this, cx| {
                                            this.set_repository(
                                                project_id.clone(),
                                                repo_id.clone(),
                                                cx,
                                            );
                                        });
                                    }),
                            );
                        }
                    }
                }
                // The list is a cached snapshot of the workspace's connected
                // repos — offer an explicit reload for repos connected on
                // another client (doubles as the Failed state's retry).
                let pane = pane.clone();
                menu.separator()
                    .item(PopupMenuItem::new("Refresh list").on_click(move |_, _, cx| {
                        pane.update(cx, |this, cx| {
                            this.repos = RepoLoad::Idle;
                            cx.notify();
                        });
                    }))
            })
            .into_any_element()
    }
}

impl Render for ProjectsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let workspace_id = active_workspace_id(&self.nav, cx);
        if let Some(workspace_id) = workspace_id.as_deref() {
            self.ensure_repos(workspace_id, cx);
        }
        let projects = workspace_id
            .map(|workspace_id| {
                Store::global(cx)
                    .collections()
                    .projects_in_workspace(&workspace_id, cx)
            })
            .unwrap_or_default();

        let mut body = card(cx).child(card_header(
            format!("Projects · {}", projects.len()),
            "Manage projects in this team.",
            cx,
        ));

        if projects.is_empty() {
            body = body.child(
                div()
                    .px_3()
                    .py_2()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().border)
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No projects in this team yet."),
            );
        } else {
            let mut list = v_flex().gap_2();
            for project in &projects {
                let color = project
                    .color
                    .as_deref()
                    .and_then(parse_hex_color)
                    .unwrap_or(cx.theme().muted_foreground);
                let prefix: SharedString = project.prefix.clone().unwrap_or_default().into();
                let project_id = project.id.clone();
                let project_name = project.name.clone();
                // Protected projects (the bootstrap dogfood board) are
                // non-deletable — the server refuses, so grey out the
                // affordance from the synced flag like the other clients.
                let protected = project.is_protected.unwrap_or(false);
                let delete_button = Button::new(row_id("project-delete", &project.id))
                    .ghost()
                    .xsmall()
                    .icon(IconName::Delete);
                let delete_button = if protected {
                    delete_button.disabled(true)
                } else {
                    delete_button.on_click(cx.listener(move |_, _, window, cx| {
                        Self::open_delete_dialog(
                            project_id.clone(),
                            project_name.clone(),
                            window,
                            cx,
                        );
                    }))
                };

                list = list.child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .px_3()
                        .py_2()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(cx.theme().border)
                        .child(div().size_2p5().rounded_full().flex_shrink_0().bg(color))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(SharedString::from(project.name.clone())),
                        )
                        .child(self.repo_picker(project, protected, cx))
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded(cx.theme().radius)
                                .border_1()
                                .border_color(cx.theme().border)
                                .text_xs()
                                .font_family(theme::terminal::FONT_FAMILY)
                                .text_color(cx.theme().muted_foreground)
                                .child(prefix),
                        )
                        .child(delete_button),
                );
            }
            body = body.child(list);
        }

        // The last rejected retarget (permission / protected / repo gone),
        // web parity with the ChangeRepositoryDialog's inline error.
        if let Some(error) = &self.link_error {
            body = body.child(error_notice(error.clone(), cx));
        }

        v_flex().child(body)
    }
}

fn row_id(kind: &str, id: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("{kind}-{id}")))
}
