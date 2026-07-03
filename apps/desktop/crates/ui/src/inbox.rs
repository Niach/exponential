//! The Inbox screen (masterplan-v3 §4.2 — mirror of
//! `apps/web/src/components/inbox/inbox-view.tsx` +
//! `routes/w/$workspaceSlug/inbox/`).
//!
//! Two tabs on a hand-built **two-button segmented control** (no dedicated
//! widget exists — spec says build it from buttons + selected state), with
//! count suffixes:
//!
//! - **"For me · N"** — notifications **grouped by issue** (the queries live
//!   in [`crate::queries::inbox`]): one card per issue with an unread dot +
//!   left-border accent, up to 3 sub-rows (per-type icon + title) then
//!   "+N more", a relative time, and click-marks-the-whole-group-read
//!   (`notifications.markRead` per unread row) before navigating to the
//!   issue. Header "Mark all read" → `notifications.markAllRead`.
//! - **"Needs your review · N"** — NOT a notification feed: synced `issues`
//!   filtered to `pr_state == 'open'` in the active workspace, rendered as
//!   issue cards with a "PR" badge.
//!
//! `is_ready` gating (§4.1): while the notifications/issues/projects shapes
//! have not seen their first up-to-date, render skeleton cards — never "All
//! caught up" over an in-flight snapshot (EXP-1 #13).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    scroll::ScrollableElement as _,
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Icon, IconName, Sizable as _,
};
use sync::Store;

use domain::rows::Notification;

use crate::icons::ExpIcon;
use crate::navigation::{active_workspace_id, nav_for_window, navigate, Navigation, Screen};
use crate::queries::{self, InboxGroup};

/// Web `InboxTab`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum InboxTab {
    ForMe,
    NeedsReview,
}

pub struct InboxView {
    nav: Entity<Navigation>,
    tab: InboxTab,
    _subscriptions: Vec<Subscription>,
}

impl InboxView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&collections.notifications, |_, _, cx| cx.notify()),
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            cx.observe(&collections.projects, |_, _, cx| cx.notify()),
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&Store::global(cx).state(), |_, _, cx| cx.notify()),
        ];

        Self {
            nav,
            tab: InboxTab::ForMe,
            _subscriptions: subscriptions,
        }
    }

    /// Web `markGroupRead`: fire `markRead` for every unread row (un-gated —
    /// the echo clears the dots), then navigate.
    fn open_group(
        &mut self,
        issue_id: String,
        unread_ids: Vec<String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !unread_ids.is_empty() {
            if let Some(trpc) = queries::trpc_client(cx) {
                cx.background_executor()
                    .spawn(async move {
                        for id in unread_ids {
                            if let Err(err) = api::notifications::notifications_mark_read(&trpc, &id)
                            {
                                log::warn!("[ui] notifications.markRead({id}) failed: {err}");
                            }
                        }
                    })
                    .detach();
            }
        }
        navigate(window, cx, Screen::IssueDetail { issue_id });
    }

    fn mark_all_read(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        cx.background_executor()
            .spawn(async move {
                if let Err(err) = api::notifications::notifications_mark_all_read(&trpc) {
                    log::warn!("[ui] notifications.markAllRead failed: {err}");
                }
            })
            .detach();
    }

    // -- cards -----------------------------------------------------------------

    /// One "For me" card (web: rounded border card, unread dot + primary
    /// left-accent, identifier + title + relative time, ≤3 typed sub-rows).
    fn render_group(&self, group: &InboxGroup, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let unread = group.unread > 0;
        let issue_id = group.issue.id.clone();
        let unread_ids: Vec<String> = group
            .items
            .iter()
            .filter(|n| n.read_at.is_none())
            .map(|n| n.id.clone())
            .collect();
        let newest_time: SharedString = group
            .items
            .first()
            .and_then(|n| n.created_at.as_deref())
            .map(relative_time)
            .unwrap_or_default()
            .into();

        let mut sub_rows = v_flex().mt_1().pl_4().gap_0p5();
        for item in group.items.iter().take(3) {
            sub_rows = sub_rows.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(type_icon(item).xsmall().flex_shrink_0())
                    .child(
                        div()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(
                                item.title.clone().unwrap_or_default(),
                            )),
                    ),
            );
        }
        if group.items.len() > 3 {
            sub_rows = sub_rows.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground.opacity(0.7))
                    .child(SharedString::from(format!("+{} more", group.items.len() - 3))),
            );
        }

        div()
            .id(SharedString::from(format!("inbox-group-{}", group.issue.id)))
            .relative()
            .w_full()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .px_3()
            .py_2()
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().accent.opacity(0.5)))
            .when(unread, |this| {
                // Web `border-l-2 border-l-primary` — gpui borders share one
                // color, so the accent is an absolute 2px bar.
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .rounded_l(cx.theme().radius)
                        .bg(cx.theme().primary),
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_group(issue_id.clone(), unread_ids.clone(), window, cx);
            }))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .when(unread, |this| {
                        this.child(
                            div()
                                .size_2()
                                .flex_shrink_0()
                                .rounded_full()
                                .bg(cx.theme().primary),
                        )
                    })
                    .child(
                        div()
                            .text_xs()
                            .flex_shrink_0()
                            .text_color(cx.theme().muted_foreground)
                            .font_family(theme::terminal::FONT_FAMILY)
                            .child(SharedString::from(group.issue.identifier.clone())),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .min_w_0()
                            .flex_1()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(group.issue.title.clone())),
                    )
                    .child(
                        div()
                            .text_xs()
                            .flex_shrink_0()
                            .text_color(cx.theme().muted_foreground)
                            .child(newest_time),
                    ),
            )
            .child(sub_rows)
    }

    /// One "Needs your review" card: identifier + title + "PR" badge.
    fn render_review_issue(
        &self,
        issue: &domain::rows::Issue,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let issue_id = issue.id.clone();
        div()
            .id(SharedString::from(format!("inbox-review-{}", issue.id)))
            .w_full()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .px_3()
            .py_2()
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().accent.opacity(0.5)))
            .on_click(cx.listener(move |_, _, window, cx| {
                navigate(
                    window,
                    cx,
                    Screen::IssueDetail {
                        issue_id: issue_id.clone(),
                    },
                );
            }))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .flex_shrink_0()
                            .text_color(cx.theme().muted_foreground)
                            .font_family(theme::terminal::FONT_FAMILY)
                            .child(SharedString::from(issue.identifier.clone())),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .min_w_0()
                            .flex_1()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(issue.title.clone())),
                    )
                    .child(
                        // Web `Badge variant="secondary"` with the PR glyph.
                        h_flex()
                            .flex_shrink_0()
                            .gap_1()
                            .items_center()
                            .rounded(cx.theme().radius)
                            .bg(cx.theme().secondary)
                            .px_1p5()
                            .py_0p5()
                            .text_xs()
                            .text_color(cx.theme().secondary_foreground)
                            .child(Icon::from(ExpIcon::GitPullRequest).xsmall())
                            .child("PR"),
                    ),
            )
    }

    fn render_skeleton(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let mut cards = v_flex().gap_2().w_full();
        for _ in 0..3 {
            cards = cards.child(
                v_flex()
                    .w_full()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().border)
                    .px_3()
                    .py_2()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(Skeleton::new().h_3().w_16())
                            .child(Skeleton::new().h_3p5().w_48()),
                    )
                    .child(Skeleton::new().h_3().w_64()),
            );
        }
        cards.into_any_element()
    }
}

impl Render for InboxView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let data = queries::inbox(cx);
        let workspace_id = active_workspace_id(&self.nav, cx);
        let review = workspace_id
            .as_deref()
            .map(|id| queries::review_issues(cx, id))
            .unwrap_or_default();

        // Segmented labels with count suffixes (web: "For me · N").
        let for_me_label: SharedString = if data.total_unread > 0 {
            format!("For me · {}", data.total_unread).into()
        } else {
            "For me".into()
        };
        let review_label: SharedString = if !review.is_empty() {
            format!("Needs your review · {}", review.len()).into()
        } else {
            "Needs your review".into()
        };

        // Header: Bell + "Inbox" + Mark-all-read (only with unreads).
        let header = h_flex()
            .mb_3()
            .items_center()
            .justify_between()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(Icon::new(IconName::Bell).small())
                    .child("Inbox"),
            )
            .when(data.total_unread > 0, |this| {
                this.child(
                    Button::new("inbox-mark-all-read")
                        .ghost()
                        .small()
                        .label("Mark all read")
                        .on_click(cx.listener(|this, _, _, cx| this.mark_all_read(cx))),
                )
            });

        // The two-button segmented control (§4.2 — built from buttons).
        let tabs = h_flex()
            .gap_1()
            .rounded(cx.theme().radius)
            .bg(cx.theme().muted.opacity(0.5))
            .p_0p5()
            .text_sm()
            .child(segment_button(
                "inbox-tab-for-me",
                for_me_label,
                self.tab == InboxTab::ForMe,
                cx.listener(|this, _, _, cx| {
                    this.tab = InboxTab::ForMe;
                    cx.notify();
                }),
                cx,
            ))
            .child(segment_button(
                "inbox-tab-needs-review",
                review_label,
                self.tab == InboxTab::NeedsReview,
                cx.listener(|this, _, _, cx| {
                    this.tab = InboxTab::NeedsReview;
                    cx.notify();
                }),
                cx,
            ));

        let body: gpui::AnyElement = if !data.is_ready {
            self.render_skeleton(cx)
        } else {
            match self.tab {
                InboxTab::ForMe => {
                    if data.groups.is_empty() {
                        empty_state(
                            Icon::from(ExpIcon::CircleCheck),
                            "All caught up",
                            "Assignments, comments and mentions on issues you follow will show up here.",
                            cx,
                        )
                    } else {
                        let mut list = v_flex().gap_2().w_full();
                        for group in &data.groups {
                            list = list.child(self.render_group(group, cx));
                        }
                        list.into_any_element()
                    }
                }
                InboxTab::NeedsReview => {
                    if review.is_empty() {
                        // Web's dashed empty card.
                        div()
                            .w_full()
                            .rounded(cx.theme().radius)
                            .border_1()
                            .border_dashed()
                            .border_color(cx.theme().border)
                            .px_4()
                            .py_10()
                            .text_center()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Nothing waiting on your review.")
                            .into_any_element()
                    } else {
                        let mut list = v_flex().gap_2().w_full();
                        for issue in &review {
                            list = list.child(self.render_review_issue(issue, cx));
                        }
                        list.into_any_element()
                    }
                }
            }
        };

        // Web: mx-auto max-w-3xl px-4 py-4 column on the list surface.
        div()
            .size_full()
            .bg(cx.theme().colors.list)
            .flex()
            .justify_center()
            .child(
                v_flex()
                    .h_full()
                    .w_full()
                    .max_w(px(768.))
                    .px_4()
                    .py_4()
                    .child(header)
                    .child(tabs)
                    .child(
                        v_flex()
                            .id("inbox-scroll")
                            .mt_3()
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .child(body),
                    ),
            )
    }
}

/// One half of the segmented control (web `TabButton`: flex-1, active =
/// background + shadow, inactive = muted → foreground on hover).
fn segment_button(
    id: &'static str,
    label: SharedString,
    active: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    div()
        .id(id)
        .flex_1()
        .rounded(cx.theme().radius)
        .px_3()
        .py_1()
        .text_center()
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .when(active, |this| {
            this.bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .shadow_sm()
        })
        .when(!active, |this| {
            this.text_color(cx.theme().muted_foreground)
                .hover(|style| style.text_color(cx.theme().foreground))
        })
        .on_click(on_click)
        .child(label)
}

/// Web `typeIcon` map (`inbox-view.tsx`): per-`notification_type` glyphs,
/// `Bell` fallback.
fn type_icon(notification: &Notification) -> Icon {
    match notification.kind.as_deref() {
        Some("issue_assigned") => Icon::from(ExpIcon::UserPlus),
        Some("issue_comment") | Some("issue_mention") => Icon::from(ExpIcon::MessageSquare),
        Some("issue_status_changed") => Icon::from(ExpIcon::CircleDot),
        Some("pr_opened") => Icon::from(ExpIcon::GitPullRequest),
        Some("pr_merged") => Icon::from(ExpIcon::GitMerge),
        _ => Icon::new(IconName::Bell),
    }
}

/// Web `relativeTime`: "just now" / "Nm" / "Nh" / "Nd" (rounded like JS).
fn relative_time(created_at: &str) -> String {
    let Some(then) = parse_epoch_seconds(created_at) else {
        return String::new();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    relative_time_between(now, then)
}

fn relative_time_between(now: i64, then: i64) -> String {
    let diff = (now - then).max(0);
    let mins = (diff as f64 / 60.).round() as i64;
    if mins < 1 {
        return "just now".to_string();
    }
    if mins < 60 {
        return format!("{mins}m");
    }
    let hours = (mins as f64 / 60.).round() as i64;
    if hours < 24 {
        return format!("{hours}h");
    }
    format!("{}d", (hours as f64 / 24.).round() as i64)
}

/// Tolerant ISO-8601 → epoch seconds. Electric forwards Postgres `timestamptz`
/// text (`2026-07-03 10:11:12.345+00` — space separator, short offset), tRPC
/// echoes RFC 3339; accept both.
fn parse_epoch_seconds(value: &str) -> Option<i64> {
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(parsed.timestamp());
    }
    for format in [
        "%Y-%m-%d %H:%M:%S%.f%#z",
        "%Y-%m-%dT%H:%M:%S%.f%#z",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
    ] {
        if let Ok(parsed) = chrono::DateTime::parse_from_str(value, format) {
            return Some(parsed.timestamp());
        }
        if let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(value, format) {
            return Some(parsed.and_utc().timestamp());
        }
    }
    None
}

/// Web `EmptyState` (shared look with the board's).
fn empty_state(
    icon: Icon,
    title: &'static str,
    description: &'static str,
    cx: &App,
) -> gpui::AnyElement {
    v_flex()
        .w_full()
        .py_10()
        .items_center()
        .justify_center()
        .gap_2()
        .child(icon.size_6().text_color(cx.theme().muted_foreground))
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .text_center()
                .child(description),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_time_rounds_like_the_web() {
        let now = 1_800_000_000_i64;
        assert_eq!(relative_time_between(now, now - 20), "just now");
        assert_eq!(relative_time_between(now, now - 120), "2m");
        assert_eq!(relative_time_between(now, now - 3 * 3600), "3h");
        assert_eq!(relative_time_between(now, now - 26 * 3600), "1d");
        assert_eq!(relative_time_between(now, now - 50 * 3600), "2d");
        // Future timestamps clamp to "just now" instead of going negative.
        assert_eq!(relative_time_between(now, now + 3600), "just now");
    }

    #[test]
    fn epoch_parser_accepts_postgres_and_rfc3339_forms() {
        // RFC 3339 (tRPC / ISO).
        assert!(parse_epoch_seconds("2026-07-03T10:11:12.345+00:00").is_some());
        assert!(parse_epoch_seconds("2026-07-03T10:11:12Z").is_some());
        // Postgres text form Electric forwards (space + short offset).
        assert!(parse_epoch_seconds("2026-07-03 10:11:12.345+00").is_some());
        assert!(parse_epoch_seconds("2026-07-03 10:11:12+00").is_some());
        // Naive fallback.
        assert!(parse_epoch_seconds("2026-07-03 10:11:12").is_some());
        assert!(parse_epoch_seconds("garbage").is_none());

        // The two zoned forms agree on the instant.
        assert_eq!(
            parse_epoch_seconds("2026-07-03T10:11:12+00:00"),
            parse_epoch_seconds("2026-07-03 10:11:12+00"),
        );
    }
}
