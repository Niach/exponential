//! Comment rows + composer for the issue-detail timeline (masterplan-v3 §4.2;
//! web parity target: `apps/web/src/components/comment-rows/regular.tsx` +
//! `comment-rows/format.ts` and the composer form at the bottom of
//! `issue-timeline.tsx`).
//!
//! The thread itself (merging comments with `issue_events`, ordering,
//! submit/edit/delete state) lives in [`crate::timeline::IssueTimeline`] —
//! mirroring the web file split where `issue-timeline.tsx` owns state and
//! `comment-rows/` owns row presentation. This module renders one comment row
//! (Avatar + name + relative time + author-or-admin `…` menu + body) and the
//! mention-capable composer strip (a lightweight multi-line input, **not**
//! the §4.5 block editor — comments have no toolbar on web; Cmd/Ctrl+Enter
//! submits). The §4.6 caret-anchored `@`-autocomplete layers onto the same
//! `InputState` when the markdown-editor track lands it.

use gpui::{div, Entity, FontWeight, IntoElement, ParentElement, SharedString, Styled};
use gpui_component::{
    avatar::Avatar,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
};

use domain::rows::{Comment, User};

use crate::description_editor::open_issue_by_identifier;
use crate::icons::ExpIcon;
use crate::markdown::{ImageCache, MarkdownView, RefResolver};
use crate::mention_input::MentionInput;
use crate::timeline::IssueTimeline;

// ---------------------------------------------------------------------------
// format.ts mirrors
// ---------------------------------------------------------------------------

/// Web `authorLabel` (`comment-rows/format.ts`): `name || email || "Someone"`,
/// except agents label as `name || "Agent"` (no email fallback — synthetic
/// widget/agent users have no meaningful address).
pub(crate) fn author_label(author: Option<&User>) -> String {
    let Some(user) = author else {
        return "Someone".to_string();
    };
    let name = user.name.clone().filter(|name| !name.is_empty());
    if user.is_agent == Some(true) {
        return name.unwrap_or_else(|| "Agent".to_string());
    }
    name.or_else(|| user.email.clone().filter(|email| !email.is_empty()))
        .unwrap_or_else(|| "Someone".to_string())
}

/// Tolerant ISO-8601 → unix seconds. Accepts the two forms Electric/tRPC
/// emit for Postgres timestamptz (`2026-07-03T10:00:00Z`-style RFC 3339 and
/// the `2026-07-03 10:00:00+00` space form) plus bare dates.
pub(crate) fn parse_epoch(timestamp: &str) -> Option<i64> {
    let trimmed = timestamp.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Some(parsed.timestamp());
    }
    // `2026-07-03 10:00:00+00` / `...+00:00` (Postgres text form).
    let t_form = trimmed.replacen(' ', "T", 1);
    for candidate in [t_form.clone(), format!("{t_form}:00"), format!("{t_form}Z")] {
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&candidate) {
            return Some(parsed.timestamp());
        }
    }
    // Bare `YYYY-MM-DD` (date columns).
    if let Ok(date) = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return Some(date.and_hms_opt(0, 0, 0)?.and_utc().timestamp());
    }
    None
}

/// Web `relativeTime` (date-fns `formatDistanceToNowStrict` + suffix):
/// `"3 minutes ago"`, `"2 hours ago"`, `"in 5 days"`. Empty on unparseable
/// input, exactly like the web helper.
pub(crate) fn relative_time(timestamp: &str, now_epoch: i64) -> String {
    let Some(then) = parse_epoch(timestamp) else {
        return String::new();
    };
    let delta = now_epoch - then;
    let (magnitude, future) = if delta >= 0 {
        (delta, false)
    } else {
        (-delta, true)
    };
    let (count, unit) = if magnitude < 60 {
        (magnitude.max(0), "second")
    } else if magnitude < 3_600 {
        (magnitude / 60, "minute")
    } else if magnitude < 86_400 {
        (magnitude / 3_600, "hour")
    } else if magnitude < 30 * 86_400 {
        (magnitude / 86_400, "day")
    } else if magnitude < 365 * 86_400 {
        (magnitude / (30 * 86_400), "month")
    } else {
        (magnitude / (365 * 86_400), "year")
    };
    let noun = if count == 1 {
        unit.to_string()
    } else {
        format!("{unit}s")
    };
    if future {
        format!("in {count} {noun}")
    } else {
        format!("{count} {noun} ago")
    }
}

// ---------------------------------------------------------------------------
// One comment row (comment-rows/regular.tsx)
// ---------------------------------------------------------------------------

/// Everything one comment row needs from the thread's state.
pub(crate) struct CommentRowProps<'a> {
    pub comment: &'a Comment,
    pub author: Option<&'a User>,
    /// Author-or-admin gate (web `canModify`) — shows the `…` Edit/Delete menu.
    pub can_modify: bool,
    /// `Some(editor)` while THIS comment is being edited (web `editing`) —
    /// the §4.6 mention-capable input.
    pub editing: Option<&'a Entity<MentionInput>>,
    pub saving: bool,
    pub now_epoch: i64,
    /// Scopes the body's `@email`/`#IDENT` pill resolution (§4.5).
    pub workspace_id: Option<&'a str>,
    /// Shared attachment-image cache (auth-gated fetch).
    pub images: &'a Entity<ImageCache>,
}

/// Web `RegularCommentRow`: avatar · (name · relative time · [`…` menu]) over
/// the rendered GFM body, or the edit textarea + Save/Cancel when editing.
pub(crate) fn comment_row(
    props: CommentRowProps<'_>,
    cx: &mut gpui::Context<IssueTimeline>,
) -> impl IntoElement {
    let name = author_label(props.author);
    let comment_id = props.comment.id.clone();
    let created = props.comment.created_at.as_deref().unwrap_or("");
    let mut meta = relative_time(created, props.now_epoch);
    if props.comment.edited_at.is_some() {
        meta.push_str(" · edited");
    }

    let header = h_flex()
        .gap_2()
        .items_baseline()
        .text_xs()
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .child(SharedString::from(name.clone())),
        )
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(meta)),
        )
        .when(props.can_modify && props.editing.is_none(), |row| {
            let edit_id = comment_id.clone();
            let delete_id = comment_id.clone();
            row.child(div().flex_1()).child(
                Button::new(SharedString::from(format!("comment-menu-{comment_id}")))
                    .ghost()
                    .xsmall()
                    .icon(
                        Icon::new(IconName::Ellipsis)
                            .text_color(cx.theme().muted_foreground),
                    )
                    .dropdown_menu({
                        let timeline = cx.entity();
                        move |menu, _, _| {
                            let timeline_edit = timeline.clone();
                            let timeline_delete = timeline.clone();
                            let edit_id = edit_id.clone();
                            let delete_id = delete_id.clone();
                            menu.item(PopupMenuItem::new("Edit").on_click(move |_, window, cx| {
                                timeline_edit.update(cx, |timeline, cx| {
                                    timeline.begin_edit(&edit_id, window, cx);
                                });
                            }))
                            .item(
                                PopupMenuItem::new("Delete").on_click(move |_, _, cx| {
                                    timeline_delete.update(cx, |timeline, cx| {
                                        timeline.delete_comment(&delete_id, cx);
                                    });
                                }),
                            )
                        }
                    }),
            )
        });

    let body: gpui::AnyElement = match props.editing {
        Some(editor) => v_flex()
            .mt_1()
            .gap_2()
            .child(editor.clone())
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new(SharedString::from(format!("comment-save-{comment_id}")))
                            .primary()
                            .xsmall()
                            .label("Save")
                            .loading(props.saving)
                            .on_click(cx.listener({
                                let comment_id = comment_id.clone();
                                move |this, _, window, cx| {
                                    this.save_edit(&comment_id, window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("comment-cancel-{comment_id}")))
                            .ghost()
                            .xsmall()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_edit(cx))),
                    ),
            )
            .into_any_element(),
        None => {
            // Read-only rendered GFM with live `@email`/`#IDENT` pills
            // (§4.5 — same decoration pass as the description).
            let source = props.comment.body.clone().unwrap_or_default();
            let mut view = MarkdownView::new(
                SharedString::from(format!("comment-body-{comment_id}")),
                source,
            )
            .images(props.images.clone());
            if let Some(workspace_id) = props.workspace_id {
                let workspace = workspace_id.to_string();
                view = view
                    .resolver(RefResolver::from_store(workspace_id))
                    .on_open_issue(move |identifier, window, cx| {
                        open_issue_by_identifier(&workspace, identifier, window, cx);
                    });
            }
            div().mt_0p5().text_sm().child(view).into_any_element()
        }
    };

    h_flex()
        .py_2()
        .gap_2p5()
        .items_start()
        .child(Avatar::new().name(SharedString::from(name)).small())
        .child(v_flex().flex_1().min_w_0().child(header).child(body))
}

// ---------------------------------------------------------------------------
// Composer (issue-timeline.tsx bottom form)
// ---------------------------------------------------------------------------

/// The composer strip: auto-growing mention-capable input + Send button.
/// State (the `InputState` entity, the submitting flag, the PressEnter
/// subscription) lives on [`IssueTimeline`]; this only lays out the row.
pub(crate) fn composer_row(
    input: &Entity<MentionInput>,
    submitting: bool,
    has_draft: bool,
    cx: &mut gpui::Context<IssueTimeline>,
) -> impl IntoElement {
    h_flex()
        .mt_2()
        .gap_2()
        .items_end()
        .child(div().flex_1().min_w_0().child(input.clone()))
        .child(
            Button::new("comment-submit")
                .primary()
                .small()
                .icon(Icon::from(ExpIcon::Send))
                .loading(submitting)
                .disabled(submitting || !has_draft)
                .on_click(cx.listener(|this, _, window, cx| this.submit_comment(window, cx))),
        )
}

use gpui::prelude::FluentBuilder as _;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn author_label_prefers_name_then_email_then_someone() {
        let user: User = serde_json::from_value(json!({
            "id": "u-1", "name": "Ada Lovelace", "email": "ada@example.com"
        }))
        .unwrap();
        assert_eq!(author_label(Some(&user)), "Ada Lovelace");

        let email_only: User = serde_json::from_value(json!({
            "id": "u-2", "name": "", "email": "no-name@example.com"
        }))
        .unwrap();
        assert_eq!(author_label(Some(&email_only)), "no-name@example.com");

        // Agents: name || "Agent", never the email (web authorLabel isAgent arm).
        let agent: User = serde_json::from_value(json!({
            "id": "u-3", "email": "bot@internal", "is_agent": true
        }))
        .unwrap();
        assert_eq!(author_label(Some(&agent)), "Agent");

        assert_eq!(author_label(None), "Someone");
    }

    #[test]
    fn parse_epoch_accepts_both_postgres_text_forms() {
        // RFC 3339.
        assert_eq!(parse_epoch("1970-01-01T00:00:00Z"), Some(0));
        // Space-separated with short offset (Electric's Postgres text form).
        assert_eq!(parse_epoch("1970-01-01 00:00:00+00"), Some(0));
        assert_eq!(parse_epoch("1970-01-01 01:00:00+01:00"), Some(0));
        // Bare date.
        assert_eq!(parse_epoch("1970-01-02"), Some(86_400));
        // Garbage → None, never a panic.
        assert_eq!(parse_epoch("not a date"), None);
        assert_eq!(parse_epoch(""), None);
    }

    #[test]
    fn relative_time_matches_strict_distance_phrasing() {
        let now = 1_000_000_i64;
        assert_eq!(relative_time("not a date", now), "");
        // 90 seconds ago → "1 minute ago" (strict truncation, like date-fns).
        let t = chrono::DateTime::from_timestamp(now - 90, 0)
            .unwrap()
            .to_rfc3339();
        assert_eq!(relative_time(&t, now), "1 minute ago");
        let t = chrono::DateTime::from_timestamp(now - 2 * 3600, 0)
            .unwrap()
            .to_rfc3339();
        assert_eq!(relative_time(&t, now), "2 hours ago");
        let t = chrono::DateTime::from_timestamp(now + 5 * 86_400, 0)
            .unwrap()
            .to_rfc3339();
        assert_eq!(relative_time(&t, now), "in 5 days");
    }
}
