//! The Actions center screen (EXP-467): the web `t/$teamSlug/actions` page
//! 1:1 — the team's reusable action prompts as a ROW list (EXP-618: the old
//! wrapping card grid unified onto the mobile row shape every other client
//! draws). The old master/detail split (Actions tool window → full-page
//! action detail) is gone: editing happens in
//! [`crate::action_editor_dialog`] behind each row's owner ⋯ menu, exactly
//! like the web's edit dialog, and the rail's Actions entry navigates here.
//! EXP-480: the page is a tab-less FULL-PAGE mode (no tool column, no tab
//! chip — `CenterPanel` unmounts the sidebar split while it is up), leading
//! with the web's plain-text [`crate::surface::glass_section_header`] over
//! a GAPPED list of [`crate::surface::glass_row_card`] rows (EXP-642).
//!
//! EXP-686 split the old three-tab page apart: machines moved to
//! [`crate::devices_view`], automations + their run log to
//! [`crate::automations_view`], and the suggestion seeds to the
//! Getting-started page's second tab (the header's lightbulb goes there).
//!
//! EXP-431 carries over: the create builtin is not a row — creation lives
//! behind the header's "New action" button, which opens the Agent page
//! composer with the creator builtin picked (EXP-825). The list is LIVE off
//! the synced `actions` shape (body-less rows; the edit dialog fetches the
//! body via `actions.get` on open). ▶ Run opens the same composer with the
//! action picked; it owns agent/model/effort and the typed input fields.
//!
//! EXP-832: `render` builds elements and NOTHING else. gpui re-renders the
//! whole window on any `notify` (a tooltip's show-delay task, a hover on a
//! managed control), so the list's derived data — the team's rows off the
//! synced collections, converted and sorted, plus the per-action automation
//! count — is computed ONCE per data change in the `observe` callbacks
//! ([`ActionsView::refresh`]) and read from `self` by `render`. The row's
//! ▶ button carries a tooltip only while it is DISABLED (the EXP-367 reason):
//! an enabled button's "Run on this device" cost ~70 full-window renders per
//! pointer sweep over the column (the managed tooltip's delay + slide tasks
//! each `notify`); without it the same sweep measures ~13.

use std::collections::HashMap;

use gpui::{
    div, px, App, ClickEvent, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled, Subscription,
    Window,
};
use gpui_component::{
    button::ButtonVariant,
    menu::{DropdownMenu as _, PopupMenuItem},
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::navigation::{
    active_team_id, nav_for_window, navigate, GettingStartedTab, Navigation, Screen,
};
use crate::native_dialog::{self, AlertSpec};
use crate::queries;

/// The page column's width cap — the web page's `md:max-w-5xl`.
const PAGE_COLUMN_W: f32 = 1024.;

/// The shared full-page scaffold every rail-navigated page uses (EXP-686 —
/// Devices, Actions, Automations): ONE scroll pane holding one centered
/// column capped at [`PAGE_COLUMN_W`] (block wrapper + `mx_auto`, the
/// EXP-179-safe centering recipe).
///
/// NO `w_full` on the column's CHILDREN (EXP-508): a percent width there
/// resolves against the UNCLAMPED ancestor available width, so sections
/// shrink-wrap and content runs off the window (the EXP-436 leak; EXP-179 has
/// the same drop-`w_full` fix). Auto width + the column's flex-col stretch
/// resolve the capped column width at every panel width. `min_w_0` rides
/// every flex hop for the same reason.
pub(crate) fn page_scaffold(
    id: &'static str,
    scroll: &ScrollHandle,
    column: gpui::Div,
) -> impl IntoElement {
    page_scaffold_with(id, scroll, column, PAGE_COLUMN_W)
}

/// [`page_scaffold`] with an explicit column cap — the Reviews page (EXP-706)
/// reads as a narrower list than the settings-shaped pages, matching the web
/// route's `max-w-3xl`. Same flex rules apply verbatim; only the cap moves.
pub(crate) fn page_scaffold_with(
    id: &'static str,
    scroll: &ScrollHandle,
    column: gpui::Div,
    width: f32,
) -> impl IntoElement {
    gpui_component::v_flex()
        .size_full()
        .min_h_0()
        .min_w_0()
        .child(crate::scroll_pane::v_scroll_pane(
            id,
            scroll,
            div().w_full().min_w_0().child(
                column
                    .w_full()
                    .min_w_0()
                    .px_4()
                    .py_4()
                    .max_w(px(width))
                    .mx_auto(),
            ),
        ))
}

/// The subtle icon-only lightbulb every list header carries next to its
/// "New …" button (EXP-686): the curated action suggestions moved to the
/// Getting-started page's second tab, and this is the way back to them from
/// the list they seed.
///
/// EXP-697: it wears the shared round glass affordance
/// ([`crate::controls::glass_icon_button`]) instead of a bare ghost icon, so
/// it reads as a control next to the outlined "New …" button.
///
/// EXP-827: at the PILL's height (`CONTROL_SM`, 24px — `web_icon_xs`), not the
/// 32px the shared affordance defaults to: it sits beside a `PillSize::Sm`
/// "New …" pill and a circle a third taller than its neighbour read as the
/// header's main control rather than the quiet way back to the suggestions.
pub(crate) fn suggestions_button(id: &'static str, cx: &App) -> gpui::AnyElement {
    crate::controls::glass_icon_button(id, Icon::from(registry::ACTION_SUGGESTION), cx)
        .web_icon_xs()
        .tooltip("Suggestions")
        .on_click(|_: &ClickEvent, window, cx| {
            navigate(
                window,
                cx,
                Screen::GettingStarted {
                    tab: GettingStartedTab::Suggestions,
                },
            );
        })
        .into_any_element()
}

pub struct ActionsView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    /// EXP-832: what the page shows, derived off the synced collections in
    /// [`Self::refresh`] — never in `render`.
    derived: ActionsDerived,
    _subscriptions: Vec<Subscription>,
}

/// EXP-832: the Actions page's data, ready to draw.
#[derive(Default)]
struct ActionsDerived {
    /// The team the rows belong to (the active team at the last refresh).
    team_id: Option<String>,
    /// The team's actions in list order, the two builtins already dropped.
    actions: Vec<api::actions::Action>,
    /// The `actions` shape's readiness (an empty list before it is "still
    /// syncing", never "no actions").
    ready: bool,
    /// EXP-583: how many automations target each action, by action id.
    automation_counts: HashMap<String, usize>,
}

impl ActionsDerived {
    /// The page's data for `team_id` (none when no team is active).
    fn compute(cx: &App, team_id: Option<String>) -> Self {
        let Some(team) = team_id.as_deref() else {
            return Self { ready: true, ..Self::default() };
        };
        let (mut actions, ready) = queries::team_actions(cx, team);
        // EXP-431/686: NEITHER builtin is a row here. Creation lives behind
        // the header's "New action" button, and "Fix merge conflicts" is
        // launched from Reviews (or MCP), never picked off this list.
        // Filtered HERE, not in `queries::team_actions`: that pool must keep
        // both — the Reviews entry point and the Start-coding dialog's
        // preselect dead-end in `select_action` without them.
        actions.retain(|action| {
            action.id != api::actions::BUILTIN_CREATE_ACTION_ID
                && action.id != api::actions::BUILTIN_FIX_CONFLICTS_ID
        });
        // EXP-583: automations are their own synced rows — each action row
        // says how many target it (the list itself lives on its own screen).
        let (automations, _) = queries::team_automations(cx, team);
        let mut automation_counts: HashMap<String, usize> = HashMap::new();
        for automation in &automations {
            *automation_counts.entry(automation.action_id.clone()).or_default() += 1;
        }
        Self {
            team_id,
            actions,
            ready,
            automation_counts,
        }
    }
}

impl ActionsView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        // Live list: re-derive on any synced actions change (EXP-268) and on
        // navigation (team switch re-scopes the read).
        let mut subscriptions = vec![cx.observe(&nav, |this, _, cx| this.refresh(cx))];
        // EXP-583: each row says how many automations target it, so the
        // `automations` rows drive this screen too.
        let watched = sync::Store::try_global(cx).map(|store| {
            let collections = store.collections();
            (
                collections.actions.clone(),
                collections.automations.clone(),
                collections.pins.clone(),
            )
        });
        if let Some((actions, automations, pins)) = watched {
            subscriptions.push(cx.observe(&actions, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&automations, |this, _, cx| this.refresh(cx)));
            // EXP-778: each row's Pin/Unpin menu entry reads the per-user
            // pins rows.
            subscriptions.push(cx.observe(&pins, |this, _, cx| this.refresh(cx)));
        }
        let derived = ActionsDerived::compute(cx, active_team_id(&nav, cx));
        Self {
            nav,
            scroll: ScrollHandle::new(),
            derived,
            _subscriptions: subscriptions,
        }
    }

    fn team_id(&self, cx: &App) -> Option<String> {
        active_team_id(&self.nav, cx)
    }

    /// EXP-832: re-derive the page off the collections and repaint — the
    /// ONE place the rows are computed.
    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.derived = ActionsDerived::compute(cx, self.team_id(cx));
        cx.notify();
    }

    /// ▶ Run — open the Agent page composer with this action picked
    /// (EXP-825: the composer owns agent/model/effort and the typed input
    /// fields).
    fn run(&mut self, action_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.team_id(cx) else {
            return;
        };
        // EXP-367: belt to the disabled button — no agent CLI, nothing to run.
        if crate::coding_flow::no_agent_reason(cx).is_some() {
            return;
        }
        let _ = team_id;
        crate::navigation::navigate_to_chat(
            window,
            cx,
            crate::navigation::ChatSeed::action(action_id),
        );
    }

    /// Destructive native actions confirm first — the web delete dialog's
    /// copy behind the shared alert window (the machines Remove pattern).
    fn prompt_delete(
        &mut self,
        action_id: String,
        name: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let spec = AlertSpec::new(
            "Delete action",
            format!(
                "Delete \"{name}\"? Live runs keep going and keep their label; \
                 this cannot be undone."
            ),
            "Delete",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            spawn_action_delete(cx, action_id.clone());
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    // -- render -------------------------------------------------------------

    /// One action row — the mobile/web `ActionRow` shape (EXP-618): glyph ·
    /// [name, 2-line description, automation count] · ▶ Run · owner ⋯ menu.
    fn render_action_row(
        &self,
        index: usize,
        action: &api::actions::Action,
        automations: usize,
        is_owner: bool,
        no_agent: Option<&SharedString>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        // EXP-811: the ONE row hover, `list_hover` (glass fillRow) on every client.
        let row_hover = theme.list_hover;
        let run_id = action.id.clone();

        // EXP-697 retired the FEED-15 "runs in a repository" glyph: the name
        // stands alone.
        let title_row = gpui_component::h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_1p5()
            .child(
                div()
                    .min_w_0()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .truncate()
                    .text_color(theme.foreground)
                    .child(SharedString::from(action.name.clone())),
            );

        let mut middle = gpui_component::v_flex()
            .flex_1()
            .min_w_0()
            .gap_0p5()
            .child(title_row);
        if let Some(description) = action.description.clone() {
            middle = middle.child(
                div()
                    .w_full()
                    .min_w_0()
                    .text_xs()
                    .text_color(muted)
                    .line_clamp(2)
                    .child(SharedString::from(description)),
            );
        }
        // EXP-583: the row no longer shows ONE trigger — automations are
        // their own rows, and several can target the same action. It says
        // how many, and the Automations tab shows which.
        if automations > 0 {
            middle = middle.child(
                gpui_component::h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_1p5()
                    .child(
                        Icon::from(registry::ACTION_AUTOMATION)
                            .xsmall()
                            .text_color(muted),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .truncate()
                            .text_color(muted)
                            .child(SharedString::from(if automations == 1 {
                                "1 automation".to_string()
                            } else {
                                format!("{automations} automations")
                            })),
                    ),
            );
        }

        // EXP-367: no agent CLI → Run disabled with the reason, never hidden.
        // EXP-832: the reason is the ONLY tooltip the button wears — an
        // enabled ▶ with a managed tooltip re-rendered the whole window on
        // every hover and kept rendering while the pointer parked on it.
        let mut run_button = crate::controls::glass_icon_button(
            ("action-run", index),
            Icon::from(registry::ACTION_RUN),
            cx,
        )
        .disabled(no_agent.is_some())
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            cx.stop_propagation();
            this.run(run_id.clone(), window, cx);
        }));
        if let Some(reason) = no_agent {
            run_button = run_button.tooltip(reason.clone());
        }
        let mut row = crate::surface::flat_row()
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2p5()
            .hover(move |this| this.bg(row_hover))
            .child(
                div().flex_shrink_0().child(
                    crate::icons::action_icon(action.icon.as_deref())
                        .xsmall()
                        .text_color(muted),
                ),
            )
            .child(middle)
            // EXP-615/686: the shared round glass ▶ (web/mobile parity).
            .child(run_button);
        // The ⋯ menu renders on non-builtin rows — builtins have no
        // editable row, no delete, and (EXP-778) no pin (they are not DB
        // rows). Pin/Unpin is personal, so every member gets it; Edit and
        // Delete stay owner-only (web parity).
        if !action.builtin {
            let edit_id = action.id.clone();
            let delete_view = cx.entity().downgrade();
            let delete_id = action.id.clone();
            let delete_name = action.name.clone();
            let pin_team_id = self.team_id(cx);
            let pinned = crate::pins::is_pinned(
                domain::contract::PIN_KIND_ACTION,
                &action.id,
                cx,
            );
            row = row.child(
                div().flex_shrink_0().child(
                    // EXP-698: the one 32px glass chrome every row action wears.
                    crate::controls::glass_icon_button(
                        ("action-menu", index),
                        Icon::from(registry::UI_MORE),
                        cx,
                    )
                        .dropdown_menu(move |mut menu, _window, cx| {
                            let edit_id = edit_id.clone();
                            let delete_view = delete_view.clone();
                            let delete_id = delete_id.clone();
                            let delete_name = delete_name.clone();
                            if let Some(team_id) = pin_team_id.clone() {
                                let pin_id = edit_id.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(if pinned { "Unpin" } else { "Pin" })
                                        .icon(Icon::from(if pinned {
                                            registry::UI_UNPIN
                                        } else {
                                            registry::UI_PIN
                                        }))
                                        .on_click(move |_, _window, cx| {
                                            crate::pins::toggle_pin(
                                                team_id.clone(),
                                                domain::contract::PIN_KIND_ACTION,
                                                pin_id.clone(),
                                                cx,
                                            );
                                        }),
                                );
                            }
                            if !is_owner {
                                return menu;
                            }
                            menu.item(
                                PopupMenuItem::new("Edit")
                                    .icon(Icon::from(registry::UI_EDIT))
                                    .on_click(move |_, window, cx| {
                                        crate::action_editor_dialog::open(
                                            window,
                                            cx,
                                            edit_id.clone(),
                                        );
                                    }),
                            )
                            .item(
                                crate::controls::danger_menu_item(
                                    "Delete",
                                    Icon::from(registry::UI_DELETE),
                                    cx,
                                )
                                    .on_click(move |_, window, cx| {
                                        let Some(view) = delete_view.upgrade() else {
                                            return;
                                        };
                                        let id = delete_id.clone();
                                        let name = delete_name.clone();
                                        view.update(cx, |this, cx| {
                                            this.prompt_delete(id, name, window, cx);
                                        });
                                    }),
                            )
                        }),
                ),
            );
        }
        row.into_any_element()
    }

    /// The web `NoCustomActionsNudge`: a dashed full-width strip below the
    /// list that opens the creator run, shown while every listed action is a
    /// builtin (EXP-618 — the old grid cell, stretched to the list shape).
    fn render_nudge(&self, team_id: String, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let hover = cx.theme().list_hover;
        div()
            .id("actions-empty-nudge")
            .w_full()
            .min_w_0()
            .rounded(px(theme::tokens::radius::MD))
            .border_1()
            .border_dashed()
            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
            .p_3()
            .cursor_pointer()
            .hover(move |this| this.bg(hover))
            .on_click(move |_: &ClickEvent, window, cx| {
                let _ = &team_id;
                crate::navigation::navigate_to_chat(
                    window,
                    cx,
                    crate::navigation::ChatSeed::action(api::actions::BUILTIN_CREATE_ACTION_ID),
                );
            })
            .child(
                gpui_component::v_flex()
                    .gap_1()
                    .child(
                        gpui_component::h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Icon::from(registry::ACTION_CREATE)
                                    .xsmall()
                                    .text_color(muted),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .child("No custom actions yet"),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("Describe one and your agent will build it."),
                    ),
            )
            .into_any_element()
    }
}

impl Render for ActionsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        // EXP-832: the rows were derived when the data last changed
        // (`refresh`); this only draws them. The team is read off the
        // derivation too, so a header and its rows never disagree.
        let team_id = self.derived.team_id.clone();
        let loading = !self.derived.ready;
        let is_owner = team_id
            .as_deref()
            .is_some_and(|team_id| crate::settings::is_owner(cx, team_id));
        let has_custom = self.derived.actions.iter().any(|action| !action.builtin);

        // Owner-only "New action" (EXP-367: disabled with the reason when no
        // agent CLI is installed, never hidden). Read ONCE per render for the
        // header and every row.
        let no_agent = crate::coding_flow::no_agent_reason(cx);
        let new_action = is_owner
            .then(|| team_id.clone())
            .flatten()
            .map(|new_team| {
                crate::surface::glass_pill_button("actions-new", crate::surface::PillSize::Sm, cx)
                    .icon(Icon::from(registry::ACTION_CREATE))
                    .label("New action")
                    .tooltip(no_agent.clone().unwrap_or_else(|| "New action".into()))
                    .disabled(no_agent.is_some())
                    .on_click(move |_, window, cx| {
                        let _ = &new_team;
                        crate::navigation::navigate_to_chat(
                            window,
                            cx,
                            crate::navigation::ChatSeed::action(
                                api::actions::BUILTIN_CREATE_ACTION_ID,
                            ),
                        );
                    })
                    .into_any_element()
            });
        // EXP-686: the lightbulb sits left of "New action" and leads to the
        // suggestion seeds on the Getting-started page.
        let trailing = gpui_component::h_flex()
            .items_center()
            .gap_1()
            .child(suggestions_button("actions-suggestions", cx))
            .children(new_action)
            .into_any_element();
        let header =
            crate::surface::glass_section_header("Actions", Some(trailing), cx);

        let rows: Vec<gpui::AnyElement> = self
            .derived
            .actions
            .iter()
            .enumerate()
            .map(|(index, action)| {
                let count = self
                    .derived
                    .automation_counts
                    .get(&action.id)
                    .copied()
                    .unwrap_or(0);
                self.render_action_row(index, action, count, is_owner, no_agent.as_ref(), cx)
            })
            .collect();
        // The nudge is a full-width strip (web parity) — appended after the
        // list.
        let nudge = (!loading && is_owner && !has_custom)
            .then(|| team_id.clone())
            .flatten()
            .map(|team_id| self.render_nudge(team_id, cx));

        // EXP-642: each row is its OWN card — a gapped column, no fused
        // bordered block. The rows render as soon as they hydrate; readiness
        // only appends the loading note (the tool-window list's behavior —
        // never blank a list that already has data).
        let mut body = gpui_component::v_flex().min_w_0().gap_2();
        if !rows.is_empty() {
            body = body.child(gpui_component::v_flex().min_w_0().children(rows));
        }
        // NO gap on the section (EXP-697): the header's own `pb_2` IS the
        // 8px to the list — a gap here doubles it. The rows keep their gap
        // inside `body`.
        let mut actions_section = gpui_component::v_flex()
            .min_w_0()
            .child(header)
            .child(body.children(nudge));
        if loading {
            actions_section = actions_section.child(
                div()
                    .pt_2()
                    .text_xs()
                    .text_color(muted)
                    .child("Loading actions…"),
            );
        }

        page_scaffold(
            "actions-screen-scroll",
            &self.scroll,
            gpui_component::v_flex().gap_6().child(actions_section),
        )
    }
}

/// `actions.delete` over tRPC — the synced collection drops the row; a live
/// run keeps going and keeps its label.
pub(crate) fn spawn_action_delete(cx: &mut App, action_id: String) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
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
}
