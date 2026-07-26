//! The Actions tool window (EXP-253): the team's reusable markdown prompts —
//! list + ▶ Run and owner-only Delete. EXP-257: creation moved into the
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
//!
//! EXP-282: the rows are FLAT and full-width (the issue-list shape — the
//! container lost its padding, the row carries `px_3` and the hover spans
//! edge to edge) with real overflow discipline, and the raw editor DIALOG is
//! gone: editing an action is inline on [`crate::action_detail`], so the
//! owner menu only opens the detail or deletes.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, App, ClickEvent, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    menu::{DropdownMenu as _, PopupMenuItem},
    ActiveTheme as _, Icon, IconName, Sizable as _,
};

use crate::icons::ExpIcon;
use crate::native_dialog::{self, AlertSpec};
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
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let run_id = action.id.clone();
        let menu_action = action.clone();
        let repo_backed = action.repository_id.is_some();
        let builtin = action.builtin;
        // EXP-277: the row itself navigates — real actions open the detail
        // screen; builtins (no stable body) open the start dialog directly.
        let click_id = action.id.clone();

        gpui_component::v_flex()
            .id(SharedString::from(format!("action-{}", action.id)))
            // EXP-282: flat full-width row (issue-list shape) — no inset
            // pill, so the hover fill spans the whole tool column. `min_w_0`
            // all the way down is what keeps a narrow sidebar from pushing
            // the trailing buttons out of the box.
            .w_full()
            .min_w_0()
            .gap_0p5()
            .px_3()
            .py_1p5()
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
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_1()
                    .child(
                        // The builtin creator gets its distinct mark (Plus,
                        // like the web's ActionCard) — real actions keep Zap.
                        // EXP-282: every leading/trailing ornament sits in a
                        // `flex_shrink_0` box so only the NAME gives way.
                        div().flex_shrink_0().child(if builtin {
                            Icon::new(IconName::Plus)
                                .xsmall()
                                .text_color(theme.muted_foreground)
                        } else {
                            Icon::from(ExpIcon::Zap)
                                .xsmall()
                                .text_color(theme.muted_foreground)
                        }),
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
                            div().flex_shrink_0().child(
                                Icon::from(ExpIcon::GitMerge)
                                    .xsmall()
                                    .text_color(theme.muted_foreground),
                            ),
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
                                .flex_shrink_0()
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
                                    // EXP-282: "Edit…" is gone with the raw
                                    // editor dialog — editing is inline on
                                    // the detail screen, so the menu just
                                    // opens it (same target as a row click).
                                    let open = menu_action.clone();
                                    let delete = menu_action.clone();
                                    let delete_panel = panel.clone();
                                    menu.item(
                                        PopupMenuItem::new("Open action").on_click(
                                            move |_, window, cx| {
                                                crate::navigation::navigate(
                                                    window,
                                                    cx,
                                                    crate::navigation::Screen::ActionDetail {
                                                        action_id: open.id.clone(),
                                                    },
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
                        div().flex_shrink_0().child(
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
                    ),
            )
            .when_some(action.description.clone(), |this, description| {
                this.child(
                    // EXP-282: one clamped line — a long description used to
                    // wrap the row into a paragraph at narrow widths.
                    div()
                        .w_full()
                        .min_w_0()
                        .truncate()
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

        let rows: Vec<gpui::AnyElement> = actions
            .iter()
            .enumerate()
            .map(|(index, action)| self.render_row(index, action, owner, cx))
            .collect();

        gpui_component::v_flex()
            .size_full()
            .min_h_0()
            .child(crate::scroll_pane::v_scroll_pane(
                "actions-scroll",
                &self.scroll,
                // EXP-282: no horizontal padding and no inter-row gap — the
                // rows own their `px_3` so hover/selection runs edge to edge
                // (issue-list parity).
                gpui_component::v_flex()
                    .py_1()
                    .children(rows)
                    .when(loading, |this| {
                        this.child(
                            div()
                                .px_3()
                                .py_2()
                                .text_xs()
                                .text_color(muted)
                                .child("Loading actions…"),
                        )
                    }),
            ))
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
    let spec = AlertSpec::new(
        format!("Delete \"{name}\"?"),
        "Team members will no longer be able to run this action. \
         A live run keeps going and keeps its label.",
        "Delete action",
    )
    .on_ok(move |_, cx| {
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
    });
    native_dialog::open_alert(window, cx, spec);
}
