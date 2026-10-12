//! EXP-1284 — the THREAD screen ([`Screen::Thread`]): one unit of work, n
//! issues and m runs, as ONE tab (drafts 1 and 2).
//!
//! * The HEADER: the thread's title with the muted `thread` word (a click
//!   renames it through `threads.update`), `headerCaption` at the right.
//! * The CHILD STRIP (`ThreadView::strip`): `Conversation` · one chip per
//!   linked issue (status glyph, mono identifier, title) · `+` · a divider ·
//!   `Guide` (a green dot while the thread's Guide has entries). A child the
//!   viewer has not seen yet (linked < 10 s before it first renders) fades in
//!   with a ring.
//! * The CONVERSATION child: the contract's blocks in order — a `Session N`
//!   block (agent mark, caption, hairline, state word) whose body is the run
//!   itself ([`crate::steer_viewer::SteerSessionView`] EMBEDDED: the Run
//!   face's status row over its thread; an ended run folds to its name and
//!   `Show work`), the issues its agent linked as `Linked` / `Filed and
//!   linked` rows, the PARALLEL card for runs one parent started; and under
//!   it all the reply composer (`Reply in this thread`).
//! * An ISSUE child = the existing issue detail, embedded; the GUIDE child =
//!   the existing Guide face fed THE thread Guide, over the merge capsule of
//!   the thread's PR.
//! * THE Menu on the `+` and on every issue chip: `Link an issue` (THE
//!   search list), `New issue in this thread`, `Start a session here` and, on a
//!   chip, `Unlink <IDENT>`.
//!
//! The model is [`crate::threads::thread_model`] (the contract's
//! `build_thread_view` over the synced rows), re-derived on every collection
//! change it reads. The run feeds this view unfolds live exactly as long as
//! its tab ([`ThreadScreenView::shutdown`]).

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use gpui::{
    div, prelude::FluentBuilder as _, px, Animation, AnimationExt as _, AnyElement, App,
    AppContext as _, ClickEvent, Entity, FocusHandle, Focusable, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Task, Window,
};
use gpui_component::{
    h_flex,
    input::{Input, InputEvent, InputState, TextareaState},
    scroll::ScrollableElement as _,
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;

use domain::thread_view::{
    ThreadBlock, ThreadChild, ThreadLinkRow, ThreadParallelBlock, ThreadRunBlock, ThreadRunState,
};

use crate::controls::PointerContextMenuExt as _;
use crate::controls::WebControl as _;
use gpui_component::button::ButtonVariants as _;
use crate::icons::registry;
use crate::navigation::Screen;
use crate::steer_viewer::SteerSessionView;
use crate::threads::ThreadModel;

/// The ×4 copy (`thread-view.json`'s UI half, phase B brief §9).
pub(crate) const THREAD_WORD: &str = "thread";
pub(crate) const CONVERSATION_LABEL: &str = "Conversation";
pub(crate) const GUIDE_LABEL: &str = "Guide";
pub(crate) const LINK_ISSUE_LABEL: &str = "Link an issue";
pub(crate) const NEW_ISSUE_LABEL: &str = "New issue in this thread";
pub(crate) const START_RUN_LABEL: &str = "Start a session here";
pub(crate) const REPLY_PLACEHOLDER: &str = "Reply in this thread";
pub(crate) const SHOW_WORK_LABEL: &str = "Show work";
pub(crate) const HIDE_WORK_LABEL: &str = "Hide work";
/// The Guide child before the thread's Guide has entries.
pub(crate) const NO_GUIDE_COPY: &str = "No Guide yet. The agent files one as it works.";

/// `Unlink EXP-1270`.
pub(crate) fn unlink_label(identifier: &str) -> String {
    format!("Unlink {identifier}")
}

/// The reply's notice when the session it would resume cannot be (review
/// T9): it ended, so the reply starts a new session in the thread.
pub(crate) const SESSION_ENDED_COPY: &str = "That session ended. Your reply starts a new session in this thread.";

/// A child linked less than this long before it first renders is NEW to the
/// viewer: it fades in with a ring.
pub(crate) const NEW_CHILD_WINDOW: Duration = Duration::from_secs(10);

/// The conversation's reading column (draft 1).
const CONVERSATION_W: f32 = 680.;
/// The header + strip's side inset.
const HEADER_INSET: f32 = 32.;
/// One strip chip's height.
const STRIP_CHIP_H: f32 = 28.;
/// The run block's agent mark and its state badge.
const BLOCK_MARK_PX: f32 = 14.;
const BLOCK_BADGE_PX: f32 = 6.;

/// Which child of the strip is on show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ThreadChildSel {
    Conversation,
    Issue(String),
    Guide,
}

// ---------------------------------------------------------------------------
// Seeds other screens read (New run tab, New issue page)
// ---------------------------------------------------------------------------

/// "Start a session here" → the New run tab `id` composes INTO a thread, with the
/// thread's unsent reply as its draft.
#[derive(Default)]
struct NewRunThreads(HashMap<String, (String, Option<String>)>);

impl gpui::Global for NewRunThreads {}

pub(crate) fn seed_new_run_thread(id: String, thread_id: String, text: Option<String>, cx: &mut App) {
    cx.default_global::<NewRunThreads>().0.insert(id, (thread_id, text));
}

/// The thread (and draft) New run tab `id` was opened for — read once when
/// its composer is built.
pub(crate) fn new_run_thread(id: &str, cx: &App) -> Option<(String, Option<String>)> {
    cx.try_global::<NewRunThreads>()?.0.get(id).cloned()
}

/// The New run tab `id` closed: its seed goes with it.
pub(crate) fn forget_new_run_thread(id: &str, cx: &mut App) {
    if cx.try_global::<NewRunThreads>().is_some_and(|seeds| seeds.0.contains_key(id)) {
        cx.global_mut::<NewRunThreads>().0.remove(id);
    }
}

/// "New issue in this thread" → the FRESH New issue page `draft_id` files
/// INTO a thread. Only the opener's seed: the draft's first write stores it
/// on the row (`issue_drafts.thread_id`), which is what a reopen reads.
#[derive(Default)]
struct DraftThreads(HashMap<String, String>);

impl gpui::Global for DraftThreads {}

/// The thread a fresh New issue page `draft_id` was opened for.
pub(crate) fn draft_thread(draft_id: &str, cx: &App) -> Option<String> {
    cx.try_global::<DraftThreads>()?.0.get(draft_id).cloned()
}

/// The New issue page's `In <thread>` chip (the thread it files into); a
/// click opens the thread.
pub(crate) fn draft_thread_chip(thread_id: &str, cx: &App) -> AnyElement {
    use crate::surface::{glass_pill, PillMode, PillSize};
    let theme = cx.theme();
    let title = crate::threads::thread_title(thread_id, cx);
    let thread_id = thread_id.to_string();
    glass_pill("draft-thread", PillSize::Sm, PillMode::Action, cx)
        .max_w(px(280.))
        .child(
            Icon::new(registry::ACTION_CHAT)
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
                    thread_id: thread_id.clone(),
                },
            );
        })
        .into_any_element()
}

/// `In <thread title>`.
pub(crate) fn in_thread_label(title: &str) -> String {
    format!("In {title}")
}

/// A thread child to select when that thread's view next renders (a new
/// issue filed into it lands on its child).
#[derive(Default)]
struct PendingThreadChild(HashMap<String, String>);

impl gpui::Global for PendingThreadChild {}

/// Open `issue_id`'s child the next time `thread_id`'s view renders with it
/// linked.
pub(crate) fn request_thread_child(thread_id: &str, issue_id: &str, cx: &mut App) {
    cx.default_global::<PendingThreadChild>()
        .0
        .insert(thread_id.to_string(), issue_id.to_string());
}

/// Open a New issue page that files into `thread_id`, on the board of the
/// thread's first issue (else the window's active board).
pub(crate) fn open_new_issue_in_thread(thread_id: &str, window: &mut Window, cx: &mut App) {
    let board_id = crate::threads::thread_model(thread_id, cx)
        .and_then(|model| {
            model
                .view
                .issue_ids
                .iter()
                .find_map(|id| model.issues.get(id).map(|issue| issue.board_id.clone()))
        })
        .unwrap_or_default();
    let draft_id = api::issue_drafts::new_draft_id();
    cx.default_global::<DraftThreads>()
        .0
        .insert(draft_id.clone(), thread_id.to_string());
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
// The writes (threads.link / unlink / update)
// ---------------------------------------------------------------------------

/// `threads.link({id, issueId})` in the background; a refusal is the
/// `Couldn't link the issue` toast (feedback-states).
pub(crate) fn link_issue(thread_id: String, issue_id: String, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        crate::action_error::failed_action("link", "the issue", None, cx);
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move {
                api::threads::threads_link(&trpc, &thread_id, api::threads::ThreadTarget::Issue(&issue_id))
            })
            .await;
        if let Err(err) = result {
            log::warn!("[threads] link failed: {err}");
            cx.update(|cx| crate::action_error::failed_action_err("link", "the issue", &err, cx));
        }
    })
    .detach();
}

/// `threads.unlink({id, issueId})`; a refusal is `Couldn't unlink the issue`.
pub(crate) fn unlink_issue(thread_id: String, issue_id: String, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        crate::action_error::failed_action("unlink", "the issue", None, cx);
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move {
                api::threads::threads_unlink(
                    &trpc,
                    &thread_id,
                    api::threads::ThreadTarget::Issue(&issue_id),
                )
            })
            .await;
        if let Err(err) = result {
            log::warn!("[threads] unlink failed: {err}");
            cx.update(|cx| crate::action_error::failed_action_err("unlink", "the issue", &err, cx));
        }
    })
    .detach();
}

/// `threads.update({id, title})` — `None`/blank clears back to the derived
/// title.
fn rename_thread(thread_id: String, title: Option<String>, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        crate::action_error::failed_action("rename", "the thread", None, cx);
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::threads::threads_update(&trpc, &thread_id, title.as_deref()) })
            .await;
        if let Err(err) = result {
            log::warn!("[threads] rename failed: {err}");
            cx.update(|cx| crate::action_error::failed_action_err("rename", "the thread", &err, cx));
        }
    })
    .detach();
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
    StartRun,
    Separator,
    Unlink(String),
}

pub(crate) fn thread_menu_rows(unlink: Option<&str>) -> Vec<ThreadMenuRow> {
    let mut rows = vec![ThreadMenuRow::LinkIssue, ThreadMenuRow::NewIssue, ThreadMenuRow::StartRun];
    if let Some(identifier) = unlink {
        rows.push(ThreadMenuRow::Separator);
        rows.push(ThreadMenuRow::Unlink(identifier.to_string()));
    }
    rows
}

/// What the menu acts on.
#[derive(Clone)]
pub(crate) struct ThreadMenuTarget {
    pub(crate) thread_id: String,
    pub(crate) team_id: Option<String>,
    /// The issue chip the menu was opened on (`(issue_id, identifier)`).
    pub(crate) chip: Option<(String, String)>,
    /// The issues already in the thread (the search list checks them).
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
    menu = menu.min_w(px(240.));
    let rows = thread_menu_rows(target.chip.as_ref().map(|(_, identifier)| identifier.as_str()));
    for row in rows {
        menu = match row {
            ThreadMenuRow::Separator => menu.separator(),
            ThreadMenuRow::LinkIssue => {
                let source = link_issue_source(target);
                let root = root.clone();
                menu.submenu_with_icon(
                    Some(Icon::new(registry::NAV_SEARCH)),
                    LINK_ISSUE_LABEL,
                    window,
                    cx,
                    move |sub, window, cx| {
                        crate::picker::search_list::menu_search_list_submenu(
                            "thread-link-issue",
                            link_issue_source_clone(&source),
                            sub,
                            root.clone(),
                            window,
                            cx,
                        )
                    },
                )
            }
            ThreadMenuRow::NewIssue => {
                let thread_id = target.thread_id.clone();
                menu.item(
                    pointer_menu_item(None, NEW_ISSUE_LABEL, |_| None, move |window, cx| {
                        open_new_issue_in_thread(&thread_id, window, cx);
                    })
                    .icon(Icon::new(registry::UI_ADD)),
                )
            }
            ThreadMenuRow::StartRun => {
                let thread_id = target.thread_id.clone();
                menu.item(
                    pointer_menu_item(None, START_RUN_LABEL, |_| None, move |window, cx| {
                        crate::screens::open_new_run_tab_in_thread(thread_id.clone(), None, window, cx);
                    })
                    .icon(Icon::new(registry::ACTION_RUN)),
                )
            }
            ThreadMenuRow::Unlink(identifier) => {
                let Some((issue_id, _)) = target.chip.clone() else {
                    continue;
                };
                let thread_id = target.thread_id.clone();
                menu.item(
                    danger_menu_item(unlink_label(&identifier), Icon::new(registry::UI_CLOSE), cx)
                        .on_click(move |_, _, cx| {
                            unlink_issue(thread_id.clone(), issue_id.clone(), cx);
                        }),
                )
            }
        };
    }
    menu
}

/// The search list's data for "Link an issue": the team's issues ranked by
/// THE engine, the ones already in the thread checked; a pick links and
/// closes the menu.
#[derive(Clone)]
struct LinkIssueSource {
    thread_id: String,
    team_id: Option<String>,
    linked: Vec<String>,
}

fn link_issue_source(target: &ThreadMenuTarget) -> LinkIssueSource {
    LinkIssueSource {
        thread_id: target.thread_id.clone(),
        team_id: target.team_id.clone(),
        linked: target.linked.clone(),
    }
}

fn link_issue_source_clone(source: &LinkIssueSource) -> crate::picker::search_list::MenuListSource {
    use crate::picker::search_list as search;
    let rows_source = source.clone();
    let pick_source = source.clone();
    search::MenuListSource {
        placeholder: search::ISSUE_SEARCH_PLACEHOLDER,
        rows: std::rc::Rc::new(move |query: &str, _window: &mut Window, cx: &mut App| {
            let Some(team_id) = rows_source.team_id.as_deref() else {
                return Vec::new();
            };
            search::local_issue_hits(team_id, query, cx)
                .into_iter()
                .map(|hit| {
                    let linked = rows_source.linked.contains(&hit.issue_id);
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
            if !pick_source.linked.iter().any(|id| id == issue_id) {
                link_issue(pick_source.thread_id.clone(), issue_id.to_string(), cx);
            }
            true
        }),
        empty: std::rc::Rc::new(|_query: &str, _cx: &App| search::ISSUE_SEARCH_NO_RESULTS.into()),
        loading: std::rc::Rc::new(|_: &App| false),
        footer: None,
    }
}

// ---------------------------------------------------------------------------
// The painters (pure over their inputs, so the styleguide draws them too)
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

/// The Guide chip's green dot (the thread's Guide has entries).
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

/// A block header: lead · `Session N` · the caption parts · a hairline · the
/// trailing state word (draft 1/2).
pub(crate) fn block_header(
    lead: AnyElement,
    label: &str,
    caption: &[String],
    state: ThreadRunState,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .text_xs()
        .child(lead)
        .child(
            div()
                .flex_shrink_0()
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.foreground)
                .child(SharedString::from(label.to_string())),
        )
        .when(!caption.is_empty(), |row| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_color(muted)
                    .child(SharedString::from(format!("· {}", caption.join(" · ")))),
            )
        })
        .child(div().flex_1().min_w(px(16.)).h(px(1.)).bg(theme.border))
        .child(
            div()
                .flex_shrink_0()
                .text_color(match state {
                    ThreadRunState::NeedsInput => theme.warning,
                    _ => muted,
                })
                .child(state.word()),
        )
        .into_any_element()
}

/// The agent mark a block (or a parallel row) leads with: the working spark
/// while busy, the mark with its badge while live, dimmed once ended.
pub(crate) fn run_mark(
    agent: Option<coding::CodingAgent>,
    state: ThreadRunState,
    busy: bool,
) -> AnyElement {
    match state {
        ThreadRunState::Ended => {
            crate::coding_selects::ended_run_lead(agent, BLOCK_MARK_PX).into_any_element()
        }
        ThreadRunState::NeedsInput => crate::coding_selects::run_lead(
            agent,
            BLOCK_MARK_PX,
            BLOCK_BADGE_PX,
            Some(crate::queries::CodingSessionDisplay::NeedsInput),
        )
        .into_any_element(),
        ThreadRunState::Live => crate::coding_selects::run_lead(
            agent,
            BLOCK_MARK_PX,
            BLOCK_BADGE_PX,
            busy.then_some(crate::queries::CodingSessionDisplay::Working),
        )
        .into_any_element(),
    }
}

/// One agent link row under a block: `<mark> Linked | Filed and linked
/// [issue chip]`.
pub(crate) fn link_row(
    mark: AnyElement,
    label: &str,
    chip: AnyElement,
    cx: &App,
) -> AnyElement {
    h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(mark)
        .child(div().flex_shrink_0().child(SharedString::from(label.to_string())))
        .child(chip)
        .into_any_element()
}

/// One row of the parallel card: mark, name, the issue chip, `device · age`.
pub(crate) fn parallel_row(
    id: impl Into<gpui::ElementId>,
    mark: AnyElement,
    name: &str,
    chip: Option<AnyElement>,
    meta: &str,
    first: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let hover = theme.list_hover;
    div()
        .id(id)
        .flex()
        .flex_row()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_2()
        .h(px(36.))
        .px_3()
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .when(!first, |row| row.border_t_1().border_color(theme.border))
        .child(mark)
        .child(
            div()
                .flex_shrink(1.)
                .min_w_0()
                .truncate()
                .text_sm()
                .text_color(theme.foreground)
                .child(SharedString::from(name.to_string())),
        )
        .children(chip)
        .child(div().flex_1())
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(SharedString::from(meta.to_string())),
        )
}

/// The parallel card's frame around its rows.
pub(crate) fn parallel_card(rows: Vec<AnyElement>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .w_full()
        .min_w_0()
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.secondary.opacity(0.4))
        .overflow_hidden()
        .children(rows)
        .into_any_element()
}

/// A parallel row's trailing meta: `mint · 22 min` (the device, the run's
/// age on the lists' short ladder).
pub(crate) fn parallel_meta(device: Option<&str>, started_at: &str, now_ms: i64) -> String {
    let age = crate::run_rows::stamp_ms(Some(started_at))
        .map(|started| domain::session_row::list_elapsed(now_ms - started));
    [device.map(str::to_string), age]
        .into_iter()
        .flatten()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A folded (ended) run: ONE line — the run's name and `Show work`.
pub(crate) fn folded_line(name: &str, toggle: AnyElement, cx: &App) -> AnyElement {
    h_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .items_center()
        .pl(px(22.))
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(div().flex_shrink(1.).min_w_0().truncate().child(SharedString::from(name.to_string())))
        .child(div().flex_shrink_0().child("·"))
        .child(toggle)
        .into_any_element()
}

/// The thread title row: the title and the muted `thread` word.
pub(crate) fn title_row(title: &str, cx: &App) -> gpui::Div {
    let theme = cx.theme();
    h_flex()
        .min_w_0()
        .gap_2()
        .items_baseline()
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_xl()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .child(SharedString::from(title.to_string())),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(THREAD_WORD),
        )
}

/// One row of a THREAD LIST (the rail's Running, Agent › Recent): the thread
/// title over its `listCaption`, led by the newest run's mark (spinning while busy, amber while it needs input) on a live thread,
/// the thread glyph on a quiet one. The whole row opens the thread.
pub(crate) struct ThreadRowSpec {
    pub(crate) id_prefix: &'static str,
    pub(crate) index: usize,
    pub(crate) title: SharedString,
    pub(crate) caption: SharedString,
    pub(crate) agent: coding::CodingAgent,
    pub(crate) live: bool,
    pub(crate) display: Option<crate::queries::CodingSessionDisplay>,
    pub(crate) active: bool,
    /// The rail's glass ground (`true`) or a list's.
    pub(crate) glass: bool,
    pub(crate) on_open: crate::run_rows::RunRowAction,
    /// My live sessions in the thread `(id, name)`: the row's right-click
    /// menu stops them (review T7, the EXP-874 grammar).
    pub(crate) stop: Vec<(String, String)>,
}

/// The rail row menu's verb (×4 `Stop session`); with several of my
/// sessions live in one thread each row names its session.
pub(crate) fn stop_session_label(name: &str, several: bool) -> String {
    if several {
        format!("Stop session · {name}")
    } else {
        "Stop session".to_string()
    }
}

pub(crate) fn thread_row(spec: ThreadRowSpec, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let (hover, active_fill) = if spec.glass {
        (
            theme::tokens::glass::FILL_ROW.to_hsla(),
            theme::tokens::glass::FILL_ACTIVE.to_hsla(),
        )
    } else {
        (theme.list_hover, theme.list_active)
    };
    let lead: AnyElement = if spec.live {
        crate::coding_selects::run_lead(Some(spec.agent), BLOCK_MARK_PX, BLOCK_BADGE_PX, spec.display)
            .into_any_element()
    } else {
        div()
            .flex_shrink_0()
            .child(Icon::new(registry::ACTION_CHAT).size(px(BLOCK_MARK_PX)).text_color(muted))
            .into_any_element()
    };
    let active = spec.active;
    let on_open = spec.on_open;
    let stop = spec.stop;
    let row = div()
        .id((SharedString::from(format!("{}-row", spec.id_prefix)), spec.index))
        .flex()
        .flex_row()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .items_start()
        .gap(px(domain::list_item::GAP))
        .py(px(6.))
        .pl(px(domain::list_item::lead_x(0)))
        .pr_3()
        .rounded(px(6.))
        .cursor_pointer()
        .when(active, |row| row.bg(active_fill))
        .hover(move |style| style.bg(if active { active_fill } else { hover }))
        .child(div().pt(px(3.)).child(lead))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .text_color(theme.foreground)
                        .child(spec.title),
                )
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .text_color(muted)
                        .child(spec.caption),
                ),
        )
        .on_click(move |event, window, cx| on_open(event, window, cx));
    if stop.is_empty() {
        return row.into_any_element();
    }
    row.pointer_context_menu(move |mut menu, _window, cx| {
        let several = stop.len() > 1;
        for (session_id, name) in &stop {
            let session_id = session_id.clone();
            menu = menu.item(
                crate::controls::danger_menu_item(
                    stop_session_label(name, several),
                    Icon::new(registry::CODING_STOP),
                    cx,
                )
                .on_click(move |_, window, cx| {
                    let host = crate::coding_flow::LocalSessions::global_ref(cx).and_then(|sessions| {
                        sessions
                            .read(cx)
                            .session_by_id(&session_id)
                            .map(|session| session.host.clone())
                    });
                    crate::session_bar::prompt_kill_session(host, session_id.clone(), window, cx);
                }),
            );
        }
        menu
    })
    .into_any_element()
}

/// Where a thread list row opens its thread.
#[derive(Clone)]
pub(crate) enum ThreadRowOpen {
    /// The rail's Running section: no list, and NO tab (EXP-923 — a live
    /// thread is one click away in Running for as long as it runs).
    LiveRail,
    /// A list that stays beside the thread (Agent › Recent).
    List(crate::navigation::TabOrigin),
}

/// [`thread_row`] for a [`crate::threads::ThreadListRow`], opening its
/// thread. `display` overrides the row's own mark state (the rail's
/// starting spinner).
#[allow(clippy::too_many_arguments)]
pub(crate) fn thread_list_row(
    id_prefix: &'static str,
    index: usize,
    row: &crate::threads::ThreadListRow,
    display: Option<crate::queries::CodingSessionDisplay>,
    active: bool,
    glass: bool,
    open: ThreadRowOpen,
    cx: &App,
) -> AnyElement {
    let thread_id = row.thread_id.clone();
    let orphan = row.orphan_session_id.clone();
    thread_row(
        ThreadRowSpec {
            id_prefix,
            index,
            title: row.title.clone(),
            caption: row.caption.clone(),
            agent: row.agent,
            live: row.live,
            display,
            active,
            glass,
            stop: row.my_live.clone(),
            on_open: Box::new(move |_, window, cx| {
                // T4: a session in no thread yet opens as the session.
                let screen = match &orphan {
                    Some(session_id) => Screen::Session {
                        session_id: session_id.clone(),
                    },
                    None => Screen::Thread {
                        thread_id: thread_id.clone(),
                    },
                };
                match &open {
                    ThreadRowOpen::LiveRail => crate::navigation::navigate_from_live_rail(window, cx, screen),
                    ThreadRowOpen::List(origin) => {
                        crate::navigation::navigate_from(window, cx, screen, origin.clone())
                    }
                }
            }),
        },
        cx,
    )
}

/// The issue page's `Threads` label (draft 3).
pub(crate) const THREADS_LABEL: &str = "Threads";

/// The issue page's `Threads` chip: a small glass pill — the thread glyph
/// (its newest run's mark while live: spinning while busy, amber while it
/// needs input) and the title; a click opens the thread.
pub(crate) fn issue_thread_chip(
    index: usize,
    chip: &crate::threads::IssueThreadChip,
    cx: &App,
) -> AnyElement {
    use crate::surface::{glass_pill, PillMode, PillSize};
    let theme = cx.theme();
    let glyph = PillSize::Sm.glyph();
    let lead: AnyElement = if chip.live {
        let display = if chip.needs_input {
            Some(crate::queries::CodingSessionDisplay::NeedsInput)
        } else if chip.busy {
            Some(crate::queries::CodingSessionDisplay::Working)
        } else {
            None
        };
        crate::coding_selects::run_lead(Some(chip.agent), glyph, 5., display).into_any_element()
    } else {
        Icon::new(registry::ACTION_CHAT)
            .with_size(px(glyph))
            .text_color(theme.muted_foreground)
            .into_any_element()
    };
    let thread_id = chip.thread_id.clone();
    glass_pill(("issue-thread-chip", index), PillSize::Sm, PillMode::Action, cx)
        .max_w(px(260.))
        .child(lead)
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(theme.foreground)
                .child(chip.title.clone()),
        )
        .on_click(move |_, window, cx| {
            crate::navigation::navigate(
                window,
                cx,
                Screen::Thread {
                    thread_id: thread_id.clone(),
                },
            );
        })
        .into_any_element()
}

/// The issue page's `Threads` row: the muted label and one chip per thread
/// holding the issue. `None` while it sits in none.
pub(crate) fn issue_threads_row(issue_id: &str, cx: &App) -> Option<AnyElement> {
    let chips = crate::threads::issue_threads(issue_id, cx);
    if chips.is_empty() {
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
                    .child(THREADS_LABEL),
            )
            .children(
                chips
                    .iter()
                    .enumerate()
                    .map(|(index, chip)| issue_thread_chip(index, chip, cx)),
            )
            .into_any_element(),
    )
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

pub(crate) struct ThreadScreenView {
    thread_id: String,
    child: ThreadChildSel,
    model: Option<ThreadModel>,
    /// Folded (ended) runs the reader opened with Show work.
    expanded: HashSet<String>,
    /// One embedded run view per unfolded run block, built on first show.
    run_views: HashMap<String, Entity<SteerSessionView>>,
    /// The issue child's detail view (built on first show, re-pointed).
    issue_view: Option<Entity<crate::issue_detail::IssueDetailView>>,
    /// The Guide child's run view (the run whose Guide face shows THE thread
    /// Guide), keyed by that run.
    guide_view: Option<(String, Entity<SteerSessionView>)>,
    reply: Entity<TextareaState>,
    mention: Entity<crate::mention_input::MentionInput>,
    mention_team: Option<String>,
    /// The inline rename field while the title is being edited.
    rename: Option<Entity<InputState>>,
    /// When each issue item was FIRST rendered, and whether it was new then
    /// (linked inside [`NEW_CHILD_WINDOW`]) — the fade-in ring.
    first_seen: HashMap<String, (Instant, bool)>,
    /// Review T14: the one `threads.get` asked before the page concludes the
    /// thread is missing (never a flash of "Thread not found").
    fetch: ThreadFetch,
    focus_handle: FocusHandle,
    _ring_tick: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl ThreadScreenView {
    pub(crate) fn new(thread_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let reply = cx.new(|cx| {
            crate::controls::web_textarea(1, 6, window, cx)
                .submit_on_enter(true)
                .placeholder(REPLY_PLACEHOLDER)
        });
        let mention = cx.new(|cx| {
            let mut mention = crate::mention_input::MentionInput::new(reply.clone(), cx);
            mention.set_appearance(false);
            mention
        });
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
            subscriptions.push(cx.observe(&collections.threads, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&collections.thread_items, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&collections.issues, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&collections.coding_sessions, |this, _, cx| this.refresh(cx)));
            subscriptions.push(cx.observe(&collections.devices, |_, _, cx| cx.notify()));
        }
        let mut this = Self {
            thread_id,
            child: ThreadChildSel::Conversation,
            model: None,
            expanded: HashSet::new(),
            run_views: HashMap::new(),
            issue_view: None,
            guide_view: None,
            reply,
            mention,
            mention_team: None,
            rename: None,
            first_seen: HashMap::new(),
            fetch: ThreadFetch::NotAsked,
            focus_handle: cx.focus_handle(),
            _ring_tick: None,
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
        let model = crate::threads::thread_model(&self.thread_id, cx);
        if let Some(model) = &model {
            self.note_seen(model, cx);
            let team = model.thread.team_id.clone();
            if team != self.mention_team {
                self.mention_team = team.clone();
                self.mention.update(cx, |mention, _| {
                    mention.set_source(team.map(crate::markdown::store_completion_source));
                });
            }
            // An unlinked issue's child closes.
            if let ThreadChildSel::Issue(issue_id) = &self.child {
                if !model.view.issue_ids.contains(issue_id) {
                    self.child = ThreadChildSel::Conversation;
                }
            }
        }
        self.model = model;
        cx.notify();
    }

    /// Record the first render of every issue item; one linked inside the
    /// window gets the ring until [`NEW_CHILD_WINDOW`] passes.
    fn note_seen(&mut self, model: &ThreadModel, cx: &mut gpui::Context<Self>) {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut armed = false;
        for item in &model.items {
            if item.issue().is_none() || self.first_seen.contains_key(&item.id) {
                continue;
            }
            let created = crate::run_rows::stamp_ms(item.created_at.as_deref()).unwrap_or(0);
            let fresh = now_ms - created < NEW_CHILD_WINDOW.as_millis() as i64;
            self.first_seen.insert(item.id.clone(), (Instant::now(), fresh));
            armed |= fresh;
        }
        if armed {
            self._ring_tick = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(NEW_CHILD_WINDOW).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            }));
        }
    }

    /// Whether the issue item `item_id` still wears its NEW ring.
    fn ringed(&self, item_id: &str) -> bool {
        self.first_seen
            .get(item_id)
            .is_some_and(|(at, fresh)| *fresh && at.elapsed() < NEW_CHILD_WINDOW)
    }

    /// The tab closed: every run feed this view unfolded goes (never a run).
    pub(crate) fn shutdown(&mut self, cx: &mut App) {
        for (_, view) in self.run_views.drain() {
            view.update(cx, |view, _| view.shutdown());
        }
        if let Some((_, view)) = self.guide_view.take() {
            view.update(cx, |view, _| view.shutdown());
        }
    }

    fn set_child(&mut self, child: ThreadChildSel, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.child == child {
            return;
        }
        // Leaving an issue child flushes its edit (the close-tab rule).
        if let (ThreadChildSel::Issue(_), Some(view)) = (&self.child, &self.issue_view) {
            view.update(cx, |detail, cx| {
                detail.flush_title(cx);
                detail.flush_description(cx);
            });
        }
        if let ThreadChildSel::Issue(issue_id) = &child {
            let view = self
                .issue_view
                .get_or_insert_with(|| cx.new(|cx| crate::issue_detail::IssueDetailView::new(window, cx)))
                .clone();
            let issue_id = issue_id.clone();
            view.update(cx, |detail, cx| detail.set_issue(issue_id, window, cx));
        }
        self.child = child;
        cx.notify();
    }

    fn run_view(
        &mut self,
        run_id: &str,
        show_work: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<SteerSessionView> {
        if let Some(view) = self.run_views.get(run_id) {
            return view.clone();
        }
        let view = crate::session_screen::embedded_run_view(run_id, show_work, window, cx);
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        self.run_views.insert(run_id.to_string(), view.clone());
        view
    }

    fn toggle_expanded(&mut self, run_id: &str, cx: &mut gpui::Context<Self>) {
        if !self.expanded.remove(run_id) {
            self.expanded.insert(run_id.to_string());
        } else if let Some(view) = self.run_views.remove(run_id) {
            // A folded run drops its feed again.
            view.update(cx, |view, _| view.shutdown());
        }
        cx.notify();
    }

    // ── the reply ────────────────────────────────────────────────────────

    /// `Reply in this thread`: to the latest LIVE run of mine (the steer
    /// send); else, when the newest run is mine and over, RESUME it with the
    /// reply (the resume stays in the thread); else start a run in this
    /// thread (the New run tab, the reply as its draft).
    fn send_reply(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let text = self.reply.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        let Some(model) = self.model.clone() else {
            return;
        };
        let Some(me) = crate::queries::active_account(cx).map(|account| account.user_id) else {
            return;
        };
        let now = chrono::Utc::now().timestamp();
        let mine = |run: &domain::rows::CodingSession| run.user_id.as_deref() == Some(me.as_str());
        let live_mine = model
            .view
            .run_ids
            .iter()
            .rev()
            .filter_map(|id| model.runs.get(id))
            .find(|run| mine(run) && crate::queries::coding_session_is_live(run, now))
            .map(|run| run.id.clone());
        match reply_target(
            live_mine.as_deref(),
            // Review T9: "dead" = the heartbeat rule, not only an ended row.
            model.latest_run().map(|run| {
                (run.id.as_str(), mine(run), !crate::queries::coding_session_is_live(run, now))
            }),
        ) {
            ReplyTarget::Steer(run_id) => {
                let view = self.run_view(&run_id, false, window, cx);
                let sent = view.update(cx, |view, cx| view.send_reply(&text, cx));
                if sent {
                    self.clear_reply(window, cx);
                } else {
                    crate::action_error::failed_action("send", "the message", None, cx);
                }
            }
            ReplyTarget::Resume(run_id) => {
                let Some(run) = model.runs.get(&run_id).cloned() else {
                    return;
                };
                // A heartbeat gone stale on a row the server has not ended
                // cannot be resumed: say so and start anew in the thread.
                let path = crate::run_rows::run_has_ended(&run)
                    .then(|| crate::work_header::resume_path_cached(&run, &mut None, cx))
                    .flatten();
                match path {
                    Some(crate::session_screen::ResumePath::Local) => {
                        crate::action_run::resume_run_on_account(
                            run_id,
                            Some(window.window_handle()),
                            crate::coding_flow::RunLanding::ThisWindow,
                            coding::LaunchOrigin::Local,
                            None,
                            Some(text),
                            cx,
                        );
                        self.clear_reply(window, cx);
                    }
                    Some(crate::session_screen::ResumePath::Remote { device_id }) => {
                        // A relay resume carries no prompt: the reply stays
                        // in the box to send once the run is live again.
                        let label = crate::queries::device_label_for_id(cx, &device_id)
                            .unwrap_or_else(|| "its machine".to_string());
                        crate::session_screen::resume_remote(run_id, device_id, label, window, cx);
                    }
                    None => {
                        crate::toast::info(SESSION_ENDED_COPY, window, cx);
                        crate::screens::open_new_run_tab_in_thread(
                            self.thread_id.clone(),
                            Some(text),
                            window,
                            cx,
                        );
                        self.clear_reply(window, cx);
                    }
                }
            }
            ReplyTarget::Start => {
                crate::screens::open_new_run_tab_in_thread(self.thread_id.clone(), Some(text), window, cx);
                self.clear_reply(window, cx);
            }
        }
    }

    fn clear_reply(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.reply.update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    // ── rename ───────────────────────────────────────────────────────────

    fn begin_rename(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(model) = &self.model else { return };
        let current = model.thread.explicit_title().unwrap_or(&model.view.title).to_string();
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder(model.view.title.clone());
            state.set_value(current, window, cx);
            state
        });
        cx.subscribe_in(&input, window, |this, input, event: &InputEvent, window, cx| match event {
            InputEvent::PressEnter { .. } => {
                let value = input.read(cx).value().trim().to_string();
                this.rename = None;
                rename_thread(this.thread_id.clone(), (!value.is_empty()).then_some(value), cx);
                cx.notify();
                let _ = window;
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

    // ── render ───────────────────────────────────────────────────────────

    fn menu_target(&self, chip: Option<(String, String)>) -> ThreadMenuTarget {
        ThreadMenuTarget {
            thread_id: self.thread_id.clone(),
            team_id: self.model.as_ref().and_then(|model| model.thread.team_id.clone()),
            chip,
            linked: self
                .model
                .as_ref()
                .map(|model| model.view.issue_ids.clone())
                .unwrap_or_default(),
        }
    }

    fn render_header(&mut self, model: &ThreadModel, cx: &mut gpui::Context<Self>) -> AnyElement {
        let title: AnyElement = match &self.rename {
            Some(input) => div()
                .w(px(420.))
                .child(Input::new(input).appearance(false).text_xl())
                .into_any_element(),
            None => title_row(&model.view.title, cx)
                .id("thread-title")
                .cursor_text()
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.begin_rename(window, cx)))
                .into_any_element(),
        };
        v_flex()
            .w_full()
            .px(px(HEADER_INSET))
            .pt_5()
            .gap_3()
            .child(title)
            .child(self.render_strip(model, cx))
            .child(div().w_full().h(px(1.)).bg(cx.theme().border))
            .into_any_element()
    }

    fn render_strip(&mut self, model: &ThreadModel, cx: &mut gpui::Context<Self>) -> AnyElement {
        let mut chips: Vec<AnyElement> = Vec::new();
        // The Guide (after the divider) never scrolls out of reach.
        let mut trailing: Vec<AnyElement> = Vec::new();
        for child in &model.view.strip {
            match child {
                ThreadChild::Conversation => {
                    let active = self.child == ThreadChildSel::Conversation;
                    chips.push(
                        strip_chip("thread-child-conversation", active, cx)
                            .child(Icon::new(registry::ACTION_CHAT).xsmall())
                            .child(CONVERSATION_LABEL)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.set_child(ThreadChildSel::Conversation, window, cx)
                            }))
                            .into_any_element(),
                    );
                }
                ThreadChild::Issue {
                    issue_id,
                    identifier,
                    title,
                } => {
                    let active = self.child == ThreadChildSel::Issue(issue_id.clone());
                    let status = model
                        .issues
                        .get(issue_id)
                        .map(|issue| crate::queries::resolve_issue_status(cx, issue));
                    let item_id = model
                        .items
                        .iter()
                        .find(|item| item.issue() == Some(issue_id.as_str()))
                        .map(|item| item.id.clone())
                        .unwrap_or_default();
                    let ringed = self.ringed(&item_id);
                    let target = self.menu_target(Some((issue_id.clone(), identifier.clone())));
                    let open = issue_id.clone();
                    let chip = strip_chip(
                        SharedString::from(format!("thread-child-{issue_id}")),
                        active,
                        cx,
                    )
                    .children(strip_issue_body(status, identifier, title, active, cx))
                    .when(ringed, |chip| chip.border_color(cx.theme().ring))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.set_child(ThreadChildSel::Issue(open.clone()), window, cx)
                    }))
                    .pointer_context_menu(move |menu, window, cx| thread_menu(menu, &target, window, cx));
                    let chip: AnyElement = if ringed {
                        div()
                            .flex_shrink_0()
                            .child(chip)
                            .with_animation(
                                SharedString::from(format!("thread-child-enter-{item_id}")),
                                Animation::new(theme::motion::ENTER).with_easing(theme::motion::decelerate()),
                                |el, delta| el.opacity(delta),
                            )
                            .into_any_element()
                    } else {
                        chip.into_any_element()
                    };
                    chips.push(chip);
                }
                ThreadChild::Add => {
                    let target = self.menu_target(None);
                    let trigger = crate::controls::ghost_icon_button(
                        "thread-child-add",
                        Icon::new(registry::UI_ADD),
                        cx,
                    );
                    chips.push(
                        crate::controls::PointerMenu::new("thread-child-add-menu", trigger, move |menu, window, cx| {
                            thread_menu(menu, &target, window, cx)
                        })
                        .into_any_element(),
                    );
                    trailing.push(strip_divider(cx));
                }
                ThreadChild::Guide { dot } => {
                    let active = self.child == ThreadChildSel::Guide;
                    trailing.push(
                        strip_chip("thread-child-guide", active, cx)
                            .child(Icon::new(registry::NAV_REVIEWS).xsmall())
                            .child(GUIDE_LABEL)
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
            .child(div().flex_1())
            .child(
                div()
                    .flex_shrink_0()
                    .pl_3()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(model.view.header_caption.clone())),
            )
            .into_any_element()
    }

    fn issue_link_chip(
        &self,
        key: &str,
        link: &ThreadLinkRow,
        model: &ThreadModel,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let status = model
            .issues
            .get(&link.issue_id)
            .map(|issue| crate::queries::resolve_issue_status(cx, issue));
        let open = link.issue_id.clone();
        let mut chip = crate::issue_chip::issue_chip(
            SharedString::from(format!("thread-link-{key}-{}", link.item_id)),
            link.identifier.clone(),
            link.title.clone(),
        )
        .max_title_width(px(220.))
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.set_child(ThreadChildSel::Issue(open.clone()), window, cx)
        }));
        if let Some(status) = status {
            chip = chip.status(status);
        }
        chip.into_any_element()
    }

    fn render_links(
        &self,
        key: &str,
        links: &[ThreadLinkRow],
        agent: Option<coding::CodingAgent>,
        model: &ThreadModel,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<AnyElement> {
        links
            .iter()
            .map(|link| {
                let mark = match agent {
                    Some(agent) => crate::coding_selects::agent_mark(agent)
                        .size(px(BLOCK_MARK_PX))
                        .into_any_element(),
                    None => Icon::new(registry::SETTINGS_AGENTS).xsmall().into_any_element(),
                };
                let chip = self.issue_link_chip(key, link, model, cx);
                link_row(mark, &link.label, chip, cx)
            })
            .collect()
    }

    fn agent_of(model: &ThreadModel, run_id: &str) -> Option<coding::CodingAgent> {
        let run = model.runs.get(run_id)?;
        Some(
            run.agent
                .as_deref()
                .and_then(coding::CodingAgent::parse)
                .unwrap_or_default(),
        )
    }

    fn render_run_block(
        &mut self,
        block: &ThreadRunBlock,
        model: &ThreadModel,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let agent = Self::agent_of(model, &block.run_id);
        let busy = model
            .runs
            .get(&block.run_id)
            .is_some_and(|run| crate::queries::session_agent_busy(run, None, chrono::Utc::now().timestamp()));
        let header = block_header(run_mark(agent, block.state, busy), &block.label, &block.caption, block.state, cx);
        let expanded = self.expanded.contains(&block.run_id);
        let body: AnyElement = if block.folded {
            let name = model
                .runs
                .get(&block.run_id)
                .map(|run| {
                    let issue = run.issue_id.as_deref().and_then(|id| model.issues.get(id));
                    crate::run_rows::run_title(run, issue, &[]).to_string()
                })
                .unwrap_or_default();
            let run_id = block.run_id.clone();
            let toggle = crate::controls::text_button(
                SharedString::from(format!("thread-show-work-{}", block.run_id)),
                if expanded { HIDE_WORK_LABEL } else { SHOW_WORK_LABEL },
                crate::controls::TextButtonVariant::Text,
                cx,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_expanded(&run_id, cx)))
            .into_any_element();
            let line = folded_line(&name, toggle, cx);
            if expanded {
                let body = self.session_body(&block.run_id, true, model, window, cx);
                v_flex().w_full().gap_2().child(line).child(body).into_any_element()
            } else {
                line
            }
        } else {
            self.session_body(&block.run_id, false, model, window, cx)
        };
        let links = self.render_links(&block.run_id, &block.links, agent, model, cx);
        v_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .child(header)
            .child(body)
            .children(links)
            .into_any_element()
    }

    /// One session block's body. MINE: the session itself, embedded (its
    /// status row over its thread — only the Guide entries THIS session
    /// wrote, `ThreadModel::session_guide`). A TEAMMATE's (review T5,
    /// EXP-312): the status row off the synced row, never a steer viewer
    /// and never its transcript.
    fn session_body(
        &mut self,
        run_id: &str,
        show_work: bool,
        model: &ThreadModel,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let me = crate::queries::active_account(cx).map(|account| account.user_id);
        let Some(run) = model.runs.get(run_id) else {
            return div().into_any_element();
        };
        if me.is_none() || run.user_id != me {
            return div()
                .pl(px(22.))
                .child(crate::run_rows::synced_status_row(
                    SharedString::from(format!("thread-status-{run_id}")),
                    run,
                    cx,
                ))
                .into_any_element();
        }
        let guide = model.session_guide(run_id);
        let view = self.run_view(run_id, show_work, window, cx);
        view.update(cx, |view, cx| view.set_embedded_results(guide, cx));
        view.into_any_element()
    }

    fn render_parallel_block(
        &mut self,
        index: usize,
        block: &ThreadParallelBlock,
        model: &ThreadModel,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let agent = block.runs.first().and_then(|run| Self::agent_of(model, &run.run_id));
        let any_busy = block.runs.iter().any(|row| {
            model
                .runs
                .get(&row.run_id)
                .is_some_and(|run| crate::queries::session_agent_busy(run, None, chrono::Utc::now().timestamp()))
        });
        let header = block_header(
            run_mark(agent, block.state, any_busy),
            &block.label,
            &block.caption,
            block.state,
            cx,
        );
        let now_ms = chrono::Utc::now().timestamp_millis();
        let rows: Vec<AnyElement> = block
            .runs
            .iter()
            .enumerate()
            .map(|(row_ix, row)| {
                let run = model.runs.get(&row.run_id);
                let agent = Self::agent_of(model, &row.run_id);
                let busy = run.is_some_and(|run| {
                    crate::queries::session_agent_busy(run, None, chrono::Utc::now().timestamp())
                });
                let chip = row.issue_id.as_ref().and_then(|issue_id| model.issues.get(issue_id)).map(|issue| {
                    let status = crate::queries::resolve_issue_status(cx, issue);
                    crate::issue_chip::issue_chip(
                        SharedString::from(format!("thread-parallel-chip-{}", row.run_id)),
                        issue.identifier.clone(),
                        issue.title.clone(),
                    )
                    .small()
                    .status(status)
                    .into_any_element()
                });
                let meta = parallel_meta(row.device_label.as_deref(), &row.started_at, now_ms);
                let run_id = row.run_id.clone();
                parallel_row(
                    SharedString::from(format!("thread-parallel-{index}-{}", row.run_id)),
                    run_mark(agent, row.state, busy),
                    &row.name,
                    chip,
                    &meta,
                    row_ix == 0,
                    cx,
                )
                .on_click(move |_, window, cx| {
                    crate::session_screen::open_session(&run_id, window, cx);
                })
                .into_any_element()
            })
            .collect();
        let key = format!("parallel-{index}");
        let links = self.render_links(&key, &block.links, agent, model, cx);
        v_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .child(header)
            .child(parallel_card(rows, cx))
            .children(links)
            .into_any_element()
    }

    fn render_conversation(
        &mut self,
        model: &ThreadModel,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let mut blocks: Vec<AnyElement> = Vec::new();
        for (index, block) in model.view.conversation.iter().enumerate() {
            blocks.push(match block {
                ThreadBlock::Run(block) => self.render_run_block(block, model, window, cx),
                ThreadBlock::Parallel(block) => self.render_parallel_block(index, block, model, cx),
            });
        }
        let empty = blocks.is_empty().then(|| {
            div()
                .w_full()
                .py_8()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("No sessions in this thread yet. Reply to start one.")
        });
        let column = v_flex()
            .w_full()
            .max_w(px(CONVERSATION_W))
            .mx_auto()
            .px(px(crate::work_header::WORK_GUTTER))
            .pt_5()
            .pb_8()
            .gap_5()
            .children(blocks)
            .children(empty);
        let reply = self.render_reply(cx);
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
                    .overflow_y_scrollbar()
                    .child(column),
            )
            .child(
                div()
                    .w_full()
                    .px(px(crate::work_header::WORK_GUTTER))
                    .pb_4()
                    .child(div().w_full().max_w(px(CONVERSATION_W)).mx_auto().child(reply)),
            )
            .into_any_element()
    }

    fn render_reply(&mut self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let empty = self.reply.read(cx).value().trim().is_empty();
        let submit = crate::composer::composer_submit("thread-reply-send", registry::UI_SEND, empty, cx)
            .tooltip(REPLY_PLACEHOLDER)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.send_reply(window, cx)));
        let composer = crate::composer::GlassComposer::new(
            div().w_full().min_w_0().child(self.mention.clone()).into_any_element(),
        )
        .inline_tools(true)
        .submit(submit);
        crate::composer::glass_composer(composer).into_any_element()
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
        let has_guide = domain::thread_view::thread_has_guide(model.thread.results.as_ref());
        // The Guide reads through the session behind the thread's PR (review
        // T8: newest open, else newest merged), else the newest session.
        let run_id = model.guide_session_id();
        let body: AnyElement = match (has_guide, run_id) {
            (true, Some(run_id)) => {
                let stale = self.guide_view.as_ref().is_none_or(|(id, _)| *id != run_id);
                if stale {
                    if let Some((_, view)) = self.guide_view.take() {
                        view.update(cx, |view, _| view.shutdown());
                    }
                    let view = crate::session_screen::guide_run_view(&run_id, window, cx);
                    cx.observe(&view, |_, _, cx| cx.notify()).detach();
                    self.guide_view = Some((run_id, view));
                }
                let view = self.guide_view.as_ref().map(|(_, view)| view.clone()).expect("built above");
                view.update(cx, |view, cx| view.set_run_face(crate::screens::RunFace::Guide, cx));
                div().flex_1().min_h_0().w_full().child(view).into_any_element()
            }
            _ => div()
                .flex_1()
                .w_full()
                .pt_10()
                .flex()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(NO_GUIDE_COPY)
                .into_any_element(),
        };
        // THE merge capsule of the thread's PR, at the bottom.
        let capsule = model.pr_target().map(|target| {
            h_flex()
                .w_full()
                .justify_center()
                .pb_4()
                .pt_2()
                .child(crate::work_header::merge_pill(
                    "thread-guide-merge",
                    &target,
                    true,
                    crate::work_header::header_action_size(false),
                    cx,
                ))
        });
        v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(body)
            .children(capsule)
            .into_any_element()
    }
}

/// Where `Reply in this thread` goes (pure for the table test): a live run
/// of mine takes the message; else the newest run, when it is mine and
/// over, resumes with it; else a new run starts in the thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReplyTarget {
    Steer(String),
    Resume(String),
    Start,
}

/// `latest` = `(run id, mine, ended)` of the thread's newest run.
pub(crate) fn reply_target(live_mine: Option<&str>, latest: Option<(&str, bool, bool)>) -> ReplyTarget {
    if let Some(run_id) = live_mine {
        return ReplyTarget::Steer(run_id.to_string());
    }
    match latest {
        Some((run_id, true, true)) => ReplyTarget::Resume(run_id.to_string()),
        _ => ReplyTarget::Start,
    }
}

/// Review T14 — the page's one `threads.get` before "Thread not found".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThreadFetch {
    NotAsked,
    InFlight,
    /// The server has it: the row is on its way, keep the skeleton.
    Found,
    /// The server does not (or refuses): missing.
    Missing,
}

/// Whether the page may say "Thread not found" (pure, review T14): the
/// threads shape has settled, no `thread_items` row names the id, and the
/// one `threads.get` came back without it.
pub(crate) fn thread_not_found(threads_ready: bool, item_names_it: bool, fetch: ThreadFetch) -> bool {
    threads_ready && !item_names_it && fetch == ThreadFetch::Missing
}

impl ThreadScreenView {
    /// [`thread_not_found`] against the store, asking `threads.get` once
    /// the moment the synced answer alone would have concluded "missing".
    fn thread_missing(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let Some(store) = Store::try_global(cx) else {
            return false;
        };
        let collections = store.collections();
        let ready = collections.threads.read(cx).is_ready();
        let named = collections
            .thread_items
            .read(cx)
            .iter()
            .any(|item| item.thread_id == self.thread_id);
        if ready && !named && self.fetch == ThreadFetch::NotAsked {
            self.fetch = ThreadFetch::InFlight;
            if let Some(trpc) = crate::queries::trpc_client(cx) {
                let id = self.thread_id.clone();
                cx.spawn(async move |this, cx| {
                    let found = cx
                        .background_executor()
                        .spawn(async move { api::threads::threads_get(&trpc, &id).is_ok() })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.fetch = if found { ThreadFetch::Found } else { ThreadFetch::Missing };
                        cx.notify();
                    });
                })
                .detach();
            } else {
                self.fetch = ThreadFetch::Missing;
            }
        }
        thread_not_found(ready, named, self.fetch)
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
            // The threads synced and this one is not among them, no item
            // names it, and the server says so too (deleted, a lost
            // membership, a foreign id): fixture `thread-not-found`.
            if self.thread_missing(cx) {
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
                                        crate::navigation::navigate(window, cx, Screen::Chat);
                                    }),
                            ),
                        ),
                    );
            }
            // FEED: skeletons, never "Loading…" (feedback-states.json).
            return v_flex()
                .size_full()
                .px(px(HEADER_INSET))
                .pt_5()
                .child(crate::feedback::skeleton_rows("thread-loading", 3, 36., cx));
        };
        // A new issue filed into this thread lands on its child once its
        // item has synced.
        let pending = cx
            .try_global::<PendingThreadChild>()
            .and_then(|pending| pending.0.get(&self.thread_id).cloned())
            .filter(|issue_id| model.view.issue_ids.contains(issue_id));
        if let Some(issue_id) = pending {
            cx.global_mut::<PendingThreadChild>().0.remove(&self.thread_id);
            self.set_child(ThreadChildSel::Issue(issue_id), window, cx);
        }
        let header = self.render_header(&model, cx);
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

    /// The "+" offers the three verbs; an issue chip adds its Unlink after a
    /// separator (draft 2's menu).
    #[test]
    fn the_thread_menu_rows() {
        assert_eq!(
            thread_menu_rows(None),
            vec![ThreadMenuRow::LinkIssue, ThreadMenuRow::NewIssue, ThreadMenuRow::StartRun]
        );
        assert_eq!(
            thread_menu_rows(Some("EXP-1270")),
            vec![
                ThreadMenuRow::LinkIssue,
                ThreadMenuRow::NewIssue,
                ThreadMenuRow::StartRun,
                ThreadMenuRow::Separator,
                ThreadMenuRow::Unlink("EXP-1270".into()),
            ]
        );
        assert_eq!(unlink_label("EXP-1270"), "Unlink EXP-1270");
    }

    /// The reply's three paths (phase B brief §2).
    #[test]
    fn the_reply_goes_to_the_live_run_else_resumes_else_starts() {
        assert_eq!(
            reply_target(Some("r3"), Some(("r4", true, false))),
            ReplyTarget::Steer("r3".into())
        );
        assert_eq!(reply_target(None, Some(("r2", true, true))), ReplyTarget::Resume("r2".into()));
        // A teammate's newest run is never resumed by me.
        assert_eq!(reply_target(None, Some(("r2", false, true))), ReplyTarget::Start);
        // Mine but not over (stale, not live): a fresh run.
        assert_eq!(reply_target(None, Some(("r2", true, false))), ReplyTarget::Start);
        assert_eq!(reply_target(None, None), ReplyTarget::Start);
    }

    /// Review T14: never "Thread not found" while the sync has not settled,
    /// an item names the thread, or before the one `threads.get` said so.
    #[test]
    fn thread_not_found_waits_for_the_sync_an_item_and_the_server() {
        assert!(!thread_not_found(false, false, ThreadFetch::Missing));
        assert!(!thread_not_found(true, true, ThreadFetch::Missing));
        assert!(!thread_not_found(true, false, ThreadFetch::NotAsked));
        assert!(!thread_not_found(true, false, ThreadFetch::InFlight));
        assert!(!thread_not_found(true, false, ThreadFetch::Found));
        assert!(thread_not_found(true, false, ThreadFetch::Missing));
    }

    #[test]
    fn the_parallel_meta_names_the_device_and_the_age() {
        let now = crate::run_rows::stamp_ms(Some("2026-10-12T11:02:00.000Z")).unwrap();
        assert_eq!(parallel_meta(Some("mint"), "2026-10-12T10:40:00.000Z", now), "mint · 22 min");
        assert_eq!(parallel_meta(None, "2026-10-12T11:01:30.000Z", now), "now");
        assert_eq!(parallel_meta(Some("macbook"), "garbage", now), "macbook");
    }

    #[test]
    fn the_copy_is_the_brief_s() {
        assert_eq!(CONVERSATION_LABEL, "Conversation");
        assert_eq!(GUIDE_LABEL, "Guide");
        assert_eq!(THREAD_WORD, "thread");
        assert_eq!(LINK_ISSUE_LABEL, "Link an issue");
        assert_eq!(NEW_ISSUE_LABEL, "New issue in this thread");
        assert_eq!(START_RUN_LABEL, "Start a session here");
        assert_eq!(REPLY_PLACEHOLDER, "Reply in this thread");
        assert_eq!(SHOW_WORK_LABEL, "Show work");
        assert_eq!(domain::thread_view::THREAD_LINKED_LABEL, "Linked");
        assert_eq!(domain::thread_view::THREAD_FILED_LABEL, "Filed and linked");
        assert_eq!(THREADS_LABEL, "Threads");
        assert_eq!(stop_session_label("Fix sync", false), "Stop session");
        assert_eq!(stop_session_label("Fix sync", true), "Stop session · Fix sync");
        assert_eq!(in_thread_label("Offline sync plan"), "In Offline sync plan");
    }
}
