//! EXP-1294 — the THREAD screen ([`Screen::Thread`]): a thread IS a root
//! session (kind `thread`), its model the contract's
//! [`domain::session_tree::build_session_thread`] over the synced rows
//! ([`crate::session_data::session_thread_view`], fixture
//! `session-thread.json`).
//!
//! * The HEADER: the title (a click renames it, `sessions.setTitle`) and a
//!   lock while private — no caption.
//! * The STRIP: `Conversation` · one chip per linked issue (status glyph,
//!   mono identifier, title) · `+` · a divider · `Guide` (a green dot while
//!   the root's guide has content). A chip linked moments before it first
//!   renders fades in with a ring.
//! * The CONVERSATION: the root's messages — a person's message a
//!   right-aligned bubble over `author · HH:MM` (`Queued` + remove while the
//!   device has not consumed it), the agent's prose as markdown, a session
//!   = THE session card ([`crate::work_card::session_card`], opens that
//!   session), question and plan cards answered in place
//!   (`sessions.answer`), events as muted lines, pictures as tiles; under it
//!   THE session composer box ([`crate::composer::session_composer_box`])
//!   sending `sessions.send({sessionId: root})`.
//! * An ISSUE child = the issue detail, embedded; the GUIDE child = the
//!   Guide page ([`crate::session_results::render`]) over the root's guide
//!   and its pull request's files (`pullRequests.files`), the Stack card and
//!   the merge capsule (`pullRequests.merge`).
//! * THE Menu on the `+` and on every issue chip: `Link an issue` (THE
//!   search list, `sessions.linkIssues`), `New issue` and, on a chip,
//!   `Unlink <IDENT>` (`sessions.unlinkIssue`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui::{
    div, prelude::FluentBuilder as _, px, Animation, AnimationExt as _, AnyElement, App,
    AppContext as _, ClickEvent, Entity, FocusHandle, Focusable, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Task, Window,
};
use gpui_component::{
    button::ButtonVariants as _,
    h_flex,
    input::{Input, InputEvent, InputState, TextareaState},
    scroll::ScrollableElement as _,
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;

use domain::rows::{Issue, MessageRow, PullRequestRow, SessionIssueRow, SessionRow};
use domain::session_results::SessionResultEntry;
use domain::session_tree::{SessionThreadRow, SessionThreadView, ThreadTab, THREAD, THREADS_LIST};

use crate::controls::PointerContextMenuExt as _;
use crate::controls::WebControl as _;
use crate::controls::WebText as _;
use crate::icons::registry;
use crate::markdown::{ImageCache, MarkdownView, RefResolver};
use crate::navigation::Screen;
use crate::session_results::GuidePage;

/// The menu copy.
pub(crate) const LINK_ISSUE_LABEL: &str = "Link an issue";
pub(crate) const NEW_ISSUE_LABEL: &str = "New issue";
/// The Guide child before the root's guide (or its pull request) has
/// anything to read.
pub(crate) const NO_GUIDE_COPY: &str = "No Guide yet. The agent files one as it works.";
/// A plan card's two answers.
pub(crate) const PLAN_APPROVE_LABEL: &str = "Approve";
pub(crate) const PLAN_REJECT_LABEL: &str = "Reject";

/// `Unlink EXP-1270`.
pub(crate) fn unlink_label(identifier: &str) -> String {
    format!("Unlink {identifier}")
}

/// A chip linked less than this long before it first renders is NEW to the
/// viewer: it fades in with a ring.
pub(crate) const NEW_CHILD_WINDOW: Duration = Duration::from_secs(10);

/// A root that has not synced this long after the page opened (with the
/// sessions shape settled) is missing — never a flash of "not found" for a
/// thread the composer just created.
pub(crate) const MISSING_GRACE: Duration = Duration::from_secs(5);

/// The conversation's reading column.
const CONVERSATION_W: f32 = 760.;
/// The header + strip's side inset.
const HEADER_INSET: f32 = 44.;
/// One strip chip's height.
const STRIP_CHIP_H: f32 = 28.;
/// The issue chip's lead (a live root's mark) and its state badge.
const CHIP_MARK_PX: f32 = 12.;
const CHIP_BADGE_PX: f32 = 5.;

/// Which child of the strip is on show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ThreadChildSel {
    Conversation,
    Issue(String),
    Guide,
}

// ---------------------------------------------------------------------------
// Seeds the New issue page reads
// ---------------------------------------------------------------------------

/// "New issue" → the FRESH New issue page `draft_id` files INTO a thread
/// (the root id). Only the opener's seed: the draft's first write stores it
/// on the row (`issue_drafts.thread_id`), which is what a reopen reads.
#[derive(Default)]
struct DraftThreads(HashMap<String, String>);

impl gpui::Global for DraftThreads {}

/// The thread root a fresh New issue page `draft_id` was opened for.
pub(crate) fn draft_thread(draft_id: &str, cx: &App) -> Option<String> {
    cx.try_global::<DraftThreads>()?.0.get(draft_id).cloned()
}

/// The New issue page's `In <title>` chip (the thread it files into); a
/// click opens the thread.
pub(crate) fn draft_thread_chip(root_id: &str, cx: &App) -> AnyElement {
    use crate::surface::{glass_pill, PillMode, PillSize};
    let theme = cx.theme();
    let title = crate::session_data::thread_title_of(root_id, cx)
        .unwrap_or_else(|| THREAD.copy.untitled.clone());
    let root_id = root_id.to_string();
    glass_pill("draft-thread", PillSize::Sm, PillMode::Action, cx)
        .max_w(px(280.))
        .child(
            Icon::new(registry::NAV_THREADS)
                .with_size(px(PillSize::Sm.glyph()))
                .text_color(theme.muted_foreground),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(theme.foreground)
                .child(SharedString::from(in_thread_label(&title))),
        )
        .on_click(move |_, window, cx| {
            crate::navigation::navigate(
                window,
                cx,
                Screen::Thread {
                    session_id: root_id.clone(),
                },
            );
        })
        .into_any_element()
}

/// `In <title>`.
pub(crate) fn in_thread_label(title: &str) -> String {
    format!("In {title}")
}

/// An issue child to select when that thread's view next renders (a new
/// issue filed into it lands on its child).
#[derive(Default)]
struct PendingThreadChild(HashMap<String, String>);

impl gpui::Global for PendingThreadChild {}

/// A New issue page filed `issue_id` into the thread `root_id`: link it
/// (`sessions.linkIssues`) and open its child the next time the thread
/// renders with it linked.
pub(crate) fn request_thread_child(root_id: &str, issue_id: &str, cx: &mut App) {
    link_issue(root_id.to_string(), issue_id.to_string(), cx);
    cx.default_global::<PendingThreadChild>()
        .0
        .insert(root_id.to_string(), issue_id.to_string());
}

/// Open a New issue page that files into `root_id`, on the board of the
/// thread's first issue (else the window's active board).
pub(crate) fn open_new_issue_in_thread(root_id: &str, window: &mut Window, cx: &mut App) {
    let board_id = crate::session_data::issues_of_root(root_id, cx)
        .into_iter()
        .next()
        .map(|issue| issue.board_id)
        .unwrap_or_default();
    let draft_id = api::issue_drafts::new_draft_id();
    cx.default_global::<DraftThreads>()
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
// The writes (sessions.linkIssues / unlinkIssue / setTitle / answer / unqueue)
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
            log::warn!("[thread] {verb} {thing} failed: {err}");
            cx.update(|cx| crate::action_error::failed_action_err(verb, thing, &err, cx));
        }
    })
    .detach();
}

/// `sessions.linkIssues({sessionId: root, issueIds: [issue]})`.
pub(crate) fn link_issue(root_id: String, issue_id: String, cx: &mut App) {
    background_write("link", "the issue", cx, move |trpc| {
        api::sessions::link_issues(trpc, &root_id, &[issue_id])
    });
}

/// `sessions.unlinkIssue({sessionId: root, issueId})`.
pub(crate) fn unlink_issue(root_id: String, issue_id: String, cx: &mut App) {
    background_write("unlink", "the issue", cx, move |trpc| {
        api::sessions::unlink_issue(trpc, &root_id, &issue_id)
    });
}

/// `sessions.setTitle` — `None`/blank clears back to the derived title.
fn rename_root(root_id: String, title: Option<String>, cx: &mut App) {
    background_write("rename", "the title", cx, move |trpc| {
        api::sessions::set_title(trpc, &root_id, title.as_deref())
    });
}

/// `sessions.answer` — a question card's option, a plan card's decision.
pub(crate) fn answer_card(input: api::sessions::AnswerInput, cx: &mut App) {
    background_write("answer", "the question", cx, move |trpc| api::sessions::answer(trpc, &input));
}

/// `sessions.unqueue` — take back a message the device has not consumed.
pub(crate) fn unqueue_message(message_id: String, cx: &mut App) {
    background_write("remove", "the message", cx, move |trpc| {
        api::sessions::unqueue(trpc, &message_id)
    });
}

/// A session card was clicked: another ROOT thread (an agent channel's
/// `thread_started` card) opens as its thread, every other session as the
/// session view.
pub(crate) fn open_session_card(session_id: &str, current_root: &str, window: &mut Window, cx: &mut App) {
    let opens_thread = crate::session_data::session(session_id, cx)
        .is_some_and(|session| session.is_root() && session.is_thread() && session.id != current_root);
    let screen = if opens_thread {
        Screen::Thread {
            session_id: session_id.to_string(),
        }
    } else {
        Screen::Session {
            session_id: session_id.to_string(),
        }
    };
    crate::navigation::navigate(window, cx, screen);
}

// ---------------------------------------------------------------------------
// THE Menu: the strip's "+" and an issue chip's context menu
// ---------------------------------------------------------------------------

/// The rows of the thread menu, in order (`unlink` = the issue chip's own
/// last row; the "+" has none). Pure, so the order is a unit test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ThreadMenuRow {
    LinkIssue,
    NewIssue,
    Separator,
    Unlink(String),
}

pub(crate) fn thread_menu_rows(unlink: Option<&str>) -> Vec<ThreadMenuRow> {
    let mut rows = vec![ThreadMenuRow::LinkIssue, ThreadMenuRow::NewIssue];
    if let Some(identifier) = unlink {
        rows.push(ThreadMenuRow::Separator);
        rows.push(ThreadMenuRow::Unlink(identifier.to_string()));
    }
    rows
}

/// What the menu acts on.
#[derive(Clone)]
pub(crate) struct ThreadMenuTarget {
    pub(crate) root_id: String,
    pub(crate) team_id: Option<String>,
    /// The issue chip the menu was opened on (`(issue_id, identifier)`).
    pub(crate) chip: Option<(String, String)>,
    /// The issues already linked (the search list checks them).
    pub(crate) linked: Vec<String>,
}

/// Build THE thread menu into `menu` (the "+" dropdown or a chip's context
/// menu).
pub(crate) fn thread_menu(
    mut menu: gpui_component::menu::PopupMenu,
    target: &ThreadMenuTarget,
    window: &mut Window,
    cx: &mut gpui::Context<gpui_component::menu::PopupMenu>,
) -> gpui_component::menu::PopupMenu {
    use crate::controls::{danger_menu_item, pointer_menu_item};
    let root = cx.entity().downgrade();
    menu = crate::picker::search_list::search_submenu_parent(menu.min_w(px(240.)));
    for row in thread_menu_rows(target.chip.as_ref().map(|(_, identifier)| identifier.as_str())) {
        menu = match row {
            ThreadMenuRow::Separator => menu.separator(),
            ThreadMenuRow::LinkIssue => {
                let target = target.clone();
                let root = root.clone();
                menu.submenu_with_icon(
                    Some(Icon::new(registry::NAV_SEARCH)),
                    LINK_ISSUE_LABEL,
                    window,
                    cx,
                    move |sub, window, cx| {
                        crate::picker::search_list::menu_search_list_submenu(
                            "thread-link-issue",
                            link_issue_source(&target),
                            sub,
                            root.clone(),
                            window,
                            cx,
                        )
                    },
                )
            }
            ThreadMenuRow::NewIssue => {
                let root_id = target.root_id.clone();
                menu.item(
                    pointer_menu_item(None, NEW_ISSUE_LABEL, |_| None, move |window, cx| {
                        open_new_issue_in_thread(&root_id, window, cx);
                    })
                    .icon(Icon::new(registry::UI_ADD)),
                )
            }
            ThreadMenuRow::Unlink(identifier) => {
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
fn link_issue_source(target: &ThreadMenuTarget) -> crate::picker::search_list::MenuListSource {
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

/// The strip's chip frame: active = the filled pill, else a ghost that
/// washes under the pointer.
pub(crate) fn strip_chip(id: impl Into<gpui::ElementId>, active: bool, cx: &App) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let hover = theme.list_hover;
    let base = div()
        .id(id)
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.))
        .h(px(STRIP_CHIP_H))
        .px(px(10.))
        .rounded(px(6.))
        .text_sm()
        .cursor_pointer()
        .border_1();
    if active {
        base.bg(theme.secondary)
            .border_color(theme.border)
            .text_color(theme.foreground)
    } else {
        base.border_color(gpui::transparent_black())
            .text_color(theme.muted_foreground)
            .hover(move |style| style.bg(hover))
    }
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
    let glyph = match status {
        Some(status) => crate::icons::resolved_status_icon(&status, cx).xsmall(),
        None => Icon::new(registry::UI_ISSUE)
            .xsmall()
            .text_color(theme.muted_foreground),
    };
    vec![
        glyph.into_any_element(),
        div()
            .flex_shrink_0()
            .text_xs()
            .font_family(theme::terminal::FONT_FAMILY)
            .text_color(theme.muted_foreground)
            .child(SharedString::from(identifier.to_string()))
            .into_any_element(),
        div()
            .max_w(px(140.))
            .truncate()
            .text_color(if active { theme.foreground } else { theme.muted_foreground })
            .child(SharedString::from(title.to_string()))
            .into_any_element(),
    ]
}

/// The Guide chip's green dot (the root's guide has content).
pub(crate) fn guide_dot(cx: &App) -> AnyElement {
    div()
        .size(px(6.))
        .rounded_full()
        .bg(cx.theme().success)
        .into_any_element()
}

/// The strip's vertical divider before Guide.
pub(crate) fn strip_divider(cx: &App) -> AnyElement {
    div()
        .w(px(1.))
        .h(px(16.))
        .mx_1()
        .bg(cx.theme().border)
        .into_any_element()
}

/// The page title (draft 3): the title at the header rung, a lock while the
/// thread is private.
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

/// `HH:MM` (local) of a wire timestamp; empty when unparseable.
pub(crate) fn clock(at: Option<&str>) -> String {
    at.and_then(crate::comments::parse_epoch)
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
        .map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

/// The caption under a person's bubble: `author · HH:MM`, a missing part
/// dropped with its separator.
pub(crate) fn bubble_caption(author: Option<&str>, time: &str) -> String {
    [author.map(str::trim).unwrap_or(""), time.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The display name of a message author: a member's label, else `Someone`.
pub(crate) fn member_name(user_id: Option<&str>, cx: &App) -> String {
    let Some(user_id) = user_id else {
        return crate::comments::author_label(None);
    };
    let user = Store::try_global(cx).and_then(|store| store.collections().users.read(cx).get(user_id).cloned());
    crate::comments::user_label(user_id, user.as_ref())
}

/// A message body as read-only chat markdown, `#IDENT`/`@email` pills scoped
/// to `team_id`.
pub(crate) fn message_markdown(id: SharedString, body: &str, team_id: Option<&str>) -> MarkdownView {
    let mut view = MarkdownView::new(id, body.to_string())
        .chat(true)
        .selectable(true)
        .autolink(true)
        .on_open_link(crate::markdown::open_agent_link);
    if let Some(team_id) = team_id {
        let team = team_id.to_string();
        view = view
            .resolver(RefResolver::from_store(team_id))
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

/// A person's message: the right-aligned bubble with its caption under it;
/// a queued one reads `Queued` and offers to take it back.
pub(crate) fn user_bubble(
    id: &str,
    body: MarkdownView,
    caption: String,
    queued: bool,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let unqueue_id = id.to_string();
    let caption_row = h_flex()
        .gap_1()
        .items_center()
        .text_2xs()
        .text_color(muted)
        .when(!caption.is_empty(), |row| row.child(SharedString::from(caption)))
        .when(queued, |row| {
            row.child(SharedString::from(format!("· {}", steer::feed::QUEUE_STRIP_TITLE)))
                .child(
                    crate::controls::ghost_icon_button(
                        SharedString::from(format!("thread-unqueue-{id}")),
                        Icon::new(registry::UI_CLOSE).xsmall(),
                        cx,
                    )
                    .tooltip(steer::feed::QUEUE_REMOVE_LABEL)
                    .on_click(move |_, _, cx| unqueue_message(unqueue_id.clone(), cx)),
                )
        });
    v_flex()
        .w_full()
        .min_w_0()
        .items_end()
        .gap_1()
        .pl_16()
        .child(
            body_text(div())
                .min_w_0()
                .max_w(px(560.))
                .rounded(px(12.))
                .border_1()
                .border_color(theme::tokens::glass::STROKE_STRONG.to_hsla())
                .bg(theme::tokens::glass::FILL_ACTIVE.to_hsla())
                .px_3()
                .py_2()
                .when(queued, |bubble| bubble.opacity(0.7))
                .child(body),
        )
        .child(caption_row)
        .into_any_element()
}

/// The agent's prose: its mark beside the markdown.
pub(crate) fn agent_prose(body: MarkdownView, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_start()
        .child(
            div().mt(px(4.)).flex_shrink_0().child(
                Icon::new(registry::CODING_ASSISTANT)
                    .xsmall()
                    .text_color(muted.opacity(0.6)),
            ),
        )
        .child(body_text(div()).flex_1().min_w_0().child(body))
        .into_any_element()
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

/// The question / plan card frame (the glass card, the accent glyph beside
/// the body column).
fn ask_frame(plan: bool, body: gpui::Div, cx: &App) -> AnyElement {
    let accent = if plan {
        cx.theme().primary
    } else {
        theme::tokens::YELLOW.to_hsla()
    };
    crate::surface::glass_card()
        .w_full()
        .min_w_0()
        .p_3()
        .child(
            h_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .items_start()
                .child(
                    div().flex_shrink_0().mt(px(4.)).child(
                        Icon::new(if plan { registry::CODING_PLAN } else { registry::UI_HELP })
                            .with_size(px(14.))
                            .text_color(accent),
                    ),
                )
                .child(body.flex_1().min_w_0()),
        )
        .into_any_element()
}

/// A question card answered in place: the question over one row per option
/// (`1`, `2` … chips); once answered, the chosen option stays lit and the
/// rest go quiet.
pub(crate) fn question_card(
    message_id: &str,
    question: MarkdownView,
    options: &[String],
    answer: Option<&str>,
    answered: bool,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let hover = theme.list_hover;
    let mut column = v_flex().gap_2().child(body_text(div()).w_full().min_w_0().child(question));
    for (index, option) in options.iter().enumerate() {
        let chosen = answer == Some(option.as_str());
        let message_id = message_id.to_string();
        let value = option.clone();
        let row = div()
            .id(SharedString::from(format!("thread-question-{message_id}-{index}")))
            .flex()
            .flex_row()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .rounded(px(theme::tokens::radius::MD))
            .border_1()
            .border_color(if chosen {
                theme::tokens::glass::STROKE_STRONG.to_hsla()
            } else {
                theme::tokens::glass::STROKE_CARD.to_hsla()
            })
            .text_sm()
            .text_color(if answered && !chosen { muted } else { theme.foreground })
            .child(div().flex_1().min_w_0().child(SharedString::from(option.clone())))
            .when(index < 9, |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .px_1()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                        .text_size(px(10.))
                        .font_family(theme::terminal::FONT_FAMILY)
                        .text_color(muted)
                        .child(SharedString::from((index + 1).to_string())),
                )
            })
            .when(!answered, |row| {
                row.cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(move |_, _, cx| {
                        answer_card(
                            api::sessions::AnswerInput {
                                message_id: message_id.clone(),
                                answer: Some(value.clone()),
                                ..Default::default()
                            },
                            cx,
                        );
                    })
            });
        column = column.child(row);
    }
    ask_frame(false, column, cx)
}

/// A plan card answered in place: the plan over Approve / Reject; once
/// decided, the decision as a muted word.
pub(crate) fn plan_card(message_id: &str, plan: MarkdownView, decision: Option<&str>, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let mut column = v_flex().gap_2().child(body_text(div()).w_full().min_w_0().child(plan));
    match decision {
        Some(decision) => {
            column = column.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(decision_word(decision))),
            );
        }
        None => {
            let decide = |decision: &'static str, label: &'static str, primary: bool| {
                let message_id = message_id.to_string();
                let button = gpui_component::button::Button::new(SharedString::from(format!(
                    "thread-plan-{message_id}-{decision}"
                )))
                .web_sm()
                .label(label);
                let button = if primary { button.primary() } else { button.ghost() };
                button.on_click(move |_, _, cx| {
                    answer_card(
                        api::sessions::AnswerInput {
                            message_id: message_id.clone(),
                            decision: Some(decision.to_string()),
                            ..Default::default()
                        },
                        cx,
                    );
                })
            };
            column = column.child(
                h_flex()
                    .gap_2()
                    .child(decide("approved", PLAN_APPROVE_LABEL, true))
                    .child(decide("rejected", PLAN_REJECT_LABEL, false)),
            );
        }
    }
    ask_frame(true, column, cx)
}

/// `approved` → `Approved`.
pub(crate) fn decision_word(decision: &str) -> String {
    let mut chars = decision.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
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
        SharedString::from(format!("thread-picture-{id}")),
        entry.label.clone(),
        images,
        cx,
    )
}

/// One conversation row of a root (a thread, an agent channel): every card
/// kind but the plain messages, which each screen paints its own way.
pub(crate) fn card_row(
    row: &SessionThreadRow,
    root_id: &str,
    team_id: Option<&str>,
    images: &Entity<ImageCache>,
    cx: &mut App,
) -> Option<AnyElement> {
    Some(match row {
        SessionThreadRow::Message { .. } => return None,
        SessionThreadRow::Session { card, .. } => {
            let session_id = card.session_id.clone();
            let root_id = root_id.to_string();
            crate::work_card::session_card(
                card,
                move |window, cx| open_session_card(&session_id, &root_id, window, cx),
                cx,
            )
        }
        SessionThreadRow::Question {
            id,
            question,
            options,
            answer,
            answered,
        } => question_card(
            id,
            message_markdown(SharedString::from(format!("thread-question-{id}")), question, team_id),
            options,
            answer.as_deref(),
            *answered,
            cx,
        ),
        SessionThreadRow::Plan { id, plan, decision } => plan_card(
            id,
            message_markdown(SharedString::from(format!("thread-plan-{id}")), plan, team_id),
            decision.as_deref(),
            cx,
        ),
        SessionThreadRow::Subagent { title, status, .. } => muted_line(
            registry::CODING_SUBAGENT,
            [title.as_str(), status.as_str()]
                .into_iter()
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
            cx,
        ),
        SessionThreadRow::Picture {
            id,
            attachment_id,
            width,
            height,
            caption,
        } => picture_row(id, attachment_id, *width, *height, caption.as_deref(), images, cx),
        SessionThreadRow::Event { text, .. } => muted_line(registry::EVENT_STATUS_CHANGED, text.clone(), cx),
    })
}

// ---------------------------------------------------------------------------
// The issue page's `Threads` row
// ---------------------------------------------------------------------------

/// A root's mark (its CLI's): the working mark while busy, the brand mark
/// with an amber corner dot while it needs you.
pub(crate) fn root_mark(root: &SessionRow, busy: bool, needs_you: bool, size: f32, badge: f32) -> AnyElement {
    let agent = root.cli.as_deref().and_then(coding::CodingAgent::parse);
    if busy {
        return crate::coding_selects::agent_working_mark(agent, size).into_any_element();
    }
    let mark = match agent {
        Some(agent) => crate::coding_selects::agent_mark(agent),
        None => Icon::new(registry::SETTINGS_AGENTS),
    };
    div()
        .relative()
        .flex_shrink_0()
        .child(mark.with_size(px(size)))
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

/// The issue page's `Threads` chip: a small glass pill — the root's mark
/// and the thread title; a click opens the thread.
fn issue_thread_chip(index: usize, root: &SessionRow, cx: &App) -> AnyElement {
    use crate::surface::{glass_pill, PillMode, PillSize};
    let tree = crate::session_data::tree_of_root(&root.id, cx);
    let live = tree.iter().filter(|session| !session.is_ended());
    let (busy, needs_you) = live.fold((false, false), |(busy, needs), session| {
        (busy || session.busy, needs || session.needs_input)
    });
    let title = crate::session_data::thread_title_of(&root.id, cx).unwrap_or_else(|| THREAD.copy.untitled.clone());
    let root_id = root.id.clone();
    glass_pill(("issue-thread-chip", index), PillSize::Sm, PillMode::Action, cx)
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
                Screen::Thread {
                    session_id: root_id.clone(),
                },
            );
        })
        .into_any_element()
}

/// The issue page's `Threads` row: the muted label (the list's own name)
/// and one chip per thread root linking the issue. `None` while none does.
pub(crate) fn issue_threads_row(issue_id: &str, cx: &App) -> Option<AnyElement> {
    let roots = crate::session_data::roots_of_issue(issue_id, cx);
    if roots.is_empty() {
        return None;
    }
    Some(
        h_flex()
            .id("issue-threads-row")
            .flex_shrink_0()
            .gap_1p5()
            .items_center()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(THREADS_LIST.copy.title.clone())),
            )
            .children(
                roots
                    .iter()
                    .enumerate()
                    .map(|(index, root)| issue_thread_chip(index, root, cx)),
            )
            .into_any_element(),
    )
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// Whether the page may say the thread is missing (pure): the sessions shape
/// settled, the root is not among its rows, and the page has been open past
/// [`MISSING_GRACE`].
pub(crate) fn thread_not_found(ready: bool, has_root: bool, open_for: Duration) -> bool {
    ready && !has_root && open_for >= MISSING_GRACE
}

/// What one render reads, re-derived on every collection change.
#[derive(Clone)]
struct ThreadModel {
    root: SessionRow,
    view: SessionThreadView,
    issues: HashMap<String, Issue>,
    links: Vec<SessionIssueRow>,
    messages: HashMap<String, MessageRow>,
    pulls: Vec<PullRequestRow>,
}

impl ThreadModel {
    fn linked_ids(&self) -> Vec<String> {
        self.view
            .tabs
            .iter()
            .filter_map(|tab| match tab {
                ThreadTab::Issue { issue_id, .. } => Some(issue_id.clone()),
                _ => None,
            })
            .collect()
    }

    /// The pull request the Guide reads and merges.
    fn pull(&self) -> Option<&PullRequestRow> {
        crate::threads_view::root_pull_request(&self.root.id, &self.pulls)
    }
}

fn thread_model(root_id: &str, cx: &App) -> Option<ThreadModel> {
    let view = crate::session_data::session_thread_view(root_id, crate::session_data::now_ms(), cx)?;
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
    let messages = crate::session_data::messages_of_session(root_id, cx)
        .into_iter()
        .map(|message| (message.id.clone(), message))
        .collect();
    Some(ThreadModel {
        root,
        view,
        issues,
        links,
        messages,
        pulls: crate::session_data::pull_requests_of_root(root_id, cx),
    })
}

/// The Guide child's own state: the open diff page, the pull request's
/// diff, the hovered stack member, a merge in flight.
#[derive(Default)]
struct GuideState {
    page: Option<GuidePage>,
    diff: Option<(String, Entity<crate::pr_diff::PrDiffView>)>,
    stack_hovered: Option<String>,
    merging: bool,
}

pub(crate) struct ThreadScreenView {
    root_id: String,
    child: ThreadChildSel,
    model: Option<ThreadModel>,
    /// The issue child's detail view (built on first show, re-pointed).
    issue_view: Option<Entity<crate::issue_detail::IssueDetailView>>,
    guide: GuideState,
    images: Entity<ImageCache>,
    reply: Entity<TextareaState>,
    mention: Entity<crate::mention_input::MentionInput>,
    mention_team: Option<String>,
    sending: bool,
    /// The inline rename field while the title is being edited.
    rename: Option<Entity<InputState>>,
    /// When each issue link was FIRST rendered, and whether it was new then
    /// (linked inside [`NEW_CHILD_WINDOW`]) — the fade-in ring.
    first_seen: HashMap<String, (Instant, bool)>,
    opened_at: Instant,
    scroll: ScrollHandle,
    row_count: usize,
    focus_handle: FocusHandle,
    _ring_tick: Option<Task<()>>,
    _missing_tick: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl ThreadScreenView {
    pub(crate) fn new(root_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let (reply, mention) =
            crate::composer::session_composer_field(THREAD.copy.placeholder.clone(), window, cx);
        let mut subscriptions = vec![cx.subscribe_in(
            &reply,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => this.send_reply(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )];
        if let Some(store) = Store::try_global(cx) {
            let collections = store.collections().clone();
            subscriptions.push(cx.observe(&collections.sessions, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&collections.session_issues, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&collections.messages, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&collections.pull_requests, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&collections.issues, |this, _, cx| this.refresh(cx)));
        }
        let transport = crate::queries::attachment_transport(cx);
        let images = cx.new(|_| ImageCache::new(transport));
        // Re-render once the grace passed, so a missing thread says so.
        let missing_tick = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(MISSING_GRACE).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        });
        let mut this = Self {
            root_id,
            child: ThreadChildSel::Conversation,
            model: None,
            issue_view: None,
            guide: GuideState::default(),
            images,
            reply,
            mention,
            mention_team: None,
            sending: false,
            rename: None,
            first_seen: HashMap::new(),
            opened_at: Instant::now(),
            scroll: ScrollHandle::new(),
            row_count: 0,
            focus_handle: cx.focus_handle(),
            _ring_tick: None,
            _missing_tick: missing_tick,
            _subscriptions: subscriptions,
        };
        this.refresh(cx);
        // A capture (and a debug launch) can open a thread on its Guide.
        if std::env::var("EXP_DEV_THREAD_CHILD").ok().as_deref() == Some("guide") {
            this.child = ThreadChildSel::Guide;
        }
        this
    }

    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        let model = thread_model(&self.root_id, cx);
        if let Some(model) = &model {
            self.note_seen(&model.links, cx);
            let team = model.root.team_id.clone();
            if team != self.mention_team {
                self.mention_team = team.clone();
                self.mention.update(cx, |mention, _| {
                    mention.set_source(team.map(crate::markdown::store_completion_source));
                });
            }
            // An unlinked issue's child closes.
            if let ThreadChildSel::Issue(issue_id) = &self.child {
                if !model.linked_ids().contains(issue_id) {
                    self.child = ThreadChildSel::Conversation;
                }
            }
            // New rows land in view.
            if model.view.rows.len() > self.row_count {
                self.scroll.scroll_to_bottom();
            }
            self.row_count = model.view.rows.len();
        }
        self.model = model;
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

    /// The tab closed: an open issue child flushes its edit.
    pub(crate) fn shutdown(&mut self, cx: &mut App) {
        if let (ThreadChildSel::Issue(_), Some(view)) = (&self.child, &self.issue_view) {
            view.update(cx, |detail, cx| {
                detail.flush_title(cx);
                detail.flush_description(cx);
            });
        }
    }

    fn set_child(&mut self, child: ThreadChildSel, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.child == child {
            return;
        }
        // Leaving an issue child flushes its edit (the close-tab rule).
        self.shutdown(cx);
        if let ThreadChildSel::Issue(issue_id) = &child {
            let view = self
                .issue_view
                .get_or_insert_with(|| cx.new(|cx| crate::issue_detail::IssueDetailView::new(window, cx)))
                .clone();
            let issue_id = issue_id.clone();
            view.update(cx, |detail, cx| detail.set_issue(issue_id, window, cx));
        }
        self.guide.page = None;
        self.child = child;
        cx.notify();
    }

    // ── the reply ────────────────────────────────────────────────────────

    /// THE reply: `sessions.send({sessionId: root, text})` — the server
    /// routes it (a live root takes it, an idle one resumes, an ended one
    /// continues in a new child).
    fn send_reply(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let text = self.reply.read(cx).value().trim().to_string();
        if text.is_empty() || self.sending {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("send", "the message", None, cx);
            return;
        };
        self.sending = true;
        self.reply.update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
        let input = api::sessions::SendInput::message(&self.root_id, &text, Vec::new());
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::sessions::send(&trpc, &input) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.sending = false;
                if let Err(err) = result {
                    // The draft comes back so nothing typed is lost.
                    if this.reply.read(cx).value().trim().is_empty() {
                        this.reply.update(cx, |input, cx| input.set_value(text.clone(), window, cx));
                    }
                    crate::action_error::failed_action_err("send", "the message", &err, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ── rename ───────────────────────────────────────────────────────────

    fn begin_rename(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(model) = &self.model else { return };
        let current = model
            .root
            .explicit_title()
            .unwrap_or(&model.view.title)
            .to_string();
        let placeholder = model.view.title.clone();
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder(placeholder);
            state.set_value(current, window, cx);
            state
        });
        cx.subscribe_in(&input, window, |this, input, event: &InputEvent, _window, cx| match event {
            InputEvent::PressEnter { .. } => {
                let value = input.read(cx).value().trim().to_string();
                this.rename = None;
                rename_root(this.root_id.clone(), (!value.is_empty()).then_some(value), cx);
                cx.notify();
            }
            InputEvent::Blur => {
                this.rename = None;
                cx.notify();
            }
            _ => {}
        })
        .detach();
        input.update(cx, |state, cx| state.focus(window, cx));
        self.rename = Some(input);
        cx.notify();
    }

    // ── the Guide ────────────────────────────────────────────────────────

    /// The pull request's diff view, built (and fetched) once per PR.
    fn pull_diff(
        &mut self,
        pull_request_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<crate::pr_diff::PrDiffView> {
        if let Some((id, view)) = &self.guide.diff {
            if id == pull_request_id {
                return view.clone();
            }
        }
        let view = cx.new(|cx| crate::pr_diff::PrDiffView::new(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        let id = pull_request_id.to_string();
        view.update(cx, |view, cx| view.set_pull_request(id.clone(), cx));
        self.guide.diff = Some((id, view.clone()));
        self.guide.page = None;
        view
    }

    fn open_guide_page(&mut self, page: Option<GuidePage>, cx: &mut gpui::Context<Self>) {
        self.guide.page = page;
        cx.notify();
    }

    /// `pullRequests.merge` — `stack` merges THROUGH the PR (every open one
    /// below lands first); a refusal is the `Couldn't merge` toast.
    fn merge(&mut self, pull_request_id: String, stack: bool, cx: &mut gpui::Context<Self>) {
        if self.guide.merging {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("merge", "the pull request", None, cx);
            return;
        };
        self.guide.merging = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::sessions::pr_merge(&trpc, &pull_request_id, stack) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.guide.merging = false;
                match result {
                    Ok(output) if !output.merged && !output.queued => {
                        crate::action_error::failed_action("merge", "the pull request", output.error, cx);
                    }
                    Ok(_) => {}
                    Err(err) => crate::action_error::failed_action_err("merge", "the pull request", &err, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ── render ───────────────────────────────────────────────────────────

    fn menu_target(&self, chip: Option<(String, String)>) -> ThreadMenuTarget {
        ThreadMenuTarget {
            root_id: self.root_id.clone(),
            team_id: self.model.as_ref().and_then(|model| model.root.team_id.clone()),
            chip,
            linked: self.model.as_ref().map(ThreadModel::linked_ids).unwrap_or_default(),
        }
    }

    fn render_header(&mut self, model: &ThreadModel, cx: &mut gpui::Context<Self>) -> AnyElement {
        let title: AnyElement = match &self.rename {
            Some(input) => div()
                .w(px(480.))
                .child(Input::new(input).appearance(false).text_2xl())
                .into_any_element(),
            None => title_row(&model.view.title, model.view.private, cx)
                .id("thread-title")
                .cursor_text()
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.begin_rename(window, cx)))
                .into_any_element(),
        };
        v_flex()
            .w_full()
            .px(px(HEADER_INSET))
            .pt_6()
            .gap_4()
            .child(title)
            .child(self.render_strip(model, cx))
            .child(div().w_full().h(px(1.)).bg(cx.theme().border))
            .into_any_element()
    }

    fn render_strip(&mut self, model: &ThreadModel, cx: &mut gpui::Context<Self>) -> AnyElement {
        let mut chips: Vec<AnyElement> = Vec::new();
        // The Guide (after the divider) never scrolls out of reach.
        let mut trailing: Vec<AnyElement> = Vec::new();
        for tab in &model.view.tabs {
            match tab {
                ThreadTab::Conversation => {
                    let active = self.child == ThreadChildSel::Conversation;
                    chips.push(
                        strip_chip("thread-child-conversation", active, cx)
                            .child(Icon::new(registry::ACTION_CHAT).xsmall())
                            .child(SharedString::from(THREAD.copy.conversation.clone()))
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.set_child(ThreadChildSel::Conversation, window, cx)
                            }))
                            .into_any_element(),
                    );
                }
                ThreadTab::Issue { issue_id, identifier } => {
                    let active = self.child == ThreadChildSel::Issue(issue_id.clone());
                    let issue = model.issues.get(issue_id);
                    let status = issue.map(|issue| crate::queries::resolve_issue_status(cx, issue));
                    let title = issue.map(|issue| issue.title.clone()).unwrap_or_default();
                    let link_id = model
                        .links
                        .iter()
                        .find(|link| link.issue_id == *issue_id)
                        .map(|link| link.id.clone())
                        .unwrap_or_default();
                    let ringed = self.ringed(&link_id);
                    let target = self.menu_target(Some((issue_id.clone(), identifier.clone())));
                    let open = issue_id.clone();
                    let chip = strip_chip(SharedString::from(format!("thread-child-{issue_id}")), active, cx)
                        .children(strip_issue_body(status, identifier, &title, active, cx))
                        .when(ringed, |chip| chip.border_color(cx.theme().ring))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.set_child(ThreadChildSel::Issue(open.clone()), window, cx)
                        }))
                        .pointer_context_menu(move |menu, window, cx| thread_menu(menu, &target, window, cx));
                    chips.push(if ringed {
                        div()
                            .flex_shrink_0()
                            .child(chip)
                            .with_animation(
                                SharedString::from(format!("thread-child-enter-{link_id}")),
                                Animation::new(theme::motion::ENTER).with_easing(theme::motion::decelerate()),
                                |el, delta| el.opacity(delta),
                            )
                            .into_any_element()
                    } else {
                        chip.into_any_element()
                    });
                }
                ThreadTab::Add => {
                    let target = self.menu_target(None);
                    let trigger =
                        crate::controls::ghost_icon_button("thread-child-add", Icon::new(registry::UI_ADD), cx);
                    chips.push(
                        crate::controls::PointerMenu::new("thread-child-add-menu", trigger, move |menu, window, cx| {
                            thread_menu(menu, &target, window, cx)
                        })
                        .into_any_element(),
                    );
                    trailing.push(strip_divider(cx));
                }
                ThreadTab::Guide { dot } => {
                    let active = self.child == ThreadChildSel::Guide;
                    trailing.push(
                        strip_chip("thread-child-guide", active, cx)
                            .child(Icon::new(registry::GUIDE_CHANGES).xsmall())
                            .child(SharedString::from(THREAD.copy.guide.clone()))
                            .when(*dot, |chip| chip.child(guide_dot(cx)))
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.set_child(ThreadChildSel::Guide, window, cx)
                            }))
                            .into_any_element(),
                    );
                }
            }
        }
        h_flex()
            .id(SharedString::from(THREAD.test_ids.tabs.clone()))
            .w_full()
            .min_w_0()
            .gap_1()
            .items_center()
            .child(
                h_flex()
                    .id("thread-strip")
                    .flex_shrink(1.)
                    .min_w_0()
                    .gap_1()
                    .items_center()
                    .overflow_x_scroll()
                    .children(chips),
            )
            .children(trailing)
            .into_any_element()
    }

    fn render_row(&self, row: &SessionThreadRow, model: &ThreadModel, cx: &mut gpui::Context<Self>) -> AnyElement {
        let team_id = model.root.team_id.as_deref();
        if let SessionThreadRow::Message {
            id,
            author,
            author_id,
            body,
            queued,
        } = row
        {
            let markdown = message_markdown(SharedString::from(format!("thread-message-{id}")), body, team_id);
            return match author.as_str() {
                domain::contract::MESSAGE_AUTHOR_KIND_USER => {
                    let at = model.messages.get(id).and_then(|message| message.created_at.as_deref());
                    let name = member_name(author_id.as_deref(), cx);
                    user_bubble(id, markdown, bubble_caption(Some(&name), &clock(at)), *queued, cx)
                }
                domain::contract::MESSAGE_AUTHOR_KIND_AGENT => agent_prose(markdown, cx),
                _ => muted_line(registry::UI_INFO, body.clone(), cx),
            };
        }
        card_row(row, &self.root_id, team_id, &self.images, cx).unwrap_or_else(|| div().into_any_element())
    }

    fn render_conversation(&mut self, model: &ThreadModel, cx: &mut gpui::Context<Self>) -> AnyElement {
        let rows: Vec<AnyElement> = model
            .view
            .rows
            .iter()
            .map(|row| self.render_row(row, model, cx))
            .collect();
        let column = v_flex()
            .id(SharedString::from(THREAD.test_ids.conversation.clone()))
            .w_full()
            .max_w(px(CONVERSATION_W))
            .mx_auto()
            .px(px(crate::work_header::WORK_GUTTER))
            .pt_8()
            .pb_8()
            .gap_5()
            .children(rows);
        let empty = self.reply.read(cx).value().trim().is_empty();
        let this = cx.entity().downgrade();
        let submit = crate::composer::SessionComposerSubmit::send(
            SharedString::from(THREAD.test_ids.composer.clone()),
            empty,
            move |_, window, cx| {
                let _ = this.update(cx, |this, cx| this.send_reply(window, cx));
            },
        )
        .loading(self.sending);
        let composer = crate::composer::session_composer_box(
            crate::composer::SessionComposerBox::new(self.mention.clone(), submit),
            cx,
        );
        v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(
                div()
                    .id("thread-conversation-scroll")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .track_scroll(&self.scroll)
                    .overflow_y_scrollbar()
                    .child(column),
            )
            .child(div().w_full().pb_2().child(composer))
            .into_any_element()
    }

    fn render_issue_child(&mut self) -> AnyElement {
        match &self.issue_view {
            Some(view) => div().flex_1().min_h_0().w_full().child(view.clone()).into_any_element(),
            None => div().flex_1().into_any_element(),
        }
    }

    fn render_guide_child(
        &mut self,
        model: &ThreadModel,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let groups = domain::session_results::parse_session_result_groups(model.root.guide.as_ref());
        let pull = model.pull().cloned();
        let diff_view = pull.as_ref().map(|pull| self.pull_diff(&pull.id, window, cx));
        let files = diff_view
            .as_ref()
            .map(|view| view.read(cx).pane_files(cx))
            .unwrap_or_default();
        // A diff page: the `← Guide` row over the filtered diff.
        if let (Some(page), Some(view)) = (self.guide.page, diff_view.as_ref()) {
            let diff = crate::session_results::guide_diff_files(&files);
            let page_view = crate::session_results::guide_page_view(&groups, &diff, page, false)
                .or_else(|| crate::session_results::guide_page_view(&groups, &diff, GuidePage::All, false));
            if let Some(page_view) = page_view {
                let filtered = page != GuidePage::All;
                view.update(cx, |view, cx| {
                    view.set_paths(filtered.then(|| page_view.paths.clone()), cx)
                });
                let this = cx.entity().downgrade();
                let back = crate::session_results::guide_page_header(
                    &page_view,
                    std::rc::Rc::new(move |_window, cx| {
                        let _ = this.update(cx, |this, cx| this.open_guide_page(None, cx));
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
        }
        let body: AnyElement = if groups.is_empty() && files.is_empty() {
            div()
                .flex_1()
                .w_full()
                .pt_10()
                .flex()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(NO_GUIDE_COPY)
                .into_any_element()
        } else {
            let this = cx.entity().downgrade();
            let on_open: crate::session_results::OnOpenGuidePage = std::rc::Rc::new(move |page, _window, cx| {
                let _ = this.update(cx, |this, cx| this.open_guide_page(Some(page), cx));
            });
            let stack = pull.as_ref().and_then(|pull| self.stack_card(model, pull, cx));
            let width = f32::from(window.viewport_size().width).min(crate::work_header::WORK_COLUMN_W)
                - 2. * crate::work_header::WORK_GUTTER;
            crate::session_results::render(
                &groups,
                width,
                &self.images,
                model.root.team_id.as_deref(),
                crate::session_results::GuideSpec {
                    diff: (!files.is_empty()).then_some(files),
                    on_open: Some(on_open),
                    stack,
                    fallback_lead: None,
                    unnumbered: false,
                    scroll: None,
                },
                cx,
            )
        };
        // THE merge capsule of the thread's open pull request.
        let capsule = pull.filter(|pull| pull.is_open()).map(|pull| {
            let stacked = model
                .root
                .team_id
                .as_deref()
                .map(|team_id| crate::session_data::team_pr_snapshot(team_id, cx))
                .is_some_and(|snapshot| {
                    snapshot.with_set(|set| domain::pr_stack::stack_position(&pull, set).is_some())
                });
            let label = if self.guide.merging {
                "Merging…"
            } else if stacked {
                domain::pr_stack::MERGE_STACK_LABEL
            } else {
                domain::contract::DIFF_UI_MERGE_PR
            };
            let pull_request_id = pull.id.clone();
            h_flex().w_full().justify_center().pb_4().pt_2().child(
                crate::surface::glass_pill_button_primary("thread-guide-merge", crate::surface::PillSize::Md)
                    .icon(
                        Icon::from(crate::icons::ExpIcon::GitMerge)
                            .with_size(px(crate::surface::PillSize::Md.glyph()))
                            .text_color(cx.theme().primary_foreground),
                    )
                    .label(label)
                    .loading(self.guide.merging)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.merge(pull_request_id.clone(), stacked, cx)
                    })),
            )
        });
        v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(body)
            .children(capsule)
            .into_any_element()
    }

    /// The Stack card of the thread's pull request: a pick opens that PR's
    /// thread, the hovered member merges through.
    fn stack_card(&self, model: &ThreadModel, pull: &PullRequestRow, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
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
                crate::navigation::navigate(
                    window,
                    cx,
                    Screen::Thread {
                        session_id: root_id.clone(),
                    },
                );
            }
        });
        let this = cx.entity().downgrade();
        let on_merge: crate::session_results::OnPickStackRow = std::rc::Rc::new(move |pull_request_id, _window, cx| {
            let id = pull_request_id.to_string();
            let _ = this.update(cx, |this, cx| this.merge(id, true, cx));
        });
        let this = cx.entity().downgrade();
        let on_hover: crate::session_results::OnHoverStackRow = std::rc::Rc::new(move |hovered, _window, cx| {
            let _ = this.update(cx, |this, cx| {
                if this.guide.stack_hovered != hovered {
                    this.guide.stack_hovered = hovered;
                    cx.notify();
                }
            });
        });
        Some(crate::session_results::stack_card(
            &view,
            None,
            Some(on_pick),
            Some(on_merge),
            self.guide.stack_hovered.clone(),
            Some(on_hover),
            cx,
        ))
    }
}

impl Focusable for ThreadScreenView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ThreadScreenView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(model) = self.model.clone() else {
            let ready = Store::try_global(cx).is_some_and(|store| store.collections().sessions.read(cx).is_ready());
            // The sessions synced, this root is not among them, and the
            // grace passed (deleted, a lost membership, a foreign id).
            if thread_not_found(ready, false, self.opened_at.elapsed()) {
                return v_flex()
                    .size_full()
                    .px_4()
                    .items_center()
                    .justify_center()
                    .child(
                        crate::controls::empty_state(
                            Icon::new(registry::WORK_FACES),
                            crate::empty_state_copy::THREAD_NOT_FOUND,
                            Some(crate::empty_state_copy::THREAD_NOT_FOUND_DESCRIPTION.into()),
                            cx,
                        )
                        .child(
                            crate::controls::empty_state_actions().child(
                                gpui_component::button::Button::new("thread-not-found-agent")
                                    .primary()
                                    .web_sm()
                                    .label(crate::empty_state_copy::THREAD_NOT_FOUND_ACTION)
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
                .child(crate::feedback::skeleton_rows("thread-loading", 3, 36., cx));
        };
        // A new issue filed into this thread lands on its child once its
        // link has synced.
        let pending = cx
            .try_global::<PendingThreadChild>()
            .and_then(|pending| pending.0.get(&self.root_id).cloned())
            .filter(|issue_id| model.linked_ids().contains(issue_id));
        if let Some(issue_id) = pending {
            cx.global_mut::<PendingThreadChild>().0.remove(&self.root_id);
            self.set_child(ThreadChildSel::Issue(issue_id), window, cx);
        }
        let header = self.render_header(&model, cx);
        let child = match self.child.clone() {
            ThreadChildSel::Conversation => self.render_conversation(&model, cx),
            ThreadChildSel::Issue(_) => self.render_issue_child(),
            ThreadChildSel::Guide => self.render_guide_child(&model, window, cx),
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
    #[test]
    fn the_thread_menu_rows() {
        assert_eq!(thread_menu_rows(None), vec![ThreadMenuRow::LinkIssue, ThreadMenuRow::NewIssue]);
        assert_eq!(
            thread_menu_rows(Some("EXP-1270")),
            vec![
                ThreadMenuRow::LinkIssue,
                ThreadMenuRow::NewIssue,
                ThreadMenuRow::Separator,
                ThreadMenuRow::Unlink("EXP-1270".into()),
            ]
        );
        assert_eq!(unlink_label("EXP-1270"), "Unlink EXP-1270");
    }

    /// Never "not found" while the sync has not settled, the root is there,
    /// or before the grace passed.
    #[test]
    fn thread_not_found_waits_for_the_sync_and_the_grace() {
        let late = MISSING_GRACE + Duration::from_millis(1);
        assert!(!thread_not_found(false, false, late));
        assert!(!thread_not_found(true, true, late));
        assert!(!thread_not_found(true, false, Duration::from_secs(1)));
        assert!(thread_not_found(true, false, late));
    }

    #[test]
    fn the_bubble_caption_names_the_author_and_the_time() {
        assert_eq!(bubble_caption(Some("Alex Carter"), "19:56"), "Alex Carter · 19:56");
        assert_eq!(bubble_caption(None, "19:56"), "19:56");
        assert_eq!(bubble_caption(Some(" "), ""), "");
        assert_eq!(clock(Some("garbage")), "");
        assert_eq!(clock(None), "");
    }

    /// No user-visible copy says "thread" (EXP-1294).
    #[test]
    fn the_copy_never_says_thread() {
        for copy in [
            LINK_ISSUE_LABEL,
            NEW_ISSUE_LABEL,
            NO_GUIDE_COPY,
            PLAN_APPROVE_LABEL,
            PLAN_REJECT_LABEL,
            THREAD.copy.placeholder.as_str(),
            THREAD.copy.conversation.as_str(),
            THREAD.copy.guide.as_str(),
        ] {
            assert!(!copy.to_lowercase().contains("thread"), "{copy}");
        }
        assert_eq!(in_thread_label("Offline sync plan"), "In Offline sync plan");
        assert_eq!(decision_word("approved"), "Approved");
        assert_eq!(decision_word(""), "");
    }
}
