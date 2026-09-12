//! EXP-850 §11 — the session's DIFF PANE.
//!
//! The bottom "Changes" band is gone (EXP-698/773's band; the merge rules and
//! the diff parse it owned stayed in [`crate::changes_bar`]). The session's
//! diff now opens as a right-hand pane INSIDE the session view: the header's
//! Diff pill toggles it, the transcript keeps the rest of the width, and a
//! per-message file card ([`crate::session_rows::FileCard`]) opens it scrolled
//! to one file.
//!
//! The pane is a header (file-list toggle when there is more than one file,
//! the selected path with its `+N −M`, a close ×) over the shared
//! [`crate::diff::DiffView`], with a collapsible file list down its left.

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, ClickEvent, Context, Entity,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, SharedString,
    StatefulInteractiveElement as _, Styled,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon};

use crate::controls::WebText as _;

use crate::icons::registry;

/// The pane's share of the session view when it opens (§11).
pub(crate) const WIDTH_FRACTION: f32 = 0.45;
/// …and the narrowest it may be.
pub(crate) const MIN_WIDTH: f32 = 360.;
/// What the transcript beside it keeps. Below this the pane takes the WHOLE
/// view instead of squeezing the conversation into a gutter.
pub(crate) const TRANSCRIPT_MIN_WIDTH: f32 = 320.;
/// The file list's own column.
pub(crate) const FILE_LIST_WIDTH: f32 = 216.;

/// The pane's width inside a session view `total` px wide (§11: 45 %, min
/// [`MIN_WIDTH`]). A view too narrow to hold both columns hands the pane the
/// whole width — the caller compares the answer with `total` to decide
/// whether the transcript still renders.
pub(crate) fn pane_width(total: f32) -> f32 {
    if total <= 0. {
        return MIN_WIDTH;
    }
    if total < MIN_WIDTH + TRANSCRIPT_MIN_WIDTH {
        return total;
    }
    (total * WIDTH_FRACTION)
        .max(MIN_WIDTH)
        .min(total - TRANSCRIPT_MIN_WIDTH)
}

/// Whether the transcript still has a column of its own beside the pane.
pub(crate) fn transcript_visible(total: f32) -> bool {
    pane_width(total) < total
}

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
    pub(crate) width: Pixels,
    pub(crate) diff: Entity<crate::diff::DiffView>,
    pub(crate) on_close: Box<dyn Fn(&mut V, &mut Context<V>) + 'static>,
    pub(crate) on_toggle_list: Box<dyn Fn(&mut V, &mut Context<V>) + 'static>,
    pub(crate) on_pick: std::rc::Rc<dyn Fn(&mut V, usize, &mut Context<V>) + 'static>,
}

/// §11 — the pane itself.
pub(crate) fn render<V: Render>(spec: DiffPaneSpec<V>, cx: &mut Context<V>) -> AnyElement {
    let DiffPaneSpec {
        files,
        selected,
        list_open,
        width,
        diff,
        on_close,
        on_toggle_list,
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
                crate::controls::glass_icon_button(
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
        .child(
            crate::controls::glass_icon_button(
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

    v_flex()
        .h_full()
        .w(width)
        .flex_shrink_0()
        .min_w_0()
        .overflow_hidden()
        .border_l_1()
        .border_color(theme::tokens::glass::STROKE_SECTION.to_hsla())
        .bg(theme::tokens::glass::FILL_SECTION.to_hsla())
        .child(header)
        .child(
            h_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .min_w_0()
                .children(list)
                .child(div().flex_1().min_w_0().h_full().child(diff)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §11: 45 % of the view, never under 360px, never eating the
    /// transcript's last 320px — and on a pane too narrow for both, the diff
    /// takes the whole width.
    #[test]
    fn the_pane_takes_45_percent_down_to_its_floor() {
        assert_eq!(pane_width(1600.), 720.);
        assert!(transcript_visible(1600.));
        // 45 % of 900 is 405 — over the floor, under the transcript's guard.
        assert_eq!(pane_width(900.), 405.);
        // 45 % of 700 is 315: the floor wins, and the transcript keeps 340.
        assert_eq!(pane_width(700.), 360.);
        assert!(transcript_visible(700.));
        // 45 % of 4000 is 1800 — nothing clamps it back down.
        assert_eq!(pane_width(4000.), 1800.);
        // Narrower than floor + guard: the pane IS the view.
        assert_eq!(pane_width(600.), 600.);
        assert!(!transcript_visible(600.));
        // An unmeasured view (first frame) opens at the floor.
        assert_eq!(pane_width(0.), MIN_WIDTH);
    }
}
