//! The Drafts center screen (EXP-878): the New issue pages this user left
//! with something in them, most recently touched first.
//!
//! A tab-less full-page screen like Devices / Chats — context-free, so it
//! leaves the rail up and lends no list to what it opens. Clicking a row
//! REOPENS its New issue page seeded from the row
//! ([`crate::issue_draft_screen::open_existing`], EXP-1170);
//! its context menu's Discard deletes it outright (there is nothing
//! destructive to confirm — a draft is by definition unfiled, and the whole
//! feature exists so nothing has to be confirmed).
//!
//! Rows come straight off the per-user `issue_drafts` shape through
//! [`crate::drafts::drafts_in_team`], which drops any row whose board no
//! longer resolves — so a draft on a trashed board is hidden, not broken.

use gpui::{
    div, prelude::FluentBuilder as _, px, ClickEvent, Entity, InteractiveElement as _, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};
use sync::Store;

use crate::controls::PointerContextMenuExt as _;
use crate::scroll_pane::page_scaffold_with;
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, Navigation};

/// The page column's cap — the web Drafts page's `md:max-w-5xl` (EXP-1647).
const DRAFTS_COLUMN_W: f32 = 1024.;

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
    /// [`domain::issue_draft::UNTITLED`]), the board it would be filed onto,
    /// when it was last touched; Discard in its context menu.
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
            .unwrap_or_else(|| domain::issue_draft::UNTITLED.into());

        let untitled = title_color == muted;

        let open_id = draft.id.clone();
        let delete_id = draft.id.clone();
        // EXP-1647: the web `ListRow` (`flex items-center gap-3 p-3`), ONE
        // line — status glyph, title, then the board and the age
        // right-aligned. No buttons on the row: Discard lives in its context
        // menu, like the web's.
        crate::surface::flat_row()
            .id(("draft-row", index))
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .p_3()
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
                crate::issue_draft_screen::open_existing(window, cx, &draft);
            }))
            .pointer_context_menu(move |menu, _, _| {
                let delete_id = delete_id.clone();
                menu.min_w(px(220.)).item(crate::controls::danger_pointer_menu_item(
                    registry::UI_DELETE,
                    "Discard draft",
                    move |_, cx| crate::drafts::delete_draft(delete_id.clone(), cx),
                ))
            })
            .child(
                div()
                    .flex_shrink_0()
                    .child(crate::icons::resolved_status_icon(&status, cx).with_size(px(16.))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .text_color(title_color)
                    .when(untitled, |title| title.italic())
                    .child(title),
            )
            .children(board.map(|board| {
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_1p5()
                    .text_xs()
                    .text_color(muted)
                    .child(crate::icons::board_icon(&board).with_size(px(12.)))
                    .child(SharedString::from(board.name.clone()))
            }))
            .children(
                draft
                    .updated_at
                    .as_deref()
                    .map(crate::inbox::relative_time)
                    .filter(|relative| !relative.is_empty())
                    .map(|relative| {
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(relative))
                    }),
            )
            .into_any_element()
    }
}

impl Render for DraftsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "DraftsView").entered();
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
            // EXP-1259 D8: the list's rows as placeholders.
            v_flex().min_w_0().child(crate::feedback::skeleton_rows(
                "drafts-skeleton",
                crate::feedback::SKELETON_ROWS,
                crate::settings::SETTINGS_ROW_H,
                cx,
            ))
        } else if drafts.is_empty() {
            v_flex().min_w_0().child(crate::controls::empty_state(
                Icon::from(registry::NAV_DRAFTS),
                crate::empty_state_copy::DRAFTS,
                None,
                cx,
            ))
        } else {
            let rows: Vec<gpui::AnyElement> = drafts
                .iter()
                .enumerate()
                .map(|(index, draft)| self.draft_row(index, draft, cx))
                .collect();
            // ×4: a filled group band with NO count (CLAUDE.md "Lists ×4"),
            // like the web Drafts list.
            v_flex()
                .min_w_0()
                .child(crate::surface::glass_section_band(None, "Drafts", None, cx))
                .children(rows)
        };

        page_scaffold_with("drafts-screen-scroll", &self.scroll, column, DRAFTS_COLUMN_W)
    }
}
