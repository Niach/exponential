//! EXP-1294 — the THREADS page ([`Screen::Threads`], the rail's `Threads`
//! entry): the ACTIVE team's threads in the Reviews list language.
//!
//! A tab-less FULL-PAGE screen like Devices / Actions: one centred column,
//! no page title. Bands Needs you · Running · Recent
//! (`domain::session_tree::partition_threads`, through the memoised
//! [`crate::session_data::thread_lists`]), each a glass band with its glyph
//! (no count) over flat gapless ONE-line rows ([`crate::pr_rows::pr_row`],
//! no dividers, EXP-1320): the lead (working mark while busy, else the
//! root's PR glyph: open emerald, merged violet, else the root session's
//! agent mark; the amber badge when it needs you) · the title (EXP-1316: it
//! names the issue already, no identifier slot) · the contract's quiet word
//! (`domain::session_tree::thread_row_word`) AND a lock when private. A row
//! opens the thread.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    px, App, ClickEvent, Entity, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::rows::{PullRequestRow, SessionRow};
use domain::session_tree::{
    thread_in_stack, thread_row_word, StackPr, ThreadRowWord, ThreadsListKey, ThreadsListRow, THREADS_LIST,
};

use crate::actions_view::page_scaffold_with;
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, Navigation, Screen};
use crate::pr_rows::{pr_row, PrRowSpec};
use crate::session_data::{self, PrSnapshot};

/// The page column's cap — the list language's `max-w-3xl` (short rows, a
/// queue, not a table).
const THREADS_COLUMN_W: f32 = 768.;

/// A row's lead glyph.
const LEAD_PX: f32 = 14.;
/// The amber needs-you dot on a row's lead.
const LEAD_BADGE_PX: f32 = 6.;

/// The pull request a row speaks for: the root's pull requests through THE
/// Guide rule (`domain::session_tree::guide_pull_request`, EXP-1306).
pub(crate) fn root_pull_request<'a>(root_id: &str, pulls: &'a [PullRequestRow]) -> Option<&'a PullRequestRow> {
    domain::session_tree::guide_pull_request(
        pulls
            .iter()
            .filter(|pull| pull.root_session_id.as_deref() == Some(root_id)),
    )
}

/// EXP-1318: a thread row's quiet word ×4 (`threads-list.json`
/// `wordCases`): the root's pull requests, whether they sit in an open
/// stack (`stackCases`, over the team's pull requests) and the row's
/// busy / needs-you.
pub(crate) fn thread_word(root_id: &str, busy: bool, needs_you: bool, team_pulls: &[PullRequestRow]) -> Option<ThreadRowWord> {
    let own: Vec<StackPr> = team_pulls
        .iter()
        .filter(|pull| pull.root_session_id.as_deref() == Some(root_id))
        .map(StackPr::from_row)
        .collect();
    let team: Vec<StackPr> = team_pulls.iter().map(StackPr::from_row).collect();
    let in_stack = thread_in_stack(&own, &team);
    thread_row_word(own.iter().map(|pull| pull.state.as_str()), busy, needs_you, in_stack)
}

/// The lead of a thread row ×4 (threads-list.json): the working mark while
/// busy, else the root's Guide PR glyph (open emerald, merged violet) with
/// the amber badge when it needs you, else the root session's mark.
pub(crate) fn thread_row_lead(
    root_id: &str,
    busy: bool,
    needs_you: bool,
    team_pulls: &[PullRequestRow],
    cx: &App,
) -> gpui::AnyElement {
    let root = session_data::session(root_id, cx);
    match root_pull_request(root_id, team_pulls) {
        _ if busy => crate::coding_selects::agent_working_mark(root_agent(root.as_ref()), LEAD_PX).into_any_element(),
        Some(pull) => {
            let (glyph, tint) = if pull.is_merged() {
                (registry::PR_MERGED, crate::pr_rows::PR_MERGED_TINT)
            } else {
                (registry::PR_OPEN, theme::tokens::GREEN)
            };
            crate::thread_screen::with_needs_you_dot(
                Icon::new(glyph).with_size(px(LEAD_PX)).text_color(tint.to_hsla()),
                needs_you,
                LEAD_BADGE_PX,
            )
        }
        None => match root.as_ref() {
            Some(root) => crate::thread_screen::root_mark(root, false, needs_you, LEAD_PX, LEAD_BADGE_PX),
            None => Icon::new(registry::SETTINGS_AGENTS)
                .with_size(px(LEAD_PX))
                .text_color(cx.theme().muted_foreground)
                .into_any_element(),
        },
    }
}

/// The row's trailing slot: the quiet word (the stack word carries the
/// stack glyph in front) AND the lock when private; `None` when both are
/// absent.
pub(crate) fn thread_row_trailing(word: Option<ThreadRowWord>, private: bool, cx: &App) -> Option<gpui::AnyElement> {
    let muted = cx.theme().muted_foreground;
    if word.is_none() && !private {
        return None;
    }
    Some(
        h_flex()
            .flex_shrink_0()
            .gap_2()
            .items_center()
            .children(word.map(|word| {
                h_flex()
                    .flex_shrink_0()
                    .gap_1()
                    .items_center()
                    .text_size(px(11.5))
                    .text_color(muted)
                    .when(word == ThreadRowWord::Stack, |this| {
                        this.child(Icon::new(registry::PR_STACK).xsmall().text_color(muted))
                    })
                    .child(word.text())
            }))
            .children(private.then(|| Icon::new(registry::UI_PRIVATE).xsmall().flex_shrink_0().text_color(muted)))
            .into_any_element(),
    )
}

/// The agent CLI a root runs.
fn root_agent(root: Option<&SessionRow>) -> Option<coding::CodingAgent> {
    root.and_then(|root| root.cli.as_deref()).and_then(coding::CodingAgent::parse)
}

pub struct ThreadsView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl ThreadsView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        subscriptions.extend(session_data::observe_session_collections(cx));
        Self {
            nav,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    fn band_glyph(key: ThreadsListKey, cx: &App) -> gpui::AnyElement {
        let quiet = cx.theme().foreground.opacity(0.7);
        match key {
            ThreadsListKey::NeedsYou => Icon::new(registry::CODING_NEEDS_INPUT)
                .xsmall()
                .flex_shrink_0()
                .text_color(theme::tokens::YELLOW.to_hsla())
                .into_any_element(),
            ThreadsListKey::Running => crate::coding_selects::agent_mark(coding::CodingAgent::default())
                .xsmall()
                .flex_shrink_0()
                .into_any_element(),
            ThreadsListKey::Recent => Icon::new(registry::UI_CHECK)
                .xsmall()
                .flex_shrink_0()
                .text_color(quiet)
                .into_any_element(),
        }
    }

    fn row(&self, row: &ThreadsListRow, snapshot: &PrSnapshot, cx: &App) -> gpui::AnyElement {
        let lead = thread_row_lead(&row.session_id, row.busy, row.needs_you, &snapshot.pulls, cx);
        let word = thread_word(&row.session_id, row.busy, row.needs_you, &snapshot.pulls);
        let trailing = thread_row_trailing(word, row.private, cx);
        let session_id = row.session_id.clone();
        pr_row(
            PrRowSpec {
                lead: Some(lead),
                trailing,
                on_click: Some(Box::new(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                    crate::navigation::navigate(
                        window,
                        cx,
                        Screen::Thread {
                            session_id: session_id.clone(),
                        },
                    );
                })),
                ..PrRowSpec::open(
                    format!("{}-{}", THREADS_LIST.test_ids.row, row.session_id),
                    row.title.clone(),
                )
            },
            cx,
        )
    }
}

impl Render for ThreadsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let ready = sync::Store::try_global(cx)
            .is_some_and(|store| store.collections().sessions.read(cx).is_ready());
        let team_id = active_team_id(&self.nav, cx);
        let groups = team_id
            .as_deref()
            .map(|team_id| session_data::thread_lists(team_id, cx))
            .unwrap_or_default();
        let column = if !ready {
            // Skeletons, never "Loading…" (feedback-states.json).
            v_flex().min_w_0().child(crate::feedback::skeleton_rows(
                "threads-skeleton",
                crate::feedback::SKELETON_ROWS,
                crate::settings::SETTINGS_ROW_H,
                cx,
            ))
        } else if groups.is_empty() {
            v_flex().min_w_0().child(crate::controls::empty_state(
                Icon::new(registry::NAV_THREADS),
                crate::empty_state_copy::THREADS,
                None,
                cx,
            ))
        } else {
            let snapshot = team_id
                .as_deref()
                .map(|team_id| session_data::team_pr_snapshot(team_id, cx))
                .unwrap_or_default();
            let mut bands: Vec<gpui::AnyElement> = Vec::new();
            for group in groups.iter() {
                let band = crate::surface::glass_section_band(
                    Some(Self::band_glyph(group.key, cx)),
                    SharedString::from(group.title.clone()),
                    None,
                    cx,
                );
                let rows: Vec<gpui::AnyElement> =
                    group.rows.iter().map(|row| self.row(row, &snapshot, cx)).collect();
                bands.push(v_flex().min_w_0().pb_2().child(band).children(rows).into_any_element());
            }
            v_flex().min_w_0().gap_2().children(bands)
        };
        page_scaffold_with("threads-screen-scroll", &self.scroll, column, THREADS_COLUMN_W)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pull(id: &str, root: &str, state: &str, created: &str) -> PullRequestRow {
        serde_json::from_value(json!({
            "id": id, "root_session_id": root, "state": state, "created_at": created,
        }))
        .unwrap()
    }

    /// The row speaks for the newest OPEN pull request, else the newest
    /// merged one; a closed one never counts.
    #[test]
    fn the_root_pull_request_prefers_open_then_merged() {
        let pulls = vec![
            pull("p1", "r1", "merged", "2026-10-13T09:00:00.000Z"),
            pull("p2", "r1", "open", "2026-10-13T08:00:00.000Z"),
            pull("p3", "r1", "closed", "2026-10-13T10:00:00.000Z"),
            pull("p4", "r2", "open", "2026-10-13T11:00:00.000Z"),
        ];
        assert_eq!(root_pull_request("r1", &pulls).map(|pull| pull.id.as_str()), Some("p2"));
        let merged = vec![pulls[0].clone(), pulls[2].clone()];
        assert_eq!(root_pull_request("r1", &merged).map(|pull| pull.id.as_str()), Some("p1"));
        assert!(root_pull_request("r3", &pulls).is_none());
    }

    /// The word follows the contract: open in a stack → stack, open idle →
    /// ready to merge, merged, else a question while it needs you.
    #[test]
    fn the_thread_word_follows_the_contract() {
        let mut stacked = pull("p2", "r1", "open", "2026-10-13T08:00:00.000Z");
        stacked.base_pull_request_id = Some("p1".into());
        let pulls = vec![
            pull("p1", "r0", "open", "2026-10-13T07:00:00.000Z"),
            stacked,
            pull("p3", "r2", "open", "2026-10-13T08:00:00.000Z"),
            pull("p4", "r3", "merged", "2026-10-13T08:00:00.000Z"),
        ];
        assert_eq!(thread_word("r1", false, false, &pulls), Some(ThreadRowWord::Stack));
        assert_eq!(thread_word("r2", false, true, &pulls), Some(ThreadRowWord::ReadyToMerge));
        assert_eq!(thread_word("r2", true, false, &pulls), None);
        assert_eq!(thread_word("r3", false, true, &pulls), Some(ThreadRowWord::Merged));
        assert_eq!(thread_word("r4", false, true, &pulls), Some(ThreadRowWord::Question));
        assert_eq!(thread_word("r4", false, false, &pulls), None);
    }
}
