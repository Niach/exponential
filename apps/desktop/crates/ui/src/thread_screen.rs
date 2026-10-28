//! EXP-1294 — the THREAD screen ([`Screen::Thread`]): a thread IS a root
//! session (kind `thread`), its model the contract's
//! [`domain::session_tree::build_session_thread`] over the synced rows
//! ([`crate::session_data::thread_view_and_messages`], fixture
//! `session-thread.json`).
//!
//! * The HEADER: the title (EXP-1316: derived, plain text, nobody renames
//!   it) and a lock while private — no caption; its trailing slot holds the
//!   Merge pill while the thread's PR is open, then the `…`, on EVERY tab.
//! * The STRIP (EXP-1334, [`thread_strip`], never scrolls): `+` ·
//!   `Conversation` pinned left (S-3: no Guide tab, stories carry what a
//!   session shows); after them the TOP-LEVEL linked issues
//!   (status glyph, mono identifier, title) then the dispatcher's DIRECT
//!   children, laid out by `partition_thread_strip` (titles → identifiers
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
//! * NEW-6 (N-1) / NEW-46: a thread with ONE session (`view.main_session_id`,
//!   no child yet): its Conversation tab = that session's raw work in place
//!   (its own composer, Stop in the header's trailing slot). The tab stays
//!   in the strip; the first child turns it into the dispatcher's
//!   conversation.
//! * An ISSUE child = the issue detail, EMBEDDED (no work header bar, no
//!   Threads row).
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
use domain::session_tree::{SessionThreadRow, SessionThreadView, ThreadTab, THREAD, THREADS_LIST};

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
/// thread the composer just created.
pub(crate) const MISSING_GRACE: Duration = Duration::from_secs(5);

/// The conversation's reading column.
const CONVERSATION_W: f32 = 760.;
/// The header + strip's side inset.
const HEADER_INSET: f32 = 44.;
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
    let title = crate::session_data::session_title_of(root_id, cx)
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
    cx.spawn(async move |cx| {
        let result = cx.background_executor().spawn(async move { api::sessions::answer(&trpc, &input) }).await;
        match result {
            // The draft goes only now: a refusal (409, 400, the network)
            // re-opens the card with its picks and text intact.
            Ok(_) => {
                cx.update(|cx| crate::plan_card::drop_draft(&message_id, cx));
            }
            Err(err) => {
                log::warn!("[thread] answer the question failed: {err}");
                cx.update(|cx| {
                    cx.default_global::<AnsweringCards>().0.remove(&message_id);
                    cx.refresh_windows();
                    crate::action_error::failed_action_err("answer", "the question", &err, cx);
                });
            }
        }
    })
    .detach();
}

/// `sessions.unqueue` — take back a message the device has not consumed.
pub(crate) fn unqueue_message(message_id: String, cx: &mut App) {
    background_write("remove", "the message", cx, move |trpc| api::sessions::unqueue(trpc, &message_id));
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

/// THE issue chip's lead ×4: the issue's STATUS glyph (an unsynced status
/// reads as the builtin backlog default, never a document glyph).
pub(crate) fn issue_glyph(status: Option<&domain::statuses::ResolvedStatus>, cx: &App) -> Icon {
    let fallback;
    let status = match status {
        Some(status) => status,
        None => {
            fallback = domain::statuses::constructed_default(domain::IssueStatus::Backlog);
            &fallback
        }
    };
    crate::icons::resolved_status_icon(status, cx).xsmall().flex_shrink_0()
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
    vec![
        issue_glyph(status.as_ref(), cx).into_any_element(),
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
/// Close PR while it is open (members only, the web's
/// `canClose={permissions.isMember}`; EXP-1338: no GitHub entry). No
/// entries = none.
///
/// Thread 58cb32df (chats.json): the same `…` leads with the chat's own
/// entries (`threadMenuEntries`: Stop, Stop and delete | Delete), the Guide
/// PR's Close after a rule.
fn guide_menu(model: &ThreadModel, cx: &App) -> Option<AnyElement> {
    use domain::session_tree::{guide_menu_entries, GuideMenuEntry};
    let root_id = model.root.id.clone();
    let chat = !crate::chat_actions::chat_entries(&root_id, cx).is_empty();
    let pull = model.pull();
    let entries = pull
        .map(|pull| guide_menu_entries(pull.state.as_deref(), thread_member(model, cx)))
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
        SharedString::from(THREAD.test_ids.guide_menu.clone()),
        Icon::new(registry::UI_MORE),
        cx,
    );
    Some(
        div()
            .flex_shrink_0()
            .mr(px(-crate::work_header::GHOST_ICON_HANG))
            .child(crate::controls::PointerMenu::for_button(trigger, move |mut menu, _, cx| {
                menu = crate::chat_actions::chat_menu_items(menu, &root_id, false, cx);
                if chat && !entries.is_empty() {
                    menu = menu.separator();
                }
                for entry in &entries {
                    menu = match entry {
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

/// EXP-1346: what every thread text renders through — the team its pills
/// resolve against and ONE chip resolver ([`thread_text_resolver`]), built
/// once per frame over the view's chip memo where the host keeps one (a long
/// Conversation used to build a resolver per message per frame and re-scan
/// the team's issues per token).
#[derive(Clone, Default)]
pub(crate) struct ThreadText {
    team_id: Option<String>,
    resolver: Option<RefResolver>,
    /// FEED-140: the reader may answer the waiting cards, so the inline
    /// plan / question card carries its actions (besides THE Dock).
    answer: bool,
}

impl ThreadText {
    /// A fresh (un-memoised) resolver over `team_id`; `None` = no pills.
    pub(crate) fn team(team_id: Option<&str>) -> Self {
        Self::memoized(team_id, None)
    }

    /// The resolver over `team_id`, memoised behind `cache` when given (the
    /// CALLER clears it when the issues or statuses notify).
    pub(crate) fn memoized(team_id: Option<&str>, cache: Option<IssueChipCache>) -> Self {
        let resolver = team_id.map(|team_id| {
            let resolver = thread_text_resolver(team_id);
            match cache {
                Some(cache) => resolver.memoized(cache),
                None => resolver,
            }
        });
        Self {
            team_id: team_id.map(str::to_string),
            resolver,
            answer: false,
        }
    }

    pub(crate) fn team_id(&self) -> Option<&str> {
        self.team_id.as_deref()
    }

    /// FEED-140: whether the inline cards answer (the reader may message).
    pub(crate) fn answering(mut self, answer: bool) -> Self {
        self.answer = answer;
        self
    }

    pub(crate) fn answers(&self) -> bool {
        self.answer
    }
}

/// A message body as read-only chat markdown, `#IDENT` / bare `ABC-12` /
/// `@email` pills scoped to `text`'s team ([`ThreadText`]). `images`
/// is the screen's one [`ImageCache`]: a person's `![..](/api/attachments/..)`
/// embed (the composer's pictures) fetches through it — a view without one
/// paints every picture as "Image unavailable" without a fetch.
pub(crate) fn message_markdown(
    id: SharedString,
    body: &str,
    text: &ThreadText,
    images: &Entity<ImageCache>,
) -> MarkdownView {
    let mut view = MarkdownView::new(id, body.to_string())
        .chat(true)
        .selectable(true)
        .autolink(true)
        .images(images.clone())
        .on_open_link(crate::markdown::open_agent_link);
    if let (Some(team), Some(resolver)) = (text.team_id.clone(), text.resolver.clone()) {
        // EXP-1347: a pill opens thread-aware (its thread tab, an open
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
        .tooltip(|window, cx| gpui_component::tooltip::Tooltip::new(steer::feed::QUEUE_REMOVE_LABEL).build(window, cx))
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
        unqueue_button("thread-unqueue", id, queued, cx, move |_, _, cx| unqueue_message(unqueue_id.clone(), cx))
    });
    let caption_row = h_flex()
        .gap_1()
        .items_center()
        .text_2xs()
        .text_color(muted)
        .when(!caption.is_empty(), |row| row.child(SharedString::from(caption)))
        .when(queued, |row| row.child(SharedString::from(format!("· {QUEUED_WORD}"))))
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

/// `plan-card.json` `copy.answering` while the card's answer is on its way.
pub(crate) fn card_answering(cx: &App) -> AnyElement {
    h_flex()
        .gap_1p5()
        .items_center()
        .mt_2()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(gpui_component::spinner::Spinner::new().xsmall())
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
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    let (content, controls) = crate::plan_card::question_parts(&facts, answer, images, window, cx);
    let column = card_column(question, content, controls, cx);
    crate::steer_viewer::ask_card_chrome(false, None, None, None, column, cx)
        .id(SharedString::from(format!("{}-{}", domain::plan_card::test_ids().card, facts.message_id)))
        .into_any_element()
}

/// EXP-1316 → EXP-1280 — a plan card IN THE CONVERSATION = THE plan card
/// ×4: `Plan ready`, the plan (pictures inline; a pending `upload:` embed
/// renders nothing), the inline surface, one block per question; once
/// decided, CLOSED (the picks, `planDecisionLabel`, the feedback). N-12: a
/// pending card the viewer may decide lives in THE Dock ([`ask_row_sheet`]);
/// FEED-140: it stays inline too, with its decision when `answer`. A
/// `superseded` card (the agent replaced it with a newer plan) paints closed
/// and COLLAPSED to `copy.superseded`.
pub(crate) fn plan_card(
    plan: MarkdownView,
    facts: crate::plan_card::PlanFacts,
    answer: bool,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    if facts.decision == Some("superseded") {
        // Web `session-thread-view.tsx`: the muted close glyph + the muted
        // line, the card at 80 %.
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
        return crate::steer_viewer::ask_card_chrome(
            true,
            Some(SharedString::from(domain::ask_dock::ASK_DOCK.copy.plan_ready.clone())),
            None,
            None,
            v_flex().w_full().min_w_0().child(line),
            cx,
        )
        .opacity(0.8)
        .id(SharedString::from(format!("{}-{}", domain::plan_card::test_ids().card, facts.message_id)))
        .into_any_element();
    }
    let (content, controls) = crate::plan_card::plan_parts(&facts, answer, images, window, cx);
    let column = card_column(plan, content, controls, cx);
    crate::steer_viewer::ask_card_chrome(true, Some(SharedString::from(domain::ask_dock::ASK_DOCK.copy.plan_ready.clone())), None, None, column, cx)
        .id(SharedString::from(format!("{}-{}", domain::plan_card::test_ids().card, facts.message_id)))
        .into_any_element()
}

fn plan_facts<'a>(row: &'a SessionThreadRow) -> Option<(&'a str, &'a str, crate::plan_card::PlanFacts<'a>)> {
    match row {
        SessionThreadRow::Plan { id, plan, questions, surface, decision, fresh, answers, feedback, surface_data, .. } => Some((
            id,
            plan,
            crate::plan_card::PlanFacts {
                message_id: id,
                questions,
                surface: surface.as_ref(),
                decision: decision.as_deref(),
                fresh: *fresh,
                answers: answers.as_ref(),
                feedback: feedback.as_deref(),
                surface_data: surface_data.as_ref(),
            },
        )),
        _ => None,
    }
}

fn question_facts<'a>(row: &'a SessionThreadRow) -> Option<(&'a str, &'a str, crate::plan_card::QuestionFacts<'a>)> {
    match row {
        SessionThreadRow::Question { id, question, options, custom, multi, surface, answer, answers, surface_data, answered, .. } => Some((
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

/// A `question` / `plan` row as its Conversation card (the thread's, the
/// agent channel's, the session view's tool-posted one); any other row =
/// `None`.
pub(crate) fn ask_row_card(
    row: &SessionThreadRow,
    text: &ThreadText,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> Option<AnyElement> {
    if let Some((id, plan, facts)) = plan_facts(row) {
        let markdown = message_markdown(
            SharedString::from(format!("thread-plan-{id}")),
            &crate::plan_card::strip_pending_uploads(plan),
            text,
            images,
        );
        return Some(plan_card(markdown, facts, text.answers(), images, window, cx));
    }
    let (id, question, facts) = question_facts(row)?;
    let markdown = message_markdown(SharedString::from(format!("thread-question-{id}")), question, text, images);
    Some(question_card(markdown, facts, text.answers(), images, window, cx))
}

/// N-12: a waiting `question` / `plan` row's sheet (plan-card.json
/// `stepper`): the plan's pages and the footer decision, or the question
/// and its Answer.
pub(crate) fn ask_row_sheet(
    row: &SessionThreadRow,
    text: &ThreadText,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> Option<crate::dock::SheetContent> {
    if let Some((id, plan, facts)) = plan_facts(row) {
        let markdown = message_markdown(
            SharedString::from(format!("thread-plan-sheet-{id}")),
            &crate::plan_card::strip_pending_uploads(plan),
            text,
            images,
        );
        let body = crate::steer_viewer::ask_body_text(cx).child(markdown).into_any_element();
        return Some(crate::plan_card::plan_sheet(&facts, body, images, window, cx));
    }
    let (id, question, facts) = question_facts(row)?;
    let markdown = message_markdown(SharedString::from(format!("thread-question-sheet-{id}")), question, text, images);
    let body = crate::steer_viewer::ask_body_text(cx).child(markdown).into_any_element();
    Some(crate::plan_card::question_sheet(&facts, body, images, window, cx))
}

/// N-12: the suggestions rows with an OPEN item as Dock sources
/// (`suggestionSources` ×4): the row leaves the Conversation and opens as
/// [`suggestions_sheet`].
pub(crate) fn suggestion_sources(rows: &[SessionThreadRow]) -> Vec<DockSource> {
    use domain::session_tree::SuggestionState;
    let rows: Vec<domain::dock::DockSuggestionsRow> = rows
        .iter()
        .filter_map(|row| match row {
            SessionThreadRow::Suggestions { id, header, items, .. } => Some(domain::dock::DockSuggestionsRow {
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

/// N-12: the Conversation rows THE Dock holds (web `threadDockedIds`): the
/// suggestions rows with an open item (S-3: a waiting plan / question is a
/// story, never docked; it stays inline).
pub(crate) fn docked_ids(rows: &[SessionThreadRow]) -> HashSet<String> {
    suggestion_sources(rows)
        .into_iter()
        .filter_map(|source| match source {
            DockSource::Suggestions { id, .. } => Some(id),
            _ => None,
        })
        .collect()
}

/// N-12: THE Dock of a root's Conversation (a thread, an agent channel;
/// `fixtures/dock.json`): the root's own cards (THE Queue, the queued
/// messages, the banners, off the hosted composer view) above THE
/// composer; returns the band and the open card's sheet. `suggestions` =
/// the root's id when its suggested-tasks cards dock too (the thread).
/// S-3: no plan / question source (a story now, inline in the
/// Conversation) and nothing opens itself.
#[allow(clippy::too_many_arguments)] // the host's facts
pub(crate) fn root_dock(
    prefix: &str,
    rows: &[SessionThreadRow],
    suggestions: Option<&str>,
    state: &Entity<crate::dock::DockState>,
    composer: &Entity<crate::steer_viewer::SteerSessionView>,
    text: &ThreadText,
    window: &mut Window,
    cx: &mut App,
) -> (Option<AnyElement>, Option<AnyElement>) {
    let mut sources = Vec::new();
    if suggestions.is_some() {
        sources.extend(suggestion_sources(rows));
    }
    sources.extend(composer.read(cx).session_dock_sources(cx));
    let dock_rows = domain::dock::dock_rows(&sources);
    let sheet = match state.read(cx).sheet_id().map(str::to_string) {
        Some(open) => match dock_rows.iter().find(|row| row.id == open).cloned() {
            Some(dock_row) => {
                // THE sheet is a sheet host: its pickers open inline.
                let content = crate::dock::sheet_host(|| match rows.iter().find(|row| row.id() == open) {
                    Some(SessionThreadRow::Suggestions { id, start_all, items, .. }) => {
                        Some(suggestions_sheet(id, *start_all, items, suggestions.unwrap_or_default(), text.team_id(), cx))
                    }
                    Some(_) => None,
                    None => composer.update(cx, |composer, cx| composer.render_session_dock_sheet(&dock_row, window, cx)),
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
    text: &ThreadText,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> Option<AnyElement> {
    let team_id = text.team_id();
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
        SessionThreadRow::Question { .. } | SessionThreadRow::Plan { .. } => {
            return ask_row_card(row, text, images, window, cx);
        }
        SessionThreadRow::Subagent { title, status, .. } => muted_line(
            registry::CODING_SUBAGENT,
            [title.as_str(), status.as_str()]
                .into_iter()
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
            cx,
        ),
        SessionThreadRow::Picture { id, attachment_id, width, height, caption } => {
            picture_row(id, attachment_id, *width, *height, caption.as_deref(), images, cx)
        }
        SessionThreadRow::Event { text, .. } => muted_line(registry::EVENT_STATUS_CHANGED, text.clone(), cx),
        // S-3: a child's end report = THE link header alone (`Report from
        // <chip> ›`); its summary never repeats.
        SessionThreadRow::Report { id, xref, .. } => crate::session_xref::session_xref(id, xref, cx),
        SessionThreadRow::Suggestions { id, header, start_all, items, .. } => {
            suggestions_card(id, header, *start_all, items, root_id, team_id, cx)
        }
        // Thread 58cb32df (chats.json): `/new` / `/compact` = the divider
        // pill; an agent's preview = THE surface card.
        SessionThreadRow::Divider { id, kind, at } => crate::surface_card::divider_row(id, *kind, at, cx),
        SessionThreadRow::Surface { id, session_id, title, surface } => {
            crate::surface_card::surface_row(id, session_id, title.as_deref(), surface, window, cx)
        }
    })
}

// ---------------------------------------------------------------------------
// EXP-1323: THE suggested tasks card (a report = `session_xref`)
// ---------------------------------------------------------------------------

/// S-3: a message posted by ANOTHER session = its link header right above
/// the bubble (a person's bubble right-aligned: the header follows it).
pub(crate) fn with_xref(
    id: &str,
    xref: Option<&domain::session_tree::SessionXref>,
    author: &str,
    bubble: AnyElement,
    cx: &App,
) -> AnyElement {
    let Some(xref) = xref else {
        return bubble;
    };
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .when(author == domain::contract::MESSAGE_AUTHOR_KIND_USER, |this| this.items_end())
        .child(crate::session_xref::session_xref(id, xref, cx))
        .child(bubble)
        .into_any_element()
}

/// S-3: the view's origin (`Started by <chip> ›` / `Reported to <chip> ›`)
/// at the top of the Conversation and of a session open in place.
fn origin_line(view: &SessionThreadView, cx: &App) -> Option<AnyElement> {
    let origin = view.origin.as_ref()?;
    let key = format!("origin-{}", view.focus_id.as_deref().unwrap_or(&view.root_id));
    Some(
        div()
            .w_full()
            .child(
                div()
                    .w_full()
                    .max_w(px(CONVERSATION_W))
                    .mx_auto()
                    .px(px(crate::work_header::WORK_GUTTER))
                    .pt_3()
                    .child(crate::session_xref::session_xref(&key, origin, cx)),
            )
            .into_any_element(),
    )
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
/// `ThreadSuggestionsSheet`): the items exactly as [`suggestions_card`]
/// lists them, hairline-divided; the footer = Start all while more than
/// one item is open.
pub(crate) fn suggestions_sheet(
    message_id: &str,
    start_all: bool,
    items: &[domain::session_tree::SuggestionRow],
    root_id: &str,
    team_id: Option<&str>,
    cx: &App,
) -> crate::dock::SheetContent {
    use gpui_component::button::Button;
    let rows = items.iter().enumerate().map(|(index, item)| {
        div()
            .w_full()
            .min_w_0()
            .when(index > 0, |row| row.border_t_1().border_color(theme::tokens::glass::STROKE_ROW.to_hsla()))
            .child(suggestion_item_row(message_id, item, root_id, team_id, cx))
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
                .id("thread-suggestions-sheet")
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
                Button::new("thread-suggestions-start-all")
                    .primary()
                    .web_sm()
                    .label(THREAD.copy.suggestions.start_all.clone())
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

/// An open item's ⋯ = THE Menu (web `session-thread-view`): Do it here ·
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
    let copy = &THREAD.copy.suggestions;
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
// The issue page's `Threads` row
// ---------------------------------------------------------------------------

/// A root's mark (its CLI's), `threadRowMarkState` ×4 (EXP-1344): the brand
/// mark with an amber corner dot while it needs you (beats busy), else the
/// working mark while busy, else the bare brand mark.
pub(crate) fn root_mark(root: &SessionRow, busy: bool, needs_you: bool, size: f32, badge: f32) -> AnyElement {
    let agent = root.cli.as_deref().and_then(coding::CodingAgent::parse);
    if domain::session_tree::thread_row_mark_state(needs_you, busy) == domain::session_tree::SessionState::Working {
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
    let (busy, needs_you) =
        live.fold((false, false), |(busy, needs), session| (busy || session.busy, needs || session.needs_input));
    let title = crate::session_data::session_title_of(&root.id, cx).unwrap_or_else(|| THREAD.copy.untitled.clone());
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
// EXP-1334: the strip (never scrolls) and the chips' subtree menus
// ---------------------------------------------------------------------------

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

/// What the strip shows open (a strip pick).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StripPick {
    Conversation,
    Issue(String),
    Session(String),
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

/// THE thread strip's inputs (the screen and the styleguide draw it):
/// `[+] [Conversation] | issue chips … +X  session chips … +Y`, nothing
/// scrolls — `+` and Conversation pinned left (S-3: no Guide tab), the
/// middle laid out by `domain::session_tree::partition_thread_strip`.
pub(crate) struct ThreadStrip {
    /// Element id prefix (`thread` on the screen).
    pub(crate) prefix: &'static str,
    pub(crate) tabs: Vec<ThreadTab>,
    pub(crate) issues: HashMap<String, StripIssue>,
    /// Each session's agent CLI (its run mark).
    pub(crate) clis: HashMap<String, Option<String>>,
    pub(crate) open: StripPick,
    /// The measured middle width (`None` = the first frame: every chip
    /// drawn, clipped).
    pub(crate) available: Option<f32>,
    /// THE thread menu's target for the `+` and the issue chips' context
    /// menus (`None` = a non-member: no `+`, no menus).
    pub(crate) menu: Option<ThreadMenuTarget>,
    pub(crate) on_pick: OnStripPick,
    /// Records the middle slot's painted width.
    pub(crate) on_slot: Option<std::rc::Rc<dyn Fn(f32, &mut App)>>,
}

/// A chip's frame: `px(10)` a side + the 1px border.
const STRIP_CHIP_CHROME: f32 = 22.;
/// The gap between a chip's own children (`strip_chip`'s `gap(6)`).
const STRIP_CHIP_INNER_GAP: f32 = 6.;
/// A session chip name's ONE max width.
const SESSION_CHIP_NAME_MAX_W: f32 = 180.;
/// Each half of `<chip> › <open one>` (a session chip with a nested one open).
const SESSION_CHIP_PART_MAX_W: f32 = 110.;

/// The open nested item of a chip (`<chip> › <open one>`).
fn open_nested(tab: &ThreadTab, open: &StripPick) -> Option<String> {
    match (tab, open) {
        (ThreadTab::Issue { nested, .. }, StripPick::Issue(id)) => {
            nested.iter().find(|item| item.issue_id == *id).map(|item| item.identifier.clone())
        }
        (ThreadTab::Session { nested, .. }, StripPick::Session(id)) => {
            nested.iter().find(|item| item.session_id == *id).map(|item| item.name.clone())
        }
        _ => None,
    }
}

/// Whether `tab`'s chip is active: it or one of its nested items is open.
fn chip_active(tab: &ThreadTab, open: &StripPick) -> bool {
    match (tab, open) {
        (ThreadTab::Issue { issue_id, nested, .. }, StripPick::Issue(id)) => {
            issue_id == id || nested.iter().any(|item| item.issue_id == *id)
        }
        (ThreadTab::Session { session_id, nested, .. }, StripPick::Session(id)) => {
            session_id == id || nested.iter().any(|item| item.session_id == *id)
        }
        _ => false,
    }
}

/// The issue chip's `› EXP-22` while a nested issue is open.
fn chip_suffix(text: String, cx: &App) -> AnyElement {
    h_flex()
        .flex_shrink_0()
        .gap(px(STRIP_CHIP_INNER_GAP))
        .items_center()
        .child(div().text_color(cx.theme().muted_foreground).child("›"))
        .child(SharedString::from(text))
        .into_any_element()
}

fn chevron(cx: &App) -> AnyElement {
    Icon::new(registry::UI_CHEVRON_DOWN)
        .xsmall()
        .flex_shrink_0()
        .text_color(cx.theme().muted_foreground)
        .into_any_element()
}

/// One item of a chip's subtree menu (row 1 = the chip itself).
#[derive(Clone)]
enum SubtreeItem {
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

impl SubtreeItem {
    fn pick(&self) -> StripPick {
        match self {
            Self::Issue { issue_id, .. } => StripPick::Issue(issue_id.clone()),
            Self::Session { session_id, .. } => StripPick::Session(session_id.clone()),
        }
    }
}

/// A chip and its nested rows, depths relative to the chip (0).
fn subtree_of(tab: &ThreadTab, spec: &ThreadStrip) -> Vec<(SubtreeItem, usize)> {
    let issue = |issue_id: &str, identifier: &str| {
        let info = spec.issues.get(issue_id).cloned().unwrap_or_default();
        SubtreeItem::Issue {
            issue_id: issue_id.to_string(),
            identifier: identifier.to_string(),
            title: info.title,
            status: info.status,
        }
    };
    let session = |session_id: &str, name: &str, mark| SubtreeItem::Session {
        session_id: session_id.to_string(),
        name: name.to_string(),
        state: chip_state(mark),
        cli: spec.clis.get(session_id).cloned().flatten(),
    };
    match tab {
        ThreadTab::Issue {
            issue_id,
            identifier,
            nested,
        } => std::iter::once((issue(issue_id, identifier), 0))
            .chain(nested.iter().map(|item| (issue(&item.issue_id, &item.identifier), item.depth)))
            .collect(),
        ThreadTab::Session { session_id, name, mark_state, nested } => {
            std::iter::once((session(session_id, name, *mark_state), 0))
                .chain(nested.iter().map(|item| (session(&item.session_id, &item.name, item.mark_state), item.depth)))
                .collect()
        }
        _ => Vec::new(),
    }
}

/// One subtree menu row: THE tree row (guides, the lead, the name, a
/// session's state word in its tone), the open one checked.
fn subtree_row(item: &SubtreeItem, guides: &domain::tree_guides::Guides, active: bool, cx: &App) -> gpui::Div {
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
        SubtreeItem::Issue { identifier, title, status, .. } => row
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
        SubtreeItem::Session { name, state, cli, .. } => {
            let word_color = match state {
                domain::session_tree::SessionState::NeedsYou | domain::session_tree::SessionState::Blocked => {
                    crate::work_card::session_tone_color(state.tone(), cx)
                }
                _ => theme.muted_foreground,
            };
            row.child(crate::run_rows::run_mark(crate::run_rows::run_agent(cli.as_deref()), *state, cx))
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
    let check =
        active.then(|| Icon::new(registry::UI_CHECK).size(px(14.)).text_color(theme.foreground).into_any_element());
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

/// THE Menu of chip subtrees: each group = a chip then its nested rows
/// (one guide set per group); a pick opens it.
fn subtree_menu(
    mut menu: gpui_component::menu::PopupMenu,
    groups: &[Vec<(SubtreeItem, usize)>],
    open: &StripPick,
    on_pick: &OnStripPick,
) -> gpui_component::menu::PopupMenu {
    menu = menu.min_w(px(300.)).scrollable(true).max_h(px(420.));
    for (index, group) in groups.iter().enumerate() {
        if index > 0 && groups.len() > 1 && group.len() > 1 {
            menu = menu.separator();
        }
        let depths: Vec<usize> = group.iter().map(|(_, depth)| *depth).collect();
        for ((item, _), guides) in group.iter().zip(domain::tree_guides::guides_for(&depths)) {
            let pick = item.pick();
            let active = *open == pick;
            let item = item.clone();
            let on_pick = on_pick.clone();
            menu = menu.item(
                gpui_component::menu::PopupMenuItem::element(move |_, cx| subtree_row(&item, &guides, active, cx))
                    .on_click(move |_, window, cx| on_pick(pick.clone(), window, cx)),
            );
        }
    }
    menu
}

/// The measured widths of one chip (`short` = an issue's identifier-only
/// width; a session chip has one).
fn measure_strip_chip(tab: &ThreadTab, spec: &ThreadStrip, window: &Window) -> (f32, f32) {
    let rem = f32::from(window.rem_size());
    let font = window.text_style().font();
    let mut mono = font.clone();
    mono.family = theme::terminal::FONT_FAMILY.into();
    let sm = |text: &str| crate::screens::measure_text(window, text, font.clone(), gpui::rems(0.875));
    let xs_mono = |text: &str| crate::screens::measure_text(window, text, mono.clone(), gpui::rems(0.75));
    let glyph = 0.75 * rem;
    let g = STRIP_CHIP_INNER_GAP;
    let suffix = open_nested(tab, &spec.open);
    let has_nested = match tab {
        ThreadTab::Issue { nested, .. } => !nested.is_empty(),
        ThreadTab::Session { nested, .. } => !nested.is_empty(),
        _ => false,
    };
    let tail = if has_nested { g + glyph } else { 0. };
    match tab {
        ThreadTab::Issue { issue_id, identifier, .. } => {
            let mut short = STRIP_CHIP_CHROME + glyph + g + xs_mono(identifier);
            if let Some(open) = suffix.as_deref() {
                short += g + sm("›") + g + xs_mono(open);
            }
            let open_id = match &spec.open {
                StripPick::Issue(id) if suffix.is_some() => id.as_str(),
                _ => issue_id.as_str(),
            };
            let title = spec.issues.get(open_id).map(|issue| issue.title.as_str()).unwrap_or_default();
            let full = if title.trim().is_empty() {
                short
            } else {
                short + g + sm(title).min(ISSUE_CHIP_TITLE_MAX_W)
            };
            (full + tail, short + tail)
        }
        ThreadTab::Session { name, .. } => {
            let cap = if suffix.is_some() { SESSION_CHIP_PART_MAX_W } else { SESSION_CHIP_NAME_MAX_W };
            let mut width = STRIP_CHIP_CHROME + domain::list_item::MARK + g + sm(name).min(cap);
            if let Some(open) = suffix.as_deref() {
                width += g + sm("›") + g + sm(open).min(cap);
            }
            (width + tail, width + tail)
        }
        _ => (0., 0.),
    }
}

/// The least a session chip may shrink to (its label ellipsized): the mark,
/// a few letters of each label half, the chevron.
fn session_chip_floor(tab: &ThreadTab, spec: &ThreadStrip, window: &Window) -> f32 {
    const MIN_TEXT: f32 = 36.;
    let g = STRIP_CHIP_INNER_GAP;
    let mut floor = STRIP_CHIP_CHROME + domain::list_item::MARK + g + MIN_TEXT;
    if open_nested(tab, &spec.open).is_some() {
        let font = window.text_style().font();
        floor += g + crate::screens::measure_text(window, "›", font, gpui::rems(0.875)) + g + MIN_TEXT;
    }
    if let ThreadTab::Session { nested, .. } = tab {
        if !nested.is_empty() {
            floor += g + 0.75 * f32::from(window.rem_size());
        }
    }
    floor
}

/// EXP-1334: what the drawn chips still overflow by goes off the session
/// chips (the active one first, then the widest), each down to its floor —
/// identifiers and "+N" never truncate, a label ellipsizes inside its chip.
/// Pure: the caps (`None` = natural width) per drawn session chip.
pub(crate) fn shrink_session_chips(
    drawn: f32,
    available: f32,
    widths: &[f32],
    floors: &[f32],
    active: Option<usize>,
) -> Vec<Option<f32>> {
    let mut caps: Vec<Option<f32>> = vec![None; widths.len()];
    let mut excess = drawn - available;
    if excess <= 0. {
        return caps;
    }
    let mut order: Vec<usize> = (0..widths.len()).collect();
    order.sort_by(|a, b| (Some(*b) == active).cmp(&(Some(*a) == active)).then(widths[*b].total_cmp(&widths[*a])));
    for index in order {
        if excess <= 0. {
            break;
        }
        let room = (widths[index] - floors[index]).max(0.);
        let cut = room.min(excess);
        if cut > 0. {
            caps[index] = Some(widths[index] - cut);
            excess -= cut;
        }
    }
    caps
}

/// Paint THE thread strip.
pub(crate) fn thread_strip(spec: ThreadStrip, window: &Window, cx: &App) -> AnyElement {
    let prefix = spec.prefix;
    let issue_tabs: Vec<&ThreadTab> = spec.tabs.iter().filter(|tab| matches!(tab, ThreadTab::Issue { .. })).collect();
    let session_tabs: Vec<&ThreadTab> =
        spec.tabs.iter().filter(|tab| matches!(tab, ThreadTab::Session { .. })).collect();

    // The layout: measured widths through the contract's partition.
    let mut session_caps: HashMap<usize, f32> = HashMap::new();
    let layout = match spec.available {
        Some(available) => {
            let issue_widths: Vec<domain::session_tree::StripIssueWidth> = issue_tabs
                .iter()
                .map(|tab| {
                    let (full, short) = measure_strip_chip(tab, &spec, window);
                    domain::session_tree::StripIssueWidth { full, short }
                })
                .collect();
            let session_widths: Vec<f32> =
                session_tabs.iter().map(|tab| measure_strip_chip(tab, &spec, window).0).collect();
            let most = issue_tabs.len().max(session_tabs.len()).max(1);
            // The "+N" is the strip chip box at rest with narrower sides
            // (`surface::strip_overflow_button`); reserve what it paints.
            let overflow_w = crate::surface::STRIP_OVERFLOW_CHROME
                + crate::screens::measure_text(window, &format!("+{most}"), window.text_style().font(), gpui::rems(0.875));
            let gap = crate::screens::chip_gap(window);
            let active_session = session_tabs.iter().position(|tab| chip_active(tab, &spec.open));
            let mut layout = domain::session_tree::partition_thread_strip(
                &issue_widths,
                &session_widths,
                available,
                gap,
                overflow_w,
                issue_tabs.iter().position(|tab| chip_active(tab, &spec.open)),
                active_session,
            );
            // What the drawn chips take; any overflow comes off the session
            // labels (never a clip), and when even their floors do not fit,
            // the issue zone folds whole into its "+N".
            let full_tier = layout.tier == domain::session_tree::StripTier::Full;
            let drawn_of = |issues: &[usize], sessions: &[usize]| {
                let mut parts: Vec<f32> = issues
                    .iter()
                    .map(|&index| if full_tier { issue_widths[index].full } else { issue_widths[index].short })
                    .collect();
                if issues.len() < issue_tabs.len() {
                    parts.push(overflow_w);
                }
                parts.extend(sessions.iter().map(|&index| session_widths[index]));
                if sessions.len() < session_tabs.len() {
                    parts.push(overflow_w);
                }
                parts.iter().sum::<f32>() + gap * parts.len().saturating_sub(1) as f32
            };
            let session_metrics = |sessions: &[usize]| -> (Vec<f32>, Vec<f32>) {
                (
                    sessions.iter().map(|&index| session_widths[index]).collect(),
                    sessions
                        .iter()
                        .map(|&index| session_chip_floor(session_tabs[index], &spec, window))
                        .collect(),
                )
            };
            let (mut widths, mut floors) = session_metrics(&layout.sessions);
            let floor_drawn =
                drawn_of(&layout.issues, &layout.sessions) - widths.iter().sum::<f32>() + floors.iter().sum::<f32>();
            if floor_drawn > available && !layout.issues.is_empty() {
                layout.issues.clear();
                // The sessions get the room the folded issues freed (less
                // the issue zone's "+N" and its gap): the chips that folded
                // to make room for the issues unfold again.
                let room = available - overflow_w - gap;
                layout.sessions = domain::session_tree::partition_thread_strip(
                    &[],
                    &session_widths,
                    room,
                    gap,
                    overflow_w,
                    None,
                    active_session,
                )
                .sessions;
                (widths, floors) = session_metrics(&layout.sessions);
            }
            let drawn = drawn_of(&layout.issues, &layout.sessions);
            let active = active_session.and_then(|active| layout.sessions.iter().position(|&index| index == active));
            for (slot, cap) in shrink_session_chips(drawn, available, &widths, &floors, active)
                .into_iter()
                .enumerate()
            {
                if let Some(cap) = cap {
                    session_caps.insert(layout.sessions[slot], cap);
                }
            }
            layout
        }
        None => domain::session_tree::ThreadStripLayout {
            tier: domain::session_tree::StripTier::Full,
            issues: (0..issue_tabs.len()).collect(),
            sessions: (0..session_tabs.len()).collect(),
        },
    };
    let full = layout.tier == domain::session_tree::StripTier::Full;

    let pick = |pick: StripPick| {
        let on_pick = spec.on_pick.clone();
        move |_: &ClickEvent, window: &mut Window, cx: &mut App| on_pick(pick.clone(), window, cx)
    };

    // One chip (with its subtree menu when it nests).
    let chip_element = |tab: &ThreadTab, cap_w: Option<f32>, cx: &App| -> AnyElement {
        let active = chip_active(tab, &spec.open);
        let suffix = open_nested(tab, &spec.open);
        let groups = vec![subtree_of(tab, &spec)];
        let nests = groups[0].len() > 1;
        let (element, id): (AnyElement, SharedString) = match tab {
            ThreadTab::Issue { issue_id, identifier, .. } => {
                let id = SharedString::from(format!("{prefix}-child-{issue_id}"));
                let info = spec.issues.get(issue_id).cloned().unwrap_or_default();
                let open_id = match &spec.open {
                    StripPick::Issue(id) if suffix.is_some() => id.clone(),
                    _ => issue_id.clone(),
                };
                let open_info = spec.issues.get(&open_id).cloned().unwrap_or_default();
                let mut body = strip_issue_body(open_info.status.clone().or(info.status.clone()), identifier, "", active, cx);
                body.truncate(2);
                let mut chip = strip_chip(id.clone(), active, cx).children(body);
                if let Some(open) = suffix.clone() {
                    chip = chip.child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .font_family(theme::terminal::FONT_FAMILY)
                            .child(chip_suffix(open, cx)),
                    );
                }
                if full && !open_info.title.trim().is_empty() {
                    chip = chip.child(
                        div()
                            .flex_shrink(1.)
                            .min_w_0()
                            .max_w(px(ISSUE_CHIP_TITLE_MAX_W))
                            .truncate()
                            .text_color(if active { cx.theme().foreground } else { cx.theme().muted_foreground })
                            .child(SharedString::from(open_info.title.clone())),
                    );
                }
                if nests {
                    chip = chip.child(chevron(cx));
                } else {
                    chip = chip.on_click(pick(StripPick::Issue(issue_id.clone())));
                }
                if info.ringed.is_some() {
                    chip = chip.border_color(cx.theme().ring);
                }
                let chip: AnyElement = match spec.menu.clone() {
                    Some(target) => {
                        let target = ThreadMenuTarget {
                            chip: Some((issue_id.clone(), identifier.clone())),
                            ..target
                        };
                        chip.pointer_context_menu(move |menu, window, cx| thread_menu(menu, &target, window, cx))
                            .into_any_element()
                    }
                    None => chip.into_any_element(),
                };
                let element = match info.ringed {
                    Some(link_id) => div()
                        .flex()
                        .flex_shrink_0()
                        .child(chip)
                        .with_animation(
                            SharedString::from(format!("{prefix}-child-enter-{link_id}")),
                            Animation::new(theme::motion::ENTER).with_easing(theme::motion::decelerate()),
                            |el, delta| el.opacity(delta),
                        )
                        .into_any_element(),
                    None => chip,
                };
                (element, id)
            }
            ThreadTab::Session {
                session_id,
                name,
                mark_state,
                ..
            } => {
                let id = SharedString::from(format!("{prefix}-session-{session_id}"));
                let cli = spec.clis.get(session_id).cloned().flatten();
                let cap = if suffix.is_some() { SESSION_CHIP_PART_MAX_W } else { SESSION_CHIP_NAME_MAX_W };
                let mut chip = strip_chip(id.clone(), active, cx)
                    .when_some(cap_w, |chip, cap_w| chip.max_w(px(cap_w)))
                    .child(
                        div().flex_shrink_0().child(crate::run_rows::run_mark(
                            crate::run_rows::run_agent(cli.as_deref()),
                            chip_state(*mark_state),
                            cx,
                        )),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_shrink(1.)
                            .max_w(px(cap))
                            .truncate()
                            .child(SharedString::from(name.clone())),
                    );
                if let Some(open) = suffix {
                    chip = chip.child(
                        h_flex()
                            .min_w_0()
                            .flex_shrink(1.)
                            .gap(px(STRIP_CHIP_INNER_GAP))
                            .items_center()
                            .child(div().flex_shrink_0().text_color(cx.theme().muted_foreground).child("›"))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_shrink(1.)
                                    .max_w(px(cap))
                                    .truncate()
                                    .child(SharedString::from(open)),
                            ),
                    );
                }
                if nests {
                    chip = chip.child(chevron(cx));
                } else {
                    chip = chip.on_click(pick(StripPick::Session(session_id.clone())));
                }
                (chip.into_any_element(), id)
            }
            _ => return div().into_any_element(),
        };
        if !nests {
            return element;
        }
        let open = spec.open.clone();
        let on_pick = spec.on_pick.clone();
        crate::controls::PointerMenu::new(
            SharedString::from(format!("{id}-menu")),
            crate::picker::PickerTrigger::new(SharedString::from(format!("{id}-trigger")).into(), element),
            move |menu, _window, _cx| subtree_menu(menu, &groups, &open, &on_pick),
        )
        .into_any_element()
    };

    // A zone's "+N": THE Menu of the hidden chips (and their subtrees).
    let overflow = |zone: &str, tabs: &[&ThreadTab], shown: &[usize], cx: &App| -> Option<AnyElement> {
        let hidden: Vec<Vec<(SubtreeItem, usize)>> = tabs
            .iter()
            .enumerate()
            .filter(|(index, _)| !shown.contains(index))
            .map(|(_, tab)| subtree_of(tab, &spec))
            .collect();
        if hidden.is_empty() {
            return None;
        }
        // Web `StripOverflowButton`: the chip box at rest, the count alone
        // (an active chip folded into it does not light the button).
        let id = SharedString::from(format!("{prefix}-{zone}-more"));
        let trigger = crate::surface::strip_overflow_button(id.clone(), hidden.len(), cx).into_any_element();
        let open = spec.open.clone();
        let on_pick = spec.on_pick.clone();
        Some(
            crate::controls::PointerMenu::new(
                SharedString::from(format!("{id}-menu")),
                crate::picker::PickerTrigger::new(SharedString::from(format!("{id}-trigger")).into(), trigger),
                move |menu, _window, _cx| subtree_menu(menu, &hidden, &open, &on_pick),
            )
            .into_any_element(),
        )
    };

    let mut lead: Vec<AnyElement> = Vec::new();
    if let Some(target) = spec.menu.clone() {
        let trigger =
            crate::controls::ghost_icon_button(SharedString::from(format!("{prefix}-child-add")), Icon::new(registry::UI_ADD), cx);
        lead.push(
            div()
                .flex_shrink_0()
                .child(crate::controls::PointerMenu::new(
                    SharedString::from(format!("{prefix}-child-add-menu")),
                    trigger,
                    move |menu, window, cx| thread_menu(menu, &target, window, cx),
                ))
                .into_any_element(),
        );
    }
    // NEW-6 (N-1): a thread with ONE session has no Conversation tab (its
    // raw work IS the body); the tab comes back with the first child.
    if spec.tabs.iter().any(|tab| matches!(tab, ThreadTab::Conversation)) {
        lead.push(
            strip_tab(SharedString::from(format!("{prefix}-child-conversation")), spec.open == StripPick::Conversation, cx)
                .child(Icon::new(registry::ACTION_CHAT).xsmall())
                .child(SharedString::from(THREAD.copy.conversation.clone()))
                .on_click(pick(StripPick::Conversation))
                .into_any_element(),
        );
    }
    let middle_index = lead.len();

    let mut middle: Vec<AnyElement> =
        layout.issues.iter().map(|&index| chip_element(issue_tabs[index], None, cx)).collect();
    middle.extend(overflow("issues", &issue_tabs, &layout.issues, cx));
    middle.extend(
        layout
            .sessions
            .iter()
            .map(|&index| chip_element(session_tabs[index], session_caps.get(&index).copied(), cx)),
    );
    middle.extend(overflow("sessions", &session_tabs, &layout.sessions, cx));

    let on_slot = spec.on_slot.clone();
    h_flex()
        .when_some(on_slot, |row, on_slot| {
            row.on_children_prepainted(move |bounds: Vec<gpui::Bounds<gpui::Pixels>>, _window, cx| {
                if let Some(slot) = bounds.get(middle_index) {
                    on_slot(f32::from(slot.size.width), cx);
                }
            })
        })
        .id(SharedString::from(THREAD.test_ids.tabs.clone()))
        .w_full()
        .min_w_0()
        .gap_1()
        .items_center()
        .children(lead)
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
        .into_any_element()
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
                ThreadTab::Issue { issue_id, nested, .. } => Some(
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
        crate::threads_view::root_pull_request(&self.root.id, &self.pulls)
    }
}

fn thread_model(root_id: &str, focus: Option<&str>, cx: &App) -> Option<ThreadModel> {
    // #EXP-1380: the view and the root's messages off ONE scan.
    let (view, own) =
        crate::session_data::thread_view_and_messages(root_id, focus, crate::session_data::now_ms(), false, cx)?;
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
    Some(ThreadModel {
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
const CONVERSATION_OVERDRAW: gpui::Pixels = px(1024.);

/// EXP-1346: the Conversation list's projection ([`ThreadScreenView::sync_list`]).
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
}

/// EXP-1346: a Conversation row's content identity (its measured height
/// moves with it): the row itself plus a message's timestamp (its caption)
/// and, for a row that may render an issue pill (a `#` in its text), the
/// chip generation — a pill's title comes from the issue, not the row, and
/// a retitled pill wraps differently.
pub(crate) fn conversation_fingerprint(row: &SessionThreadRow, at: Option<&str>, chip_gen: u64) -> u64 {
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

pub(crate) struct ThreadScreenView {
    root_id: String,
    child: ThreadChildSel,
    /// EXP-1346: shared, never deep-cloned per frame.
    model: Option<Rc<ThreadModel>>,
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
    text: ThreadText,
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
    /// NEW-6 (N-1): the thread's ONE session (`view.main_session_id`) while
    /// it is the main thing: its raw work is the body, in place.
    main: Option<(String, Entity<crate::session_screen::SessionScreenView>)>,
    _main_sub: Option<Subscription>,
    /// EXP-1334: the strip's middle slot, measured (the partition budget).
    strip_slot: Option<f32>,
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
            subscriptions.push(cx.observe(&collections.sessions, |this, _, cx| this.queue_refresh(cx)));
            subscriptions.push(cx.observe(&collections.session_issues, |this, _, cx| this.queue_refresh(cx)));
            subscriptions.push(cx.observe(&collections.messages, |this, _, cx| this.queue_refresh(cx)));
            // NEW-8: a message's session is its conversation's.
            subscriptions.push(cx.observe(&collections.conversations, |this, _, cx| this.queue_refresh(cx)));
            subscriptions.push(cx.observe(&collections.pull_requests, |this, _, cx| this.queue_refresh(cx)));
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
            text: ThreadText::default(),
            focus_handle: cx.focus_handle(),
            _ring_tick: None,
            focus: None,
            _focus_sub: None,
            tail: None,
            _tail_sub: None,
            main: None,
            _main_sub: None,
            strip_slot: None,
            _missing_tick: missing_tick,
            _subscriptions: subscriptions,
        };
        this.refresh(cx);
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
        // Thread 58cb32df: a surface card arriving while the chat is open
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
        self.refresh(cx);
    }

    /// The focused session's id.
    pub(crate) fn focus_id(&self) -> Option<&str> {
        self.focus.as_ref().map(|(id, _)| id.as_str())
    }

    /// EXP-1347: the issue ids of this thread's issue tabs (nested ones too).
    pub(crate) fn issue_tab_ids(&self) -> Vec<String> {
        self.model.as_ref().map(|model| model.linked_ids()).unwrap_or_default()
    }

    /// EXP-1347: an issue pill of one of this thread's issues selects its tab.
    pub(crate) fn open_issue_tab(&mut self, issue_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.select_child(ThreadChildSel::Issue(issue_id), window, cx);
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

    // ── render ───────────────────────────────────────────────────────────

    fn menu_target(&self, chip: Option<(String, String)>) -> ThreadMenuTarget {
        ThreadMenuTarget {
            root_id: self.root_id.clone(),
            team_id: self.model.as_ref().and_then(|model| model.root.team_id.clone()),
            chip,
            linked: self.model.as_ref().map(|model| model.linked_ids()).unwrap_or_default(),
        }
    }

    fn render_header(&mut self, model: &ThreadModel, window: &Window, cx: &mut gpui::Context<Self>) -> AnyElement {
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
        // NEW-6 (N-1): the thread's ONE session's Stop while it is the body.
        // Thread 58cb32df (chats.json STOP): the pill = the menu's Stop,
        // `sessions.end` on the ROOT (the whole tree), shown while the root
        // is not ended and the viewer may steer the chat.
        let root_live = crate::chat_actions::chat_entries(&model.root.id, cx)
            .contains(&domain::chats::ThreadMenuEntry::Stop);
        let stop = root_live.then(|| {
            let root_id = model.root.id.clone();
            crate::session_screen::stop_session_pill(
                "thread-session-stop",
                crate::surface::PillSize::Md,
                cx,
            )
            .tooltip(SharedString::from(domain::chats::CHATS.copy.menu.stop.clone()))
            .on_click(move |_, _, cx| crate::chat_actions::stop_chat(&root_id, cx))
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
            .child(self.render_strip(model, window, cx))
            .child(div().w_full().h(px(1.)).bg(cx.theme().border))
            .into_any_element()
    }

    /// EXP-1334: THE thread strip ([`thread_strip`]) over the view's tabs.
    fn render_strip(&mut self, model: &ThreadModel, window: &Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let member = thread_member(model, cx);
        let open = match (self.focus_id(), &self.child) {
            (Some(focus), _) => StripPick::Session(focus.to_string()),
            (None, ThreadChildSel::Conversation) => StripPick::Conversation,
            (None, ThreadChildSel::Issue(id)) => StripPick::Issue(id.clone()),
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
        let this = cx.entity().downgrade();
        let on_pick: OnStripPick = std::rc::Rc::new(move |pick, window, cx| match pick {
            StripPick::Session(session_id) => open_session_in_place(session_id, window, cx),
            other => {
                let child = match other {
                    StripPick::Issue(id) => ThreadChildSel::Issue(id),
                    _ => ThreadChildSel::Conversation,
                };
                let _ = this.update(cx, |this, cx| this.select_child(child, window, cx));
            }
        });
        let slot = cx.entity().downgrade();
        thread_strip(
            ThreadStrip {
                prefix: "thread",
                tabs: model.view.tabs.clone(),
                issues,
                clis,
                open,
                available: self.strip_slot,
                menu: member.then(|| self.menu_target(None)),
                on_pick,
                on_slot: Some(std::rc::Rc::new(move |width, cx| {
                    let _ = slot.update(cx, |this, cx| {
                        if this.strip_slot.is_none_or(|prev| (prev - width).abs() > 0.5) {
                            this.strip_slot = Some(width);
                            cx.notify();
                        }
                    });
                })),
            },
            window,
            cx,
        )
    }

    fn render_row(
        &self,
        row: &SessionThreadRow,
        model: &ThreadModel,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if let SessionThreadRow::Message {
            id,
            author,
            author_id,
            body,
            queued,
            xref,
            ..
        } = row
        {
            let markdown = message_markdown(
                SharedString::from(format!("thread-message-{id}")),
                body,
                &self.text,
                &self.images,
            );
            let bubble = match author.as_str() {
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
            return with_xref(id, xref.as_ref(), author, bubble, cx);
        }
        card_row(row, &self.root_id, &self.text, &self.images, window, cx).unwrap_or_else(|| div().into_any_element())
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
    fn sync_list(&mut self, model: &ThreadModel, may_message: bool) {
        let inputs = (self.model_gen, self.chip_gen, may_message, self.tail.is_some());
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
            // #EXP-1380 FOLLOW: an arrival keeps a reader inside the slack
            // on the tail.
            crate::chat_transcript::rearm_near_tail(&self.list);
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
        let mut keys: Vec<RowKey> = self
            .conv
            .rows
            .iter()
            .enumerate()
            .map(|(offset, &index)| {
                let row = &model.view.rows[index];
                let at = match row {
                    SessionThreadRow::Message { id, .. } => {
                        model.messages.get(id).and_then(|message| message.created_at.as_deref())
                    }
                    _ => None,
                };
                let mut fingerprint = conversation_fingerprint(row, at, chip_gen);
                // The first row carries `Load earlier` while rows hide above.
                if offset == 0 && start > 0 {
                    fingerprint = fingerprint.rotate_left(1) ^ 1;
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
        for op in plan_list_sync(&self.conv.keys, &keys) {
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
        let content = match (self.conv.rows.get(ix), self.model.clone()) {
            (Some(&index), Some(model)) => match model.view.rows.get(index) {
                Some(row) => Some(self.render_row(row, &model, window, cx)),
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
                    .when(ix > 0, |this| this.pt_5())
                    .children(earlier)
                    .child(content),
            )
            .into_any_element()
    }

    /// The Conversation's jump-to-bottom button (#EXP-1380: past
    /// `transcript.followSlack`, [`crate::chat_transcript::jump_overlay`]).
    fn render_jump_to_bottom(&self, band: f32, cx: &mut gpui::Context<Self>) -> AnyElement {
        crate::chat_transcript::jump_overlay(
            "thread-conversation-scroll-jump-to-bottom",
            &self.list,
            self.conv.keys.len(),
            band,
            cx.entity_id(),
            cx,
        )
    }

    fn render_conversation(&mut self, model: &ThreadModel, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        // N-12: THE Dock (dock.json ×4) floats over the Conversation's
        // bottom edge: the tree's waiting cards, the root's Queue, queued
        // messages and banners, then THE composer; a reader who may not
        // reply keeps the cards inline and gets no band.
        let may_message = self.composer.read(cx).may_message();
        let (band, sheet) = if may_message {
            root_dock(
                "thread",
                &model.view.rows,
                Some(&self.root_id),
                &self.dock,
                &self.composer,
                &self.text,
                window,
                cx,
            )
        } else {
            (None, None)
        };
        self.sync_list(model, may_message);
        let band_height = if may_message { self.dock.read(cx).band_height() } else { 0. };
        let list = gpui::list(
            self.list.clone(),
            cx.processor(|this, ix: usize, window, cx| this.render_list_row(ix, window, cx)),
        )
        .pt_8()
        .pb(px(band_height.max(16.) + crate::dock::DOCK_GAP));
        let scroller = div()
            .id(SharedString::from(THREAD.test_ids.conversation.clone()))
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
            .children(origin_line(&model.view, cx))
            .child(scroller)
            .into_any_element()
    }

    fn render_issue_child(&mut self) -> AnyElement {
        match &self.issue_view {
            Some(view) => div().flex_1().min_h_0().w_full().child(view.clone()).into_any_element(),
            None => div().flex_1().into_any_element(),
        }
    }
}

impl Focusable for ThreadScreenView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ThreadScreenView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // EXP-1346: an Rc handle, never a deep clone of the model.
        let Some(model) = self.model.clone() else {
            let ready = Store::try_global(cx).is_some_and(|store| store.collections().sessions.read(cx).is_ready());
            // The sessions synced, this root is not among them, and the
            // grace passed (deleted, a lost membership, a foreign id).
            if thread_not_found(ready, false, self.opened_at.elapsed()) {
                return v_flex().size_full().px_4().items_center().justify_center().child(
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
        // EXP-1346: ONE memoised chip resolver per frame for every text.
        self.text = ThreadText::memoized(model.root.team_id.as_deref(), Some(self.chip_cache.clone()))
            .answering(self.composer.read(cx).may_message());
        // NEW-6 (N-1): a thread with ONE session = that session's raw work
        // in place of the Conversation (its composer, Stop in the header).
        let on_conversation = self.focus.is_none() && self.child == ThreadChildSel::Conversation;
        let main = on_conversation.then(|| model.view.main_session_id.clone()).flatten();
        self.sync_main(main.as_deref(), window, cx);
        let header = self.render_header(&model, window, cx);
        // EXP-1323: the live tail's viewer rides the Conversation only.
        let live = (on_conversation && main.is_none())
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
        if let Some((_, view)) = self.focus.clone().or_else(|| self.main.clone()) {
            let inner = view.read(cx).inner().clone();
            let scrolled = inner.read(cx).body_scrolled();
            return v_flex()
                .track_focus(&self.focus_handle)
                .size_full()
                .min_h_0()
                .overflow_hidden()
                .child(header)
                .children(origin_line(&model.view, cx))
                .child(crate::work_header::work_body(
                    div().flex_1().min_h_0().child(inner).into_any_element(),
                    false,
                    scrolled,
                ));
        }
        let child = match self.child.clone() {
            ThreadChildSel::Conversation => self.render_conversation(&model, window, cx),
            ThreadChildSel::Issue(_) => self.render_issue_child(),
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

    /// EXP-1334: an overflowing strip shrinks the ACTIVE session chip first,
    /// then the widest, each to its floor; a strip that fits keeps them all.
    #[test]
    fn overflow_comes_off_the_session_labels() {
        assert_eq!(shrink_session_chips(300., 320., &[120., 100.], &[60., 60.], Some(1)), vec![None, None]);
        assert_eq!(
            shrink_session_chips(330., 300., &[120., 100.], &[60., 60.], Some(1)),
            vec![None, Some(70.)]
        );
        assert_eq!(
            shrink_session_chips(400., 300., &[120., 100.], &[60., 60.], Some(1)),
            vec![Some(60.), Some(60.)]
        );
        assert_eq!(shrink_session_chips(330., 300., &[120., 100.], &[60., 60.], None), vec![Some(90.), None]);
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
            THREAD.copy.placeholder.as_str(),
            THREAD.copy.conversation.as_str(),
        ] {
            assert!(!copy.to_lowercase().contains("thread"), "{copy}");
        }
        assert_eq!(in_thread_label("Offline sync plan"), "In Offline sync plan");
        for option in domain::ask_dock::plan_options() {
            assert!(!option.label.to_lowercase().contains("thread"), "{}", option.label);
        }
    }
}
