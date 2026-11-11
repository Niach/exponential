//! EXP-1294 / #EXP-1732 — THE SESSION SCREEN ([`Screen::Chat`],
//! fixtures/session-screen.json ×4): a chat IS a root session, its model
//! the contract's [`domain::session_tree::build_session_chat`] over the
//! synced rows ([`crate::session_data::chat_view_and_messages`]). A team
//! agent's and the Assistant's conversation render THIS view too
//! ([`crate::agent_screen`] wraps it once the conversation exists); an
//! app's editor draws the same chrome ([`crate::app_screen`]).
//!
//! * THE HEADER ([`screen_header`]): the lead avatar (a chat's lead disc,
//!   the agent's disc, the Assistant's brand mark), the title (EXP-1316:
//!   derived, plain text; a lock while private) over ONE subtitle line
//!   (`session_screen_subtitle`: state · device · trigger), then the merge
//!   status pill while the chat's PR is open, an agent's telephone, THE gear
//!   (agents and the Assistant: the agent's settings page in the pane) and
//!   the `…`.
//! * ON THE TITLE ROW (#EXP-1762), THE Segmented ([`screen_tabs`], the app
//!   editor's): Chat · Issues · Sessions · Results, each with its muted count.
//! * THE BODY (the IDE is WIDE: the beside rule, [`screen_beside`]): the
//!   Chat tab alone = the chat full width; any other tab, or an opened item,
//!   = the chat pane (5fr, min 360px) beside the tab pane (7fr), a hairline
//!   between. The chat = the root's messages plus its children's question /
//!   plan cards on THE transcript (#EXP-1736's MessageBubble rows), THE Dock
//!   and THE session composer ([`crate::composer::session_composer_card`],
//!   `sessions.send({sessionId: root})`). The Chat tab ALWAYS shows the
//!   transcript, a one-session chat's too (its raw work = a Sessions row).
//! * THE TABS: Issues = THE issue rows of the tree led by the `+` (THE Menu:
//!   `Link an issue` › the search list, `New issue`; a root link's row
//!   `Unlink <IDENT>` on its context menu), a row opens that issue EMBEDDED
//!   in the pane; Sessions = THE tree rows (SessionRow small + the guides)
//!   of every session under the root, a row opens its raw work in the pane;
//!   Results = results-tab.json's body. An opened item (an issue, a session,
//!   a plan, the settings) carries a `‹` back to its tab.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, AppContext as _, ClickEvent,
    Entity, FocusHandle, Focusable, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Task,
    Window,
};
use gpui_component::{button::ButtonVariants as _, h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};
use sync::Store;

use domain::rows::{Issue, MessageRow, PullRequestRow, SessionIssueRow, SessionRow};
use domain::session_results::SessionResultEntry;
use domain::dock::DockSource;
use domain::session_tree::{SessionChatRow, SessionChatView, ChatTab, SESSION_CHAT, CHATS_LIST};
use domain::results_tab::RESULTS_TAB;
use domain::session_screen::{
    session_screen, session_screen_kind, SessionScreen, SessionScreenCounts, SessionScreenKind, SessionScreenLayout,
    SessionScreenOpen, SessionScreenTab, SessionScreenTabEntry, SESSION_SCREEN,
};

use crate::controls::PointerContextMenuExt as _;
use crate::controls::WebControl as _;
use crate::controls::WebText as _;
use crate::icons::registry;
use crate::markdown::{ImageCache, IssueChipCache, MarkdownView, RefResolver};
use crate::navigation::Screen;

/// The menu copy.
pub(crate) const LINK_ISSUE_LABEL: &str = "Link an issue";
pub(crate) const NEW_ISSUE_LABEL: &str = "New issue";

/// `Unlink EXP-1270`.
pub(crate) fn unlink_label(identifier: &str) -> String {
    format!("Unlink {identifier}")
}

/// A chip linked less than this long before it first renders is NEW to the
/// viewer: it fades in with a ring.
pub(crate) const NEW_CHILD_WINDOW: Duration = Duration::from_secs(10);

/// A root that has not synced this long after the page opened (with the
/// sessions shape settled) is missing — never a flash of "not found" for a
/// chat the composer just created.
pub(crate) const MISSING_GRACE: Duration = Duration::from_secs(5);

/// The conversation's reading column: THE chat column (#EXP-1547), the Dock's.
pub(crate) const CONVERSATION_W: f32 = crate::work_header::CHAT_COLUMN_W;
/// The header + strip's side inset.
const HEADER_INSET: f32 = 44.;
/// The issue chip's lead (a live root's mark) and its state badge.
const CHIP_MARK_PX: f32 = 12.;
const CHIP_BADGE_PX: f32 = 5.;

/// #EXP-1732: the item opened over the tab pane (a session open in place is
/// the view's `focus`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChatOpened {
    /// The issue's embedded detail.
    Issue(String),
    /// A plan card (plan-page.json): its plan page.
    Plan(String),
    /// THE gear's settings (agents, the Assistant).
    Settings,
}

impl ChatOpened {
    /// session-screen.json's `open` value.
    fn open(&self) -> &'static str {
        match self {
            Self::Issue(_) => SessionScreenOpen::Issue.as_str(),
            Self::Plan(_) => SessionScreenOpen::Plan.as_str(),
            Self::Settings => SessionScreenOpen::Settings.as_str(),
        }
    }
}

// ---------------------------------------------------------------------------
// Seeds the New issue page reads
// ---------------------------------------------------------------------------

/// "New issue" → the FRESH New issue page `draft_id` files INTO a chat
/// (the root id). Only the opener's seed: the draft's first write stores it
/// on the row (`issue_drafts.session_id`), which is what a reopen reads.
#[derive(Default)]
struct DraftChats(HashMap<String, String>);

impl gpui::Global for DraftChats {}

/// The chat root a fresh New issue page `draft_id` was opened for.
pub(crate) fn draft_chat(draft_id: &str, cx: &App) -> Option<String> {
    cx.try_global::<DraftChats>()?.0.get(draft_id).cloned()
}

/// The New issue page's `In <title>` chip (the chat it files into); a
/// click opens the chat.
pub(crate) fn draft_chat_chip(root_id: &str, cx: &App) -> AnyElement {
    use crate::surface::{glass_pill, PillMode, PillSize};
    let theme = cx.theme();
    let title = crate::session_data::session_title_of(root_id, cx)
        .unwrap_or_else(|| SESSION_CHAT.copy.untitled.clone());
    let root_id = root_id.to_string();
    glass_pill("draft-chat", PillSize::Sm, PillMode::Action, cx)
        .max_w(px(280.))
        .child(
            Icon::new(registry::NAV_CHATS)
                .with_size(px(PillSize::Sm.glyph()))
                .text_color(theme.muted_foreground),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(theme.foreground)
                .child(SharedString::from(in_chat_label(&title))),
        )
        .on_click(move |_, window, cx| {
            crate::navigation::navigate(
                window,
                cx,
                Screen::Chat {
                    session_id: root_id.clone(),
                },
            );
        })
        .into_any_element()
}

/// `In <title>`.
pub(crate) fn in_chat_label(title: &str) -> String {
    format!("In {title}")
}

/// An issue child to select when that chat's view next renders (a new
/// issue filed into it lands on its child).
#[derive(Default)]
struct PendingChatChild(HashMap<String, String>);

/// #EXP-1662: the plan a way in wants its chat to open (root id → plan card
/// id), read by that chat's view on its next frame.
#[derive(Default)]
struct PendingChatPlan(HashMap<String, String>);

impl gpui::Global for PendingChatPlan {}

/// #EXP-1662 / #EXP-1732: open the plan `message_id` beside the chat
/// `root_id` (the caller navigates there; on that chat already, this alone
/// does it).
pub(crate) fn request_chat_plan(root_id: &str, message_id: &str, cx: &mut App) {
    cx.default_global::<PendingChatPlan>().0.insert(root_id.to_string(), message_id.to_string());
    cx.refresh_windows();
}

impl gpui::Global for PendingChatChild {}

/// #FEED-191 (needs-you.json): what a Needs you row wants its chat to show (root
/// id → the opening), read by that chat's view on its next frame.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ChatOpening {
    /// The Conversation, scrolled to the card (THE waiting chip's way).
    Card(String),
    /// The Results tab (it holds the PR block).
    Results,
}

#[derive(Default)]
struct PendingChatOpening(HashMap<String, ChatOpening>);

impl gpui::Global for PendingChatOpening {}

/// Show the Conversation of `root_id` scrolled to the card `message_id` (the
/// caller navigates there; on that chat already, this alone does it).
pub(crate) fn request_chat_card(root_id: &str, message_id: &str, cx: &mut App) {
    cx.default_global::<PendingChatOpening>().0.insert(root_id.to_string(), ChatOpening::Card(message_id.to_string()));
    cx.refresh_windows();
}

/// Show the Results tab of `root_id` (the caller navigates there).
pub(crate) fn request_chat_results(root_id: &str, cx: &mut App) {
    cx.default_global::<PendingChatOpening>().0.insert(root_id.to_string(), ChatOpening::Results);
    cx.refresh_windows();
}

/// #FEED-191 (plan-page.json): decisions taken ON the plan tab: `asked` = card →
/// its chat's root while the answer travels, `landed` = the chats whose answer
/// succeeded, read by that chat's view on its next frame (a decision from the
/// Conversation never enters, it stays put).
#[derive(Default)]
struct PlanTabDecisions {
    asked: HashMap<String, String>,
    landed: HashSet<String>,
}

impl gpui::Global for PlanTabDecisions {}

/// The next answer of `message_id` (posted by `session_id`) was decided on the
/// plan tab: its chat returns to the Conversation once it succeeds.
pub(crate) fn decide_on_plan_tab(message_id: &str, session_id: &str, cx: &mut App) {
    let Some(root) = crate::session_data::session(session_id, cx).map(|session| session.root_id_or_self().to_string()) else {
        return;
    };
    cx.default_global::<PlanTabDecisions>().asked.insert(message_id.to_string(), root);
}

/// An answer's outcome: a tab decision that succeeded lands, a refusal stays on
/// the tab.
fn settle_plan_tab_decision(message_id: &str, ok: bool, cx: &mut App) {
    let decisions = cx.default_global::<PlanTabDecisions>();
    if let Some(root) = decisions.asked.remove(message_id).filter(|_| ok) {
        decisions.landed.insert(root);
    }
}

/// A New issue page filed `issue_id` into the chat `root_id`: link it
/// (`sessions.linkIssues`) and open its child the next time the chat
/// renders with it linked.
pub(crate) fn request_chat_child(root_id: &str, issue_id: &str, cx: &mut App) {
    link_issue(root_id.to_string(), issue_id.to_string(), cx);
    cx.default_global::<PendingChatChild>()
        .0
        .insert(root_id.to_string(), issue_id.to_string());
}

/// Open a New issue page that files into `root_id`, on the board of the
/// chat's first issue (else the window's active board).
pub(crate) fn open_new_issue_in_chat(root_id: &str, window: &mut Window, cx: &mut App) {
    let board_id = crate::session_data::issues_of_root(root_id, cx)
        .into_iter()
        .next()
        .map(|issue| issue.board_id)
        .unwrap_or_default();
    let draft_id = api::issue_drafts::new_draft_id();
    cx.default_global::<DraftChats>()
        .0
        .insert(draft_id.clone(), root_id.to_string());
    crate::navigation::navigate(
        window,
        cx,
        Screen::IssueDraft {
            draft_id,
            board_id,
            status_id: None,
        },
    );
}

// ---------------------------------------------------------------------------
// The writes (sessions.linkIssues / unlinkIssue / answer / unqueue)
// ---------------------------------------------------------------------------

/// Run `call` with the tRPC client in the background; a failure is the
/// `Couldn't <verb> <thing>` toast (feedback-states).
fn background_write<T: Send + 'static>(
    verb: &'static str,
    thing: &'static str,
    cx: &mut App,
    call: impl FnOnce(&api::TrpcClient) -> Result<T, api::ApiError> + Send + 'static,
) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        crate::action_error::failed_action(verb, thing, None, cx);
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx.background_executor().spawn(async move { call(&trpc) }).await;
        if let Err(err) = result {
            log::warn!("[chat] {verb} {thing} failed: {err}");
            cx.update(|cx| crate::action_error::failed_action_err(verb, thing, &err, cx));
        }
    })
    .detach();
}

/// `sessions.linkIssues({sessionId: root, issueIds: [issue]})`.
pub(crate) fn link_issue(root_id: String, issue_id: String, cx: &mut App) {
    background_write("link", "the issue", cx, move |trpc| api::sessions::link_issues(trpc, &root_id, &[issue_id]));
}

/// `sessions.unlinkIssue({sessionId: root, issueId})`.
pub(crate) fn unlink_issue(root_id: String, issue_id: String, cx: &mut App) {
    background_write("unlink", "the issue", cx, move |trpc| api::sessions::unlink_issue(trpc, &root_id, &issue_id));
}

/// Cards whose answer is on its way (or landed before `answered_at`
/// synced): a second click never answers twice, and the card goes quiet.
#[derive(Default)]
struct AnsweringCards(HashSet<String>);

impl gpui::Global for AnsweringCards {}

/// Whether `message_id`'s answer is already on its way.
pub(crate) fn answering(message_id: &str, cx: &App) -> bool {
    cx.try_global::<AnsweringCards>()
        .is_some_and(|cards| cards.0.contains(message_id))
}

/// `sessions.answer` — a question card's option, a plan card's decision.
/// Once per card: a failure re-opens it (and is the usual toast).
pub(crate) fn answer_card(input: api::sessions::AnswerInput, cx: &mut App) {
    let message_id = input.message_id.clone();
    if !cx.default_global::<AnsweringCards>().0.insert(message_id.clone()) {
        return;
    }
    cx.refresh_windows();
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        cx.default_global::<AnsweringCards>().0.remove(&message_id);
        crate::action_error::failed_action("answer", "the question", None, cx);
        return;
    };
    let plan = crate::sealing::plan_answer(&message_id, cx);
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { crate::sealing::answer_blocking(&trpc, &input, &plan) })
            .await;
        match result {
            // The draft goes only now: a refusal (409, 400, the network)
            // re-opens the card with its picks and text intact.
            Ok(_) => {
                cx.update(|cx| {
                    crate::plan_card::drop_draft(&message_id, cx);
                    // #EXP-1464: an expanded row's card answered = it collapses.
                    settle_answered_row(&message_id, cx);
                    settle_plan_tab_decision(&message_id, true, cx);
                    cx.refresh_windows();
                });
            }
            Err(err) => {
                log::warn!("[chat] answer the question failed: {err}");
                cx.update(|cx| {
                    cx.default_global::<AnsweringCards>().0.remove(&message_id);
                    settle_plan_tab_decision(&message_id, false, cx);
                    cx.refresh_windows();
                    crate::action_error::failed_action_err("answer", "the question", &err, cx);
                });
            }
        }
    })
    .detach();
}

/// #EXP-1561 (waiting-cards.json DISMISS): THE answer path with
/// `{messageId, dismissed: true}` and nothing else; once per card like an
/// answer (a refusal re-opens it, the usual toast).
pub(crate) fn dismiss_card(message_id: String, cx: &mut App) {
    answer_card(api::sessions::AnswerInput { message_id, dismissed: Some(true), ..Default::default() }, cx);
}

/// `sessions.unqueue` — take back a message the device has not consumed.
#[cfg(test)]
pub(crate) fn unqueue_message(message_id: String, cx: &mut App) {
    background_write("remove", "the message", cx, move |trpc| api::sessions::unqueue(trpc, &message_id));
}

/// A session card (or a `Report from` / event line) was clicked: #EXP-1464
/// opens it by inbox-route.json `session_target`
/// ([`crate::navigation::session_screen_for`]): the viewer's own agent
/// conversation root = the agent's page, a chat root = its chat, a
/// chat's child = in its chat (its session view on the chat's tab),
/// anything else under an agent = its own session screen. #FEED-191: what it
/// opens wears the Back here ([`crate::navigation::open_session_link`]).
pub(crate) fn open_session_card(session_id: &str, window: &mut Window, cx: &mut App) {
    crate::navigation::open_session_link(session_id, window, cx);
}

// ---------------------------------------------------------------------------
// #EXP-1464: what a conversation's session rows do (chat-row-action.json)
// ---------------------------------------------------------------------------

/// The client-held row state: the expanded session cards (keyed by the
/// conversation's root + the session), the sessions the viewer answered
/// from an expanded card, and the card each expanded row shows (its message
/// id → that row) so an answer collapses it.
#[derive(Default)]
struct ChatRowState {
    expanded: HashSet<(String, String)>,
    answered: HashSet<String>,
    showing: HashMap<String, (String, String)>,
}

impl gpui::Global for ChatRowState {}

fn row_expanded(root_id: &str, session_id: &str, cx: &App) -> bool {
    cx.try_global::<ChatRowState>()
        .is_some_and(|state| state.expanded.contains(&(root_id.to_string(), session_id.to_string())))
}

fn toggle_row(root_id: &str, session_id: &str, cx: &mut App) {
    let state = cx.default_global::<ChatRowState>();
    let key = (root_id.to_string(), session_id.to_string());
    if !state.expanded.remove(&key) {
        state.expanded.insert(key);
    }
    cx.refresh_windows();
}

/// An answer landed: the expanded row that showed `message_id` collapses
/// and its session reads `Answered` (chat-row-action.json `answered`).
fn settle_answered_row(message_id: &str, cx: &mut App) {
    let Some(state) = cx.try_global::<ChatRowState>() else {
        return;
    };
    if !state.showing.contains_key(message_id) {
        return;
    }
    let state = cx.default_global::<ChatRowState>();
    if let Some(key) = state.showing.remove(message_id) {
        state.expanded.remove(&key);
        state.answered.insert(key.1);
    }
    cx.refresh_windows();
}

/// The session's waiting `question` / `plan` card as its Conversation row
/// (`session_waiting_card` over its own messages), `None` while unsynced.
fn waiting_card_row(session_id: &str, cx: &App) -> Option<SessionChatRow> {
    let messages = crate::session_data::messages_of_session(session_id, cx);
    let waiting = domain::chat_row_action::session_waiting_card(
        session_id,
        messages.iter().map(|message| domain::chat_row_action::WaitingMessage {
            id: &message.id,
            session_id: &message.session_id,
            created_at: message.created_at.as_deref().unwrap_or_default(),
            answered_at: message.answered_at.as_deref(),
            card: message.card.as_ref(),
        }),
    )?;
    let message = messages.iter().find(|message| message.id == waiting)?;
    domain::session_tree::ask_card_row(message)
}

/// A session row's action (chat-row-action.json) in the conversation
/// rooted at `root_id` over its `rows`.
fn session_row_action(
    kind: &str,
    session_id: Option<&str>,
    root_id: &str,
    rows: &[SessionChatRow],
    cx: &App,
) -> domain::chat_row_action::ChatRowAction {
    let session = session_id.and_then(|id| crate::session_data::session(id, cx));
    let state = session.as_ref().map(domain::session_tree::session_state);
    let blocked = session.as_ref().is_some_and(|session| session.blocked.is_some());
    let inline = session_id.is_some_and(|id| domain::chat_row_action::waiting_card_inline(rows, id));
    let answered = session_id.is_some_and(|id| {
        cx.try_global::<ChatRowState>()
            .is_some_and(|state| state.answered.contains(id))
    });
    domain::chat_row_action::chat_row_action(kind, session_id, root_id, state, blocked, inline, answered)
}

/// THE session card as a conversation row: `expand` toggles it in place
/// (chevron down + THE question / plan card under it, `Open chat` in its
/// label line), `open` opens it, `none` does nothing.
#[allow(clippy::too_many_arguments)] // the row's facts + the painter's
fn session_card_row(
    card: &domain::session_tree::SessionCard,
    root_id: &str,
    rows: &[SessionChatRow],
    text: &ChatText,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    use domain::chat_row_action::RowAction;
    let action = session_row_action("session", Some(&card.session_id), root_id, rows, cx);
    let mut shown = card.clone();
    if let Some(word) = &action.kind_word {
        shown.kind_word = word.clone();
    }
    if let Some(tone) = action.tone {
        shown.tone = tone.to_string();
    }
    let session_id = card.session_id.clone();
    match action.action {
        RowAction::None => crate::work_card::session_card_row(&shown, None, None, cx),
        RowAction::Open => crate::work_card::session_card_row(
            &shown,
            Some(registry::UI_CHEVRON_RIGHT),
            Some(Box::new(move |window, cx| open_session_card(&session_id, window, cx))),
            cx,
        ),
        RowAction::Expand => {
            let expanded = row_expanded(root_id, &card.session_id, cx);
            let toggle_root = root_id.to_string();
            let header = crate::work_card::session_card_row(
                &shown,
                Some(if expanded { registry::UI_CHEVRON_DOWN } else { registry::UI_CHEVRON_RIGHT }),
                Some(Box::new(move |_, cx| toggle_row(&toggle_root, &session_id, cx))),
                cx,
            );
            if !expanded {
                return header;
            }
            let waiting = waiting_card_row(&card.session_id, cx);
            let key = (root_id.to_string(), card.session_id.clone());
            {
                let state = cx.default_global::<ChatRowState>();
                state.showing.retain(|_, shown| *shown != key);
                if let Some(row) = &waiting {
                    state.showing.insert(row.id().to_string(), key);
                }
            }
            let open_id = card.session_id.clone();
            let open_chat = gpui_component::button::Button::new(SharedString::from(format!(
                "{}-{}",
                domain::chat_row_action::test_ids().open_chat,
                card.session_id
            )))
            .link()
            .cursor_pointer()
            .xsmall()
            .label(domain::chat_row_action::copy().open_chat.clone())
            .on_click(move |_, window, cx| open_session_card(&open_id, window, cx));
            let body = match &waiting {
                Some(row) => ask_row_card_with(row, text, images, Some(open_chat.into_any_element()), window, cx),
                None => None,
            }
            .unwrap_or_else(|| {
                crate::feedback::skeleton_value(
                    SharedString::from(format!("{}-skeleton-{}", domain::chat_row_action::test_ids().expanded, card.session_id)),
                    160.,
                    cx,
                )
            });
            v_flex()
                .id(SharedString::from(format!(
                    "{}-{}",
                    domain::chat_row_action::test_ids().expanded,
                    card.session_id
                )))
                .w_full()
                .min_w_0()
                .gap_2()
                .child(header)
                .child(body)
                .into_any_element()
        }
    }
}

/// An event line (`… was not picked up`): a button opening the session it
/// names when its action is not `none`, else the plain muted line.
fn event_row(
    id: &str,
    session_id: Option<&str>,
    line: &str,
    root_id: &str,
    rows: &[SessionChatRow],
    cx: &App,
) -> AnyElement {
    // #EXP-1772: an event line is THE inline marker; one about a session
    // keeps its press (chat-row-action.json).
    let action = session_row_action("event", session_id, root_id, rows, cx);
    let press: Option<Rc<dyn Fn(&mut Window, &mut App)>> = match action.session_id {
        Some(target) if action.chevron => Some(Rc::new(move |window, cx| open_session_card(&target, window, cx))),
        _ => None,
    };
    crate::surface_card::transcript_marker(
        SharedString::from(format!("{}-{id}", domain::chat_row_action::test_ids().event)),
        line,
        press,
        cx,
    )
}

// ---------------------------------------------------------------------------
// THE Menu: the strip's "+" and an issue chip's context menu
// ---------------------------------------------------------------------------

/// The rows of the chat menu, in order (`unlink` = the issue chip's own
/// last row; the "+" has none). Pure, so the order is a unit test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChatMenuRow {
    LinkIssue,
    NewIssue,
    Separator,
    Unlink(String),
}

pub(crate) fn chat_menu_rows(unlink: Option<&str>) -> Vec<ChatMenuRow> {
    let mut rows = vec![ChatMenuRow::LinkIssue, ChatMenuRow::NewIssue];
    if let Some(identifier) = unlink {
        rows.push(ChatMenuRow::Separator);
        rows.push(ChatMenuRow::Unlink(identifier.to_string()));
    }
    rows
}

/// What the menu acts on.
#[derive(Clone)]
pub(crate) struct ChatMenuTarget {
    pub(crate) root_id: String,
    pub(crate) team_id: Option<String>,
    /// The issue chip the menu was opened on (`(issue_id, identifier)`).
    pub(crate) chip: Option<(String, String)>,
    /// The issues already linked (the search list checks them).
    pub(crate) linked: Vec<String>,
}

/// Build THE chat menu into `menu` (the "+" dropdown or a chip's context
/// menu).
pub(crate) fn chat_menu(
    mut menu: gpui_component::menu::PopupMenu,
    target: &ChatMenuTarget,
    window: &mut Window,
    cx: &mut gpui::Context<gpui_component::menu::PopupMenu>,
) -> gpui_component::menu::PopupMenu {
    use crate::controls::{danger_menu_item, pointer_menu_item};
    let root = cx.entity().downgrade();
    menu = crate::picker::search_list::search_submenu_parent(menu.min_w(px(240.)));
    for row in chat_menu_rows(target.chip.as_ref().map(|(_, identifier)| identifier.as_str())) {
        menu = match row {
            ChatMenuRow::Separator => menu.separator(),
            ChatMenuRow::LinkIssue => {
                let target = target.clone();
                let root = root.clone();
                menu.submenu_with_icon(
                    Some(Icon::new(registry::NAV_SEARCH)),
                    LINK_ISSUE_LABEL,
                    window,
                    cx,
                    move |sub, window, cx| {
                        crate::picker::search_list::menu_search_list_submenu(
                            "chat-link-issue",
                            link_issue_source(&target),
                            sub,
                            root.clone(),
                            window,
                            cx,
                        )
                    },
                )
            }
            ChatMenuRow::NewIssue => {
                let root_id = target.root_id.clone();
                menu.item(
                    pointer_menu_item(None, NEW_ISSUE_LABEL, |_| None, move |window, cx| {
                        open_new_issue_in_chat(&root_id, window, cx);
                    })
                    .icon(Icon::new(registry::UI_ADD)),
                )
            }
            ChatMenuRow::Unlink(identifier) => {
                let Some((issue_id, _)) = target.chip.clone() else {
                    continue;
                };
                let root_id = target.root_id.clone();
                menu.item(
                    danger_menu_item(unlink_label(&identifier), Icon::new(registry::UI_CLOSE), cx)
                        .on_click(move |_, _, cx| {
                            unlink_issue(root_id.clone(), issue_id.clone(), cx);
                        }),
                )
            }
        };
    }
    menu
}

/// The search list's data for "Link an issue": the team's issues ranked by
/// THE engine, the linked ones checked; a pick links and closes the menu.
fn link_issue_source(target: &ChatMenuTarget) -> crate::picker::search_list::MenuListSource {
    use crate::picker::search_list as search;
    let rows_target = target.clone();
    let pick_target = target.clone();
    search::MenuListSource {
        placeholder: search::ISSUE_SEARCH_PLACEHOLDER,
        rows: std::rc::Rc::new(move |query: &str, _window: &mut Window, cx: &mut App| {
            let Some(team_id) = rows_target.team_id.as_deref() else {
                return Vec::new();
            };
            search::local_issue_hits(team_id, query, cx)
                .into_iter()
                .map(|hit| {
                    let linked = rows_target.linked.contains(&hit.issue_id);
                    search::MenuListRow {
                        id: hit.issue_id.clone(),
                        body: search::issue_row_body(
                            Some(
                                crate::icons::resolved_status_icon(&hit.status, cx)
                                    .xsmall()
                                    .into_any_element(),
                            ),
                            hit.identifier.clone(),
                            hit.title.clone(),
                            linked.then(|| search::row_check(cx)),
                            cx,
                        ),
                        disabled: linked,
                    }
                })
                .collect()
        }),
        pick: std::rc::Rc::new(move |issue_id: &str, _window: &mut Window, cx: &mut App| {
            if !pick_target.linked.iter().any(|id| id == issue_id) {
                link_issue(pick_target.root_id.clone(), issue_id.to_string(), cx);
            }
            true
        }),
        empty: std::rc::Rc::new(|_query: &str, _cx: &App| search::ISSUE_SEARCH_NO_RESULTS.into()),
        loading: std::rc::Rc::new(|_: &App| false),
        footer: None,
    }
}

// ---------------------------------------------------------------------------
// The painters (pure over their inputs; the agent channel and the
// styleguide draw them too)
// ---------------------------------------------------------------------------

/// THE issue chip's lead ×4: the issue's STATUS glyph. S-3: an unresolved
/// status (the issue row not synced) draws NO glyph, never a Backlog one.
pub(crate) fn issue_glyph(status: Option<&domain::statuses::ResolvedStatus>, cx: &App) -> Option<Icon> {
    status.map(|status| crate::icons::resolved_status_icon(status, cx).xsmall().flex_shrink_0())
}

/// The page title (draft 3): the title at the header rung, a lock while the
/// chat is private.
pub(crate) fn title_row(title: &str, private: bool, cx: &App) -> gpui::Div {
    let theme = cx.theme();
    h_flex()
        .min_w_0()
        .gap_2()
        .items_center()
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_2xl()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .child(SharedString::from(title.to_string())),
        )
        .when(private, |row| {
            row.child(
                Icon::new(registry::UI_PRIVATE)
                    .small()
                    .flex_shrink_0()
                    .text_color(theme.muted_foreground),
            )
        })
}

/// Web `permissions.isMember` for the chat's team (route `sessions/$sessionId`):
/// the one gate on the strip's `+` and the Guide menu's Close PR. A root
/// with no team (never synced that far) is not a member of anything.
fn chat_member(model: &ChatModel, cx: &App) -> bool {
    model
        .root
        .team_id
        .as_deref()
        .is_some_and(|team_id| crate::settings::is_member(cx, team_id))
}

/// EXP-1307 — the Guide `…` over the Guide PR (#EXP-1710: merge-train.json
/// `menu`, `merge_menu_entries`): the Checks submenu (any viewer, the PR has
/// rows), Merge now (owners: THE shared alert card `merge-now-pr`), Hold |
/// Resume auto-merge (members), Close PR (members, a live PR; EXP-1338: no
/// GitHub entry). No entries = none.
///
/// Session 58cb32df (chats.json): the same `…` leads with the chat's own
/// entries (`chatMenuEntries`: Stop, Stop and delete | Delete), the Guide
/// PR's rows after a rule.
fn guide_menu(model: &ChatModel, cx: &App) -> Option<AnyElement> {
    use domain::merge_train::{merge_menu_entries, pr_merge_state, MergeMenuEntry, MERGE_TRAIN};
    let root_id = model.root.id.clone();
    let chat = !crate::chat_actions::chat_entries(&root_id, cx).is_empty();
    let pull = model.pull();
    // #EXP-1611: the PR's synced check rows lead the menu (any viewer).
    let checks = pull.map(|pull| crate::pr_checks::checks_of(&pull.id, cx)).unwrap_or_default();
    let member = chat_member(model, cx);
    let owner = member
        && model
            .root
            .team_id
            .as_deref()
            .is_some_and(|team_id| crate::settings::is_owner(cx, team_id));
    let entries = pull
        .map(|pull| {
            merge_menu_entries(pull.state.as_deref(), member, owner, checks.len(), Some(pr_merge_state(pull)))
        })
        .unwrap_or_default();
    if entries.is_empty() && !chat {
        return None;
    }
    let pull_request_id = pull.map(|pull| pull.id.clone()).unwrap_or_default();
    // The batch line names the issues the PR closes beyond one.
    let others = Store::try_global(cx)
        .map(|store| {
            store
                .collections()
                .pull_request_issues
                .read(cx)
                .iter()
                .filter(|link| link.pull_request_id == pull_request_id)
                .count()
                .saturating_sub(1)
        })
        .unwrap_or(0);
    let trigger = crate::controls::ghost_icon_button(
        SharedString::from(SESSION_CHAT.test_ids.guide_menu.clone()),
        Icon::new(registry::UI_MORE),
        cx,
    )
    .accessibility_label("More");
    Some(
        div()
            .flex_shrink_0()
            .mr(px(-crate::work_header::GHOST_ICON_HANG))
            .child(crate::controls::PointerMenu::for_button(trigger, move |mut menu, window, cx| {
                menu = crate::chat_actions::chat_menu_items(menu, &root_id, false, cx);
                if chat && !entries.is_empty() {
                    menu = menu.separator();
                }
                let copy = &MERGE_TRAIN.copy.menu;
                for entry in &entries {
                    let id = pull_request_id.clone();
                    menu = match entry {
                        MergeMenuEntry::Checks => crate::pr_checks::checks_submenu(menu, &checks, window, cx),
                        MergeMenuEntry::MergeNow => menu.item(crate::controls::pointer_menu_item(
                            Some(registry::PR_MERGED),
                            copy.merge_now.clone(),
                            |_| None,
                            move |window, cx| crate::pr_merge::ask_merge_now(&id, window, cx),
                        )),
                        MergeMenuEntry::Hold | MergeMenuEntry::Resume => {
                            let hold = *entry == MergeMenuEntry::Hold;
                            let (icon, label) = if hold {
                                (registry::PR_HOLD, copy.hold.clone())
                            } else {
                                (registry::PR_RESUME, copy.resume.clone())
                            };
                            menu.item(crate::controls::pointer_menu_item(Some(icon), label, |_| None, move |_, cx| {
                                crate::pr_merge::run_train_op(id.clone(), crate::pr_merge::TrainOp::Hold(hold), cx)
                            }))
                        }
                        MergeMenuEntry::ClosePr => menu.item(
                            crate::controls::danger_menu_item(
                                domain::contract::DIFF_UI_CLOSE_PR,
                                Icon::new(registry::PR_CLOSED),
                                cx,
                            )
                            .on_click(move |_, window, cx| {
                                crate::issue_header::prompt_close_pr(
                                    id.clone(),
                                    crate::issue_header::close_pr_description(others),
                                    window,
                                    cx,
                                );
                            }),
                        ),
                    };
                }
                menu
            }))
            .into_any_element(),
    )
}

/// `HH:MM` (local) of a wire timestamp; empty when unparseable.
pub(crate) fn clock(at: Option<&str>) -> String {
    at.and_then(crate::comments::parse_epoch)
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
        .map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

/// [`clock`] of an epoch-ms instant (a feed row's `at`).
pub(crate) fn clock_ms(at: i64) -> String {
    chrono::DateTime::from_timestamp_millis(at)
        .map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

/// The display name of a message author: a member's label, else `Someone`.
pub(crate) fn member_name(user_id: Option<&str>, cx: &App) -> String {
    let Some(user_id) = user_id else {
        return crate::comments::author_label(None);
    };
    let user = Store::try_global(cx).and_then(|store| store.collections().users.read(cx).get(user_id).cloned());
    crate::comments::user_label(user_id, user.as_ref())
}

/// EXP-1309: the chip resolver EVERY chat text renders through (an agent's
/// reply, a person's message, a plan, a question, a Guide section): web
/// `ChatMarkdown`'s `bareIssueRefs` — BARE mode (EXP-760), because agents
/// write `FB-8` without a `#`; a token chips only when it resolves to a
/// synced issue of `team_id`. Display-only, the stored text is untouched.
pub(crate) fn chat_text_resolver(team_id: &str) -> RefResolver {
    RefResolver::from_store(team_id).bare(true)
}

/// EXP-1346: what every chat text renders through — the team its pills
/// resolve against and ONE chip resolver ([`chat_text_resolver`]), built
/// once per frame over the view's chip memo where the host keeps one (a long
/// Conversation used to build a resolver per message per frame and re-scan
/// the team's issues per token).
#[derive(Clone, Default)]
pub(crate) struct ChatText {
    team_id: Option<String>,
    resolver: Option<RefResolver>,
    /// FEED-140: the reader may answer the waiting cards, so the inline
    /// plan / question card carries its actions (besides THE Dock).
    answer: bool,
    /// #EXP-1561: the reader may dismiss the waiting cards (their ×);
    /// follows `answer` unless set on its own (the styleguide's specimen).
    dismiss: bool,
}

impl ChatText {
    /// A fresh (un-memoised) resolver over `team_id`; `None` = no pills.
    pub(crate) fn team(team_id: Option<&str>) -> Self {
        Self::memoized(team_id, None)
    }

    /// The resolver over `team_id`, memoised behind `cache` when given (the
    /// CALLER clears it when the issues or statuses notify).
    pub(crate) fn memoized(team_id: Option<&str>, cache: Option<IssueChipCache>) -> Self {
        let resolver = team_id.map(|team_id| {
            let resolver = chat_text_resolver(team_id);
            match cache {
                Some(cache) => resolver.memoized(cache),
                None => resolver,
            }
        });
        Self {
            team_id: team_id.map(str::to_string),
            resolver,
            answer: false,
            dismiss: false,
        }
    }

    pub(crate) fn team_id(&self) -> Option<&str> {
        self.team_id.as_deref()
    }

    /// FEED-140: whether the inline cards answer (the reader may message).
    pub(crate) fn answering(mut self, answer: bool) -> Self {
        self.answer = answer;
        self.dismiss = answer;
        self
    }

    /// #EXP-1561: the waiting cards carry their × (without their actions).
    pub(crate) fn dismissing(mut self, dismiss: bool) -> Self {
        self.dismiss = dismiss;
        self
    }

    pub(crate) fn dismisses(&self) -> bool {
        self.dismiss
    }

    pub(crate) fn answers(&self) -> bool {
        self.answer
    }
}

/// A message body as read-only chat markdown, `#IDENT` / bare `ABC-12` /
/// `@email` pills scoped to `text`'s team ([`ChatText`]). `images`
/// is the screen's one [`ImageCache`]: a person's `![..](/api/attachments/..)`
/// embed (the composer's pictures) fetches through it — a view without one
/// paints every picture as "Image unavailable" without a fetch.
pub(crate) fn message_markdown(
    id: SharedString,
    body: &str,
    text: &ChatText,
    images: &Entity<ImageCache>,
) -> MarkdownView {
    let mut view = MarkdownView::new(id, body.to_string())
        .chat(true)
        .selectable(true)
        .autolink(true)
        .images(images.clone())
        .on_open_link(crate::links::link_opener(text.team_id.clone()));
    if let Some(team) = text.team_id.clone() {
        view = view.chip_app_links(team);
    }
    if let (Some(team), Some(resolver)) = (text.team_id.clone(), text.resolver.clone()) {
        // EXP-1347: a pill opens chat-aware (its chat tab, an open
        // tab, else a new one).
        view = view
            .resolver(resolver)
            .on_open_issue(move |identifier, window, cx| {
                crate::description_editor::open_issue_by_identifier(&team, identifier, window, cx);
            });
    }
    view
}

fn body_text(element: gpui::Div) -> gpui::Div {
    element
        .text_size(px(theme::tokens::transcript::BODY_SIZE))
        .line_height(px(theme::tokens::transcript::BODY_LINE_HEIGHT))
}

/// The queued row's caption word (session-chat.json: a person's row
/// reads `queued`), lowercase like the web.
pub(crate) const QUEUED_WORD: &str = "queued";

/// EXP-1317: a queued row offers its `Remove from queue` X only to its
/// author, and only while it is queued.
#[cfg(test)]
pub(crate) fn unqueue_offered(queued: bool, own: bool) -> bool {
    queued && own
}

/// EXP-1317: the X's element id, keyed on (message id, queued): the
/// consumed row never inherits the queued button's state.
pub(crate) fn unqueue_button_id(prefix: &str, message_id: &str, queued: bool) -> SharedString {
    let state = if queued { "queued" } else { "consumed" };
    SharedString::from(format!("{prefix}-{message_id}-{state}"))
}

/// EXP-1317: THE `Remove from queue` X (the chat bubble and the session
/// view's queue strip). Its tooltip is gpui's NATIVE per-frame tooltip on a
/// wrapper keyed by [`unqueue_button_id`], never the button's managed
/// overlay tooltip: that overlay hides only on a hover-out or a press, so a
/// row that flipped queued → consumed under a still pointer kept it open; a
/// native tooltip is re-requested by its element every frame and dies with
/// the button.
pub(crate) fn unqueue_button(
    prefix: &str,
    message_id: &str,
    queued: bool,
    cx: &App,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let id = unqueue_button_id(prefix, message_id, queued);
    div()
        .id(id.clone())
        .flex_shrink_0()
        .tooltip(|window, cx| gpui_component::tooltip::Tooltip::new(steer::feed::QUEUE_REMOVE_LABEL).build(window, cx))
        .child(
            crate::controls::ghost_icon_button(
                SharedString::from(format!("{id}-x")),
                Icon::new(registry::UI_CLOSE).xsmall(),
                cx,
            )
            .accessibility_label(steer::feed::QUEUE_REMOVE_LABEL)
            .on_click(on_click),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// #EXP-1736 (transcript-grouping.json): ONE transcript, THE message bubble
// ---------------------------------------------------------------------------

/// The fixture's layout (avatar, gaps, the width cap, the radius).
pub(crate) fn transcript_sizes() -> &'static domain::transcript_grouping::TranscriptLayout {
    domain::transcript_grouping::layout()
}

/// THE message bubble's facts (web `MessageBubble`, iOS / Android
/// `MessageBubble`): the glass card, 16 px radius, its `HH:mm`
/// bottom-right inside; the viewer's own sits right, anyone else's left
/// (under [`sender_row`]'s avatar) with its sender's name over a group's
/// first bubble.
pub(crate) struct MessageBubble {
    /// The bubble's element id (its press, when it links).
    pub id: SharedString,
    pub body: AnyElement,
    /// `HH:MM`, empty = none.
    pub time: String,
    /// The viewer's own (right, no name).
    pub own: bool,
    /// The sender's name line (the `author` line; `name ›` when it links).
    pub name: Option<String>,
    /// #EXP-1772: a cross-session sender (a RELAY): its name line sits
    /// ABOVE the bubble, outside it, and is the only press (it opens this
    /// session); the bubble itself is never a link.
    pub link: Option<String>,
    /// A person's row the device has not consumed yet.
    pub queued: bool,
    /// The author's `Remove from queue` X while queued.
    pub unqueue: Option<AnyElement>,
    /// A definite width (the raw session view's measured prompt), else
    /// content-sized up to `layout.maxWidth.wide` of the column.
    pub width: Option<gpui::Pixels>,
}

/// The bubble's text block (tests measure from it).
pub(crate) const MESSAGE_BUBBLE_BODY: &str = "message-bubble-body";

/// THE bubble ([`MessageBubble`]).
pub(crate) fn message_bubble(bubble: MessageBubble, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let sizes = transcript_sizes();
    let MessageBubble { id, body, time, own, name, link, queued, unqueue, width } = bubble;
    let name = name.filter(|name| !name.is_empty() && !own);
    // #EXP-1772: a relay's name line goes above the bubble as its press.
    let relay = link.and_then(|session_id| name.clone().map(|name| (session_id, name)));
    let inner_name = if relay.is_some() { None } else { name };
    let relay_line = relay.map(|(session_id, name)| {
        let foreground = theme.foreground;
        let label = SharedString::from(name);
        div()
            .id(SharedString::from(format!("{id}-relay")))
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .max_w_full()
            .min_w_0()
            .truncate()
            .text_size(px(sizes.name_size))
            .font_weight(FontWeight::MEDIUM)
            .text_color(muted)
            .cursor_pointer()
            .hover(move |style| style.text_color(foreground))
            .active(|style| style.opacity(0.85))
            .child(label)
            .on_click(move |_, window, cx| crate::navigation::open_session_link(&session_id, window, cx))
    });
    let card = body_text(div())
        .id(id)
        .min_w_0()
        .rounded(px(sizes.radius))
        .border_1()
        // Today's phone bubble ×4: 14 across, 10 down.
        .px(px(14.))
        .py(px(10.))
        .map(|card| match width {
            Some(width) => card.w(width),
            None => card.max_w(gpui::relative(sizes.max_width.wide)),
        })
        .map(|card| {
            if own {
                card.border_color(theme::tokens::glass::STROKE_STRONG.to_hsla())
                    .bg(theme::tokens::glass::FILL_ACTIVE.to_hsla())
            } else {
                card.border_color(theme::tokens::glass::STROKE_CARD.to_hsla()).bg(crate::dock::card_fill())
            }
        })
        .when(queued, |card| card.opacity(0.7))
        // The sender's name: the bubble's own `author` line, at its top.
        .when_some(inner_name, |card, name| {
            card.child(
                div()
                    .max_w_full()
                    .min_w_0()
                    .truncate()
                    .pb(px(2.))
                    .text_size(px(sizes.name_size))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(muted)
                    .child(SharedString::from(name)),
            )
        })
        .child(div().min_w_0().debug_selector(|| MESSAGE_BUBBLE_BODY.to_string()).child(body))
        .when(!time.is_empty(), |card| {
            card.child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .text_2xs()
                    .text_color(muted)
                    .child(SharedString::from(time)),
            )
        });
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .map(|column| if own { column.items_end() } else { column.items_start() })
        .children(relay_line)
        .child(card)
        .when(queued || unqueue.is_some(), |column| {
            column.child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .text_2xs()
                    .text_color(muted)
                    .when(queued, |row| row.child(SharedString::from(QUEUED_WORD)))
                    .children(unqueue),
            )
        })
        .into_any_element()
}

/// The muted SESSION avatar (a sender with no agent): the session glyph on
/// the muted disc, [`crate::user_avatar::agent_avatar`]'s geometry.
pub(crate) fn session_avatar(size: f32, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .flex_shrink_0()
        .size(px(size))
        .rounded_full()
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.secondary)
        .child(
            Icon::new(registry::CODING_ASSISTANT)
                .with_size(px((size / 2. + 2.).round()))
                .text_color(theme.muted_foreground),
        )
        .into_any_element()
}

/// THE sender's avatar at `layout.avatar`: a member's (picture, else the
/// initials in the member's hue), an agent's own, else the session's.
pub(crate) fn sender_avatar(sender: &domain::transcript_grouping::TranscriptSender, cx: &mut App) -> AnyElement {
    let image = sender_image(sender, cx);
    sender_avatar_of(sender, image, cx)
}

/// #EDE-65 lane O: a member sender's picture, resolved (loading it) where
/// the app is mutable; the Chat tab's document paints the avatar later
/// ([`sender_avatar_of`]).
pub(crate) fn sender_image(sender: &domain::transcript_grouping::TranscriptSender, cx: &mut App) -> Option<std::sync::Arc<gpui::Image>> {
    if sender.kind != domain::transcript_grouping::SenderKind::Member {
        return None;
    }
    let image = Store::try_global(cx)
        .and_then(|store| store.collections().users.read(cx).get(&sender.id).cloned())
        .and_then(|user| user.image);
    crate::user_avatar::cached_avatar_image(cx, image.as_deref())
}

/// [`sender_avatar`] with the member's picture already resolved.
pub(crate) fn sender_avatar_of(
    sender: &domain::transcript_grouping::TranscriptSender,
    image: Option<std::sync::Arc<gpui::Image>>,
    cx: &App,
) -> AnyElement {
    use domain::transcript_grouping::SenderKind;
    let side = transcript_sizes().avatar;
    let size = gpui_component::Size::Size(px(side));
    match sender.kind {
        SenderKind::Member => crate::user_avatar::avatar_element(&sender.id, &sender.name, image, size),
        SenderKind::Agent => {
            let def = crate::session_data::agent_def(&sender.id, cx);
            crate::user_avatar::agent_avatar(
                def.as_ref().and_then(|def| def.icon.as_deref()),
                def.as_ref().and_then(|def| def.color.as_deref()),
                size,
                cx,
            )
        }
        SenderKind::Session => session_avatar(side, cx),
    }
}

/// One transcript row under its [`TranscriptRowLayout`]: anyone else's
/// row (a bubble, the agent's trace) sits in the bubble column right of
/// the sender's avatar (a group's first row; a spacer of its width on the
/// later ones, `layout.avatarGap` between); the viewer's own and every card
/// are the content alone.
///
/// [`TranscriptRowLayout`]: domain::transcript_grouping::TranscriptRowLayout
pub(crate) fn sender_row(
    layout: Option<&domain::transcript_grouping::TranscriptRowLayout>,
    content: AnyElement,
    cx: &mut App,
) -> AnyElement {
    let Some(layout) = layout.filter(|layout| layout.side == domain::transcript_grouping::TranscriptSide::Other) else {
        return content;
    };
    let sizes = transcript_sizes();
    let avatar = match layout.sender.as_ref() {
        Some(sender) if layout.avatar => Some(sender_avatar(sender, cx)),
        _ => None,
    };
    h_flex()
        .w_full()
        .min_w_0()
        .items_start()
        .gap(px(sizes.avatar_gap))
        .child(div().flex_shrink_0().w(px(sizes.avatar)).children(avatar))
        .child(v_flex().flex_1().min_w_0().child(content))
        .into_any_element()
}

/// #EXP-1736: a row layout's identity for the list's measure (its avatar,
/// name and gap move the row's height).
pub(crate) fn layout_fingerprint(layout: &domain::transcript_grouping::TranscriptRowLayout) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::hash::DefaultHasher::new();
    layout.side.hash(&mut hasher);
    layout.sender.hash(&mut hasher);
    layout.avatar.hash(&mut hasher);
    layout.name.hash(&mut hasher);
    layout.gap.to_bits().hash(&mut hasher);
    hasher.finish()
}

/// A [`domain::session_tree::SessionXref`] as a session the transcript knows.
pub(crate) fn xref_session(xref: &domain::session_tree::SessionXref) -> domain::transcript_grouping::TranscriptSession {
    domain::transcript_grouping::TranscriptSession {
        id: xref.session_id.clone(),
        title: xref.name.clone(),
        agent_id: xref.agent_id.clone(),
    }
}

/// A card-less message row (or a child's report) as THE bubble under its
/// layout (`key` prefixes the element ids, `at` its createdAt); a system
/// line = the muted line; `None` for every card ([`card_row`]).
#[cfg(test)]
pub(crate) fn transcript_message(
    row: &SessionChatRow,
    layout: Option<&domain::transcript_grouping::TranscriptRowLayout>,
    key: &str,
    at: Option<&str>,
    text: &ChatText,
    images: &Entity<ImageCache>,
    cx: &App,
) -> Option<AnyElement> {
    use domain::transcript_grouping::TranscriptSide;
    let (id, body, queued) = match row {
        // #EXP-1772: a system note is THE inline marker, never a bubble.
        SessionChatRow::Message { id, author, body, .. } if author == domain::contract::MESSAGE_AUTHOR_KIND_SYSTEM => {
            return Some(crate::surface_card::transcript_marker(SharedString::from(format!("{key}-marker-{id}")), body, None, cx));
        }
        // #EXP-1772: a blank agent turn draws nothing (no empty bubble).
        SessionChatRow::Message { body, .. } if body.trim().is_empty() => return Some(div().into_any_element()),
        SessionChatRow::Message { id, body, queued, .. } => (id, body.as_str(), *queued),
        // A child's end report = its session's bubble: the summary (else
        // the line itself).
        SessionChatRow::Report { id, text, summary, .. } => {
            (id, if summary.trim().is_empty() { text.as_str() } else { summary.as_str() }, false)
        }
        _ => return None,
    };
    let own = layout.map_or(false, |layout| layout.side == TranscriptSide::Own);
    let sender = layout.and_then(|layout| layout.sender.as_ref());
    let name = layout
        .filter(|layout| layout.name)
        .and_then(|layout| layout.sender.as_ref())
        .map(|sender| sender.display_name());
    let markdown = message_markdown(SharedString::from(format!("{key}-{id}")), body, text, images);
    let unqueue_id = id.clone();
    let unqueue = unqueue_offered(queued, own).then(|| {
        unqueue_button("chat-unqueue", id, queued, cx, move |_, _, cx| unqueue_message(unqueue_id.clone(), cx))
    });
    Some(message_bubble(
        MessageBubble {
            id: SharedString::from(format!("{key}-bubble-{id}")),
            body: markdown.into_any_element(),
            time: clock(at),
            own,
            name,
            link: sender.and_then(|sender| sender.link.clone()),
            queued,
            unqueue,
            width: None,
        },
        cx,
    ))
}

/// A muted one-liner (an event, a system message, a subagent).
pub(crate) fn muted_line(glyph: crate::icons::ExpIcon, text: String, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .text_xs()
        .text_color(muted)
        .child(Icon::from(glyph).xsmall().flex_shrink_0().text_color(muted.opacity(0.7)))
        .child(div().min_w_0().truncate().child(SharedString::from(text)))
        .into_any_element()
}

/// `plan-card.json` `copy.answering` while the card's answer is on its way.
pub(crate) fn card_answering(cx: &App) -> AnyElement {
    h_flex()
        .gap_1p5()
        .items_center()
        .mt_2()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(crate::feedback::Spinner::new().icon(crate::icons::registry::UI_LOADING).xsmall())
        .child(SharedString::from(domain::plan_card::copy().answering.clone()))
        .into_any_element()
}

/// A card's column in the Conversation: the markdown body and the rich
/// content (surface, questions; EXP-1280), then the closed lines.
fn card_column(body: MarkdownView, content: Vec<AnyElement>, controls: AnyElement, cx: &App) -> gpui::Div {
    v_flex()
        .w_full()
        .min_w_0()
        .child(crate::steer_viewer::ask_body_text(cx).child(body))
        .children(content)
        .child(controls)
}

/// EXP-1316 → EXP-1280 — a question card IN THE CONVERSATION = THE card ×4:
/// the question, the inline surface, its options; once answered, CLOSED
/// (the picks highlighted, the answer line). N-12: a pending card the
/// viewer may answer lives in THE Dock ([`ask_row_sheet`]); FEED-140: it
/// stays inline too, with its Answer when `answer` (the reader may reply).
pub(crate) fn question_card(
    question: MarkdownView,
    facts: crate::plan_card::QuestionFacts,
    answer: bool,
    meta: Option<AnyElement>,
    dismiss: Option<AnyElement>,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    let (content, controls) = crate::plan_card::question_parts(&facts, answer, images, window, cx);
    let column = card_column(question, content, controls, cx);
    let card = crate::steer_viewer::ask_card_chrome_with(false, None, label_meta(meta), None, dismiss, column, cx);
    crate::waiting_cards::ringed(card, facts.message_id, cx)
        .id(SharedString::from(format!("{}-{}", domain::plan_card::test_ids().card, facts.message_id)))
        .into_any_element()
}

/// #EXP-1662 (plan-page.json): a plan in the Conversation = ONE row (THE
/// session card's box, `layout.rowHeight`): the glyph (amber while waiting,
/// emerald check when approved, muted otherwise), `plan_row_label`, the status
/// word, flex, the chevron; a press opens the chat's plan tab on it
/// (`opens`: only where a team reads it, the styleguide's are inert). The ×
/// while it waits sits beside the press. A superseded plan keeps its closed
/// card ([`plan_superseded_card`]), a dismissed one its line.
#[allow(clippy::too_many_arguments)] // the row's facts + the painter's
pub(crate) fn plan_row(
    message_id: &str,
    session_id: &str,
    plan: &str,
    decision: Option<&str>,
    fresh: bool,
    opens: bool,
    dismiss: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let copy = &domain::plan_page::PLAN_PAGE.copy;
    let (foreground, muted, success) = {
        let theme = cx.theme();
        (theme.foreground, theme.muted_foreground, theme.success)
    };
    let needs_you = theme::tokens::YELLOW.to_hsla();
    let (glyph, tint) = match decision {
        None => (registry::CODING_PLAN, needs_you),
        Some("approved") => (registry::UI_CHECK, success),
        Some(_) => (registry::CODING_PLAN, muted),
    };
    let status = match decision {
        None => copy.needs_you.clone(),
        Some(decision) => domain::ask_dock::plan_decision_label(decision, fresh),
    };
    let press = h_flex()
        .id(SharedString::from(format!("plan-row-{message_id}")))
        .when(opens, |row| {
            row.role(gpui::Role::Button)
                .aria_label(SharedString::from(domain::plan_page::plan_row_label(plan)))
        })
        .flex_1()
        .min_w_0()
        .h_full()
        .px_3()
        .gap_2()
        .items_center()
        .when(opens, |row| {
            let (session_id, id) = (session_id.to_string(), message_id.to_string());
            row.cursor_pointer().on_click(move |_, window, cx| {
                cx.stop_propagation();
                crate::plan_page::open(&session_id, &id, window, cx);
            })
        })
        .child(Icon::new(glyph).small().flex_shrink_0().text_color(tint))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(foreground)
                .child(SharedString::from(domain::plan_page::plan_row_label(plan))),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(if decision.is_none() { needs_you } else { muted })
                .child(SharedString::from(status)),
        )
        .child(div().flex_1())
        .child(Icon::new(registry::UI_CHEVRON_RIGHT).small().flex_shrink_0().text_color(muted));
    h_flex()
        .id(SharedString::from(format!("{}-{message_id}", domain::plan_card::test_ids().card)))
        .w_full()
        .min_w_0()
        .h(px(domain::plan_page::PLAN_PAGE.layout.row_height))
        .items_center()
        .overflow_hidden()
        .rounded(px(theme::tokens::radius::LG))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .bg(crate::dock::card_fill())
        .hover(|row| row.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla()))
        .child(press)
        .when_some(dismiss, |row, dismiss| row.child(div().flex_shrink_0().mr_2().child(dismiss)))
        .into_any_element()
}

/// FEED-140: a plan the agent replaced paints closed, never its body (web
/// `ChatPlanRow`): the muted close glyph + the muted line, the card at 80 %.
fn plan_superseded_card(message_id: &str, cx: &App) -> AnyElement {
    let label = domain::ask_dock::plan_decision_label("superseded", false);
    let muted = cx.theme().muted_foreground;
    let line = h_flex()
        .gap_1p5()
        .items_center()
        .mt_1()
        .text_xs()
        .text_color(muted)
        .child(Icon::new(registry::UI_CLOSE).xsmall().text_color(muted))
        .child(div().min_w_0().truncate().child(SharedString::from(label)));
    crate::steer_viewer::ask_card_chrome(
        true,
        Some(SharedString::from(domain::ask_dock::ASK_DOCK.copy.plan_ready.clone())),
        None,
        None,
        v_flex().w_full().min_w_0().child(line),
        cx,
    )
    .opacity(0.8)
    .id(SharedString::from(format!("{}-{message_id}", domain::plan_card::test_ids().card)))
    .into_any_element()
}

/// #EXP-1464: a trailing element of a card's label line (`Open chat`),
/// pushed to its end.
fn label_meta(meta: Option<AnyElement>) -> Option<AnyElement> {
    meta.map(|meta| div().ml_auto().flex_shrink_0().child(meta).into_any_element())
}

pub(crate) fn plan_facts<'a>(row: &'a SessionChatRow) -> Option<(&'a str, &'a str, crate::plan_card::PlanFacts<'a>)> {
    match row {
        SessionChatRow::Plan { id, session_id, plan, questions, surface, decision, fresh, answers, feedback, surface_data } => Some((
            id,
            plan,
            crate::plan_card::PlanFacts {
                message_id: id,
                session_id,
                questions,
                surface: surface.as_ref(),
                decision: decision.as_deref(),
                fresh: *fresh,
                answers: answers.as_ref(),
                feedback: feedback.as_deref(),
                surface_data: surface_data.as_ref(),
                on_tab: false,
            },
        )),
        _ => None,
    }
}

pub(crate) fn question_facts<'a>(row: &'a SessionChatRow) -> Option<(&'a str, &'a str, crate::plan_card::QuestionFacts<'a>)> {
    match row {
        SessionChatRow::Question { id, question, options, custom, multi, surface, answer, answers, surface_data, answered, .. } => Some((
            id,
            question,
            crate::plan_card::QuestionFacts {
                message_id: id,
                options,
                multi: *multi,
                custom: *custom,
                surface: surface.as_ref(),
                answer: answer.as_deref(),
                answers: answers.as_deref(),
                surface_data: surface_data.as_ref(),
                answered: *answered,
            },
        )),
        _ => None,
    }
}

/// A `question` / `plan` row as its Conversation card (the chat's, the
/// agent channel's, the session view's tool-posted one); any other row =
/// `None`.
pub(crate) fn ask_row_card(
    row: &SessionChatRow,
    text: &ChatText,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> Option<AnyElement> {
    ask_row_card_with(row, text, images, None, window, cx)
}

/// [`ask_row_card`] with `meta` trailing the card's label line (#EXP-1464:
/// an expanded session row's `Open chat`).
pub(crate) fn ask_row_card_with(
    row: &SessionChatRow,
    text: &ChatText,
    images: &Entity<ImageCache>,
    meta: Option<AnyElement>,
    window: &Window,
    cx: &mut App,
) -> Option<AnyElement> {
    // #EXP-1561 (waiting-cards.json): a DISMISSED card = ONE quiet line.
    match row {
        SessionChatRow::Plan { id, plan, decision, .. } if decision.as_deref() == Some("dismissed") => {
            return Some(crate::waiting_cards::dismissed_card(true, plan, id, cx));
        }
        SessionChatRow::Question { id, question, dismissed: Some(true), .. } => {
            return Some(crate::waiting_cards::dismissed_card(false, question, id, cx));
        }
        _ => {}
    }
    // #EXP-1561: a WAITING card the reader may dismiss carries its × (not
    // while its answer is on the way).
    let waiting = matches!(
        row,
        SessionChatRow::Plan { decision: None, .. } | SessionChatRow::Question { answered: false, .. }
    );
    let dismiss = (waiting && text.dismisses() && !answering(row.id(), cx))
        .then(|| crate::waiting_cards::dismiss_button(row.id(), cx));
    // #EXP-1662: a plan = ONE row, also under an expanded session row.
    if let SessionChatRow::Plan { id, session_id, plan, decision, fresh, .. } = row {
        if decision.as_deref() == Some("superseded") {
            return Some(plan_superseded_card(id, cx));
        }
        return Some(plan_row(id, session_id, plan, decision.as_deref(), *fresh, text.team_id().is_some(), dismiss, cx));
    }
    let (id, question, facts) = question_facts(row)?;
    let markdown = message_markdown(SharedString::from(format!("chat-question-{id}")), question, text, images);
    Some(question_card(markdown, facts, text.answers(), meta, dismiss, images, window, cx))
}

/// N-12: a waiting `question` / `plan` row's sheet (plan-card.json
/// `stepper`): the plan's pages and the footer decision, or the question
/// and its Answer.
pub(crate) fn ask_row_sheet(
    row: &SessionChatRow,
    text: &ChatText,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> Option<crate::dock::SheetContent> {
    if let Some((id, plan, facts)) = plan_facts(row) {
        let markdown = message_markdown(
            SharedString::from(format!("chat-plan-sheet-{id}")),
            &crate::plan_card::strip_pending_uploads(plan),
            text,
            images,
        );
        let body = crate::steer_viewer::ask_body_text(cx).child(markdown).into_any_element();
        return Some(crate::plan_card::plan_sheet(&facts, body, images, window, cx));
    }
    let (id, question, facts) = question_facts(row)?;
    let markdown = message_markdown(SharedString::from(format!("chat-question-sheet-{id}")), question, text, images);
    let body = crate::steer_viewer::ask_body_text(cx).child(markdown).into_any_element();
    Some(crate::plan_card::question_sheet(&facts, body, images, window, cx))
}

/// N-12: the suggestions rows with an OPEN item as Dock sources
/// (`suggestionSources` ×4): the row leaves the Conversation and opens as
/// [`suggestions_sheet`].
pub(crate) fn suggestion_sources(rows: &[SessionChatRow]) -> Vec<DockSource> {
    use domain::session_tree::SuggestionState;
    let rows: Vec<domain::dock::DockSuggestionsRow> = rows
        .iter()
        .filter_map(|row| match row {
            SessionChatRow::Suggestions { id, header, items, .. } => Some(domain::dock::DockSuggestionsRow {
                id: id.clone(),
                header: header.clone(),
                items: items
                    .iter()
                    .map(|item| domain::dock::DockSuggestionItem {
                        state: match item.state {
                            SuggestionState::Open => "open",
                            SuggestionState::Started => "started",
                            SuggestionState::Here => "here",
                            SuggestionState::Filed => "filed",
                        }
                        .into(),
                        title: item.title.clone(),
                    })
                    .collect(),
            }),
            _ => None,
        })
        .collect();
    domain::dock::suggestion_sources(&rows)
}

/// N-12: the Conversation rows THE Dock holds (web `chatDockedIds`): the
/// suggestions rows with an open item (S-3: a waiting plan / question is
/// never docked; it stays inline).
pub(crate) fn docked_ids(rows: &[SessionChatRow]) -> HashSet<String> {
    suggestion_sources(rows)
        .into_iter()
        .filter_map(|source| match source {
            DockSource::Suggestions { id, .. } => Some(id),
            _ => None,
        })
        .collect()
}

/// A picture card as the Guide's tile (a click opens the lightbox).
pub(crate) fn picture_row(
    id: &str,
    attachment_id: &str,
    width: Option<f64>,
    height: Option<f64>,
    caption: Option<&str>,
    images: &Entity<ImageCache>,
    cx: &mut App,
) -> AnyElement {
    let dimension = |value: Option<f64>| value.filter(|value| *value > 0.).map(|value| value.round() as u32);
    let entry = SessionResultEntry {
        topic: String::new(),
        label: caption.unwrap_or_default().to_string(),
        attachment_id: attachment_id.to_string(),
        width: dimension(width),
        height: dimension(height),
        inline: true,
        caption: caption.map(str::to_string),
    };
    crate::session_results::tile(
        &entry,
        domain::session_results::SESSION_RESULT_TILE_HEIGHT,
        SharedString::from(format!("chat-picture-{id}")),
        entry.label.clone(),
        images,
        None,
        cx,
    )
}

/// One conversation row of a root (a chat, an agent channel): every card
/// kind but the plain messages and the reports, which are THE bubble
/// ([`transcript_message`]).
pub(crate) fn card_row(
    row: &SessionChatRow,
    root_id: &str,
    rows: &[SessionChatRow],
    text: &ChatText,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> Option<AnyElement> {
    let team_id = text.team_id();
    Some(match row {
        SessionChatRow::Message { .. } => return None,
        // #EXP-1464 (chat-row-action.json): expand / open / nothing.
        SessionChatRow::Session { card, .. } => session_card_row(card, root_id, rows, text, images, window, cx),
        SessionChatRow::Question { .. } | SessionChatRow::Plan { .. } => {
            return ask_row_card(row, text, images, window, cx);
        }
        SessionChatRow::Subagent { title, status, .. } => muted_line(
            registry::CODING_SUBAGENT,
            [title.as_str(), status.as_str()]
                .into_iter()
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
            cx,
        ),
        SessionChatRow::Picture { id, attachment_id, width, height, caption } => {
            picture_row(id, attachment_id, *width, *height, caption.as_deref(), images, cx)
        }
        SessionChatRow::Event { id, text, session_id, .. } => {
            event_row(id, session_id.as_deref(), text, root_id, rows, cx)
        }
        // #EXP-1736: a child's end report = its session's bubble
        // ([`transcript_message`]).
        SessionChatRow::Report { .. } => return None,
        SessionChatRow::Suggestions { id, header, start_all, items, .. } => {
            suggestions_card(id, header, *start_all, items, team_id, cx)
        }
        // Session 58cb32df (chats.json): `/new` / `/compact` = the divider
        // pill; an agent's preview = THE surface card.
        SessionChatRow::Divider { id, kind, at } => crate::surface_card::divider_row(id, *kind, at, cx),
        // EXP-1459: sealed content this install cannot open (yet).
        SessionChatRow::Locked { id, reason, .. } => crate::surface_card::locked_pill(id, *reason, cx),
        SessionChatRow::Surface { id, session_id, title, surface } => {
            crate::surface_card::surface_row(id, session_id, title.as_deref(), surface, window, cx)
        }
    })
}

// ---------------------------------------------------------------------------
// EXP-1323: THE suggested tasks card (a report = `session_xref`)
// ---------------------------------------------------------------------------

/// The suggestion writes in flight: `<message>:<item>` (Start all =
/// `<message>:*`) — the item's buttons lock until the write lands.
#[derive(Default)]
struct PendingSuggestions(HashSet<String>);

impl gpui::Global for PendingSuggestions {}

/// The pending key of one item (or, without one, the card's Start all) —
/// web `suggestionKey`.
pub(crate) fn suggestion_key(message_id: &str, item_id: Option<&str>) -> String {
    format!("{message_id}:{}", item_id.unwrap_or("*"))
}

fn suggestion_pending(key: &str, cx: &App) -> bool {
    cx.try_global::<PendingSuggestions>().is_some_and(|pending| pending.0.contains(key))
}

/// One suggestion write (`sessions.startSuggestion` / `startAllSuggestions`
/// / `dismissSuggestion` / `fileSuggestion`): once per key while in flight;
/// a failure is the `Couldn't <verb> <thing>` toast.
fn suggestion_write(
    key: String,
    verb: &'static str,
    thing: &'static str,
    cx: &mut App,
    call: impl FnOnce(&api::TrpcClient) -> Result<serde::de::IgnoredAny, api::ApiError> + Send + 'static,
) {
    if !cx.default_global::<PendingSuggestions>().0.insert(key.clone()) {
        return;
    }
    cx.refresh_windows();
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        cx.default_global::<PendingSuggestions>().0.remove(&key);
        crate::action_error::failed_action(verb, thing, None, cx);
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx.background_executor().spawn(async move { call(&trpc) }).await;
        cx.update(|cx| {
            cx.default_global::<PendingSuggestions>().0.remove(&key);
            cx.refresh_windows();
            if let Err(err) = result {
                log::warn!("[chat] {verb} {thing} failed: {err}");
                crate::action_error::failed_action_err(verb, thing, &err, cx);
            }
        });
    })
    .detach();
}

/// Start one item: a child (`device_id` / `model` from the Options menu)
/// or, with `here`, a message to the posting session (Do it here).
pub(crate) fn start_suggestion(
    message_id: &str,
    item_id: &str,
    here: bool,
    device_id: Option<String>,
    model: Option<String>,
    cx: &mut App,
) {
    let where_ = if here {
        api::sessions::SuggestionWhere::Here
    } else {
        api::sessions::SuggestionWhere::Child
    };
    let input = api::sessions::StartSuggestionInput {
        device_id,
        model,
        ..api::sessions::StartSuggestionInput::new(message_id, item_id, where_)
    };
    suggestion_write(suggestion_key(message_id, Some(item_id)), "start", "the task", cx, move |trpc| {
        api::sessions::start_suggestion(trpc, &input)
    });
}

fn start_all_suggestions(message_id: String, cx: &mut App) {
    suggestion_write(suggestion_key(&message_id, None), "start", "the tasks", cx, move |trpc| {
        api::sessions::start_all_suggestions(trpc, &message_id)
    });
}

fn dismiss_suggestion(message_id: String, item_id: String, cx: &mut App) {
    suggestion_write(suggestion_key(&message_id, Some(&item_id)), "dismiss", "the task", cx, move |trpc| {
        api::sessions::dismiss_suggestion(trpc, &message_id, &item_id)
    });
}

fn file_suggestion(message_id: String, item_id: String, cx: &mut App) {
    suggestion_write(suggestion_key(&message_id, Some(&item_id)), "file", "the issue", cx, move |trpc| {
        api::sessions::file_suggestion(trpc, &message_id, &item_id)
    });
}

/// THE suggested tasks card ×4 = the Work card's shell: a header
/// (`3 suggested tasks`, `· 2 started`, Start all while more than one is
/// open), then hairline-divided items. N-12: while any item is open the
/// row is a DockCard and this list is its sheet ([`suggestions_sheet`]);
/// the card stays in the Conversation for the rows with none. Open =
/// title, text, the ⋯ menu ([`suggestion_menu`]) and Start; started = the
/// child's session card (its caption under the name) opening it; here =
/// `Sent here`; filed = `Filed EXP-9`.
pub(crate) fn suggestions_card(
    message_id: &str,
    header: &str,
    start_all: bool,
    items: &[domain::session_tree::SuggestionRow],
    team_id: Option<&str>,
    cx: &App,
) -> AnyElement {
    use gpui_component::button::Button;
    let copy = &SESSION_CHAT.copy.suggestions;
    let muted = cx.theme().muted_foreground;
    let all_key = suggestion_key(message_id, None);
    let head = h_flex()
        .w_full()
        .min_w_0()
        .min_h(px(36.))
        .px_3()
        .py_1p5()
        .gap_2()
        .items_center()
        .child(Icon::new(registry::UI_CHECKLIST).xsmall().flex_shrink_0().text_color(muted))
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().foreground)
                .child(SharedString::from(header.to_string())),
        )
        .child(div().flex_1())
        .when(start_all, |this| {
            let message_id = message_id.to_string();
            this.child(
                Button::new(SharedString::from(format!("{}-start-all-{message_id}", SESSION_CHAT.test_ids.suggestions)))
                    .primary()
                    .web_xs()
                    .label(copy.start_all.clone())
                    .loading(suggestion_pending(&all_key, cx))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        start_all_suggestions(message_id.clone(), cx);
                    }),
            )
        });
    let rows = items
        .iter()
        .map(|item| suggestion_item_row(message_id, item, team_id, cx));
    v_flex()
        .id(SharedString::from(format!("{}-{message_id}", SESSION_CHAT.test_ids.suggestions)))
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(theme::tokens::radius::LG))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .bg(crate::dock::card_fill())
        .text_xs()
        .child(head)
        .children(rows.map(|row| {
            div()
                .w_full()
                .min_w_0()
                .border_t_1()
                .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
                .child(row)
        }))
        .into_any_element()
}

fn suggestion_item_row(
    message_id: &str,
    item: &domain::session_tree::SuggestionRow,
    team_id: Option<&str>,
    cx: &App,
) -> AnyElement {
    use domain::session_tree::SuggestionState;
    use gpui_component::{button::Button, Disableable as _};
    let copy = &SESSION_CHAT.copy.suggestions;
    let muted = cx.theme().muted_foreground;
    let line = || h_flex().w_full().min_w_0().min_h(px(36.)).px_3().py_1p5().gap_2().items_center();
    match item.state {
        SuggestionState::Started => match &item.card {
            Some(card) => {
                let session_id = card.session_id.clone();
                crate::work_card::session_card_bare(
                    card,
                    move |window, cx| open_session_card(&session_id, window, cx),
                    cx,
                )
            }
            // Started, the child's row not synced yet.
            None => line()
                .text_color(muted)
                .child(crate::feedback::Spinner::new().icon(crate::icons::registry::UI_LOADING).xsmall())
                .child(div().min_w_0().truncate().child(SharedString::from(item.title.clone())))
                .into_any_element(),
        },
        SuggestionState::Here | SuggestionState::Filed => {
            let state_line: AnyElement = if item.state == SuggestionState::Here {
                div().child(SharedString::from(copy.sent_here.clone())).into_any_element()
            } else {
                let (before, after) = copy.filed.split_once("{issue}").unwrap_or((copy.filed.as_str(), ""));
                let identifier = item.identifier.clone().unwrap_or_default();
                let issue_id = item.issue_id.clone();
                let hover = cx.theme().foreground;
                h_flex()
                    .items_center()
                    .child(SharedString::from(before.to_string()))
                    .child(
                        div()
                            .id(SharedString::from(format!("chat-suggestion-filed-{message_id}-{}", item.id)))
                            .role(gpui::Role::Button)
                            .aria_label(SharedString::from(identifier.clone()))
                            .font_family(theme::terminal::FONT_FAMILY)
                            .cursor_pointer()
                            .hover(move |style| style.text_color(hover))
                            .child(SharedString::from(identifier))
                            .on_click(move |_, window, cx| {
                                if let Some(issue_id) = issue_id.clone() {
                                    crate::navigation::navigate(window, cx, Screen::IssueDetail { issue_id });
                                }
                            }),
                    )
                    .child(SharedString::from(after.to_string()))
                    .into_any_element()
            };
            line()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(muted)
                        .child(SharedString::from(item.title.clone())),
                )
                .child(div().flex_shrink_0().text_color(muted).child(state_line))
                .into_any_element()
        }
        SuggestionState::Open => {
            let key = suggestion_key(message_id, Some(&item.id));
            let busy = suggestion_pending(&key, cx);
            let id = |part: &str| SharedString::from(format!("chat-suggestion-{part}-{message_id}-{}", item.id));
            let (start_message, start_item) = (message_id.to_string(), item.id.clone());
            let options = Button::new(id("options"))
                .ghost()
                .web_icon_xs()
                .icon(Icon::new(registry::UI_MORE))
                .tooltip(copy.options.clone())
                .disabled(busy);
            let menu_message = message_id.to_string();
            let menu_item = item.id.clone();
            let menu_team = team_id.map(str::to_string);
            let text = item.text.trim();
            h_flex()
                .w_full()
                .min_w_0()
                .px_3()
                .py(px(10.))
                .gap_3()
                .items_start()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(cx.theme().foreground)
                                .child(SharedString::from(item.title.clone())),
                        )
                        .when(!text.is_empty(), |this| {
                            this.child(div().mt_0p5().text_color(muted).child(SharedString::from(text.to_string())))
                        }),
                )
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .gap_1p5()
                        .items_center()
                        // THE IDE menu, in-window (inside the Dock sheet
                        // too: a layer of the window), hung off its right edge.
                        .child(
                            crate::controls::PointerMenu::for_button(options, move |menu, window, cx| {
                                suggestion_menu(menu, &menu_message, &menu_item, menu_team.as_deref(), window, cx)
                            })
                            .anchor(gpui::Anchor::TopRight),
                        )
                        .child(
                            Button::new(id("start"))
                                .primary()
                                .web_xs()
                                .label(copy.start.clone())
                                .loading(busy)
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    start_suggestion(&start_message, &start_item, false, None, None, cx);
                                }),
                        ),
                )
                .into_any_element()
        }
    }
}

/// N-12: THE suggestions sheet (the DockCard opened, web
/// `ChatSuggestionsSheet`): the items exactly as [`suggestions_card`]
/// lists them, hairline-divided; the footer = Start all while more than
/// one item is open.
pub(crate) fn suggestions_sheet(
    message_id: &str,
    start_all: bool,
    items: &[domain::session_tree::SuggestionRow],
    team_id: Option<&str>,
    cx: &App,
) -> crate::dock::SheetContent {
    use gpui_component::button::Button;
    let rows = items.iter().enumerate().map(|(index, item)| {
        div()
            .w_full()
            .min_w_0()
            .when(index > 0, |row| row.border_t_1().border_color(theme::tokens::glass::STROKE_ROW.to_hsla()))
            .child(suggestion_item_row(message_id, item, team_id, cx))
    });
    let body = div()
        .id(SharedString::from(format!("{}-{message_id}", domain::dock::DOCK.test_ids.sheet_body)))
        .flex_shrink(1.)
        .min_h_0()
        .overflow_y_scroll()
        .px_1()
        .pb_3()
        .child(
            v_flex()
                .id("chat-suggestions-sheet")
                .w_full()
                .min_w_0()
                .text_xs()
                .children(rows),
        )
        .into_any_element();
    let footer = start_all.then(|| {
        let message_id = message_id.to_string();
        let all_key = suggestion_key(&message_id, None);
        crate::dock::sheet_footer(
            h_flex().w_full().justify_end().child(
                Button::new("chat-suggestions-start-all")
                    .primary()
                    .web_sm()
                    .label(SESSION_CHAT.copy.suggestions.start_all.clone())
                    .loading(suggestion_pending(&all_key, cx))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        start_all_suggestions(message_id.clone(), cx);
                    }),
            ),
        )
    });
    crate::dock::SheetContent { meta: None, action: None, body, footer }
}

/// An open item's ⋯ = THE Menu (web `session-chat-view`): Do it here ·
/// Start on… (the team's devices that can start NOW — online with a
/// runnable agent; disabled when there is none) · Start with… (the
/// contract models, never a default row) · File as issue · Dismiss (plain
/// rows, no glyphs).
fn suggestion_menu(
    menu: gpui_component::menu::PopupMenu,
    message_id: &str,
    item_id: &str,
    team_id: Option<&str>,
    window: &mut Window,
    cx: &mut gpui::Context<gpui_component::menu::PopupMenu>,
) -> gpui_component::menu::PopupMenu {
    use crate::controls::{pointer_label_item, pointer_menu_item};
    let copy = &SESSION_CHAT.copy.suggestions;
    let devices: Vec<_> = team_id
        .map(|team_id| crate::settings::agent_detail::agent_devices(team_id, cx))
        .unwrap_or_default()
        .into_iter()
        .filter(|device| device.can_start_now())
        .collect();
    let (here_message, here_item) = (message_id.to_string(), item_id.to_string());
    let (device_message, device_item) = (message_id.to_string(), item_id.to_string());
    let (model_message, model_item) = (message_id.to_string(), item_id.to_string());
    let (file_message, file_item) = (message_id.to_string(), item_id.to_string());
    let (dismiss_message, dismiss_item) = (message_id.to_string(), item_id.to_string());
    let menu = menu.min_w(px(224.)).item(pointer_menu_item(
        None,
        copy.here.clone(),
        |_| None,
        move |_, cx| start_suggestion(&here_message, &here_item, true, None, None, cx),
    ));
    let menu = if devices.is_empty() {
        // No target: the entry stays, disabled (web's disabled submenu).
        menu.item(pointer_label_item(copy.start_on.clone(), true))
    } else {
        menu.submenu(copy.start_on.clone(), window, cx, move |mut sub, _, _| {
            for device in &devices {
                let (message_id, item_id) = (device_message.clone(), device_item.clone());
                let device_id = device.device_id.clone();
                sub = sub.item(pointer_menu_item(
                    Some(crate::icons::device_icon(device.icon.as_deref(), device.server)),
                    device.label.clone(),
                    |_| None,
                    move |_, cx| start_suggestion(&message_id, &item_id, false, Some(device_id.clone()), None, cx),
                ));
            }
            sub
        })
    };
    menu.submenu(copy.start_with.clone(), window, cx, move |mut sub, _, _| {
        for model in domain::contract::CODING_MODEL_VALUES {
            let (message_id, item_id) = (model_message.clone(), model_item.clone());
            let label = crate::steer_viewer::model_label(model);
            let model = model.to_string();
            sub = sub.item(pointer_menu_item(
                None,
                label,
                |_| None,
                move |_, cx| start_suggestion(&message_id, &item_id, false, None, Some(model.clone()), cx),
            ));
        }
        sub
    })
    .separator()
    .item(pointer_menu_item(
        None,
        copy.file.clone(),
        |_| None,
        move |_, cx| file_suggestion(file_message.clone(), file_item.clone(), cx),
    ))
    .item(pointer_menu_item(
        None,
        copy.dismiss.clone(),
        |_| None,
        move |_, cx| dismiss_suggestion(dismiss_message.clone(), dismiss_item.clone(), cx),
    ))
}

// ---------------------------------------------------------------------------
// The issue page's `Chats` row
// ---------------------------------------------------------------------------

/// A root's mark (its CLI's), `chatRowMarkState` ×4 (EXP-1344): the brand
/// mark with an amber corner dot while it needs you (beats busy), else the
/// working mark while busy, else the bare brand mark.
pub(crate) fn root_mark(root: &SessionRow, busy: bool, needs_you: bool, size: f32, badge: f32) -> AnyElement {
    let agent = root.cli.as_deref().and_then(coding::CodingAgent::parse);
    if domain::session_tree::chat_row_mark_state(needs_you, busy) == domain::session_tree::SessionState::Working {
        return crate::coding_selects::agent_working_mark(agent, size).into_any_element();
    }
    let mark = match agent {
        Some(agent) => crate::coding_selects::agent_mark(agent),
        None => Icon::new(registry::SETTINGS_AGENTS),
    };
    with_needs_you_dot(mark.with_size(px(size)), needs_you, badge)
}

/// `mark` with the amber needs-you dot on its top-right corner while
/// `needs_you` ([`root_mark`]'s badge; the Chats rows put it on a PR glyph).
pub(crate) fn with_needs_you_dot(mark: impl IntoElement, needs_you: bool, badge: f32) -> AnyElement {
    div()
        .relative()
        .flex_shrink_0()
        .child(mark)
        .when(needs_you, |this| {
            this.child(
                div()
                    .absolute()
                    .top(px(-2.))
                    .right(px(-2.))
                    .size(px(badge))
                    .rounded_full()
                    .bg(theme::tokens::YELLOW.to_hsla())
                    .border_1()
                    .border_color(theme::tokens::BACKGROUND.to_hsla()),
            )
        })
        .into_any_element()
}

/// The issue page's `Chats` chip: a small glass pill — the root's mark
/// and the chat title; a click opens the chat.
fn issue_chat_chip(index: usize, root: &SessionRow, cx: &App) -> AnyElement {
    use crate::surface::{glass_pill, PillMode, PillSize};
    let tree = crate::session_data::tree_of_root(&root.id, cx);
    let live = tree.iter().filter(|session| !session.is_ended());
    let (busy, needs_you) =
        live.fold((false, false), |(busy, needs), session| (busy || session.busy, needs || session.needs_input));
    let title = crate::session_data::session_title_of(&root.id, cx).unwrap_or_else(|| SESSION_CHAT.copy.untitled.clone());
    let root_id = root.id.clone();
    glass_pill(("issue-chat-chip", index), PillSize::Sm, PillMode::Action, cx)
        .max_w(px(260.))
        .child(root_mark(root, busy, needs_you, CHIP_MARK_PX, CHIP_BADGE_PX))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(cx.theme().foreground)
                .child(SharedString::from(title)),
        )
        .on_click(move |_, window, cx| {
            crate::navigation::navigate(
                window,
                cx,
                Screen::Chat {
                    session_id: root_id.clone(),
                },
            );
        })
        .into_any_element()
}

/// The issue page's `Chats` row: the muted label (the list's own name)
/// and one chip per chat root linking the issue. `None` while none does.
pub(crate) fn issue_chats_row(issue_id: &str, cx: &App) -> Option<AnyElement> {
    let roots = crate::session_data::roots_of_issue(issue_id, cx);
    if roots.is_empty() {
        return None;
    }
    Some(
        h_flex()
            .id("issue-chats-row")
            .flex_shrink_0()
            .gap_1p5()
            .items_center()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(CHATS_LIST.copy.title.clone())),
            )
            .children(
                roots
                    .iter()
                    .enumerate()
                    .map(|(index, root)| issue_chat_chip(index, root, cx)),
            )
            .into_any_element(),
    )
}

// ---------------------------------------------------------------------------
// #EXP-1732 THE session screen's chrome (session-screen.json ×4): a chat, an
// agent's or the Assistant's conversation and an app's editor draw the SAME
// header, THE Segmented under it, the beside split and the tree rows.
// ---------------------------------------------------------------------------

/// A press with no payload (the gear, an opened item's back).
pub(crate) type OnPress = Rc<dyn Fn(&mut Window, &mut App)>;
/// A header tab's pick.
pub(crate) type OnTabPick = Rc<dyn Fn(SessionScreenTab, &mut Window, &mut App)>;
/// A tree row's pick (an issue or a session of the Issues / Sessions tab).
pub(crate) type OnTreePick = Rc<dyn Fn(&TreeItem, &mut Window, &mut App)>;

/// #EXP-1785: how long a chip's pick waits for the opened session's feed
/// to hold its card before it rings alone.
const WAITING_REVEAL_WAIT: Duration = Duration::from_secs(5);

/// The Segmented's height (the app editor's).
const SCREEN_TABS_H: f32 = 32.;

/// THE header's inputs.
pub(crate) struct ScreenHeader {
    /// The lead avatar: a chat's lead disc, the agent's disc, the
    /// Assistant's brand mark, an app's glyph + version pill.
    pub(crate) avatar: AnyElement,
    /// The title (one truncated line).
    pub(crate) title: AnyElement,
    /// ONE line (`session_screen_subtitle`); empty = none.
    pub(crate) subtitle: String,
    /// #EXP-1729: the subtitle's tone (the app editor's `editorHeader`:
    /// amber / foreground); `None` = muted.
    pub(crate) subtitle_color: Option<gpui::Hsla>,
    /// Before the gear: the merge status pill, an app's Publish, an agent's
    /// telephone.
    pub(crate) trailing: Vec<AnyElement>,
    /// THE gear (agents, the Assistant, apps): its press and whether the
    /// settings are open.
    pub(crate) gear: Option<(OnPress, bool)>,
    /// The `…`.
    pub(crate) menu: Option<AnyElement>,
    /// A session link's Back over the row (chats.json THE WAY BACK).
    pub(crate) back: Option<AnyElement>,
    /// THE Segmented ([`screen_tabs`]).
    pub(crate) tabs: AnyElement,
}

/// THE session screen header: the Back (a link opened it), the avatar, the
/// title over its subtitle, THE Segmented (#EXP-1762: ON the title row,
/// web `session-screen-frame.tsx`), the trailing slot, the gear, the `…`;
/// the hairline spans the content card.
pub(crate) fn screen_header(header: ScreenHeader, cx: &App) -> AnyElement {
    let gear = header.gear.map(|(on_press, open)| gear_button(on_press, open, cx));
    let theme = cx.theme();
    let subtitle_color = header.subtitle_color.unwrap_or(theme.muted_foreground);
    let subtitle = (!header.subtitle.is_empty()).then(|| {
        div()
            .min_w_0()
            .truncate()
            .text_sm()
            .text_color(subtitle_color)
            .child(SharedString::from(header.subtitle))
    });
    v_flex()
        .w_full()
        .flex_shrink_0()
        .child(
            v_flex()
                .w_full()
                .px(px(HEADER_INSET))
                // #EXP-1762: ONE header line (web `py-3`).
                .py_3()
                .gap_3()
                .children(header.back)
                .child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .gap_3()
                        .items_center()
                        .child(div().flex_shrink_0().child(header.avatar))
                        // Web `min-w-40 flex-1`: the title keeps 160 px; the
                        // waiting chips (#EXP-1785) give way before it.
                        .child(v_flex().flex_1().min_w(px(160.)).child(header.title).children(subtitle))
                        .child(div().flex_shrink_0().child(header.tabs))
                        .children(header.trailing)
                        .children(gear)
                        .children(header.menu),
                ),
        )
        .child(div().w_full().h(px(1.)).bg(theme.border))
        .into_any_element()
}

/// THE gear: the existing ghost icon button (concept `nav-settings`, label
/// `copy.settings`), toggled while the settings are open.
pub(crate) fn gear_button(on_press: OnPress, open: bool, cx: &App) -> AnyElement {
    let button = crate::controls::ghost_icon_button(
        SharedString::from(SESSION_SCREEN.test_ids.gear.clone()),
        Icon::new(registry::NAV_SETTINGS),
        cx,
    )
    .tooltip(SESSION_SCREEN.copy.settings.clone())
    .toggled(open)
    .on_click(move |_, window, cx| on_press(window, cx));
    div().flex_shrink_0().child(button).into_any_element()
}

/// THE Segmented under the header (the app editor's, every kind's): the
/// kind's tabs in order, each its label then its count as a muted number;
/// it scrolls sideways when it overflows, never wraps.
pub(crate) fn screen_tabs(
    tabs: &[SessionScreenTabEntry],
    active: SessionScreenTab,
    on_pick: OnTabPick,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let segments: Vec<AnyElement> = tabs
        .iter()
        .map(|tab| {
            let id = tab.id;
            let count = tab.count.map(|count| count.to_string());
            let name = eui_core::segmented::item_accessibility_label(&tab.label, count.as_deref());
            let on_pick = on_pick.clone();
            // TAB SIZING ×4: sized to its label (+ count), never stretched
            // to equal widths, never squeezed.
            crate::controls::segmented_item(active == id, cx)
                .flex_none()
                .px_3()
                .id(SharedString::from(format!("{}-{}", SESSION_SCREEN.test_ids.tabs, id.as_str())))
                .role(gpui::Role::Tab)
                .aria_label(SharedString::from(name))
                .aria_selected(active == id)
                .child(SharedString::from(tab.label.clone()))
                .children(count.map(|count| div().text_color(muted).child(SharedString::from(count))))
                .on_click(move |_, window, cx| on_pick(id, window, cx))
                .into_any_element()
        })
        .collect();
    // The strip scrolls sideways when it overflows (the capsule keeps its
    // content width inside the scrolling row).
    h_flex()
        .id(SharedString::from(format!("{}-scroll", SESSION_SCREEN.test_ids.tabs)))
        .w_full()
        .min_w_0()
        .overflow_x_scroll()
        .child(
            crate::controls::segmented(cx)
                .id(SharedString::from(SESSION_SCREEN.test_ids.tabs.clone()))
                .debug_selector(|| SESSION_SCREEN.test_ids.tabs.clone())
                .flex_shrink_0()
                .w_auto()
                .justify_start()
                .h(px(SCREEN_TABS_H))
                .children(segments),
        )
        .into_any_element()
}

/// The beside rule (wide = the IDE, always): the chat pane (`beside.chat`
/// fr, min `beside.chatMinWidth` px, THE chat body with its composer) : the
/// tab pane (`beside.pane` fr), a hairline between.
pub(crate) fn screen_beside(chat: AnyElement, pane: AnyElement, cx: &App) -> AnyElement {
    let beside = &SESSION_SCREEN.beside;
    let share = beside.chat / (beside.chat + beside.pane);
    h_flex()
        .id(SharedString::from(SESSION_SCREEN.test_ids.beside.clone()))
        .debug_selector(|| SESSION_SCREEN.test_ids.beside.clone())
        .flex_1()
        .min_h_0()
        .w_full()
        .items_stretch()
        .child(
            v_flex()
                .h_full()
                .min_h_0()
                .w(gpui::relative(share))
                .min_w(px(beside.chat_min_width))
                .flex_shrink_0()
                .border_r_1()
                .border_color(cx.theme().border)
                .child(chat),
        )
        .child(
            v_flex()
                .id(SharedString::from(SESSION_SCREEN.test_ids.pane.clone()))
                .debug_selector(|| SESSION_SCREEN.test_ids.pane.clone())
                .h_full()
                .min_h_0()
                .min_w_0()
                .flex_1()
                .child(pane),
        )
        .into_any_element()
}

/// An opened item in the pane (an issue, a session, a plan, settings): the
/// `‹` back to where it came from over its body.
pub(crate) fn pane_item(label: &str, on_back: OnPress, body: AnyElement, cx: &App) -> AnyElement {
    let back = crate::session_screen::back_link_row(
        "session-pane-back",
        label.to_string(),
        move |window, cx| on_back(window, cx),
        cx,
    );
    v_flex()
        .size_full()
        .min_h_0()
        .child(div().w_full().flex_shrink_0().px(px(crate::work_header::WORK_GUTTER)).pt_4().pb_2().child(back))
        .child(div().flex_1().min_h_0().w_full().flex().flex_col().child(body))
        .into_any_element()
}

/// The back's label: the tab the item was opened from (`Back` from Chat).
pub(crate) fn pane_back_label(active: SessionScreenTab) -> String {
    match active {
        SessionScreenTab::Chat => SESSION_SCREEN.copy.back.clone(),
        tab => tab.label(),
    }
}

/// A session opened in the pane: its raw work and its own composer (the
/// in-place session view's body).
pub(crate) fn session_pane(view: &Entity<crate::session_screen::SessionScreenView>, cx: &App) -> AnyElement {
    let inner = view.read(cx).inner().clone();
    let scrolled = inner.read(cx).body_scrolled(cx);
    let via_app = crate::session_screen::via_app_line(inner.read(cx).session_row(), cx)
        .map(|line| div().w_full().px(px(crate::work_header::WORK_GUTTER)).pb_1().child(line));
    v_flex()
        .size_full()
        .min_h_0()
        .children(via_app)
        .child(crate::work_header::work_body(div().flex_1().min_h_0().child(inner).into_any_element(), false, scrolled))
        .into_any_element()
}

/// session-screen.json STATUS WORD (`session_screen_status`): THE tree's
/// lowercase word (needs input > working > queued > done > idle), never
/// dropped; `tree` may or may not carry the root.
pub(crate) fn tree_status_word(root: Option<&SessionRow>, tree: &[SessionRow]) -> String {
    use domain::session_screen::{session_screen_status, SessionScreenStatusInput};
    let input = |row: &SessionRow, is_root: bool| SessionScreenStatusInput {
        status: row.status.clone().unwrap_or_default(),
        busy: row.busy,
        needs_input: row.needs_input,
        root: is_root,
    };
    let root_id = root.map(|root| root.id.as_str());
    let mut rows: Vec<SessionScreenStatusInput> = Vec::with_capacity(tree.len() + 1);
    if let Some(root) = root.filter(|root| !tree.iter().any(|row| row.id == root.id)) {
        rows.push(input(root, true));
    }
    rows.extend(tree.iter().map(|row| input(row, Some(row.id.as_str()) == root_id)));
    session_screen_status(&rows)
}

/// session-screen.json TONE ×4: the subtitle's colour (attention = the
/// needs-input amber, active = the foreground, muted = the muted default).
pub(crate) fn tone_color(tone: domain::apps::EditorHeaderTone, cx: &App) -> gpui::Hsla {
    match tone {
        domain::apps::EditorHeaderTone::Attention => theme::tokens::YELLOW.to_hsla(),
        domain::apps::EditorHeaderTone::Active => cx.theme().foreground,
        domain::apps::EditorHeaderTone::Muted => cx.theme().muted_foreground,
    }
}

/// The header's subtitle (`session_screen_subtitle`): THE tree's status word
/// ([`tree_status_word`]), the root's device's name (else the agent's), an
/// agent's first enabled trigger.
pub(crate) fn screen_subtitle(
    root: Option<&SessionRow>,
    tree: &[SessionRow],
    agent: Option<&domain::rows::AgentDefRow>,
    cx: &App,
) -> String {
    let status = tree_status_word(root, tree);
    let device_id = root
        .and_then(|root| root.device_id.clone())
        .or_else(|| agent.and_then(|agent| agent.device_id.clone()));
    let device = device_id
        .as_deref()
        .and_then(|id| crate::entity_preview::find_device(id, cx))
        .and_then(|device| device.label);
    let trigger = agent.and_then(|agent| {
        agent
            .trigger_list()
            .into_iter()
            .find(|trigger| trigger.get("enabled").and_then(serde_json::Value::as_bool) != Some(false))
            .map(|trigger| domain::agent_settings::trigger_summary(&trigger))
    });
    domain::session_screen::session_screen_subtitle(Some(status.as_str()), device.as_deref(), trigger.as_deref())
}

/// Open `session_id` IN PLACE (its chat hosts the raw work).
fn open_session_in_place(session_id: String, window: &mut Window, cx: &mut App) {
    crate::navigation::navigate(window, cx, Screen::Session { session_id });
}

/// One row of the Issues / Sessions tab (THE tree rows).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TreeItem {
    Issue {
        issue_id: String,
        identifier: String,
        title: String,
        status: Option<domain::statuses::ResolvedStatus>,
    },
    Session {
        session_id: String,
        name: String,
        state: domain::session_tree::SessionState,
        cli: Option<String>,
    },
}

impl TreeItem {
    /// The row's id (the issue's, the session's).
    pub(crate) fn key(&self) -> &str {
        match self {
            Self::Issue { issue_id, .. } => issue_id,
            Self::Session { session_id, .. } => session_id,
        }
    }

    /// The row's accessible name.
    fn name(&self) -> String {
        match self {
            Self::Issue { identifier, title, .. } => format!("{identifier} {title}"),
            Self::Session { name, .. } => name.clone(),
        }
    }
}

/// Every session UNDER `root_id` (never the root; its children and theirs,
/// subagents included) as THE tree rows, depth-first in creation order;
/// `tree` = [`crate::session_data::tree_of_root`].
pub(crate) fn session_tree_items(root_id: &str, tree: &[SessionRow], cx: &App) -> Vec<(TreeItem, usize)> {
    fn push<'a>(
        node: &'a SessionRow,
        depth: usize,
        kids: &HashMap<&'a str, Vec<&'a SessionRow>>,
        seen: &mut HashSet<&'a str>,
        out: &mut Vec<(&'a SessionRow, usize)>,
    ) {
        if !seen.insert(node.id.as_str()) {
            return;
        }
        out.push((node, depth));
        for &child in kids.get(node.id.as_str()).into_iter().flatten() {
            push(child, depth + 1, kids, seen, out);
        }
    }
    let below: Vec<&SessionRow> = tree.iter().filter(|session| session.id != root_id).collect();
    let ids: HashSet<&str> = below.iter().map(|session| session.id.as_str()).collect();
    let mut kids: HashMap<&str, Vec<&SessionRow>> = HashMap::new();
    let mut tops: Vec<&SessionRow> = Vec::new();
    for &session in &below {
        match session.parent_id.as_deref().filter(|parent| ids.contains(parent)) {
            Some(parent) => kids.entry(parent).or_default().push(session),
            None => tops.push(session),
        }
    }
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    for top in tops {
        push(top, 0, &kids, &mut seen, &mut ordered);
    }
    // A cycle's leftovers still show, at the top.
    for &session in &below {
        push(session, 0, &kids, &mut seen, &mut ordered);
    }
    ordered
        .into_iter()
        .map(|(session, depth)| {
            let name = crate::session_data::session_title_of(&session.id, cx)
                .or_else(|| session.title.clone())
                .unwrap_or_else(|| SESSION_CHAT.copy.untitled.clone());
            (
                TreeItem::Session {
                    session_id: session.id.clone(),
                    name,
                    state: domain::session_tree::session_tab_mark_state(session),
                    cli: session.cli.clone(),
                },
                depth,
            )
        })
        .collect()
}

/// One tree row's body: an issue = THE issue row (status glyph, mono
/// identifier, title); a session = THE tree row (the run mark, the name, its
/// state word in its tone); the guides over the nesting.
fn tree_row_body(item: &TreeItem, guides: &domain::tree_guides::Guides, cx: &App) -> gpui::Div {
    use domain::list_item::{GAP, INDENT, SMALL};
    let theme = cx.theme();
    let row = h_flex()
        .relative()
        .flex_1()
        .min_w_0()
        .h(px(SMALL))
        .gap(px(GAP))
        .items_center()
        .pl(px(INDENT * guides.depth() as f32))
        .children(crate::tree_guides::guide_layer(guides, 0., 0.));
    match item {
        TreeItem::Issue { identifier, title, status, .. } => row
            .children(issue_glyph(status.as_ref(), cx))
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
                    .min_w_0()
                    .flex_shrink(1.)
                    .truncate()
                    .text_sm()
                    .text_color(theme.foreground)
                    .child(SharedString::from(title.clone())),
            ),
        TreeItem::Session { name, state, cli, .. } => {
            let word_color = match state {
                domain::session_tree::SessionState::NeedsYou => crate::work_card::session_tone_color(state.tone(), cx),
                _ => theme.muted_foreground,
            };
            row.child(crate::run_rows::run_mark(crate::run_rows::run_agent(cli.as_deref()), *state, cx))
                .child(
                    div()
                        .min_w_0()
                        .flex_shrink(1.)
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
    }
}

/// THE tree rows of the Issues / Sessions tab (`items` with their depths):
/// each a list item named by its text, the open one selected; a press picks
/// it. `menu` = the issue rows' context menu (Unlink) for the root's links.
pub(crate) fn tree_rows(
    prefix: &str,
    items: &[(TreeItem, usize)],
    open: Option<&str>,
    on_pick: OnTreePick,
    menu: Option<(ChatMenuTarget, HashSet<String>)>,
    cx: &App,
) -> Vec<AnyElement> {
    let depths: Vec<usize> = items.iter().map(|(_, depth)| *depth).collect();
    let guides = domain::tree_guides::guides_for(&depths);
    let theme = cx.theme();
    let (hover, selected) = (theme.list_hover, theme.list_active);
    items
        .iter()
        .zip(guides)
        .map(|((item, _), guides)| {
            let key = item.key().to_string();
            let active = open == Some(key.as_str());
            let picked = item.clone();
            let on_pick = on_pick.clone();
            let row = h_flex()
                .id(SharedString::from(format!("{prefix}-{key}")))
                .role(gpui::Role::ListItem)
                .aria_label(SharedString::from(item.name()))
                .aria_selected(active)
                .w_full()
                .min_w_0()
                .px_2()
                .rounded_md()
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .when(active, |row| row.bg(selected))
                .child(tree_row_body(item, &guides, cx))
                .on_click(move |_, window, cx| on_pick(&picked, window, cx));
            let target = match (&menu, item) {
                (Some((target, linked)), TreeItem::Issue { issue_id, identifier, .. }) if linked.contains(issue_id) => {
                    Some(ChatMenuTarget { chip: Some((issue_id.clone(), identifier.clone())), ..target.clone() })
                }
                _ => None,
            };
            match target {
                Some(target) => row
                    .pointer_context_menu(move |menu, window, cx| chat_menu(menu, &target, window, cx))
                    .into_any_element(),
                None => row.into_any_element(),
            }
        })
        .collect()
}

/// A tab body's scrolling column (the Issues / Sessions lists).
pub(crate) fn tab_list(id: &str, lead: Option<AnyElement>, rows: Vec<AnyElement>) -> AnyElement {
    div()
        .id(SharedString::from(id.to_string()))
        .debug_selector({
            let id = id.to_string();
            move || id.clone()
        })
        .flex_1()
        .min_h_0()
        .w_full()
        .overflow_y_scroll()
        .child(
            v_flex()
                .w_full()
                .px(px(crate::work_header::WORK_GUTTER))
                .pt_4()
                .pb_10()
                .gap_0p5()
                .children(lead)
                .children(rows),
        )
        .into_any_element()
}

/// The Issues tab's `+` (THE Menu: Link an issue › the search list, New
/// issue), right-aligned over the rows.
pub(crate) fn issues_add(prefix: &str, target: ChatMenuTarget, cx: &App) -> AnyElement {
    let trigger = crate::controls::ghost_icon_button(
        SharedString::from(format!("{prefix}-issues-add")),
        Icon::new(registry::UI_ADD),
        cx,
    )
    .tooltip(SESSION_SCREEN.copy.link_issue.clone());
    h_flex()
        .w_full()
        .pb_1()
        .child(div().flex_1())
        .child(crate::controls::PointerMenu::new(
            SharedString::from(format!("{prefix}-issues-add-menu")),
            trigger,
            move |menu, window, cx| chat_menu(menu, &target, window, cx),
        ))
        .into_any_element()
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// Whether the page may say the chat is missing (pure): the sessions shape
/// settled, the root is not among its rows, and the page has been open past
/// [`MISSING_GRACE`].
pub(crate) fn chat_not_found(ready: bool, has_root: bool, open_for: Duration) -> bool {
    ready && !has_root && open_for >= MISSING_GRACE
}

/// What one render reads, re-derived on every collection change.
#[derive(Clone)]
struct ChatModel {
    root: SessionRow,
    view: Rc<SessionChatView>,
    /// #EXP-1732: every issue of the tree (each session's links, in tree
    /// order, every issue once), by id, and their order.
    issues: HashMap<String, Issue>,
    issue_order: Vec<String>,
    /// The ROOT's own links (the Unlink menu, the NEW ring).
    links: Vec<SessionIssueRow>,
    pulls: Vec<PullRequestRow>,
    /// #EXP-1732: the whole tree (the root first): the Sessions tab.
    tree: Vec<SessionRow>,
}

impl ChatModel {
    /// Every issue of the tree.
    fn linked_ids(&self) -> Vec<String> {
        self.issue_order.clone()
    }

    /// The pull request the Guide reads and merges.
    fn pull(&self) -> Option<&PullRequestRow> {
        crate::chats_view::root_pull_request(&self.root.id, &self.pulls)
    }
}

fn chat_model(root_id: &str, cx: &App) -> Option<ChatModel> {
    // #LOGS-7: THE root view, shared with the transcript's suggested tasks.
    let view = crate::session_data::root_chat_view(root_id, crate::session_data::now_ms(), cx)?;
    let root = crate::session_data::session(root_id, cx)?;
    let store = Store::try_global(cx)?;
    let collections = store.collections();
    let all_links = collections.session_issues.read(cx);
    let links: Vec<SessionIssueRow> = all_links.iter().filter(|link| link.session_id == root_id).cloned().collect();
    let tree = crate::session_data::tree_of_root(root_id, cx);
    let issue_rows = collections.issues.read(cx);
    let mut issues: HashMap<String, Issue> = HashMap::new();
    let mut issue_order = Vec::new();
    for session in &tree {
        let mut own_links: Vec<&SessionIssueRow> = all_links.iter().filter(|link| link.session_id == session.id).collect();
        own_links.sort_by(|a, b| {
            a.sort_order
                .unwrap_or(0.0)
                .total_cmp(&b.sort_order.unwrap_or(0.0))
                .then_with(|| a.issue_id.cmp(&b.issue_id))
        });
        for link in own_links {
            if issues.contains_key(&link.issue_id) {
                continue;
            }
            if let Some(issue) = issue_rows.get(&link.issue_id) {
                issue_order.push(issue.id.clone());
                issues.insert(issue.id.clone(), issue.clone());
            }
        }
    }
    Some(ChatModel {
        root,
        view,
        issues,
        issue_order,
        links,
        pulls: crate::session_data::pull_requests_of_root(root_id, cx),
        tree,
    })
}

/// #EXP-1732: the Issues tab's rows: the chat's issue tabs (sub-issues
/// nested under their parent), else (an agent's tree) every issue of the
/// tree in order.
fn screen_issue_items(model: &ChatModel, cx: &App) -> Vec<(TreeItem, usize)> {
    let item = |issue_id: &str, identifier: &str, depth: usize| {
        let issue = model.issues.get(issue_id);
        (
            TreeItem::Issue {
                issue_id: issue_id.to_string(),
                identifier: issue.map(|issue| issue.identifier.clone()).unwrap_or_else(|| identifier.to_string()),
                title: issue.map(|issue| issue.title.clone()).unwrap_or_default(),
                status: issue.map(|issue| crate::queries::resolve_issue_status(cx, issue)),
            },
            depth,
        )
    };
    let mut out = Vec::new();
    for tab in &model.view.tabs {
        if let ChatTab::Issue { issue_id, identifier, nested } = tab {
            out.push(item(issue_id, identifier, 0));
            out.extend(nested.iter().map(|nested| item(&nested.issue_id, &nested.identifier, nested.depth)));
        }
    }
    if out.is_empty() {
        out = model
            .issue_order
            .iter()
            .filter_map(|id| model.issues.get(id))
            .map(|issue| item(&issue.id, &issue.identifier, 0))
            .collect();
    }
    out
}

/// #EXP-1732: a chat's header avatar: THE chat lead disc (needs you, then
/// working, then the PR's glyph, else the chat glyph).
fn chat_header_lead(model: &ChatModel, cx: &App) -> AnyElement {
    use domain::chats::ChatLead;
    let lead = if model.view.needs_you {
        ChatLead::NeedsYou
    } else if model.view.busy {
        ChatLead::Working
    } else {
        match domain::session_tree::chat_row_lead_pr(model.pulls.iter().filter_map(|pull| pull.state.as_deref())) {
            Some(domain::session_tree::PrLead::Open) => ChatLead::Open,
            Some(domain::session_tree::PrLead::Merged) => ChatLead::Merged,
            None => ChatLead::Waiting,
        }
    };
    let agent = model.root.cli.as_deref().and_then(coding::CodingAgent::parse);
    crate::chat_list::chat_lead_disc(lead, agent, false, cx)
}

/// EXP-1346: how far beyond the viewport a transcript list renders and
/// measures rows (the session feed's `FEED_OVERDRAW`).
pub(crate) const CONVERSATION_OVERDRAW: gpui::Pixels = px(1024.);

/// The Results tab's own state: THE review pane open, the pull request's
/// diff, the hovered stack member. Merges ride [`crate::pr_merge`]'s state.
#[derive(Default)]
struct ResultsState {
    review: bool,
    diff: Option<(String, Entity<crate::pr_diff::PrDiffView>)>,
    stack_hovered: Option<String>,
}

/// #EXP-1545: a Live child's relay viewer, read for its Queue only.
struct QueueFeed {
    /// The child's name as the chat's tabs show it.
    name: String,
    view: Entity<crate::steer_viewer::SteerSessionView>,
    /// #EXP-1678: when the chat attached it (the probe deadline's clock).
    attached: Instant,
    /// This process runs the child: its engine, no network stream at all.
    local: bool,
    _sub: Subscription,
}

/// EXP-1686 (`chat_mode`): the agent whose conversation this tree is;
/// `None` = a chat.
fn conversation_agent(model: &ChatModel) -> Option<&str> {
    let agent_id = model.root.agent_id.as_deref();
    (domain::session_tree::chat_mode(agent_id) == domain::session_tree::ChatMode::Bubbles)
        .then_some(agent_id)
        .flatten()
}

/// #EXP-1545: the tree's Live (not ended) children with their tab names
/// (top-level chips and the sessions folded under them). EXP-1686: an agent's
/// chat has no chips, so its children come straight from the tree.
fn live_children(view: &SessionChatView, root: &SessionRow, cx: &App) -> Vec<(String, String)> {
    let root_id = root.id.as_str();
    if domain::session_tree::chat_mode(root.agent_id.as_deref()) == domain::session_tree::ChatMode::Bubbles {
        return crate::session_data::tree_of_root(root_id, cx)
            .into_iter()
            .filter(|row| row.id != root_id && !row.is_ended())
            .map(|row| (row.id, row.title.unwrap_or_default()))
            .collect();
    }
    view.tabs
        .iter()
        .filter_map(|tab| match tab {
            ChatTab::Session { session_id, name, nested, .. } => Some(
                std::iter::once((session_id.clone(), name.clone()))
                    .chain(nested.iter().map(|item| (item.session_id.clone(), item.name.clone())))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .filter(|(id, _)| id != root_id)
        .filter(|(id, _)| crate::session_data::session(id, cx).is_some_and(|row| !row.is_ended()))
        .collect()
}

pub(crate) struct SessionChatScreenView {
    root_id: String,
    /// EXP-1686: an agent's chat: the call covers the conversation while it
    /// runs (the agent header's telephone focuses it).
    call_panel: Entity<etalk_gpui::voice_call::CallPanel>,
    results: ResultsState,
    /// #EXP-1732: the header tab on show (session-screen.json `active`).
    tab: SessionScreenTab,
    /// #EXP-1732: the item opened over the tab pane (the session open in
    /// place is `focus`).
    opened: Option<ChatOpened>,
    /// The `focus` was opened from the Sessions tab of a tree no route
    /// hosts in place (an agent's): this view owns its session view.
    focus_owned: bool,
    /// THE gear's agent settings page (built on first open) and its scroll.
    settings: Option<(String, Entity<crate::settings::agent_detail::AgentDetailPane>)>,
    settings_scroll: gpui::ScrollHandle,
    /// EXP-1346: shared, never deep-cloned per frame.
    model: Option<Rc<ChatModel>>,
    /// #EXP-1380: a [`Self::queue_refresh`] is pending.
    refresh_queued: bool,
    /// #LOGS-3: a refresh is due when the held chat view re-derives.
    held_wake: bool,
    /// The opened issue's detail view (built on first show, re-pointed).
    issue_view: Option<Entity<crate::issue_detail::IssueDetailView>>,
    images: Entity<ImageCache>,
    /// #EXP-1732 THE Chat tab: the ROOT's own session view (its raw work,
    /// its cards, THE Dock and THE composer posting to the root), ALWAYS,
    /// for every kind.
    root_view: Entity<crate::session_screen::SessionScreenView>,
    /// The ORIGIN of the session open in the pane (its starter), off the
    /// tree's view with that focus.
    focus_origin: Option<domain::session_tree::SessionXref>,
    /// When each issue link was FIRST rendered, and whether it was new then
    /// (linked inside [`NEW_CHILD_WINDOW`]) — the fade-in ring.
    first_seen: HashMap<String, (Instant, bool)>,
    opened_at: Instant,
    /// The chip memo behind [`Self::text`] (cleared when the issues or
    /// statuses notify) and this frame's resolver over it.
    chip_cache: IssueChipCache,
    text: ChatText,
    focus_handle: FocusHandle,
    _ring_tick: Option<Task<()>>,
    /// EXP-1316 (1b): the session open IN PLACE (`Screen::Session` of this
    /// tree, or a Sessions row): its id and the screen view whose transcript
    /// the tab pane hosts.
    focus: Option<(String, Entity<crate::session_screen::SessionScreenView>)>,
    _focus_sub: Option<Subscription>,
    /// #EXP-1545 (dock.json CHAT QUEUE): one relay viewer per Live child
    /// of the tree, read for its Queue only (its per-session slots, seeded
    /// LOCAL FIRST from the last-known memo): the root view's Dock shows the
    /// picked one's ([`SteerSessionView::set_queue_from`]).
    ///
    /// [`SteerSessionView::set_queue_from`]: crate::steer_viewer::SteerSessionView::set_queue_from
    queue_feeds: HashMap<String, QueueFeed>,
    /// #EXP-1678: remote children read closed (their activeAt then, when).
    closed_queue_reads: HashMap<String, (Option<i64>, Instant)>,
    /// #EXP-1678: a probe re-read is scheduled.
    probe_timer: Option<gpui::Task<()>>,
    /// #EXP-1562 (`?card=`): a waiting card to scroll to (and ring) in the
    /// root's session view.
    reveal: Option<String>,
    /// #EXP-1785: a waiting chip's question UNDER the root: the session it
    /// opened in place and the card to scroll to (and ring) once that
    /// session's feed holds it; given up after [`WAITING_REVEAL_WAIT`].
    waiting_reveal: Option<(String, String, Instant)>,
    /// #EXP-1662: the opened plan's body state and the plan it last painted
    /// (a new plan opens from its top).
    plan_state: crate::plan_page::PlanPageState,
    plan_painted: Option<String>,
    _missing_tick: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl SessionChatScreenView {
    pub(crate) fn new(root_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let id = root_id.clone();
        let root_view = cx.new(|cx| crate::session_screen::SessionScreenView::new(id, window, cx));
        // THE root composer: Send⇄Stop over the whole tree, the chat's ids.
        let inner = root_view.read(cx).inner().clone();
        inner.update(cx, |inner, cx| {
            inner.host_root_composer(
                SESSION_CHAT.copy.placeholder.clone(),
                crate::steer_viewer::ComposerIds::new(
                    SESSION_CHAT.test_ids.composer.clone(),
                    "chat-attach",
                    "chat-model",
                    "chat-context",
                ),
                window,
                cx,
            )
        });
        // The Chat tab repaints with the root's feed.
        let mut subscriptions = vec![cx.observe(&root_view, |_, _, cx| cx.notify())];
        if let Some(store) = Store::try_global(cx) {
            let collections = store.collections().clone();
            subscriptions.push(cx.observe(&collections.sessions, |this, _, cx| this.queue_refresh(cx)));
            subscriptions.push(cx.observe(&collections.session_issues, |this, _, cx| this.queue_refresh(cx)));
            subscriptions.push(cx.observe(&collections.messages, |this, _, cx| this.queue_refresh(cx)));
            subscriptions.push(cx.observe(&collections.pull_requests, |this, _, cx| this.queue_refresh(cx)));
            // EXP-1686: an agent's chat header reads its `agents` row.
            subscriptions.push(cx.observe(&collections.agents, |_, _, cx| cx.notify()));
            // #EXP-1611: the header's Merge pill + Checks submenu read them.
            subscriptions.push(cx.observe(&collections.pull_request_checks, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.issues, |this, _, cx| {
                this.chip_cache.borrow_mut().clear();
                this.queue_refresh(cx);
            }));
            subscriptions.push(cx.observe(&collections.issue_statuses, |this, _, cx| {
                this.chip_cache.borrow_mut().clear();
                cx.notify();
            }));
        }
        // The merge capsule's armed / merging state.
        let merge_state = crate::pr_merge::MergeState::global(cx);
        subscriptions.push(cx.observe(&merge_state, |_, _, cx| cx.notify()));
        let transport = crate::queries::attachment_transport(cx);
        let images = cx.new(|_| ImageCache::new(transport));
        // A picture tile (the Guide's, a picture card) asks the cache for its
        // slot while painting and gets `Loading`; the bytes land later and
        // only the CACHE notifies — without this the tile stayed an empty
        // frame until something else re-rendered the chat.
        subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
        // Re-render once the grace passed, so a missing chat says so.
        let missing_tick = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(MISSING_GRACE).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        });
        let mut this = Self {
            root_id,
            call_panel: cx.new(etalk_gpui::voice_call::CallPanel::new),
            results: ResultsState::default(),
            tab: SessionScreenTab::Chat,
            opened: None,
            focus_owned: false,
            settings: None,
            settings_scroll: gpui::ScrollHandle::new(),
            model: None,
            refresh_queued: false,
            held_wake: false,
            issue_view: None,
            images,
            root_view,
            focus_origin: None,
            first_seen: HashMap::new(),
            opened_at: Instant::now(),
            chip_cache: Default::default(),
            text: ChatText::default(),
            focus_handle: cx.focus_handle(),
            _ring_tick: None,
            focus: None,
            _focus_sub: None,
            queue_feeds: HashMap::new(),
            closed_queue_reads: HashMap::new(),
            probe_timer: None,
            reveal: None,
            waiting_reveal: None,
            plan_state: Default::default(),
            plan_painted: None,
            _missing_tick: missing_tick,
            _subscriptions: subscriptions,
        };
        this.refresh(cx);
        // A capture (and a debug launch) can open a chat on its Results tab.
        if std::env::var("EXP_DEV_CHAT_CHILD").ok().as_deref() == Some("results") {
            this.tab = SessionScreenTab::Results;
        }
        this
    }

    /// #EXP-1380: one [`Self::refresh`] per effect cycle, however many
    /// collections synced in it.
    fn queue_refresh(&mut self, cx: &mut gpui::Context<Self>) {
        if self.refresh_queued {
            return;
        }
        self.refresh_queued = true;
        // #LOGS-3: a task, not `defer`: the sync drain applies every delta
        // already queued (a burst names several shapes) before the task
        // runs, so the chat re-derives once per burst, not once per shape.
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |this, cx| {
                this.refresh_queued = false;
                this.refresh(cx);
            });
        })
        .detach();
    }

    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        let model = chat_model(&self.root_id, cx);
        // #LOGS-3: the view answered held (a busy team, an expensive derive):
        // ask again once it re-derives.
        if let Some(at) = crate::session_data::root_chat_view_stale_until(&self.root_id) {
            if !self.held_wake {
                self.held_wake = true;
                let wait = at.saturating_duration_since(cx.background_executor().now());
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(wait).await;
                    let _ = this.update(cx, |this, cx| {
                        this.held_wake = false;
                        this.queue_refresh(cx);
                        // The transcript's suggestions read the same view.
                        let inner = this.root_view.read(cx).inner().clone();
                        inner.update(cx, |_, cx| cx.notify());
                    });
                })
                .detach();
            }
        }
        // The pane's session: its own origin (the tree's view focused on it).
        self.focus_origin = self.focus.as_ref().and_then(|(id, _)| {
            crate::session_data::chat_view_and_messages(&self.root_id, Some(id), crate::session_data::now_ms(), false, cx)
                .and_then(|(view, _)| view.origin)
        });
        if let Some(model) = &model {
            self.note_seen(&model.links, cx);
            // An unlinked issue closes.
            if let Some(ChatOpened::Issue(issue_id)) = &self.opened {
                if !model.linked_ids().contains(issue_id) {
                    self.opened = None;
                }
            }
        }
        // Session 58cb32df: a surface card arriving while the chat is open
        // plays its entrance once.
        if let Some(model) = &model {
            crate::surface_card::sync_rows(cx.entity_id(), &model.view.rows, cx);
        }
        self.model = model.map(Rc::new);
        cx.notify();
    }

    /// Record the first render of every issue link; one linked inside the
    /// window gets the ring until [`NEW_CHILD_WINDOW`] passes.
    fn note_seen(&mut self, links: &[SessionIssueRow], cx: &mut gpui::Context<Self>) {
        let now_ms = crate::session_data::now_ms();
        let mut armed = false;
        for link in links {
            if self.first_seen.contains_key(&link.id) {
                continue;
            }
            let created = link
                .created_at
                .as_deref()
                .and_then(crate::comments::parse_epoch)
                .map(|seconds| seconds * 1000)
                .unwrap_or(0);
            let fresh = now_ms - created < NEW_CHILD_WINDOW.as_millis() as i64;
            self.first_seen.insert(link.id.clone(), (Instant::now(), fresh));
            armed |= fresh;
        }
        if armed {
            self._ring_tick = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(NEW_CHILD_WINDOW).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            }));
        }
    }

    /// The tab closed: an open issue flushes its edit; the root's session
    /// view and a session this view opened in the pane drop their feeds
    /// (never the sessions).
    pub(crate) fn shutdown(&mut self, cx: &mut App) {
        self.root_view.update(cx, |view, cx| view.shutdown(cx));
        for (_, feed) in self.queue_feeds.drain() {
            feed.view.update(cx, |view, _| view.shutdown());
        }
        self.flush_issue(cx);
        if self.focus_owned {
            if let Some((_, view)) = self.focus.take() {
                view.update(cx, |view, cx| view.shutdown(cx));
            }
            self.focus_owned = false;
            self._focus_sub = None;
        }
    }

    /// An opened issue flushes its pending title / description (the
    /// close-tab rule).
    fn flush_issue(&self, cx: &mut App) {
        if let (Some(ChatOpened::Issue(_)), Some(view)) = (&self.opened, &self.issue_view) {
            view.update(cx, |detail, cx| {
                detail.flush_title(cx);
                detail.flush_description(cx);
            });
        }
    }

    /// #EXP-1732: open `item` over the tab pane (the beside rule shows it
    /// next to the chat).
    pub(crate) fn open_item(&mut self, item: ChatOpened, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.focus.is_none() && self.opened.as_ref() == Some(&item) {
            return;
        }
        self.flush_issue(cx);
        self.leave_focus(window, cx);
        if let ChatOpened::Issue(issue_id) = &item {
            let view = self
                .issue_view
                .get_or_insert_with(|| {
                    cx.new(|cx| {
                        let mut detail = crate::issue_detail::IssueDetailView::new(window, cx);
                        detail.set_embedded(true);
                        detail
                    })
                })
                .clone();
            let issue_id = issue_id.clone();
            view.update(cx, |detail, cx| detail.set_issue(issue_id, window, cx));
        }
        self.results.review = false;
        self.opened = Some(item);
        cx.notify();
    }

    /// #EXP-1732: the opened item's `‹`: back to the tab it came from.
    pub(crate) fn close_item(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.flush_issue(cx);
        self.opened = None;
        self.leave_focus(window, cx);
        cx.notify();
    }

    /// Leave the session open in the pane: one this view opened goes, one
    /// the route opened (`Screen::Session` of this tree) navigates back to
    /// `Screen::Chat` (the same tab).
    fn leave_focus(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.focus.is_none() {
            return;
        }
        if self.focus_owned {
            if let Some((_, view)) = self.focus.take() {
                view.update(cx, |view, cx| view.shutdown(cx));
            }
            self.focus_owned = false;
            self._focus_sub = None;
            self.refresh(cx);
            return;
        }
        let session_id = self.root_id.clone();
        window.defer(cx, move |window, cx| {
            crate::navigation::navigate(window, cx, Screen::Chat { session_id });
        });
    }

    /// #EXP-1732: pick a header tab; an opened item closes.
    pub(crate) fn set_tab(&mut self, tab: SessionScreenTab, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.opened.is_some() || self.focus.is_some() {
            self.close_item(window, cx);
        } else if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.results.review = false;
        cx.notify();
    }

    /// The Sessions tab's row: that session's raw work IN PLACE. A chat's
    /// session rides the route (`Screen::Session`, the panel focuses it
    /// here); any other tree's (an agent's) is this view's own.
    fn open_session_here(&mut self, session_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.focus_id() == Some(session_id.as_str()) {
            return;
        }
        self.flush_issue(cx);
        self.opened = None;
        if crate::screens::in_place_root(&session_id, cx).as_deref() == Some(self.root_id.as_str()) {
            window.defer(cx, move |window, cx| open_session_in_place(session_id, window, cx));
            cx.notify();
            return;
        }
        if self.focus_owned {
            if let Some((_, view)) = self.focus.take() {
                view.update(cx, |view, cx| view.shutdown(cx));
            }
        }
        let id = session_id.clone();
        let view = cx.new(|cx| crate::session_screen::SessionScreenView::new(id, window, cx));
        self._focus_sub = Some(cx.observe(&view, |_, _, cx| cx.notify()));
        self.focus = Some((session_id, view));
        self.focus_owned = true;
        self.refresh(cx);
    }

    /// EXP-1316 (1b): show `focus`'s session IN PLACE (the panel calls this
    /// on a `Screen::Session` of this tree), or none (`Screen::Chat`).
    /// #EXP-1732: it opens in the tab pane, beside the chat.
    pub(crate) fn set_focus(
        &mut self,
        focus: Option<(String, Entity<crate::session_screen::SessionScreenView>)>,
        cx: &mut gpui::Context<Self>,
    ) {
        // #EXP-1732: the root itself is never opened as a session: its work
        // IS the Chat tab.
        let focus = focus.filter(|(id, _)| *id != self.root_id);
        // The chat's own route never closes a session this view opened.
        if focus.is_none() && self.focus_owned {
            return;
        }
        let same = match (&self.focus, &focus) {
            (Some((a, view_a)), Some((b, view_b))) => a == b && view_a == view_b,
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        if self.focus_owned {
            if let Some((_, view)) = self.focus.take() {
                view.update(cx, |view, cx| view.shutdown(cx));
            }
            self.focus_owned = false;
        }
        if focus.is_some() {
            self.flush_issue(cx);
            self.opened = None;
        }
        self._focus_sub = focus
            .as_ref()
            .map(|(_, view)| cx.observe(view, |_, _, cx| cx.notify()));
        self.focus = focus;
        self.refresh(cx);
    }

    /// The focused session's id.
    pub(crate) fn focus_id(&self) -> Option<&str> {
        self.focus.as_ref().map(|(id, _)| id.as_str())
    }

    /// EXP-1347: the issue ids of this chat's tree (nested ones too).
    pub(crate) fn issue_tab_ids(&self) -> Vec<String> {
        self.model.as_ref().map(|model| model.linked_ids()).unwrap_or_default()
    }

    /// EXP-1347: an issue pill of one of this chat's issues opens it in the
    /// pane.
    pub(crate) fn open_issue_tab(&mut self, issue_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.open_item(ChatOpened::Issue(issue_id), window, cx);
    }

    /// #EXP-1562: a waiting card: scroll it to the centre of the chat and
    /// ring it ([`Self::take_reveal`]); the chat is always on show.
    fn select_waiting(&mut self, id: &str, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.reveal = Some(id.to_string());
        cx.notify();
    }

    /// #EXP-1785 (waiting-cards.json `waitingTarget`): a header chip's pick.
    /// A plan opens THE plan page as the opened item (any session's plan);
    /// the root's question scrolls the Chat tab to its card (the IDE is wide:
    /// the chat is always on show); a question under the root opens THAT
    /// session in place, scrolled to its card.
    fn pick_waiting(&mut self, card: &domain::waiting_cards::WaitingCardInput, window: &mut Window, cx: &mut gpui::Context<Self>) {
        use domain::waiting_cards::{waiting_target, WaitingOpen};
        let target = waiting_target(card.kind, &card.session_id, &self.root_id);
        match target.open {
            WaitingOpen::Plan => self.open_item(ChatOpened::Plan(card.id.clone()), window, cx),
            WaitingOpen::Card => self.select_waiting(&card.id, window, cx),
            WaitingOpen::Session => {
                self.waiting_reveal = Some((target.session_id.clone(), card.id.clone(), Instant::now()));
                self.open_session_here(target.session_id, window, cx);
                cx.notify();
            }
        }
    }

    /// #EXP-1785: the pending reveal in the session opened in place, once
    /// its feed holds the card (the ring alone once the wait ran out).
    fn take_waiting_reveal(&mut self, cx: &mut gpui::Context<Self>) {
        let Some((session_id, card_id, at)) = self.waiting_reveal.clone() else { return };
        let Some((focus_id, view)) = self.focus.clone() else {
            if at.elapsed() > WAITING_REVEAL_WAIT {
                self.waiting_reveal = None;
            }
            return;
        };
        if focus_id != session_id {
            if at.elapsed() > WAITING_REVEAL_WAIT {
                self.waiting_reveal = None;
            }
            return;
        }
        let inner = view.read(cx).inner().clone();
        let found = inner.update(cx, |inner, cx| inner.try_reveal_card(&card_id, cx));
        if found {
            self.waiting_reveal = None;
        } else if at.elapsed() > WAITING_REVEAL_WAIT {
            self.waiting_reveal = None;
            crate::waiting_cards::highlight(&card_id, cx);
        }
    }

    /// #EXP-1562 (`?card=`): the pending reveal, scrolled to and ringed in
    /// the root's session view.
    fn take_reveal(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(id) = self.reveal.take() else { return };
        let inner = self.root_view.read(cx).inner().clone();
        inner.update(cx, |inner, cx| inner.reveal_card(&id, cx));
    }

    // ── render ───────────────────────────────────────────────────────────

    fn menu_target(&self, chip: Option<(String, String)>) -> ChatMenuTarget {
        ChatMenuTarget {
            root_id: self.root_id.clone(),
            team_id: self.model.as_ref().and_then(|model| model.root.team_id.clone()),
            chip,
            linked: self.model.as_ref().map(|model| model.linked_ids()).unwrap_or_default(),
        }
    }

    /// #EXP-1732 THE header (session-screen.json): the lead avatar, the
    /// title over its subtitle, the merge status pill, an agent's telephone,
    /// THE gear (agents, the Assistant), the `…`; THE Segmented under it.
    fn render_header(
        &mut self,
        model: &ChatModel,
        screen: &SessionScreen,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        // #EXP-1710 (merge-train.json STATUS PILL): the PR's merge STATUS
        // (any viewer; a closed PR has none), its tap the Checks list.
        let member = chat_member(model, cx);
        let merge = model.pull().cloned().and_then(|pull| {
            crate::work_header::merge_status_pill(
                SharedString::from(domain::merge_train::MERGE_TRAIN.test_ids.status_pill.clone()),
                &pull,
                member,
                crate::surface::PillSize::Md,
                cx,
            )
        });
        // #FEED-191 (chats.json THE WAY BACK): a session link opened this.
        let back = crate::session_screen::session_link_back(window, cx);
        let this = cx.entity().downgrade();
        let on_tab: OnTabPick = Rc::new(move |tab, window, cx| {
            let _ = this.update(cx, |this, cx| this.set_tab(tab, window, cx));
        });
        let tabs = screen_tabs(&screen.tabs, screen.active, on_tab, cx);
        let gear = screen.gear.then(|| {
            let open = matches!(self.opened, Some(ChatOpened::Settings));
            let this = cx.entity().downgrade();
            let on_press: OnPress = Rc::new(move |window, cx| {
                let _ = this.update(cx, |this, cx| {
                    if open {
                        this.close_item(window, cx);
                    } else {
                        this.open_item(ChatOpened::Settings, window, cx);
                    }
                });
            });
            (on_press, open)
        });
        // #EXP-1785 (waiting-cards.json): the tree's WAITING plans and
        // questions, at the START of the trailing slot (after the tabs).
        let this = cx.entity().downgrade();
        let on_waiting: crate::waiting_cards::OnWaitingPick = Rc::new(move |card, window, cx| {
            let _ = this.update(cx, |this, cx| this.pick_waiting(card, window, cx));
        });
        let waiting = crate::waiting_cards::header_chips(&crate::waiting_cards::chat_waiting_cards(&model.view.rows), on_waiting, cx);
        let (avatar, title, trailing, menu, agent) = match conversation_agent(model) {
            // An agent's (the Assistant's) conversation: its disc, its NAME,
            // the telephone, THE agent menu.
            Some(agent_id) => {
                let agent = crate::session_data::agent_def(agent_id, cx);
                let name = agent.as_ref().map(crate::agent_screen::agent_def_name).unwrap_or_default();
                let panel = self.call_panel.read(cx).focus_handle(cx);
                let call = agent.as_ref().map(|agent| crate::agent_screen::agent_header_call(&agent.id, &name, panel, cx));
                let menu = crate::agent_screen::agent_header_menu(agent_id, cx);
                let avatar = crate::agent_screen::agent_header_disc(agent.as_ref(), cx);
                let title = title_row(&name, false, cx).id("chat-title").into_any_element();
                (avatar, title, merge.into_iter().chain(call).collect::<Vec<_>>(), menu, agent)
            }
            // EXP-1316: a chat's title is derived, plain text (no rename).
            None => {
                let title = title_row(&model.view.title, model.view.private, cx).id("chat-title").into_any_element();
                (chat_header_lead(model, cx), title, merge.into_iter().collect::<Vec<_>>(), guide_menu(model, cx), None)
            }
        };
        let subtitle = screen_subtitle(Some(&model.root), &model.tree, agent.as_ref(), cx);
        let tone = domain::session_screen::session_screen_tone(&tree_status_word(Some(&model.root), &model.tree), false);
        let subtitle_color = Some(tone_color(tone, cx));
        let trailing = waiting.into_iter().chain(trailing).collect();
        screen_header(ScreenHeader { avatar, title, subtitle, subtitle_color, trailing, gear, menu, back, tabs }, cx)
    }

    /// #EXP-1545 + #EXP-1678 (dock.json ONE CHILD STREAM): keep a viewer
    /// for every Live child THIS process runs (its engine: no network) and
    /// ONE eNet viewer among the rest, on the child
    /// [`domain::dock::chat_queue_probe`] names (one per remote child was a
    /// burst of joins to one device on every chat open). Each repaints the
    /// chat when its Queue or what the probe reads moved.
    fn sync_queue_feeds(&mut self, live: Vec<(String, String)>, window: &mut Window, cx: &mut gpui::Context<Self>) {
        use domain::dock::{chat_queue_child_state, chat_queue_probe, ChildQueueState, ChatQueueProbe, CLOSED_QUEUE_TTL_MS, PROBE_DEADLINE_MS};
        let ttl = Duration::from_millis(CLOSED_QUEUE_TTL_MS);
        let active_at = |id: &str, cx: &App| {
            crate::session_data::session(id, cx).and_then(|row| {
                row.last_turn_at
                    .as_deref()
                    .and_then(domain::session_tree::iso_ms)
                    .or_else(|| row.started_at.as_deref().and_then(domain::session_tree::iso_ms))
            })
        };
        let now = Instant::now();
        let root_open = self.root_view.read(cx).inner().read(cx).dock_queue().is_some();
        let mut keep: HashSet<String> = HashSet::new();
        let mut probes = Vec::new();
        for (id, _) in &live {
            let local = self.queue_feeds.get(id).map(|feed| feed.local)
                .unwrap_or_else(|| crate::session_screen::local_engine(id, cx).is_some());
            if local {
                keep.insert(id.clone());
                continue;
            }
            let at = active_at(id, cx);
            let remembered = self
                .closed_queue_reads
                .get(id)
                .is_some_and(|(seen_at, when)| *seen_at == at && now.duration_since(*when) < ttl);
            let (open, stream) = match self.queue_feeds.get(id) {
                Some(feed) => {
                    let view = feed.view.read(cx);
                    let attached = now.duration_since(feed.attached).as_millis() as u64;
                    (view.dock_queue().is_some(), Some(view.probe_stream(attached)))
                }
                None => (false, None),
            };
            probes.push(ChatQueueProbe { session_id: id.clone(), active_at: at, state: chat_queue_child_state(open, remembered, stream) });
        }
        for probe in &probes {
            if probe.state != ChildQueueState::Closed {
                continue;
            }
            let fresh = self
                .closed_queue_reads
                .get(&probe.session_id)
                .is_some_and(|(seen_at, when)| *seen_at == probe.active_at && now.duration_since(*when) < ttl);
            if !fresh {
                self.closed_queue_reads.insert(probe.session_id.clone(), (probe.active_at, now));
            }
        }
        let wanted = chat_queue_probe(root_open, &probes);
        if let Some(wanted) = &wanted {
            keep.insert(wanted.clone());
        }
        let gone: Vec<String> = self.queue_feeds.keys().filter(|id| !keep.contains(id.as_str())).cloned().collect();
        for id in gone {
            if let Some(feed) = self.queue_feeds.remove(&id) {
                feed.view.update(cx, |view, _| view.shutdown());
            }
        }
        let mut attached_remote = false;
        for (id, name) in live {
            if !keep.contains(&id) {
                continue;
            }
            if let Some(feed) = self.queue_feeds.get_mut(&id) {
                feed.name = name;
                continue;
            }
            // A child THIS process runs reads its engine: an eNet viewer of
            // our own run found only our own endpoint (`no_endpoint`) and
            // fell back to a relay tunnel to ourselves.
            let (feed_kind, source) = crate::session_screen::resolve_source(&id, cx);
            let local = feed_kind == crate::session_screen::SessionFeed::Local;
            attached_remote |= !local;
            let view = cx.new(|cx| {
                crate::steer_viewer::SteerSessionView::with_source(id.clone(), source, window, cx)
            });
            let read = |view: &crate::steer_viewer::SteerSessionView| (view.dock_queue(), view.probe_stream(0).phase, view.probe_stream(0).replayed);
            let mut last = read(view.read(cx));
            let sub = cx.observe(&view, move |_, view, cx| {
                let now = read(view.read(cx));
                if now != last {
                    last = now;
                    cx.notify();
                }
            });
            self.queue_feeds.insert(id, QueueFeed { name, view, attached: now, local, _sub: sub });
        }
        // Re-read at the probe deadline after an attach, and (every remote
        // child read closed) once the closed memory runs out.
        let rearm = if attached_remote {
            Some(Duration::from_millis(PROBE_DEADLINE_MS))
        } else if wanted.is_none() && !probes.is_empty() && !root_open && self.probe_timer.is_none() {
            Some(ttl)
        } else {
            None
        };
        if let Some(after) = rearm {
            self.probe_timer = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(after).await;
                let _ = this.update(cx, |this, cx| {
                    this.probe_timer = None;
                    cx.notify();
                });
            }));
        }
    }

    /// #EXP-1545 (`chatDockQueue` ×4): the Live child whose Queue the
    /// Dock's `tasks` card shows, with its name; `None` = the root's own
    /// (open, or nothing to show at all).
    fn chat_queue_from(&self, model: &ChatModel, cx: &App) -> Option<(Entity<crate::steer_viewer::SteerSessionView>, String)> {
        use domain::dock::{chat_dock_queue, ChatQueueCandidate};
        let active_at = |row: &SessionRow| {
            row.last_turn_at
                .as_deref()
                .and_then(domain::session_tree::iso_ms)
                .or_else(|| row.started_at.as_deref().and_then(domain::session_tree::iso_ms))
        };
        let mut candidates = vec![ChatQueueCandidate {
            session_id: self.root_id.clone(),
            name: model.view.title.clone(),
            root: true,
            ended: model.root.is_ended(),
            active_at: active_at(&model.root),
            open: self.root_view.read(cx).inner().read(cx).dock_queue().is_some(),
        }];
        for (id, feed) in &self.queue_feeds {
            let row = crate::session_data::session(id, cx);
            candidates.push(ChatQueueCandidate {
                session_id: id.clone(),
                name: feed.name.clone(),
                root: false,
                ended: row.as_ref().is_none_or(SessionRow::is_ended),
                active_at: row.as_ref().and_then(active_at),
                open: feed.view.read(cx).dock_queue().is_some(),
            });
        }
        let pick = chat_dock_queue(&candidates)?;
        let name = pick.session_name?;
        let feed = self.queue_feeds.get(&pick.session_id)?;
        Some((feed.view.clone(), name))
    }

    // ── the Results tab ──────────────────────────────────────────────────

    /// The pull request's diff view, built (and fetched) once per PR.
    fn pull_diff(
        &mut self,
        pull_request_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<crate::pr_diff::PrDiffView> {
        if let Some((id, view)) = &self.results.diff {
            if id == pull_request_id {
                return view.clone();
            }
        }
        let view = cx.new(|cx| crate::pr_diff::PrDiffView::new(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        let id = pull_request_id.to_string();
        view.update(cx, |view, cx| view.set_pull_request(id.clone(), cx));
        self.results.diff = Some((id, view.clone()));
        self.results.review = false;
        view
    }

    fn open_review(&mut self, open: bool, cx: &mut gpui::Context<Self>) {
        self.results.review = open;
        cx.notify();
    }

    /// results-tab.json: the PR block (the Stack band + THE StackRail, ONE
    /// Changes row opening THE review pane), then the results INLINE; a picture
    /// opens THE lightbox over its result's pictures.
    fn render_results_child(&mut self, model: &ChatModel, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let pull = model.pull().cloned();
        let diff_view = pull.as_ref().map(|pull| self.pull_diff(&pull.id, window, cx));
        let files = diff_view.as_ref().map(|view| view.read(cx).pane_files(cx)).unwrap_or_default();
        let (additions, deletions) =
            files.iter().fold((0u32, 0u32), |(a, d), file| (a + file.additions, d + file.deletions));
        if let (true, Some(view)) = (self.results.review && !files.is_empty(), diff_view.as_ref()) {
            let this = cx.entity().downgrade();
            let back = crate::results_tab::review_header(
                files.len(),
                additions,
                deletions,
                std::rc::Rc::new(move |_window, cx| {
                    let _ = this.update(cx, |this, cx| this.open_review(false, cx));
                }),
                cx,
            );
            return v_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(crate::issue_detail::centered_column(div().w_full().child(back)))
                .child(div().flex_1().min_h_0().w_full().child(view.clone()))
                .into_any_element();
        }
        let lead = pull.as_ref().map(|pull| {
            let this = cx.entity().downgrade();
            let changes = (!files.is_empty()).then(|| {
                crate::results_tab::changes_row(
                    files.len(),
                    additions,
                    deletions,
                    Some(std::rc::Rc::new(move |_window, cx| {
                        let _ = this.update(cx, |this, cx| this.open_review(true, cx));
                    })),
                    cx,
                )
            });
            v_flex()
                .w_full()
                .min_w_0()
                .gap_3()
                .children(self.stack_card(model, pull, cx).map(|card| {
                    div().id(SharedString::from(RESULTS_TAB.tab.test_ids.stack.clone())).child(card)
                }))
                .children(changes)
                .into_any_element()
        });
        let results = crate::results_tab::results_of_root(&self.root_id, cx);
        let images = self.images.clone();
        let body = crate::results_tab::results_tab_body(&results, lead, &images, &crate::results_tab::store_surface, window, cx);
        div()
            .id(SharedString::from(RESULTS_TAB.tab.test_ids.list.clone()))
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .child(crate::issue_detail::centered_column(
                v_flex().px(px(crate::work_header::WORK_GUTTER)).pt_6().pb_10().child(body),
            ))
            .into_any_element()
    }

    /// The Stack card of the chat's pull request (the Guide's): a pick
    /// opens that PR's chat, the hovered member merges through.
    fn stack_card(&self, model: &ChatModel, pull: &PullRequestRow, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let team_id = model.root.team_id.as_deref()?;
        let snapshot = crate::session_data::team_pr_snapshot(team_id, cx);
        let view = snapshot.with_set(|set| domain::pr_stack::stack_view(pull, set))?;
        let roots: HashMap<String, String> = snapshot
            .pulls
            .iter()
            .filter_map(|pull| Some((pull.id.clone(), pull.root_session_id.clone()?)))
            .collect();
        let on_pick: crate::session_results::OnPickStackRow = std::rc::Rc::new(move |pull_request_id, window, cx| {
            if let Some(root_id) = roots.get(pull_request_id) {
                crate::navigation::navigate(window, cx, Screen::Chat { session_id: root_id.clone() });
            }
        });
        // Merge through the hovered member = the stack's ONE confirm.
        let on_merge: crate::session_results::OnPickStackRow = std::rc::Rc::new(move |pull_request_id, window, cx| {
            crate::pr_merge::ask_stack_merge_mode(
                pull_request_id,
                domain::pr_stack::StackConfirmMode::Through,
                window,
                cx,
            );
        });
        let this = cx.entity().downgrade();
        let on_hover: crate::session_results::OnHoverStackRow = std::rc::Rc::new(move |hovered, _window, cx| {
            let _ = this.update(cx, |this, cx| {
                if this.results.stack_hovered != hovered {
                    this.results.stack_hovered = hovered;
                    cx.notify();
                }
            });
        });
        Some(crate::session_results::stack_card(
            &view,
            None,
            Some(on_pick),
            Some(on_merge),
            self.results.stack_hovered.clone(),
            Some(on_hover),
            cx,
        ))
    }

    /// The plan card `id` (a grandchild's card is not among the root's
    /// rows: the synced message's).
    fn plan_row(&self, model: &ChatModel, id: &str, cx: &App) -> Option<SessionChatRow> {
        model.view.rows.iter().find(|row| row.id() == id).cloned().or_else(|| {
            let store = Store::try_global(cx)?;
            let message = store.collections().messages.read(cx).get(id).cloned()?;
            domain::session_tree::ask_card_row(&message).filter(|row| matches!(row, SessionChatRow::Plan { .. }))
        })
    }

    /// #FEED-191: the person left this chat (the pane shows another one, or
    /// none): an opened plan closes, the next visit opens on its tab.
    pub(crate) fn leave(&mut self) {
        if matches!(self.opened, Some(ChatOpened::Plan(_))) {
            self.opened = None;
        }
    }

    /// The plan page (plan-page.json) opened in the pane: only the content,
    /// on the chat's own ground, no composer.
    fn render_plan_child(&mut self, row: &SessionChatRow, window: &Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        if self.plan_painted.as_deref() != Some(row.id()) {
            self.plan_painted = Some(row.id().to_string());
            self.plan_state.scroll.set_offset(gpui::point(px(0.), px(0.)));
        }
        let page = crate::plan_page::plan_page(
            crate::plan_page::PlanPageProps {
                row,
                text: &self.text,
                images: &self.images,
                state: &self.plan_state,
                active: None,
                answer: self.text.answers(),
            },
            window,
            cx,
        );
        div().flex_1().min_h_0().w_full().child(page).into_any_element()
    }

    fn render_issue_child(&mut self) -> AnyElement {
        match &self.issue_view {
            Some(view) => div().flex_1().min_h_0().w_full().child(view.clone()).into_any_element(),
            None => div().flex_1().into_any_element(),
        }
    }

    /// THE gear's settings: the agent's settings page (agent-settings.json:
    /// prompt, triggers, defaults; the Assistant's = its own page), built on
    /// first open.
    fn render_settings(&mut self, model: &ChatModel, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let Some(agent_id) = conversation_agent(model).map(str::to_string) else {
            return div().flex_1().into_any_element();
        };
        let existing = self.settings.as_ref().filter(|(id, _)| *id == agent_id).map(|(_, pane)| pane.clone());
        let pane = match existing {
            Some(pane) => pane,
            None => {
                let nav = crate::navigation::nav_for_window(window, cx);
                let shared = crate::sidebar::rail_shared_for_window(window, cx);
                let id = agent_id.clone();
                let pane = cx.new(|cx| crate::settings::agent_detail::AgentDetailPane::for_agent(id, nav, shared, window, cx));
                self.settings = Some((agent_id, pane.clone()));
                pane
            }
        };
        crate::scroll_pane::v_scroll_pane(
            "session-settings-scroll",
            &self.settings_scroll,
            div().w_full().px_6().py_4().child(pane),
        )
        .into_any_element()
    }

    /// The Issues tab: the `+` (members), then THE issue rows of the tree; a
    /// row opens that issue in place.
    fn render_issues(&self, model: &ChatModel, cx: &mut gpui::Context<Self>) -> AnyElement {
        let items = screen_issue_items(model, cx);
        let open = match &self.opened {
            Some(ChatOpened::Issue(id)) => Some(id.clone()),
            _ => None,
        };
        let this = cx.entity().downgrade();
        let on_pick: OnTreePick = Rc::new(move |item, window, cx| {
            if let TreeItem::Issue { issue_id, .. } = item {
                let issue_id = issue_id.clone();
                let _ = this.update(cx, |this, cx| this.open_item(ChatOpened::Issue(issue_id), window, cx));
            }
        });
        let member = chat_member(model, cx);
        let linked: HashSet<String> = model.links.iter().map(|link| link.issue_id.clone()).collect();
        let menu = member.then(|| (self.menu_target(None), linked));
        let lead = member.then(|| issues_add("chat", self.menu_target(None), cx));
        let rows = tree_rows(&SESSION_SCREEN.test_ids.issues, &items, open.as_deref(), on_pick, menu, cx);
        tab_list(&SESSION_SCREEN.test_ids.issues, lead, rows)
    }

    /// The Sessions tab: THE tree rows of every session under the root; a
    /// row opens that session in place.
    fn render_sessions(&self, model: &ChatModel, cx: &mut gpui::Context<Self>) -> AnyElement {
        let items = session_tree_items(&self.root_id, &model.tree, cx);
        let this = cx.entity().downgrade();
        let on_pick: OnTreePick = Rc::new(move |item, window, cx| {
            if let TreeItem::Session { session_id, .. } = item {
                let session_id = session_id.clone();
                let _ = this.update(cx, |this, cx| this.open_session_here(session_id, window, cx));
            }
        });
        let rows = tree_rows(&SESSION_SCREEN.test_ids.sessions, &items, self.focus_id(), on_pick, None, cx);
        tab_list(&SESSION_SCREEN.test_ids.sessions, None, rows)
    }

    /// The tab pane: an opened item (the session in place, an issue, a
    /// plan, the settings) under its `‹`, else the active tab's body.
    fn render_pane(
        &mut self,
        model: &ChatModel,
        screen: &SessionScreen,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        // The `‹` goes back to the tab the item came from (the opened item
        // makes Sessions / Issues active, session-screen.json).
        let label = pane_back_label(self.tab);
        let this = cx.entity().downgrade();
        let on_back: OnPress = Rc::new(move |window, cx| {
            let _ = this.update(cx, |this, cx| this.close_item(window, cx));
        });
        if let Some((_, view)) = self.focus.clone() {
            // #EXP-1736: the view's S-3 origin = the sender of the prompt
            // that started it (its bubble replaces the `Started by` line).
            let inner = view.read(cx).inner().clone();
            let origin = self.focus_origin.clone();
            if inner.read(cx).transcript_origin_xref() != origin.as_ref() {
                inner.update(cx, |inner, cx| inner.set_transcript_origin(origin, cx));
            }
            let body = session_pane(&view, cx);
            return pane_item(&label, on_back, body, cx);
        }
        match self.opened.clone() {
            Some(ChatOpened::Issue(_)) => {
                let body = self.render_issue_child();
                pane_item(&label, on_back, body, cx)
            }
            Some(ChatOpened::Plan(id)) => {
                let body = match self.plan_row(model, &id, cx) {
                    Some(row) => self.render_plan_child(&row, window, cx),
                    None => div().flex_1().into_any_element(),
                };
                pane_item(&label, on_back, body, cx)
            }
            Some(ChatOpened::Settings) => {
                let body = self.render_settings(model, window, cx);
                pane_item(&label, on_back, body, cx)
            }
            None => match screen.active {
                SessionScreenTab::Issues => self.render_issues(model, cx),
                SessionScreenTab::Sessions => self.render_sessions(model, cx),
                SessionScreenTab::Results => self.render_results_child(model, window, cx),
                _ => div().flex_1().into_any_element(),
            },
        }
    }

    /// #EXP-1732: this screen's `session_screen` (the IDE is WIDE: the
    /// beside rule always). An app's editor session opened as a chat reads
    /// as a chat here (the app editor is its own screen).
    fn screen_of(&self, model: &ChatModel, cx: &mut gpui::Context<Self>) -> SessionScreen {
        let agent_id = conversation_agent(model);
        let assistant = agent_id
            .and_then(|agent_id| crate::session_data::agent_def(agent_id, cx))
            .is_some_and(|agent| agent.is_assistant());
        let kind = match session_screen_kind(agent_id, model.root.app_id.as_deref(), assistant) {
            SessionScreenKind::App => SessionScreenKind::Session,
            kind => kind,
        };
        let open = if self.focus.is_some() {
            Some(SessionScreenOpen::Session.as_str())
        } else {
            self.opened.as_ref().map(ChatOpened::open)
        };
        let counts = SessionScreenCounts {
            issues: screen_issue_items(model, cx).len() as u64,
            sessions: model.tree.iter().filter(|session| session.id != model.root.id).count() as u64,
            results: crate::results_tab::results_of_root(&self.root_id, cx).len() as u64,
        };
        session_screen(kind.as_str(), true, Some(self.tab.as_str()), open, counts)
    }
}

impl Focusable for SessionChatScreenView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SessionChatScreenView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "SessionChatScreenView").entered();
        // EXP-1346: an Rc handle, never a deep clone of the model.
        let Some(model) = self.model.clone() else {
            let ready = Store::try_global(cx).is_some_and(|store| store.collections().sessions.read(cx).is_ready());
            // The sessions synced, this root is not among them, and the
            // grace passed (deleted, a lost membership, a foreign id).
            if chat_not_found(ready, false, self.opened_at.elapsed()) {
                return v_flex().size_full().px_4().items_center().justify_center().child(
                    crate::controls::empty_state(
                        Icon::new(registry::WORK_FACES),
                        crate::empty_state_copy::CHAT_NOT_FOUND,
                        Some(crate::empty_state_copy::CHAT_NOT_FOUND_DESCRIPTION.into()),
                        cx,
                    )
                    .child(
                        crate::controls::empty_state_actions().child(
                            gpui_component::button::Button::new("chat-not-found-agent")
                                .primary()
                                .web_sm()
                                .label(crate::empty_state_copy::CHAT_NOT_FOUND_ACTION)
                                .on_click(|_, window, cx| {
                                    crate::navigation::navigate(window, cx, Screen::Home);
                                }),
                        ),
                    ),
                );
            }
            // Skeletons, never "Loading…" (feedback-states.json).
            return v_flex()
                .size_full()
                .px(px(HEADER_INSET))
                .pt_6()
                .child(crate::feedback::skeleton_rows("chat-loading", 3, 36., cx));
        };
        let agent_chat = conversation_agent(&model).map(str::to_string);
        self.call_panel.update(cx, |panel, cx| panel.set_agent(agent_chat.clone(), cx));
        // A new issue filed into this chat opens in the pane once its link
        // has synced.
        let pending = cx
            .try_global::<PendingChatChild>()
            .and_then(|pending| pending.0.get(&self.root_id).cloned())
            .filter(|issue_id| model.linked_ids().contains(issue_id));
        if let Some(issue_id) = pending {
            cx.global_mut::<PendingChatChild>().0.remove(&self.root_id);
            self.open_item(ChatOpened::Issue(issue_id), window, cx);
        }
        // #EXP-1662: a way in asked for this chat's plan: it opens beside.
        let pending_plan = cx.try_global::<PendingChatPlan>().and_then(|pending| pending.0.get(&self.root_id).cloned());
        if let Some(message_id) = pending_plan {
            cx.global_mut::<PendingChatPlan>().0.remove(&self.root_id);
            self.open_item(ChatOpened::Plan(message_id), window, cx);
        }
        // #FEED-191 (needs-you.json): a Needs you row asked for this chat.
        let opening = cx.try_global::<PendingChatOpening>().and_then(|pending| pending.0.get(&self.root_id).cloned());
        if let Some(opening) = opening {
            cx.global_mut::<PendingChatOpening>().0.remove(&self.root_id);
            match opening {
                ChatOpening::Card(message_id) => self.select_waiting(&message_id, window, cx),
                ChatOpening::Results => self.set_tab(SessionScreenTab::Results, window, cx),
            }
        }
        // #FEED-191 (plan-page.json): a decision taken on the opened plan
        // landed: it closes, the agent works on in the chat.
        if cx.try_global::<PlanTabDecisions>().is_some_and(|decisions| decisions.landed.contains(&self.root_id)) {
            cx.global_mut::<PlanTabDecisions>().landed.remove(&self.root_id);
            if matches!(self.opened, Some(ChatOpened::Plan(_))) {
                self.close_item(window, cx);
            }
        }
        // EXP-1346: ONE memoised chip resolver per frame for every text.
        self.text = ChatText::memoized(model.root.team_id.as_deref(), Some(self.chip_cache.clone()))
            .answering(self.root_view.read(cx).inner().read(cx).may_message());
        // EXP-1323: what is on screen is read: the chat (the whole tree's
        // agent replies) and the session open in the pane (its own).
        crate::session_data::mark_read_if_unread(&self.root_id, true, cx);
        if let Some(focus) = self.focus_id().map(str::to_string) {
            crate::session_data::mark_read_if_unread(&focus, false, cx);
        }
        let screen = self.screen_of(&model, cx);
        let header = self.render_header(&model, &screen, window, cx);
        // #EXP-1732 THE Chat tab (every layout, every kind): the ROOT's own
        // session view: its raw work, its cards inline, THE Dock and THE
        // composer posting to the root. #EXP-1736: its origin = the sender
        // of the prompt that started it.
        self.take_reveal(cx);
        self.take_waiting_reveal(cx);
        let inner = self.root_view.read(cx).inner().clone();
        let origin = model.view.origin.clone();
        if inner.read(cx).transcript_origin_xref() != origin.as_ref() {
            inner.update(cx, |inner, cx| inner.set_transcript_origin(origin, cx));
        }
        // #EXP-1545: a Live child's Queue rides the root's Dock.
        let children = live_children(&model.view, &model.root, cx);
        self.sync_queue_feeds(children, window, cx);
        let queue_from = self.chat_queue_from(&model, cx);
        inner.update(cx, |inner, _| inner.set_queue_from(queue_from));
        let chat = session_pane(&self.root_view, cx);
        // EXP-1686: the call panel covers an agent chat's body while it runs.
        let chat = match agent_chat {
            Some(_) => div()
                .relative()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(chat)
                .child(self.call_panel.clone())
                .into_any_element(),
            None => chat,
        };
        // #EXP-1732 (session-screen.json LAYOUT): wide = the beside rule.
        let body = match screen.layout {
            SessionScreenLayout::Chat => chat,
            SessionScreenLayout::Beside | SessionScreenLayout::Tab | SessionScreenLayout::Item => {
                let pane = self.render_pane(&model, &screen, window, cx);
                screen_beside(chat, pane, cx)
            }
        };
        v_flex()
            .track_focus(&self.focus_handle)
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .child(header)
            .child(body)
    }
}

#[cfg(test)]
#[path = "long_chat_bench.rs"]
mod long_chat_bench;

#[cfg(test)]
mod tests {
    use super::*;

    /// plan-page.json #FEED-191: a tab decision lands only when it succeeds; a
    /// decision from the Conversation never enters.
    #[gpui::test]
    fn a_decision_on_the_plan_tab_lands_only_on_success(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            cx.default_global::<PlanTabDecisions>().asked.insert("m1".into(), "r1".into());
            cx.default_global::<PlanTabDecisions>().asked.insert("m2".into(), "r2".into());
            settle_plan_tab_decision("m1", true, cx);
            settle_plan_tab_decision("m2", false, cx);
            settle_plan_tab_decision("m3", true, cx);
            let decisions = cx.global::<PlanTabDecisions>();
            assert!(decisions.landed.contains("r1"));
            assert!(!decisions.landed.contains("r2"));
            assert_eq!(decisions.landed.len(), 1);
            assert!(decisions.asked.is_empty());
        });
    }

    /// #EXP-1732 (session-screen.json), an agent's chat the whole path: THE
    /// tabs and the gear in its header, the chat full width on Chat; a way in
    /// opens the plan BESIDE the chat, across frames and a decision; its `‹`
    /// closes it; any other tab opens the pane beside the chat too.
    #[gpui::test]
    #[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
    fn an_agent_chats_plan_opens_beside_the_chat(cx: &mut gpui::TestAppContext) {
        use sync::ShapeRow as _;
        fn seed_plan(decision: Option<&str>, cx: &mut App) {
            let message: MessageRow = serde_json::from_value(serde_json::json!({
                "id": "m1",
                "conversation_id": "c1",
                "session_id": "r1",
                "author_kind": "agent",
                "body": "",
                "card": { "kind": "plan", "plan": "# Ship it", "decision": decision },
                "created_at": "2026-11-04T10:00:00.000Z",
            }))
            .unwrap();
            let collections = Store::global(cx).collections().clone();
            collections.messages.update(cx, |collection, cx| {
                collection.seed(message.key(), message);
                cx.notify();
            });
        }
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
            let store = Store::open(cx, None, None);
            cx.set_global(store);
            // The relay view asks for it once the session row exists (#EXP-1412).
            crate::queries::set_own_device_id_for_test("d1", cx);
            let collections = Store::global(cx).collections().clone();
            let root: SessionRow = serde_json::from_value(serde_json::json!({
                "id": "r1",
                "team_id": "t1",
                "root_id": "r1",
                "agent_id": "ag1",
                "status": "idle",
                "shared": true,
                "created_at": "2026-11-04T09:00:00.000Z",
                "updated_at": "2026-11-04T09:00:00.000Z",
            }))
            .unwrap();
            collections.sessions.update(cx, |collection, cx| {
                collection.seed(root.key(), root);
                cx.notify();
            });
            let conversation: domain::rows::ConversationRow = serde_json::from_value(serde_json::json!({
                "id": "c1",
                "session_id": "r1",
                "created_at": "2026-11-04T09:00:00.000Z",
            }))
            .unwrap();
            collections.conversations.update(cx, |collection, cx| {
                collection.seed(conversation.key(), conversation);
                cx.notify();
            });
            seed_plan(None, cx);
        });
        let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
        let beside: &'static str = SESSION_SCREEN.test_ids.beside.as_str();
        let draw = |cx: &mut gpui::VisualTestContext| {
            for _ in 0..3 {
                cx.update(|window, cx| window.draw(cx).clear(cx));
                cx.run_until_parked();
            }
        };
        draw(cx);
        // The chat full width, THE tabs over it; no strip, no plan tab.
        view.read_with(cx, |view, _| assert_eq!((view.tab, view.opened.clone()), (SessionScreenTab::Chat, None)));
        assert!(cx.debug_bounds(SESSION_SCREEN.test_ids.tabs.as_str()).is_some(), "THE Segmented");
        assert!(cx.debug_bounds(beside).is_none(), "Chat alone = the chat full width");
        assert!(cx.debug_bounds(domain::plan_page::PLAN_PAGE.test_ids.tab.as_str()).is_none(), "no plan tab");

        // A way in opens the plan beside the chat, and it stays.
        cx.update(|_, cx| request_chat_plan("r1", "m1", cx));
        cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
        draw(cx);
        view.read_with(cx, |view, _| assert_eq!(view.opened, Some(ChatOpened::Plan("m1".into()))));
        assert!(cx.debug_bounds(beside).is_some());

        // Answering keeps it open.
        cx.update(|_, cx| seed_plan(Some("approved"), cx));
        draw(cx);
        view.read_with(cx, |view, _| assert_eq!(view.opened, Some(ChatOpened::Plan("m1".into()))));

        // Its back closes it: the chat full width again.
        cx.update(|window, cx| view.update(cx, |view, cx| view.close_item(window, cx)));
        draw(cx);
        view.read_with(cx, |view, _| assert_eq!(view.opened, None));
        assert!(cx.debug_bounds(beside).is_none());

        // Results (an agent's chat has one now) opens beside the chat.
        cx.update(|window, cx| view.update(cx, |view, cx| view.set_tab(SessionScreenTab::Results, window, cx)));
        draw(cx);
        view.read_with(cx, |view, _| assert_eq!(view.tab, SessionScreenTab::Results));
        assert!(cx.debug_bounds(beside).is_some());
    }

    /// One agent chat ("r1") holding one agent message `body`.
    fn seed_agent_chat(body: &'static str, cx: &mut gpui::TestAppContext) {
        use sync::ShapeRow as _;
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
            let store = Store::open(cx, None, None);
            cx.set_global(store);
            crate::queries::set_own_device_id_for_test("d1", cx);
            let collections = Store::global(cx).collections().clone();
            let root: SessionRow = serde_json::from_value(serde_json::json!({
                "id": "r1",
                "team_id": "t1",
                "root_id": "r1",
                "agent_id": "ag1",
                "status": "idle",
                "shared": true,
                "created_at": "2026-11-04T09:00:00.000Z",
                "updated_at": "2026-11-04T09:00:00.000Z",
            }))
            .unwrap();
            collections.sessions.update(cx, |collection, cx| {
                collection.seed(root.key(), root);
                cx.notify();
            });
            let conversation: domain::rows::ConversationRow = serde_json::from_value(serde_json::json!({
                "id": "c1",
                "session_id": "r1",
                "created_at": "2026-11-04T09:00:00.000Z",
            }))
            .unwrap();
            collections.conversations.update(cx, |collection, cx| {
                collection.seed(conversation.key(), conversation);
                cx.notify();
            });
            let message: MessageRow = serde_json::from_value(serde_json::json!({
                "id": "m1",
                "conversation_id": "c1",
                "session_id": "r1",
                "author_kind": "agent",
                "body": body,
                "created_at": "2026-11-04T10:00:00.000Z",
            }))
            .unwrap();
            collections.messages.update(cx, |collection, cx| {
                collection.seed(message.key(), message);
                cx.notify();
            });
        });
    }

    /// #EXP-1732: the agent's reply as the root's session view streams it
    /// (the Chat tab IS that view).
    fn stream_reply(view: &Entity<SessionChatScreenView>, body: &'static str, cx: &mut gpui::VisualTestContext) {
        cx.update(|_, cx| {
            let inner = view.read(cx).root_view.read(cx).inner().clone();
            inner.update(cx, |inner, cx| {
                inner.apply_events_for_test(
                    vec![
                        steer::ViewerEvent::Phase(steer::ViewerPhase::Live),
                        steer::ViewerEvent::Activity(1, steer::ActivityEvent::narration(body)),
                    ],
                    cx,
                )
            });
        });
    }

    /// #EDE-65 lane O: the Chat tab's transcript document (the root's
    /// session view's).
    fn chat_document(view: &Entity<SessionChatScreenView>, cx: &mut gpui::VisualTestContext) -> Entity<eui_gpui::view::SurfaceView> {
        cx.update(|_, cx| {
            let inner = view.read(cx).root_view.read(cx).inner().clone();
            let document = inner.read(cx).chat_document().cloned();
            document.expect("the Chat tab mounts its transcript document")
        })
    }

    /// The document's painted Markdowns `(y, text, id)`, top to bottom (a
    /// bubble's text is its MessageBubble's Markdown part).
    fn painted_markdowns(document: &Entity<eui_gpui::view::SurfaceView>, cx: &mut gpui::VisualTestContext) -> Vec<(f32, String, String)> {
        cx.update(|_, cx| {
            let document = document.read(cx);
            let mut texts: Vec<(f32, String, String)> = document
                .placed_nodes()
                .iter()
                .filter(|node| node.component == "Markdown" && !node.hidden && !node.removed)
                .filter_map(|node| {
                    let bounds = document.painted_bounds(&node.id)?;
                    Some((f32::from(bounds.origin.y), node.props.get("text")?.as_str()?.to_string(), node.id.clone()))
                })
                .collect();
            texts.sort_by(|a, b| a.0.total_cmp(&b.0));
            texts
        })
    }

    /// The painted bounds of the bubble text `text` in the Chat tab.
    fn bubble_text_bounds(view: &Entity<SessionChatScreenView>, text: &str, cx: &mut gpui::VisualTestContext) -> gpui::Bounds<gpui::Pixels> {
        let document = chat_document(view, cx);
        let id = painted_markdowns(&document, cx).into_iter().find(|(_, painted, _)| painted == text).map(|(_, _, id)| id).expect("the bubble's text paints");
        cx.update(|_, cx| document.read(cx).painted_bounds(&id)).expect("painted")
    }

    /// #EDE-65 lane O: the Chat tab's transcript IS the shared chat
    /// document: a person's prompt and the agent's narration are its
    /// bubbles, a tool row is the view's own row painted by the host (a
    /// host row, measured at its own height), in the feed's order.
    #[gpui::test]
    #[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
    fn the_chat_tab_draws_its_transcript_as_the_chat_document(cx: &mut gpui::TestAppContext) {
        seed_agent_chat("Hey", cx);
        let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
        cx.update(|_, cx| {
            let inner = view.read(cx).root_view.read(cx).inner().clone();
            inner.update(cx, |inner, cx| {
                inner.apply_events_for_test(
                    vec![
                        steer::ViewerEvent::Phase(steer::ViewerPhase::Live),
                        steer::ViewerEvent::Activity(1, steer::ActivityEvent::user_message("Ship the fix")),
                        steer::ViewerEvent::Activity(2, steer::ActivityEvent::tool("Bash", Some("cargo test".into()))),
                        steer::ViewerEvent::Activity(3, steer::ActivityEvent::narration("All green.")),
                    ],
                    cx,
                )
            });
        });
        for _ in 0..4 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.run_until_parked();
        }
        let document = chat_document(&view, cx);
        let markdowns = painted_markdowns(&document, cx);
        let y_of = |text: &str| markdowns.iter().find(|(_, painted, _)| painted == text).map(|(y, _, _)| *y);
        let prompt = y_of("Ship the fix").expect("the prompt is the document's bubble");
        let reply = y_of("All green.").expect("the narration is the document's bubble");
        assert!(prompt < reply, "the feed's order: {markdowns:?}");
        // The tool row: a host row the view painted, between the two.
        let painted = cx.update(|_, cx| view.read(cx).root_view.read(cx).inner().read(cx).chat_transcript().painted());
        let tool = painted
            .iter()
            .filter(|id| id.starts_with("chat_row_host.r"))
            .filter_map(|id| {
                let bounds = cx.update(|_, cx| document.read(cx).painted_bounds(id))?;
                let y = f32::from(bounds.origin.y);
                (y > prompt && y < reply).then(|| (id.clone(), bounds))
            })
            .next();
        let (id, bounds) = tool.expect("the tool row is painted by the host between the bubbles");
        assert!(f32::from(bounds.size.height) > 0., "the host row has its height");
        let measured = cx.update(|_, cx| view.read(cx).root_view.read(cx).inner().read(cx).chat_transcript().host_height(&id));
        assert!(measured.is_some_and(|height| height > 0.), "the host measured its row: {measured:?}");
        // The row's own content PAINTED inside the frame the document gave it
        // (a host row whose element failed or was clipped away left its
        // space blank).
        let key = id.strip_prefix("chat_row_host.").expect("a host id");
        let selector: &'static str = Box::leak(format!("chat-host-row.{key}").into_boxed_str());
        let content = cx.debug_bounds(selector).expect("the host row's content paints");
        assert!(f32::from(content.size.height) > 0., "{content:?}");
        assert!((f32::from(content.origin.y) - f32::from(bounds.origin.y)).abs() < 1., "it paints in its frame: {content:?} vs {bounds:?}");
        assert!(f32::from(content.size.height) <= f32::from(bounds.size.height) + 0.5, "inside its frame: {content:?} vs {bounds:?}");
    }

    /// #EDE-65 lane O (transcript-grouping.json `maxWidth.wide`): an `in`
    /// bubble's cap is 62 % of the column RIGHT of the avatar column (32 +
    /// 8 px), as the IDE's own bubble was, not of the whole row.
    #[gpui::test]
    fn an_in_bubbles_cap_is_of_the_column_right_of_the_avatar(cx: &mut gpui::TestAppContext) {
        seed_agent_chat("Hey", cx);
        let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
        let long = "A reply long enough to wrap: ".repeat(12);
        cx.update(|_, cx| {
            let inner = view.read(cx).root_view.read(cx).inner().clone();
            inner.update(cx, |inner, cx| {
                inner.apply_events_for_test(
                    vec![steer::ViewerEvent::Phase(steer::ViewerPhase::Live), steer::ViewerEvent::Activity(1, steer::ActivityEvent::narration(&long))],
                    cx,
                )
            });
        });
        for _ in 0..4 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.run_until_parked();
        }
        let document = chat_document(&view, cx);
        let (row, bubble) = cx.update(|_, cx| {
            let document = document.read(cx);
            let id = |prefix: &str| {
                document.placed_nodes().iter().find(|n| n.id.starts_with(prefix) && !n.hidden && !n.removed).map(|n| n.id.clone()).expect(prefix)
            };
            let row = document.painted_bounds(&id("chat_row_agent.")).expect("the row paints");
            let bubble = document.painted_bounds(&id("chat_agent_bubble_in.bubble.")).expect("the bubble paints");
            (row, bubble)
        });
        let sizes = transcript_sizes();
        let column = f32::from(row.size.width) - sizes.avatar - sizes.avatar_gap;
        let cap = column * sizes.max_width.wide;
        assert!((f32::from(bubble.size.width) - cap).abs() < 1., "bubble {} vs 62 % of {column} = {cap}", f32::from(bubble.size.width));
        assert!((f32::from(bubble.origin.x) - f32::from(row.origin.x) - sizes.avatar - sizes.avatar_gap).abs() < 1., "right of the avatar column");
    }

    /// #VAPP-258 (#EDE-65 lane O): the live reply (the run's last item, a
    /// narration, while live) is the one bubble whose Markdown streams (its
    /// unfinished tail held back), as `render_narration` did; once a row
    /// follows it, or the run settles, it reads whole.
    #[gpui::test]
    fn the_live_reply_streams_in_the_chat_document(cx: &mut gpui::TestAppContext) {
        seed_agent_chat("Hey", cx);
        let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
        let apply = |events: Vec<steer::ViewerEvent>, cx: &mut gpui::VisualTestContext| {
            cx.update(|_, cx| {
                let inner = view.read(cx).root_view.read(cx).inner().clone();
                inner.update(cx, |inner, cx| inner.apply_events_for_test(events, cx));
            });
            for _ in 0..3 {
                cx.update(|window, cx| window.draw(cx).clear(cx));
                cx.run_until_parked();
            }
        };
        apply(
            vec![
                steer::ViewerEvent::Phase(steer::ViewerPhase::Live),
                steer::ViewerEvent::Activity(1, steer::ActivityEvent::narration("Earlier **bo")),
                steer::ViewerEvent::Activity(2, steer::ActivityEvent::tool("Bash", Some("ls".into()))),
                steer::ViewerEvent::Activity(3, steer::ActivityEvent::narration("Say **bo")),
            ],
            cx,
        );
        let document = chat_document(&view, cx);
        let streams = |text: &str, cx: &mut gpui::VisualTestContext| {
            assert!(painted_markdowns(&document, cx).iter().any(|(_, painted, _)| painted == text), "{text:?} paints");
            cx.update(|_, cx| document.read(cx).host_plugin().markdown_options_for(text, cx).streaming)
        };
        assert!(streams("Say **bo", cx), "the live reply streams");
        assert!(!streams("Earlier **bo", cx), "an earlier reply reads whole");
        apply(vec![steer::ViewerEvent::Activity(4, steer::ActivityEvent::tool("Bash", Some("pwd".into())))], cx);
        assert!(!streams("Say **bo", cx), "a row after it: it reads whole");
    }

    /// The root's session view under the Chat tab.
    fn chat_inner(view: &Entity<SessionChatScreenView>, cx: &mut gpui::VisualTestContext) -> Entity<crate::steer_viewer::SteerSessionView> {
        cx.update(|_, cx| view.read(cx).root_view.read(cx).inner().clone())
    }

    fn apply_events(view: &Entity<SessionChatScreenView>, events: Vec<steer::ViewerEvent>, cx: &mut gpui::VisualTestContext) {
        let inner = chat_inner(view, cx);
        inner.update(cx, |inner, cx| inner.apply_events_for_test(events, cx));
    }

    /// ONE frame, nothing deferred run after it.
    fn one_frame(cx: &mut gpui::VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn settle_frames(cx: &mut gpui::VisualTestContext) {
        for _ in 0..4 {
            one_frame(cx);
            cx.run_until_parked();
        }
    }

    /// The document's transcript list `(offset, viewport, content)`.
    fn chat_extent(view: &Entity<SessionChatScreenView>, cx: &mut gpui::VisualTestContext) -> (f32, f32, f32) {
        let document = chat_document(view, cx);
        cx.update(|_, cx| document.read(cx).scroll_extent(crate::chat_tab_transcript::SCROLL)).expect("the transcript laid out")
    }

    fn chat_at_bottom(view: &Entity<SessionChatScreenView>, cx: &mut gpui::VisualTestContext) -> bool {
        let (offset, viewport, content) = chat_extent(view, cx);
        (offset - (content - viewport).max(0.)).abs() < 0.5
    }

    fn chat_follows(view: &Entity<SessionChatScreenView>, cx: &mut gpui::VisualTestContext) -> bool {
        let inner = chat_inner(view, cx);
        cx.update(|_, cx| inner.read(cx).chat_transcript().following())
    }

    /// A wheel over the transcript (`dy` > 0 = up), dispatched without
    /// drawing a frame.
    #[allow(deprecated)]
    fn wheel_chat(view: &Entity<SessionChatScreenView>, dy: f32, cx: &mut gpui::VisualTestContext) {
        let document = chat_document(view, cx);
        let list = cx.update(|_, cx| document.read(cx).painted_bounds(crate::chat_tab_transcript::SCROLL)).expect("the transcript painted");
        let position = list.center();
        let event = gpui::ScrollWheelEvent { position, delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(dy))), ..Default::default() };
        cx.update(|window, cx| {
            window.dispatch_event(gpui::InputEvent::to_platform_input(event), cx);
        });
    }

    /// Every host row the document laid out now: its painted id, the
    /// height it was laid out at and the height the host measured.
    fn host_rows_now(view: &Entity<SessionChatScreenView>, cx: &mut gpui::VisualTestContext) -> Vec<(String, f32, Option<f32>)> {
        let document = chat_document(view, cx);
        let inner = chat_inner(view, cx);
        cx.update(|_, cx| {
            let document = document.read(cx);
            let transcript = inner.read(cx).chat_transcript();
            document
                .placed_nodes()
                .iter()
                .filter(|node| node.component == "Embed" && !node.removed && !node.hidden && node.id.starts_with("chat_row_host.r"))
                .filter_map(|node| Some((node.id.clone(), document.frame(node.index)?.h, transcript.host_height(&node.id))))
                .collect()
        })
    }

    /// #EDE-65 lane O: a host row (a tool row) is measured where the
    /// document is about to paint it: from the first frame it shows, and a
    /// row arriving later on the frame it arrives, it is laid out at its
    /// measured height (never at the estimate, then a jump).
    #[gpui::test]
    fn a_host_row_paints_at_its_measured_height_from_its_first_frame(cx: &mut gpui::TestAppContext) {
        seed_agent_chat("Hey", cx);
        let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
        let mut events = vec![steer::ViewerEvent::Phase(steer::ViewerPhase::Live)];
        for n in 0..6u64 {
            events.push(steer::ViewerEvent::Activity(2 * n + 1, steer::ActivityEvent::tool("Bash", Some(format!("cargo test -p crate{n}")))));
            events.push(steer::ViewerEvent::Activity(2 * n + 2, steer::ActivityEvent::narration("Checked.")));
        }
        apply_events(&view, events, cx);
        let mut seen = 0;
        for frame in 0..4 {
            one_frame(cx);
            for (id, laid_out, measured) in host_rows_now(&view, cx) {
                seen += 1;
                assert_eq!(Some(laid_out), measured, "frame {frame}: {id} laid out at its measured height");
                assert!(laid_out > 0.);
            }
            cx.run_until_parked();
        }
        assert!(seen > 0, "the tool rows laid out");
        // Every frame drawn so far (the updates draw some too): no row ever
        // painted in a frame other than its measured height.
        let inner = chat_inner(&view, cx);
        let misfits = cx.update(|_, cx| inner.read(cx).chat_transcript().misfits());
        assert!(misfits.is_empty(), "rows painted at an unmeasured height: {misfits:?}");
        apply_events(&view, vec![steer::ViewerEvent::Activity(20, steer::ActivityEvent::tool("Read", Some("src/lib.rs".into())))], cx);
        one_frame(cx);
        let rows = host_rows_now(&view, cx);
        assert!(rows.len() > 6, "the new row laid out: {rows:?}");
        for (id, laid_out, measured) in rows {
            assert_eq!(Some(laid_out), measured, "{id} on the frame it arrived");
        }
        let misfits = cx.update(|_, cx| inner.read(cx).chat_transcript().misfits());
        assert!(misfits.is_empty(), "the new row painted at an unmeasured height: {misfits:?}");
    }

    /// #LOGS-10: the conversation cards whose calls sit above the loaded
    /// rows (the head) are rows of the document's window, one each: only the
    /// ones near the viewport lay out, never all of them inside the first
    /// row; once settled, each lays out at its measured height (a card's
    /// surface builds a frame after it first shows, as an anchored one's).
    #[gpui::test]
    fn head_cards_are_windowed_rows_above_the_first(cx: &mut gpui::TestAppContext) {
        use sync::ShapeRow as _;
        const CARDS: usize = 120;
        seed_agent_chat("Hey", cx);
        cx.update(|cx| {
            let collections = Store::global(cx).collections().clone();
            collections.messages.update(cx, |collection, cx| {
                for n in 0..CARDS {
                    let message: MessageRow = serde_json::from_value(serde_json::json!({
                        "id": format!("card{n:02}"), "conversation_id": "c1", "session_id": "r1", "author_kind": "agent", "body": "",
                        "created_at": format!("2026-11-04T{:02}:{:02}:30.000Z", 10 + n / 60, n % 60),
                        "card": {"kind": "surface", "title": "Report", "surface": {"version": "1.0",
                            "root": {"id": "root", "component": "Stack", "children": [{"id": "n", "component": "StatTile", "props": {"label": "New", "value": format!("{n}")}}]}}},
                    }))
                    .unwrap();
                    collection.seed(message.key(), message);
                }
                cx.notify();
            });
        });
        let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
        let mut events = vec![steer::ViewerEvent::Phase(steer::ViewerPhase::Live)];
        for n in 0..4u64 {
            events.push(steer::ViewerEvent::Activity(2 * n + 1, steer::ActivityEvent::user_message("Ship the fix")));
            events.push(steer::ViewerEvent::Activity(2 * n + 2, steer::ActivityEvent::narration("All green.")));
        }
        apply_events(&view, events, cx);
        settle_frames(cx);
        let card_rows = |cx: &mut gpui::VisualTestContext| {
            let document = chat_document(&view, cx);
            cx.update(|_, cx| {
                let document = document.read(cx);
                document
                    .placed_nodes()
                    .iter()
                    .filter(|node| node.component == "Embed" && !node.removed && !node.hidden && node.id.starts_with("chat_row_host.card-"))
                    .filter_map(|node| Some((node.id.clone(), document.frame(node.index)?)))
                    .collect::<Vec<_>>()
            })
        };
        // At the newest row, the head cards above the window are not laid out.
        assert!(card_rows(cx).len() < CARDS, "{} of {CARDS} head cards laid out", card_rows(cx).len());
        // Up to the top: they lay out as rows of their own, a window of them.
        for _ in 0..80 {
            wheel_chat(&view, 2_000., cx);
            settle_frames(cx);
        }
        let rows = card_rows(cx);
        assert!(!rows.is_empty() && rows.len() < CARDS, "a window of the head cards: {}", rows.len());
        let inner = chat_inner(&view, cx);
        for (id, frame) in &rows {
            let measured = cx.update(|_, cx| inner.read(cx).chat_transcript().host_height(id));
            assert_eq!(measured, Some(frame.h), "{id} at its measured height");
        }
    }

    /// #EDE-65 lane O FOLLOW (the old list's `FollowMode::Tail`): while the
    /// reader is at the bottom a new row keeps the transcript at its bottom
    /// in the SAME frame; a wheel up stops following (a new row leaves the
    /// reader where they are); back at the bottom it follows again.
    #[gpui::test]
    fn the_chat_tab_follows_its_newest_row_in_the_frame_it_arrives(cx: &mut gpui::TestAppContext) {
        seed_agent_chat("Hey", cx);
        let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
        let line = "A line of the agent's narration that wraps once or twice in the reading column of the chat, so rows differ.";
        let mut events = vec![steer::ViewerEvent::Phase(steer::ViewerPhase::Live)];
        for n in 0..40u64 {
            events.push(steer::ViewerEvent::Activity(2 * n + 1, steer::ActivityEvent::tool("Bash", Some(format!("step {n}")))));
            events.push(steer::ViewerEvent::Activity(2 * n + 2, steer::ActivityEvent::narration(&line.repeat(1 + (n % 3) as usize))));
        }
        apply_events(&view, events, cx);
        settle_frames(cx);
        assert!(chat_at_bottom(&view, cx), "it opens at the bottom: {:?}", chat_extent(&view, cx));
        let mut seq = 100;
        let mut arrive = |kind: u64, view: &Entity<SessionChatScreenView>, cx: &mut gpui::VisualTestContext| {
            seq += 1;
            let event = if kind % 2 == 0 { steer::ActivityEvent::tool("Bash", Some(format!("more {seq}"))) } else { steer::ActivityEvent::narration(&line.repeat(3)) };
            apply_events(view, vec![steer::ViewerEvent::Activity(seq, event)], cx);
        };
        // (The update that lands a row draws its frame: checked right after
        // it, and after one more.)
        for kind in 0..4 {
            arrive(kind, &view, cx);
            assert!(chat_at_bottom(&view, cx), "row {kind}: at the bottom on the frame it arrived: {:?}", chat_extent(&view, cx));
            one_frame(cx);
            assert!(chat_at_bottom(&view, cx), "row {kind}: still at the bottom: {:?}", chat_extent(&view, cx));
            cx.run_until_parked();
        }
        wheel_chat(&view, 300., cx);
        settle_frames(cx);
        assert!(!chat_follows(&view, cx), "a wheel up stops following");
        // The reader's view stays put (the estimated rows far above may
        // move the offset; what is on screen does not move).
        let document = chat_document(&view, cx);
        let list_top = cx.update(|_, cx| document.read(cx).painted_bounds(crate::chat_tab_transcript::SCROLL)).expect("painted").top();
        let (y, _, id) = painted_markdowns(&document, cx).into_iter().find(|(y, _, _)| *y >= f32::from(list_top)).expect("a bubble on screen");
        arrive(1, &view, cx);
        for _ in 0..2 {
            let now = painted_markdowns(&document, cx).into_iter().find(|(_, _, painted)| *painted == id).map(|(y, _, _)| y);
            // Within the keep engine's own half-pixel hold (`keep.rs` `hold`:
            // `<= 0.5` counts as in place; #EXP-1782's transcript ends at the
            // Dock band's top, so a re-measured row above can land on it).
            assert!(now.is_some_and(|now| (now - y).abs() <= 0.5), "a reader above stays put: {y} -> {now:?}");
            one_frame(cx);
        }
        assert!(!chat_at_bottom(&view, cx), "not pulled to the bottom");
        cx.run_until_parked();
        wheel_chat(&view, -1_000_000., cx);
        settle_frames(cx);
        assert!(chat_follows(&view, cx), "back at the bottom it follows again");
        arrive(0, &view, cx);
        assert!(chat_at_bottom(&view, cx), "following again: {:?}", chat_extent(&view, cx));
        one_frame(cx);
        assert!(chat_at_bottom(&view, cx), "following again: {:?}", chat_extent(&view, cx));
    }

    /// #EXP-1782: the transcript ends at the Dock band's top edge; a reader
    /// scrolled up while FIFTY rows land below stays within the keep
    /// engine's half pixel of where they were: the hold never accumulates.
    #[gpui::test]
    fn a_reader_above_does_not_drift_while_fifty_rows_land(cx: &mut gpui::TestAppContext) {
        seed_agent_chat("Hey", cx);
        let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
        let line = "A line of the agent's narration that wraps once or twice in the reading column of the chat, so rows differ.";
        let mut events = vec![steer::ViewerEvent::Phase(steer::ViewerPhase::Live)];
        for n in 0..40u64 {
            events.push(steer::ViewerEvent::Activity(2 * n + 1, steer::ActivityEvent::tool("Bash", Some(format!("step {n}")))));
            events.push(steer::ViewerEvent::Activity(2 * n + 2, steer::ActivityEvent::narration(&line.repeat(1 + (n % 3) as usize))));
        }
        apply_events(&view, events, cx);
        settle_frames(cx);
        wheel_chat(&view, 300., cx);
        settle_frames(cx);
        assert!(!chat_follows(&view, cx), "a wheel up stops following");
        let document = chat_document(&view, cx);
        let list_top = cx.update(|_, cx| document.read(cx).painted_bounds(crate::chat_tab_transcript::SCROLL)).expect("painted").top();
        let (y, _, id) = painted_markdowns(&document, cx).into_iter().find(|(y, _, _)| *y >= f32::from(list_top)).expect("a bubble on screen");
        for seq in 100..150u64 {
            let event = if seq % 2 == 0 { steer::ActivityEvent::tool("Bash", Some(format!("more {seq}"))) } else { steer::ActivityEvent::narration(&line.repeat(1 + (seq % 3) as usize)) };
            apply_events(&view, vec![steer::ViewerEvent::Activity(seq, event)], cx);
            one_frame(cx);
            let now = painted_markdowns(&document, cx).into_iter().find(|(_, _, painted)| *painted == id).map(|(y, _, _)| y);
            assert!(now.is_some_and(|now| (now - y).abs() <= 0.5), "row {seq}: a reader above stays put: {y} -> {now:?}");
        }
        settle_frames(cx);
        let now = painted_markdowns(&document, cx).into_iter().find(|(_, _, painted)| *painted == id).map(|(y, _, _)| y);
        assert!(now.is_some_and(|now| (now - y).abs() <= 0.5), "after fifty rows: {y} -> {now:?}");
        assert!(!chat_at_bottom(&view, cx), "not pulled to the bottom");
    }

    /// #EDE-65 lane O PAGING: the reader scrolled to the top of the window
    /// (the paging row painted): the older rows load ABOVE them and the row
    /// they were looking at stays exactly where it was on screen.
    #[gpui::test]
    fn older_rows_loading_keep_the_readers_row_in_place(cx: &mut gpui::TestAppContext) {
        seed_agent_chat("Hey", cx);
        let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
        let total = domain::contract::STEER_FEED_WINDOW as u64 + 120;
        // Narration and tool rows alternating (bubbles and host rows).
        let events: Vec<steer::ViewerEvent> = (0..total)
            .map(|n| {
                let event = if n % 2 == 0 {
                    steer::ActivityEvent::tool("Bash", Some(format!("step {n}")))
                } else if n % 3 == 0 {
                    steer::ActivityEvent::narration(&format!("n{n} {}", "and a longer line that wraps in the column ".repeat(4)))
                } else {
                    steer::ActivityEvent::narration(&format!("n{n}"))
                };
                steer::ViewerEvent::Activity(n + 1, event)
            })
            .collect();
        apply_events(&view, events, cx);
        settle_frames(cx);
        let document = chat_document(&view, cx);
        let number = |text: &str| text.strip_prefix('n').and_then(|rest| rest.split(' ').next()).and_then(|n| n.parse::<u64>().ok());
        // To the top: the paging row paints and the older rows load (the
        // update that dispatches the wheel draws those frames).
        wheel_chat(&view, 10_000_000., cx);
        settle_frames(cx);
        // The reader was at the top: the window's first row right under the
        // paging row (8 px). The older rows came in ABOVE it, and it is
        // exactly where it was.
        let list_top = f32::from(cx.update(|_, cx| document.read(cx).painted_bounds(crate::chat_tab_transcript::SCROLL)).expect("painted").top());
        let rows: Vec<f32> = cx.update(|_, cx| {
            let document = document.read(cx);
            let mut ys: Vec<f32> = document
                .placed_nodes()
                .iter()
                .filter(|node| !node.removed && !node.hidden && node.id.starts_with("chat_row.r"))
                .filter_map(|node| document.painted_bounds(&node.id).map(|b| f32::from(b.top())))
                .filter(|y| *y >= list_top - 0.5)
                .collect();
            ys.sort_by(f32::total_cmp);
            ys
        });
        let first = rows.first().copied().expect("rows on screen");
        assert!((first - (list_top + 8.)).abs() < 0.5, "the reader's row stays 8 px under the top: {first} vs {list_top}");
        let first_bubble = painted_markdowns(&document, cx).into_iter().find(|(y, _, _)| *y >= list_top).map(|(_, text, _)| text).expect("a bubble");
        assert_eq!(number(&first_bubble), Some(121), "the old window's first bubble leads the screen: {first_bubble:?}");
        assert!(chat_extent(&view, cx).0 > 100., "the older rows came in above it: {:?}", chat_extent(&view, cx));
    }

    /// EXP-1228: a right press on an agent chat's bubbles opens THE
    /// transcript menu (Copy): the Chat tab is the root's session view.
    #[gpui::test]
    fn a_right_press_on_an_agent_chat_opens_the_transcript_menu(cx: &mut gpui::TestAppContext) {
        seed_agent_chat("Hey, what's up?", cx);
        let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
        stream_reply(&view, "Hey, what's up?", cx);
        for _ in 0..3 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.run_until_parked();
        }
        let bounds = bubble_text_bounds(&view, "Hey, what's up?", cx);
        let opens = || crate::steer_viewer::TRANSCRIPT_MENU_OPENS.with(|opens| opens.get());
        let before = opens();
        cx.simulate_mouse_down(
            bounds.origin + gpui::point(px(4.), px(4.)),
            gpui::MouseButton::Right,
            gpui::Modifiers::default(),
        );
        assert_eq!(opens(), before + 1, "the right press opens the transcript menu");
    }

    /// EXP-1739: real chat text (bold, code spans, paragraphs, a list) swept
    /// with the pointer in the chat screen (the root's session view), under the window's selection
    /// layer and the shell's cached dock: a right press hands the selection
    /// to Copy, a code span as its code (it paints over a placeholder of
    /// non-breaking spaces, which copied as blank: Copy stayed disabled).
    /// #EXP-1732: the session view's bubble wraps at its own width (a code
    /// span never breaks: it moves to the next line whole), so the sweeps
    /// find each line by probing instead of fixed offsets.
    #[gpui::test]
    fn a_right_press_on_selected_chat_text_enables_copy(cx: &mut gpui::TestAppContext) {
        use gpui::{Modifiers, MouseButton, Pixels, Render, StyleRefinement, VisualTestContext};

        /// The shell's shape: the dock cached under the Root's selection layer.
        struct Shell(Entity<SessionChatScreenView>);
        impl Render for Shell {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                let _span = tracing::trace_span!("render", view = "Shell").entered();
                div()
                    .size_full()
                    .child(gpui_base::TextSelectionLayer)
                    .child(gpui::AnyView::from(self.0.clone()).cached(StyleRefinement::default().size_full()))
            }
        }
        const CODE: &str = "curl -fsSL https://exponential.at/desktop/setup.sh | sudo sh";
        const REPLY: &str = "**Package repos:** yes. `pkg.exponential.at` is a small server.\n\n`curl -fsSL https://exponential.at/desktop/setup.sh | sudo sh` installs it.\n\n- **Phone writes:** live test still running.";
        seed_agent_chat(REPLY, cx);
        let (shell, cx) = cx.add_window_view(|window, cx| Shell(cx.new(|cx| SessionChatScreenView::new("r1".to_string(), window, cx))));
        let cx: &mut VisualTestContext = cx;
        let view = shell.read_with(cx, |shell, _| shell.0.clone());
        stream_reply(&view, REPLY, cx);
        for _ in 0..3 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
        // #EXP-1736: the bubble sits under its sender's avatar + name line, so
        // the sweeps measure from the bubble's own text block.
        let body = bubble_text_bounds(&view, REPLY, cx);
        let text = |x: f32, y: f32| body.origin + gpui::point(px(x), px(y));
        // Right-click Copy's text after sweeping `from` → `to` ("" = disabled).
        let copy_after_sweep = |from: gpui::Point<Pixels>, to: gpui::Point<Pixels>, cx: &mut VisualTestContext| {
            let off = gpui::point(px(4.), px(4.));
            cx.simulate_mouse_down(off, MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_up(off, MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
            cx.run_until_parked();
            cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
            cx.run_until_parked();
            cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
            cx.run_until_parked();
            cx.simulate_mouse_down(to, MouseButton::Right, Modifiers::default());
            cx.simulate_mouse_up(to, MouseButton::Right, Modifiers::default());
            cx.run_until_parked();
            crate::steer_viewer::TRANSCRIPT_MENU_TEXT.with(|text| text.borrow().clone())
        };
        // The bubble's lines, probed: a sweep across each band of the text
        // block (past its right edge = to the line's end).
        let first = 10.;
        let prose = copy_after_sweep(text(0., first), text(144., first), cx);
        assert!(prose.starts_with("Package"), "prose: {prose:?}");
        let line_of = |needle: &str, cx: &mut VisualTestContext| {
            (1..40)
                .map(|band| band as f32 * 8.)
                .find(|&y| {
                    let line = copy_after_sweep(text(0., y), text(1000., y), cx);
                    line.trim_start_matches(|c: char| !c.is_alphanumeric()).starts_with(needle)
                })
                .unwrap_or_else(|| panic!("a line starts with {needle:?}"))
        };
        let second = line_of("curl", cx);
        // A sweep INSIDE the code span (it leads its line) copies its code.
        let code = copy_after_sweep(text(20., second), text(120., second), cx);
        assert_eq!(code, CODE, "a code span copies its code");
        let all = copy_after_sweep(text(0., first), text(f32::from(body.size.width) - 2., f32::from(body.size.height) - 8.), cx);
        for part in ["Package repos:", "pkg.exponential.at is a small server.", "curl -fsSL https://exponential.at/desktop/setup.sh | sudo sh installs it", "Phone writes: live test"] {
            assert!(all.contains(part), "multi-paragraph Copy carries {part:?}: {all:?}");
        }
        assert!(!all.contains('\u{a0}'), "no placeholder spaces: {all:?}");
    }

    /// chats.json THE WAY BACK, the desktop's whole path: an agent's chat, a
    /// card opening a chat it started and one opening
    /// its own child: each lands with a Back to the chat; the same screens
    /// reached another way wear none, nor does a session opened in place.
    #[gpui::test]
    #[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
    fn a_session_opened_from_an_agent_chat_has_a_way_back(cx: &mut gpui::TestAppContext) {
        use sync::ShapeRow as _;
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
            let store = Store::open(cx, None, None);
            cx.set_global(store);
            crate::queries::set_own_device_id_for_test("d1", cx);
            let collections = Store::global(cx).collections().clone();
            let session = |id: &str, root: &str, parent: Option<&str>, agent: Option<&str>| -> SessionRow {
                serde_json::from_value(serde_json::json!({
                    "id": id,
                    "team_id": "t1",
                    "root_id": root,
                    "parent_id": parent,
                    "agent_id": agent,
                    "status": "idle",
                    "shared": true,
                    "created_at": "2026-11-04T09:00:00.000Z",
                    "updated_at": "2026-11-04T09:00:00.000Z",
                }))
                .unwrap()
            };
            collections.sessions.update(cx, |collection, cx| {
                for row in [
                    session("chat", "chat", None, Some("ag1")),
                    session("chat-child", "chat", Some("chat"), None),
                    session("plain-chat", "plain-chat", None, None),
                    session("plain-chat-child", "plain-chat", Some("plain-chat"), None),
                ] {
                    collection.seed(row.key(), row);
                }
                cx.notify();
            });
            let conversation: domain::rows::ConversationRow = serde_json::from_value(serde_json::json!({
                "id": "c1",
                "session_id": "chat",
                "created_at": "2026-11-04T09:00:00.000Z",
            }))
            .unwrap();
            collections.conversations.update(cx, |collection, cx| {
                collection.seed(conversation.key(), conversation);
                cx.notify();
            });
        });
        const BACK: &str = "chat-back-link";
        let chat = Screen::Agent { agent_id: "ag1".into() };
        let plain = Screen::Chat { session_id: "plain-chat".into() };
        let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("plain-chat".to_string(), window, cx));
        let draw = |cx: &mut gpui::VisualTestContext| {
            for _ in 0..3 {
                cx.update(|window, cx| window.draw(cx).clear(cx));
                cx.run_until_parked();
            }
        };
        let screen_on_show = |cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| crate::navigation::nav_for_window(window, cx).read(cx).screen().cloned())
        };
        let start = chat.clone();
        cx.update(|window, cx| {
            crate::navigation::nav_for_window(window, cx);
            crate::navigation::navigate(window, cx, start);
        });

        // A child of the chat is no in-place tab: its own screen, whose header
        // is the way back to the chat.
        cx.update(|_, cx| {
            assert_eq!(crate::screens::in_place_root("chat-child", cx), None);
            assert_eq!(crate::navigation::session_screen_for("chat-child", cx), Screen::Session { session_id: "chat-child".into() });
        });

        // The rail's chat: no Back.
        let rail = plain.clone();
        cx.update(|window, cx| crate::navigation::navigate(window, cx, rail));
        draw(cx);
        assert!(cx.debug_bounds(BACK).is_none());
        cx.update(|window, cx| crate::navigation::go_back(window, cx));
        assert_eq!(screen_on_show(cx), Some(chat.clone()));

        // The chat's card opens the chat: the Back, which returns.
        cx.update(|window, cx| open_session_card("plain-chat", window, cx));
        assert_eq!(screen_on_show(cx), Some(plain.clone()));
        draw(cx);
        assert!(cx.debug_bounds(BACK).is_some());
        cx.update(|window, cx| assert_eq!(crate::navigation::session_back_screen(window, cx), Some(chat.clone())));
        cx.update(|window, cx| crate::navigation::go_back(window, cx));
        assert_eq!(screen_on_show(cx), Some(chat.clone()));

        // A session of the chat on show opens in place: its strip returns.
        let rail = plain.clone();
        cx.update(|window, cx| crate::navigation::navigate(window, cx, rail));
        cx.update(|window, cx| open_session_card("plain-chat-child", window, cx));
        assert_eq!(screen_on_show(cx), Some(Screen::Session { session_id: "plain-chat-child".into() }));
        cx.update(|window, cx| assert_eq!(crate::navigation::session_back_screen(window, cx), None));
        cx.update(|window, cx| crate::navigation::go_back(window, cx));

        // An opened plan keeps the link's Back and wears its own `‹`.
        cx.update(|window, cx| crate::navigation::go_back(window, cx));
        assert_eq!(screen_on_show(cx), Some(chat.clone()));
        cx.update(|window, cx| open_session_card("plain-chat", window, cx));
        cx.update(|_, cx| {
            let collections = Store::global(cx).collections().clone();
            let conversation: domain::rows::ConversationRow = serde_json::from_value(serde_json::json!({
                "id": "c2",
                "session_id": "plain-chat",
                "created_at": "2026-11-04T09:00:00.000Z",
            }))
            .unwrap();
            collections.conversations.update(cx, |collection, cx| {
                collection.seed(conversation.key(), conversation);
                cx.notify();
            });
            let message: MessageRow = serde_json::from_value(serde_json::json!({
                "id": "m1",
                "conversation_id": "c2",
                "session_id": "plain-chat",
                "author_kind": "agent",
                "body": "",
                "card": { "kind": "plan", "plan": "# Ship it", "decision": null },
                "created_at": "2026-11-04T10:00:00.000Z",
            }))
            .unwrap();
            collections.messages.update(cx, |collection, cx| {
                collection.seed(message.key(), message);
                cx.notify();
            });
        });
        draw(cx);
        cx.update(|window, cx| view.update(cx, |view, cx| view.open_item(ChatOpened::Plan("m1".into()), window, cx)));
        draw(cx);
        assert!(cx.debug_bounds(SESSION_SCREEN.test_ids.beside.as_str()).is_some(), "the plan opens beside");
        assert!(cx.debug_bounds("session-pane-back").is_some(), "the opened plan's back");
        assert!(cx.debug_bounds(BACK).is_some());
        cx.update(|window, cx| view.update(cx, |view, cx| view.close_item(window, cx)));
        draw(cx);
        assert!(cx.debug_bounds("session-pane-back").is_none());
        assert!(cx.debug_bounds(BACK).is_some());
    }

    /// chats.json THE WAY BACK: an agent's chat reached through a link
    /// (a cross-session bubble, #EXP-1736), wears the Back in THE agent header.
    #[gpui::test]
    #[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
    fn an_agent_chat_without_a_strip_wears_the_links_back(cx: &mut gpui::TestAppContext) {
        use sync::ShapeRow as _;
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
            let store = Store::open(cx, None, None);
            cx.set_global(store);
            crate::queries::set_own_device_id_for_test("d1", cx);
            let collections = Store::global(cx).collections().clone();
            let root: SessionRow = serde_json::from_value(serde_json::json!({
                "id": "chat",
                "team_id": "t1",
                "root_id": "chat",
                "agent_id": "ag1",
                "status": "idle",
                "shared": true,
                "created_at": "2026-11-04T09:00:00.000Z",
                "updated_at": "2026-11-04T09:00:00.000Z",
            }))
            .unwrap();
            collections.sessions.update(cx, |collection, cx| {
                collection.seed(root.key(), root);
                cx.notify();
            });
            let conversation: domain::rows::ConversationRow = serde_json::from_value(serde_json::json!({
                "id": "c1",
                "session_id": "chat",
                "created_at": "2026-11-04T09:00:00.000Z",
            }))
            .unwrap();
            collections.conversations.update(cx, |collection, cx| {
                collection.seed(conversation.key(), conversation);
                cx.notify();
            });
        });
        let (_view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("chat".to_string(), window, cx));
        let draw = |cx: &mut gpui::VisualTestContext| {
            for _ in 0..3 {
                cx.update(|window, cx| window.draw(cx).clear(cx));
                cx.run_until_parked();
            }
        };
        draw(cx);
        assert!(cx.debug_bounds("chat-back-link").is_none(), "nothing to return to");
        cx.update(|window, cx| {
            crate::navigation::nav_for_window(window, cx);
            crate::navigation::navigate(window, cx, Screen::Chat { session_id: "other".into() });
            crate::navigation::open_session_link("chat", window, cx);
        });
        draw(cx);
        assert!(cx.debug_bounds("chat-back-link").is_some(), "the link's Back");
    }

    #[test]
    fn the_chat_menu_rows() {
        assert_eq!(chat_menu_rows(None), vec![ChatMenuRow::LinkIssue, ChatMenuRow::NewIssue]);
        assert_eq!(
            chat_menu_rows(Some("EXP-1270")),
            vec![
                ChatMenuRow::LinkIssue,
                ChatMenuRow::NewIssue,
                ChatMenuRow::Separator,
                ChatMenuRow::Unlink("EXP-1270".into()),
            ]
        );
        assert_eq!(unlink_label("EXP-1270"), "Unlink EXP-1270");
    }

    /// Never "not found" while the sync has not settled, the root is there,
    /// or before the grace passed.
    #[test]
    fn chat_not_found_waits_for_the_sync_and_the_grace() {
        let late = MISSING_GRACE + Duration::from_millis(1);
        assert!(!chat_not_found(false, false, late));
        assert!(!chat_not_found(true, true, late));
        assert!(!chat_not_found(true, false, Duration::from_secs(1)));
        assert!(chat_not_found(true, false, late));
    }

    /// EXP-1309: every chat text (messages, plans, questions, Guide
    /// sections) chips BARE `FB-8` tokens, like web `ChatMarkdown`'s
    /// `bareIssueRefs` and the steering feed (EXP-760).
    #[test]
    fn chat_texts_chip_bare_identifiers() {
        assert!(chat_text_resolver("team-1").bare);
    }

    /// EXP-1317: the X lives only on the author's queued row, and its id
    /// (so its tooltip state) changes when the row flips queued → consumed.
    #[test]
    fn the_unqueue_x_dies_with_the_queued_state() {
        assert_eq!(QUEUED_WORD, "queued");
        assert!(unqueue_offered(true, true));
        assert!(!unqueue_offered(false, true), "a consumed row carries no X");
        assert!(!unqueue_offered(true, false), "only the author's");
        assert_ne!(
            unqueue_button_id("chat-unqueue", "m-1", true),
            unqueue_button_id("chat-unqueue", "m-1", false)
        );
        assert_ne!(
            unqueue_button_id("chat-unqueue", "m-1", true),
            unqueue_button_id("chat-unqueue", "m-2", true)
        );
    }

    #[test]
    fn the_copy_never_names_the_old_word() {
        for copy in [
            LINK_ISSUE_LABEL,
            NEW_ISSUE_LABEL,
            SESSION_CHAT.copy.placeholder.as_str(),
            SESSION_CHAT.copy.conversation.as_str(),
        ] {
            assert!(!copy.to_lowercase().contains("thread"), "{copy}");
        }
        assert_eq!(in_chat_label("Offline sync plan"), "In Offline sync plan");
        for option in domain::ask_dock::plan_options() {
            assert!(!option.label.to_lowercase().contains("thread"), "{}", option.label);
        }
    }

    /// #EDE-65 lane O: the Chat tab drawn HEADLESS under the platform's real
    /// text system, one picture (the inner loop for its look). Run with
    /// `EXP_CHAT_SHOTS=/tmp/chat cargo test -p ui -- --ignored chat_tab_picture --nocapture`.
    #[test]
    #[ignore]
    fn chat_tab_picture() {
        use std::sync::Arc;
        use sync::ShapeRow as _;
        let dir = std::env::var("EXP_CHAT_SHOTS").expect("EXP_CHAT_SHOTS = the output folder");
        let mut cx = gpui::HeadlessAppContext::with_platform(
            crate::test_text_system(),
            Arc::new(gpui_component_assets::Assets),
            gpui_platform::current_headless_renderer,
        );
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
            let store = Store::open(cx, None, None);
            cx.set_global(store);
            crate::queries::set_own_device_id_for_test("d1", cx);
            let collections = Store::global(cx).collections().clone();
            let root: SessionRow = serde_json::from_value(serde_json::json!({
                "id": "r1", "team_id": "t1", "root_id": "r1", "agent_id": "ag1", "status": "idle", "shared": true,
                "title": "Triage",
                "created_at": "2026-11-04T09:00:00.000Z", "updated_at": "2026-11-04T09:00:00.000Z",
            }))
            .unwrap();
            collections.sessions.update(cx, |c, cx| {
                c.seed(root.key(), root);
                cx.notify();
            });
            let agent: domain::rows::AgentDefRow = serde_json::from_value(serde_json::json!({
                "id": "ag1", "team_id": "t1", "name": "Triage", "icon": "bug", "color": "#a855f7",
            }))
            .unwrap();
            collections.agents.update(cx, |c, cx| {
                c.seed(agent.key(), agent);
                cx.notify();
            });
            let board: domain::rows::Board = serde_json::from_value(serde_json::json!({"id": "b1", "team_id": "t1", "name": "Product Feedback", "slug": "fb", "prefix": "FB", "sort_order": 0})).unwrap();
            collections.boards.update(cx, |c, cx| {
                c.seed(board.key(), board);
                cx.notify();
            });
            for (id, n, title) in [("i3", 3, "Syntax highlighting ships every language in the editor bundle"), ("i8", 8, "Editor takes seconds to open on older phones"), ("i9", 9, "Screenshots attached to feedback are cut off on Android")] {
                let issue: domain::rows::Issue = serde_json::from_value(serde_json::json!({"id": id, "board_id": "b1", "number": n, "identifier": format!("FB-{n}"), "title": title, "status": "backlog"})).unwrap();
                collections.issues.update(cx, |c, cx| {
                    c.seed(issue.key(), issue);
                    cx.notify();
                });
            }
        });
        let long = vec![
            steer::ViewerEvent::Phase(steer::ViewerPhase::Live),
            steer::ViewerEvent::Activity(1, steer::ActivityEvent::user_message("Check Product Feedback for new widget reports.")),
            steer::ViewerEvent::Activity(2, steer::ActivityEvent::tool("mcp__exponential__exponential_issues_list", Some("Product Feedback · 2 new".into()))),
            steer::ViewerEvent::Activity(3, steer::ActivityEvent::tool("mcp__exponential__exponential_issues_get", Some("FB-8".into()))),
            steer::ViewerEvent::Activity(4, steer::ActivityEvent::tool("mcp__exponential__exponential_issues_get", Some("FB-9".into()))),
            steer::ViewerEvent::Activity(5, steer::ActivityEvent::narration("Two new widget reports on Product Feedback.\nFB-8 is a duplicate of FB-3, merged it. FB-9 needs code, so I started a chat:")),
            steer::ViewerEvent::Activity(6, steer::ActivityEvent::tool("mcp__exponential__exponential_sessions_send", Some("Sort today's widget reports".into()))),
            steer::ViewerEvent::Activity(7, steer::ActivityEvent::user_message("Message from Release (session s9): The 2.4 build is green, ship it?")),
            steer::ViewerEvent::Activity(8, steer::ActivityEvent::user_message("Label the Android ones too, and check the board every hour.")),
            steer::ViewerEvent::Activity(9, steer::ActivityEvent::tool("Grep", Some("subscribeShapes( · 6 matches across 3 files".into()))),
            steer::ViewerEvent::Activity(10, steer::ActivityEvent::narration("Done, 3 issues labelled android.")),
        ];
        let short = vec![
            steer::ViewerEvent::Phase(steer::ViewerPhase::Live),
            steer::ViewerEvent::Activity(1, steer::ActivityEvent::user_message("Label the Android ones too, and check the board every hour.")),
            steer::ViewerEvent::Activity(2, steer::ActivityEvent::tool("mcp__exponential__exponential_issues_list", Some("search: android · 3 issues".into()))),
            steer::ViewerEvent::Activity(3, steer::ActivityEvent::narration("Done, 3 issues labelled android. The board now carries the android label on every report that names a phone, and the hourly check runs from the next full hour on, so a new report never waits longer than an hour.")),
        ];
        for (name, events) in [("chat-tab-long", long), ("chat-tab-short", short)] {
            let window = cx
                .open_window(gpui::size(gpui::px(1100.), gpui::px(760.)), |window, cx| cx.new(|cx| SessionChatScreenView::new("r1".to_string(), window, cx)))
                .expect("a headless window");
            let view = window.root(&mut cx).expect("the view");
            cx.update_window(window.into(), |_, _, cx| {
                let inner = view.read(cx).root_view.read(cx).inner().clone();
                inner.update(cx, |inner, cx| inner.apply_events_for_test(events, cx));
            })
            .expect("events");
            for _ in 0..6 {
                cx.update_window(window.into(), |_, window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                })
                .expect("a frame draws");
                cx.run_until_parked();
            }
            let shot = cx.capture_screenshot(window.into()).expect("a capture");
            shot.save(format!("{dir}/{name}.png")).expect("saved");
        }
        std::mem::forget(cx);
    }

    // ── #EDE-65 lane O: THE composer is the chat document's footer ─────────

    /// The Chat tab's live root view, its composer document and a draw.
    fn chat_tab_composer(cx: &mut gpui::TestAppContext) -> (Entity<crate::steer_viewer::SteerSessionView>, Entity<eui_gpui::view::SurfaceView>, &mut gpui::VisualTestContext) {
        seed_agent_chat("Hey", cx);
        let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
        let inner = cx.update(|_, cx| view.read(cx).root_view.read(cx).inner().clone());
        cx.update(|_, cx| inner.update(cx, |inner, cx| inner.apply_events_for_test(vec![steer::ViewerEvent::Phase(steer::ViewerPhase::Live)], cx)));
        settle(cx);
        let document = cx.update(|_, cx| inner.read(cx).composer_document()).expect("THE composer is the chat document's footer");
        crate::steer_viewer::chat_tab_composer::DISPATCHED.with(|dispatched| dispatched.borrow_mut().clear());
        (inner, document, cx)
    }

    fn settle(cx: &mut gpui::VisualTestContext) {
        for _ in 0..4 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.run_until_parked();
        }
    }

    fn dispatched() -> Vec<crate::steer_viewer::chat_tab_composer::ComposerEvent> {
        crate::steer_viewer::chat_tab_composer::DISPATCHED.with(|dispatched| dispatched.borrow().clone())
    }

    fn type_draft(document: &Entity<eui_gpui::view::SurfaceView>, text: &str, cx: &mut gpui::VisualTestContext) {
        cx.update(|window, cx| document.update(cx, |document, cx| document.type_into("chat_composer", text, window, cx)));
        cx.run_until_parked();
        // The field's debounce, flushed (what a press or a key does first).
        cx.update(|window, cx| document.update(cx, |document, cx| document.flush_field("chat_composer", None, window, cx)));
        settle(cx);
    }

    /// Typing is THE draft; a send goes through the view's send (no server
    /// here: refused, its draft goes back into the field).
    #[gpui::test]
    fn the_chat_tabs_composer_types_and_sends(cx: &mut gpui::TestAppContext) {
        use crate::steer_viewer::chat_tab_composer::ComposerEvent;
        let (inner, document, cx) = chat_tab_composer(cx);
        cx.update(|_, cx| {
            let document = document.read(cx);
            for id in ["chat_composer", "chat_plus_button"] {
                assert!(document.painted_bounds(id).is_some(), "{id} paints");
            }
        });
        type_draft(&document, "Ship it", cx);
        assert_eq!(cx.update(|_, cx| inner.read(cx).composer_draft(cx)), "Ship it", "{:?} field {:?}", dispatched(), cx.update(|_, cx| document.read(cx).field_text("chat_composer", cx)));
        cx.update(|window, cx| document.update(cx, |document, cx| document.submit_composer("chat_composer", window, cx)));
        settle(cx);
        assert!(dispatched().contains(&ComposerEvent::Send("Ship it".into())), "{:?}", dispatched());
        assert_eq!(cx.update(|_, cx| inner.read(cx).composer_draft(cx)), "Ship it", "a refused send keeps its draft");
        assert_eq!(cx.update(|_, cx| document.read(cx).field_text("chat_composer", cx)).as_deref(), Some("Ship it"), "…and the field shows it again");
    }

    /// A running turn makes Send the Stop (one round button), which stops it.
    #[gpui::test]
    fn the_chat_tabs_composer_stops_a_running_turn(cx: &mut gpui::TestAppContext) {
        use crate::steer_viewer::chat_tab_composer::ComposerEvent;
        let (inner, document, cx) = chat_tab_composer(cx);
        cx.update(|_, cx| {
            inner.update(cx, |inner, cx| inner.apply_events_for_test(
                vec![
                    steer::ViewerEvent::Activity(1, steer::ActivityEvent::user_message("Ship the fix")),
                    steer::ViewerEvent::Activity(2, steer::ActivityEvent::turn(steer::TurnState::Started)),
                ],
                cx,
            ))
        });
        settle(cx);
        let busy = cx.update(|_, cx| {
            let document = document.read(cx);
            document.surface().layout_node(document.index_of("chat_composer").unwrap()).and_then(|node| node.props.get("busy").cloned())
        });
        assert_eq!(busy, Some(serde_json::json!(true)), "the composer is busy while the turn runs");
        cx.update(|window, cx| document.update(cx, |document, cx| document.submit_composer("chat_composer", window, cx)));
        settle(cx);
        assert_eq!(dispatched(), vec![ComposerEvent::Stop]);
    }

    /// THE `/` menu above the composer: the keys move its highlight and
    /// Enter picks the row (its insertion in the draft, nothing sent).
    #[gpui::test]
    fn the_chat_tabs_slash_menu_picks_a_row_by_key(cx: &mut gpui::TestAppContext) {
        use crate::steer_viewer::chat_tab_composer::ComposerEvent;
        let (inner, document, cx) = chat_tab_composer(cx);
        type_draft(&document, "/", cx);
        assert!(cx.update(|_, cx| document.read(cx).painted_bounds("chat_slash").is_some()), "the / menu opens over the composer");
        let items = crate::slash_commands::slash_items("/", Some(steer::activity::SessionAgent::Claude), &[]);
        let second = match &items[1] {
            crate::slash_commands::SlashItem::Command(command) => crate::slash_commands::insertion(command),
            other => panic!("{other:?}"),
        };
        for key in ["down", "enter"] {
            let taken = cx.update(|window, cx| document.update(cx, |document, cx| document.composer_key("chat_composer", key, &gpui::Modifiers::default(), false, window, cx)));
            assert!(taken, "{key} goes to the menu");
            settle(cx);
        }
        assert_eq!(dispatched().iter().filter(|e| matches!(e, ComposerEvent::Key(_))).count(), 2);
        assert_eq!(cx.update(|_, cx| inner.read(cx).composer_draft(cx)), second);
        assert_eq!(cx.update(|_, cx| document.read(cx).field_text("chat_composer", cx)), Some(second.clone()), "the insertion is in the field");
        assert!(!dispatched().iter().any(|e| matches!(e, ComposerEvent::Send(_))), "a pick never sends");
    }

    /// The "+" Menu is THE options menu (its rows from the data); "Photo or
    /// file" opens the file chooser, the pick's chip sits in the strip and
    /// removes it; a pasted image stages the same way.
    #[gpui::test]
    fn the_chat_tabs_plus_menu_attaches_and_a_chip_unattaches(cx: &mut gpui::TestAppContext) {
        use crate::steer_viewer::chat_tab_composer::{ComposerEvent, PlusAction};
        let (inner, document, cx) = chat_tab_composer(cx);
        let items = cx.update(|_, cx| {
            let document = document.read(cx);
            document.surface().layout_node(document.index_of("chat_plus").unwrap()).and_then(|node| node.props.get("items").cloned())
        });
        assert_eq!(items.as_ref().and_then(|items| items[0]["value"].as_str()), Some("add-file"), "{items:?}");
        assert!(cx.update(|window, cx| inner.update(cx, |inner, cx| inner.open_composer_plus(None, window, cx))));
        settle(cx);
        assert!(cx.update(|_, cx| document.read(cx).layers().iter().any(|layer| layer.owner == "chat_plus")), "the + opens its Menu");
        let item = cx.update(|_, cx| document.read(cx).index_of("chat_plus.item.0")).expect("Photo or file");
        cx.update(|window, cx| document.update(cx, |document, cx| document.press(item, window, cx)));
        settle(cx);
        assert!(dispatched().contains(&ComposerEvent::Plus(PlusAction::AddFile)), "{:?}", dispatched());
        assert!(cx.did_prompt_for_paths(), "the file chooser opens");
        let dir = std::env::temp_dir().join(format!("ui-chat-tab-composer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("shot.png");
        std::fs::write(&png, b"\x89PNG\r\n\x1a\n0000").unwrap();
        let picked = png.clone();
        cx.simulate_path_prompt_response(move |_| Some(vec![picked]));
        settle(cx);
        assert_eq!(cx.update(|_, cx| inner.read(cx).composer_files()), 1);
        let remove = cx.update(|_, cx| {
            let document = document.read(cx);
            document.placed_nodes().iter().find(|node| node.id.starts_with("chat_file") && node.id.contains("remove") && !node.hidden && !node.removed).map(|node| node.id.clone())
        });
        let remove = remove.expect("the file's chip is removable");
        let index = cx.update(|_, cx| document.read(cx).index_of(&remove)).unwrap();
        cx.update(|window, cx| document.update(cx, |document, cx| document.press(index, window, cx)));
        settle(cx);
        assert!(dispatched().iter().any(|e| matches!(e, ComposerEvent::Unattach(_))), "{:?}", dispatched());
        assert_eq!(cx.update(|_, cx| inner.read(cx).composer_files()), 0, "the chip removed it");
        // A pasted image: the upload hook stages it, `attach` {files} follows.
        let pasted = eui_gpui::view::PastedImage { name: "image.png".into(), mime: "image/png".into(), path: png.clone(), size: 12 };
        cx.update(|_, cx| document.update(cx, |document, cx| document.composer_paste("chat_composer", vec![pasted], cx)));
        settle(cx);
        assert!(dispatched().contains(&ComposerEvent::Attach { pasted: true }));
        assert_eq!(cx.update(|_, cx| inner.read(cx).composer_files()), 1, "the paste staged it");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
