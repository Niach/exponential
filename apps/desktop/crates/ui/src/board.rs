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
//! #FEED-191 (chats.json `shell.boardChips`): the board's pills over the list:
//! All issues · My issues N (Drafts stays a rail entry on wide), My issues being
//! the cross-board assignee list (`queries::my_issues`). A board switch lands on
//! All issues again, like web's `?chip=`.
//!
//! State ownership (§4.1): this entity owns the scope [`IssueQuery`] and
//! hands it to the list, which re-renders off the synced collections.

use gpui::{
    div, px, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{h_flex, v_flex};
use sync::Store;

use domain::chats::CHATS;

use crate::issue_list::{IssueListView, IssueQuery};
use crate::queries;
use crate::surface::{glass_pill, PillMode, PillSize};

/// The board's pills (`shell.boardChips`, less `drafts`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BoardChip {
    #[default]
    All,
    Mine,
}

/// The My issues pill's label: a count after a non-zero one.
fn mine_label(count: usize) -> String {
    let mine = &CHATS.copy.board_chips.mine;
    if count > 0 { format!("{mine} {count}") } else { mine.clone() }
}

pub struct BoardView {
    query: IssueQuery,
    /// The board on screen; `None` = none yet.
    board_id: Option<String>,
    chip: BoardChip,
    issue_list: Entity<IssueListView>,
    /// The My issues pill's count, re-derived only when a collection it reads
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

    /// The team + viewer My issues reads: the board's team.
    fn mine_scope(&self, cx: &gpui::App) -> Option<(String, String)> {
        let board_id = self.board_id.as_deref()?;
        let team_id = Store::global(cx).collections().boards.read(cx).get(board_id)?.team_id.clone();
        Some((team_id, queries::active_account(cx)?.user_id))
    }

    fn sync_query(&mut self, cx: &mut gpui::Context<Self>) {
        let query = match (self.chip, self.mine_scope(cx), self.board_id.clone()) {
            (BoardChip::Mine, Some((team_id, user_id)), _) => IssueQuery::MyIssues { team_id, user_id },
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
        let Some((team_id, user_id)) = self.mine_scope(cx) else {
            return 0;
        };
        let key = (team_id.clone(), user_id.clone(), queries::board_data_key(cx));
        let data = self.mine.get_or_insert_with(key, || queries::my_issues(cx, &team_id, &user_id));
        data.groups.iter().map(|group| group.issues.len()).sum()
    }

    fn pills(&mut self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let mine = mine_label(self.mine_count(cx));
        let chips = [(BoardChip::All, CHATS.copy.board_chips.all.clone()), (BoardChip::Mine, mine)];
        let selected = self.chip;
        h_flex().flex_shrink_0().w_full().px_4().pt_3().pb_2().gap_2().children(chips.into_iter().map(|(chip, label)| {
            let id = SharedString::from(match chip {
                BoardChip::All => "board-chip-all",
                BoardChip::Mine => "board-chip-mine",
            });
            glass_pill(id, PillSize::Md, PillMode::Select { selected: chip == selected }, cx)
                .h(px(36.))
                .px_4()
                .child(SharedString::from(label))
                .on_click(cx.listener(move |this, _, _, cx| this.set_chip(chip, cx)))
        }))
    }
}

impl Render for BoardView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "BoardView").entered();
        let bulk_bar = self.issue_list.update(cx, |list, cx| list.bulk_bar(cx));
        let pills = self.board_id.is_some().then(|| self.pills(cx));

        v_flex()
            .size_full()
            .relative()
            .children(pills)
            .child(div().flex_1().min_h_0().child(self.issue_list.clone()))
            .children(bulk_bar.map(crate::issue_list::bulk_bar_dock))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `My issues N`, a zero count = the bare word.
    #[test]
    fn the_mine_pill_counts() {
        assert_eq!(mine_label(0), "My issues");
        assert_eq!(mine_label(3), "My issues 3");
    }
}
