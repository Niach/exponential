//! The issue-detail attachment rail (masterplan-v3 §4.2: "an attachment rail
//! — image thumbnails from the `attachments` collection"; web reference:
//! `apps/web/src/components/issue-editor/attachment-rail.tsx`).
//!
//! Desktop v1 renders compact chips (image glyph + filename + size) off the
//! synced `attachments` rows for the issue, plus the web's trailing
//! "N images" count label. Fetching the actual bitmaps needs authenticated
//! `/api/attachments/{id}` requests — that pipeline belongs to the §4.5
//! editor's image path; the rail upgrades to real thumbnails when it lands.
//! Upload entry points also live with the editor (web puts the attach button
//! in the same strip; the §4.5 one-upload-path rule keeps it there).

use gpui::{div, App, IntoElement, ParentElement, SharedString, Styled};
use gpui_component::{h_flex, ActiveTheme as _, Icon, Sizable as _};
use sync::Store;

use domain::rows::Attachment;

use crate::icons::ExpIcon;

/// The rail, or `None` when the issue has no attachments (web renders the
/// strip only when there is something to show — the attach button lives in
/// the editor toolbar on desktop).
pub(crate) fn attachments_row(issue_id: &str, cx: &App) -> Option<impl IntoElement> {
    let mut attachments: Vec<Attachment> = Store::global(cx)
        .collections()
        .attachments
        .read(cx)
        .iter()
        .filter(|attachment| attachment.issue_id.as_deref() == Some(issue_id))
        .cloned()
        .collect();
    if attachments.is_empty() {
        return None;
    }
    attachments.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    let count_label = if attachments.len() == 1 {
        "1 image".to_string()
    } else {
        format!("{} images", attachments.len())
    };

    Some(
        h_flex()
            .w_full()
            .px_4()
            .py_3()
            .gap_2()
            .items_center()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1p5()
                    .overflow_hidden()
                    .children(attachments.iter().map(|attachment| chip(attachment, cx))),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(count_label)),
            ),
    )
}

/// One attachment chip: image glyph + truncating filename + optional size
/// (web chip layout, minus the remove ✕ — removal edits the description
/// markdown, which is the §4.5 editor's job).
fn chip(attachment: &Attachment, cx: &App) -> impl IntoElement {
    let label = attachment
        .filename
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Image".to_string());

    let mut row = h_flex()
        .flex_shrink_0()
        .gap_1()
        .px_1p5()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border.opacity(0.5))
        .bg(cx.theme().secondary.opacity(0.4))
        .items_center()
        .child(
            Icon::from(ExpIcon::Image)
                .xsmall()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .max_w(gpui::px(96.))
                .text_xs()
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(SharedString::from(label)),
        );

    if let Some(size) = attachment.size_bytes.filter(|size| *size > 0) {
        row = row.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .whitespace_nowrap()
                .child(SharedString::from(format_bytes(size))),
        );
    }
    row
}

/// `12.3 KB` / `1.2 MB` style size label.
fn format_bytes(bytes: i64) -> String {
    const KB: f64 = 1024.;
    const MB: f64 = 1024. * 1024.;
    let bytes = bytes as f64;
    if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes / KB)
    } else {
        format!("{bytes:.0} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_labels_scale() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(4096), "4 KB");
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.0 MB");
    }
}
