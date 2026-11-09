//! #FEED-191 THE WIDE INBOX (web `components/team/inbox-popover.tsx` +
//! `InboxView compact`): an icon in the rail's header row, the unread dot on its
//! corner, opening a menu-surface popover anchored under it: the `Inbox` caption
//! with the mark-all-read glyph, then the inbox's groups in the compact density
//! (the bare type glyph, identifier, title, time, the unread dot; a read row
//! dims), scrolling past the max height. A row marks read, opens by its target
//! (inbox-route.json) and closes the popover.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, rems, AnyElement, App, ClickEvent, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement,
    Render, RenderOnce, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    popover::Popover,
    v_flex, ActiveTheme as _, Icon, Selectable as _, Sizable as _,
};
use sync::Store;

use crate::controls::WebControl as _;
use crate::icons::{registry, ExpIcon};
use crate::navigation::{nav_for_window, navigate, Navigation, Screen};
use crate::queries;

/// The popover's width and rows' max height (web `w-[24rem]`, `max-h-[24rem]`).
const WIDTH_REMS: f32 = 24.;
const MAX_ROWS_REMS: f32 = 24.;

/// A read row's dim (web `opacity-60`).
const READ_OPACITY: f32 = 0.6;

/// Open the window's Inbox popover (the `OpenInbox` action, an OS notification,
/// an `~/inbox` link).
pub(crate) fn open(window: &mut Window, cx: &mut App) {
    set_open(true, window, cx);
}

fn set_open(open: bool, window: &mut Window, cx: &mut App) {
    let nav = nav_for_window(window, cx);
    nav.update(cx, |nav, cx| {
        if nav.inbox_open != open {
            nav.inbox_open = open;
            cx.notify();
        }
    });
    window.refresh();
}

/// Fire-and-forget `notifications.markRead` over a group's unread rows (the
/// web `markGroupRead`): the Electric echo clears the dots.
fn mark_read(unread_ids: &[String], cx: &mut App) {
    if unread_ids.is_empty() {
        return;
    }
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    let ids = unread_ids.to_vec();
    cx.background_executor()
        .spawn(async move {
            for id in ids {
                if let Err(err) = api::notifications::notifications_mark_read(&trpc, &id) {
                    log::warn!("[ui] notifications.markRead({id}) failed: {err}");
                }
            }
        })
        .detach();
}

/// The header's mark-all-read: every unread row of the viewer; the cleared
/// `read_at`s stream back over the notifications shape. A failure is the
/// failed-action toast with Retry (the call is idempotent).
fn mark_all_read(cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        crate::action_error::failed_action(MARK_ALL_VERB, MARK_ALL_THING, None, cx);
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::notifications::notifications_mark_all_read(&trpc) })
            .await;
        if let Err(err) = result {
            log::warn!("[ui] notifications.markAllRead failed: {err}");
            let description = crate::action_error::action_error_description(&err);
            let _ = cx.update(|cx| {
                crate::action_error::failed_action_with_retry(
                    MARK_ALL_VERB,
                    MARK_ALL_THING,
                    description,
                    |_, cx| mark_all_read(cx),
                    cx,
                )
            });
        }
    })
    .detach();
}

/// `Couldn't mark all read` (feedback-states.json `errors.titlePattern`).
const MARK_ALL_VERB: &str = "mark";
const MARK_ALL_THING: &str = "all read";

/// Latest-notification kind → the row's leading type glyph (the meaning table
/// shared across all clients).
fn type_icon(kind: Option<&str>) -> Icon {
    match kind {
        Some(domain::contract::NOTIFICATION_TYPE_ISSUE_ASSIGNED) => Icon::from(ExpIcon::UserPlus),
        Some(domain::contract::NOTIFICATION_TYPE_ISSUE_COMMENT) => Icon::from(ExpIcon::MessageSquare),
        Some(domain::contract::NOTIFICATION_TYPE_ISSUE_MENTION) => Icon::new(registry::NOTIFICATION_ISSUE_MENTION),
        Some(domain::contract::NOTIFICATION_TYPE_ISSUE_STATUS_CHANGED) => Icon::new(registry::NOTIFICATION_ISSUE_STATUS_CHANGED),
        Some(domain::contract::NOTIFICATION_TYPE_PR_OPENED) => Icon::from(ExpIcon::GitPullRequest),
        Some(domain::contract::NOTIFICATION_TYPE_PR_MERGED) => Icon::from(ExpIcon::GitMerge),
        // SLOP-4: a widget reporter answered on an issue.
        Some(domain::contract::NOTIFICATION_TYPE_REPORTER_REPLY) => Icon::new(registry::NOTIFICATION_REPORTER_REPLY),
        // EXP-801: an agent's message.
        Some(domain::contract::NOTIFICATION_TYPE_AGENT_MESSAGE) => Icon::new(registry::NOTIFICATION_AGENT_MESSAGE),
        // EXP-980: a run that hit a rate limit.
        Some(domain::contract::NOTIFICATION_TYPE_SESSION_BLOCKED) => Icon::new(registry::NOTIFICATION_SESSION_BLOCKED),
        // #EXP-1534: one of the viewer's devices lost an agent login.
        Some(domain::contract::NOTIFICATION_TYPE_AGENT_SIGNED_OUT) => Icon::new(registry::NOTIFICATION_AGENT_SIGNED_OUT),
        _ => Icon::new(registry::NAV_NOTIFICATIONS),
    }
}

/// The compact row: glyph · identifier · title (medium while unread) · time ·
/// the unread dot (its slot kept while read, so the times line up).
struct RowParts {
    id: SharedString,
    icon: Icon,
    identifier: Option<String>,
    title: String,
    time: String,
    unread: bool,
}

fn compact_row(parts: RowParts, cx: &App) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    crate::surface::flat_row_compact()
        .id(parts.id)
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .items_center()
        .cursor_pointer()
        .hover(|style| style.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
        .when(!parts.unread, |row| row.opacity(READ_OPACITY))
        .child(
            h_flex()
                .size_4()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .child(parts.icon.xsmall().text_color(muted)),
        )
        .children(parts.identifier.map(|identifier| {
            div().flex_shrink_0().text_xs().text_color(muted).child(SharedString::from(identifier))
        }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .when(parts.unread, |text| text.font_weight(FontWeight::MEDIUM))
                .text_color(theme.foreground)
                .child(SharedString::from(parts.title)),
        )
        .child(div().flex_shrink_0().text_xs().text_color(muted).child(SharedString::from(parts.time)))
        .child(div().size_2().flex_shrink_0().rounded_full().when(parts.unread, |dot| dot.bg(theme.primary)))
}

fn time_of(created_at: Option<&str>) -> String {
    created_at.map(crate::inbox::relative_time).unwrap_or_default()
}

/// The popover's body: its own view, so a sync moves the rows while it is up.
pub(crate) struct InboxPopoverBody {
    scroll: ScrollHandle,
    data: queries::Memo<queries::InboxDataKey, queries::InboxData>,
    _subscriptions: Vec<Subscription>,
}

impl InboxPopoverBody {
    fn new(cx: &mut gpui::Context<Self>) -> Self {
        let collections = Store::global(cx).collections().clone();
        Self {
            scroll: ScrollHandle::new(),
            data: queries::Memo::default(),
            _subscriptions: vec![
                cx.observe(&collections.notifications, |_, _, cx| cx.notify()),
                cx.observe(&collections.issues, |_, _, cx| cx.notify()),
                cx.observe(&collections.boards, |_, _, cx| cx.notify()),
                cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            ],
        }
    }

    /// One issue group: the latest notification's type glyph; a press marks the
    /// group read and opens the issue.
    fn issue_row(group: &queries::InboxGroup, cx: &App) -> AnyElement {
        let unread_ids: Vec<String> = group.items.iter().filter(|item| item.read_at.is_none()).map(|item| item.id.clone()).collect();
        // Items are newest first.
        let latest = group.items.first();
        let issue_id = group.issue.id.clone();
        compact_row(
            RowParts {
                id: SharedString::from(format!("inbox-row-{}", group.issue.id)),
                icon: type_icon(latest.and_then(|item| item.kind.as_deref())),
                identifier: Some(group.issue.identifier.clone()),
                title: group.issue.title.clone(),
                time: time_of(latest.and_then(|item| item.created_at.as_deref())),
                unread: group.unread > 0,
            },
            cx,
        )
        .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            mark_read(&unread_ids, cx);
            set_open(false, window, cx);
            navigate(window, cx, Screen::IssueDetail { issue_id: issue_id.clone() });
        })
        .into_any_element()
    }

    /// One agent message (EXP-801): a press marks it read and, #EXP-1456
    /// (inbox-route.json), opens the session it names (#EXP-1662: a session
    /// waiting on its own plan opens its chat's plan tab); a broadcast only
    /// marks read.
    fn message_row(entry: &queries::MessageInboxEntry, cx: &App) -> AnyElement {
        let unread = entry.unread() > 0;
        let unread_ids: Vec<String> = if unread { vec![entry.item.id.clone()] } else { Vec::new() };
        let session_id = entry.session_id();
        compact_row(
            RowParts {
                id: SharedString::from(format!("inbox-message-{}", entry.item.id)),
                icon: type_icon(Some(domain::contract::NOTIFICATION_TYPE_AGENT_MESSAGE)),
                identifier: None,
                title: entry.item.title.clone().unwrap_or_default(),
                time: time_of(entry.item.created_at.as_deref()),
                unread,
            },
            cx,
        )
        .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            mark_read(&unread_ids, cx);
            set_open(false, window, cx);
            if let Some(id) = &session_id {
                let screen = crate::plan_page::screen_for_session(id, cx);
                crate::plan_page::prime_session(id, cx);
                navigate(window, cx, screen);
            }
        })
        .into_any_element()
    }

    /// One blocked session (EXP-980): a press marks it read and opens the
    /// session (a pruned one only marks read). #EXP-1534: a signed-out login
    /// opens its device's settings.
    fn session_row(entry: &queries::SessionInboxEntry, cx: &App) -> AnyElement {
        let unread = entry.unread() > 0;
        let unread_ids: Vec<String> = if unread { vec![entry.item.id.clone()] } else { Vec::new() };
        let device_id = match entry.target() {
            domain::inbox_route::InboxRowTarget::Device { device_id } => Some(device_id),
            _ => None,
        };
        let session_id = entry.session_id();
        compact_row(
            RowParts {
                id: SharedString::from(format!("inbox-session-{}", entry.item.id)),
                icon: type_icon(entry.item.kind.as_deref()),
                identifier: None,
                title: entry.item.title.clone().unwrap_or_default(),
                time: time_of(entry.item.created_at.as_deref()),
                unread,
            },
            cx,
        )
        .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            mark_read(&unread_ids, cx);
            set_open(false, window, cx);
            if let Some(device_id) = device_id.clone() {
                navigate(window, cx, Screen::Devices);
                crate::device_settings::open(window, cx, device_id);
                return;
            }
            // EXP-1250: a cross-team row opens WITHOUT switching the team.
            if let Some(id) = &session_id {
                let screen = crate::plan_page::screen_for_session(id, cx);
                crate::plan_page::prime_session(id, cx);
                navigate(window, cx, screen);
            }
        })
        .into_any_element()
    }
}

impl Render for InboxPopoverBody {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "InboxPopoverBody").entered();
        let data = {
            let app: &App = cx;
            self.data.get_or_insert_with(queries::inbox_data_key(app), || queries::inbox(app))
        };
        let copy = domain::chats::CHATS.copy.inbox.clone();
        let muted = cx.theme().muted_foreground;
        let mark_all = (data.total_unread > 0).then(|| {
            crate::controls::ghost_icon_button("inbox-mark-all-read", Icon::from(registry::NOTIFICATION_MARK_READ), cx)
                .tooltip("Mark all read")
                .on_click(|_: &ClickEvent, _, cx: &mut App| mark_all_read(cx))
        });
        let body: AnyElement = if !data.is_ready {
            crate::feedback::skeleton_rows("inbox-skeleton", crate::feedback::SKELETON_ROWS, eui_gpui::controls::FLAT_ROW_COMPACT_H, cx).into_any_element()
        } else if data.groups.is_empty() {
            crate::controls::empty_state_list(Icon::new(registry::NAV_INBOX), crate::empty_state_copy::INBOX, cx).into_any_element()
        } else {
            let rows: Vec<AnyElement> = data
                .groups
                .iter()
                .map(|entry| match entry {
                    queries::InboxEntry::Issue(group) => Self::issue_row(group, cx),
                    queries::InboxEntry::Message(entry) => Self::message_row(entry, cx),
                    queries::InboxEntry::Session(entry) => Self::session_row(entry, cx),
                })
                .collect();
            div()
                .id("inbox-popover-rows")
                .relative()
                .w_full()
                .max_h(rems(MAX_ROWS_REMS))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .child(v_flex().w_full().gap_0p5().children(rows))
                .into_any_element()
        };
        v_flex()
            .id("inbox-popover")
            .w_full()
            .min_w_0()
            .child(
                h_flex()
                    .h_8()
                    .w_full()
                    .px_2()
                    .items_center()
                    .justify_between()
                    .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(muted).child(SharedString::from(copy)))
                    .children(mark_all),
            )
            .child(body)
    }
}

/// THE header icon + its popover.
#[derive(IntoElement)]
pub(crate) struct InboxButton {
    nav: Entity<Navigation>,
}

impl InboxButton {
    pub(crate) fn new(nav: Entity<Navigation>) -> Self {
        Self { nav }
    }
}

impl RenderOnce for InboxButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let open = self.nav.read(cx).inbox_open;
        let unread = queries::inbox_unread(cx);
        let body = window.use_keyed_state("inbox-popover-body", cx, |_, cx| InboxPopoverBody::new(cx));
        let trigger = Button::new("nav-inbox")
            .ghost()
            .web_icon_sm()
            .flex_shrink_0()
            .selected(open)
            .icon(registry::NAV_INBOX)
            .tooltip(domain::chats::CHATS.copy.inbox.clone());
        let nav = self.nav.clone();
        div()
            .relative()
            .flex_shrink_0()
            .child(
                Popover::new("inbox-popover-surface")
                    .open(open)
                    .anchor(gpui::Anchor::TopLeft)
                    .w(rems(WIDTH_REMS))
                    .p_1()
                    .trigger(trigger)
                    .on_open_change(move |open, window, cx| {
                        let open = *open;
                        nav.update(cx, |nav, cx| {
                            nav.inbox_open = open;
                            cx.notify();
                        });
                        window.refresh();
                    })
                    .content(move |_, _, _| body.clone()),
            )
            .when(unread, |this| {
                this.child(
                    crate::sidebar::rail_dot(theme::tokens::PRIMARY.to_hsla())
                        .id("nav-inbox-dot")
                        .absolute()
                        .top(px(4.))
                        .right(px(4.)),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row without a stamp reads no time.
    #[test]
    fn a_row_without_a_stamp_has_no_time() {
        assert_eq!(time_of(None), "");
        assert!(!time_of(Some("2026-11-05T00:00:00.000Z")).is_empty());
    }
}
