//! The Actions tool window (EXP-253): the team's reusable markdown prompts —
//! list + ▶ Run and owner-only Edit/Delete. EXP-257: creation moved into the
//! server-defined virtual **"Create action"** builtin (pinned first in this
//! list; its run IS the creator — an MCP-wired agent session authoring the
//! action), so the old "Describe with Claude"/"Write manually" headers and
//! the local templates are gone. Run opens the unified Start-coding dialog's
//! Actions tab ([`crate::start_coding_dialog::open_for_action`]), which owns
//! agent/model/effort choices and the typed input fields; the trust-gated
//! runner itself lives in [`crate::action_run`].

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, AppContext as _, ClickEvent, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    dialog::DialogButtonProps,
    input::{Input, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _,
};

use crate::action_run::{fetch_repositories, ActionRepoRow};
use crate::icons::ExpIcon;
use crate::navigation::{active_team_id, nav_for_window, Navigation};
use crate::queries;

// ---------------------------------------------------------------------------
// ActionsPanel — the tool-window list
// ---------------------------------------------------------------------------

/// Fetch lifecycle (the settings/run-bar load-gate pattern).
enum Load {
    Idle,
    Loading,
    Ready,
}

pub struct ActionsPanel {
    nav: Entity<Navigation>,
    /// The team the loaded list belongs to (scope-change reset key).
    team_id: Option<String>,
    load: Load,
    actions: Vec<api::actions::Action>,
    error: Option<SharedString>,
    /// Bumped per fetch — a stale response checks it before landing.
    generation: u64,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl ActionsPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        // The builtin "Create action" run authors the new action via MCP
        // during its session — refetch the rail when any run of it ends
        // (EXP-257: replaces the deleted describe-task exit hook; the exit
        // announcement covers child exit, tab close, and window teardown).
        let local_sessions = crate::coding_flow::LocalSessions::global(cx);
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.subscribe(
                &local_sessions,
                |this, _, event: &crate::coding_flow::ActionRunEnded, cx| {
                    if event.action_id == api::actions::BUILTIN_CREATE_ACTION_ID {
                        this.refetch(cx);
                    }
                },
            ),
        ];
        Self {
            nav,
            team_id: None,
            load: Load::Idle,
            actions: Vec::new(),
            error: None,
            generation: 0,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Render-time load gate: reset on team change, fetch once while Idle.
    fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        let team = active_team_id(&self.nav, cx);
        if team != self.team_id {
            self.team_id = team;
            self.actions.clear();
            self.error = None;
            self.load = Load::Idle;
            self.generation += 1;
        }
        if !matches!(self.load, Load::Idle) {
            return;
        }
        self.refetch(cx);
    }

    /// (Re)fetch the active team's actions. The builtin is pinned FIRST by
    /// its flag (EXP-257 — never by sort order).
    pub(crate) fn refetch(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.team_id.clone() else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.load = Load::Loading;
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::actions::list(&trpc, &team_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.load = Load::Ready;
                match result {
                    Ok(mut actions) => {
                        // Stable: keeps the server order within each half.
                        actions.sort_by_key(|action| !action.builtin);
                        this.actions = actions;
                        this.error = None;
                    }
                    Err(err) => {
                        this.error = Some(SharedString::from(format!(
                            "Could not load actions: {err}"
                        )));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// ▶ Run — open the unified Start-coding dialog's Actions tab with this
    /// action preselected (EXP-257: the dialog owns agent/model/effort and
    /// the typed input fields; the trust gate rides its launch).
    fn run(&mut self, action_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.team_id.clone() else {
            return;
        };
        crate::start_coding_dialog::open_for_action(window, cx, team_id, action_id);
    }

    /// Owner Delete, behind a confirm (destructive native actions confirm
    /// first — the client contract).
    fn prompt_delete(
        &mut self,
        action_id: String,
        name: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let panel = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _window, _cx| {
            let panel = panel.clone();
            let action_id = action_id.clone();
            alert
                .confirm()
                .overlay_closable(true)
                .close_button(true)
                .width(px(416.))
                .title(SharedString::from(format!("Delete \"{name}\"?")))
                .description(
                    "Team members will no longer be able to run this action. \
                     A live run keeps going and keeps its label.",
                )
                .button_props(DialogButtonProps::default().ok_text("Delete action"))
                .on_ok(move |_, _, cx| {
                    let Some(trpc) = queries::trpc_client(cx) else {
                        return true;
                    };
                    let panel = panel.clone();
                    let action_id = action_id.clone();
                    cx.spawn(async move |cx| {
                        let result = cx
                            .background_executor()
                            .spawn(async move { api::actions::delete(&trpc, &action_id) })
                            .await;
                        let _ = cx.update(|cx| {
                            if let Err(err) = result {
                                log::warn!("actions: delete failed: {err}");
                            }
                            if let Some(panel) = panel.upgrade() {
                                panel.update(cx, |panel, cx| panel.refetch(cx));
                            }
                        });
                    })
                    .detach();
                    true
                })
        });
    }

    // -- render -------------------------------------------------------------

    fn render_row(
        &self,
        index: usize,
        action: &api::actions::Action,
        owner: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let run_id = action.id.clone();
        let edit_action = action.clone();
        let repo_backed = action.repository_id.is_some();
        let builtin = action.builtin;

        gpui_component::v_flex()
            .id(SharedString::from(format!("action-{}", action.id)))
            .w_full()
            .gap_0p5()
            .px_2()
            .py_1p5()
            .rounded(theme.radius)
            .hover(|this| this.bg(theme.accent.opacity(0.3)))
            .child(
                gpui_component::h_flex()
                    .items_center()
                    .gap_1()
                    .child(
                        // The builtin creator gets its distinct mark (Plus,
                        // like the web's ActionCard) — real actions keep Zap.
                        if builtin {
                            Icon::new(IconName::Plus)
                                .xsmall()
                                .text_color(theme.muted_foreground)
                        } else {
                            Icon::from(ExpIcon::Zap)
                                .xsmall()
                                .text_color(theme.muted_foreground)
                        },
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .text_color(theme.foreground)
                            .child(SharedString::from(action.name.clone())),
                    )
                    .when(repo_backed, |this| {
                        this.child(
                            Icon::from(ExpIcon::GitMerge)
                                .xsmall()
                                .text_color(theme.muted_foreground),
                        )
                    })
                    // No owner menu on the builtin — it is server-defined,
                    // non-editable and non-deletable.
                    .when(owner && !builtin, |this| {
                        let panel = cx.entity().downgrade();
                        this.child(
                            Button::new(("action-menu", index))
                                .ghost()
                                .xsmall()
                                .icon(IconName::Ellipsis)
                                .dropdown_menu(move |menu, _window, _cx| {
                                    // Direct closures (the members-menu
                                    // pattern) — never App-global dispatch
                                    // from an overlay into an unfocused view.
                                    let edit = edit_action.clone();
                                    let edit_panel = panel.clone();
                                    let delete = edit_action.clone();
                                    let delete_panel = panel.clone();
                                    menu.item(
                                        PopupMenuItem::new("Edit…").on_click(
                                            move |_, window, cx| {
                                                let Some(panel) = edit_panel.upgrade() else {
                                                    return;
                                                };
                                                let Some(team_id) = panel
                                                    .read(cx)
                                                    .team_id
                                                    .clone()
                                                else {
                                                    return;
                                                };
                                                open_action_editor(
                                                    window,
                                                    cx,
                                                    team_id,
                                                    edit.clone(),
                                                    edit_panel.clone(),
                                                );
                                            },
                                        ),
                                    )
                                    .separator()
                                    .item(
                                        PopupMenuItem::new("Delete…").on_click(
                                            move |_, window, cx| {
                                                let Some(panel) = delete_panel.upgrade()
                                                else {
                                                    return;
                                                };
                                                let id = delete.id.clone();
                                                let name = delete.name.clone();
                                                panel.update(cx, |panel, cx| {
                                                    panel.prompt_delete(id, name, window, cx);
                                                });
                                            },
                                        ),
                                    )
                                }),
                        )
                    })
                    .child(
                        Button::new(("action-run", index))
                            .primary()
                            .xsmall()
                            .icon(Icon::from(ExpIcon::Play))
                            .tooltip("Run on this device")
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.run(run_id.clone(), window, cx);
                            })),
                    ),
            )
            .when_some(action.description.clone(), |this, description| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(SharedString::from(description)),
                )
            })
            .into_any_element()
    }
}

impl Render for ActionsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_loaded(cx);
        // Copied out (Hsla is Copy) — the theme borrow must not overlap the
        // row-render closures' mutable cx borrow.
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let owner = self
            .team_id
            .as_deref()
            .is_some_and(|team_id| crate::settings::is_owner(cx, team_id));
        let loading = matches!(self.load, Load::Loading) && self.actions.is_empty();

        let rows: Vec<gpui::AnyElement> = self
            .actions
            .clone()
            .iter()
            .enumerate()
            .map(|(index, action)| self.render_row(index, action, owner, cx))
            .collect();

        gpui_component::v_flex()
            .size_full()
            .min_h_0()
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .text_color(danger)
                        .child(error),
                )
            })
            .child(crate::scroll_pane::v_scroll_pane(
                "actions-scroll",
                &self.scroll,
                gpui_component::v_flex()
                    .p_1()
                    .gap_0p5()
                    .children(rows)
                    .when(loading, |this| {
                        this.child(
                            div()
                                .p_2()
                                .text_xs()
                                .text_color(muted)
                                .child("Loading actions…"),
                        )
                    })
                    .when(
                        !loading && self.actions.is_empty() && self.error.is_none(),
                        |this| {
                            // Practically unreachable (the server always
                            // appends the builtin) — kept for a degraded
                            // fetch against an older server.
                            this.child(
                                div()
                                    .p_2()
                                    .text_xs()
                                    .text_color(muted)
                                    .child("No actions yet."),
                            )
                        },
                    ),
            ))
    }
}

// ---------------------------------------------------------------------------
// The raw editor dialog (EXP-257: EDIT-ONLY — creation is the builtin run)
// ---------------------------------------------------------------------------

struct ActionEditorView {
    /// The edited action's id.
    editing: String,
    name: Entity<InputState>,
    description: Entity<InputState>,
    body: Entity<InputState>,
    /// The chosen repo (`None` = repo-less).
    repository: Option<ActionRepoRow>,
    /// The edited action's repo binding as loaded — an update only sends
    /// `repositoryId` when the picker actually CHANGED it (a failed or
    /// still-loading repos fetch must never silently strip the binding).
    initial_repository_id: Option<String>,
    repos: Vec<ActionRepoRow>,
    /// The picker rows landed (fetch succeeded) — the gate for trusting the
    /// picker state on save.
    repos_loaded: bool,
    submitting: bool,
    error: Option<SharedString>,
    panel: gpui::WeakEntity<ActionsPanel>,
}

impl ActionEditorView {
    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.submitting {
            return;
        }
        let name = self.name.read(cx).value().trim().to_string();
        let description = self.description.read(cx).value().trim().to_string();
        let body = self.body.read(cx).value().to_string();
        if name.is_empty() || body.trim().is_empty() {
            self.error = Some("Name and body are required.".into());
            cx.notify();
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };
        self.submitting = true;
        self.error = None;
        cx.notify();

        let editing = self.editing.clone();
        let repository_id = self.repository.as_ref().map(|repo| repo.id.clone());
        // Only a REAL picker change rides the update; an unloaded picker
        // (fetch failed / still in flight) must not clobber the binding.
        let repository_patch = if !self.repos_loaded {
            api::Patch::Omit
        } else if repository_id == self.initial_repository_id {
            api::Patch::Omit
        } else {
            api::Patch::set_or_null(repository_id.clone())
        };
        let panel = self.panel.clone();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    let mut input = api::actions::ActionUpdate::new(editing);
                    input.name = Some(name);
                    input.description = Some(description);
                    input.repository_id = repository_patch;
                    input.body = Some(body);
                    api::actions::update(&trpc, &input).map(|_| ())
                })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                this.submitting = false;
                match result {
                    Ok(()) => {
                        if let Some(panel) = panel.upgrade() {
                            panel.update(cx, |panel, cx| panel.refetch(cx));
                        }
                        window.close_dialog(cx);
                    }
                    Err(err) => {
                        this.error = Some(SharedString::from(format!("{err}")));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}

impl Render for ActionEditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let repo_label: SharedString = match &self.repository {
            Some(repo) => repo.full_name.clone().into(),
            None => "No repository".into(),
        };
        let repos = self.repos.clone();
        gpui_component::v_flex()
            .gap_2()
            .child(field_label("Name", cx))
            .child(Input::new(&self.name).small())
            .child(field_label("Description (optional)", cx))
            .child(Input::new(&self.description).small())
            .child(field_label("Repository", cx))
            .child(
                Button::new("action-repo")
                    .ghost()
                    .xsmall()
                    .label(repo_label)
                    .dropdown_menu({
                        let view = cx.entity().downgrade();
                        move |mut menu, _window, _cx| {
                            for pick in std::iter::once(None)
                                .chain((0..repos.len()).map(Some))
                            {
                                let label: SharedString = match pick {
                                    Some(index) => repos[index].full_name.clone().into(),
                                    None => "No repository".into(),
                                };
                                let view = view.clone();
                                menu = menu.item(PopupMenuItem::new(label).on_click(
                                    move |_, _, cx| {
                                        if let Some(view) = view.upgrade() {
                                            view.update(cx, |view, cx| {
                                                view.repository = pick.and_then(|index| {
                                                    view.repos.get(index).cloned()
                                                });
                                                cx.notify();
                                            });
                                        }
                                    },
                                ));
                            }
                            menu
                        }
                    }),
            )
            .child(field_label("Instructions (markdown)", cx))
            .child(Input::new(&self.body))
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.danger)
                        .child(error),
                )
            })
    }
}

fn field_label(text: &'static str, cx: &App) -> gpui::Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

fn open_action_editor(
    window: &mut Window,
    cx: &mut App,
    team_id: String,
    existing: api::actions::Action,
    panel: gpui::WeakEntity<ActionsPanel>,
) {
    let existing_repo = existing.repository_id.clone();
    let view = cx.new(|cx| ActionEditorView {
        editing: existing.id.clone(),
        name: cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("e.g. Code review");
            state.set_value(existing.name.clone(), window, cx);
            state
        }),
        description: cx.new(|cx| {
            let mut state =
                InputState::new(window, cx).placeholder("One line on what this action does");
            if let Some(description) = existing.description.clone() {
                state.set_value(description, window, cx);
            }
            state
        }),
        body: cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .rows(12)
                .placeholder("# What to do\n\nStep-by-step markdown instructions…");
            state.set_value(existing.body.clone(), window, cx);
            state
        }),
        repository: None,
        initial_repository_id: existing_repo.clone(),
        repos: Vec::new(),
        repos_loaded: false,
        submitting: false,
        error: None,
        panel,
    });

    // Fetch the repo picker's rows off the foreground; pre-select the
    // edited action's repo once they land.
    if let Some(trpc) = queries::trpc_client(cx) {
        let view_for_fetch = view.downgrade();
        cx.spawn(async move |cx| {
            let rows = cx
                .background_executor()
                .spawn(async move { fetch_repositories(&trpc, &team_id) })
                .await;
            let _ = cx.update(|cx| {
                if let Some(view) = view_for_fetch.upgrade() {
                    view.update(cx, |view, cx| match rows {
                        Ok(rows) => {
                            view.repository = existing_repo
                                .as_deref()
                                .and_then(|id| rows.iter().find(|row| row.id == id).cloned());
                            view.repos = rows;
                            view.repos_loaded = true;
                            cx.notify();
                        }
                        Err(err) => {
                            // Leave repos_loaded=false — save then omits the
                            // binding instead of stripping it.
                            log::warn!("actions: repositories.list failed: {err}");
                            cx.notify();
                        }
                    });
                }
            });
        })
        .detach();
    }

    window.open_dialog(cx, move |dialog, _window, cx| {
        let busy = view.read(cx).submitting;
        dialog
            .w(px(560.))
            .title("Edit action")
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
