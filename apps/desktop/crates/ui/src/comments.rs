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
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Icon,
};

use domain::rows::{Comment, User};

use crate::comment_attachments::{
    attach_button, comment_attachments_strip, pending_attachments_strip, MAX_COMMENT_ATTACHMENTS,
};
use crate::emoji_picker::{emoji_picker_popover, EmojiPicker};
use crate::controls::WebControl as _;
use crate::description_editor::open_issue_by_identifier;
use crate::icons::registry;
use crate::markdown::{ImageCache, MarkdownView, RefResolver};
use crate::mention_input::MentionInput;
use crate::timeline::{IssueTimeline, PendingCommentAttachment, PendingScope};

// ---------------------------------------------------------------------------
// format.ts mirrors
// ---------------------------------------------------------------------------

/// Web `authorLabel` (`comment-rows/format.ts`): `name || email || "Someone"`.
pub(crate) fn author_label(author: Option<&User>) -> String {
    let Some(user) = author else {
        return "Someone".to_string();
    };
    let name = user.name.clone().filter(|name| !name.is_empty());
    name.or_else(|| user.email.clone().filter(|email| !email.is_empty()))
        .unwrap_or_else(|| "Someone".to_string())
}

/// Display label for a resolved `user_id` whose row may be missing: the row's
/// [`author_label`] when present, else the `Member <LAST4>` fallback (the
/// server no longer syncs user rows for public-team co-members, so an id
/// referenced by an issue/comment/event can resolve to no row).
pub(crate) fn user_label(user_id: &str, user: Option<&User>) -> String {
    match user {
        Some(user) => author_label(Some(user)),
        None => domain::member_fallback_label(user_id),
    }
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
    relative_time_epoch(then, now_epoch)
}

/// [`relative_time`]'s core, for callers that already hold epoch seconds
/// (EXP-662: the run registry's `recorded_at`) instead of a wire timestamp.
pub(crate) fn relative_time_epoch(then: i64, now_epoch: i64) -> String {
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
    /// EXP-554, edit mode only: linked attachments staged for removal (hidden
    /// from the strip, dropped from the id set the save sends).
    pub edit_removed: Option<&'a std::collections::HashSet<String>>,
    /// EXP-554, edit mode only: attachments picked during this edit.
    pub edit_pending: &'a [PendingCommentAttachment],
    pub now_epoch: i64,
    /// EXP-698 round 5: whether the timeline rail continues above/below this
    /// row (false on the feed's first/last row).
    pub line_above: bool,
    pub line_below: bool,
    /// Scopes the body's `@email`/`#IDENT` pill resolution (§4.5).
    pub team_id: Option<&'a str>,
    /// Shared attachment-image cache (auth-gated fetch).
    pub images: &'a Entity<ImageCache>,
    /// EXP-551: the timeline's shared picker + whether the EDIT composer's
    /// popover is open. Passed in from `&self` — see [`emoji_button`].
    pub emoji_picker: &'a Entity<EmojiPicker>,
    pub emoji_open: bool,
}

/// Web `RegularCommentRow`: avatar · (name · relative time · [`…` menu]) over
/// the rendered GFM body, or the edit textarea + Save/Cancel when editing.
pub(crate) fn comment_row(
    props: CommentRowProps<'_>,
    cx: &mut gpui::Context<IssueTimeline>,
) -> impl IntoElement {
    let name = match props.comment.author_id.as_deref() {
        Some(id) => user_label(id, props.author),
        None => author_label(props.author),
    };
    let comment_id = props.comment.id.clone();
    let created = props.comment.created_at.as_deref().unwrap_or("");
    let mut meta = relative_time(created, props.now_epoch);
    if props.comment.edited_at.is_some() {
        meta.push_str(" · edited");
    }

    let header = h_flex()
        .w_full()
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
                // EXP-698 round 5: BARE ⋮ on every client — the bubble is
                // already a card, so a glass ring around its menu glyph reads
                // as a second surface. Gate stays author-only.
                Button::new(SharedString::from(format!("comment-menu-{comment_id}")))
                    .ghost()
                    .web_icon_xs()
                    .icon(Icon::new(registry::UI_MORE_VERTICAL))
                    .dropdown_menu({
                        let timeline = cx.entity();
                        move |menu, _, cx| {
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
                                crate::controls::danger_menu_item(
                                    "Delete",
                                    Icon::new(registry::UI_DELETE),
                                    cx,
                                )
                                .on_click(move |_, _, cx| {
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
        // EXP-698 round 5: the row's own bubble IS the card now — the edit
        // strip keeps its rhythm but drops the nested fill/stroke it carried
        // since EXP-568 (a card inside a card composites to a third material).
        Some(editor) => v_flex()
            .mt_1()
            .gap_1p5()
            .child(editor.clone())
            // EXP-554: kept attachments (with the staging ✕) + this edit's
            // picks, above the Save/Cancel row.
            .children(comment_attachments_strip(
                &comment_id,
                props.images,
                props.edit_removed,
                cx,
            ))
            .children(pending_attachments_strip(
                props.edit_pending,
                PendingScope::Edit,
                cx,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(emoji_button(
                        SharedString::from(format!("comment-edit-emoji-{comment_id}")),
                        PendingScope::Edit,
                        props.emoji_picker,
                        props.emoji_open,
                        cx,
                    ))
                    .child(attach_button(
                        SharedString::from(format!("comment-edit-attach-{comment_id}")),
                        PendingScope::Edit,
                        edit_attachments_full(&props, cx),
                        cx,
                    ))
                    .child(
                        Button::new(SharedString::from(format!("comment-save-{comment_id}")))
                            .primary()
                            .web_xs()
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
                        crate::surface::glass_pill_button(SharedString::from(format!("comment-cancel-{comment_id}")), crate::surface::PillSize::Sm, cx)
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_edit(cx))),
                    ),
            )
            .into_any_element(),
        None => {
            // Read-only rendered GFM with live `@email`/`#IDENT` pills
            // (§4.5 — same decoration pass as the description).
            let source = props.comment.body.clone().unwrap_or_default();
            // EXP-554: an attachment-only comment has NO body — rendering an
            // empty MarkdownView would leave a phantom line above the strip.
            let rendered = (!source.trim().is_empty()).then(|| {
                let mut view = MarkdownView::new(
                    SharedString::from(format!("comment-body-{comment_id}")),
                    source,
                )
                // EXP-521: comment bodies join the window selection layer —
                // sweep-select and copy across comments like on the web.
                .selectable(true)
                .images(props.images.clone());
                if let Some(team_id) = props.team_id {
                    let team = team_id.to_string();
                    view = view
                        .resolver(RefResolver::from_store(team_id))
                        .on_open_issue(move |identifier, window, cx| {
                            open_issue_by_identifier(&team, identifier, window, cx);
                        });
                }
                div().mt_0p5().text_sm().child(view)
            });
            v_flex()
                .w_full()
                .children(rendered)
                .children(comment_attachments_strip(
                    &comment_id,
                    props.images,
                    None,
                    cx,
                ))
                .into_any_element()
        }
    };

    // `.w_full()` is load-bearing on the bubble and the composer below:
    // without an explicit width the flex sizes to content, so a long
    // unwrappable comment line blows the row past the column (clipped text)
    // while an empty thread leaves the composer tiny (EXP-67).
    //
    // EXP-698 round 5: the comment is a CARD on the timeline rail — the
    // mobile shape (`RegularCommentRow`), now shared by web and the IDE.
    // EXP-547: picture + initials fallback (web `RegularCommentRow` renders
    // `AvatarImage`), so same-initials authors stay distinct. The 24px avatar
    // is the marker; it centres in the 28px gutter (gpui-component's `Size`
    // ladder has no 28 rung).
    let avatar = crate::user_avatar::user_avatar(
        props.comment.author_id.as_deref().unwrap_or_default(),
        &name,
        props.author.and_then(|user| user.image.as_deref()),
        gpui_component::Size::Small,
        cx,
    )
    .into_any_element();
    let bubble = crate::surface::glass_card()
        .w_full()
        .min_w_0()
        .px_3()
        .pt_2()
        .pb_2p5()
        .child(header)
        .child(body);
    // `pt_1` matches the row's `marker_top`, keeping the bubble's header level
    // with the avatar; `pb_2` is the gap to the next row and rides the
    // CONTENT, never a margin — the row's own height is what the gutter
    // stretches over, so the rail runs through the gap instead of stopping
    // short of it (and no margin has to survive the flex wrapper).
    let content = v_flex()
        .w_full()
        .min_w_0()
        .pt_1()
        .pb_2()
        .child(bubble)
        .into_any_element();

    crate::timeline::timeline_row(
        avatar,
        24.,
        4.,
        props.line_above,
        props.line_below,
        content,
    )
}

// ---------------------------------------------------------------------------
// Composer (issue-timeline.tsx bottom form)
// ---------------------------------------------------------------------------

/// Is this comment's attachment set already at the server cap? (Edit mode —
/// kept rows plus this edit's picks.)
fn edit_attachments_full(props: &CommentRowProps<'_>, cx: &gpui::App) -> bool {
    let kept = crate::comment_attachments::comment_attachments(&props.comment.id, cx)
        .into_iter()
        .filter(|row| {
            props
                .edit_removed
                .is_none_or(|staged| !staged.contains(&row.id))
        })
        .count();
    kept + props.edit_pending.len() >= MAX_COMMENT_ATTACHMENTS
}

/// EXP-568: the composer CARD — attachment chips on top, the borderless
/// auto-growing mention input in the middle, and the tool row along the
/// bottom (image · attach · `#` · emoji … submit). One glass card instead of
/// the old input-plus-buttons strip, so the whole thing reads as a single
/// compose surface the way the web and mobile composers do.
///
/// State (the `InputState` entity, the submitting flag, the pending picks, the
/// PressEnter subscription) lives on [`IssueTimeline`]; this only lays it out.
pub(crate) fn composer_row(
    input: &Entity<MentionInput>,
    submitting: bool,
    has_draft: bool,
    pending: &[PendingCommentAttachment],
    emoji_picker: &Entity<EmojiPicker>,
    emoji_open: bool,
    cx: &mut gpui::Context<IssueTimeline>,
) -> impl IntoElement {
    let full = pending.len() >= MAX_COMMENT_ATTACHMENTS;
    div().w_full().mt_2().child(crate::composer::glass_composer(
        crate::composer::GlassComposer::new(input.clone().into_any_element())
            .strip(pending_attachments_strip(pending, PendingScope::Composer, cx))
            .tool(
                composer_tool("comment-image", registry::EDITOR_IMAGE, "Insert image", cx)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.pick_comment_images(PendingScope::Composer, window, cx);
                    })),
            )
            .tool(attach_button(
                "comment-attach",
                PendingScope::Composer,
                full,
                cx,
            ))
            .tool(
                composer_tool(
                    "comment-issue-ref",
                    registry::EDITOR_ISSUE_REF,
                    "Link an issue",
                    cx,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.insert_issue_ref_trigger(PendingScope::Composer, window, cx);
                })),
            )
            .tool(
                emoji_button(
                    "comment-emoji",
                    PendingScope::Composer,
                    emoji_picker,
                    emoji_open,
                    cx,
                )
                .into_any_element(),
            )
            .submit(submit_button(submitting, submitting || !has_draft, cx)),
    ))
}

/// The comment composer's send: the shared round ghost submit
/// ([`crate::composer::composer_submit`]) on the `ui-submit` glyph, plus this
/// surface's loading flag and handler.
fn submit_button(
    submitting: bool,
    disabled: bool,
    cx: &mut gpui::Context<IssueTimeline>,
) -> Button {
    crate::composer::composer_submit("comment-submit", registry::UI_SUBMIT, disabled, cx)
        .loading(submitting)
        .on_click(cx.listener(|this, _, window, cx| this.submit_comment(window, cx)))
}

/// A composer tool button: the shared 24px ghost glyph
/// ([`crate::composer::composer_tool`]) plus a tooltip.
fn composer_tool(
    id: &'static str,
    icon: crate::icons::ExpIcon,
    tooltip: &'static str,
    cx: &mut gpui::Context<IssueTimeline>,
) -> Button {
    crate::composer::composer_tool(id, icon, cx).tooltip(tooltip)
}

/// EXP-551: the emoji trigger both comment composers grow — a smiley that
/// opens the shared picker; a pick inserts the UNICODE at the cursor.
///
/// `picker`/`open` come from the timeline's `&self`, NEVER from
/// `cx.entity().read(cx)`: this renders inside `IssueTimeline::render`, where
/// gpui holds the entity's lease, so reading it back through the map is a
/// guaranteed `double_lease_panic` (it aborted desktop 0.14.16 on every issue
/// open).
fn emoji_button(
    id: impl Into<gpui::ElementId>,
    scope: PendingScope,
    picker: &Entity<EmojiPicker>,
    open: bool,
    cx: &mut gpui::Context<IssueTimeline>,
) -> impl IntoElement {
    let id = id.into();
    let popover_id = gpui::ElementId::Name(SharedString::from(format!("{id:?}-popover")));
    emoji_picker_popover(
        popover_id,
        Button::new(id)
            .ghost()
            .web_icon_xs()
            .icon(Icon::new(registry::EDITOR_EMOJI).text_color(cx.theme().muted_foreground))
            .tooltip("Emoji"),
        picker.clone(),
        open,
        cx.listener(move |this, open: &bool, _window, cx| {
            this.set_emoji_open(scope, *open, cx);
        }),
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

        assert_eq!(author_label(None), "Someone");
    }

    #[test]
    fn user_label_falls_back_to_member_when_row_missing() {
        let user: User = serde_json::from_value(json!({
            "id": "u-1", "name": "Ada Lovelace"
        }))
        .unwrap();
        // Present row → the normal author label.
        assert_eq!(user_label("u-1", Some(&user)), "Ada Lovelace");
        // Missing row (un-synced co-member) → the Member fallback, not "Someone".
        assert_eq!(user_label("user_abc123ef", None), "Member 23EF");
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
