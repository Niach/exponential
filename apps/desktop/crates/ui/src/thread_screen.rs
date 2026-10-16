//! EXP-1294 — the THREAD screen ([`Screen::Thread`]): a thread IS a root
//! session (kind `thread`), its model the contract's
//! [`domain::session_tree::build_session_thread`] over the synced rows
//! ([`crate::session_data::session_thread_view`], fixture
//! `session-thread.json`).
//!
//! * The HEADER: the title (EXP-1316: derived, plain text, nobody renames
//!   it) and a lock while private — no caption; its trailing slot holds the
//!   Merge pill while the Guide's PR is open, then the Guide `…`, on EVERY
//!   tab.
//! * The STRIP: `Conversation` · one chip per linked issue (status glyph,
//!   mono identifier, title) · `+` · a divider · `Guide` (a green dot while
//!   the root's guide has content), every chip 28px. A chip linked moments
//!   before it first renders fades in with a ring.
//! * The CONVERSATION: the root's messages — a person's message a
//!   right-aligned bubble over `author · HH:MM` (`Queued` + remove while the
//!   device has not consumed it), the agent's prose as markdown, a session
//!   = THE session card ([`crate::work_card::session_card`], opens that
//!   session), question and plan cards = THE session view's card
//!   (`crate::steer_viewer::ask_card` + its option rows, the free-text row
//!   and the plan's note; `sessions.answer`), events as muted lines, pictures as tiles; under it
//!   THE session composer box ([`crate::composer::session_composer_box`])
//!   sending `sessions.send({sessionId: root})`.
//! * An ISSUE child = the issue detail, EMBEDDED (no work header bar, no
//!   Threads row); the GUIDE child = the Guide page
//!   ([`crate::session_results::render`]) over the root's guide and its pull
//!   request's files (`pullRequests.files`) and the Stack card.
//! * THE Menu on the `+` and on every issue chip: `Link an issue` (THE
//!   search list, `sessions.linkIssues`), `New issue` and, on a chip,
//!   `Unlink <IDENT>` (`sessions.unlinkIssue`).

use std::collections::{HashMap, HashSet};
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
    input::{InputEvent, InputState},
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;

use domain::rows::{Issue, MessageRow, PullRequestRow, SessionIssueRow, SessionRow};
use domain::session_results::SessionResultEntry;
use domain::session_tree::{QuestionOption, SessionThreadRow, SessionThreadView, ThreadTab, THREAD, THREADS_LIST};

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
/// An issue chip's floor while the strip shrinks it (glyph + identifier +
/// a few title letters and the ellipsis).
const ISSUE_CHIP_MIN_W: f32 = 112.;
/// An issue chip title's ONE max width (then the ellipsis).
const ISSUE_CHIP_TITLE_MAX_W: f32 = 140.;
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
/// on the row (`issue_drafts.session_id`), which is what a reopen reads.
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

/// Cards whose answer is on its way (or landed before `answered_at`
/// synced): a second click never answers twice, and the card goes quiet.
#[derive(Default)]
struct AnsweringCards(HashSet<String>);

impl gpui::Global for AnsweringCards {}

/// Whether `message_id`'s answer is already on its way.
fn answering(message_id: &str, cx: &App) -> bool {
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
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::sessions::answer(&trpc, &input) })
            .await;
        if let Err(err) = result {
            log::warn!("[thread] answer the question failed: {err}");
            cx.update(|cx| {
                cx.default_global::<AnsweringCards>().0.remove(&message_id);
                cx.refresh_windows();
                crate::action_error::failed_action_err("answer", "the question", &err, cx);
            });
        }
    })
    .detach();
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
            .flex_shrink(1.)
            .min_w_0()
            .max_w(px(ISSUE_CHIP_TITLE_MAX_W))
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

/// Web `permissions.isMember` for the thread's team (route `threads/$threadId`):
/// the one gate on the strip's `+` and the Guide menu's Close PR. A root
/// with no team (never synced that far) is not a member of anything.
fn thread_member(model: &ThreadModel, cx: &App) -> bool {
    model
        .root
        .team_id
        .as_deref()
        .is_some_and(|team_id| crate::settings::is_member(cx, team_id))
}

/// EXP-1307 — the Guide `…` (`guide_menu_entries` over the Guide PR):
/// Open PR on GitHub, then Close PR while it is open (members only, the
/// web's `canClose={permissions.isMember}`). No entries = none.
fn guide_menu(model: &ThreadModel, cx: &App) -> Option<AnyElement> {
    use domain::session_tree::{guide_menu_entries, GuideMenuEntry};
    let pull = model.pull()?;
    let entries = guide_menu_entries(pull.state.as_deref(), thread_member(model, cx));
    if entries.is_empty() {
        return None;
    }
    let url = pull.url.as_deref().map(str::trim).filter(|url| !url.is_empty()).map(str::to_string);
    let pull_request_id = pull.id.clone();
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
        SharedString::from(THREAD.test_ids.guide_menu.clone()),
        Icon::new(registry::UI_MORE),
        cx,
    );
    Some(
        div()
            .flex_shrink_0()
            .mr(px(-crate::work_header::GHOST_ICON_HANG))
            .child(crate::controls::PointerMenu::for_button(trigger, move |mut menu, _, cx| {
                for entry in &entries {
                    menu = match entry {
                        GuideMenuEntry::OpenGithub => {
                            let url = url.clone();
                            menu.item(
                                crate::controls::pointer_label_item(
                                    domain::contract::DIFF_UI_OPEN_ON_GITHUB,
                                    url.is_none(),
                                )
                                .icon(Icon::new(registry::UI_GITHUB))
                                .on_click(move |_, _, cx| {
                                    if let Some(url) = url.clone() {
                                        crate::settings::open_url(cx, url);
                                    }
                                }),
                            )
                        }
                        GuideMenuEntry::ClosePr => {
                            let pull_request_id = pull_request_id.clone();
                            menu.item(
                                crate::controls::danger_menu_item(
                                    domain::contract::DIFF_UI_CLOSE_PR,
                                    Icon::new(registry::PR_CLOSED),
                                    cx,
                                )
                                .on_click(move |_, window, cx| {
                                    crate::issue_header::prompt_close_pr(
                                        pull_request_id.clone(),
                                        crate::issue_header::close_pr_description(others),
                                        window,
                                        cx,
                                    );
                                }),
                            )
                        }
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

/// EXP-1309: the chip resolver EVERY thread text renders through (an agent's
/// reply, a person's message, a plan, a question, a Guide section): web
/// `ThreadMarkdown`'s `bareIssueRefs` — BARE mode (EXP-760), because agents
/// write `FB-8` without a `#`; a token chips only when it resolves to a
/// synced issue of `team_id`. Display-only, the stored text is untouched.
pub(crate) fn thread_text_resolver(team_id: &str) -> RefResolver {
    RefResolver::from_store(team_id).bare(true)
}

/// A message body as read-only chat markdown, `#IDENT` / bare `ABC-12` /
/// `@email` pills scoped to `team_id` ([`thread_text_resolver`]). `images`
/// is the screen's one [`ImageCache`]: a person's `![..](/api/attachments/..)`
/// embed (the composer's pictures) fetches through it — a view without one
/// paints every picture as "Image unavailable" without a fetch.
pub(crate) fn message_markdown(
    id: SharedString,
    body: &str,
    team_id: Option<&str>,
    images: &Entity<ImageCache>,
) -> MarkdownView {
    let mut view = MarkdownView::new(id, body.to_string())
        .chat(true)
        .selectable(true)
        .autolink(true)
        .images(images.clone())
        .on_open_link(crate::markdown::open_agent_link);
    if let Some(team_id) = team_id {
        let team = team_id.to_string();
        view = view
            .resolver(thread_text_resolver(team_id))
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

/// The queued row's caption word (session-thread.json: a person's row
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

/// EXP-1317: THE `Remove from queue` X (the thread bubble and the session
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
        .tooltip(|window, cx| {
            gpui_component::tooltip::Tooltip::new(steer::feed::QUEUE_REMOVE_LABEL).build(window, cx)
        })
        .child(
            crate::controls::ghost_icon_button(
                SharedString::from(format!("{id}-x")),
                Icon::new(registry::UI_CLOSE).xsmall(),
                cx,
            )
            .on_click(on_click),
        )
        .into_any_element()
}

/// A person's message: the right-aligned bubble with its caption under it;
/// a queued one reads `Queued`, and its AUTHOR (`own`) may take it back.
pub(crate) fn user_bubble(
    id: &str,
    body: MarkdownView,
    caption: String,
    queued: bool,
    own: bool,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let unqueue_id = id.to_string();
    let unqueue = unqueue_offered(queued, own).then(|| {
        unqueue_button("thread-unqueue", id, queued, cx, move |_, _, cx| {
            unqueue_message(unqueue_id.clone(), cx)
        })
    });
    let caption_row = h_flex()
        .gap_1()
        .items_center()
        .text_2xs()
        .text_color(muted)
        .when(!caption.is_empty(), |row| row.child(SharedString::from(caption)))
        .when(queued, |row| {
            row.child(SharedString::from(format!("· {QUEUED_WORD}")))
        })
        .children(unqueue);
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

/// The free-text row a thread question card adds under its options (the
/// card's options never carry it; the session view reads the agent's own
/// row, claude's `Type something.`).
pub(crate) const FREE_TEXT_LABEL: &str = "Type something.";
/// A plan card's heading (the session view's).
pub(crate) const PLAN_HEADING: &str = "Plan ready";

/// The one inline field open on a thread card: a question's free-text row
/// or a plan's Reject note (EXP-820, the session view's rule).
struct CardInlineState {
    message_id: String,
    plan_reject: bool,
    state: Entity<InputState>,
    _subscription: Subscription,
}

#[derive(Default)]
struct CardInline(Option<CardInlineState>);

impl gpui::Global for CardInline {}

fn card_inline_open(message_id: &str, cx: &App) -> Option<(bool, Entity<InputState>)> {
    cx.try_global::<CardInline>()?
        .0
        .as_ref()
        .filter(|open| open.message_id == message_id)
        .map(|open| (open.plan_reject, open.state.clone()))
}

/// Open the inline field on `message_id`'s free-text row (or its plan
/// Reject), focused; Enter sends it.
fn open_card_inline(message_id: &str, plan_reject: bool, window: &mut Window, cx: &mut App) {
    if card_inline_open(message_id, cx).is_some_and(|(open_reject, _)| open_reject == plan_reject) {
        return;
    }
    let placeholder = if plan_reject {
        crate::steer_viewer::PLAN_REJECT_PLACEHOLDER
    } else {
        crate::steer_viewer::FREE_TEXT_PLACEHOLDER
    };
    let state = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
    let subscription = cx.subscribe(&state, |_, event: &InputEvent, cx| {
        if let InputEvent::PressEnter { shift: false, .. } = event {
            send_card_inline(cx);
        }
    });
    state.update(cx, |state, cx| state.focus(window, cx));
    cx.set_global(CardInline(Some(CardInlineState {
        message_id: message_id.to_string(),
        plan_reject,
        state,
        _subscription: subscription,
    })));
    cx.refresh_windows();
}

/// Send the open inline field: a question's typed answer (needs text), a
/// plan's reject with its note (an empty note is a plain reject).
fn send_card_inline(cx: &mut App) {
    let Some(open) = cx.try_global::<CardInline>().and_then(|inline| inline.0.as_ref()) else {
        return;
    };
    let text = open.state.read(cx).value().trim().to_string();
    let input = if open.plan_reject {
        api::sessions::AnswerInput {
            message_id: open.message_id.clone(),
            decision: Some("rejected".to_string()),
            feedback: (!text.is_empty()).then_some(text),
            ..Default::default()
        }
    } else if text.is_empty() {
        return;
    } else {
        api::sessions::AnswerInput {
            message_id: open.message_id.clone(),
            answer: Some(text),
            ..Default::default()
        }
    };
    cx.set_global(CardInline(None));
    answer_card(input, cx);
}

/// The open inline field under its row: the glass field + the round Send.
fn card_inline_field(message_id: &str, state: &Entity<InputState>, plan_reject: bool, window: &Window, cx: &App) -> AnyElement {
    let empty = state.read(cx).value().trim().is_empty();
    let send = crate::composer::composer_submit_kind(
        SharedString::from(format!("thread-card-send-{message_id}")),
        crate::composer::SubmitKind::Send,
        !plan_reject && empty,
        cx,
    )
    .on_click(|_, _, cx| {
        cx.stop_propagation();
        send_card_inline(cx);
    });
    crate::steer_viewer::ask_inline_field(state, send, window, cx).into_any_element()
}

/// `Answering…` while the card's answer is on its way.
fn card_answering(cx: &App) -> AnyElement {
    h_flex()
        .gap_1p5()
        .items_center()
        .mt_2()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(gpui_component::spinner::Spinner::new().xsmall())
        .child("Answering…")
        .into_any_element()
}

/// EXP-1316 — a question card = THE session view's card: the question over
/// its numbered option rows and (`custom`) the free-text row that opens an
/// inline field; once answered, CLOSED (the check + the chosen answer).
#[allow(clippy::too_many_arguments)] // one row's facts
pub(crate) fn question_card(
    message_id: &str,
    question: MarkdownView,
    options: &[QuestionOption],
    custom: bool,
    answer: Option<&str>,
    answered: bool,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    let mut column = v_flex()
        .w_full()
        .min_w_0()
        .child(crate::steer_viewer::ask_body_text(cx).child(question));
    if answered {
        column = column.child(crate::steer_viewer::ask_resolved_line(answer, false, cx));
    } else if answering(message_id, cx) {
        column = column.child(card_answering(cx));
    } else {
        let inline = card_inline_open(message_id, cx).filter(|(plan_reject, _)| !plan_reject);
        let mut rows = v_flex().mt_2().w_full().min_w_0().gap_1();
        for (index, option) in options.iter().enumerate() {
            let id = message_id.to_string();
            let value = option.label.clone();
            rows = rows.child(
                crate::steer_viewer::ask_option_button(
                    SharedString::from(format!("thread-question-{message_id}-{index}")),
                    option.label.clone(),
                    option.description.clone(),
                    false,
                    false,
                    false,
                    index,
                    cx,
                )
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    answer_card(
                        api::sessions::AnswerInput {
                            message_id: id.clone(),
                            answer: Some(value.clone()),
                            ..Default::default()
                        },
                        cx,
                    );
                }),
            );
        }
        if custom {
            let id = message_id.to_string();
            rows = rows.child(
                crate::steer_viewer::ask_option_button(
                    SharedString::from(format!("thread-question-{message_id}-custom")),
                    FREE_TEXT_LABEL.to_string(),
                    None,
                    false,
                    inline.is_some(),
                    false,
                    options.len(),
                    cx,
                )
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    open_card_inline(&id, false, window, cx);
                }),
            );
            if let Some((_, state)) = &inline {
                rows = rows.child(card_inline_field(message_id, state, false, window, cx));
            }
        }
        column = column.child(rows);
    }
    crate::steer_viewer::ask_card(false, None, None, column, cx)
}

/// EXP-1316 — a plan card = THE session view's plan card: `Plan ready`,
/// the plan, Approve (primary) and Reject (opens the note field); once
/// decided, CLOSED (the check + the decision).
pub(crate) fn plan_card(
    message_id: &str,
    plan: MarkdownView,
    decision: Option<&str>,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    let mut column = v_flex()
        .w_full()
        .min_w_0()
        .child(crate::steer_viewer::ask_body_text(cx).child(plan));
    match decision {
        Some(decision) => {
            column = column.child(crate::steer_viewer::ask_resolved_line(Some(&decision_word(decision)), false, cx));
        }
        None if answering(message_id, cx) => column = column.child(card_answering(cx)),
        None => {
            let inline = card_inline_open(message_id, cx).filter(|(plan_reject, _)| *plan_reject);
            let approve_id = message_id.to_string();
            let reject_id = message_id.to_string();
            let mut rows = v_flex()
                .mt_2()
                .w_full()
                .min_w_0()
                .gap_1()
                .child(
                    crate::steer_viewer::ask_option_button(
                        SharedString::from(format!("thread-plan-{message_id}-approved")),
                        PLAN_APPROVE_LABEL.to_string(),
                        None,
                        true,
                        false,
                        false,
                        0,
                        cx,
                    )
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        answer_card(
                            api::sessions::AnswerInput {
                                message_id: approve_id.clone(),
                                decision: Some("approved".to_string()),
                                ..Default::default()
                            },
                            cx,
                        );
                    }),
                )
                .child(
                    crate::steer_viewer::ask_option_button(
                        SharedString::from(format!("thread-plan-{message_id}-rejected")),
                        PLAN_REJECT_LABEL.to_string(),
                        None,
                        false,
                        inline.is_some(),
                        false,
                        1,
                        cx,
                    )
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        open_card_inline(&reject_id, true, window, cx);
                    }),
                );
            if let Some((_, state)) = &inline {
                rows = rows.child(card_inline_field(message_id, state, true, window, cx));
            }
            column = column.child(rows);
        }
    }
    crate::steer_viewer::ask_card(true, Some(SharedString::from(PLAN_HEADING)), None, column, cx)
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
        None,
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
    window: &Window,
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
            custom,
            answer,
            answered,
        } => question_card(
            id,
            message_markdown(SharedString::from(format!("thread-question-{id}")), question, team_id, images),
            options,
            *custom,
            answer.as_deref(),
            *answered,
            window,
            cx,
        ),
        SessionThreadRow::Plan { id, plan, decision } => plan_card(
            id,
            message_markdown(SharedString::from(format!("thread-plan-{id}")), plan, team_id, images),
            decision.as_deref(),
            window,
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
        SessionThreadRow::Report { id, session_id, text, .. } => report_line(id, session_id, text, root_id, cx),
        SessionThreadRow::Suggestions {
            id,
            header,
            start_all,
            items,
            ..
        } => suggestions_card(id, header, *start_all, items, root_id, team_id, cx),
    })
}

// ---------------------------------------------------------------------------
// EXP-1323: a child's report line + THE suggested tasks card
// ---------------------------------------------------------------------------

/// A child's end report = ONE event line `↩ Report from <session> ›`
/// opening that session; its summary is never repeated (the parent's next
/// message relays it).
pub(crate) fn report_line(id: &str, session_id: &str, text: &str, root_id: &str, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let hover = cx.theme().foreground;
    let session_id = session_id.to_string();
    let root_id = root_id.to_string();
    h_flex()
        .id(SharedString::from(format!("{}-{id}", THREAD.test_ids.report)))
        .max_w_full()
        .min_w_0()
        .gap_1p5()
        .items_center()
        .text_xs()
        .text_color(muted)
        .cursor_pointer()
        .hover(move |style| style.text_color(hover))
        .child(Icon::new(registry::NOTIFICATION_REPORTER_REPLY).xsmall().flex_shrink_0())
        .child(div().min_w_0().truncate().child(SharedString::from(text.to_string())))
        .child(Icon::new(registry::UI_CHEVRON_RIGHT).xsmall().flex_shrink_0())
        .on_click(move |_, window, cx| open_session_card(&session_id, &root_id, window, cx))
        .into_any_element()
}

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
                log::warn!("[thread] {verb} {thing} failed: {err}");
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
/// open), then hairline-divided items — open = title, text, × · Options ·
/// Do it here · Start; started = the child's session card (its caption
/// under the name) opening it; here = `Sent here`; filed = `Filed EXP-9`.
pub(crate) fn suggestions_card(
    message_id: &str,
    header: &str,
    start_all: bool,
    items: &[domain::session_tree::SuggestionRow],
    root_id: &str,
    team_id: Option<&str>,
    cx: &App,
) -> AnyElement {
    use gpui_component::button::Button;
    let copy = &THREAD.copy.suggestions;
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
                Button::new(SharedString::from(format!("{}-start-all-{message_id}", THREAD.test_ids.suggestions)))
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
        .map(|item| suggestion_item_row(message_id, item, root_id, team_id, cx));
    v_flex()
        .id(SharedString::from(format!("{}-{message_id}", THREAD.test_ids.suggestions)))
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(theme::tokens::radius::LG))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .bg(theme::tokens::glass::FILL_CARD.to_hsla())
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
    root_id: &str,
    team_id: Option<&str>,
    cx: &App,
) -> AnyElement {
    use domain::session_tree::SuggestionState;
    use gpui_component::{button::Button, Disableable as _};
    let copy = &THREAD.copy.suggestions;
    let muted = cx.theme().muted_foreground;
    let line = || h_flex().w_full().min_w_0().min_h(px(36.)).px_3().py_1p5().gap_2().items_center();
    match item.state {
        SuggestionState::Started => match &item.card {
            Some(card) => {
                let session_id = card.session_id.clone();
                let root_id = root_id.to_string();
                crate::work_card::session_card_bare(
                    card,
                    move |window, cx| open_session_card(&session_id, &root_id, window, cx),
                    cx,
                )
            }
            // Started, the child's row not synced yet.
            None => line()
                .text_color(muted)
                .child(gpui_component::spinner::Spinner::new().xsmall())
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
                            .id(SharedString::from(format!("thread-suggestion-filed-{message_id}-{}", item.id)))
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
            let id = |part: &str| SharedString::from(format!("thread-suggestion-{part}-{message_id}-{}", item.id));
            let (dismiss_message, dismiss_item) = (message_id.to_string(), item.id.clone());
            let (here_message, here_item) = (message_id.to_string(), item.id.clone());
            let (start_message, start_item) = (message_id.to_string(), item.id.clone());
            let options = Button::new(id("options"))
                .outline()
                .web_xs()
                .label(copy.options.clone())
                .dropdown_caret(true)
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
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_2()
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
                                    this.child(
                                        div().mt_0p5().text_color(muted).child(SharedString::from(text.to_string())),
                                    )
                                }),
                        )
                        .child(
                            Button::new(id("dismiss"))
                                .ghost()
                                .web_icon_xs()
                                .icon(Icon::new(registry::UI_CLOSE))
                                .tooltip(copy.dismiss.clone())
                                .disabled(busy)
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    dismiss_suggestion(dismiss_message.clone(), dismiss_item.clone(), cx);
                                }),
                        ),
                )
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .gap_1p5()
                        .items_center()
                        .child(crate::controls::PointerMenu::for_button(options, move |menu, window, cx| {
                            suggestion_menu(menu, &menu_message, &menu_item, menu_team.as_deref(), window, cx)
                        }))
                        .child(
                            Button::new(id("here"))
                                .outline()
                                .web_xs()
                                .label(copy.here.clone())
                                .disabled(busy)
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    start_suggestion(&here_message, &here_item, true, None, None, cx);
                                }),
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

/// An open item's Options = THE Menu: Start on… (the team's runnable
/// devices) · Start with… (the contract models, never a default row) ·
/// File as issue · Dismiss.
fn suggestion_menu(
    menu: gpui_component::menu::PopupMenu,
    message_id: &str,
    item_id: &str,
    team_id: Option<&str>,
    window: &mut Window,
    cx: &mut gpui::Context<gpui_component::menu::PopupMenu>,
) -> gpui_component::menu::PopupMenu {
    use crate::controls::{danger_menu_item, pointer_menu_item};
    let copy = &THREAD.copy.suggestions;
    let devices = team_id
        .map(|team_id| crate::settings::agent_detail::agent_devices(team_id, cx))
        .unwrap_or_default();
    let (device_message, device_item) = (message_id.to_string(), item_id.to_string());
    let (model_message, model_item) = (message_id.to_string(), item_id.to_string());
    let (file_message, file_item) = (message_id.to_string(), item_id.to_string());
    let (dismiss_message, dismiss_item) = (message_id.to_string(), item_id.to_string());
    menu.min_w(px(224.))
        .submenu(copy.start_on.clone(), window, cx, move |mut sub, _, _| {
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
        .submenu(copy.start_with.clone(), window, cx, move |mut sub, _, _| {
            for model in domain::contract::CODING_MODEL_VALUES {
                let (message_id, item_id) = (model_message.clone(), model_item.clone());
                let label = crate::steer_viewer::model_label(model);
                let model = model.to_string();
                sub = sub.item(pointer_menu_item(None, label, |_| None, move |_, cx| {
                    start_suggestion(&message_id, &item_id, false, None, Some(model.clone()), cx)
                }));
            }
            sub
        })
        .separator()
        .item(pointer_menu_item(Some(registry::UI_ADD), copy.file.clone(), |_| None, move |_, cx| {
            file_suggestion(file_message.clone(), file_item.clone(), cx)
        }))
        .item(
            danger_menu_item(copy.dismiss.clone(), Icon::new(registry::UI_CLOSE), cx).on_click(move |_, _, cx| {
                dismiss_suggestion(dismiss_message.clone(), dismiss_item.clone(), cx)
            }),
        )
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
    with_needs_you_dot(mark.with_size(px(size)), needs_you, badge)
}

/// `mark` with the amber needs-you dot on its top-right corner while
/// `needs_you` ([`root_mark`]'s badge; the Threads rows put it on a PR glyph).
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
// EXP-1316 (1b): the session chips and the tree popover
// ---------------------------------------------------------------------------

/// One row of the session-tree popover: the whole tree, flat, in tree
/// order (a parent, then its children by creation), each with its tree
/// guides (`list-item.json` SessionRow small + TreeGuides).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SessionTreeRow {
    pub(crate) session_id: String,
    pub(crate) name: String,
    pub(crate) state: domain::session_tree::SessionState,
    pub(crate) cli: Option<String>,
    pub(crate) guides: domain::tree_guides::Guides,
}

/// The tree under `root_id` as popover rows (pure): depth-first, children in
/// creation order, names off THE session card (`Session N` by tree
/// ordinal).
pub(crate) fn session_tree_rows(root_id: &str, sessions: &[&SessionRow], now_ms: i64) -> Vec<SessionTreeRow> {
    use domain::session_tree::{session_card, tree_order};
    let tree: Vec<&SessionRow> = sessions
        .iter()
        .copied()
        .filter(|session| session.id == root_id || session.root_id_or_self() == root_id)
        .collect();
    let ordered = tree_order(&tree);
    let ordinal = |id: &str| ordered.iter().position(|session| session.id == id).map_or(1, |ix| ix + 1);
    let mut out: Vec<(&SessionRow, usize)> = Vec::new();
    fn walk<'a>(parent: &'a SessionRow, depth: usize, ordered: &[&'a SessionRow], out: &mut Vec<(&'a SessionRow, usize)>) {
        out.push((parent, depth));
        for child in ordered.iter().copied().filter(|child| child.parent_id.as_deref() == Some(parent.id.as_str())) {
            walk(child, depth + 1, ordered, out);
        }
    }
    if let Some(root) = ordered.iter().copied().find(|session| session.id == root_id) {
        walk(root, 0, &ordered, &mut out);
    }
    let guides = domain::tree_guides::guides_for(&out.iter().map(|(_, depth)| *depth).collect::<Vec<_>>());
    out.into_iter()
        .zip(guides)
        .map(|((session, _), guides)| SessionTreeRow {
            session_id: session.id.clone(),
            name: session_card(session, ordinal(&session.id), now_ms).name,
            state: domain::session_tree::session_state(session),
            cli: session.cli.clone(),
            guides,
        })
        .collect()
}

/// A chip's mark state as THE session state (the run mark reads it).
fn chip_state(mark: domain::session_tree::SessionTabMarkState) -> domain::session_tree::SessionState {
    use domain::session_tree::{SessionState, SessionTabMarkState};
    match mark {
        SessionTabMarkState::NeedsYou => SessionState::NeedsYou,
        SessionTabMarkState::Working => SessionState::Working,
        SessionTabMarkState::Waiting => SessionState::Waiting,
        SessionTabMarkState::Ended => SessionState::Ended,
    }
}

/// Open `session_id` IN PLACE (its thread hosts the raw work).
fn open_session_in_place(session_id: String, window: &mut Window, cx: &mut App) {
    crate::navigation::navigate(window, cx, Screen::Session { session_id });
}

/// One popover row: guides, the run mark, the name, the state word in its
/// tone; the current session washed.
fn tree_popover_row(row: &SessionTreeRow, active: bool, cx: &App) -> AnyElement {
    use domain::list_item::{lead_x, BASE, GAP, SMALL};
    let theme = cx.theme();
    let row_hover = theme::tokens::glass::FILL_ROW.to_hsla();
    let row_active = theme::tokens::glass::FILL_ACTIVE.to_hsla();
    let session_id = row.session_id.clone();
    let word = row.state.word();
    let word_color = match row.state {
        domain::session_tree::SessionState::NeedsYou | domain::session_tree::SessionState::Blocked => {
            crate::work_card::session_tone_color(row.state.tone(), cx)
        }
        _ => theme.muted_foreground,
    };
    crate::surface::flat_row()
        .id(SharedString::from(format!("thread-tree-{}", row.session_id)))
        .relative()
        .flex()
        .flex_row()
        .w_full()
        .min_w_0()
        .items_center()
        .gap(px(GAP))
        .h(px(SMALL))
        .pr_3()
        .pl(px(lead_x(row.guides.depth())))
        .rounded(px(theme::tokens::radius::MD))
        .when(active, |this| this.bg(row_active))
        .cursor_pointer()
        .hover(move |style| style.bg(if active { row_active } else { row_hover }))
        .children(crate::tree_guides::guide_layer(&row.guides, BASE, 0.))
        .child(crate::run_rows::run_mark(crate::run_rows::run_agent(row.cli.as_deref()), row.state, cx))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_sm()
                .text_color(theme.foreground)
                .child(SharedString::from(row.name.clone())),
        )
        .child(div().flex_shrink_0().text_sm().text_color(word_color).child(SharedString::from(word)))
        .on_click(move |_, window, cx| open_session_in_place(session_id.clone(), window, cx))
        .into_any_element()
}

/// Hovering the session chips (or the popover itself) keeps the tree
/// popover open; leaving both closes it after a beat.
#[derive(Default)]
struct TreeHover {
    chips: bool,
    popover: bool,
    open: bool,
}

/// How long the popover outlives the pointer (crossing the gap from the
/// chips onto it).
const TREE_CLOSE_DELAY: Duration = Duration::from_millis(160);

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

fn thread_model(root_id: &str, focus: Option<&str>, cx: &App) -> Option<ThreadModel> {
    let view = crate::session_data::session_thread_view(root_id, focus, crate::session_data::now_ms(), cx)?;
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
/// diff, the hovered stack member. Merges ride [`crate::pr_merge`]'s state.
#[derive(Default)]
struct GuideState {
    page: Option<GuidePage>,
    diff: Option<(String, Entity<crate::pr_diff::PrDiffView>)>,
    stack_hovered: Option<String>,
}

pub(crate) struct ThreadScreenView {
    root_id: String,
    child: ThreadChildSel,
    model: Option<ThreadModel>,
    /// The issue child's detail view (built on first show, re-pointed).
    issue_view: Option<Entity<crate::issue_detail::IssueDetailView>>,
    guide: GuideState,
    images: Entity<ImageCache>,
    /// THE session composer, targeting the ROOT: the session view over the
    /// root's live source (`session_screen::root_composer_view`), whose
    /// composer stack this screen paints under the conversation.
    composer: Entity<crate::steer_viewer::SteerSessionView>,
    /// When each issue link was FIRST rendered, and whether it was new then
    /// (linked inside [`NEW_CHILD_WINDOW`]) — the fade-in ring.
    first_seen: HashMap<String, (Instant, bool)>,
    opened_at: Instant,
    scroll: ScrollHandle,
    /// EXP-1316 (1n): the conversation follows its tail like the session
    /// feed (opens on the newest row, pinned until the reader scrolls up).
    follow: crate::controls::TailFollow,
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
    tree: TreeHover,
    _tree_close: Option<Task<()>>,
    _missing_tick: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl ThreadScreenView {
    pub(crate) fn new(root_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let composer = crate::session_screen::root_composer_view(
            &root_id,
            THREAD.copy.placeholder.clone(),
            crate::steer_viewer::ComposerIds::new(
                THREAD.test_ids.composer.clone(),
                "thread-attach",
                "thread-model",
                "thread-context",
            ),
            window,
            cx,
        );
        // The composer repaints with its root's feed (the model pin, the
        // ring, Send⇄Stop, the queue strip).
        let mut subscriptions = vec![cx.observe(&composer, |_, _, cx| cx.notify())];
        if let Some(store) = Store::try_global(cx) {
            let collections = store.collections().clone();
            subscriptions.push(cx.observe(&collections.sessions, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&collections.session_issues, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&collections.messages, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&collections.pull_requests, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&collections.issues, |this, _, cx| this.refresh(cx)));
        }
        // The merge capsule's armed / merging state.
        let merge_state = crate::pr_merge::MergeState::global(cx);
        subscriptions.push(cx.observe(&merge_state, |_, _, cx| cx.notify()));
        let transport = crate::queries::attachment_transport(cx);
        let images = cx.new(|_| ImageCache::new(transport));
        // A picture tile (the Guide's, a picture card) asks the cache for its
        // slot while painting and gets `Loading`; the bytes land later and
        // only the CACHE notifies — without this the tile stayed an empty
        // frame until something else re-rendered the thread.
        subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
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
            composer,
            first_seen: HashMap::new(),
            opened_at: Instant::now(),
            scroll: ScrollHandle::new(),
            follow: crate::controls::TailFollow::default(),
            focus_handle: cx.focus_handle(),
            _ring_tick: None,
            focus: None,
            _focus_sub: None,
            tail: None,
            _tail_sub: None,
            tree: TreeHover::default(),
            _tree_close: None,
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
        let focus = self.focus.as_ref().map(|(id, _)| id.clone());
        let model = thread_model(&self.root_id, focus.as_deref(), cx);
        if let Some(model) = &model {
            self.note_seen(&model.links, cx);
            // An unlinked issue's child closes.
            if let ThreadChildSel::Issue(issue_id) = &self.child {
                if !model.linked_ids().contains(issue_id) {
                    self.child = ThreadChildSel::Conversation;
                }
            }
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

    /// The tab closed: an open issue child flushes its edit (EXP-1323: and
    /// the live tail's viewer lets go of its relay socket).
    pub(crate) fn shutdown(&mut self, cx: &mut App) {
        if let Some((_, view)) = self.tail.take() {
            view.update(cx, |view, cx| view.shutdown(cx));
        }
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
        self.guide.page = None;
        self.child = child;
        cx.notify();
    }

    /// EXP-1316 (1b): show `focus`'s session IN PLACE (the panel calls this
    /// on a `Screen::Session` of this tree), or the thread again (`None`).
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
        self.tree.open = false;
        self.refresh(cx);
    }

    /// The focused session's id.
    pub(crate) fn focus_id(&self) -> Option<&str> {
        self.focus.as_ref().map(|(id, _)| id.as_str())
    }

    /// A strip chip other than a session: leave the in-place session (back
    /// to `Screen::Thread`, the same tab) and show `child`.
    fn select_child(&mut self, child: ThreadChildSel, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.focus.is_some() {
            let session_id = self.root_id.clone();
            window.defer(cx, move |window, cx| {
                crate::navigation::navigate(window, cx, Screen::Thread { session_id });
            });
        }
        self.set_child(child, window, cx);
    }

    /// The session chips' / popover's hover edge.
    fn tree_hover(&mut self, chips: Option<bool>, popover: Option<bool>, cx: &mut gpui::Context<Self>) {
        if let Some(chips) = chips {
            self.tree.chips = chips;
        }
        if let Some(popover) = popover {
            self.tree.popover = popover;
        }
        if self.tree.chips || self.tree.popover {
            self._tree_close = None;
            if !self.tree.open {
                self.tree.open = true;
                cx.notify();
            }
            return;
        }
        self._tree_close = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TREE_CLOSE_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                if !this.tree.chips && !this.tree.popover && this.tree.open {
                    this.tree.open = false;
                    cx.notify();
                }
            });
        }));
    }

    /// How many rows the hover tree would list (web `treeRows.length`).
    fn tree_row_count(&self, cx: &App) -> usize {
        let sessions = crate::session_data::tree_of_root(&self.root_id, cx);
        let refs: Vec<&SessionRow> = sessions.iter().collect();
        session_tree_rows(&self.root_id, &refs, crate::session_data::now_ms()).len()
    }

    /// The tree popover under the session chips.
    fn render_tree_popover(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let sessions = crate::session_data::tree_of_root(&self.root_id, cx);
        let refs: Vec<&SessionRow> = sessions.iter().collect();
        let rows = session_tree_rows(&self.root_id, &refs, crate::session_data::now_ms());
        let current = self.focus_id().map(str::to_string);
        let theme = cx.theme();
        let panel = v_flex()
            .id("thread-tree-popover")
            .min_w(px(320.))
            .max_w(px(460.))
            .p_1()
            .rounded(px(theme::tokens::radius::LG))
            .bg(theme.popover)
            .border_1()
            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
            .shadow_lg()
            .children(
                rows.iter()
                    .map(|row| tree_popover_row(row, current.as_deref() == Some(row.session_id.as_str()), cx)),
            )
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| this.tree_hover(None, Some(*hovered), cx)));
        div()
            .absolute()
            .top(px(STRIP_CHIP_H + 6.))
            .right_0()
            .child(gpui::deferred(panel).with_priority(1))
            .into_any_element()
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
        // EXP-1316: the title is derived, plain text (no rename).
        let title = title_row(&model.view.title, model.view.private, cx).id("thread-title");
        // EXP-1316: the trailing slot on EVERY tab — the Merge pill while the
        // Guide's PR is open (`Merge PR` | `Merge stack`), then the Guide `…`.
        let merge = model.pull().filter(|pull| pull.is_open()).map(|pull| {
            crate::work_header::merge_pill(
                SharedString::from(THREAD.test_ids.guide_merge.clone()),
                &pull.id,
                true,
                crate::surface::PillSize::Md,
                cx,
            )
        });
        // EXP-1316 (1b): the open session's Stop while it is live.
        let stop = self.focus.as_ref().filter(|(_, view)| view.read(cx).stoppable(cx)).map(|(session_id, _)| {
            let session_id = session_id.clone();
            crate::session_screen::stop_session_pill(
                "thread-session-stop",
                crate::surface::PillSize::Md,
                cx,
            )
            .on_click(move |_, _, cx| crate::steer_viewer::stop_session(&session_id, cx))
        });
        let menu = guide_menu(model, cx);
        v_flex()
            .w_full()
            .px(px(HEADER_INSET))
            .pt_6()
            .gap_4()
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .items_center()
                    .child(div().flex_1().min_w_0().child(title))
                    .children(merge)
                    .children(stop)
                    .children(menu),
            )
            .child(self.render_strip(model, cx))
            .child(div().w_full().h(px(1.)).bg(cx.theme().border))
            .into_any_element()
    }

    fn render_strip(&mut self, model: &ThreadModel, cx: &mut gpui::Context<Self>) -> AnyElement {
        let mut chips: Vec<AnyElement> = Vec::new();
        // The Guide (after the divider) never scrolls out of reach.
        let mut trailing: Vec<AnyElement> = Vec::new();
        let mut sessions: Vec<AnyElement> = Vec::new();
        // EXP-1316: Conversation leads; the issue chips shrink (each title
        // ellipsized at one max width) and scroll past their minimum; the
        // `+` sits right after them OUTSIDE that scroller, so it is never
        // scrolled or clipped away.
        let mut lead: Vec<AnyElement> = Vec::new();
        let mut add: Option<AnyElement> = None;
        let member = thread_member(model, cx);
        for tab in &model.view.tabs {
            match tab {
                ThreadTab::Conversation => {
                    let active = self.focus.is_none() && self.child == ThreadChildSel::Conversation;
                    lead.push(
                        strip_chip("thread-child-conversation", active, cx)
                            .child(Icon::new(registry::ACTION_CHAT).xsmall())
                            .child(SharedString::from(THREAD.copy.conversation.clone()))
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.select_child(ThreadChildSel::Conversation, window, cx)
                            }))
                            .into_any_element(),
                    );
                }
                ThreadTab::Issue { issue_id, identifier } => {
                    let active = self.focus.is_none() && self.child == ThreadChildSel::Issue(issue_id.clone());
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
                        .flex_shrink(1.)
                        .min_w(px(ISSUE_CHIP_MIN_W))
                        .children(strip_issue_body(status, identifier, &title, active, cx))
                        .when(ringed, |chip| chip.border_color(cx.theme().ring))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.select_child(ThreadChildSel::Issue(open.clone()), window, cx)
                        }))
                        .pointer_context_menu(move |menu, window, cx| thread_menu(menu, &target, window, cx));
                    chips.push(if ringed {
                        div()
                            .flex()
                            .flex_shrink(1.)
                            .min_w(px(ISSUE_CHIP_MIN_W))
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
                // Web `add={permissions.isMember ? … : undefined}`: a
                // non-member sees no `+` (the divider still splits the strip).
                ThreadTab::Add if !member => trailing.push(strip_divider(cx)),
                ThreadTab::Add => {
                    let target = self.menu_target(None);
                    let trigger =
                        crate::controls::ghost_icon_button("thread-child-add", Icon::new(registry::UI_ADD), cx);
                    add = Some(
                        div()
                            .flex_shrink_0()
                            .child(crate::controls::PointerMenu::new(
                                "thread-child-add-menu",
                                trigger,
                                move |menu, window, cx| thread_menu(menu, &target, window, cx),
                            ))
                            .into_any_element(),
                    );
                    trailing.push(strip_divider(cx));
                }
                // EXP-1316 (1b): the sessions at the viewed level — the
                // agent's mark + the name, muted unless open in place.
                ThreadTab::Session {
                    session_id,
                    name,
                    mark_state,
                } => {
                    let active = self.focus_id() == Some(session_id.as_str());
                    let cli = crate::session_data::session(session_id, cx).and_then(|row| row.cli);
                    let open = session_id.clone();
                    sessions.push(
                        strip_chip(SharedString::from(format!("thread-session-{session_id}")), active, cx)
                            .child(crate::run_rows::run_mark(
                                crate::run_rows::run_agent(cli.as_deref()),
                                chip_state(*mark_state),
                                cx,
                            ))
                            .child(div().max_w(px(180.)).truncate().child(SharedString::from(name.clone())))
                            .on_click(move |_, window, cx| open_session_in_place(open.clone(), window, cx))
                            .into_any_element(),
                    );
                }
                ThreadTab::Guide { dot } => {
                    if !sessions.is_empty() {
                        // Web: the hover tree only for a tree of MORE than
                        // one row (`treeRows.length > 1`) — a lone session
                        // has nothing to map.
                        let tree = self.tree_row_count(cx) > 1;
                        let group = div()
                            .id("thread-session-chips")
                            .relative()
                            .flex()
                            .flex_row()
                            .flex_shrink_0()
                            .items_center()
                            .gap_1()
                            .children(std::mem::take(&mut sessions))
                            .when(tree, |group| {
                                group.on_hover(
                                    cx.listener(|this, hovered: &bool, _, cx| this.tree_hover(Some(*hovered), None, cx)),
                                )
                            })
                            .when(tree && self.tree.open, |group| group.child(self.render_tree_popover(cx)));
                        trailing.push(group.into_any_element());
                        trailing.push(strip_divider(cx));
                    }
                    let active = self.focus.is_none() && self.child == ThreadChildSel::Guide;
                    trailing.push(
                        strip_chip("thread-child-guide", active, cx)
                            .child(Icon::new(registry::GUIDE_CHANGES).xsmall())
                            .child(SharedString::from(THREAD.copy.guide.clone()))
                            .when(*dot, |chip| chip.child(guide_dot(cx)))
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.select_child(ThreadChildSel::Guide, window, cx)
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
            .children(lead)
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
            .children(add)
            .children(trailing)
            .into_any_element()
    }

    fn render_row(
        &self,
        row: &SessionThreadRow,
        model: &ThreadModel,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let team_id = model.root.team_id.as_deref();
        if let SessionThreadRow::Message {
            id,
            author,
            author_id,
            body,
            queued,
            ..
        } = row
        {
            let markdown = message_markdown(
                SharedString::from(format!("thread-message-{id}")),
                body,
                team_id,
                &self.images,
            );
            return match author.as_str() {
                domain::contract::MESSAGE_AUTHOR_KIND_USER => {
                    let at = model.messages.get(id).and_then(|message| message.created_at.as_deref());
                    let name = member_name(author_id.as_deref(), cx);
                    let own = author_id.is_some()
                        && crate::queries::active_account(cx).map(|me| me.user_id) == *author_id;
                    user_bubble(id, markdown, bubble_caption(Some(&name), &clock(at)), *queued, own, cx)
                }
                domain::contract::MESSAGE_AUTHOR_KIND_AGENT => agent_prose(markdown, cx),
                _ => muted_line(registry::UI_INFO, body.clone(), cx),
            };
        }
        card_row(row, &self.root_id, team_id, &self.images, window, cx).unwrap_or_else(|| div().into_any_element())
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

    /// EXP-1323: the live tail after the last row ([`sync_tail`]'s viewer).
    fn render_tail(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let (_, view) = self.tail.as_ref()?;
        let inner = view.read(cx).inner().clone();
        inner.update(cx, |inner, cx| inner.live_tail(cx))
    }

    fn render_conversation(&mut self, model: &ThreadModel, window: &Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let rows: Vec<AnyElement> = model
            .view
            .rows
            .iter()
            .map(|row| self.render_row(row, model, window, cx))
            .collect();
        let tail = self.render_tail(cx);
        let column = v_flex()
            .id(SharedString::from(THREAD.test_ids.conversation.clone()))
            .w_full()
            .max_w(px(CONVERSATION_W))
            .mx_auto()
            .px(px(crate::work_header::WORK_GUTTER))
            .pt_8()
            .pb_8()
            .gap_5()
            .children(rows)
            .children(tail);
        // THE session composer of the session view, targeting the root.
        let composer = self
            .composer
            .update(cx, |composer, cx| composer.render_session_composer(window, cx));
        v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(self.follow.scroller(
                "thread-conversation-scroll",
                &self.scroll,
                column,
                cx.entity_id(),
                cx,
            ))
            .children(composer.map(|composer| div().w_full().pb_2().child(composer)))
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
                    viewer: model.root.guide.clone().map(|guide| crate::session_results::GuideViewerSource {
                        guide: std::rc::Rc::new(guide),
                        team_id: model.root.team_id.clone(),
                    }),
                },
                cx,
            )
        };
        // EXP-1316: the Merge pill lives in the header's trailing slot.
        v_flex().flex_1().min_h_0().w_full().child(body).into_any_element()
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
        // EXP-1323: the live tail's viewer rides the Conversation only.
        let live = (self.focus.is_none() && self.child == ThreadChildSel::Conversation)
            .then(|| model.view.live_session_id.clone())
            .flatten();
        self.sync_tail(live.as_deref(), window, cx);
        // EXP-1323: what is on screen is read — the thread (the whole tree's
        // agent replies) or the session open in place (its own).
        match self.focus_id().map(str::to_string) {
            Some(focus) => crate::session_data::mark_read_if_unread(&focus, false, cx),
            None => crate::session_data::mark_read_if_unread(&self.root_id, true, cx),
        }
        // EXP-1316 (1b): a session open in place = its raw work under the
        // thread's own header + strip (no session header of its own).
        if let Some((_, view)) = self.focus.clone() {
            let inner = view.read(cx).inner().clone();
            let scrolled = inner.read(cx).body_scrolled();
            return v_flex()
                .track_focus(&self.focus_handle)
                .size_full()
                .min_h_0()
                .overflow_hidden()
                .child(header)
                .child(crate::work_header::work_body(
                    div().flex_1().min_h_0().child(inner).into_any_element(),
                    false,
                    scrolled,
                ));
        }
        let child = match self.child.clone() {
            ThreadChildSel::Conversation => self.render_conversation(&model, window, cx),
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

    /// EXP-1309: every thread text (messages, plans, questions, Guide
    /// sections) chips BARE `FB-8` tokens, like web `ThreadMarkdown`'s
    /// `bareIssueRefs` and the steering feed (EXP-760).
    #[test]
    fn thread_texts_chip_bare_identifiers() {
        assert!(thread_text_resolver("team-1").bare);
    }

    /// EXP-1316 (1b): the tree popover lists the WHOLE tree depth-first
    /// (children by creation), names off the session card, guides off the
    /// depths.
    #[test]
    fn the_tree_popover_walks_the_tree_depth_first() {
        let row = |value: serde_json::Value| -> SessionRow { serde_json::from_value(value).unwrap() };
        let sessions = [
            row(serde_json::json!({"id": "r", "root_id": "r", "kind": "thread", "status": "running", "busy": true, "created_at": "2026-10-14T10:00:00Z"})),
            row(serde_json::json!({"id": "b", "root_id": "r", "parent_id": "r", "depth": 1, "status": "ended", "created_at": "2026-10-14T10:02:00Z"})),
            row(serde_json::json!({"id": "a", "root_id": "r", "parent_id": "r", "depth": 1, "status": "idle", "needs_input": true, "title": "Profile the launch path", "created_at": "2026-10-14T10:01:00Z"})),
            row(serde_json::json!({"id": "a1", "root_id": "r", "parent_id": "a", "depth": 2, "status": "running", "created_at": "2026-10-14T10:03:00Z"})),
            row(serde_json::json!({"id": "x", "root_id": "x", "kind": "thread", "status": "idle", "created_at": "2026-10-14T09:00:00Z"})),
        ];
        let refs: Vec<&SessionRow> = sessions.iter().collect();
        let rows = session_tree_rows("r", &refs, 0);
        let ids: Vec<&str> = rows.iter().map(|row| row.session_id.as_str()).collect();
        assert_eq!(ids, vec!["r", "a", "a1", "b"]);
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, vec!["Session 1", "Profile the launch path", "Session 4", "Session 3"]);
        let depths: Vec<usize> = rows.iter().map(|row| row.guides.depth()).collect();
        assert_eq!(depths, vec![0, 1, 2, 1]);
        assert!(rows[1].guides.tee, "a sibling follows");
        assert_eq!(rows[1].state, domain::session_tree::SessionState::NeedsYou);
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
            unqueue_button_id("thread-unqueue", "m-1", true),
            unqueue_button_id("thread-unqueue", "m-1", false)
        );
        assert_ne!(
            unqueue_button_id("thread-unqueue", "m-1", true),
            unqueue_button_id("thread-unqueue", "m-2", true)
        );
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
