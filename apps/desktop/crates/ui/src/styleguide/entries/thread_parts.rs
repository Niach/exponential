//! EXP-1284/EXP-1294: `thread-parts` (Special components): the thread
//! screen's pieces, the IDE half of the styleguide entry, case for case with
//! the web one — the REAL painters (`crate::thread_screen`) over the
//! contract model (`domain::session_tree::build_session_thread`): the header
//! twice (`fixtures/session-thread.json`'s first case, then the private one
//! with its lock): the title and the ONE tab strip (+ · Conversation · the
//! issue chips · Guide with its dot); then (EXP-1334) the fold case's strip
//! at three widths (wide: every chip full; narrower: identifiers; narrow:
//! each zone folded into its own "+N", a nested session open), a session
//! chip's subtree and an issue chip's sub-issues drawn OPEN, and the "+"
//! menu (Link an issue ›, New issue); then every conversation row in order
//! (EXP-1334: no root card) — the person's bubble, the agent's prose, an
//! event line, a child's session card, a question answered in place (an
//! option with its description, EXP-1322) and the queued reply; the rows
//! the first case has no example of (a plan, a subagent, a picture) follow
//! from the second case, then (EXP-1323) the dispatcher case's report line
//! and suggestions card in every item state. THE composer the thread hosts
//! is the `session-thread` entry's (the web's too). Last (EXP-1339), the
//! Threads list: THE page's bands and rows
//! ([`crate::threads_view::thread_list_bands`]) over `threads-list.json`'s
//! first case through `partition_threads`, with demo pull requests so every
//! trailing word shows: stack, ready to merge, a question, and a merged row
//! (the violet pr-merged lead, no word). No lock on a list row.
//!
//! No `sync::Store` here: authors read `Someone`, no `#IDENT` pills, and the
//! image cache has no transport (the picture paints its placeholder box).

use std::collections::HashMap;

use gpui::{
    div, px, AnyElement, App, Div, Entity, IntoElement as _, ParentElement as _, SharedString, Styled as _,
    Window,
};
use gpui_component::menu::{PopupMenu, PopupMenuItem};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon};

use domain::rows::{MessageRow, PullRequestRow, SessionIssueRow, SessionReadRow, SessionRow};
use domain::session_tree::{
    build_session_thread, partition_threads, SessionState, SessionTabMarkState, SessionThreadInput,
    SessionThreadRow, SessionThreadView, ThreadTab, ThreadsListGroup, ThreadsListInput, TreeIssue,
};
use domain::statuses::ResolvedStatus;

use crate::icons::registry;
use crate::markdown::ImageCache;
use crate::session_data::PrSnapshot;
use crate::thread_screen::{
    agent_prose, bubble_caption, card_row, clock, issue_glyph, message_markdown, muted_line, thread_menu,
    thread_strip, title_row, user_bubble, StripIssue, StripPick, ThreadMenuTarget, ThreadStrip,
};

use super::composer_dialog::OpenMenu;
use super::fixture_rows;

pub(crate) const ID: &str = "thread-parts";
pub(crate) const OWNER: &str = "EXP-1293";

/// One fixture case: the view, plus the messages it was built from (a
/// bubble's caption reads its `createdAt`).
pub(super) struct Case {
    pub(super) view: SessionThreadView,
    messages: Vec<MessageRow>,
    issues: Vec<TreeIssue>,
}

/// EXP-1323: the index of the first fixture case whose name starts with
/// `prefix` (the dispatcher case: suggestions, a report, an aside).
pub(super) fn case_named(prefix: &str) -> usize {
    let fixture: serde_json::Value =
        serde_json::from_str(fixture_rows::SESSION_THREAD_JSON).expect("session-thread.json parses");
    fixture["cases"]
        .as_array()
        .and_then(|cases| {
            cases
                .iter()
                .position(|case| case["name"].as_str().is_some_and(|name| name.starts_with(prefix)))
        })
        .expect("the fixture case exists")
}

/// The EXP-1323 dispatcher case's name prefix.
pub(super) const DISPATCHER_CASE: &str = "EXP-1323: a live";

/// Fixture case `index`, built through the shared model.
pub(super) fn case(index: usize) -> Case {
    let fixture: serde_json::Value =
        serde_json::from_str(fixture_rows::SESSION_THREAD_JSON).expect("session-thread.json parses");
    let input = &fixture["cases"][index]["input"];
    let root: SessionRow = fixture_rows::row(&input["root"]);
    let sessions: Vec<SessionRow> = fixture_rows::rows(&input["sessions"]);
    let links: Vec<SessionIssueRow> = fixture_rows::rows(&input["sessionIssues"]);
    let issues: Vec<TreeIssue> = serde_json::from_value(input["issues"].clone()).unwrap_or_default();
    let messages: Vec<MessageRow> = fixture_rows::rows(&input["messages"]);
    let pulls: Vec<PullRequestRow> = fixture_rows::rows(&input["pullRequests"]);
    let view = build_session_thread(
        SessionThreadInput {
            root: &root,
            sessions: &sessions.iter().collect::<Vec<_>>(),
            session_issues: &links.iter().collect::<Vec<_>>(),
            issues: &issues,
            messages: &messages.iter().collect::<Vec<_>>(),
            pull_requests: &pulls.iter().collect::<Vec<_>>(),
            now_ms: fixture_rows::now_ms(&input["now"]),
        },
        None,
    );
    Case { view, messages, issues }
}

/// EXP-1334's case: folded issue + session chips, a child's asks.
pub(super) const FOLD_CASE: &str = "EXP-1334: the root has no chip";

/// EXP-1334: the web entry's two extra issues on the fold case, so each
/// zone has something to fold: `(id, identifier, title)`.
const EXTRA_ISSUES: [(&str, &str, &str); 2] = [("i-5", "EXP-25", "Context ring"), ("i-6", "EXP-26", "Model picker")];

/// The fold case's tabs with the extra issues after its own, before the
/// sessions (web `FOLDED_TABS`).
fn folded_tabs(view: &SessionThreadView) -> Vec<ThreadTab> {
    let trailing = |tab: &ThreadTab| matches!(tab, ThreadTab::Session { .. } | ThreadTab::Guide { .. });
    let mut tabs: Vec<ThreadTab> = view.tabs.iter().filter(|tab| !trailing(tab)).cloned().collect();
    tabs.extend(EXTRA_ISSUES.iter().map(|(id, identifier, _)| ThreadTab::Issue {
        issue_id: id.to_string(),
        identifier: identifier.to_string(),
        nested: Vec::new(),
    }));
    tabs.extend(view.tabs.iter().filter(|tab| trailing(tab)).cloned());
    tabs
}

/// The demo statuses (no store here), the web's: in progress, backlog, by
/// turns.
fn status_of(index: usize) -> ResolvedStatus {
    domain::statuses::constructed_default([domain::IssueStatus::InProgress, domain::IssueStatus::Backlog][index % 2])
}

/// The strip's issue info: a title for every issue the case (and the
/// extras) names, the status by the chip's index among the issue chips (a
/// nested row's by its index under the chip + 1, the web `issueSubtree`).
fn strip_issues(case: &Case, tabs: &[ThreadTab]) -> HashMap<String, StripIssue> {
    let mut issues: HashMap<String, StripIssue> = case
        .issues
        .iter()
        .enumerate()
        .map(|(index, issue)| {
            (
                issue.id.clone(),
                StripIssue {
                    title: issue.title.clone(),
                    status: Some(status_of(index)),
                    ringed: None,
                },
            )
        })
        .collect();
    for (id, _, title) in EXTRA_ISSUES {
        issues.entry(id.to_string()).or_insert_with(|| StripIssue {
            title: title.to_string(),
            status: None,
            ringed: None,
        });
    }
    for (index, tab) in tabs.iter().filter(|tab| matches!(tab, ThreadTab::Issue { .. })).enumerate() {
        let ThreadTab::Issue { issue_id, nested, .. } = tab else {
            continue;
        };
        if let Some(issue) = issues.get_mut(issue_id) {
            issue.status = Some(status_of(index));
        }
        for (row, item) in nested.iter().enumerate() {
            if let Some(issue) = issues.get_mut(&item.issue_id) {
                issue.status = Some(status_of(row + 1));
            }
        }
    }
    issues
}

/// THE strip the thread screen draws ([`thread_strip`]) over `tabs`, `open`
/// picked, laid out in `width` px through the contract's partition over the
/// MEASURED middle slot (like the screen).
fn strip(
    case: &Case,
    tabs: Vec<ThreadTab>,
    open: StripPick,
    width: f32,
    prefix: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let slot = window.use_keyed_state(SharedString::from(format!("{prefix}-slot")), cx, |_, _| None::<f32>);
    let available = *slot.read(cx);
    let issues = strip_issues(case, &tabs);
    let strip = thread_strip(
        ThreadStrip {
            prefix,
            tabs,
            issues,
            clis: Default::default(),
            open,
            available,
            menu: Some(ThreadMenuTarget {
                root_id: case.view.root_id.clone(),
                team_id: None,
                chip: None,
                linked: Vec::new(),
            }),
            on_pick: std::rc::Rc::new(|_, _, _| {}),
            on_slot: Some(std::rc::Rc::new(move |measured, cx| {
                let changed = slot.update(cx, |slot, _| {
                    let changed = slot.is_none_or(|prev| (prev - measured).abs() > 0.5);
                    *slot = Some(measured);
                    changed
                });
                if changed {
                    cx.refresh_windows();
                }
            })),
        },
        window,
        cx,
    );
    div().w(px(width)).max_w_full().child(strip).into_any_element()
}

/// The header's width (web `48rem`).
const HEADER_W: f32 = 768.;
/// EXP-1334: the fold case's three tiers. Wide: every chip full (web
/// `84rem`); narrower: the issues short (web `54rem`); narrow: each zone
/// folded, the open nested session's chip kept (web `36rem`). The IDE's
/// chips are narrower than the web's (a 14px rem), so its widths are too.
const WIDE_W: f32 = 1200.;
const NARROWER_W: f32 = 960.;
const NARROW_W: f32 = 560.;

/// The header (web `Header`): the title (a lock when private) over THE
/// strip.
fn header(case: &Case, prefix: &'static str, window: &mut Window, cx: &mut App) -> Div {
    v_flex()
        .w(px(HEADER_W))
        .max_w_full()
        .gap_4()
        .child(title_row(&case.view.title, case.view.private, cx))
        .child(strip(case, case.view.tabs.clone(), StripPick::Conversation, HEADER_W, prefix, window, cx))
}

/// One row of a chip's subtree.
enum SubtreeRow {
    Issue {
        identifier: String,
        title: String,
        status: Option<ResolvedStatus>,
    },
    Session {
        name: String,
        state: SessionState,
    },
}

/// A session chip's lead ×4: the thread row mark of THAT session.
fn chip_state(mark: SessionTabMarkState) -> SessionState {
    match mark {
        SessionTabMarkState::NeedsYou => SessionState::NeedsYou,
        SessionTabMarkState::Working => SessionState::Working,
        SessionTabMarkState::Waiting => SessionState::Waiting,
        SessionTabMarkState::Ended => SessionState::Ended,
    }
}

/// A chip and its nested rows with their picks, depths relative to the chip
/// (0).
fn subtree_of(tab: &ThreadTab, issues: &HashMap<String, StripIssue>) -> Vec<(StripPick, SubtreeRow, usize)> {
    match tab {
        ThreadTab::Issue {
            issue_id,
            identifier,
            nested,
        } => {
            let issue = |id: &str, identifier: &str, depth: usize| {
                let info = issues.get(id).cloned().unwrap_or_default();
                (
                    StripPick::Issue(id.to_string()),
                    SubtreeRow::Issue {
                        identifier: identifier.to_string(),
                        title: info.title,
                        status: info.status,
                    },
                    depth,
                )
            };
            std::iter::once(issue(issue_id, identifier, 0))
                .chain(nested.iter().map(|item| issue(&item.issue_id, &item.identifier, item.depth)))
                .collect()
        }
        ThreadTab::Session {
            session_id,
            name,
            mark_state,
            nested,
        } => {
            let session = |id: &str, name: &str, mark: SessionTabMarkState, depth: usize| {
                (
                    StripPick::Session(id.to_string()),
                    SubtreeRow::Session {
                        name: name.to_string(),
                        state: chip_state(mark),
                    },
                    depth,
                )
            };
            std::iter::once(session(session_id, name, *mark_state, 0))
                .chain(nested.iter().map(|item| session(&item.session_id, &item.name, item.mark_state, item.depth)))
                .collect()
        }
        _ => Vec::new(),
    }
}

/// One subtree menu row, the way the thread screen paints it: THE tree row
/// (guides, the lead, the name, a session's state word in its tone), the
/// open one checked.
fn subtree_row(item: &SubtreeRow, guides: &domain::tree_guides::Guides, active: bool, cx: &App) -> Div {
    use domain::list_item::{GAP, INDENT, SMALL};
    let theme = cx.theme();
    let mut row = h_flex()
        .relative()
        .flex_shrink_0()
        .h(px(SMALL))
        .gap(px(GAP))
        .items_center()
        .pl(px(INDENT * guides.depth() as f32))
        .children(crate::tree_guides::guide_layer(guides, 0., 0.));
    row = match item {
        SubtreeRow::Issue {
            identifier,
            title,
            status,
        } => row
            .child(issue_glyph(status.as_ref(), cx))
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .font_family(theme::terminal::FONT_FAMILY)
                    .text_color(theme.muted_foreground)
                    .child(SharedString::from(identifier.clone())),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(px(320.))
                    .truncate()
                    .text_sm()
                    .text_color(theme.foreground)
                    .child(SharedString::from(title.clone())),
            ),
        SubtreeRow::Session { name, state } => {
            let word_color = match state {
                SessionState::NeedsYou | SessionState::Blocked => {
                    crate::work_card::session_tone_color(state.tone(), cx)
                }
                _ => theme.muted_foreground,
            };
            row.child(crate::run_rows::run_mark(crate::run_rows::run_agent(None), *state, cx))
                .child(
                    div()
                        .flex_shrink_0()
                        .max_w(px(320.))
                        .truncate()
                        .text_sm()
                        .text_color(theme.foreground)
                        .child(SharedString::from(name.clone())),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_sm()
                        .text_color(word_color)
                        .child(SharedString::from(state.word())),
                )
        }
    };
    let check = active.then(|| {
        Icon::new(registry::UI_CHECK)
            .size(px(14.))
            .text_color(theme.foreground)
            .into_any_element()
    });
    crate::controls::pointer_row_fill(
        h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .items_center()
            .child(row)
            .child(div().flex_1())
            .children(check),
        false,
    )
}

/// THE Menu of a chip's subtree (the thread screen's), drawn open: the chip
/// then its nested rows, one guide set, the open one checked.
fn subtree_menu(
    mut menu: PopupMenu,
    tab: Option<&ThreadTab>,
    issues: &HashMap<String, StripIssue>,
    open: &StripPick,
) -> PopupMenu {
    menu = menu.min_w(px(300.));
    let Some(tab) = tab else {
        return menu;
    };
    let rows = subtree_of(tab, issues);
    let depths: Vec<usize> = rows.iter().map(|(_, _, depth)| *depth).collect();
    for ((pick, row, _), guides) in rows.into_iter().zip(domain::tree_guides::guides_for(&depths)) {
        let active = *open == pick;
        menu = menu.item(PopupMenuItem::element(move |_, cx| subtree_row(&row, &guides, active, cx)));
    }
    menu
}

/// EXP-1334: the three menus drawn OPEN under the strips (the web's
/// `MenuPanel`s): a session chip's subtree (the nested `g2` open), an issue
/// chip's sub-issues (`i-3` open), the "+" (THE thread menu).
fn open_menus(fold: &Case, tabs: &[ThreadTab], window: &mut Window, cx: &mut App) -> Div {
    let muted = cx.theme().muted_foreground;
    let caption = |text: &'static str| div().text_xs().text_color(muted).child(text);
    let issues = strip_issues(fold, tabs);
    let session_tab = fold.view.tabs.iter().find(|tab| matches!(tab, ThreadTab::Session { .. })).cloned();
    let issue_tab = fold.view.tabs.iter().find(|tab| matches!(tab, ThreadTab::Issue { .. })).cloned();
    let session_menu: Entity<OpenMenu> = {
        let issues = issues.clone();
        window.use_keyed_state("sg-thread-parts-session-subtree", cx, move |window, cx| {
            OpenMenu::new(window, cx, move |menu, _, _| {
                subtree_menu(menu, session_tab.as_ref(), &issues, &StripPick::Session("g2".into()))
            })
        })
    };
    let issue_menu: Entity<OpenMenu> = window.use_keyed_state("sg-thread-parts-issue-subtree", cx, move |window, cx| {
        OpenMenu::new(window, cx, move |menu, _, _| {
            subtree_menu(menu, issue_tab.as_ref(), &issues, &StripPick::Issue("i-3".into()))
        })
    });
    let target = ThreadMenuTarget {
        root_id: fold.view.root_id.clone(),
        team_id: None,
        chip: None,
        linked: Vec::new(),
    };
    let add_menu: Entity<OpenMenu> = window.use_keyed_state("sg-thread-parts-add-menu", cx, move |window, cx| {
        OpenMenu::new(window, cx, move |menu, window, cx| thread_menu(menu, &target, window, cx))
    });
    h_flex()
        .mt_2()
        .flex_wrap()
        .items_start()
        .gap_8()
        .child(
            v_flex()
                .gap_2()
                .child(session_menu)
                .child(caption("A session chip's subtree, opened by a click (phones: the bottom sheet)")),
        )
        .child(v_flex().gap_2().child(issue_menu).child(caption("An issue chip's sub-issues")))
        .child(v_flex().gap_2().child(add_menu).child(caption("The \"+\" (left of Conversation)")))
}

/// One conversation row, the way the thread screen paints it.
pub(super) fn row(
    case: &Case,
    row: &SessionThreadRow,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    if let SessionThreadRow::Message { id, author, body, queued, .. } = row {
        let markdown = message_markdown(SharedString::from(format!("sg-thread-message-{id}")), body, &crate::thread_screen::ThreadText::default(), images);
        return match author.as_str() {
            domain::contract::MESSAGE_AUTHOR_KIND_USER => {
                let at = case
                    .messages
                    .iter()
                    .find(|message| message.id == *id)
                    .and_then(|message| message.created_at.as_deref());
                let name = crate::comments::author_label(None);
                user_bubble(id, markdown, bubble_caption(Some(&name), &clock(at)), *queued, true, cx)
            }
            domain::contract::MESSAGE_AUTHOR_KIND_AGENT => agent_prose(markdown, cx),
            _ => muted_line(registry::UI_INFO, body.clone(), cx),
        };
    }
    card_row(row, &case.view.root_id, &crate::thread_screen::ThreadText::default(), images, window, cx).unwrap_or_else(|| div().into_any_element())
}

/// The rows of the second case the first has no example of.
fn extra_rows(view: &SessionThreadView) -> Vec<&SessionThreadRow> {
    view.rows
        .iter()
        .filter(|row| {
            matches!(
                row,
                SessionThreadRow::Plan { .. } | SessionThreadRow::Subagent { .. } | SessionThreadRow::Picture { .. }
            )
        })
        .collect()
}

/// EXP-1323: the dispatcher case's report line and suggestions card.
fn dispatcher_rows(view: &SessionThreadView) -> Vec<&SessionThreadRow> {
    view.rows
        .iter()
        .filter(|row| matches!(row, SessionThreadRow::Report { .. } | SessionThreadRow::Suggestions { .. }))
        .collect()
}

/// EXP-1339: the demo pull requests per thread of the Threads list case
/// (`threads-list.json` `cases[0]`), so each word shows: `d` an open PR
/// alone (ready to merge), `e` an open PR based on another open one
/// (stack), `c` a merged PR (the violet lead, no word); `a` needs you with
/// no PR (question).
fn list_pull_requests() -> Vec<PullRequestRow> {
    let pull = |id: &str, root: &str, state: &str, base: Option<&str>| -> PullRequestRow {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "root_session_id": root,
            "state": state,
            "base_pull_request_id": base,
            "created_at": "2026-10-13T06:00:00.000Z",
        }))
        .expect("a demo pull request")
    };
    vec![
        pull("pr-d", "d", "open", None),
        pull("pr-base", "elsewhere", "open", None),
        pull("pr-e", "e", "open", Some("pr-base")),
        pull("pr-c", "c", "merged", None),
    ]
}

/// EXP-1339: the Threads list case — `cases[0]` partitioned by the domain
/// (`partition_threads`) plus the demo pull requests.
pub(super) fn threads_list_case() -> (Vec<ThreadsListGroup>, PrSnapshot) {
    let fixture: serde_json::Value =
        serde_json::from_str(fixture_rows::THREADS_LIST_JSON).expect("threads-list.json parses");
    let input = &fixture["cases"][0]["input"];
    let sessions: Vec<SessionRow> = fixture_rows::rows(&input["sessions"]);
    let links: Vec<SessionIssueRow> = fixture_rows::rows(&input["sessionIssues"]);
    let issues: Vec<TreeIssue> = serde_json::from_value(input["issues"].clone()).unwrap_or_default();
    let messages: Vec<MessageRow> = fixture_rows::rows(&input["messages"]);
    let reads: Vec<SessionReadRow> = fixture_rows::rows(&input["reads"]);
    let groups = partition_threads(ThreadsListInput {
        sessions: &sessions.iter().collect::<Vec<_>>(),
        session_issues: &links.iter().collect::<Vec<_>>(),
        issues: &issues,
        messages: &messages.iter().collect::<Vec<_>>(),
        team_id: input["teamId"].as_str(),
        reads: &reads.iter().collect::<Vec<_>>(),
        now_ms: None,
    });
    let snapshot = PrSnapshot {
        pulls: list_pull_requests(),
        ..PrSnapshot::default()
    };
    (groups, snapshot)
}

/// The Threads list's width (the page's column, web `max-w-3xl`).
const LIST_W: f32 = 768.;

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window
        .use_keyed_state("sg-thread-parts-images", cx, |_, _| ImageCache::new(None))
        .clone();
    let first = case(0);
    let second = case(1);
    let mut conversation = v_flex().w(px(HEADER_W)).max_w_full().gap_5();
    for item in &first.view.rows {
        conversation = conversation.child(row(&first, item, &images, window, cx));
    }
    for item in extra_rows(&second.view) {
        conversation = conversation.child(row(&second, item, &images, window, cx));
    }
    let dispatcher = case(case_named(DISPATCHER_CASE));
    for item in dispatcher_rows(&dispatcher.view) {
        conversation = conversation.child(row(&dispatcher, item, &images, window, cx));
    }
    // The header twice: the first case, then the private one with its lock.
    let header_first = header(&first, "sg-thread", window, cx);
    let header_private = header(&second, "sg-thread-private", window, cx);
    // EXP-1334: the folded strip at three widths (full → identifiers → each
    // zone's "+N", a nested session open), then the menus, open.
    let fold = case(case_named(FOLD_CASE));
    let tabs = folded_tabs(&fold.view);
    let wide = strip(&fold, tabs.clone(), StripPick::Conversation, WIDE_W, "sg-thread-wide", window, cx);
    let narrower = strip(&fold, tabs.clone(), StripPick::Conversation, NARROWER_W, "sg-thread-narrower", window, cx);
    let narrow = strip(&fold, tabs.clone(), StripPick::Session("g2".into()), NARROW_W, "sg-thread-narrow", window, cx);
    let menus = open_menus(&fold, &tabs, window, cx);
    let muted = cx.theme().muted_foreground;
    let caption = |text: &'static str| div().text_xs().text_color(muted).child(text);
    v_flex()
        .gap_10()
        .text_sm()
        .child(header_first)
        .child(header_private)
        .child(
            v_flex()
                .w(px(WIDE_W))
                .max_w_full()
                .gap_3()
                .child(caption("Wide: every chip full (identifier + title)"))
                .child(wide)
                .child(caption("Narrower: the issues give up their titles first"))
                .child(narrower)
                .child(caption("Narrow: each zone folds into its own \"+N\"; the open chip (a nested session) stays"))
                .child(narrow)
                .child(menus),
        )
        .child(conversation)
        .child({
            let (groups, snapshot) = threads_list_case();
            v_flex()
                .w(px(LIST_W))
                .max_w_full()
                .gap_2()
                .child(caption("The Threads list: the page's rows (stack, ready to merge, a question; merged = the violet lead, no word)"))
                .children(crate::threads_view::thread_list_bands(&groups, &snapshot, cx))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo draws every row kind the conversation has, the session card
    /// first.
    #[test]
    fn the_demo_draws_every_row_kind() {
        let first = case(0);
        let second = case(1);
        let dispatcher = case(case_named(DISPATCHER_CASE));
        let kinds: std::collections::BTreeSet<&str> = first
            .view
            .rows
            .iter()
            .chain(extra_rows(&second.view))
            .chain(dispatcher_rows(&dispatcher.view))
            .map(|row| match row {
                SessionThreadRow::Message { .. } => "message",
                SessionThreadRow::Session { .. } => "session",
                SessionThreadRow::Question { .. } => "question",
                SessionThreadRow::Plan { .. } => "plan",
                SessionThreadRow::Subagent { .. } => "subagent",
                SessionThreadRow::Picture { .. } => "picture",
                SessionThreadRow::Event { .. } => "event",
                SessionThreadRow::Report { .. } => "report",
                SessionThreadRow::Suggestions { .. } => "suggestions",
            })
            .collect();
        assert_eq!(kinds.len(), 9, "{kinds:?}");
        // EXP-1334: the dispatcher has no card of its own.
        assert!(!matches!(first.view.rows.first(), Some(SessionThreadRow::Session { .. })));
        assert_eq!(first.view.title, "#EXP-12 Login fails on Safari + 1");
        // The second header is the private one.
        assert!(second.view.private);
    }

    /// EXP-1339: the Threads list specimen shows every word (merged as
    /// the violet lead with no text) and no row carries a lock.
    #[test]
    fn the_threads_list_shows_every_word() {
        use domain::session_tree::ThreadRowWord;
        let (groups, snapshot) = threads_list_case();
        let words: std::collections::BTreeSet<&str> = groups
            .iter()
            .flat_map(|group| group.rows.iter())
            .filter(|row| row.depth == 0)
            .filter_map(|row| {
                crate::threads_view::thread_word(&row.session_id, row.busy, row.needs_you, &snapshot.pulls)
            })
            .map(ThreadRowWord::key)
            .collect();
        assert_eq!(words, ["merged", "question", "readyToMerge", "stack"].into_iter().collect());
        assert_eq!(ThreadRowWord::Merged.text(), None);
        let merged = crate::threads_view::root_pull_request("c", &snapshot.pulls).expect("c's pull request");
        assert!(merged.is_merged());
    }

    /// EXP-1322: the demo's open question shows an option's description
    /// under its label (the session view's card).
    #[test]
    fn the_open_question_carries_a_described_option() {
        let first = case(0);
        let described = first.view.rows.iter().any(|row| match row {
            SessionThreadRow::Question { options, answered: false, .. } => {
                options.iter().any(|option| option.description.is_some())
            }
            _ => false,
        });
        assert!(described, "an unanswered question with a described option");
    }

    /// EXP-1334: the fold case's strip carries the web's two extra issues
    /// after its own and before the sessions; the statuses alternate in
    /// progress / backlog by chip index; the open ids are the web's.
    #[test]
    fn the_fold_strip_is_the_webs() {
        let fold = case(case_named(FOLD_CASE));
        let tabs = folded_tabs(&fold.view);
        let identifiers: Vec<&str> = tabs
            .iter()
            .filter_map(|tab| match tab {
                ThreadTab::Issue { identifier, .. } => Some(identifier.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(identifiers, vec!["EXP-21", "EXP-24", "EXP-25", "EXP-26"]);
        let last_issue = tabs.iter().rposition(|tab| matches!(tab, ThreadTab::Issue { .. })).unwrap();
        let first_session = tabs.iter().position(|tab| matches!(tab, ThreadTab::Session { .. })).unwrap();
        assert!(last_issue < first_session);
        assert!(matches!(tabs.last(), Some(ThreadTab::Guide { .. })));
        let issues = strip_issues(&fold, &tabs);
        assert_eq!(issues["i-5"].title, "Context ring");
        assert_eq!(issues["i-6"].title, "Model picker");
        let status = |id: &str| issues[id].status.as_ref().map(|status| status.group_key.clone());
        assert_eq!(status("i-5"), status("i-1"));
        assert_ne!(status("i-6"), status("i-5"));
        assert!(tabs.iter().any(|tab| matches!(tab, ThreadTab::Session { nested, .. } if nested.iter().any(|row| row.session_id == "g2"))));
        assert!(tabs.iter().any(|tab| matches!(tab, ThreadTab::Issue { nested, .. } if nested.iter().any(|row| row.issue_id == "i-3"))));
    }
}
