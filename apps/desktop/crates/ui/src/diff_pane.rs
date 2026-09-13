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
//!
//! EXP-862 gave it two things: a DRAGGABLE left edge (the width is the
//! reader's, persisted once for every session in [`crate::ui_prefs`]) and a
//! SCOPE chip — the pane can show one turn's files or one edit, and the chip
//! is both the label for that and the way back to the whole branch.

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, ClickEvent, Context, Entity,
    InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, ParentElement as _, Pixels,
    Render, SharedString, StatefulInteractiveElement as _, Styled,
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
/// The grab strip on the pane's left edge (EXP-862) — 8px, the same hit
/// width every split in the IDE offers.
const RESIZE_HANDLE_W: f32 = 8.;

/// EXP-862 — `requested` clamped into a session view `total` px wide: never
/// under [`MIN_WIDTH`], never over `total - `[`TRANSCRIPT_MIN_WIDTH`], and on
/// a view too narrow to hold both columns the pane takes the WHOLE width (the
/// caller compares the answer with `total` to decide whether the transcript
/// still renders). Pure — this is the one rule the drag, the persisted width
/// and the opening width all go through.
pub(crate) fn clamp_pane_width(requested: f32, total: f32) -> f32 {
    if total <= 0. {
        return MIN_WIDTH;
    }
    if total < MIN_WIDTH + TRANSCRIPT_MIN_WIDTH {
        return total;
    }
    requested.max(MIN_WIDTH).min(total - TRANSCRIPT_MIN_WIDTH)
}

/// The width the pane OPENS at inside a view `total` px wide (§11: 45 %,
/// clamped) — what a reader who has never dragged the edge gets.
pub(crate) fn pane_width(total: f32) -> f32 {
    clamp_pane_width(total * WIDTH_FRACTION, total)
}

/// Whether the transcript still has a column of its own beside a pane
/// `width` px wide.
pub(crate) fn transcript_visible(width: f32, total: f32) -> bool {
    width < total
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
    /// EXP-862: what the pane is SHOWING, when it is not the whole branch —
    /// "This turn: 3 files" / "This edit". `None` = the session scope, where
    /// the header's Diff pill already says it.
    pub(crate) scope_label: Option<SharedString>,
    pub(crate) diff: Entity<crate::diff::DiffView>,
    pub(crate) on_close: Box<dyn Fn(&mut V, &mut Context<V>) + 'static>,
    pub(crate) on_toggle_list: Box<dyn Fn(&mut V, &mut Context<V>) + 'static>,
    /// The scope chip's click: back to the whole branch.
    pub(crate) on_show_session: Box<dyn Fn(&mut V, &mut Context<V>) + 'static>,
    /// EXP-862 — the left edge went down at this window x. The pane cannot
    /// own the rest of the gesture (the pointer leaves it immediately), so
    /// the HOST captures the window's mouse-move until the button comes up
    /// and clamps each position through [`clamp_pane_width`].
    pub(crate) on_resize: Box<dyn Fn(&mut V, Pixels, &mut Context<V>) + 'static>,
    pub(crate) on_pick: std::rc::Rc<dyn Fn(&mut V, usize, &mut Context<V>) + 'static>,
}

/// §11 — the pane itself.
pub(crate) fn render<V: Render>(spec: DiffPaneSpec<V>, cx: &mut Context<V>) -> AnyElement {
    let DiffPaneSpec {
        files,
        selected,
        list_open,
        width,
        scope_label,
        diff,
        on_close,
        on_toggle_list,
        on_show_session,
        on_resize,
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

    // EXP-862 — the drag strip on the pane's LEFT edge. Absolute, so it
    // overlays the border rather than taking a column of its own, and
    // deliberately NOT `h_resizable`: that component owns the widths of a
    // whole panel group, and this is one edge whose width is persisted by
    // the host.
    let handle = div()
        .id("session-diff-resize")
        .absolute()
        .left_0()
        .top_0()
        .h_full()
        .w(px(RESIZE_HANDLE_W))
        .cursor_col_resize()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                cx.stop_propagation();
                on_resize(this, event.position.x, cx);
            }),
        );

    v_flex()
        .relative()
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
        .child(handle)
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
        assert!(transcript_visible(pane_width(1600.), 1600.));
        // 45 % of 900 is 405 — over the floor, under the transcript's guard.
        assert_eq!(pane_width(900.), 405.);
        // 45 % of 700 is 315: the floor wins, and the transcript keeps 340.
        assert_eq!(pane_width(700.), 360.);
        assert!(transcript_visible(pane_width(700.), 700.));
        // 45 % of 4000 is 1800 — nothing clamps it back down.
        assert_eq!(pane_width(4000.), 1800.);
        // Narrower than floor + guard: the pane IS the view.
        assert_eq!(pane_width(600.), 600.);
        assert!(!transcript_visible(pane_width(600.), 600.));
        // An unmeasured view (first frame) opens at the floor.
        assert_eq!(pane_width(0.), MIN_WIDTH);
    }

    /// EXP-862 — the DRAG's rule: a dragged width lands between the floor and
    /// the transcript's guard whatever the pointer asks for, a persisted
    /// width from a wider window is clamped back into this one, and a view
    /// too narrow for both columns still hands the pane everything.
    #[test]
    fn a_dragged_width_is_clamped_into_the_view() {
        // Inside the band: taken verbatim.
        assert_eq!(clamp_pane_width(500., 1600.), 500.);
        // Dragged past the right edge / to nothing: the floor.
        assert_eq!(clamp_pane_width(10., 1600.), MIN_WIDTH);
        assert_eq!(clamp_pane_width(-400., 1600.), MIN_WIDTH);
        // Dragged over the transcript: its last 320px survive.
        assert_eq!(clamp_pane_width(1590., 1600.), 1280.);
        // A width remembered from a 2400px window, re-opened at 900.
        assert_eq!(clamp_pane_width(1080., 900.), 580.);
        // Under floor + guard the pane is the view, whatever was asked for.
        assert_eq!(clamp_pane_width(400., 600.), 600.);
        assert_eq!(clamp_pane_width(1000., 0.), MIN_WIDTH);
        // The opening width IS this rule applied to 45 %.
        assert_eq!(pane_width(1600.), clamp_pane_width(1600. * 0.45, 1600.));
    }
}
