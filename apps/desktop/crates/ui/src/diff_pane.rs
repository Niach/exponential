//! EXP-850 §11 — the session's DIFF PANE.
//!
//! The bottom "Changes" band is gone (EXP-698/773's band; the merge rules and
//! the diff parse it owned stayed in [`crate::changes_bar`]). A per-message
//! file card ([`crate::session_rows::FileCard`]) opens the pane scrolled to
//! one file.
//!
//! EXP-877 made it a FULL PAGE under the run's header rather than a
//! right-hand split: the diff and the transcript are two things you read, not
//! one thing you read while glancing at the other, and the split gave each
//! half too little. The drag handle, the persisted width and the 45 % opening
//! share went with it — a page has no width to remember. Its content is
//! centred in the same [`crate::work_header::WORK_COLUMN_W`] column the run's
//! header and transcript use, so nothing shifts sideways when you toggle it.
//!
//! The pane is a header (file-list toggle when there is more than one file,
//! the selected path with its `+N −M`, the scope chip, a close ×) over the
//! shared [`crate::diff::DiffView`], with a collapsible file list down its
//! left. The SCOPE chip is both the label for a narrowed view (one turn's
//! files, one edit) and the way back to the whole branch.

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, ClickEvent, Context, Entity,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon};

use crate::controls::WebText as _;

use crate::icons::registry;

/// The file list's own column.
pub(crate) const FILE_LIST_WIDTH: f32 = 216.;

/// One row of the pane's file list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaneFile {
    pub(crate) path: SharedString,
    pub(crate) additions: u32,
    pub(crate) deletions: u32,
}

/// Everything one painting of the pane needs. Generic over the hosting view,
/// like [`crate::changes_bar`] was, so the pane's open/selected state stays
/// the caller's.
pub(crate) struct DiffPaneSpec<V: Render> {
    pub(crate) files: Vec<PaneFile>,
    /// The file the header names (an index into `files`).
    pub(crate) selected: usize,
    /// Whether the left file list is unfolded.
    pub(crate) list_open: bool,
    /// EXP-862: what the pane is SHOWING, when it is not the whole branch —
    /// "This turn: 3 files" / "This edit". `None` = the session scope, where
    /// the header's Diff pill already says it.
    pub(crate) scope_label: Option<SharedString>,
    pub(crate) diff: Entity<crate::diff::DiffView>,
    pub(crate) on_close: Box<dyn Fn(&mut V, &mut Context<V>) + 'static>,
    pub(crate) on_toggle_list: Box<dyn Fn(&mut V, &mut Context<V>) + 'static>,
    /// The scope chip's click: back to the whole branch.
    pub(crate) on_show_session: Box<dyn Fn(&mut V, &mut Context<V>) + 'static>,
    pub(crate) on_pick: std::rc::Rc<dyn Fn(&mut V, usize, &mut Context<V>) + 'static>,
}

/// §11/EXP-877 — the pane itself: a full page, its content centred in the
/// work column.
pub(crate) fn render<V: Render>(spec: DiffPaneSpec<V>, cx: &mut Context<V>) -> AnyElement {
    let DiffPaneSpec {
        files,
        selected,
        list_open,
        scope_label,
        diff,
        on_close,
        on_toggle_list,
        on_show_session,
        on_pick,
    } = spec;
    let muted = cx.theme().muted_foreground;
    let green = theme::tokens::GREEN.to_hsla();
    let danger = cx.theme().danger;
    let multiple = files.len() > 1;
    let current = files.get(selected).or_else(|| files.first());

    let header = h_flex()
        .w_full()
        .flex_shrink_0()
        .h(px(32.))
        .px_2()
        .gap_2()
        .items_center()
        .border_b_1()
        .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
        // The file-list toggle only exists when there is more than one file
        // to list (§11).
        .when(multiple, |this| {
            this.child(
                // EXP-862: a glyph on a row is a GHOST button — the circle is
                // for the primary actions only.
                crate::controls::ghost_icon_button(
                    "session-diff-files",
                    Icon::new(registry::NAV_FILES),
                    cx,
                )
                .tooltip(if list_open {
                    "Hide file list"
                } else {
                    "Show file list"
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    on_toggle_list(this, cx);
                })),
            )
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_xs()
                .font_family(theme::terminal::FONT_FAMILY)
                .child(
                    current
                        .map(|file| file.path.clone())
                        .unwrap_or_else(|| SharedString::from("Changes")),
                ),
        )
        .children(current.map(|file| {
            h_flex()
                .flex_shrink_0()
                .gap_1p5()
                .text_2xs()
                .font_family(theme::terminal::FONT_FAMILY)
                .child(
                    div()
                        .text_color(green)
                        .child(SharedString::from(format!("+{}", file.additions))),
                )
                .child(
                    div()
                        .text_color(danger)
                        .child(SharedString::from(format!("-{}", file.deletions))),
                )
        }))
        // EXP-862 — the SCOPE chip: what the pane is showing when it is not
        // the whole branch, and the click that goes back to it.
        .children(scope_label.map(|label| {
            crate::surface::glass_pill(
                "session-diff-scope",
                crate::surface::PillSize::Sm,
                crate::surface::PillMode::Action,
                cx,
            )
            .flex_shrink_0()
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("Show all changes").build(window, cx)
            })
            .child(div().text_2xs().child(label))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                cx.stop_propagation();
                on_show_session(this, cx);
            }))
        }))
        .child(
            crate::controls::ghost_icon_button(
                "session-diff-close",
                Icon::new(registry::UI_CLOSE),
                cx,
            )
            .tooltip("Close diff")
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                cx.stop_propagation();
                on_close(this, cx);
            })),
        );

    let list = (multiple && list_open).then(|| {
        let mut column = v_flex()
            .id("session-diff-file-list")
            .h_full()
            .w(px(FILE_LIST_WIDTH))
            .flex_shrink_0()
            .overflow_y_scroll()
            .p_1()
            .gap_0p5()
            .border_r_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla());
        for (index, file) in files.iter().enumerate() {
            let on_pick = on_pick.clone();
            let active = index == selected;
            column = column.child(
                crate::surface::flat_row()
                    .id(("session-diff-file", index))
                    .flex()
                    .w_full()
                    .min_w_0()
                    .gap_1p5()
                    .items_center()
                    .px_1p5()
                    .py_1()
                    .cursor_pointer()
                    .when(active, |this| {
                        this.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla())
                    })
                    .hover(|this| this.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_2xs()
                            .font_family(theme::terminal::FONT_FAMILY)
                            .text_color(if active { cx.theme().foreground } else { muted })
                            .child(file.path.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_2xs()
                            .font_family(theme::terminal::FONT_FAMILY)
                            .text_color(green)
                            .child(SharedString::from(format!("+{}", file.additions))),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_2xs()
                            .font_family(theme::terminal::FONT_FAMILY)
                            .text_color(danger)
                            .child(SharedString::from(format!("-{}", file.deletions))),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        on_pick(this, index, cx);
                    })),
            );
        }
        column
    });

    // EXP-877: the page is full-bleed, its CONTENT is the work column — the
    // same block the header and the transcript sit in, so toggling the diff
    // never shifts the run sideways.
    v_flex()
        .size_full()
        .min_w_0()
        .items_center()
        .overflow_hidden()
        .child(
            v_flex()
                .w_full()
                .max_w(px(crate::work_header::WORK_COLUMN_W))
                .h_full()
                .min_w_0()
                .overflow_hidden()
                .child(header)
                .child(
                    h_flex()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .min_w_0()
                        .children(list)
                        .child(div().flex_1().min_w_0().h_full().child(diff)),
                ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-877: the pane is a PAGE, so it has exactly one width rule left —
    /// the shared work column. (The 45 % share, the 360px floor, the
    /// transcript's 320px guard and the drag that moved between them are all
    /// gone with the split.)
    #[test]
    fn the_pane_is_the_work_column_wide() {
        assert_eq!(crate::work_header::WORK_COLUMN_W, 896.);
        // The file list still fits inside it with room for a diff.
        assert!(FILE_LIST_WIDTH * 2. < crate::work_header::WORK_COLUMN_W);
    }
}
