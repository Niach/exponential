//! The Actions tool window (EXP-253): the team's reusable markdown prompts —
//! list + ▶ Run and owner-only Edit/Delete. EXP-257: creation moved into the
//! virtual **"Create action"** builtin (pinned first in this list; its run IS
//! the creator — an MCP-wired agent session authoring the action), so the old
//! "Describe with Claude"/"Write manually" headers and the local templates
//! are gone. EXP-268: the list is LIVE — it reads the synced `actions` shape
//! (body-less rows; the editor fetches the body via `actions.get` on open),
//! so an MCP-created action appears without any refetch machinery. Run opens
//! the unified Start-coding dialog's Actions tab
//! ([`crate::start_coding_dialog::open_for_action`]), which owns
//! agent/model/effort choices and the typed input fields; the runner itself
//! lives in [`crate::action_run`].

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, AppContext as _, ClickEvent, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
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

pub struct ActionsPanel {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl ActionsPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        // Live list: re-render on any synced actions change (EXP-268) and on
        // navigation (team switch re-scopes the read).
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        let actions_collection =
            sync::Store::try_global(cx).map(|store| store.collections().actions.clone());
        if let Some(collection) = actions_collection {
            subscriptions.push(cx.observe(&collection, |_, _, cx| cx.notify()));
        }
        Self {
            nav,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    fn team_id(&self, cx: &App) -> Option<String> {
        active_team_id(&self.nav, cx)
    }

    /// ▶ Run — open the unified Start-coding dialog's Actions tab with this
    /// action preselected (EXP-257: the dialog owns agent/model/effort and
    /// the typed input fields).
    fn run(&mut self, action_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.team_id(cx) else {
            return;
        };
        crate::start_coding_dialog::open_for_action(window, cx, team_id, action_id);
    }

    /// Owner Delete, behind a confirm (destructive native actions confirm
    /// first — the client contract). The synced collection drops the row —
    /// no refetch needed.
    fn prompt_delete(
        &mut self,
        action_id: String,
        name: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        prompt_delete_action(window, cx, action_id, name);
    }

    // -- render -------------------------------------------------------------

    fn render_row(
        &self,
        index: usize,
        action: &api::actions::Action,
        owner: bool,
        team_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let run_id = action.id.clone();
        let edit_action = action.clone();
        let edit_team_id = team_id.to_string();
        let repo_backed = action.repository_id.is_some();
        let builtin = action.builtin;
        // EXP-277: the row itself navigates — real actions open the detail
        // screen; builtins (no stable body) open the start dialog directly.
        let click_id = action.id.clone();

        gpui_component::v_flex()
            .id(SharedString::from(format!("action-{}", action.id)))
            .w_full()
            .gap_0p5()
            .px_2()
            .py_1p5()
            .rounded(theme.radius)
            .hover(|this| this.bg(theme.list_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                if builtin {
                    this.run(click_id.clone(), window, cx);
                } else {
                    crate::navigation::navigate(
                        window,
                        cx,
                        crate::navigation::Screen::ActionDetail {
                            action_id: click_id.clone(),
                        },
                    );
                }
            }))
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
                            // Swallow the press so opening the menu never
                            // also fires the row navigation (EXP-277).
                            div()
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    |_, _, cx: &mut App| cx.stop_propagation(),
                                )
                                .child(
                                    Button::new(("action-menu", index))
                                .ghost()
                                .xsmall()
                                .icon(IconName::Ellipsis)
                                .dropdown_menu(move |menu, _window, _cx| {
                                    // Direct closures (the members-menu
                                    // pattern) — never App-global dispatch
                                    // from an overlay into an unfocused view.
                                    let edit = edit_action.clone();
                                    let edit_team_id = edit_team_id.clone();
                                    let delete = edit_action.clone();
                                    let delete_panel = panel.clone();
                                    menu.item(
                                        PopupMenuItem::new("Edit…").on_click(
                                            move |_, window, cx| {
                                                open_action_editor(
                                                    window,
                                                    cx,
                                                    edit_team_id.clone(),
                                                    edit.clone(),
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
                                ),
                        )
                    })
                    .child(
                        Button::new(("action-run", index))
                            .primary()
                            .xsmall()
                            .icon(Icon::from(ExpIcon::Play))
                            .tooltip("Run on this device")
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                cx.stop_propagation();
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
        // Copied out (Hsla is Copy) — the theme borrow must not overlap the
        // row-render closures' mutable cx borrow.
        let muted = cx.theme().muted_foreground;
        let team_id = self.team_id(cx);
        let owner = team_id
            .as_deref()
            .is_some_and(|team_id| crate::settings::is_owner(cx, team_id));
        let (actions, ready) = match team_id.as_deref() {
            Some(team_id) => queries::team_actions(cx, team_id),
            None => (Vec::new(), true),
        };
        let loading = !ready;
        let team_id = team_id.unwrap_or_default();

        let rows: Vec<gpui::AnyElement> = actions
            .iter()
            .enumerate()
            .map(|(index, action)| self.render_row(index, action, owner, &team_id, cx))
            .collect();

        gpui_component::v_flex()
            .size_full()
            .min_h_0()
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
                    }),
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
    /// The body landed from `actions.get` (EXP-268: synced rows carry no
    /// body) — save is refused until then so it can never blank the prompt.
    body_loaded: bool,
    submitting: bool,
    error: Option<SharedString>,
}

impl ActionEditorView {
    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.submitting {
            return;
        }
        if !self.body_loaded {
            self.error = Some("Still loading the prompt — try again in a moment.".into());
            cx.notify();
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
                    // The synced collection picks the change up — no refetch.
                    Ok(()) => window.close_dialog(cx),
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

/// The owner delete confirm + `actions.delete` (shared by the tool-window
/// menu and the action-detail sidebar — EXP-277).
pub(crate) fn prompt_delete_action(
    window: &mut Window,
    cx: &mut App,
    action_id: String,
    name: String,
) {
    window.open_alert_dialog(cx, move |alert, _window, _cx| {
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
                let action_id = action_id.clone();
                cx.spawn(async move |cx| {
                    let result = cx
                        .background_executor()
                        .spawn(async move { api::actions::delete(&trpc, &action_id) })
                        .await;
                    let _ = cx.update(|_| {
                        if let Err(err) = result {
                            log::warn!("actions: delete failed: {err}");
                        }
                    });
                })
                .detach();
                true
            })
    });
}

fn field_label(text: &'static str, cx: &App) -> gpui::Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

/// The owner edit dialog (`pub(crate)` — the action-detail sidebar reuses
/// it, EXP-277).
pub(crate) fn open_action_editor(
    window: &mut Window,
    cx: &mut App,
    team_id: String,
    existing: api::actions::Action,
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
            InputState::new(window, cx)
                .multi_line(true)
                .rows(12)
                .placeholder("Loading prompt…")
        }),
        repository: None,
        initial_repository_id: existing_repo.clone(),
        repos: Vec::new(),
        repos_loaded: false,
        body_loaded: false,
        submitting: false,
        error: None,
    });

    // The synced row carries no body (EXP-268) — fetch the fresh one and
    // seed the prompt field once it lands.
    if let Some(trpc) = queries::trpc_client(cx) {
        let view_for_body = view.downgrade();
        let action_id = existing.id.clone();
        let window_handle = window.window_handle();
        cx.spawn(async move |cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::actions::get(&trpc, &action_id) })
                .await;
            let _ = window_handle.update(cx, |_, window, cx| {
                let Some(view) = view_for_body.upgrade() else {
                    return;
                };
                view.update(cx, |view, cx| match result {
                    Ok(action) => {
                        view.body.update(cx, |state, cx| {
                            state.set_value(action.body.clone(), window, cx);
                        });
                        view.body_loaded = true;
                        cx.notify();
                    }
                    Err(err) => {
                        view.error = Some(SharedString::from(format!(
                            "Could not load the prompt: {err}"
                        )));
                        cx.notify();
                    }
                });
            });
        })
        .detach();
    }

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
