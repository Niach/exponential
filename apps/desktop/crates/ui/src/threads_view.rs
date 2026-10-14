//! EXP-1294 — the THREADS page ([`Screen::Threads`], the rail's `Threads`
//! entry): the ACTIVE team's threads in the Reviews list language.
//!
//! A tab-less FULL-PAGE screen like Devices / Actions: one centred column,
//! no page title. Bands Needs you · Running · Recent
//! (`domain::session_tree::partition_threads`, through the memoised
//! [`crate::session_data::thread_lists`]), each a glass band with its glyph
//! (no count) over flush ONE-line rows ([`crate::pr_rows::pr_row`]): the
//! lead (the root's PR glyph tinted by its state, else the root session's
//! agent mark) · the first linked issue's identifier (mono, muted) · the
//! title · one quiet word (`question`, `ready to merge`, `stack`,
//! `merged`) · a lock when private. A row opens the thread.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    px, App, ClickEvent, Entity, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::rows::{MessageRow, PullRequestRow, SessionRow};
use domain::session_tree::{ThreadsListKey, ThreadsListRow, THREADS_LIST};

use crate::actions_view::page_scaffold_with;
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, Navigation, Screen};
use crate::pr_rows::{pr_row, PrRowSpec, STACK_WORD};
use crate::session_data::{self, PrSnapshot};

/// The page column's cap — the list language's `max-w-3xl` (short rows, a
/// queue, not a table).
const THREADS_COLUMN_W: f32 = 768.;

/// A row's lead glyph.
const LEAD_PX: f32 = 14.;
/// The amber needs-you dot on a row's lead.
const LEAD_BADGE_PX: f32 = 6.;

/// The quiet words a row ends on.
pub(crate) const QUESTION_WORD: &str = "question";
pub(crate) const READY_WORD: &str = "ready to merge";
pub(crate) const MERGED_WORD: &str = "merged";

/// The pull request a row speaks for: the root's pull requests through THE
/// Guide rule (`domain::session_tree::guide_pull_request`, EXP-1306).
pub(crate) fn root_pull_request<'a>(root_id: &str, pulls: &'a [PullRequestRow]) -> Option<&'a PullRequestRow> {
    domain::session_tree::guide_pull_request(
        pulls
            .iter()
            .filter(|pull| pull.root_session_id.as_deref() == Some(root_id)),
    )
}

/// Whether the root waits on an unanswered question or plan card.
pub(crate) fn asks_a_question(messages: &[MessageRow]) -> bool {
    messages.iter().any(|message| {
        message.answered_at.is_none()
            && matches!(
                message.card_kind(),
                Some(domain::contract::MESSAGE_CARD_KIND_QUESTION)
                    | Some(domain::contract::MESSAGE_CARD_KIND_PLAN)
            )
    })
}

/// The row's quiet word (pure): a question waiting on you first, then the
/// PR's state — `stack` for an open member of a stack, `ready to merge` for
/// another open one, `merged`; else none.
pub(crate) fn quiet_word(needs_you: bool, question: bool, pull: Option<&PullRequestRow>, stacked: bool) -> Option<&'static str> {
    if needs_you && question {
        return Some(QUESTION_WORD);
    }
    let pull = pull?;
    if pull.is_open() {
        Some(if stacked { STACK_WORD } else { READY_WORD })
    } else if pull.is_merged() {
        Some(MERGED_WORD)
    } else {
        None
    }
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
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let root = session_data::session(&row.session_id, cx);
        let pull = root_pull_request(&row.session_id, &snapshot.pulls);
        let stacked = pull.is_some_and(|pull| {
            pull.is_open() && snapshot.with_set(|set| domain::pr_stack::stack_position(pull, set).is_some())
        });
        let question = row.needs_you && asks_a_question(&session_data::messages_of_session(&row.session_id, cx));
        let word = quiet_word(row.needs_you, question, pull, stacked);
        // threads-list.json: a spinner while busy, an amber dot when it
        // needs you — over the PR glyph once the thread has a PR.
        let lead: gpui::AnyElement = match pull {
            _ if row.busy => crate::coding_selects::agent_working_mark(root_agent(root.as_ref()), LEAD_PX)
                .into_any_element(),
            Some(pull) => {
                let (glyph, tint) = if pull.is_merged() {
                    (registry::PR_MERGED, theme::tokens::BLUE)
                } else {
                    (registry::PR_OPEN, theme::tokens::GREEN)
                };
                crate::thread_screen::with_needs_you_dot(
                    Icon::new(glyph).with_size(px(LEAD_PX)).text_color(tint.to_hsla()),
                    row.needs_you,
                    LEAD_BADGE_PX,
                )
            }
            None => match root.as_ref() {
                Some(root) => crate::thread_screen::root_mark(root, false, row.needs_you, LEAD_PX, LEAD_BADGE_PX),
                None => Icon::new(registry::SETTINGS_AGENTS)
                    .with_size(px(LEAD_PX))
                    .text_color(muted)
                    .into_any_element(),
            },
        };
        // The stack word carries the stack glyph in front (draft 2).
        let word_el = word.map(|word| {
            h_flex()
                .flex_shrink_0()
                .gap_1()
                .items_center()
                .text_size(px(11.5))
                .text_color(muted)
                .when(word == STACK_WORD, |this| {
                    this.child(Icon::new(registry::PR_STACK).xsmall().text_color(muted))
                })
                .child(word)
                .into_any_element()
        });
        let lock = row.private.then(|| {
            Icon::new(registry::UI_PRIVATE)
                .xsmall()
                .flex_shrink_0()
                .text_color(muted)
                .into_any_element()
        });
        let trailing = match (word_el, lock) {
            (None, None) => None,
            (word, lock) => Some(
                h_flex()
                    .flex_shrink_0()
                    .gap_2()
                    .items_center()
                    .children(word)
                    .children(lock)
                    .into_any_element(),
            ),
        };
        let session_id = row.session_id.clone();
        pr_row(
            PrRowSpec {
                lead: Some(lead),
                identifier: Some(SharedString::from(
                    session_data::first_issue_identifier(&row.session_id, cx).unwrap_or_default(),
                )),
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

    #[test]
    fn the_quiet_word_puts_a_question_first_then_the_pr_state() {
        let open = pull("p", "r", "open", "2026-10-13T08:00:00.000Z");
        let merged = pull("p", "r", "merged", "2026-10-13T08:00:00.000Z");
        let closed = pull("p", "r", "closed", "2026-10-13T08:00:00.000Z");
        assert_eq!(quiet_word(true, true, Some(&open), false), Some("question"));
        assert_eq!(quiet_word(false, true, Some(&open), false), Some("ready to merge"));
        assert_eq!(quiet_word(false, false, Some(&open), true), Some("stack"));
        assert_eq!(quiet_word(false, false, Some(&merged), false), Some("merged"));
        assert_eq!(quiet_word(false, false, Some(&closed), false), None);
        assert_eq!(quiet_word(true, false, None, false), None);
    }

    #[test]
    fn an_unanswered_question_or_plan_card_asks() {
        let message = |kind: &str, answered: Option<&str>| -> MessageRow {
            serde_json::from_value(json!({
                "id": "m", "session_id": "r", "card": {"kind": kind}, "answered_at": answered,
            }))
            .unwrap()
        };
        assert!(asks_a_question(&[message("question", None)]));
        assert!(asks_a_question(&[message("plan", None)]));
        assert!(!asks_a_question(&[message("question", Some("2026-10-13T08:00:00.000Z"))]));
        assert!(!asks_a_question(&[message("event", None)]));
    }
}
