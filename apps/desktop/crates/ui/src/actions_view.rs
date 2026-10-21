//! The Actions center screen (EXP-467): the web `t/$teamSlug/actions` page
//! 1:1 — the team's SAVED PROMPTS (EXP-1294) as a ROW list (EXP-618). A row
//! opens the action's PAGE ([`crate::action_view`]: its prompt), and the
//! rail's Actions entry navigates here.
//! EXP-480: the page is a tab-less FULL-PAGE mode, leading with the web's
//! plain-text [`crate::surface::glass_section_header`] over a GAPPED list of
//! rows (EXP-642).
//!
//! NEW-2: the list is LIVE off the synced `prompts` shape (body-less rows;
//! the page fetches the body via `prompts.get` on open). ▶ Run opens the
//! composer with the prompt picked; it owns agent/model/effort. NEW-6 moves
//! prompts into the composer's `/` menu and Settings → Prompts.
//!
//! EXP-832: `render` builds elements and NOTHING else — the team's rows are
//! computed ONCE per data change in the `observe` callbacks
//! ([`ActionsView::refresh`]) and read from `self` by `render`. The row's
//! ▶ button carries a tooltip only while it is DISABLED (the EXP-367 reason).

use gpui::{
    div, px, App, ClickEvent, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled, Subscription,
    Window,
};
use gpui_component::{
    menu::DropdownMenu as _,
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use crate::icons::registry;
use crate::navigation::{
    active_team_id, nav_for_window, navigate, Navigation, Screen,
};
use crate::native_dialog::{self, AlertSpec};
use crate::queries;

/// An action row's height (`py_2p5` around a title and a two-line
/// description): the first load's skeleton rows take it (EXP-1259 D8).
const ACTION_ROW_H: f32 = 56.;

/// The page column's width cap — the web page's `md:max-w-5xl`.
const PAGE_COLUMN_W: f32 = 1024.;

/// The shared full-page scaffold every rail-navigated page uses (EXP-686 —
/// Devices, Actions, an action's page): ONE scroll pane holding one centered
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

/// [`page_scaffold`] with an explicit column cap — a list page (EXP-706)
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
    /// The team's actions in list order.
    actions: Vec<api::prompts::Prompt>,
    /// The `actions` shape's readiness (an empty list before it is "still
    /// syncing", never "no actions").
    ready: bool,
}

impl ActionsDerived {
    /// The page's data for `team_id` (none when no team is active).
    fn compute(cx: &App, team_id: Option<String>) -> Self {
        let Some(team) = team_id.as_deref() else {
            return Self { ready: true, ..Self::default() };
        };
        let (actions, ready) = queries::team_prompts(cx, team);
        Self {
            team_id,
            actions,
            ready,
        }
    }
}

impl ActionsView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        // Live list: re-derive on any synced actions change (EXP-268) and on
        // navigation (team switch re-scopes the read).
        let mut subscriptions = vec![cx.observe(&nav, |this, _, cx| this.refresh(cx))];
        let watched = sync::Store::try_global(cx).map(|store| {
            let collections = store.collections();
            (collections.prompts.clone(), collections.pins.clone())
        });
        if let Some((actions, pins)) = watched {
            subscriptions.push(cx.observe(&actions, |this, _, cx| this.refresh(cx)));
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
        // EXP-367: belt to the disabled button — no agent CLI, nothing to run.
        if crate::coding_flow::no_agent_reason(cx).is_some() {
            return;
        }
        crate::navigation::navigate_to_chat(
            window,
            cx,
            crate::navigation::ChatSeed::action(action_id),
        );
    }

    /// Destructive native actions confirm first: the prompts fixture's
    /// `delete-action` (EXP-1230) behind the shared alert window.
    fn prompt_delete(
        &mut self,
        action_id: String,
        name: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let spec = AlertSpec::from_prompt("Delete action", &domain::prompts::delete_action(&name))
        .on_ok(move |_, cx| {
            spawn_action_delete(cx, action_id.clone());
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    // -- render -------------------------------------------------------------

    /// One action row — the mobile/web `ActionRow` shape (EXP-618): glyph ·
    /// [name, 2-line description] · ▶ Run · ⋯ menu.
    fn render_action_row(
        &self,
        index: usize,
        action: &api::prompts::Prompt,
        is_owner: bool,
        no_agent: Option<&SharedString>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        // EXP-811: the ONE row hover, `list_hover` (glass fillRow) on every client.
        let row_hover = theme.list_hover;
        let run_id = action.id.clone();

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

        let middle = gpui_component::v_flex()
            .flex_1()
            .min_w_0()
            .gap_0p5()
            .child(title_row);
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
        // SLOP-2: a row opens the action's PAGE — for every member (a reader
        // gets the prompt read-only).
        let row_edit_id = action.id.clone();
        let row = crate::surface::flat_row()
            .id(("action-row", index))
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2p5()
            .hover(move |this| this.bg(row_hover))
            .cursor_pointer()
            .on_click(move |_: &ClickEvent, window, cx| {
                open_action_page(window, cx, row_edit_id.clone());
            })
            .child(
                div().flex_shrink_0().child(
                    crate::icons::action_icon(action.icon.as_deref())
                        .xsmall()
                        .text_color(muted),
                ),
            )
            .child(middle);
        // The ▶ and the menu are their own targets; the row's click must not
        // fire underneath them (web parity).
        let mut controls = gpui_component::h_flex()
            .id(("action-row-controls", index))
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .on_click(|_: &ClickEvent, _, cx| cx.stop_propagation())
            // EXP-615/686: the shared round glass ▶ (web/mobile parity).
            .child(run_button);
        // Pin/Unpin is personal, so every member gets it; Edit and Delete
        // stay owner-only (web parity).
        {
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
            controls = controls.child(
                div().flex_shrink_0().child(
                    // EXP-862: a row's "..." is a GHOST glyph, never a circle.
                    crate::controls::ghost_icon_button(
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
                                    crate::controls::pointer_label_item(if pinned { "Unpin" } else { "Pin" }, false)
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
                                crate::controls::pointer_label_item("Edit", false)
                                    .icon(Icon::from(registry::UI_EDIT))
                                    .on_click(move |_, window, cx| {
                                        open_action_page(window, cx, edit_id.clone());
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
        row.child(controls).into_any_element()
    }

}

impl Render for ActionsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // EXP-832: the rows were derived when the data last changed
        // (`refresh`); this only draws them. The team is read off the
        // derivation too, so a header and its rows never disagree.
        let team_id = self.derived.team_id.clone();
        let loading = !self.derived.ready;
        let is_owner = team_id
            .as_deref()
            .is_some_and(|team_id| crate::settings::is_owner(cx, team_id));
        // EXP-367: read ONCE per render for every row.
        let no_agent = crate::coding_flow::no_agent_reason(cx);
        let rows: Vec<gpui::AnyElement> = self
            .derived
            .actions
            .iter()
            .enumerate()
            .map(|(index, action)| {
                self.render_action_row(index, action, is_owner, no_agent.as_ref(), cx)
            })
            .collect();

        // EXP-642: each row is its OWN card — a gapped column, no fused
        // bordered block. The rows render as soon as they hydrate (never
        // blank a list that already has data): EXP-1259 D8 — a first load
        // with nothing yet is skeleton rows, a load behind rows spins the
        // band.
        let header = crate::surface::glass_section_header_loading(
            "Actions",
            loading && !rows.is_empty(),
            None,
            cx,
        );
        let mut body = gpui_component::v_flex().min_w_0().gap_2();
        if !rows.is_empty() {
            body = body.child(gpui_component::v_flex().min_w_0().children(rows));
        } else if loading {
            body = body.child(crate::feedback::skeleton_rows(
                "actions-skeleton",
                crate::feedback::SKELETON_ROWS,
                ACTION_ROW_H,
                cx,
            ));
        } else {
            body = body.child(crate::controls::empty_state_list(
                Icon::new(registry::ACTION_RUN),
                crate::empty_state_copy::ACTIONS,
                cx,
            ));
        }
        // NO gap on the section (EXP-697): the header's own `pb_2` IS the
        // 8px to the list — a gap here doubles it. The rows keep their gap
        // inside `body`.
        let actions_section = gpui_component::v_flex()
            .min_w_0()
            .child(header)
            .child(body);

        page_scaffold(
            "actions-screen-scroll",
            &self.scroll,
            gpui_component::v_flex().gap_6().child(actions_section),
        )
    }
}

/// Open one action's page (SLOP-2) — the row's click, its menu's "Edit",
/// an entity chip and the legacy `action-editor:<uuid>` dev id all land
/// here.
pub(crate) fn open_action_page(window: &mut Window, cx: &mut App, action_id: String) {
    navigate(window, cx, Screen::Action { action_id });
}

/// `prompts.delete` over tRPC — the synced collection drops the row; a live
/// run keeps going and keeps its label.
pub(crate) fn spawn_action_delete(cx: &mut App, action_id: String) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::prompts::delete(&trpc, &action_id) })
            .await;
        let _ = cx.update(|_| {
            if let Err(err) = result {
                log::warn!("actions: delete failed: {err}");
            }
        });
    })
    .detach();
}
