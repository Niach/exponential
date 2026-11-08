//! EXP-1284/EXP-1294: `chat-parts` (Special components): the chat
//! screen's pieces, the IDE half of the styleguide entry, case for case with
//! the web one — the REAL painters (`crate::session_chat`) over the
//! contract model (`domain::session_tree::build_session_chat`): the header
//! twice (`fixtures/session-chat.json`'s first case, then the private one
//! with its lock): the title and the ONE tab strip (+ · Conversation · the
//! one issue chip or the group tab · Results; NEW-6: the
//! private one is a ONE-session chat, so no Conversation tab); then
//! (#EXP-1433) the strip's two modes: ONE issue keeps its chip; anything
//! more = ONE group tab (the count at rest; active with a session's name or
//! an issue's identifier while it is open), the group's Menu (issues, a
//! separator, the session tree) and the "+" menu (Link an issue ›, New
//! issue) drawn OPEN; then every conversation row in order
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

use std::collections::HashMap;

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

use crate::icons::registry;
use crate::markdown::ImageCache;
use crate::session_data::PrSnapshot;
use crate::session_chat::{
    agent_prose, bubble_caption, card_row, clock, message_markdown, muted_line, chat_menu,
    strip_group_items, strip_group_menu, chat_strip, title_row, user_bubble, StripIssue, StripPick,
    ChatMenuTarget, ChatStrip,
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

/// The web entry's two extra issues on the fold case: `(id, identifier,
/// title)`.
const EXTRA_ISSUES: [(&str, &str, &str); 2] = [("i-5", "EXP-25", "Context ring"), ("i-6", "EXP-26", "Model picker")];

/// #EXP-1433 `chips`: Conversation and ONE issue (web `ONE_ISSUE_TABS`).
fn one_issue_tabs(view: &SessionChatView) -> Vec<ChatTab> {
    let mut tabs: Vec<ChatTab> =
        view.tabs.iter().filter(|tab| matches!(tab, ChatTab::Add | ChatTab::Conversation)).cloned().collect();
    tabs.push(ChatTab::Issue {
        issue_id: EXTRA_ISSUES[0].0.to_string(),
        identifier: EXTRA_ISSUES[0].1.to_string(),
        nested: Vec::new(),
    });
    tabs
}

/// #EXP-1433 `group`: the fold case's tabs with the extra issues after its
/// own, before the sessions (web `GROUP_TABS`).
fn group_tabs(view: &SessionChatView) -> Vec<ChatTab> {
    let trailing = |tab: &ChatTab| matches!(tab, ChatTab::Session { .. });
    let mut tabs: Vec<ChatTab> = view.tabs.iter().filter(|tab| !trailing(tab)).cloned().collect();
    tabs.extend(EXTRA_ISSUES.iter().map(|(id, identifier, _)| ChatTab::Issue {
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
/// extras) names, the status by the issue's index in the strip's flat issue
/// list (a chip's sub-issues after it, web `stripIssues`).
fn strip_issues(case: &Case, tabs: &[ChatTab]) -> HashMap<String, StripIssue> {
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
    let flat: Vec<&str> = tabs
        .iter()
        .flat_map(|tab| match tab {
            ChatTab::Issue { issue_id, nested, .. } => std::iter::once(issue_id.as_str())
                .chain(nested.iter().map(|item| item.issue_id.as_str()))
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect();
    for (index, id) in flat.into_iter().enumerate() {
        if let Some(issue) = issues.get_mut(id) {
            issue.status = Some(status_of(index));
        }
    }
    issues
}

/// The strip's spec over `tabs` (the screen's inputs, no store).
fn strip_spec(case: &Case, tabs: Vec<ChatTab>, open: StripPick, prefix: &'static str) -> ChatStrip {
    let issues = strip_issues(case, &tabs);
    ChatStrip {
        prefix,
        tabs,
        issues,
        clis: Default::default(),
        open,
        menu: Some(ChatMenuTarget {
            root_id: case.view.root_id.clone(),
            team_id: None,
            chip: None,
            linked: Vec::new(),
        }),
        on_pick: std::rc::Rc::new(|_, _, _| {}),
        // The web specimen's: Results pinned right.
        results: true,
        // #EXP-1562: the case's WAITING cards as the strip's chips.
        waiting: crate::waiting_cards::chat_waiting_cards(&case.view.rows),
        on_waiting: Some(std::rc::Rc::new(|_, _, _| {})),
        // #EXP-1662: the case's plan as the strip's chip. #FEED-191: the visit
        // KEEPS its approved plan, so the chip shows THE check.
        plan: crate::session_chat::tab_plan(&case.view.rows, approved_plan(&case.view.rows))
            .and_then(crate::session_chat::strip_plan_of),
    }
}

/// #FEED-191: the case's newest approved plan (the specimen's `kept`).
fn approved_plan(rows: &[SessionChatRow]) -> Option<&str> {
    rows.iter().rev().find_map(|row| match row {
        SessionChatRow::Plan { id, decision: Some(decision), .. } if decision == "approved" => Some(id.as_str()),
        _ => None,
    })
}

/// THE strip the chat screen draws ([`chat_strip`]) over `tabs`, `open`
/// picked, `width` px wide.
fn strip(
    case: &Case,
    tabs: Vec<ChatTab>,
    open: StripPick,
    width: f32,
    prefix: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let strip = chat_strip(strip_spec(case, tabs, open, prefix), window, cx);
    div().w(px(width)).max_w_full().child(strip).into_any_element()
}

/// The header's width (web `48rem`).
const HEADER_W: f32 = 768.;

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

/// #EXP-1433: the menus drawn OPEN under the strips (the web's
/// `MenuPanel`s): the group's Menu (`g2` open) and the "+" (THE chat
/// menu).
fn open_menus(fold: &Case, tabs: &[ChatTab], window: &mut Window, cx: &mut App) -> Div {
    let muted = cx.theme().muted_foreground;
    let caption = |text: &'static str| div().text_xs().text_color(muted).child(text);
    let (issues, sessions) = strip_group_items(&strip_spec(fold, tabs.to_vec(), StripPick::Conversation, "sg-group"));
    let group_menu: Entity<OpenMenu> = window.use_keyed_state("sg-chat-parts-group-menu", cx, move |window, cx| {
        OpenMenu::new(window, cx, move |menu, _, _| {
            strip_group_menu(menu, &issues, &sessions, &StripPick::Session("g2".into()), None)
        })
    });
    let target = ChatMenuTarget {
        root_id: fold.view.root_id.clone(),
        team_id: None,
        chip: None,
        linked: Vec::new(),
    };
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
                .gap_2()
                .child(group_menu)
                .child(caption("The group's Menu: issues, a separator, the session tree (phones: the bottom sheet)")),
        )
        .child(v_flex().gap_2().child(add_menu).child(caption("The \"+\" (left of Conversation)")))
}

/// One conversation row, the way the chat screen paints it.
pub(super) fn row(
    case: &Case,
    row: &SessionChatRow,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    if let SessionChatRow::Message { id, author, body, queued, xref, .. } = row {
        let markdown = message_markdown(SharedString::from(format!("sg-chat-message-{id}")), body, &crate::session_chat::ChatText::default(), images);
        let bubble = match author.as_str() {
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
        // S-3: another session's message = its link header above the bubble.
        return crate::session_chat::with_xref(id, xref.as_ref(), author, bubble, cx);
    }
    card_row(row, &case.view.root_id, &case.view.rows, &crate::session_chat::ChatText::default(), images, window, cx).unwrap_or_else(|| div().into_any_element())
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
    let header_first = header(&first, "sg-chat", window, cx);
    let header_private = header(&second, "sg-chat-private", window, cx);
    // #EXP-1433: the strip's two modes, then the menus, open.
    let fold = case(case_named(FOLD_CASE));
    let group = group_tabs(&fold.view);
    let one = strip(&fold, one_issue_tabs(&fold.view), StripPick::Conversation, HEADER_W, "sg-chat-one", window, cx);
    let rest = strip(&fold, group.clone(), StripPick::Conversation, HEADER_W, "sg-chat-group", window, cx);
    let session_open =
        strip(&fold, group.clone(), StripPick::Session("g2".into()), HEADER_W, "sg-chat-group-session", window, cx);
    let issue_open =
        strip(&fold, group.clone(), StripPick::Issue("i-3".into()), HEADER_W, "sg-chat-group-issue", window, cx);
    let menus = open_menus(&fold, &group, window, cx);
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
                .child(caption("One session (no child yet): no Conversation tab, the session's own work is the body")),
        )
        .child(
            v_flex()
                .w(px(HEADER_W))
                .max_w_full()
                .gap_3()
                .child(caption("One issue, no session: its chip stays"))
                .child(one)
                .child(caption("More: ONE group tab, the count muted"))
                .child(rest)
                .child(caption("A session open from the group: the tab is active and names it"))
                .child(session_open)
                .child(caption("An issue open from the group"))
                .child(issue_open)
                .child(menus),
        )
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

    /// #EXP-1433: the group case's strip carries the web's two extra issues
    /// after its own and before the sessions; the statuses alternate in
    /// progress / backlog by the flat issue index; the one-issue case is
    /// `chips`, the group case `group` with every issue and session counted.
    #[test]
    fn the_strip_modes_are_the_webs() {
        use domain::session_tree::{chat_strip_mode, StripMode};
        let fold = case(case_named(FOLD_CASE));
        let tabs = group_tabs(&fold.view);
        let identifiers: Vec<&str> = tabs
            .iter()
            .filter_map(|tab| match tab {
                ChatTab::Issue { identifier, .. } => Some(identifier.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(identifiers, vec!["EXP-21", "EXP-24", "EXP-25", "EXP-26"]);
        let last_issue = tabs.iter().rposition(|tab| matches!(tab, ChatTab::Issue { .. })).unwrap();
        let first_session = tabs.iter().position(|tab| matches!(tab, ChatTab::Session { .. })).unwrap();
        assert!(last_issue < first_session);
        let issues = strip_issues(&fold, &tabs);
        assert_eq!(issues["i-5"].title, "Context ring");
        assert_eq!(issues["i-6"].title, "Model picker");
        let status = |id: &str| issues[id].status.as_ref().map(|status| status.group_key.clone());
        assert_ne!(status("i-6"), status("i-5"));
        let spec = strip_spec(&fold, tabs, StripPick::Conversation, "t");
        let (group_issues, group_sessions) = strip_group_items(&spec);
        assert!(group_sessions.iter().any(|(item, _)| matches!(item, crate::session_chat::StripGroupItem::Session { session_id, .. } if session_id == "g2")));
        assert!(group_issues.iter().any(|item| matches!(item, crate::session_chat::StripGroupItem::Issue { issue_id, .. } if issue_id == "i-3")));
        let mode = chat_strip_mode(group_issues.len(), group_sessions.len());
        assert_eq!(mode.mode, StripMode::Group);
        assert_eq!(mode.count, group_issues.len() + group_sessions.len());
        let one = strip_spec(&fold, one_issue_tabs(&fold.view), StripPick::Conversation, "t");
        let (one_issues, one_sessions) = strip_group_items(&one);
        assert_eq!(chat_strip_mode(one_issues.len(), one_sessions.len()).mode, StripMode::Chips);
    }
}
