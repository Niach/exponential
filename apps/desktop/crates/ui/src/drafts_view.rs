//! The Drafts center screen (EXP-878): the create-issue dialogs this user
//! closed with something in them, most recently touched first.
//!
//! A tab-less full-page screen like Devices / Reviews — context-free, so it
//! leaves the rail up and lends no list to what it opens. Clicking a row
//! REOPENS its dialog seeded from the row ([`crate::create_issue_dialog::open_draft`]);
//! the row's hover-revealed ✕ deletes it outright (there is nothing
//! destructive to confirm — a draft is by definition unfiled, and the whole
//! feature exists so nothing has to be confirmed).
//!
//! Rows come straight off the per-user `issue_drafts` shape through
//! [`crate::drafts::drafts_in_team`], which drops any row whose board no
//! longer resolves — so a draft on a trashed board is hidden, not broken.

use gpui::{
    div, ClickEvent, Entity, InteractiveElement as _, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;

use crate::actions_view::page_scaffold_with;
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, Navigation};

/// The page column's cap — the Reviews width (`max-w-3xl`): these rows are a
/// title and a board, not a table.
const DRAFTS_COLUMN_W: f32 = 768.;

/// The hover group the row's ✕ hides in (the rail's pinned-row recipe).
const DRAFT_ROW_GROUP: &str = "draft-row";

pub struct DraftsView {
    #[allow(dead_code)] // held for the team-switch re-render subscription
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl DraftsView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        if let Some(store) = Store::try_global(cx) {
            let collections = store.collections().clone();
            subscriptions.push(cx.observe(&collections.issue_drafts, |_, _, cx| cx.notify()));
            // The rows join through boards (visibility + glyph) and the team
            // status vocabulary (the leading status icon).
            subscriptions.push(cx.observe(&collections.boards, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.issue_statuses, |_, _, cx| cx.notify()));
        }
        Self {
            nav,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// One draft row: the resolved status glyph, the title (or a muted
    /// "Untitled draft"), the board it would be filed onto, when it was last
    /// touched, and the hover-revealed delete.
    fn draft_row(
        &self,
        index: usize,
        draft: &domain::rows::IssueDraftRow,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let row_hover = theme.sidebar_accent.opacity(0.5);

        let board = draft
            .board_id
            .as_deref()
            .and_then(|board_id| Store::global(cx).collections().boards.read(cx).get(board_id))
            .cloned();
        let statuses = crate::queries::team_statuses(cx, draft.team_id.as_deref().unwrap_or(""));
        let status = crate::queries::resolve_status_ref(
            draft.status_id.as_deref(),
            domain::IssueStatus::Backlog,
            &statuses,
        );

        let title = crate::drafts::draft_title(draft);
        let title_color = if title.is_some() {
            theme.foreground
        } else {
            muted
        };
        let title: SharedString = title
            .map(SharedString::from)
            .unwrap_or_else(|| "Untitled draft".into());

        let mut meta = h_flex()
            .min_w_0()
            .items_center()
            .gap_1p5()
            .text_xs()
            .text_color(muted);
        if let Some(board) = &board {
            meta = meta
                .child(
                    crate::icons::board_icon(board)
                        .xsmall()
                        .flex_shrink_0()
                        .text_color(muted),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(SharedString::from(board.name.clone())),
                );
        }
        if let Some(updated_at) = draft.updated_at.as_deref() {
            let relative = crate::inbox::relative_time(updated_at);
            if !relative.is_empty() {
                meta = meta
                    .child(div().flex_shrink_0().child("·"))
                    .child(div().flex_shrink_0().child(SharedString::from(relative)));
            }
        }

        let open_id = draft.id.clone();
        let delete_id = draft.id.clone();
        crate::surface::flat_row()
            .id(("draft-row", index))
            .group(DRAFT_ROW_GROUP)
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_2p5()
            .px_3()
            .py_2p5()
            .cursor_pointer()
            .hover(move |style| style.bg(row_hover))
            .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                let Some(draft) = Store::global(cx)
                    .collections()
                    .issue_drafts
                    .read(cx)
                    .get(&open_id)
                    .cloned()
                else {
                    return; // raced a delete elsewhere
                };
                crate::create_issue_dialog::open_draft(window, cx, &draft);
            }))
            .child(
                div()
                    .flex_shrink_0()
                    .child(crate::icons::resolved_status_icon(&status, cx).xsmall()),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .text_color(title_color)
                            .child(title),
                    )
                    .child(meta),
            )
            .child(
                div()
                    .invisible()
                    .group_hover(DRAFT_ROW_GROUP, |style| style.visible())
                    .flex_shrink_0()
                    .child(
                        Button::new(("draft-row-delete", index))
                            .ghost()
                            .cursor_pointer()
                            .xsmall()
                            .icon(Icon::from(registry::UI_DELETE))
                            .tooltip("Delete draft")
                            .on_click(move |_, _window, cx| {
                                // The row underneath reopens the draft — a
                                // delete must not do both.
                                cx.stop_propagation();
                                crate::drafts::delete_draft(delete_id.clone(), cx);
                            }),
                    ),
            )
            .into_any_element()
    }
}

impl Render for DraftsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let team_id = active_team_id(&self.nav, cx);
        let is_ready = Store::try_global(cx).is_some_and(|store| {
            let collections = store.collections();
            collections.issue_drafts.read(cx).is_ready() && collections.boards.read(cx).is_ready()
        });
        let drafts = team_id
            .as_deref()
            .map(|team_id| crate::drafts::drafts_in_team(team_id, cx))
            .unwrap_or_default();

        // §4.1 `is_ready`: a skeleton while the shape has not caught up —
        // an empty collection before that is "still syncing", never "no
        // drafts".
        let column = if !is_ready {
            v_flex()
                .min_w_0()
                .gap_2()
                .child(Skeleton::new().h_3p5().w_40())
                .child(Skeleton::new().h_3p5().w_48())
                .child(Skeleton::new().h_3p5().w_32())
        } else if drafts.is_empty() {
            v_flex().min_w_0().child(crate::controls::empty_state(
                Icon::from(registry::NAV_DRAFTS),
                "No drafts",
                "Close the new-issue dialog with something in it and it waits for you here.",
                cx,
            ))
        } else {
            let count = drafts.len();
            let rows: Vec<gpui::AnyElement> = drafts
                .iter()
                .enumerate()
                .map(|(index, draft)| self.draft_row(index, draft, cx))
                .collect();
            v_flex()
                .min_w_0()
                .child(
                    crate::surface::glass_section_band(None, "Drafts", None, cx).child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(cx.theme().foreground.opacity(0.5))
                            .child(SharedString::from(format!("{count}"))),
                    ),
                )
                .children(rows)
        };

        page_scaffold_with("drafts-screen-scroll", &self.scroll, column, DRAFTS_COLUMN_W)
    }
}
