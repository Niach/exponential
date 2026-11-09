//! EXP-1294 — the CHAT screen ([`Screen::Chat`]): a chat IS a root
//! session (kind `thread`), its model the contract's
//! [`domain::session_tree::build_session_chat`] over the synced rows
//! ([`crate::session_data::chat_view_and_messages`], fixture
//! `session-chat.json`).
//!
//! * The HEADER: the title (EXP-1316: derived, plain text, nobody renames
//!   it) and a lock while private — no caption; its trailing slot holds the
//!   Merge pill while the chat's PR is open, then the `…`, on EVERY tab.
//! * The STRIP (EXP-1334, [`chat_strip`], never scrolls): `+` ·
//!   `Conversation` pinned left (S-3: no Guide tab, results carry what a
//!   session shows); after them the TOP-LEVEL linked issues
//!   (status glyph, mono identifier, title) then the dispatcher's DIRECT
//!   children, laid out by `partition_chat_strip` (titles → identifiers
//!   → a "+N" menu per zone). A chip with sub-issues / deeper sessions wears
//!   a chevron and a click opens THE Menu of its subtree (the tree rows);
//!   it reads `<chip> › <open one>` while a nested one is open. A chip
//!   linked moments before it first renders fades in with a ring.
//! * The CONVERSATION (no root card: it IS the dispatcher): the root's
//!   messages plus its children's question / plan cards — a person's message a
//!   right-aligned bubble over `author · HH:MM` (`Queued` + remove while the
//!   device has not consumed it), the agent's prose as markdown, a session
//!   = THE session card ([`crate::work_card::session_card`], opens that
//!   session), question and plan cards = THE session view's card
//!   (`crate::steer_viewer::ask_card` + its option rows, the free-text row
//!   and the plan's note; `sessions.answer`), events as muted lines, pictures as tiles; under it
//!   THE session composer box ([`crate::composer::session_composer_card`])
//!   sending `sessions.send({sessionId: root})`.
//! * NEW-6 (N-1) / NEW-46: a chat with ONE session (`view.main_session_id`,
//!   no child yet): its Conversation tab = that session's raw work in place
//!   (its own composer, Stop in the header's trailing slot). The tab stays
//!   in the strip; the first child turns it into the dispatcher's
//!   conversation.
//! * An ISSUE child = the issue detail, EMBEDDED (no work header bar, no
//!   Chats row).
//! * THE Menu on the `+` and on every issue chip: `Link an issue` (THE
//!   search list, `sessions.linkIssues`), `New issue` and, on a chip,
//!   `Unlink <IDENT>` (`sessions.unlinkIssue`).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    div, prelude::FluentBuilder as _, px, Animation, AnimationExt as _, AnyElement, App, AppContext as _, ClickEvent,
    Entity, FocusHandle, Focusable, FollowMode, FontWeight, InteractiveElement as _, IntoElement, ListAlignment,
    ListState, ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Task,
    Window,
};
use gpui_component::{button::ButtonVariants as _, h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};
use sync::Store;

use domain::rows::{Issue, MessageRow, PullRequestRow, SessionIssueRow, SessionRow};
use domain::session_results::SessionResultEntry;
use domain::dock::DockSource;
use domain::session_tree::{SessionChatRow, SessionChatView, ChatTab, SESSION_CHAT, CHATS_LIST};
use domain::results_tab::RESULTS_TAB;

use crate::controls::PointerContextMenuExt as _;
use crate::controls::WebControl as _;
use crate::controls::WebText as _;
use crate::icons::registry;
use crate::markdown::{ImageCache, IssueChipCache, MarkdownView, RefResolver};
use crate::navigation::Screen;
use crate::transcript_rows::{plan_list_sync, ListOp, RowKey, WORKING_ROW_ID};

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
/// An issue chip title's ONE max width (then the ellipsis).
const ISSUE_CHIP_TITLE_MAX_W: f32 = 140.;
/// The issue chip's lead (a live root's mark) and its state badge.
const CHIP_MARK_PX: f32 = 12.;
const CHIP_BADGE_PX: f32 = 5.;

/// Which child of the strip is on show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChatChildSel {
    Conversation,
    Issue(String),
    /// results-tab.json: the strip's last tab.
    Results,
    /// #EXP-1662 THE plan tab: the plan a row opened, else (`None`, the
    /// chip) `plan_tab_target`.
    Plan(Option<String>),
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

/// #EXP-1662: the plan a way in wants its chat's plan tab on (root id →
/// plan card id), read by that chat's view on its next frame.
#[derive(Default)]
struct PendingChatPlan(HashMap<String, String>);

impl gpui::Global for PendingChatPlan {}

/// #EXP-1662: select the plan tab of the chat `root_id` on `message_id`
/// (the caller navigates there; on that chat already, this alone does it).
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
    generation: u64,
}

impl gpui::Global for ChatRowState {}

/// Bumped whenever a row expands, collapses or reads `Answered`: the
/// Conversation lists re-key on it ([`row_state_fingerprint`]).
pub(crate) fn chat_row_generation(cx: &App) -> u64 {
    cx.try_global::<ChatRowState>().map_or(0, |state| state.generation)
}

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
    state.generation = state.generation.wrapping_add(1);
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
        state.generation = state.generation.wrapping_add(1);
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

/// What a row's client state adds to its measure: an expanded session card
/// carries its waiting card (so a synced / changed card re-measures).
pub(crate) fn row_state_fingerprint(row: &SessionChatRow, root_id: &str, cx: &App) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let SessionChatRow::Session { card, .. } = row else {
        return 0;
    };
    if !row_expanded(root_id, &card.session_id, cx) {
        return 0;
    }
    let mut hasher = std::hash::DefaultHasher::new();
    serde_json::to_string(&waiting_card_row(&card.session_id, cx)).unwrap_or_default().hash(&mut hasher);
    hasher.finish() | 1
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
    let action = session_row_action("event", session_id, root_id, rows, cx);
    match action.session_id {
        Some(target) if action.chevron => muted_line_button(
            SharedString::from(format!("{}-{id}", domain::chat_row_action::test_ids().event)),
            line.to_string(),
            move |window, cx| open_session_card(&target, window, cx),
            cx,
        ),
        _ => event_line(line.to_string(), cx),
    }
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

/// The strip's chip frame (an issue, a session, a folded zone): THE strip
/// chip box ×2 (EXP-1334 [`crate::surface::strip_chip_frame`], the box the
/// work tabs strip wears too) with its hairline at rest.
pub(crate) fn strip_chip(id: impl Into<gpui::ElementId>, active: bool, cx: &App) -> gpui::Stateful<gpui::Div> {
    crate::surface::strip_chip_frame(id, active, true, cx)
}

/// The strip's TAB frame (Conversation, Guide): the same box with no stroke
/// at rest (web `stripTabClass(active)` unbordered).
pub(crate) fn strip_tab(id: impl Into<gpui::ElementId>, active: bool, cx: &App) -> gpui::Stateful<gpui::Div> {
    crate::surface::strip_chip_frame(id, active, false, cx)
}

/// THE issue chip's lead ×4: the issue's STATUS glyph. S-3: an unresolved
/// status (the issue row not synced) draws NO glyph, never a Backlog one.
pub(crate) fn issue_glyph(status: Option<&domain::statuses::ResolvedStatus>, cx: &App) -> Option<Icon> {
    status.map(|status| crate::icons::resolved_status_icon(status, cx).xsmall().flex_shrink_0())
}

/// The issue chip's body in the strip: status glyph, mono identifier, title.
pub(crate) fn strip_issue_body(
    status: Option<domain::statuses::ResolvedStatus>,
    identifier: &str,
    title: &str,
    active: bool,
    cx: &App,
) -> Vec<AnyElement> {
    let theme = cx.theme();
    let mut body: Vec<AnyElement> = issue_glyph(status.as_ref(), cx)
        .map(IntoElement::into_any_element)
        .into_iter()
        .collect();
    body.extend([
        div()
            .flex_shrink_0()
            .text_xs()
            .font_family(theme::terminal::FONT_FAMILY)
            .text_color(theme.muted_foreground)
            .child(SharedString::from(identifier.to_string()))
            .into_any_element(),
        div()
            .flex_shrink(1.)
            .min_w_0()
            .max_w(px(ISSUE_CHIP_TITLE_MAX_W))
            .truncate()
            .text_color(if active { theme.foreground } else { theme.muted_foreground })
            .child(SharedString::from(title.to_string()))
            .into_any_element(),
    ]);
    body
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
    /// A cross-session sender: a press anywhere on the bubble opens it.
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
    let tip = name.clone().unwrap_or_default();
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
        .when_some(name.filter(|name| !name.is_empty() && !own), |card, name| {
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
        })
        .when_some(link, |card, session_id| {
            let tip = SharedString::from(tip);
            // EXP-1744: a pressable bubble is a button named by its sender
            // (what the retired xref row carried).
            card.role(gpui::Role::Button)
                .aria_label(tip.clone())
                .cursor_pointer()
                .hover(|style| style.border_color(theme::tokens::glass::STROKE_STRONG.to_hsla()))
                .active(|style| style.opacity(0.85))
                .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx))
                .on_click(move |_, window, cx| crate::navigation::open_session_link(&session_id, window, cx))
        });
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .map(|column| if own { column.items_end() } else { column.items_start() })
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
    use domain::transcript_grouping::SenderKind;
    let side = transcript_sizes().avatar;
    let size = gpui_component::Size::Size(px(side));
    match sender.kind {
        SenderKind::Member => {
            let image = Store::try_global(cx)
                .and_then(|store| store.collections().users.read(cx).get(&sender.id).cloned())
                .and_then(|user| user.image);
            crate::user_avatar::user_avatar(&sender.id, &sender.name, image.as_deref(), size, cx)
        }
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

/// A conversation row as the grouping reads it: a card-less message, a
/// child's report (a message from its session), a subagent line (the
/// agent's trace), every other card full.
pub(crate) fn transcript_item(row: &SessionChatRow) -> domain::transcript_grouping::TranscriptItem {
    use domain::transcript_grouping::TranscriptItem;
    match row {
        SessionChatRow::Message { id, author, author_id, author_session_id, .. } => {
            TranscriptItem::message(id.as_str(), author, author_id.as_deref(), author_session_id.as_deref())
        }
        SessionChatRow::Report { id, session_id, .. } => TranscriptItem::message(
            id.as_str(),
            domain::contract::MESSAGE_AUTHOR_KIND_AGENT,
            None,
            Some(session_id.as_str()),
        ),
        SessionChatRow::Subagent { id, .. } => TranscriptItem::trace(id.as_str()),
        other => TranscriptItem::full(other.id()),
    }
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

/// #EXP-1736: the grouping input of a conversation's `rows`: the viewer,
/// the chat (its session, title, agent, the S-3 `origin` that started it
/// and whether `rows` begin at the conversation's first row: the origin
/// sends that first message), the members and agents it names off the
/// synced rows, the sessions its links know (every row's xref + the origin).
pub(crate) fn conversation_input<'a>(
    rows: impl IntoIterator<Item = &'a SessionChatRow>,
    session_id: &str,
    title: &str,
    agent_id: Option<&str>,
    origin: Option<&domain::session_tree::SessionXref>,
    from_start: bool,
    cx: &App,
) -> domain::transcript_grouping::TranscriptInput {
    use domain::transcript_grouping::{TranscriptAgent, TranscriptChat, TranscriptInput, TranscriptMember};
    let store = Store::try_global(cx);
    let mut input = TranscriptInput {
        // No store (a styleguide entry) = no viewer, never a panic.
        viewer_id: store.and_then(|_| crate::queries::active_account(cx)).map(|me| me.user_id),
        chat: TranscriptChat {
            session_id: session_id.to_string(),
            title: title.to_string(),
            agent_id: agent_id.filter(|id| !id.is_empty()).map(str::to_string),
            origin_session_id: origin.map(|origin| origin.session_id.clone()),
            from_start,
        },
        ..TranscriptInput::default()
    };
    if let Some(agent) = input.chat.agent_id.as_deref() {
        if let Some(name) = crate::session_data::agent_def(agent, cx).and_then(|def| def.name).filter(|name| !name.trim().is_empty()) {
            input.agents.push(TranscriptAgent { id: agent.to_string(), name });
        }
    }
    let mut seen: HashSet<String> = HashSet::new();
    if let Some(origin) = origin {
        seen.insert(origin.session_id.clone());
        input.sessions.push(xref_session(origin));
    }
    let mut members: HashSet<String> = HashSet::new();
    for row in rows {
        let xref = match row {
            SessionChatRow::Message { author_id, xref, .. } => {
                if let Some(user_id) = author_id.as_deref().filter(|id| members.insert(id.to_string())) {
                    let user = store.and_then(|store| store.collections().users.read(cx).get(user_id).cloned());
                    if let Some(user) = user {
                        input.members.push(TranscriptMember {
                            id: user_id.to_string(),
                            name: crate::comments::author_label(Some(&user)),
                        });
                    }
                }
                xref.as_ref()
            }
            SessionChatRow::Report { xref, .. } => Some(xref),
            _ => None,
        };
        if let Some(xref) = xref.filter(|xref| seen.insert(xref.session_id.clone())) {
            input.sessions.push(xref_session(xref));
        }
    }
    input
}

/// A card-less message row (or a child's report) as THE bubble under its
/// layout (`key` prefixes the element ids, `at` its createdAt); a system
/// line = the muted line; `None` for every card ([`card_row`]).
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
        SessionChatRow::Message { author, body, .. } if author == domain::contract::MESSAGE_AUTHOR_KIND_SYSTEM => {
            return Some(muted_line(registry::UI_INFO, body.clone(), cx));
        }
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

/// An event line (web `session-chat-view` `chat-event`): the muted
/// text alone, no glyph.
pub(crate) fn event_line(text: String, cx: &App) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(SharedString::from(text))
        .into_any_element()
}

/// #EXP-1464: [`event_line`] as a button (chat-row-action.json): the
/// trailing chevron, hover = foreground, press = the pressed wash; it hugs
/// its text (web `chat-event`, `self-start`).
pub(crate) fn muted_line_button(
    id: SharedString,
    text: String,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;
    h_flex().w_full().min_w_0().child(
        h_flex()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(text.clone()))
            .max_w_full()
            .min_w_0()
            .gap_1()
            .items_center()
            .text_xs()
            .text_color(muted)
            .cursor_pointer()
            .rounded(px(theme::tokens::radius::SM))
            .hover(move |style| style.text_color(foreground))
            .active(|style| style.opacity(0.7))
            .child(div().min_w_0().truncate().child(SharedString::from(text)))
            .child(Icon::new(registry::UI_CHEVRON_RIGHT).with_size(px(14.)).flex_shrink_0())
            .on_click(move |_, window, cx| on_click(window, cx)),
    )
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

/// N-12: THE Dock of a root's Conversation (a chat, an agent channel;
/// `fixtures/dock.json`): the root's own cards (THE Queue, the queued
/// messages, the banners, off the hosted composer view) above THE
/// composer; returns the band and the open card's sheet. `suggestions` =
/// the root's id when its suggested-tasks cards dock too (the chat).
/// S-3: no plan / question source (inline in the
/// Conversation) and nothing opens itself. #EXP-1545: `queue_from` = the
/// Live child ([`domain::dock::chat_dock_queue`]'s pick) whose Queue the
/// `tasks` card shows, named in its caption, its sheet that child's Trace;
/// `None` = the root's own.
#[allow(clippy::too_many_arguments)] // the host's facts
pub(crate) fn root_dock(
    prefix: &str,
    rows: &[SessionChatRow],
    suggestions: Option<&str>,
    state: &Entity<crate::dock::DockState>,
    composer: &Entity<crate::steer_viewer::SteerSessionView>,
    queue_from: Option<(&Entity<crate::steer_viewer::SteerSessionView>, &str)>,
    text: &ChatText,
    window: &mut Window,
    cx: &mut App,
) -> (Option<AnyElement>, Option<AnyElement>) {
    let mut sources = Vec::new();
    if suggestions.is_some() {
        sources.extend(suggestion_sources(rows));
    }
    sources.extend(composer.read(cx).session_dock_sources(cx));
    if let Some((child, name)) = queue_from {
        let queue = child.read(cx).dock_queue();
        for source in &mut sources {
            if matches!(source, DockSource::Tasks { .. }) {
                *source = DockSource::Tasks { session_name: Some(name.to_string()), queue: queue.clone() };
            }
        }
    }
    let dock_rows = domain::dock::dock_rows(&sources);
    let sheet = match state.read(cx).sheet_id().map(str::to_string) {
        Some(open) => match dock_rows.iter().find(|row| row.id == open).cloned() {
            Some(dock_row) => {
                // THE sheet is a sheet host: its pickers open inline.
                let content = crate::dock::sheet_host(|| match rows.iter().find(|row| row.id() == open) {
                    Some(SessionChatRow::Suggestions { id, start_all, items, .. }) => {
                        Some(suggestions_sheet(id, *start_all, items, text.team_id(), cx))
                    }
                    Some(_) => None,
                    None => match queue_from {
                        Some((child, _)) if dock_row.kind == domain::dock::DockKind::Tasks => {
                            child.update(cx, |child, cx| child.render_session_dock_sheet(&dock_row, window, cx))
                        }
                        _ => composer.update(cx, |composer, cx| composer.render_session_dock_sheet(&dock_row, window, cx)),
                    },
                });
                content.map(|content| crate::dock::sheet(state, &dock_row, content, window, cx))
            }
            None => {
                state.update(cx, |state, _| state.close());
                None
            }
        },
        None => None,
    };
    let (actions, card) = composer.update(cx, |composer, cx| {
        composer.dev_type_composer_text(window, cx);
        (composer.session_dock_actions(cx), composer.render_session_composer(window, cx))
    });
    let band = crate::dock::band(
        state,
        crate::dock::Band { id: SharedString::from(prefix.to_string()), rows: dock_rows, actions, composer: card },
        window,
        cx,
    );
    (band, sheet)
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
// EXP-1334: the strip (never scrolls) and the chips' subtree menus
// ---------------------------------------------------------------------------

/// A chip's mark state as THE session state (the run mark reads it; S-3:
/// they are one type, the four display states).
fn chip_state(mark: domain::session_tree::SessionTabMarkState) -> domain::session_tree::SessionState {
    mark
}

/// Open `session_id` IN PLACE (its chat hosts the raw work).
fn open_session_in_place(session_id: String, window: &mut Window, cx: &mut App) {
    crate::navigation::navigate(window, cx, Screen::Session { session_id });
}

/// What the strip shows open (a strip pick).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StripPick {
    Conversation,
    Issue(String),
    Session(String),
    Results,
    /// #EXP-1662: the plan tab.
    Plan,
}

/// #EXP-1662: the strip's plan chip.
#[derive(Clone)]
pub(crate) struct StripPlan {
    /// `plan_row_label`.
    pub(crate) label: String,
    /// Its plan waits: the needs-you dot.
    pub(crate) waiting: bool,
    /// #FEED-191 (`plan_tab_glyph`): approved = THE check in emerald.
    pub(crate) glyph: domain::plan_page::PlanTabGlyph,
}

/// #FEED-191 (`plan_tab_glyph`): the plan chip's lead glyph: approved = THE
/// check in the plan row's emerald, else the plan glyph in the chip's colour.
fn plan_tab_icon(glyph: domain::plan_page::PlanTabGlyph, cx: &App) -> Icon {
    let icon = Icon::new(if glyph.icon == "ui-check" { registry::UI_CHECK } else { registry::CODING_PLAN })
        .xsmall()
        .flex_shrink_0();
    if glyph.tone == "approved" {
        icon.text_color(cx.theme().success)
    } else {
        icon
    }
}

/// #EXP-1662: a plan row as the strip's chip (a non-plan row = `None`).
pub(crate) fn strip_plan_of(row: &SessionChatRow) -> Option<StripPlan> {
    match row {
        SessionChatRow::Plan { plan, decision, .. } => {
            Some(StripPlan {
                label: domain::plan_page::plan_row_label(plan),
                waiting: decision.is_none(),
                glyph: domain::plan_page::plan_tab_glyph(decision.as_deref()),
            })
        }
        _ => None,
    }
}

/// #EXP-1662 / #FEED-191: the plan the chat's tab targets (`plan_tab_target`
/// over its plan rows, oldest first, and the plan `kept` this visit).
pub(crate) fn tab_plan<'a>(rows: &'a [SessionChatRow], kept: Option<&str>) -> Option<&'a SessionChatRow> {
    let plans: Vec<(&str, Option<&str>)> = rows
        .iter()
        .filter_map(|row| match row {
            SessionChatRow::Plan { id, decision, .. } => Some((id.as_str(), decision.as_deref())),
            _ => None,
        })
        .collect();
    let target = domain::plan_page::plan_tab_target(&plans, kept)?;
    rows.iter().find(|row| row.id() == target)
}

/// One linked issue as the strip paints it.
#[derive(Clone, Default)]
pub(crate) struct StripIssue {
    pub(crate) title: String,
    pub(crate) status: Option<domain::statuses::ResolvedStatus>,
    /// The link id while the chip wears its NEW ring.
    pub(crate) ringed: Option<String>,
}

pub(crate) type OnStripPick = std::rc::Rc<dyn Fn(StripPick, &mut Window, &mut App)>;

/// THE chat strip's inputs (the screen and the styleguide draw it):
/// `[+] [Conversation] | the one issue chip OR the group tab | Results`,
/// nothing scrolls (#EXP-1433, `domain::session_tree::chat_strip_mode`).
pub(crate) struct ChatStrip {
    /// Element id prefix (`chat` on the screen).
    pub(crate) prefix: &'static str,
    pub(crate) tabs: Vec<ChatTab>,
    pub(crate) issues: HashMap<String, StripIssue>,
    /// Each session's agent CLI (its run mark).
    pub(crate) clis: HashMap<String, Option<String>>,
    pub(crate) open: StripPick,
    /// THE chat menu's target for the `+` and the issue chip's context
    /// menu (`None` = a non-member: no `+`, no menu).
    pub(crate) menu: Option<ChatMenuTarget>,
    pub(crate) on_pick: OnStripPick,
    /// results-tab.json: the Results tab pinned right after a divider (no
    /// dot); an agent's chat has none.
    pub(crate) results: bool,
    /// #EXP-1562 (waiting-cards.json CHIPS): the conversation's WAITING
    /// cards (Conversation order) and what a chip's press does; empty =
    /// no chips.
    pub(crate) waiting: Vec<domain::waiting_cards::WaitingCardInput>,
    pub(crate) on_waiting: Option<crate::waiting_cards::OnWaitingPick>,
    /// #EXP-1662: the plan chip, right after Conversation (`None` = the
    /// chat holds no plan the tab targets).
    pub(crate) plan: Option<StripPlan>,
}

/// One row of the strip's group menu (#EXP-1433).
#[derive(Clone)]
pub(crate) enum StripGroupItem {
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

impl StripGroupItem {
    fn pick(&self) -> StripPick {
        match self {
            Self::Issue { issue_id, .. } => StripPick::Issue(issue_id.clone()),
            Self::Session { session_id, .. } => StripPick::Session(session_id.clone()),
        }
    }

    /// The group tab's label while this row is open: the identifier, or the
    /// session's name.
    fn open_label(&self) -> &str {
        match self {
            Self::Issue { identifier, .. } => identifier,
            Self::Session { name, .. } => name,
        }
    }
}

/// #EXP-1433: the strip's items: EVERY issue of the tree (a chip's
/// sub-issues after it) and every session under the root as THE tree rows
/// (depth relative to the root's direct children, 0).
pub(crate) fn strip_group_items(spec: &ChatStrip) -> (Vec<StripGroupItem>, Vec<(StripGroupItem, usize)>) {
    let issue = |issue_id: &str, identifier: &str| {
        let info = spec.issues.get(issue_id).cloned().unwrap_or_default();
        StripGroupItem::Issue {
            issue_id: issue_id.to_string(),
            identifier: identifier.to_string(),
            title: info.title,
            status: info.status,
        }
    };
    let session = |session_id: &str, name: &str, mark| StripGroupItem::Session {
        session_id: session_id.to_string(),
        name: name.to_string(),
        state: chip_state(mark),
        cli: spec.clis.get(session_id).cloned().flatten(),
    };
    let mut issues = Vec::new();
    let mut sessions = Vec::new();
    for tab in &spec.tabs {
        match tab {
            ChatTab::Issue { issue_id, identifier, nested } => {
                issues.push(issue(issue_id, identifier));
                issues.extend(nested.iter().map(|item| issue(&item.issue_id, &item.identifier)));
            }
            ChatTab::Session { session_id, name, mark_state, nested } => {
                sessions.push((session(session_id, name, *mark_state), 0));
                sessions.extend(nested.iter().map(|item| (session(&item.session_id, &item.name, item.mark_state), item.depth)));
            }
            _ => {}
        }
    }
    (issues, sessions)
}

/// One group menu row: an issue = THE issue search list's row (status
/// glyph, mono identifier, title); a session = THE tree row (guides, the
/// run mark, the name, its state word in its tone); the open one checked.
fn group_row(item: &StripGroupItem, guides: &domain::tree_guides::Guides, active: bool, cx: &App) -> gpui::Div {
    use domain::list_item::{GAP, INDENT, SMALL};
    let theme = cx.theme();
    let mut row = h_flex()
        .relative()
        .flex_1()
        .min_w_0()
        .h(px(SMALL))
        .gap(px(GAP))
        .items_center()
        .pl(px(INDENT * guides.depth() as f32))
        .children(crate::tree_guides::guide_layer(guides, 0., 0.));
    row = match item {
        StripGroupItem::Issue { identifier, title, status, .. } => row
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
        StripGroupItem::Session { name, state, cli, .. } => {
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
    };
    let check =
        active.then(|| Icon::new(registry::UI_CHECK).size(px(14.)).text_color(theme.foreground).into_any_element());
    crate::controls::pointer_row_fill(
        h_flex().flex_1().min_w_0().gap_2().items_center().child(row).children(check),
        false,
    )
}

/// #EXP-1433: THE Menu of the group tab: the issue rows, a separator, then
/// the session tree rows (one guide set); a pick opens it in place.
pub(crate) fn strip_group_menu(
    mut menu: gpui_component::menu::PopupMenu,
    issues: &[StripGroupItem],
    sessions: &[(StripGroupItem, usize)],
    open: &StripPick,
    on_pick: Option<&OnStripPick>,
) -> gpui_component::menu::PopupMenu {
    menu = menu.min_w(px(384.)).max_w(px(384.)).scrollable(true).max_h(px(448.));
    let flat = domain::tree_guides::guides_for(&vec![0; issues.len()]);
    let depths: Vec<usize> = sessions.iter().map(|(_, depth)| *depth).collect();
    let tree = domain::tree_guides::guides_for(&depths);
    let rows = issues
        .iter()
        .zip(flat)
        .map(|(item, guides)| (item, guides, false))
        .chain(sessions.iter().zip(tree).enumerate().map(|(at, ((item, _), guides))| (item, guides, at == 0)));
    for (item, guides, first_session) in rows {
        if first_session && !issues.is_empty() {
            menu = menu.separator();
        }
        let pick = item.pick();
        let active = *open == pick;
        let item = item.clone();
        let mut row = gpui_component::menu::PopupMenuItem::element(move |_, cx| group_row(&item, &guides, active, cx));
        if let Some(on_pick) = on_pick.cloned() {
            row = row.on_click(move |_, window, cx| on_pick(pick.clone(), window, cx));
        }
        menu = menu.item(row);
    }
    menu
}

/// #EXP-1433: THE group tab: the strip chip box, the `session-tree` glyph
/// and the count (muted) at rest; ACTIVE with the open item's label
/// (truncated at `stripGroupMaxWidth`) while one of its items is open.
pub(crate) fn strip_group_tab(id: SharedString, count: usize, open_label: Option<&str>, cx: &App) -> gpui::Stateful<gpui::Div> {
    let active = open_label.is_some();
    let tooltip = SharedString::from(match open_label {
        Some(label) => label.to_string(),
        None => SESSION_CHAT.copy.strip_group.clone(),
    });
    strip_chip(id, active, cx)
        .child(Icon::new(registry::SESSION_TREE).xsmall().flex_shrink_0())
        .child(match open_label {
            Some(label) => div()
                .min_w_0()
                .max_w(px(SESSION_CHAT.strip_group_max_width as f32))
                .truncate()
                .child(SharedString::from(label.to_string())),
            None => div()
                .flex_shrink_0()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(count.to_string())),
        })
        .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx))
}

/// Paint THE chat strip (#EXP-1433): `+` · Conversation | the ONE issue
/// chip (`chips`) or ONE group tab (`group`) | Results — nothing scrolls.
pub(crate) fn chat_strip(spec: ChatStrip, _window: &Window, cx: &App) -> AnyElement {
    let prefix = spec.prefix;
    let (issues, sessions) = strip_group_items(&spec);
    let mode = domain::session_tree::chat_strip_mode(issues.len(), sessions.len());

    let pick = |pick: StripPick| {
        let on_pick = spec.on_pick.clone();
        move |_: &ClickEvent, window: &mut Window, cx: &mut App| on_pick(pick.clone(), window, cx)
    };

    let mut lead: Vec<AnyElement> = Vec::new();
    if let Some(target) = spec.menu.clone() {
        let trigger =
            crate::controls::ghost_icon_button(SharedString::from(format!("{prefix}-child-add")), Icon::new(registry::UI_ADD), cx)
                .accessibility_label("Add an issue");
        lead.push(
            div()
                .flex_shrink_0()
                .child(crate::controls::PointerMenu::new(
                    SharedString::from(format!("{prefix}-child-add-menu")),
                    trigger,
                    move |menu, window, cx| chat_menu(menu, &target, window, cx),
                ))
                .into_any_element(),
        );
    }
    // NEW-6 (N-1): a chat with ONE session has no Conversation tab (its
    // raw work IS the body); the tab comes back with the first child.
    // The Results or plan tab open on a one-session chat brings the
    // Conversation tab back (the way home, web draws it always).
    if spec.tabs.iter().any(|tab| matches!(tab, ChatTab::Conversation))
        || matches!(spec.open, StripPick::Results | StripPick::Plan)
    {
        lead.push(
            strip_tab(SharedString::from(format!("{prefix}-child-conversation")), spec.open == StripPick::Conversation, cx)
                .child(Icon::new(registry::ACTION_CHAT).xsmall())
                .child(SharedString::from(SESSION_CHAT.copy.conversation.clone()))
                .on_click(pick(StripPick::Conversation))
                .into_any_element(),
        );
    }
    // #EXP-1662 (plan-page.json): THE plan chip, right after Conversation.
    if let Some(plan) = spec.plan.clone() {
        let mut chip = strip_chip(SharedString::from(domain::plan_page::PLAN_PAGE.test_ids.tab.clone()), spec.open == StripPick::Plan, cx)
            .debug_selector(|| domain::plan_page::PLAN_PAGE.test_ids.tab.clone())
            .max_w(px(domain::plan_page::PLAN_PAGE.layout.tab_max_width))
            .child(plan_tab_icon(plan.glyph, cx))
            .child(div().min_w_0().truncate().child(SharedString::from(plan.label)))
            .when(plan.waiting, |chip| {
                chip.child(div().flex_shrink_0().size(px(6.)).rounded_full().bg(theme::tokens::YELLOW.to_hsla()))
            })
            .on_click(pick(StripPick::Plan));
        chip.style().flex_shrink = Some(1.);
        lead.push(chip.into_any_element());
    }
    // #EXP-1562: the WAITING chips, right after Conversation (none = the
    // first of the tablist), before the issue / group zone.
    if let Some(on_waiting) = spec.on_waiting.clone() {
        lead.extend(crate::waiting_cards::strip_chips(prefix, &spec.waiting, on_waiting, cx));
    }

    let middle: Option<AnyElement> = match (mode.mode, issues.first()) {
        // `chips`: the ONE issue keeps its chip (full title, truncated).
        (domain::session_tree::StripMode::Chips, Some(StripGroupItem::Issue { issue_id, identifier, title, status })) => {
            let active = spec.open == StripPick::Issue(issue_id.clone());
            let id = SharedString::from(format!("{prefix}-child-{issue_id}"));
            let info = spec.issues.get(issue_id).cloned().unwrap_or_default();
            let mut chip = strip_chip(id, active, cx)
                .children(strip_issue_body(status.clone(), identifier, title, active, cx))
                .on_click(pick(StripPick::Issue(issue_id.clone())));
            if info.ringed.is_some() {
                chip = chip.border_color(cx.theme().ring);
            }
            let chip: AnyElement = match spec.menu.clone() {
                Some(target) => {
                    let target = ChatMenuTarget { chip: Some((issue_id.clone(), identifier.clone())), ..target };
                    chip.pointer_context_menu(move |menu, window, cx| chat_menu(menu, &target, window, cx))
                        .into_any_element()
                }
                None => chip.into_any_element(),
            };
            Some(match info.ringed {
                Some(link_id) => div()
                    .flex()
                    .min_w_0()
                    .child(chip)
                    .with_animation(
                        SharedString::from(format!("{prefix}-child-enter-{link_id}")),
                        Animation::new(theme::motion::ENTER).with_easing(theme::motion::decelerate()),
                        |el, delta| el.opacity(delta),
                    )
                    .into_any_element(),
                None => chip,
            })
        }
        // `group`: ONE icon tab over every issue and session.
        (domain::session_tree::StripMode::Group, _) => {
            let open_label = issues
                .iter()
                .chain(sessions.iter().map(|(item, _)| item))
                .find(|item| item.pick() == spec.open)
                .map(|item| item.open_label().to_string());
            let id = SharedString::from(if prefix == "chat" {
                SESSION_CHAT.test_ids.group.clone()
            } else {
                format!("{prefix}-strip-group")
            });
            let tab = strip_group_tab(id.clone(), mode.count, open_label.as_deref(), cx).into_any_element();
            let open = spec.open.clone();
            let on_pick = spec.on_pick.clone();
            Some(
                crate::controls::PointerMenu::new(
                    SharedString::from(format!("{id}-menu")),
                    crate::picker::PickerTrigger::new(SharedString::from(format!("{id}-trigger")).into(), tab),
                    move |menu, _window, _cx| strip_group_menu(menu, &issues, &sessions, &open, Some(&on_pick)),
                )
                .into_any_element(),
            )
        }
        _ => None,
    };

    // results-tab.json: pinned right after a divider (the Guide's place).
    let results = spec.results.then(|| {
        strip_tab(SharedString::from(RESULTS_TAB.tab.test_ids.tab.clone()), spec.open == StripPick::Results, cx)
            .child(Icon::new(registry::CHAT_RESULTS).xsmall())
            .child(SharedString::from(RESULTS_TAB.tab.copy.label.clone()))
            .on_click(pick(StripPick::Results))
            .into_any_element()
    });
    h_flex()
        .id(SharedString::from(SESSION_CHAT.test_ids.tabs.clone()))
        .w_full()
        .min_w_0()
        .gap_1()
        .items_center()
        .children(lead)
        .when(mode.count > 0, |row| row.child(strip_divider(cx)))
        .child(
            h_flex()
                .id(SharedString::from(format!("{prefix}-strip")))
                .flex_1()
                .min_w_0()
                .gap_1()
                .items_center()
                .overflow_x_hidden()
                .children(middle),
        )
        .when_some(results, |row, results| row.child(strip_divider(cx)).child(results))
        .into_any_element()
}

/// The strip's vertical divider before Results.
pub(crate) fn strip_divider(cx: &App) -> AnyElement {
    div().w(px(1.)).h(px(16.)).mx_1().bg(cx.theme().border).into_any_element()
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
    view: SessionChatView,
    issues: HashMap<String, Issue>,
    links: Vec<SessionIssueRow>,
    messages: HashMap<String, MessageRow>,
    pulls: Vec<PullRequestRow>,
}

impl ChatModel {
    fn linked_ids(&self) -> Vec<String> {
        self.view
            .tabs
            .iter()
            .filter_map(|tab| match tab {
                ChatTab::Issue { issue_id, nested, .. } => Some(
                    std::iter::once(issue_id.clone())
                        .chain(nested.iter().map(|item| item.issue_id.clone()))
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// The pull request the Guide reads and merges.
    fn pull(&self) -> Option<&PullRequestRow> {
        crate::chats_view::root_pull_request(&self.root.id, &self.pulls)
    }
}

fn chat_model(root_id: &str, focus: Option<&str>, cx: &App) -> Option<ChatModel> {
    // #EXP-1380: the view and the root's messages off ONE scan.
    let (view, own) =
        crate::session_data::chat_view_and_messages(root_id, focus, crate::session_data::now_ms(), false, cx)?;
    let root = crate::session_data::session(root_id, cx)?;
    let store = Store::try_global(cx)?;
    let links: Vec<SessionIssueRow> = store
        .collections()
        .session_issues
        .read(cx)
        .iter()
        .filter(|link| link.session_id == root_id)
        .cloned()
        .collect();
    let issues = crate::session_data::issues_of_root(root_id, cx)
        .into_iter()
        .map(|issue| (issue.id.clone(), issue))
        .collect();
    let messages = own.into_iter().map(|message| (message.id.clone(), message)).collect();
    Some(ChatModel {
        root,
        view,
        issues,
        links,
        messages,
        pulls: crate::session_data::pull_requests_of_root(root_id, cx),
    })
}

/// EXP-1346: how far beyond the viewport the Conversation renders and
/// measures rows (the session feed's `FEED_OVERDRAW`).
pub(crate) const CONVERSATION_OVERDRAW: gpui::Pixels = px(1024.);

/// EXP-1346: the Conversation list's projection ([`SessionChatScreenView::sync_list`]).
#[derive(Default)]
struct ConversationRows {
    /// Indices into the model's rows, in list order.
    rows: Vec<usize>,
    /// Whether the live tail is the last list row.
    tail: bool,
    /// What the list was last told.
    keys: Vec<RowKey>,
    /// The inputs the projection was built from (model generation, chip
    /// generation, may message, tail shown).
    synced: Option<(u64, u64, bool, bool)>,
    /// #EXP-1380: the WHOLE projection (indices into the model's rows);
    /// `rows` is its window from `start`.
    all: Vec<usize>,
    /// The window's first row in `all`.
    start: usize,
    /// The window (`transcript.window`, pinned once painted).
    window: crate::chat_transcript::TranscriptWindow,
    /// The indexes (into `all`) of WAITING plan / question cards.
    waiting: Vec<usize>,
    /// What the keys were built for (the inputs, the start).
    windowed: Option<((u64, u64, bool, bool), usize)>,
    /// #EXP-1736: the window's rows under transcript-grouping.json (one
    /// per `rows` entry): side, sender, avatar, name, gap.
    layouts: Vec<domain::transcript_grouping::TranscriptRowLayout>,
}

/// EXP-1346: a Conversation row's content identity (its measured height
/// moves with it): the row itself plus a message's timestamp (its caption)
/// and, for a row that may render an issue pill (a `#` in its text), the
/// chip generation — a pill's title comes from the issue, not the row, and
/// a retitled pill wraps differently.
pub(crate) fn conversation_fingerprint(row: &SessionChatRow, at: Option<&str>, chip_gen: u64) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::hash::DefaultHasher::new();
    let json = serde_json::to_string(row).unwrap_or_default();
    json.hash(&mut hasher);
    at.hash(&mut hasher);
    if json.contains('#') {
        chip_gen.hash(&mut hasher);
    }
    hasher.finish()
}

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

/// chats.json AGENT CHAT: the chat holds a plan: the one on show, else its tab's.
fn agent_chat_holds_plan(shown: Option<&SessionChatRow>, model: &ChatModel, kept: Option<&str>) -> bool {
    shown.is_some() || tab_plan(&model.view.rows, kept).is_some()
}

/// chats.json AGENT CHAT (`agent_chat_page`): whether an agent's chat keeps `child`.
fn agent_chat_shows(child: &ChatChildSel, holds_plan: bool) -> bool {
    let page = match child {
        ChatChildSel::Conversation => "conversation",
        ChatChildSel::Issue(_) => "issue",
        ChatChildSel::Results => "results",
        ChatChildSel::Plan(_) => "plan",
    };
    domain::session_kind::agent_chat_page(page, holds_plan) == page
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
    child: ChatChildSel,
    /// EXP-1346: shared, never deep-cloned per frame.
    model: Option<Rc<ChatModel>>,
    /// Bumped on every [`Self::refresh`]: what [`Self::sync_list`] diffs on.
    model_gen: u64,
    /// #EXP-1380: a [`Self::queue_refresh`] is pending.
    refresh_queued: bool,
    /// Bumped whenever [`Self::chip_cache`] is cleared (an issue or a status
    /// changed): the pills' titles moved, so the rows showing them
    /// re-measure ([`conversation_fingerprint`]).
    chip_gen: u64,
    /// The issue child's detail view (built on first show, re-pointed).
    issue_view: Option<Entity<crate::issue_detail::IssueDetailView>>,
    images: Entity<ImageCache>,
    /// THE session composer, targeting the ROOT: the session view over the
    /// root's live source (`session_screen::root_composer_view`), whose
    /// composer stack this screen paints under the conversation.
    composer: Entity<crate::steer_viewer::SteerSessionView>,
    /// N-12: THE Dock's state (the fan, the open sheet, the band height).
    dock: Entity<crate::dock::DockState>,
    /// When each issue link was FIRST rendered, and whether it was new then
    /// (linked inside [`NEW_CHILD_WINDOW`]) — the fade-in ring.
    first_seen: HashMap<String, (Instant, bool)>,
    opened_at: Instant,
    /// EXP-1346: the virtualised Conversation (the session feed's recipe,
    /// EXP-776): `gpui::list` renders and measures only the rows in and
    /// around the viewport; [`FollowMode::Tail`] = EXP-1316 (1n)'s follow
    /// (opens on the newest row, pinned until the reader scrolls up).
    list: ListState,
    /// The projection the list indexes into: indices into the model's rows
    /// (the docked asks left out), then the live tail row while it shows.
    conv: ConversationRows,
    /// The chip memo behind [`Self::text`] (cleared when the issues or
    /// statuses notify) and this frame's resolver over it.
    chip_cache: IssueChipCache,
    text: ChatText,
    focus_handle: FocusHandle,
    _ring_tick: Option<Task<()>>,
    /// EXP-1316 (1b): the session open IN PLACE (`Screen::Session` of this
    /// tree): its id and the screen view whose transcript the body hosts.
    focus: Option<(String, Entity<crate::session_screen::SessionScreenView>)>,
    _focus_sub: Option<Subscription>,
    /// EXP-1323: the live tail's relay viewer of `view.live_session_id`
    /// (the Conversation's Tasks card + status line), while it shows.
    tail: Option<(String, Entity<crate::session_screen::SessionScreenView>)>,
    _tail_sub: Option<Subscription>,
    /// NEW-6 (N-1): the chat's ONE session (`view.main_session_id`) while
    /// it is the main thing: its raw work is the body, in place.
    main: Option<(String, Entity<crate::session_screen::SessionScreenView>)>,
    _main_sub: Option<Subscription>,
    /// #EXP-1545 (dock.json CHAT QUEUE): one relay viewer per Live child
    /// of the tree, read for its Queue only (its per-session slots, seeded
    /// LOCAL FIRST from the last-known memo), while the Conversation shows.
    queue_feeds: HashMap<String, QueueFeed>,
    /// #EXP-1678: remote children read closed (their activeAt then, when).
    closed_queue_reads: HashMap<String, (Option<i64>, Instant)>,
    /// #EXP-1678: a probe re-read is scheduled.
    probe_timer: Option<gpui::Task<()>>,
    /// #EXP-1562: a waiting chip's card to scroll to (and ring) once the
    /// Conversation shows.
    reveal: Option<String>,
    /// #EXP-1662: the plan tab's body state and the plan it last painted (a
    /// new plan opens from its top).
    plan_state: crate::plan_page::PlanPageState,
    plan_painted: Option<String>,
    /// #FEED-191 (plan-page.json `kept`): the plan the tab showed during this
    /// visit of the chat; cleared when the person leaves it ([`Self::leave`]).
    plan_kept: Option<String>,
    _missing_tick: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl SessionChatScreenView {
    pub(crate) fn new(root_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let composer = crate::session_screen::root_composer_view(
            &root_id,
            SESSION_CHAT.copy.placeholder.clone(),
            crate::steer_viewer::ComposerIds::new(
                SESSION_CHAT.test_ids.composer.clone(),
                "chat-attach",
                "chat-model",
                "chat-context",
            ),
            window,
            cx,
        );
        // The composer repaints with its root's feed (the model pin, the
        // ring, Send⇄Stop, the queue strip).
        let mut subscriptions = vec![cx.observe(&composer, |_, _, cx| cx.notify())];
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
                this.chip_gen = this.chip_gen.wrapping_add(1);
                this.queue_refresh(cx);
            }));
            subscriptions.push(cx.observe(&collections.issue_statuses, |this, _, cx| {
                this.chip_cache.borrow_mut().clear();
                this.chip_gen = this.chip_gen.wrapping_add(1);
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
            child: ChatChildSel::Conversation,
            model: None,
            model_gen: 0,
            refresh_queued: false,
            chip_gen: 0,
            issue_view: None,
            images,
            composer,
            dock: cx.new(|_| crate::dock::DockState::default()),
            first_seen: HashMap::new(),
            opened_at: Instant::now(),
            list: {
                let list = ListState::new(0, ListAlignment::Top, CONVERSATION_OVERDRAW);
                list.set_follow_mode(FollowMode::Tail);
                list
            },
            conv: ConversationRows::default(),
            chip_cache: Default::default(),
            text: ChatText::default(),
            focus_handle: cx.focus_handle(),
            _ring_tick: None,
            focus: None,
            _focus_sub: None,
            tail: None,
            _tail_sub: None,
            main: None,
            _main_sub: None,
            queue_feeds: HashMap::new(),
            closed_queue_reads: HashMap::new(),
            probe_timer: None,
            reveal: None,
            plan_state: Default::default(),
            plan_painted: None,
            plan_kept: None,
            _missing_tick: missing_tick,
            _subscriptions: subscriptions,
        };
        this.refresh(cx);
        // A capture (and a debug launch) can open a chat on its Results tab.
        if std::env::var("EXP_DEV_CHAT_CHILD").ok().as_deref() == Some("results") {
            this.child = ChatChildSel::Results;
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
        let view = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = view.update(cx, |this, cx| {
                this.refresh_queued = false;
                this.refresh(cx);
            });
        });
    }

    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        let focus = self.focus.as_ref().map(|(id, _)| id.clone());
        let model = chat_model(&self.root_id, focus.as_deref(), cx);
        if let Some(model) = &model {
            self.note_seen(&model.links, cx);
            // An unlinked issue's child closes.
            if let ChatChildSel::Issue(issue_id) = &self.child {
                if !model.linked_ids().contains(issue_id) {
                    self.child = ChatChildSel::Conversation;
                }
            }
        }
        // Session 58cb32df: a surface card arriving while the chat is open
        // plays its entrance once.
        if let Some(model) = &model {
            crate::surface_card::sync_rows(cx.entity_id(), &model.view.rows, cx);
        }
        self.model = model.map(Rc::new);
        self.model_gen = self.model_gen.wrapping_add(1);
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

    /// Whether the issue link `link_id` still wears its NEW ring.
    fn ringed(&self, link_id: &str) -> bool {
        self.first_seen
            .get(link_id)
            .is_some_and(|(at, fresh)| *fresh && at.elapsed() < NEW_CHILD_WINDOW)
    }

    /// The tab closed: an open issue child flushes its edit (EXP-1323: and
    /// the live tail's viewer lets go of its relay socket).
    pub(crate) fn shutdown(&mut self, cx: &mut App) {
        if let Some((_, view)) = self.tail.take() {
            view.update(cx, |view, cx| view.shutdown(cx));
        }
        if let Some((_, view)) = self.main.take() {
            view.update(cx, |view, cx| view.shutdown(cx));
        }
        self._main_sub = None;
        for (_, feed) in self.queue_feeds.drain() {
            feed.view.update(cx, |view, _| view.shutdown());
        }
        if let (ChatChildSel::Issue(_), Some(view)) = (&self.child, &self.issue_view) {
            view.update(cx, |detail, cx| {
                detail.flush_title(cx);
                detail.flush_description(cx);
            });
        }
    }

    fn set_child(&mut self, child: ChatChildSel, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.child == child {
            return;
        }
        // Leaving an issue child flushes its edit (the close-tab rule).
        self.shutdown(cx);
        if let ChatChildSel::Issue(issue_id) = &child {
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
        self.child = child;
        cx.notify();
    }

    /// EXP-1316 (1b): show `focus`'s session IN PLACE (the panel calls this
    /// on a `Screen::Session` of this tree), or the chat again (`None`).
    pub(crate) fn set_focus(
        &mut self,
        focus: Option<(String, Entity<crate::session_screen::SessionScreenView>)>,
        cx: &mut gpui::Context<Self>,
    ) {
        let same = match (&self.focus, &focus) {
            (Some((a, view_a)), Some((b, view_b))) => a == b && view_a == view_b,
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
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

    /// EXP-1347: the issue ids of this chat's issue tabs (nested ones too).
    pub(crate) fn issue_tab_ids(&self) -> Vec<String> {
        self.model.as_ref().map(|model| model.linked_ids()).unwrap_or_default()
    }

    /// EXP-1347: an issue pill of one of this chat's issues selects its tab.
    pub(crate) fn open_issue_tab(&mut self, issue_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.select_child(ChatChildSel::Issue(issue_id), window, cx);
    }

    /// A strip chip other than a session: leave the in-place session (back
    /// to `Screen::Chat`, the same tab) and show `child`.
    fn select_child(&mut self, child: ChatChildSel, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.focus.is_some() {
            let session_id = self.root_id.clone();
            window.defer(cx, move |window, cx| {
                crate::navigation::navigate(window, cx, Screen::Chat { session_id });
            });
        }
        self.set_child(child, window, cx);
    }

    /// #EXP-1562: a waiting chip: show the Conversation (another tab
    /// open), then scroll its card to the centre and ring it
    /// ([`Self::take_reveal`]).
    fn select_waiting(&mut self, id: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let on_conversation = self.focus.is_none() && self.child == ChatChildSel::Conversation;
        if !on_conversation {
            self.select_child(ChatChildSel::Conversation, window, cx);
        }
        self.reveal = Some(id.to_string());
        cx.notify();
    }

    /// #EXP-1562: the pending reveal, once the Conversation is the body:
    /// THE list's row (the chat) or the ONE session's view (NEW-6).
    fn take_reveal(&mut self, model: &ChatModel, cx: &mut gpui::Context<Self>) {
        if self.focus.is_some() || self.child != ChatChildSel::Conversation {
            return;
        }
        let Some(id) = self.reveal.take() else { return };
        if let Some((_, view)) = self.main.clone() {
            let inner = view.read(cx).inner().clone();
            inner.update(cx, |inner, cx| inner.reveal_card(&id, cx));
            return;
        }
        let row = self.conv.rows.iter().position(|&index| model.view.rows.get(index).is_some_and(|row| row.id() == id));
        match row {
            Some(ix) => crate::waiting_cards::reveal_in_list(&self.list, ix, &id, cx),
            // Not in the list (yet): the ring alone.
            None => crate::waiting_cards::highlight(&id, cx),
        }
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

    fn render_header(&mut self, model: &ChatModel, window: &Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        // EXP-1316: the trailing slot on EVERY tab — the PR's pill, then the
        // Guide `…`. #EXP-1710 (merge-train.json STATUS PILL): the pill is
        // the PR's merge STATUS (any viewer; a closed PR has none), its tap
        // the Checks list; approval mode's `Needs approval` is its only
        // action (members).
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
        // EXP-1686 (chats.json AGENT CHAT): an agent's chat wears THE agent
        // header (the disc, its NAME, call, the agent menu); the strip only
        // while it holds a plan.
        // #FEED-191 (chats.json THE WAY BACK): a session link opened this: the
        // Back, strip or not; on the plan tab the Conversation tab is the way.
        let on_plan = self.focus.is_none() && matches!(self.child, ChatChildSel::Plan(_));
        let back = (!on_plan).then(|| crate::session_screen::session_link_back(window, cx)).flatten();
        if let Some(agent_id) = conversation_agent(model) {
            let agent = crate::session_data::agent_def(agent_id, cx);
            let panel = self.call_panel.read(cx).focus_handle(cx);
            let strip = agent_chat_holds_plan(self.shown_plan(model, cx).as_ref(), model, self.plan_kept.as_deref()).then(|| self.render_strip(model, window, cx));
            return crate::agent_screen::agent_header(agent.as_ref(), merge.map(IntoElement::into_any_element), back, strip, panel, cx);
        }
        // EXP-1316: the title is derived, plain text (no rename).
        let title = title_row(&model.view.title, model.view.private, cx).id("chat-title");
        // S-3 (owner 2026-10-28): no Stop pill; the composer's stop-turn
        // is the one stop.
        let menu = guide_menu(model, cx);
        // The rule under the strip spans the content card (web `border-b`), so
        // it meets the plan tab's outline hairline.
        v_flex()
            .w_full()
            .child(
                v_flex()
                    .w_full()
                    .px(px(HEADER_INSET))
                    .pt_6()
                    .pb_3()
                    .gap_4()
                    .child(
                        v_flex()
                            .w_full()
                            .min_w_0()
                            .gap_1()
                            .children(back)
                            .child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .gap_2()
                                    .items_center()
                                    .child(div().flex_1().min_w_0().child(title))
                                    .children(merge)
                                    .children(menu),
                            ),
                    )
                    .child(self.render_strip(model, window, cx)),
            )
            .child(div().w_full().h(px(1.)).bg(cx.theme().border))
            .into_any_element()
    }

    /// EXP-1334: THE chat strip ([`chat_strip`]) over the view's tabs.
    fn render_strip(&mut self, model: &ChatModel, window: &Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let member = chat_member(model, cx);
        let open = match (self.focus_id(), &self.child) {
            (Some(focus), _) => StripPick::Session(focus.to_string()),
            (None, ChatChildSel::Conversation) => StripPick::Conversation,
            (None, ChatChildSel::Issue(id)) => StripPick::Issue(id.clone()),
            (None, ChatChildSel::Results) => StripPick::Results,
            (None, ChatChildSel::Plan(_)) => StripPick::Plan,
        };
        let issues: HashMap<String, StripIssue> = model
            .issues
            .values()
            .map(|issue| {
                let link_id = model
                    .links
                    .iter()
                    .find(|link| link.issue_id == issue.id)
                    .map(|link| link.id.clone())
                    .unwrap_or_default();
                (
                    issue.id.clone(),
                    StripIssue {
                        title: issue.title.clone(),
                        status: Some(crate::queries::resolve_issue_status(cx, issue)),
                        ringed: self.ringed(&link_id).then_some(link_id),
                    },
                )
            })
            .collect();
        let clis: HashMap<String, Option<String>> = crate::session_data::tree_of_root(&self.root_id, cx)
            .into_iter()
            .map(|session| (session.id, session.cli))
            .collect();
        // chats.json AGENT CHAT: Conversation and the plan, nothing else.
        let agent_chat = conversation_agent(model).is_some();
        let this = cx.entity().downgrade();
        let on_pick: OnStripPick = std::rc::Rc::new(move |pick, window, cx| match pick {
            StripPick::Session(session_id) => open_session_in_place(session_id, window, cx),
            other => {
                let child = match other {
                    StripPick::Issue(id) => ChatChildSel::Issue(id),
                    StripPick::Results => ChatChildSel::Results,
                    StripPick::Plan => ChatChildSel::Plan(None),
                    _ => ChatChildSel::Conversation,
                };
                let _ = this.update(cx, |this, cx| {
                    // The chip of the open plan tab keeps the plan it shows.
                    let child = match (&child, &this.child) {
                        (ChatChildSel::Plan(_), open @ ChatChildSel::Plan(_)) => open.clone(),
                        _ => child,
                    };
                    this.select_child(child, window, cx);
                });
            }
        });
        chat_strip(
            ChatStrip {
                prefix: "chat",
                results: !agent_chat,
                plan: self
                    .shown_plan(model, cx)
                    .or_else(|| tab_plan(&model.view.rows, self.plan_kept.as_deref()).cloned())
                    .as_ref()
                    .and_then(strip_plan_of),
                waiting: if agent_chat { Vec::new() } else { crate::waiting_cards::chat_waiting_cards(&model.view.rows) },
                on_waiting: (!agent_chat).then(|| {
                    let this = cx.entity().downgrade();
                    std::rc::Rc::new(move |id: &str, window: &mut Window, cx: &mut App| {
                        let _ = this.update(cx, |this, cx| this.select_waiting(id, window, cx));
                    }) as _
                }),
                tabs: model.view.tabs.clone(),
                issues,
                clis,
                open,
                menu: (member && !agent_chat).then(|| self.menu_target(None)),
                on_pick,
            },
            window,
            cx,
        )
    }

    fn render_row(
        &self,
        row: &SessionChatRow,
        layout: Option<&domain::transcript_grouping::TranscriptRowLayout>,
        model: &ChatModel,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        // #EXP-1736: ONE transcript for every chat: a message (or a
        // report) = THE bubble under its sender, every card as it was.
        let at = model.messages.get(row.id()).and_then(|message| message.created_at.as_deref());
        let content = match transcript_message(row, layout, "chat-message", at, &self.text, &self.images, cx) {
            Some(bubble) => bubble,
            None => card_row(row, &self.root_id, &model.view.rows, &self.text, &self.images, window, cx)
                .unwrap_or_else(|| div().into_any_element()),
        };
        sender_row(layout, content, cx)
    }

    /// EXP-1323: keep ONE relay viewer on `live` (the view's
    /// `live_session_id`) while the Conversation shows; `None` drops it. A
    /// REMOTE viewer always — it never takes a local engine's handle.
    fn sync_tail(&mut self, live: Option<&str>, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.tail.as_ref().map(|(id, _)| id.as_str()) == live {
            return;
        }
        if let Some((_, view)) = self.tail.take() {
            view.update(cx, |view, cx| view.shutdown(cx));
        }
        self._tail_sub = None;
        if let Some(live) = live {
            let id = live.to_string();
            let view = cx.new(|cx| crate::session_screen::SessionScreenView::remote(id.clone(), window, cx));
            self._tail_sub = Some(cx.observe(&view, |_, _, cx| cx.notify()));
            self.tail = Some((id, view));
        }
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
        let root_open = self.composer.read(cx).dock_queue().is_some();
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
            open: self.composer.read(cx).dock_queue().is_some(),
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

    /// NEW-6 (N-1): keep the ONE session's view while it is the body.
    fn sync_main(&mut self, main: Option<&str>, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.main.as_ref().map(|(id, _)| id.as_str()) == main {
            return;
        }
        if let Some((_, view)) = self.main.take() {
            view.update(cx, |view, cx| view.shutdown(cx));
        }
        self._main_sub = None;
        if let Some(main) = main {
            let id = main.to_string();
            let view = cx.new(|cx| crate::session_screen::SessionScreenView::new(id.clone(), window, cx));
            self._main_sub = Some(cx.observe(&view, |_, _, cx| cx.notify()));
            self.main = Some((id, view));
        }
    }

    /// EXP-1323: the live tail after the last row ([`sync_tail`]'s viewer).
    fn render_tail(&self, window: &Window, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let (_, view) = self.tail.as_ref()?;
        let inner = view.read(cx).inner().clone();
        inner.update(cx, |inner, cx| inner.live_tail(window, cx))
    }

    /// EXP-1346: refresh the Conversation's projection and tell the list
    /// what changed — only when its inputs moved (the model, whether the
    /// reader may reply = whether the asks dock, the live tail), never per
    /// frame. Rows key by POSITION (ids ascend, [`plan_list_sync`]'s rule)
    /// with a content fingerprint, so an append is one splice and a row
    /// that changed (or slid under another) re-measures.
    fn sync_list(&mut self, model: &ChatModel, may_message: bool, cx: &App) {
        // #EXP-1464: an expanded / collapsed session row re-measures.
        let row_gen = chat_row_generation(cx);
        let inputs = (self.model_gen, self.chip_gen ^ row_gen.rotate_left(32), may_message, self.tail.is_some());
        if self.conv.synced != Some(inputs) {
            self.conv.synced = Some(inputs);
            // FEED-140: a waiting plan / question stays inline (with its
            // actions) besides THE Dock; only the open suggestions leave.
            let docked = if may_message {
                docked_ids(&model.view.rows)
            } else {
                HashSet::new()
            };
            self.conv.all = model
                .view
                .rows
                .iter()
                .enumerate()
                .filter(|(_, row)| !docked.contains(row.id()))
                .map(|(index, _)| index)
                .collect();
            self.conv.waiting =
                crate::chat_transcript::waiting_indexes(self.conv.all.iter().map(|&index| &model.view.rows[index]));
            self.conv.tail = self.tail.is_some();
        }
        // #EXP-1380 WINDOW: the newest rows (up to the oldest waiting
        // card), pinned once painted; only the window is fingerprinted and
        // measured.
        let total = self.conv.all.len();
        let start = self.conv.window.start(total, &self.conv.waiting);
        if self.conv.windowed == Some((inputs, start)) {
            return;
        }
        self.conv.windowed = Some((inputs, start));
        self.conv.start = start;
        self.conv.rows = self.conv.all[start..].to_vec();
        let chip_gen = self.chip_gen;
        // #EXP-1736: who sent each row, the groups, the gaps; the S-3
        // origin sends the conversation's first message (when the window
        // holds it: no `Load earlier` above).
        let origin = if model.view.focus_id.is_none() { model.view.origin.as_ref() } else { None };
        let window_rows: Vec<&SessionChatRow> = self.conv.rows.iter().map(|&index| &model.view.rows[index]).collect();
        let mut input = conversation_input(
            window_rows.iter().copied(),
            &model.view.root_id,
            &model.view.title,
            model.root.agent_id.as_deref(),
            origin,
            start == 0,
            cx,
        );
        input.items = window_rows.iter().map(|row| transcript_item(row)).collect();
        drop(window_rows);
        self.conv.layouts = domain::transcript_grouping::transcript_layout(&input);
        let layouts = &self.conv.layouts;
        let mut keys: Vec<RowKey> = self
            .conv
            .rows
            .iter()
            .enumerate()
            .map(|(offset, &index)| {
                let row = &model.view.rows[index];
                let at = match row {
                    SessionChatRow::Message { id, .. } => {
                        model.messages.get(id).and_then(|message| message.created_at.as_deref())
                    }
                    _ => None,
                };
                let mut fingerprint =
                    conversation_fingerprint(row, at, chip_gen) ^ row_state_fingerprint(row, &model.view.root_id, cx);
                // The first row carries `Load earlier` while rows hide above.
                if offset == 0 && start > 0 {
                    fingerprint = fingerprint.rotate_left(1) ^ 1;
                }
                // #EXP-1736: the avatar, the name and the gap move its height.
                if let Some(layout) = layouts.get(offset) {
                    fingerprint ^= layout_fingerprint(layout);
                }
                // Absolute positions: a widened window is a front splice.
                RowKey { id: (start + offset) as u64, fingerprint }
            })
            .collect();
        if self.conv.tail {
            keys.push(RowKey {
                id: WORKING_ROW_ID,
                fingerprint: 0,
            });
        }
        let ops = plan_list_sync(&self.conv.keys, &keys);
        // #EXP-1380 FOLLOW: an arrival keeps a reader inside the slack on
        // the tail.
        crate::chat_transcript::rearm_on_arrival(&self.list, &ops);
        for op in ops {
            match op {
                ListOp::Splice { range, count } => self.list.splice(range, count),
                ListOp::Remeasure(range) => self.list.remeasure_items(range),
            }
        }
        self.conv.keys = keys;
    }

    /// One Conversation list row: the reading column (the old column's
    /// measure and gutter), the row gap above every row but the first.
    fn render_list_row(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        // #EXP-1736: the grouping's gap above every row (the live tail
        // starts a group of its own).
        let layout = self.conv.rows.get(ix).and_then(|_| self.conv.layouts.get(ix).cloned());
        let gap = match (&layout, ix) {
            (_, 0) => 0.,
            (Some(layout), _) => layout.gap,
            (None, _) => transcript_sizes().gap_between,
        };
        let content = match (self.conv.rows.get(ix), self.model.clone()) {
            (Some(&index), Some(model)) => match model.view.rows.get(index) {
                Some(row) => Some(self.render_row(row, layout.as_ref(), &model, window, cx)),
                None => None,
            },
            // EXP-1323: the live tail after the last row.
            _ => self.render_tail(window, cx),
        };
        let Some(content) = content else {
            return div().into_any_element();
        };
        // #EXP-1380 WINDOW: `Load earlier` over the window's first row.
        let earlier = (ix == 0 && self.conv.start > 0).then(|| {
            crate::chat_transcript::load_earlier_button(
                cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.conv.window.load_earlier();
                    this.list.pause_following_tail();
                    cx.notify();
                }),
                cx,
            )
        });
        div()
            .w_full()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(CONVERSATION_W))
                    .mx_auto()
                    .px(px(crate::work_header::WORK_GUTTER))
                    .pt(px(gap))
                    .children(earlier)
                    .child(content),
            )
            .into_any_element()
    }

    /// The Conversation's jump-to-bottom button (#EXP-1380: past
    /// `transcript.followSlack`, [`crate::chat_transcript::jump_overlay`]).
    fn render_jump_to_bottom(&self, band: f32, cx: &mut gpui::Context<Self>) -> AnyElement {
        crate::chat_transcript::jump_overlay(
            "chat-conversation-scroll-jump-to-bottom",
            &self.list,
            self.conv.keys.len(),
            band,
            cx.entity_id(),
            cx,
        )
    }

    fn render_conversation(&mut self, model: &ChatModel, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        // N-12: THE Dock (dock.json ×4) floats over the Conversation's
        // bottom edge: the tree's waiting cards, the root's Queue, queued
        // messages and banners, then THE composer; a reader who may not
        // reply keeps the cards inline and gets no band.
        let may_message = self.composer.read(cx).may_message();
        let queue_from = if may_message { self.chat_queue_from(model, cx) } else { None };
        let (band, sheet) = if may_message {
            root_dock(
                "chat",
                &model.view.rows,
                Some(&self.root_id),
                &self.dock,
                &self.composer,
                queue_from.as_ref().map(|(view, name)| (view, name.as_str())),
                &self.text,
                window,
                cx,
            )
        } else {
            (None, None)
        };
        self.sync_list(model, may_message, cx);
        self.take_reveal(model, cx);
        let band_height = if may_message { self.dock.read(cx).band_height() } else { 0. };
        let list = gpui::list(
            self.list.clone(),
            cx.processor(|this, ix: usize, window, cx| this.render_list_row(ix, window, cx)),
        )
        .pt_8()
        .pb(px(band_height.max(16.) + crate::dock::DOCK_GAP));
        // EXP-1228: the transcript's right-click menu (Copy) over a chat's
        // transcript and an agent's chat bubbles alike, THE raw session
        // view's host; an image or media tile's own menu claims the press.
        let scroller = crate::steer_viewer::with_transcript_menu(div())
            .id(SharedString::from(SESSION_CHAT.test_ids.conversation.clone()))
            .debug_selector(|| SESSION_CHAT.test_ids.conversation.clone())
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .child(crate::scroll_pane::v_list_pane(list, &self.list))
            .children(band)
            .child(self.render_jump_to_bottom(band_height, cx))
            .children(sheet);
        v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(scroller)
            .into_any_element()
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

    /// #EXP-1662: the plan the open plan tab shows: the one a row opened (a
    /// grandchild's card is not among the root's rows), else the tab's target.
    fn shown_plan(&self, model: &ChatModel, cx: &App) -> Option<SessionChatRow> {
        let ChatChildSel::Plan(opened) = &self.child else {
            return None;
        };
        let opened = opened.as_deref().and_then(|id| {
            model.view.rows.iter().find(|row| row.id() == id).cloned().or_else(|| {
                let store = Store::try_global(cx)?;
                let message = store.collections().messages.read(cx).get(id).cloned()?;
                domain::session_tree::ask_card_row(&message).filter(|row| matches!(row, SessionChatRow::Plan { .. }))
            })
        });
        opened.or_else(|| tab_plan(&model.view.rows, self.plan_kept.as_deref()).cloned())
    }

    /// #FEED-191 (plan-page.json `kept`): the plan the tab showed during THIS
    /// visit: a plan a row opened, else the tab's target whenever it has one.
    fn hold_plan(&mut self, model: &ChatModel) {
        if let ChatChildSel::Plan(Some(opened)) = &self.child {
            self.plan_kept = Some(opened.clone());
        }
        if let Some(target) = tab_plan(&model.view.rows, self.plan_kept.as_deref()) {
            self.plan_kept = Some(target.id().to_string());
        }
    }

    /// #FEED-191: the person left this chat (the pane shows another one, or
    /// none): a decided plan leaves the strip on the next visit.
    pub(crate) fn leave(&mut self) {
        self.plan_kept = None;
    }

    /// #EXP-1662: THE plan tab's body: only the content swaps, on the chat's
    /// own ground, no composer.
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
        // EXP-1686 (chats.json AGENT CHAT): an agent's chat shows the conversation
        // and its plan: a stale Results (or issue) selection falls back.
        let agent_chat = conversation_agent(&model).map(str::to_string);
        if agent_chat.is_some()
            && !agent_chat_shows(&self.child, agent_chat_holds_plan(self.shown_plan(&model, cx).as_ref(), &model, self.plan_kept.as_deref()))
        {
            self.set_child(ChatChildSel::Conversation, window, cx);
        }
        self.call_panel.update(cx, |panel, cx| panel.set_agent(agent_chat.clone(), cx));
        // A new issue filed into this chat lands on its child once its
        // link has synced.
        let pending = cx
            .try_global::<PendingChatChild>()
            .and_then(|pending| pending.0.get(&self.root_id).cloned())
            .filter(|issue_id| model.linked_ids().contains(issue_id));
        if let Some(issue_id) = pending {
            cx.global_mut::<PendingChatChild>().0.remove(&self.root_id);
            self.set_child(ChatChildSel::Issue(issue_id), window, cx);
        }
        // #EXP-1662: a way in asked for this chat's plan tab on a plan.
        let pending_plan = cx.try_global::<PendingChatPlan>().and_then(|pending| pending.0.get(&self.root_id).cloned());
        if let Some(message_id) = pending_plan {
            cx.global_mut::<PendingChatPlan>().0.remove(&self.root_id);
            self.set_child(ChatChildSel::Plan(Some(message_id)), window, cx);
        }
        // #FEED-191 (needs-you.json): a Needs you row asked for this chat.
        let opening = cx.try_global::<PendingChatOpening>().and_then(|pending| pending.0.get(&self.root_id).cloned());
        if let Some(opening) = opening {
            cx.global_mut::<PendingChatOpening>().0.remove(&self.root_id);
            match opening {
                ChatOpening::Card(message_id) => self.select_waiting(&message_id, window, cx),
                ChatOpening::Results if agent_chat.is_none() => self.select_child(ChatChildSel::Results, window, cx),
                ChatOpening::Results => {}
            }
        }
        // #FEED-191 (plan-page.json): a decision taken on the plan tab landed:
        // back to the Conversation, where the agent works on.
        if cx.try_global::<PlanTabDecisions>().is_some_and(|decisions| decisions.landed.contains(&self.root_id)) {
            cx.global_mut::<PlanTabDecisions>().landed.remove(&self.root_id);
            if matches!(self.child, ChatChildSel::Plan(_)) {
                self.set_child(ChatChildSel::Conversation, window, cx);
            }
        }
        // #FEED-191: the plan this visit keeps on the strip.
        self.hold_plan(&model);
        // Nothing left to show on the plan tab: the Conversation.
        if matches!(self.child, ChatChildSel::Plan(_)) && self.shown_plan(&model, cx).is_none() {
            self.set_child(ChatChildSel::Conversation, window, cx);
        }
        // EXP-1346: ONE memoised chip resolver per frame for every text.
        self.text = ChatText::memoized(model.root.team_id.as_deref(), Some(self.chip_cache.clone()))
            .answering(self.composer.read(cx).may_message());
        // NEW-6 (N-1): a chat with ONE session = that session's raw work
        // in place of the Conversation (its composer, Stop in the header).
        let on_conversation = self.focus.is_none() && self.child == ChatChildSel::Conversation;
        // S-3 (chats.json CHAT MODE): an agent conversation always paints
        // its bubbles, never the root's raw work in place.
        let bubbles = domain::session_tree::chat_mode(model.root.agent_id.as_deref())
            == domain::session_tree::ChatMode::Bubbles;
        let main = (on_conversation && !bubbles).then(|| model.view.main_session_id.clone()).flatten();
        self.sync_main(main.as_deref(), window, cx);
        let header = self.render_header(&model, window, cx);
        // EXP-1323: the live tail's viewer rides the Conversation only.
        let live = (on_conversation && main.is_none())
            .then(|| model.view.live_session_id.clone())
            .flatten();
        self.sync_tail(live.as_deref(), window, cx);
        // #EXP-1545: the Live children's Queues ride the Conversation's Dock.
        let children = if on_conversation && main.is_none() && self.child == ChatChildSel::Conversation {
            live_children(&model.view, &model.root, cx)
        } else {
            Vec::new()
        };
        self.sync_queue_feeds(children, window, cx);
        // EXP-1323: what is on screen is read — the chat (the whole tree's
        // agent replies) or the session open in place (its own).
        match self.focus_id().map(str::to_string) {
            Some(focus) => crate::session_data::mark_read_if_unread(&focus, false, cx),
            None => crate::session_data::mark_read_if_unread(&self.root_id, true, cx),
        }
        // EXP-1316 (1b): a session open in place = its raw work under the
        // chat's own header + strip (no session header of its own).
        if let Some((_, view)) = self.focus.clone().or_else(|| self.main.clone()) {
            // #EXP-1562: a chip's reveal on the ONE session's raw work.
            self.take_reveal(&model, cx);
            let inner = view.read(cx).inner().clone();
            // #EXP-1736: the view's S-3 origin = the sender of the prompt
            // that started it (its bubble replaces the `Started by` line).
            let origin = model.view.origin.clone();
            if inner.read(cx).transcript_origin_xref() != origin.as_ref() {
                inner.update(cx, |inner, cx| inner.set_transcript_origin(origin, cx));
            }
            let scrolled = inner.read(cx).body_scrolled();
            // #VAPP-229: the in-place session's `via <app>` line under the
            // chat header (which stays caption-free).
            let via_app = crate::session_screen::via_app_line(inner.read(cx).session_row(), cx)
                .map(|line| {
                    div()
                        .w_full()
                        .child(
                            div()
                                .w_full()
                                .max_w(px(CONVERSATION_W))
                                .mx_auto()
                                .px(px(crate::work_header::WORK_GUTTER))
                                .pt_1()
                                .child(line),
                        )
                        .into_any_element()
                });
            return v_flex()
                .track_focus(&self.focus_handle)
                .size_full()
                .min_h_0()
                .overflow_hidden()
                .child(header)
                .children(via_app)
                .child(crate::work_header::work_body(
                    div().flex_1().min_h_0().child(inner).into_any_element(),
                    false,
                    scrolled,
                ));
        }
        let child = match self.child.clone() {
            ChatChildSel::Conversation => self.render_conversation(&model, window, cx),
            ChatChildSel::Issue(_) => self.render_issue_child(),
            ChatChildSel::Results => self.render_results_child(&model, window, cx),
            ChatChildSel::Plan(_) => match self.shown_plan(&model, cx) {
                Some(row) => self.render_plan_child(&row, window, cx),
                None => div().flex_1().into_any_element(),
            },
        };
        // EXP-1686: the call panel covers an agent chat's body while it runs.
        let child = match agent_chat {
            Some(_) => div()
                .relative()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(child)
                .child(self.call_panel.clone())
                .into_any_element(),
            None => child,
        };
        v_flex()
            .track_focus(&self.focus_handle)
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .child(header)
            .child(child)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The "+" offers Link an issue and New issue; an issue chip adds its
    /// Unlink after a separator. No "start a session" row any more.
    fn plan_row_of(id: &str, decision: Option<&str>) -> SessionChatRow {
        SessionChatRow::Plan {
            id: id.into(),
            session_id: "s".into(),
            plan: format!("# {id}"),
            questions: Vec::new(),
            surface: None,
            decision: decision.map(Into::into),
            fresh: false,
            answers: None,
            feedback: None,
            surface_data: None,
        }
    }

    /// chats.json AGENT CHAT: the bug of #EXP-1686 × #EXP-1662: the fallback sent
    /// the open plan tab back to the Conversation on the next frame.
    #[test]
    fn an_agent_chat_keeps_its_plan_tab_open() {
        assert!(agent_chat_shows(&ChatChildSel::Plan(None), true));
        assert!(agent_chat_shows(&ChatChildSel::Plan(Some("m1".into())), true));
        assert!(agent_chat_shows(&ChatChildSel::Conversation, true));
        assert!(agent_chat_shows(&ChatChildSel::Conversation, false));
        assert!(!agent_chat_shows(&ChatChildSel::Plan(None), false));
        assert!(!agent_chat_shows(&ChatChildSel::Results, true));
        assert!(!agent_chat_shows(&ChatChildSel::Issue("i".into()), true));
    }

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

    /// chats.json AGENT CHAT, the whole path: an agent posts a plan, a way in
    /// opens it: the tab stays open across frames with the strip up, the person
    /// switches back and forth, a decision keeps it, a superseded plan leaves.
    #[gpui::test]
    fn an_agent_chats_plan_stays_open_with_its_strip(cx: &mut gpui::TestAppContext) {
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
        let tab: &'static str = domain::plan_page::PLAN_PAGE.test_ids.tab.as_str();
        let draw = |cx: &mut gpui::VisualTestContext| {
            for _ in 0..3 {
                cx.update(|window, cx| window.draw(cx).clear(cx));
                cx.run_until_parked();
            }
        };
        draw(cx);
        // The Conversation first, the strip already up: there is a plan to switch to.
        view.read_with(cx, |view, _| assert_eq!(view.child, ChatChildSel::Conversation));
        assert!(cx.debug_bounds(tab).is_some(), "the strip holds the plan tab");

        // A way in opens it, and it stays.
        cx.update(|_, cx| request_chat_plan("r1", "m1", cx));
        cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
        draw(cx);
        view.read_with(cx, |view, _| assert_eq!(view.child, ChatChildSel::Plan(Some("m1".into()))));
        assert!(cx.debug_bounds(tab).is_some());

        // Back to the Conversation and into the plan again.
        cx.update(|window, cx| view.update(cx, |view, cx| view.select_child(ChatChildSel::Conversation, window, cx)));
        draw(cx);
        view.read_with(cx, |view, _| assert_eq!(view.child, ChatChildSel::Conversation));
        cx.update(|window, cx| view.update(cx, |view, cx| view.select_child(ChatChildSel::Plan(None), window, cx)));
        draw(cx);
        view.read_with(cx, |view, _| assert_eq!(view.child, ChatChildSel::Plan(None)));

        // Answering keeps the plan on show, its tab still there.
        cx.update(|_, cx| seed_plan(Some("approved"), cx));
        draw(cx);
        view.read_with(cx, |view, _| assert_eq!(view.child, ChatChildSel::Plan(None)));
        assert!(cx.debug_bounds(tab).is_some());

        // Superseded: nothing to switch to, the Conversation and no strip.
        cx.update(|_, cx| seed_plan(Some("superseded"), cx));
        draw(cx);
        view.read_with(cx, |view, _| assert_eq!(view.child, ChatChildSel::Conversation));
        assert!(cx.debug_bounds(tab).is_none());

        // A stale Results selection never survives in an agent's chat.
        cx.update(|window, cx| view.update(cx, |view, cx| view.select_child(ChatChildSel::Results, window, cx)));
        draw(cx);
        view.read_with(cx, |view, _| assert_eq!(view.child, ChatChildSel::Conversation));
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

    /// EXP-1228: a right press on an agent chat's bubbles opens THE
    /// transcript menu (Copy), the raw session view's host; it never did on
    /// the chat screen.
    #[gpui::test]
    fn a_right_press_on_an_agent_chat_opens_the_transcript_menu(cx: &mut gpui::TestAppContext) {
        seed_agent_chat("Hey, what's up?", cx);
        let (_view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
        for _ in 0..3 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.run_until_parked();
        }
        let bounds = cx
            .debug_bounds(SESSION_CHAT.test_ids.conversation.as_str())
            .expect("the conversation paints");
        let opens = || crate::steer_viewer::TRANSCRIPT_MENU_OPENS.with(|opens| opens.get());
        let before = opens();
        cx.simulate_mouse_down(
            bounds.origin + gpui::point(px(40.), px(40.)),
            gpui::MouseButton::Right,
            gpui::Modifiers::default(),
        );
        assert_eq!(opens(), before + 1, "the right press opens the transcript menu");
    }

    /// EXP-1739: real chat text (bold, code spans, paragraphs, a list) swept
    /// with the pointer in the chat screen, under the window's selection
    /// layer and the shell's cached dock: a right press hands the selection
    /// to Copy, a code span as its code (it paints over a placeholder of
    /// non-breaking spaces, which copied as blank: Copy stayed disabled).
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
        seed_agent_chat(
            "**Package repos:** yes. `pkg.exponential.at` is a small server.\n\nInstall with `curl -fsSL https://exponential.at/desktop/setup.sh | sudo sh`.\n\n- **Phone writes:** live test still running.",
            cx,
        );
        let (_shell, cx) = cx.add_window_view(|window, cx| Shell(cx.new(|cx| SessionChatScreenView::new("r1".to_string(), window, cx))));
        let cx: &mut VisualTestContext = cx;
        for _ in 0..3 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
        let bounds = cx.debug_bounds(SESSION_CHAT.test_ids.conversation.as_str()).expect("the conversation paints");
        let at = |x: f32, y: f32| bounds.origin + gpui::point(px(x), px(y));
        // #EXP-1736: the bubble sits under its sender's avatar + name line, so
        // the sweeps measure from the bubble's own text block.
        let body = cx.debug_bounds(MESSAGE_BUBBLE_BODY).expect("the bubble's text paints");
        let text = |x: f32, y: f32| body.origin + gpui::point(px(x), px(y));
        // Right-click Copy's text after sweeping `from` → `to` ("" = disabled).
        let copy_after_sweep = |from: gpui::Point<Pixels>, to: gpui::Point<Pixels>, cx: &mut VisualTestContext| {
            let off = at(1800., 900.);
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
        // The bubble's lines: the first paragraph, the second, the list item.
        // #EXP-1736: the bubble's width cap may wrap the long code line, so
        // the sweep walks down the second paragraph until it hits the span
        // (`Install with ` is ~13 characters, the span starts right after).
        let first = 10.;
        let prose = copy_after_sweep(text(0., first), text(144., first), cx);
        assert!(prose.starts_with("Package"), "prose: {prose:?}");
        let mut code = String::new();
        for step in 0..16 {
            let y = 24. + step as f32 * 4.;
            code = copy_after_sweep(text(110., y), text(150., y), cx);
            if !code.is_empty() {
                break;
            }
        }
        assert_eq!(code, "curl -fsSL https://exponential.at/desktop/setup.sh | sudo sh", "a code span copies its code");
        let all = copy_after_sweep(text(0., first), text(f32::from(body.size.width) - 2., f32::from(body.size.height) - 8.), cx);
        for part in ["Package repos:", "pkg.exponential.at is a small server.", "Install with curl -fsSL", "Phone writes: live test"] {
            assert!(all.contains(part), "multi-paragraph Copy carries {part:?}: {all:?}");
        }
        assert!(!all.contains('\u{a0}'), "no placeholder spaces: {all:?}");
    }

    /// chats.json THE WAY BACK, the desktop's whole path: an agent's chat (no
    /// plan, so no strip), a card opening a chat it started and one opening
    /// its own child: each lands with a Back to the chat; the same screens
    /// reached another way wear none, nor does a session opened in place.
    #[gpui::test]
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

        // The plan tab's way back is the Conversation tab.
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
        cx.update(|window, cx| view.update(cx, |view, cx| view.select_child(ChatChildSel::Plan(None), window, cx)));
        draw(cx);
        assert!(cx.debug_bounds(domain::plan_page::PLAN_PAGE.test_ids.tab.as_str()).is_some());
        assert!(cx.debug_bounds(BACK).is_none(), "the plan tab: no Back");
        cx.update(|window, cx| view.update(cx, |view, cx| view.select_child(ChatChildSel::Conversation, window, cx)));
        draw(cx);
        assert!(cx.debug_bounds(BACK).is_some());
    }

    /// chats.json THE WAY BACK: a chat with no strip, reached through a link
    /// (a cross-session bubble, #EXP-1736), wears the Back in THE agent header.
    #[gpui::test]
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
        assert!(cx.debug_bounds(domain::plan_page::PLAN_PAGE.test_ids.tab.as_str()).is_none(), "no strip");
        cx.update(|window, cx| {
            crate::navigation::nav_for_window(window, cx);
            crate::navigation::navigate(window, cx, Screen::Chat { session_id: "other".into() });
            crate::navigation::open_session_link("chat", window, cx);
        });
        draw(cx);
        assert!(cx.debug_bounds("chat-back-link").is_some(), "the Back with no strip");
    }

    /// #EXP-1662: the chip targets `plan_tab_target` over the chat's rows and
    /// wears the dot while that plan waits.
    #[test]
    fn the_plan_chip_follows_the_tab_target() {
        let rows = [plan_row_of("old", Some("approved")), plan_row_of("new", None), plan_row_of("done", Some("rejected"))];
        let target = tab_plan(&rows, None).unwrap();
        assert_eq!(target.id(), "new");
        let chip = strip_plan_of(target).unwrap();
        assert_eq!(chip.label, "Plan: new");
        assert!(chip.waiting);
        assert_eq!(chip.glyph.icon, "coding-plan");
        assert!(tab_plan(&[plan_row_of("gone", Some("superseded"))], Some("gone")).is_none());
        // #FEED-191: a decided plan leaves the strip unless this visit keeps it,
        // and an approved one leads with the check.
        let decided = [plan_row_of("old", Some("approved"))];
        assert!(tab_plan(&decided, None).is_none());
        let kept = strip_plan_of(tab_plan(&decided, Some("old")).unwrap()).unwrap();
        assert_eq!((kept.glyph.icon, kept.glyph.tone, kept.waiting), ("ui-check", "approved", false));
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
}
