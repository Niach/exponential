//! The board-view screen — web parity target
//! `apps/web/src/routes/t/$teamSlug/boards/$boardSlug/index.tsx`
//! (masterplan-v3 §4.2 "Board view").
//!
//! Composition mirrors the web route: the virtualized [`IssueListView`] fills
//! the view. The same view also backs **My Issues** (web
//! `my-issues/index.tsx` renders the identical list with `canCreate=false`).
//!
//! EXP-862 removed issue filtering from every client; EXP-1294 removed the
//! board's Tidy up strip with the builtin actions — the list has the whole
//! view.
//!
//! EXP-289/EXP-426: the list's bulk-action bar is still rendered by THIS
//! view, never by the list itself. It floats over the bottom of the list, so starting a selection
//! never moves a row (the EXP-289 no-jump invariant) and the list keeps the
//! whole panel.
//!
//! #FEED-191 + #EXP-1836 (chats.json `shell.boardChips`): the board's tabs over
//! the list, THE session screen's Segmented strip (left-aligned, a muted count
//! after the label): All issues · My issues N (Drafts stays a rail entry on
//! wide), My issues being the viewer's assigned issues on THIS board
//! (`queries::my_issues`, `domain::chats::board_mine_issues`). A board switch
//! lands on All issues again, like web's `?chip=`.
//!
//! State ownership (§4.1): this entity owns the scope [`IssueQuery`] and
//! hands it to the list, which re-renders off the synced collections.

use gpui::{
    div, px, AppContext as _, Entity, InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::ActiveTheme as _;
use gpui_component::{h_flex, v_flex};
use sync::Store;

use domain::chats::CHATS;

use crate::issue_list::{IssueListView, IssueQuery};
use crate::queries;
/// The strip's height: the session screen's tab strip (`session_chat`
/// `SCREEN_TABS_H`).
const BOARD_TABS_H: f32 = 32.;

/// The board's tabs (`shell.boardChips`, less `drafts`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BoardChip {
    #[default]
    All,
    Mine,
}

/// A tab's count (web `boardChipCount`): My issues only, none at zero.
fn chip_count(chip: BoardChip, mine: usize) -> Option<String> {
    match chip {
        BoardChip::Mine if mine > 0 => Some(mine.to_string()),
        _ => None,
    }
}

pub struct BoardView {
    query: IssueQuery,
    /// The board on screen; `None` = none yet.
    board_id: Option<String>,
    chip: BoardChip,
    issue_list: Entity<IssueListView>,
    /// The My issues tab's count, re-derived only when a collection it reads
    /// moved (gpui re-renders on every window refresh).
    mine: queries::Memo<(String, String, queries::BoardDataKey), queries::BoardData>,
    _subscriptions: Vec<Subscription>,
}

impl BoardView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let issue_list = cx.new(|cx| IssueListView::new(window, cx));

        let subscriptions = vec![
            // EXP-289: the floating bulk bar lives in THIS view's tree, so a
            // selection change inside the list (checkbox, Cmd/Shift-click,
            // Cmd-A, Escape) has to re-render the board too.
            cx.observe(&issue_list, |_, _, cx| cx.notify()),
        ];

        // DEV-ONLY (the EXP_DEV_* family): a capture lands on My issues.
        let dev_mine = ["EXP_DEV_TOOL", "EXP_DEV_SCREEN"]
            .iter()
            .any(|var| matches!(std::env::var(var).ok().as_deref().map(str::trim), Some("my-issues" | "inbox-my-issues")));

        Self {
            query: IssueQuery::None,
            board_id: None,
            chip: if dev_mine { BoardChip::Mine } else { BoardChip::All },
            issue_list,
            mine: queries::Memo::default(),
            _subscriptions: subscriptions,
        }
    }

    /// Point the board at a new scope (called by the list panel every render):
    /// another board lands on All issues.
    pub fn set_board(&mut self, board_id: Option<String>, cx: &mut gpui::Context<Self>) {
        if self.board_id != board_id {
            if self.board_id.is_some() {
                self.chip = BoardChip::All;
            }
            self.board_id = board_id;
        }
        self.sync_query(cx);
    }

    /// The `OpenMyIssues` action: the next board shown opens on My issues.
    pub(crate) fn show_my_issues(&mut self, cx: &mut gpui::Context<Self>) {
        self.set_chip(BoardChip::Mine, cx);
    }

    fn set_chip(&mut self, chip: BoardChip, cx: &mut gpui::Context<Self>) {
        self.chip = chip;
        self.sync_query(cx);
        cx.notify();
    }

    /// What My issues reads: (board, its team, the viewer).
    fn mine_scope(&self, cx: &gpui::App) -> Option<(String, String, String)> {
        let board_id = self.board_id.clone()?;
        let team_id = Store::global(cx).collections().boards.read(cx).get(&board_id)?.team_id.clone();
        Some((board_id, team_id, queries::active_account(cx)?.user_id))
    }

    fn sync_query(&mut self, cx: &mut gpui::Context<Self>) {
        let query = match (self.chip, self.mine_scope(cx), self.board_id.clone()) {
            (BoardChip::Mine, Some((board_id, team_id, user_id)), _) => IssueQuery::MyIssues { board_id, team_id, user_id },
            (_, _, Some(board_id)) => IssueQuery::Board { board_id },
            _ => IssueQuery::None,
        };
        self.set_query(query, cx);
    }

    fn set_query(&mut self, query: IssueQuery, cx: &mut gpui::Context<Self>) {
        if self.query == query {
            return;
        }
        self.query = query.clone();
        self.issue_list.update(cx, |list, cx| {
            list.set_query(query, cx);
        });
        cx.notify();
    }

    fn mine_count(&mut self, cx: &gpui::App) -> usize {
        let Some((board_id, _, user_id)) = self.mine_scope(cx) else {
            return 0;
        };
        let key = (board_id.clone(), user_id.clone(), queries::board_data_key(cx));
        let data = self.mine.get_or_insert_with(key, || queries::my_issues(cx, &board_id, &user_id));
        data.groups.iter().map(|group| group.issues.len()).sum()
    }

    /// THE session screen's tab strip (`session_chat::screen_tabs`' recipe
    /// over the same `controls::segmented` capsule): each tab sized to its
    /// label, the count a muted number after it, left-aligned.
    fn tabs(&mut self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let mine = self.mine_count(cx);
        let selected = self.chip;
        let muted = cx.theme().muted_foreground;
        let chips = [(BoardChip::All, &CHATS.copy.board_chips.all), (BoardChip::Mine, &CHATS.copy.board_chips.mine)];
        let segments: Vec<_> = chips
            .into_iter()
            .map(|(chip, label)| {
                let id = match chip {
                    BoardChip::All => "board-chip-all",
                    BoardChip::Mine => "board-chip-mine",
                };
                let count = chip_count(chip, mine);
                let name = eui_core::segmented::item_accessibility_label(label, count.as_deref());
                crate::controls::segmented_item(chip == selected, cx)
                    .flex_none()
                    .px_3()
                    .id(SharedString::from(id))
                    .debug_selector(move || id.to_string())
                    .role(gpui::Role::Tab)
                    .aria_label(SharedString::from(name))
                    .aria_selected(chip == selected)
                    .child(SharedString::from(label.clone()))
                    .children(count.map(|count| div().text_color(muted).child(SharedString::from(count))))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_chip(chip, cx)))
            })
            .collect();
        h_flex()
            .id("board-chips-scroll")
            .flex_shrink_0()
            .w_full()
            .min_w_0()
            .overflow_x_scroll()
            .px_4()
            .pt_3()
            .pb_2()
            .child(
                crate::controls::segmented(cx)
                    .id(SharedString::from(CHATS.test_ids.board_chip.clone()))
                    .flex_shrink_0()
                    .w_auto()
                    .justify_start()
                    .h(px(BOARD_TABS_H))
                    .children(segments),
            )
    }
}

impl Render for BoardView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "BoardView").entered();
        let bulk_bar = self.issue_list.update(cx, |list, cx| list.bulk_bar(cx));
        let tabs = self.board_id.is_some().then(|| self.tabs(cx));

        v_flex()
            .size_full()
            .relative()
            .children(tabs)
            .child(div().flex_1().min_h_0().child(self.issue_list.clone()))
            .children(bulk_bar.map(crate::issue_list::bulk_bar_dock))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// My issues carries its count (none at zero); All issues never.
    #[test]
    fn the_mine_tab_counts() {
        assert_eq!(chip_count(BoardChip::Mine, 0), None);
        assert_eq!(chip_count(BoardChip::Mine, 3).as_deref(), Some("3"));
        assert_eq!(chip_count(BoardChip::All, 3), None);
    }
}
