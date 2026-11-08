//! EXP-1284/EXP-1294: `chat-parts` (Special components): the chat
//! screen's pieces, the IDE half of the styleguide entry, case for case with
//! the web one — the REAL painters (`crate::session_chat`) over the
//! contract model (`domain::session_tree::build_session_chat`): #EXP-1732
//! THE session screen's header twice (`fixtures/session-chat.json`'s first
//! case, then the private one with its lock): the lead disc, the title over
//! its subtitle, the `…`, THE Segmented under it (Chat · Issues · Sessions ·
//! Results with the muted counts); then the fold case's Issues tab (THE issue
//! rows, sub-issues nested, the `+`) and Sessions tab (THE tree rows), and the
//! "+" menu (Link an issue ›, New issue) drawn OPEN; then every conversation
//! row in order
//! (EXP-1334: no root card) — the person's bubble, the agent's prose, an
//! event line, a child's session card, a question answered in place (an
//! option with its description, EXP-1322) and the queued reply; the rows
//! the first case has no example of (a plan, a subagent, a picture) follow
//! from the second case, then (EXP-1323) the dispatcher case's report line
//! and suggestions card in every item state. THE composer the chat hosts
//! is the `session-chat` entry's (the web's too). Last (EXP-1339), the
//! Chats list: THE page's bands and rows
//! ([`crate::chats_view::chat_list_bands`]) over `chat-list.json`'s
//! first case through `partition_chats`, with demo pull requests so every
//! trailing word shows: stack, ready to merge, a question, and a merged row
//! (the violet pr-merged lead, no word). No lock on a list row.
//!
//! No `sync::Store` here: authors read `Someone`, no `#IDENT` pills, and the
//! image cache has no transport (the picture paints its placeholder box).


use gpui::{
    div, px, AnyElement, App, Div, Entity, IntoElement as _, ParentElement as _, SharedString, Styled as _,
    Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use domain::rows::{MessageRow, PullRequestRow, SessionIssueRow, SessionReadRow, SessionRow};
use domain::session_tree::{
    build_session_chat, partition_chats, SessionChatInput,
    SessionChatRow, SessionChatView, ChatTab, ChatsListGroup, ChatsListInput, TreeIssue,
};
use domain::statuses::ResolvedStatus;

use crate::markdown::ImageCache;
use crate::session_data::PrSnapshot;
use crate::session_chat::{
    card_row, chat_menu, conversation_input, message_markdown, sender_row, title_row, transcript_item,
    transcript_message, ChatMenuTarget, MessageBubble, TreeItem,
};
use domain::transcript_grouping::{
    transcript_layout, TranscriptAgent, TranscriptChat, TranscriptInput, TranscriptItem, TranscriptMember,
    TranscriptRowLayout, TranscriptSession,
};

use super::composer_dialog::OpenMenu;
use super::fixture_rows;

pub(crate) const ID: &str = "chat-parts";
pub(crate) const OWNER: &str = "EXP-1293";

/// One fixture case: the view, plus the messages it was built from (a
/// bubble's caption reads its `createdAt`).
pub(super) struct Case {
    pub(super) view: SessionChatView,
    messages: Vec<MessageRow>,
    issues: Vec<TreeIssue>,
}

/// EXP-1323: the index of the first fixture case whose name starts with
/// `prefix` (the dispatcher case: suggestions, a report, an aside).
pub(super) fn case_named(prefix: &str) -> usize {
    let fixture: serde_json::Value =
        serde_json::from_str(fixture_rows::SESSION_CHAT_JSON).expect("session-chat.json parses");
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

/// The ended private chat's name prefix (its header carries the lock).
pub(super) const PRIVATE_CASE: &str = "an ended private chat";

/// Fixture case `index`, built through the shared model.
pub(super) fn case(index: usize) -> Case {
    let fixture: serde_json::Value =
        serde_json::from_str(fixture_rows::SESSION_CHAT_JSON).expect("session-chat.json parses");
    let input = &fixture["cases"][index]["input"];
    let root: SessionRow = fixture_rows::row(&input["root"]);
    let sessions: Vec<SessionRow> = fixture_rows::rows(&input["sessions"]);
    let links: Vec<SessionIssueRow> = fixture_rows::rows(&input["sessionIssues"]);
    let issues: Vec<TreeIssue> = serde_json::from_value(input["issues"].clone()).unwrap_or_default();
    let messages: Vec<MessageRow> = fixture_rows::messages(&input["messages"]);
    let pulls: Vec<PullRequestRow> = fixture_rows::rows(&input["pullRequests"]);
    let view = build_session_chat(
        SessionChatInput {
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

/// EXP-1334's case: issues with sub-issues + a session subtree.
pub(super) const FOLD_CASE: &str = "EXP-1334: the root has no chip";

/// The demo statuses (no store here), the web's: in progress, backlog, by
/// turns.
fn status_of(index: usize) -> ResolvedStatus {
    domain::statuses::constructed_default([domain::IssueStatus::InProgress, domain::IssueStatus::Backlog][index % 2])
}

/// #EXP-1732: the case's Issues tab rows: its issue tabs (sub-issues nested
/// under their parent), the statuses by turns, the titles off the case.
pub(super) fn issue_items(case: &Case) -> Vec<(TreeItem, usize)> {
    let title = |id: &str| case.issues.iter().find(|issue| issue.id == id).map(|issue| issue.title.clone()).unwrap_or_default();
    let mut out: Vec<(TreeItem, usize)> = Vec::new();
    for tab in &case.view.tabs {
        if let ChatTab::Issue { issue_id, identifier, nested } = tab {
            let rows = std::iter::once((issue_id, identifier, 0)).chain(nested.iter().map(|n| (&n.issue_id, &n.identifier, n.depth)));
            for (issue_id, identifier, depth) in rows {
                let index = out.len();
                out.push((
                    TreeItem::Issue {
                        issue_id: issue_id.clone(),
                        identifier: identifier.clone(),
                        title: title(issue_id),
                        status: Some(status_of(index)),
                    },
                    depth,
                ));
            }
        }
    }
    out
}

/// #EXP-1732: the case's Sessions tab rows (THE tree rows, no store here:
/// the view's session tabs and the sessions folded under them).
pub(super) fn session_items(case: &Case) -> Vec<(TreeItem, usize)> {
    let mut out = Vec::new();
    for tab in &case.view.tabs {
        if let ChatTab::Session { session_id, name, mark_state, nested } = tab {
            out.push((TreeItem::Session { session_id: session_id.clone(), name: name.clone(), state: *mark_state, cli: None }, 0));
            out.extend(nested.iter().map(|item| {
                (
                    TreeItem::Session { session_id: item.session_id.clone(), name: item.name.clone(), state: item.mark_state, cli: None },
                    item.depth,
                )
            }));
        }
    }
    out
}

/// The header's width (web `48rem`).
const HEADER_W: f32 = 768.;

/// THE session screen header (#EXP-1732) over `case`: the lead disc, the
/// title (a lock when private) over its subtitle, the `…`, THE Segmented with
/// the case's counts.
fn header(case: &Case, cx: &mut App) -> Div {
    use domain::session_screen::{session_screen, SessionScreenCounts};
    let counts = SessionScreenCounts {
        issues: issue_items(case).len() as u64,
        sessions: session_items(case).len() as u64,
        results: 0,
    };
    let screen = session_screen("session", true, None, None, counts);
    let tabs = crate::session_chat::screen_tabs(&screen.tabs, screen.active, std::rc::Rc::new(|_, _, _| {}), cx);
    let avatar = crate::chat_list::chat_lead_disc(domain::chats::ChatLead::Waiting, None, false, cx);
    let menu = crate::controls::ghost_icon_button("sg-chat-more", gpui_component::Icon::new(crate::icons::registry::UI_MORE), cx)
        .tooltip("More")
        .into_any_element();
    let title = title_row(&case.view.title, case.view.private, cx).into_any_element();
    let subtitle = domain::session_screen::session_screen_subtitle(Some("Idle"), Some("Alex's MacBook"), None);
    v_flex().w(px(HEADER_W)).max_w_full().child(crate::session_chat::screen_header(
        crate::session_chat::ScreenHeader {
            avatar,
            title,
            subtitle,
            subtitle_color: None,
            trailing: Vec::new(),
            gear: None,
            menu: Some(menu),
            back: None,
            tabs,
        },
        cx,
    ))
}

/// #EXP-1732: the fold case's Issues and Sessions tab bodies (THE tree rows,
/// the first issue open) and the "+" menu drawn OPEN (the web's
/// `MenuPanel`).
fn tab_bodies(fold: &Case, window: &mut Window, cx: &mut App) -> Div {
    let muted = cx.theme().muted_foreground;
    let caption = |text: &'static str| div().text_xs().text_color(muted).child(text);
    let target = ChatMenuTarget {
        root_id: fold.view.root_id.clone(),
        team_id: None,
        chip: None,
        linked: Vec::new(),
    };
    let issues = issue_items(fold);
    let open = issues.first().map(|(item, _)| item.key().to_string());
    let issue_rows = crate::session_chat::tree_rows("sg-chat-issues", &issues, open.as_deref(), std::rc::Rc::new(|_, _, _| {}), None, cx);
    let sessions = session_items(fold);
    let session_rows = crate::session_chat::tree_rows("sg-chat-sessions", &sessions, None, std::rc::Rc::new(|_, _, _| {}), None, cx);
    let add = crate::session_chat::issues_add("sg-chat", target.clone(), cx);
    let add_menu: Entity<OpenMenu> = window.use_keyed_state("sg-chat-parts-add-menu", cx, move |window, cx| {
        OpenMenu::new(window, cx, move |menu, window, cx| chat_menu(menu, &target, window, cx))
    });
    h_flex()
        .mt_2()
        .flex_wrap()
        .items_start()
        .gap_8()
        .child(
            v_flex()
                .w(px(384.))
                .gap_2()
                .child(caption("Issues: the + (Link an issue, New issue), THE issue rows, sub-issues nested; one open"))
                .child(v_flex().w_full().child(add).children(issue_rows)),
        )
        .child(
            v_flex()
                .w(px(384.))
                .gap_2()
                .child(caption("Sessions: THE tree rows of every session under the root"))
                .child(v_flex().w_full().children(session_rows)),
        )
        .child(v_flex().gap_2().child(add_menu).child(caption("The \"+\" menu")))
}

/// One conversation row, the way the chat screen paints it: THE bubble
/// under its sender (#EXP-1736), every card as it is.
pub(super) fn row(
    case: &Case,
    row: &SessionChatRow,
    layout: Option<&TranscriptRowLayout>,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    let text = crate::session_chat::ChatText::default();
    let at = case
        .messages
        .iter()
        .find(|message| message.id == row.id())
        .and_then(|message| message.created_at.as_deref());
    let content = match transcript_message(row, layout, "sg-chat-message", at, &text, images, cx) {
        Some(bubble) => bubble,
        None => card_row(row, &case.view.root_id, &case.view.rows, &text, images, window, cx)
            .unwrap_or_else(|| div().into_any_element()),
    };
    sender_row(layout, content, cx)
}

/// #EXP-1736: `rows` of `case` as the chat screen lays them out: the
/// grouping (the case's first person = the viewer, no store here), each row
/// under its gap.
pub(super) fn conversation_rows(
    case: &Case,
    rows: &[&SessionChatRow],
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> Vec<AnyElement> {
    let mut input = conversation_input(
        rows.iter().copied(),
        &case.view.root_id,
        &case.view.title,
        None,
        case.view.origin.as_ref(),
        true,
        cx,
    );
    input.viewer_id = rows.iter().find_map(|row| match row {
        SessionChatRow::Message { author, author_id, .. } if author == domain::contract::MESSAGE_AUTHOR_KIND_USER => {
            author_id.clone()
        }
        _ => None,
    });
    input.items = rows.iter().map(|row| transcript_item(row)).collect();
    let layouts = transcript_layout(&input);
    rows.iter()
        .zip(&layouts)
        .map(|(item, layout)| {
            let painted = row(case, item, Some(layout), images, window, cx);
            div().w_full().pt(px(layout.gap)).child(painted).into_any_element()
        })
        .collect()
}

/// #EXP-1736: THE transcript grouped (transcript-grouping.json): a member
/// (initials in their hue), the agent's group of two bubbles (one avatar,
/// one name), a cross-session report (`name ›`, a press opens it), the
/// viewer's own bubble right with neither.
pub(super) fn grouped_sample(images: &Entity<ImageCache>, cx: &mut App) -> AnyElement {
    let input = TranscriptInput {
        viewer_id: Some("sg-u1".into()),
        chat: TranscriptChat {
            session_id: "sg-a1".into(),
            title: "Triage".into(),
            agent_id: Some("sg-ag1".into()),
            origin_session_id: None,
            from_start: true,
        },
        members: vec![TranscriptMember { id: "sg-u2".into(), name: "Dana Reyes".into() }],
        agents: vec![TranscriptAgent { id: "sg-ag1".into(), name: "Triage".into() }],
        sessions: vec![TranscriptSession {
            id: "sg-c1".into(),
            title: "Fix the sign-in redirect".into(),
            agent_id: None,
        }],
        items: vec![
            TranscriptItem::message("g1", "user", Some("sg-u2"), None),
            TranscriptItem::message("g2", "agent", None, None),
            TranscriptItem::message("g3", "agent", None, None),
            TranscriptItem::message("g4", "agent", None, Some("sg-c1")),
            TranscriptItem::message("g5", "user", Some("sg-u1"), None),
        ],
    };
    let bodies = [
        ("Sign-in loops back to the login page on Safari.", "09:12"),
        ("Looking at it: the redirect drops the return path.", "09:13"),
        ("I started a session to fix it.", "09:13"),
        ("Opened PR #128: the redirect keeps the return path.", "09:31"),
        ("Thanks, merging after review.", "09:34"),
    ];
    let text = crate::session_chat::ChatText::default();
    let layouts = transcript_layout(&input);
    let rows: Vec<AnyElement> = layouts
        .iter()
        .zip(bodies)
        .map(|(layout, (body, time))| {
            let bubble = crate::session_chat::message_bubble(
                MessageBubble {
                    id: SharedString::from(format!("sg-grouped-bubble-{}", layout.id)),
                    body: message_markdown(SharedString::from(format!("sg-grouped-{}", layout.id)), body, &text, images)
                        .into_any_element(),
                    time: time.to_string(),
                    own: layout.side == domain::transcript_grouping::TranscriptSide::Own,
                    name: layout
                        .sender
                        .as_ref()
                        .filter(|_| layout.name)
                        .map(|sender| sender.display_name()),
                    link: layout.sender.as_ref().and_then(|sender| sender.link.clone()),
                    queued: false,
                    unqueue: None,
                    width: None,
                },
                cx,
            );
            let painted = sender_row(Some(layout), bubble, cx);
            div().w_full().pt(px(layout.gap)).child(painted).into_any_element()
        })
        .collect();
    v_flex().w_full().children(rows).into_any_element()
}

/// The rows of the second case the first has no example of.
fn extra_rows(view: &SessionChatView) -> Vec<&SessionChatRow> {
    view.rows
        .iter()
        .filter(|row| {
            matches!(
                row,
                SessionChatRow::Plan { .. } | SessionChatRow::Subagent { .. } | SessionChatRow::Picture { .. }
            )
        })
        .collect()
}

/// EXP-1323: the dispatcher case's report line and suggestions card.
fn dispatcher_rows(view: &SessionChatView) -> Vec<&SessionChatRow> {
    view.rows
        .iter()
        .filter(|row| matches!(row, SessionChatRow::Report { .. } | SessionChatRow::Suggestions { .. }))
        .collect()
}

/// EXP-1339: the demo pull requests per chat of the Chats list case
/// (`chat-list.json` `cases[0]`), so each word shows: `d` an open PR
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

/// EXP-1339: the Chats list case — `cases[0]` partitioned by the domain
/// (`partition_chats`) plus the demo pull requests.
pub(super) fn chats_list_case() -> (Vec<ChatsListGroup>, PrSnapshot) {
    let fixture: serde_json::Value =
        serde_json::from_str(fixture_rows::CHATS_LIST_JSON).expect("chat-list.json parses");
    let input = &fixture["cases"][0]["input"];
    let sessions: Vec<SessionRow> = fixture_rows::rows(&input["sessions"]);
    let links: Vec<SessionIssueRow> = fixture_rows::rows(&input["sessionIssues"]);
    let issues: Vec<TreeIssue> = serde_json::from_value(input["issues"].clone()).unwrap_or_default();
    let messages: Vec<MessageRow> = fixture_rows::messages(&input["messages"]);
    let reads: Vec<SessionReadRow> = fixture_rows::rows(&input["reads"]);
    let groups = partition_chats(ChatsListInput {
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

/// The Chats list's width (the page's column, web `max-w-3xl`).
const LIST_W: f32 = 768.;

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window
        .use_keyed_state("sg-chat-parts-images", cx, |_, _| ImageCache::new(None))
        .clone();
    let first = case(0);
    let second = case(case_named(PRIVATE_CASE));
    // #EXP-1736: the grouped sample, then each case's rows under the
    // grouping (one block per case).
    let grouped = grouped_sample(&images, cx);
    let first_rows: Vec<&SessionChatRow> = first.view.rows.iter().collect();
    let first_block = conversation_rows(&first, &first_rows, &images, window, cx);
    let second_block = conversation_rows(&second, &extra_rows(&second.view), &images, window, cx);
    let dispatcher = case(case_named(DISPATCHER_CASE));
    let dispatcher_block = conversation_rows(&dispatcher, &dispatcher_rows(&dispatcher.view), &images, window, cx);
    let conversation = v_flex()
        .w(px(HEADER_W))
        .max_w_full()
        .gap_5()
        .child(grouped)
        .child(v_flex().w_full().children(first_block))
        .child(v_flex().w_full().children(second_block))
        .child(v_flex().w_full().children(dispatcher_block));
    // #EXP-1732: THE header twice: the first case, then the private one
    // with its lock; then the fold case's Issues and Sessions tabs.
    let header_first = header(&first, cx);
    let header_private = header(&second, cx);
    let fold = case(case_named(FOLD_CASE));
    let bodies = tab_bodies(&fold, window, cx);
    let muted = cx.theme().muted_foreground;
    let caption = |text: &'static str| div().text_xs().text_color(muted).child(text);
    v_flex()
        .gap_10()
        .text_sm()
        .child(header_first)
        .child(
            v_flex()
                .gap_3()
                .child(header_private)
                .child(caption("One session (no child yet): the session's own work is the chat")),
        )
        .child(bodies)
        .child(conversation)
        .child({
            let (groups, snapshot) = chats_list_case();
            v_flex()
                .w(px(LIST_W))
                .max_w_full()
                .gap_2()
                .child(caption("The Chats list: the page's rows (stack, ready to merge, a question; merged = the violet lead, no word)"))
                .children(crate::chats_view::chat_list_bands(&groups, &snapshot, cx))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-1459: sealed-content.json `placeholder.rowCases` through the
    /// chat model on `session-chat.json`'s first case.
    #[test]
    fn locked_row_cases_match_the_fixture() {
        let fixture: serde_json::Value =
            serde_json::from_str(fixture_rows::SESSION_CHAT_JSON).expect("session-chat.json parses");
        let input = &fixture["cases"][0]["input"];
        let root: SessionRow = fixture_rows::row(&input["root"]);
        let sessions: Vec<SessionRow> = fixture_rows::rows(&input["sessions"]);
        let links: Vec<SessionIssueRow> = fixture_rows::rows(&input["sessionIssues"]);
        let issues: Vec<TreeIssue> = serde_json::from_value(input["issues"].clone()).unwrap_or_default();
        let pulls: Vec<PullRequestRow> = fixture_rows::rows(&input["pullRequests"]);
        let sealed: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../../packages/domain-contract/fixtures/sealed-content.json"
        ))
        .unwrap();
        for c in sealed["placeholder"]["rowCases"].as_array().unwrap() {
            let m = &c["message"];
            let message = MessageRow {
                id: m["id"].as_str().unwrap().into(),
                session_id: root.id.clone(),
                author_kind: m["authorKind"].as_str().map(str::to_string),
                body: Some(String::new()),
                card: m.get("card").cloned(),
                consumed_at: Some("2026-10-13T10:00:00.000Z".into()),
                created_at: Some("2026-10-13T10:05:00.000Z".into()),
                seal: match m["locked"].as_str() {
                    Some("noKey") => Some(domain::sealed::SealState::Locked),
                    _ => Some(domain::sealed::SealState::Unreadable),
                },
                ..MessageRow::default()
            };
            let view = build_session_chat(
                SessionChatInput {
                    root: &root,
                    sessions: &sessions.iter().collect::<Vec<_>>(),
                    session_issues: &links.iter().collect::<Vec<_>>(),
                    issues: &issues,
                    messages: &[&message],
                    pull_requests: &pulls.iter().collect::<Vec<_>>(),
                    now_ms: fixture_rows::now_ms(&input["now"]),
                },
                None,
            );
            let rows: Vec<serde_json::Value> = view
                .rows
                .iter()
                .filter(|row| row.id() == message.id)
                .map(|row| serde_json::to_value(row).unwrap())
                .collect();
            assert_eq!(rows, vec![c["expected"].clone()], "{}", c["name"]);
        }
    }

    /// The demo draws every row kind the conversation has, the session card
    /// first.
    #[test]
    fn the_demo_draws_every_row_kind() {
        let first = case(0);
        let second = case(case_named(PRIVATE_CASE));
        let dispatcher = case(case_named(DISPATCHER_CASE));
        let kinds: std::collections::BTreeSet<&str> = first
            .view
            .rows
            .iter()
            .chain(extra_rows(&second.view))
            .chain(dispatcher_rows(&dispatcher.view))
            .map(|row| match row {
                SessionChatRow::Message { .. } => "message",
                SessionChatRow::Session { .. } => "session",
                SessionChatRow::Question { .. } => "question",
                SessionChatRow::Plan { .. } => "plan",
                SessionChatRow::Subagent { .. } => "subagent",
                SessionChatRow::Picture { .. } => "picture",
                SessionChatRow::Event { .. } => "event",
                SessionChatRow::Report { .. } => "report",
                SessionChatRow::Suggestions { .. } => "suggestions",
                SessionChatRow::Divider { .. } => "divider",
                SessionChatRow::Locked { .. } => "locked",
                SessionChatRow::Surface { .. } => "surface",
            })
            .collect();
        assert_eq!(kinds.len(), 9, "{kinds:?}");
        // EXP-1334: the dispatcher has no card of its own.
        assert!(!matches!(first.view.rows.first(), Some(SessionChatRow::Session { .. })));
        assert_eq!(first.view.title, "#EXP-12 Login fails on Safari + 1");
        // The second header is the private one.
        assert!(second.view.private);
    }

    /// EXP-1339: the Chats list specimen shows every word (merged as
    /// the violet lead with no text) and no row carries a lock.
    #[test]
    fn the_chats_list_shows_every_word() {
        use domain::session_tree::ChatRowWord;
        let (groups, snapshot) = chats_list_case();
        let words: std::collections::BTreeSet<&str> = groups
            .iter()
            .flat_map(|group| group.rows.iter())
            .filter(|row| row.depth == 0)
            .filter_map(|row| {
                crate::chats_view::chat_word(&row.session_id, row.busy, row.needs_you, row.offline, &snapshot.pulls)
            })
            .map(ChatRowWord::key)
            .collect();
        assert_eq!(words, ["merged", "question", "readyToMerge", "stack"].into_iter().collect());
        assert_eq!(ChatRowWord::Merged.text(), None);
        let merged = crate::chats_view::root_pull_request("c", &snapshot.pulls).expect("c's pull request");
        assert!(merged.is_merged());
    }

    /// EXP-1322: the demo's open question shows an option's description
    /// under its label (the session view's card).
    #[test]
    fn the_open_question_carries_a_described_option() {
        let first = case(0);
        let described = first.view.rows.iter().any(|row| match row {
            SessionChatRow::Question { options, answered: false, .. } => {
                options.iter().any(|option| option.description.is_some())
            }
            _ => false,
        });
        assert!(described, "an unanswered question with a described option");
    }

    /// #EXP-1732: the fold case's Issues tab = every issue of the tree, a
    /// sub-issue nested under its parent; its Sessions tab = the tree rows.
    #[test]
    fn the_tab_rows_are_the_trees() {
        let fold = case(case_named(FOLD_CASE));
        let issues = issue_items(&fold);
        assert!(issues.iter().any(|(item, _)| item.key() == "i-3"), "the nested issue is a row");
        assert!(issues.iter().any(|(_, depth)| *depth > 0), "a sub-issue is nested");
        let sessions = session_items(&fold);
        assert!(sessions.iter().any(|(item, _)| item.key() == "g2"));
    }
}
