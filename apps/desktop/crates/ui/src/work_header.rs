//! EXP-877 — the ONE work header shared by the issue detail and the session
//! view, byte-identical with the web `WorkHeader`.
//!
//! EXP-1162 (contract `detail-chrome.json`): a compact FIXED bar — the
//! [`collapsed_title`] on its left, its right cluster on the right (EXP-1294:
//! for an issue `[Start] [pin] [⋯]`; the session header names its own). The
//! issue face's large title (the editable input, [`TITLE_SIZE`]) and its
//! property tray are the first rows of its SCROLLING body, and the bar's
//! title appears only once that row has scrolled under it; a face with no
//! title row is always collapsed and keeps its tray fixed under the bar. No
//! hairline: the body wears the edge strips ([`work_body`]). The column caps
//! at [`WORK_COLUMN_W`], the same as the issue body, transcript and diff.

use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, InteractiveElement as _,
    IntoElement, ParentElement, SharedString, Styled, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _};

use crate::icons::{registry, ExpIcon};
use crate::issue_detail::centered_column;
use crate::surface::{glass_pill_button, glass_pill_button_primary, PillSize};

/// The shared work column width — web `max-w-4xl` (896px): header, issue
/// body, transcript and the full-page diff all cap to it.
pub(crate) const WORK_COLUMN_W: f32 = 896.;

/// EXP-1191 — the work column's ONE content box: [`WORK_COLUMN_W`] inset by
/// this much on both sides (web `md:px-5`, 1.25rem). Every face (issue,
/// draft, run, changes, results) puts its VISIBLE edges on it: text starts,
/// chip / card / box outer borders, hairline dividers, the leading glyph of
/// a hover-filled row (its fill may hang outside), the bar's collapsed
/// title and trailing cluster. A block that pads itself (the WYSIWYG
/// editor's `WYSIWYG_BLOCK_PADDING_X`) takes the REMAINDER, never adds to it.
pub(crate) const WORK_GUTTER: f32 = 20.;

/// A 32px ghost icon button's box-to-glyph inset (`controls::
/// ghost_icon_button`): a trailing / leading ghost glyph hangs its hit box
/// into the gutter by this much so the GLYPH sits on the content edge.
pub(crate) const GHOST_ICON_HANG: f32 = 8.;

/// EXP-926 / FEED-45 — ONE size rule per PLACEMENT, and the placement is the
/// only thing that decides it.
///
/// A header action (Start, Merge PR, Stop) sits in exactly two places: IN the
/// compact bar's right cluster, or INSIDE the property tray beside status,
/// priority, labels, due date and board. In the bar it matches the bar's
/// controls ([`PillSize::Lg`], the 36px rung); in the tray it matches the
/// CHIPS ([`PillSize::Sm`], what `pickers::chip_button` wears).
pub(crate) fn header_action_size(in_tray: bool) -> PillSize {
    if in_tray {
        PillSize::Sm
    } else {
        PillSize::Lg
    }
}

/// EXP-916 — the ONE way out to GitHub: a ghost glyph in the right cluster,
/// present exactly while the subject HAS a pull request. `None` otherwise —
/// an empty link is worse than no link. Callers name the element so two
/// clusters can carry it at once.
pub(crate) fn github_button(
    id: impl Into<gpui::ElementId>,
    pr_url: Option<&str>,
    cx: &mut App,
) -> Option<AnyElement> {
    let url = pr_url.map(str::trim).filter(|url| !url.is_empty())?.to_string();
    Some(
        crate::controls::ghost_icon_button(id, Icon::new(registry::UI_GITHUB), cx)
            .tooltip(domain::contract::DIFF_UI_OPEN_ON_GITHUB)
            .on_click(move |_, _, cx| {
                crate::settings::open_url(cx, url.clone());
            })
            .into_any_element(),
    )
}

// ---------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------

/// The ONE Merge look: a pill (primary or glass) with the merge glyph,
/// two-click armed (`Merge PR` → `Confirm merge`, then `Merging…` until the
/// Electric echo settles) through [`crate::pr_merge::two_click`]. EXP-1294:
/// it merges ONE pull request (`pullRequests.merge`); a member of an open
/// stack asks its one confirm first (EXP-1248). A refusal is the error toast.
pub(crate) fn merge_pill(
    id: impl Into<gpui::ElementId>,
    pull_request_id: &str,
    primary: bool,
    // EXP-926: the placement's size, never the pill's own opinion.
    size: PillSize,
    cx: &mut App,
) -> AnyElement {
    let (armed, merging) = {
        let state = crate::pr_merge::MergeState::global(cx);
        let state = state.read(cx);
        (state.armed(pull_request_id), state.merging(pull_request_id))
    };
    let glyph = if armed {
        cx.theme().danger
    } else if primary {
        cx.theme().primary_foreground
    } else {
        cx.theme().muted_foreground
    };
    // EXP-1248: ONE control, renamed on a member of an open stack.
    let settled = crate::pr_merge::merge_label_for(pull_request_id, cx);
    let pull_request_id = pull_request_id.to_string();
    let mut button = if primary {
        glass_pill_button_primary(id, size)
    } else {
        glass_pill_button(id, size, cx)
    }
    .icon(
        Icon::from(ExpIcon::GitMerge)
            .with_size(px(size.glyph()))
            .text_color(glyph),
    )
    // EXP-916: the settled label is the CONTRACT's, so the pill reads the
    // same word on all four clients.
    .label(if merging {
        "Merging…"
    } else if armed {
        "Confirm merge"
    } else {
        settled
    })
    .tooltip("Merge: completes every linked issue")
    .on_click(move |_, window, cx| {
        if crate::pr_merge::ask_stack_merge(&pull_request_id, window, cx) {
            return;
        }
        crate::pr_merge::two_click(crate::pr_merge::MergeOp::merge(pull_request_id.clone()), cx);
    });
    if merging {
        button = button.disabled(true);
    }
    button.into_any_element()
}

// ---------------------------------------------------------------------------
// The header frame
// ---------------------------------------------------------------------------

/// The issue face's LARGE title block (EXP-1162: the first row of its
/// scrolling body; the run faces wear only the compact title): the web's
/// title Textarea to the pixel, `pt-4 pb-1`, 24px semibold on a 32px line
/// (NOT gpui's `text_2xl`, which is 21px on the 14px rem).
pub(crate) const TITLE_PT: f32 = 16.;
pub(crate) const TITLE_PB: f32 = 4.;
pub(crate) const TITLE_SIZE: f32 = 24.;
pub(crate) const TITLE_LINE: f32 = 32.;
/// The multi-line widget's own insets (`Size::Medium` `input_py` /
/// `input_px`), applied underneath any refined style: the editable title's
/// wrapper gives them back so its text lands on the block above.
pub(crate) const TITLE_WIDGET_PY: f32 = 8.;
pub(crate) const TITLE_WIDGET_PX: f32 = 10.;
/// EXP-1162: the compact bar's vertical inset around its 36px controls.
pub(crate) const BAR_PY: f32 = 8.;
/// The collapsed title's two lines: the mono identifier over the title.
const COLLAPSED_ID_SIZE: f32 = 11.;
const COLLAPSED_ID_LINE: f32 = 14.;
const COLLAPSED_TITLE_SIZE: f32 = 14.;
const COLLAPSED_TITLE_LINE: f32 = 20.;

/// EXP-1162 — the COLLAPSED title, the compact bar's left side: the mono
/// identifier (small, muted) over the title on ONE truncated line. `fade_in`
/// = the issue face's break (contract `detail-chrome.json`): the element
/// fades in over `collapseMs` while rising `collapseRise`, keyed by the id so
/// it replays each time the title breaks again. Faces that are ALWAYS
/// collapsed (Run, Changes, Results) pass `None` and never animate.
/// D8 — [`collapsed_title`] while its subject has not synced: the title
/// line's placeholder bar (never "Loading…"), the line's own height.
pub(crate) fn collapsed_title_placeholder(id: impl Into<gpui::ElementId>, cx: &App) -> AnyElement {
    h_flex()
        .w_full()
        .h(px(COLLAPSED_TITLE_LINE))
        .items_center()
        .child(crate::feedback::skeleton_value(id, 160., cx))
        .into_any_element()
}

pub(crate) fn collapsed_title(
    identifier: Option<SharedString>,
    title: impl Into<SharedString>,
    fade_in: Option<gpui::ElementId>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let block = v_flex()
        .relative()
        .w_full()
        .min_w_0()
        .children(identifier.map(|identifier| {
            div()
                .min_w_0()
                .truncate()
                .font_family(theme.mono_font_family.clone())
                .text_size(px(COLLAPSED_ID_SIZE))
                .line_height(px(COLLAPSED_ID_LINE))
                .text_color(theme.muted_foreground)
                .child(identifier)
        }))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(COLLAPSED_TITLE_SIZE))
                .line_height(px(COLLAPSED_TITLE_LINE))
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(title.into()),
        );
    match fade_in {
        None => block.into_any_element(),
        Some(id) => {
            use gpui::AnimationExt as _;
            let rise = theme::motion::travel(domain::detail_chrome::COLLAPSE_RISE);
            block
                .with_animation(
                    id,
                    gpui::Animation::new(std::time::Duration::from_millis(
                        domain::detail_chrome::COLLAPSE_MS,
                    )),
                    move |block, delta| block.opacity(delta).top(px(rise * (1. - delta))),
                )
                .into_any_element()
        }
    }
}

/// EXP-1162 — the compact bar's height: one Lg-rung control + its inset.
pub(crate) const BAR_H: f32 = theme::tokens::size::CONTROL_LG + 2. * BAR_PY;
/// Until the cluster has been measured once, a floating title row keeps this
/// far clear of it (Start + pin + `…`, with room to spare).
pub(crate) const CLUSTER_W_FALLBACK: f32 = 240.;
/// The gap a floating title row keeps from the cluster beside it.
pub(crate) const CLUSTER_GAP: f32 = 12.;

/// The cluster measure a floating bar reports back to its view: the width
/// cell and the view to repaint when it changes.
pub(crate) type ClusterMeasure = (Rc<std::cell::Cell<Option<f32>>>, gpui::EntityId);

/// EXP-1162 — the Issue face's FLOATING bar (web `WorkHeader floating`): it
/// overlays the top of the scrolling body (the caller's wrapper is
/// `relative()`, this is its LAST child) and takes no row of its own. At
/// rest it paints nothing but its right cluster, on the large title's first
/// line; the title row keeps clear of the cluster's width (`measure`, only
/// passed at rest — the collapsed cluster may grow Merge, and
/// the title's wrap must not follow it). Collapsed, it shows the compact
/// title and fades in its ground — the panel at `scrim` + the `edgeTop`
/// strip under it — over `collapseMs`. Nothing but the cluster's buttons
/// carries a hitbox, so the bar's empty left never blocks the title editor
/// (or anything else) under it, nor the scroll wheel.
pub(crate) fn render_floating_bar(
    title: Option<AnyElement>,
    right: Vec<AnyElement>,
    collapsed: bool,
    measure: Option<ClusterMeasure>,
    below: Option<AnyElement>,
) -> AnyElement {
    let cluster = h_flex()
        .relative()
        .flex_shrink_0()
        .items_center()
        .gap_1()
        .children(right)
        .when_some(measure, |cluster, (cell, entity_id)| {
            cluster.child(
                gpui::canvas(
                    move |bounds, _, cx| {
                        let width = f32::from(bounds.size.width);
                        let known = cell.get();
                        cell.set(Some(width));
                        if known.is_none_or(|known| (known - width).abs() > 0.5) {
                            cx.defer(move |cx| cx.notify(entity_id));
                        }
                    },
                    |_, _: (), _, _| {},
                )
                .absolute()
                .size_full(),
            )
        });
    // EXP-1191: with the tray pinned under the bar the fade hangs under the
    // TRAY's ground instead.
    let pinned = below.is_some();
    let ground = collapsed.then(|| {
        use gpui::AnimationExt as _;
        // EXP-1256: the band starts at the card's top edge — inside its
        // hairlines, its top corners on the card's inner radius.
        crate::surface::scrim_band(true)
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(BAR_H))
            .when(!pinned, |ground| {
                ground.child(
                    crate::surface::edge_fade_top_scrim()
                        .top(px(BAR_H))
                        .left_0()
                        .right_0(),
                )
            })
            .with_animation(
                "work-bar-ground",
                gpui::Animation::new(std::time::Duration::from_millis(
                    domain::detail_chrome::COLLAPSE_MS,
                )),
                |ground, delta| ground.opacity(delta),
            )
    });
    let bar = div()
        .relative()
        .h(px(BAR_H))
        .children(ground)
        .child(centered_column(
            h_flex()
                .h(px(BAR_H))
                .items_center()
                .gap_3()
                .px(px(WORK_GUTTER))
                .child(div().flex_1().min_w_0().children(title))
                .child(cluster),
        ));
    // EXP-1191: the pinned tray, on the same ground, the fade under it.
    let below = below.map(|below| {
        crate::surface::scrim_band(false)
            .relative()
            .pb(px(12.))
            .child(
                crate::surface::edge_fade_top_scrim()
                    .top(gpui::relative(1.))
                    .left_0()
                    .right_0(),
            )
            .child(centered_column(div().child(below)))
    });
    v_flex()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .child(bar)
        .children(below)
        .into_any_element()
}

/// EXP-1162 — the area under the work header: `body` (a scroll container or
/// a pane with its own) with the TOP edge strip overlaid on it, and the
/// bottom one when `bottom_edge` (the body runs to the pane's end).
///
/// EXP-1261: the top strip only once the body has SCROLLED (`scrolled`, the
/// caller's read of its scroll offset, e.g. [`list_scrolled`]). At
/// rest nothing has slid under the header, so a strip there only veiled the
/// body's first row. Reading the offset at render is enough: a scroll
/// notifies the scrolling view, and gpui re-renders every ancestor view of
/// a dirty one, so the host's frame sees the new offset.
pub(crate) fn work_body(body: AnyElement, bottom_edge: bool, scrolled: bool) -> AnyElement {
    div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .w_full()
        .child(body)
        .when(scrolled, |body| body.child(crate::surface::edge_fade_top()))
        .when(bottom_edge, |body| body.child(crate::surface::edge_fade_bottom()))
        .into_any_element()
}

/// EXP-1261 — has a `gpui::list` left its top? Past its first item, or into
/// it.
pub(crate) fn list_scrolled(list: &gpui::ListState) -> bool {
    let top = list.logical_scroll_top();
    top.item_ix > 0 || top.offset_in_item > px(0.)
}

// ---------------------------------------------------------------------------
// EXP-1170 — the issue face's scrolling title rows, shared by the detail and
// the New issue page (the detail in DRAFT mode)
// ---------------------------------------------------------------------------

/// The large editable title (the `TITLE_*` block): the borderless
/// auto-grow textarea at [`TITLE_SIZE`]. `under_parent_line` = a line above
/// already took the header's top inset. `on_tab` moves focus into the
/// description (web EXP-10 parity) and answers whether it did — only then is
/// the Tab consumed; Shift+Enter is swallowed so no keyboard path puts a
/// newline in a title (EXP-230).
pub(crate) fn title_input_row(
    input: &gpui::Entity<gpui_component::input::TextareaState>,
    under_parent_line: bool,
    on_tab: impl Fn(&mut Window, &mut App) -> bool + 'static,
) -> gpui::Div {
    use gpui_component::input::{self, Textarea};
    let top = if under_parent_line {
        0.
    } else {
        TITLE_PT - TITLE_WIDGET_PY
    };
    div()
        // EXP-877: the multi-line widget insets its text box by
        // `TITLE_WIDGET_PX` / `TITLE_WIDGET_PY` underneath any refined style
        // (no public size knob on `Textarea`), so the wrapper gives that much
        // back on the sides and on top, and pulls the bottom in to `TITLE_PB`
        // with a negative margin: title at `WORK_GUTTER` / `TITLE_PT`,
        // `TITLE_PB` under it.
        .px(px(WORK_GUTTER - TITLE_WIDGET_PX))
        .pt(px(top))
        .pb(px(0.))
        .mb(px(TITLE_PB - TITLE_WIDGET_PY))
        // Tab jumps from the title into the description editor. Capture runs
        // before the input's own Tab handling; Shift+Tab (`OutdentInline`) is
        // a different action, so it keeps its default behavior.
        .capture_action(move |_: &input::IndentInline, window, cx| {
            if on_tab(window, cx) {
                cx.stop_propagation();
            }
        })
        // Shift+Enter would insert a newline in the auto-grow input
        // (`submit_on_enter` only intercepts plain Enter) — swallow it.
        .capture_action(|action: &input::Enter, _window, cx| {
            if action.shift {
                cx.stop_propagation();
            }
        })
        // Style the INPUT itself (EXP-181): the widget's own text size and
        // padding override wrapper styles; `refine_style` runs last, so these
        // win. The explicit line height lifts the widget's fixed 1.25rem
        // (which would clip 24px glyphs), and h_auto releases its fixed box.
        .child(
            Textarea::new(input)
                .appearance(false)
                .text_size(px(TITLE_SIZE))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .line_height(px(TITLE_LINE))
                .px_0()
                .h_auto(),
        )
}

/// EXP-1162 — has the large title row scrolled under the floating bar? Its
/// measured content bottom (`title_bottom`, `None` until the first paint)
/// plus the body's scroll offset against the bar's bottom edge.
pub(crate) fn title_collapsed(
    body_scroll: &gpui::ScrollHandle,
    title_bottom: &std::cell::Cell<Option<f32>>,
) -> bool {
    domain::detail_chrome::is_title_collapsed(
        true,
        title_bottom
            .get()
            .map(|bottom| f64::from(bottom + f32::from(body_scroll.offset().y))),
        f64::from(BAR_H),
    )
}

/// EXP-1191 — whether the Issue face's tray reached the bar's bottom edge
/// (its measured content top + the scroll offset vs `BAR_H`): from there on
/// it stays PINNED under the bar, the web `WorkStickyTray`.
pub(crate) fn tray_pinned(
    body_scroll: &gpui::ScrollHandle,
    tray_top: &std::cell::Cell<Option<f32>>,
) -> bool {
    tray_top
        .get()
        .is_some_and(|top| top + f32::from(body_scroll.offset().y) <= BAR_H)
}

/// The measuring state a floating title row needs: the body's scroll handle,
/// the row's measured content bottom, the tray's measured content top and
/// the bar cluster's measured width — all owned by the view (they outlive
/// one frame).
pub(crate) struct TitleChrome<'a> {
    pub body_scroll: &'a gpui::ScrollHandle,
    pub title_bottom: &'a Rc<std::cell::Cell<Option<f32>>>,
    /// EXP-1191: the tray slot's content top ([`tray_pinned`]) and its
    /// height (the spacer it leaves in the body while pinned).
    pub tray_top: &'a Rc<std::cell::Cell<Option<f32>>>,
    pub tray_h: &'a Rc<std::cell::Cell<Option<f32>>>,
    pub cluster_w: &'a Rc<std::cell::Cell<Option<f32>>>,
    /// The view to repaint when a measure moves the break.
    pub entity_id: gpui::EntityId,
}

/// EXP-1162 — the issue face's floating header AND the rows it hands to the
/// scrolling body: `title` (the parent line + the large title) shifted so
/// its first line centres on the bar and kept clear of the cluster, then
/// `below` (the property tray, the merge caption). Returns `(bar, rows)`;
/// the bar is [`render_floating_bar`] over `compact` (the collapsed title,
/// shown only once `collapsed`) and `right` (the cluster).
///
/// EXP-1191: the tray stays in view on every face. `pinned` (the view's
/// [`tray_pinned`]) is the frame its slot reached the bar: from there the
/// ONE tray rides the bar, under it (gpui has no `position: sticky`; the
/// tray holds entities that render once), and the body keeps a spacer of
/// its measured height, so nothing moves.
#[allow(clippy::too_many_arguments)]
pub(crate) fn scrolling_title_rows(
    chrome: TitleChrome<'_>,
    collapsed: bool,
    title: Vec<AnyElement>,
    tray: AnyElement,
    extra: Vec<AnyElement>,
    compact: Option<AnyElement>,
    right: Vec<AnyElement>,
    pinned: bool,
) -> (AnyElement, AnyElement) {
    // The row's bottom edge, measured as it paints. The canvas reports it in
    // content space for the next render and, when the break it implies
    // differs from the one this frame was rendered with (a scroll offset
    // restored before the first measure, a title that grew a line), asks for
    // ONE repaint — the only notify.
    let probe = {
        let handle = chrome.body_scroll.clone();
        let cell = chrome.title_bottom.clone();
        let entity_id = chrome.entity_id;
        gpui::canvas(
            move |bounds, _window, cx| {
                let viewport_top = handle.bounds().top();
                let bottom = bounds.bottom();
                cell.set(Some(f32::from(bottom - viewport_top - handle.offset().y)));
                let now = domain::detail_chrome::is_title_collapsed(
                    true,
                    Some(f64::from(f32::from(bottom))),
                    f64::from(f32::from(viewport_top) + BAR_H),
                );
                if now != collapsed {
                    cx.defer(move |cx| cx.notify(entity_id));
                }
            },
            |_, _: (), _, _| {},
        )
        .absolute()
        .size_full()
    };
    // EXP-1162 (web twin): the floating bar's cluster sits ON the title's
    // first line — the row keeps clear of the cluster's measured width and
    // shifts up so that line centres on the bar's (`TITLE_PT` + half a
    // `TITLE_LINE` → `BAR_H / 2`). Shifted, the row's box would end ABOVE the
    // bar's bottom edge and read collapsed at rest, so it keeps a `slack` pad
    // that reaches `TITLE_PB` past the bar and hands it back with a negative
    // margin: the tray does not move, the break comes after a few px of
    // scroll.
    let cluster_w = chrome.cluster_w.get().unwrap_or(CLUSTER_W_FALLBACK);
    let shift = BAR_H / 2. - TITLE_PT - TITLE_LINE / 2.;
    let slack = (BAR_H - (shift + TITLE_PT + TITLE_LINE)).max(0.);
    let title = v_flex()
        .relative()
        .w_full()
        .min_w_0()
        .mt(px(shift))
        .pb(px(slack))
        .mb(px(-slack))
        .pr(px(cluster_w + CLUSTER_GAP))
        .children(title)
        .child(probe);
    // EXP-1191: the tray slot's top edge (and, unpinned, its height),
    // measured like the title's bottom; a frame that crossed the pin line
    // asks for one repaint.
    let tray_probe = {
        let handle = chrome.body_scroll.clone();
        let top_cell = chrome.tray_top.clone();
        let h_cell = chrome.tray_h.clone();
        let entity_id = chrome.entity_id;
        gpui::canvas(
            move |bounds, _window, cx| {
                let viewport_top = handle.bounds().top();
                let top = bounds.top();
                top_cell.set(Some(f32::from(top - viewport_top - handle.offset().y)));
                if !pinned {
                    h_cell.set(Some(f32::from(bounds.size.height)));
                }
                let now = f32::from(top - viewport_top) <= BAR_H;
                if now != pinned {
                    cx.defer(move |cx| cx.notify(entity_id));
                }
            },
            |_, _: (), _, _| {},
        )
        .absolute()
        .size_full()
    };
    let tray_h = chrome.tray_h.get();
    let (slot, pinned_tray) = match (pinned, tray_h) {
        (true, Some(height)) => (div().h(px(height)), Some(tray)),
        _ => (div().child(tray), None),
    };
    let slot = slot.relative().w_full().min_w_0().child(tray_probe);
    let rows = v_flex()
        .w_full()
        .min_w_0()
        .child(title)
        .child(slot)
        .children(extra)
        .into_any_element();
    // At rest the cluster reports its width for the title row above.
    let measure = (!collapsed).then(|| (chrome.cluster_w.clone(), chrome.entity_id));
    let header = render_floating_bar(
        compact,
        right,
        collapsed,
        measure,
        pinned_tray,
    );
    (header, rows)
}

/// EXP-417/EXP-568 — the property TRAY under the title: ONE plain row
/// (EXP-1191: no card chrome, the first chip on the title's left edge), the
/// property chips growing from the left, `actions` floating on its right
/// edge (`ml_auto`, so they keep their distance even after wrapping).
/// `flex_1 + min_w_0` give the tray the column's definite width, which is
/// what its own `flex_wrap` wraps the chips against.
pub(crate) fn property_tray(chips: Vec<AnyElement>, actions: Vec<AnyElement>) -> AnyElement {
    let properties = crate::surface::property_row()
        .children(chips)
        .when(!actions.is_empty(), |tray| {
            tray.child(
                h_flex()
                    .ml_auto()
                    .flex_shrink_0()
                    .items_center()
                    .gap_1()
                    .children(actions),
            )
        });
    h_flex()
        .w_full()
        .items_center()
        .px(px(WORK_GUTTER))
        // Web `pt-3` between the title row and the tray.
        .pt(px(12.))
        .child(properties.flex_1().min_w_0())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-926 / FEED-45 — ONE size rule per placement, pinned as a rule
    /// rather than as a pixel: in the bar the actions wear the bar's own
    /// rung, in the tray they wear the chips'.
    #[test]
    fn a_header_action_takes_its_size_from_its_placement() {
        assert_eq!(header_action_size(false), PillSize::Lg);
        assert_eq!(header_action_size(true), PillSize::Sm);
        // In the bar means the bar's 36px rung (web `h-9`).
        assert_eq!(
            header_action_size(false).height(),
            theme::tokens::size::CONTROL_LG
        );
        // In the tray means the CHIPS' — what `pickers::chip_button` wears.
        assert_eq!(
            header_action_size(true).height(),
            theme::tokens::size::CONTROL_SM
        );
    }

    /// EXP-889 — the tray's pills are ONE box. The reporter's Merge PR pill
    /// stood a rung taller (and a type size bigger) than the status /
    /// priority / label / Stop pills beside it, so this DRAWS the tray and
    /// measures every capsule in it: the merge pill must be exactly
    /// [`PillSize::Sm`] tall, like every sibling.
    ///
    /// The probe is a `canvas` sized `absolute().size_full()` inside a
    /// wrapper around each pill: it reports the wrapper's laid-out box, i.e.
    /// the pill's own.
    #[gpui::test]
    async fn every_tray_pill_is_the_same_box(cx: &mut gpui::TestAppContext) {
        use std::cell::Cell;
        use std::rc::Rc;

        #[derive(Default)]
        struct Measured {
            chip: Rc<Cell<f32>>,
            stop: Rc<Cell<f32>>,
            merge: Rc<Cell<f32>>,
        }

        struct Tray(Measured);

        fn probe(out: Rc<Cell<f32>>, child: AnyElement) -> AnyElement {
            div()
                .relative()
                .flex_shrink_0()
                .child(child)
                .child(
                    gpui::canvas(
                        move |bounds, _, _| out.set(f32::from(bounds.size.height)),
                        |_, _: (), _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .into_any_element()
        }

        impl gpui::Render for Tray {
            fn render(
                &mut self,
                _window: &mut Window,
                cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                // The real tray, the real chips, the real trailing cluster
                // (`issue_header::chip_row`).
                crate::surface::property_row()
                    .child(probe(
                        self.0.chip.clone(),
                        crate::pickers::chip_button("prop-status", cx)
                            .label("In Review")
                            .into_any_element(),
                    ))
                    .child(
                        h_flex()
                            .ml_auto()
                            .flex_shrink_0()
                            .items_center()
                            .gap_1()
                            .child(probe(
                                self.0.merge.clone(),
                                merge_pill(
                                    "header-merge-pr",
                                    "pr-1",
                                    true,
                                    header_action_size(true),
                                    cx,
                                ),
                            ))
                            .child(probe(
                                self.0.stop.clone(),
                                crate::session_screen::stop_session_pill(
                                    "work-stop",
                                    header_action_size(true),
                                    cx,
                                )
                                .into_any_element(),
                            )),
                    )
            }
        }

        let measured = Measured::default();
        let (chip, stop, merge) = (
            measured.chip.clone(),
            measured.stop.clone(),
            measured.merge.clone(),
        );
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (_view, cx) = cx.add_window_view(|_, _| Tray(measured));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let expected = PillSize::Sm.height();
        assert!(
            (chip.get() - expected).abs() < 2.,
            "a property chip is the Sm rung: {} vs {expected}",
            chip.get()
        );
        assert!(
            (stop.get() - expected).abs() < 2.,
            "the Stop pill is the Sm rung: {} vs {expected}",
            stop.get()
        );
        assert_eq!(
            merge.get(),
            stop.get(),
            "EXP-889: the Merge pill is the SAME box as the Stop pill beside it",
        );
        assert!(
            (merge.get() - expected).abs() < 2.,
            "EXP-889: the Merge pill is the Sm rung: {} vs {expected}",
            merge.get()
        );
    }

    /// EXP-926 / FEED-45 — the OTHER placement, drawn: in the bar (the
    /// session header) Merge PR and Stop are the bar's own Lg box —
    /// chip-sized capsules there read as strays. Same probe as the tray's
    /// test.
    #[gpui::test]
    async fn the_actions_in_the_bar_are_the_bar_rung(cx: &mut gpui::TestAppContext) {
        use std::cell::Cell;
        use std::rc::Rc;

        #[derive(Default)]
        struct Measured {
            stop: Rc<Cell<f32>>,
            merge: Rc<Cell<f32>>,
        }

        struct Cluster(Measured);

        fn probe(out: Rc<Cell<f32>>, child: AnyElement) -> AnyElement {
            div()
                .relative()
                .flex_shrink_0()
                .child(child)
                .child(
                    gpui::canvas(
                        move |bounds, _, _| out.set(f32::from(bounds.size.height)),
                        |_, _: (), _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .into_any_element()
        }

        impl gpui::Render for Cluster {
            fn render(
                &mut self,
                _window: &mut Window,
                cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                // A bar right cluster (`render_floating_bar`'s).
                h_flex()
                    .items_center()
                    .gap_1()
                    .child(probe(
                        self.0.merge.clone(),
                        merge_pill("session-merge", "pr-1", true, header_action_size(false), cx),
                    ))
                    .child(probe(
                        self.0.stop.clone(),
                        crate::session_screen::stop_session_pill(
                            "work-stop",
                            header_action_size(false),
                            cx,
                        )
                        .into_any_element(),
                    ))
            }
        }

        let measured = Measured::default();
        let (stop, merge) = (measured.stop.clone(), measured.merge.clone());
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (_view, cx) = cx.add_window_view(|_, _| Cluster(measured));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let expected = crate::controls::CTL_LG_H;
        assert!(
            (merge.get() - expected).abs() < 2.,
            "EXP-926: the Merge pill is the bar's rung: {} vs {expected}",
            merge.get()
        );
        assert!(
            (stop.get() - expected).abs() < 2.,
            "EXP-926: the Stop pill is the bar's rung: {} vs {expected}",
            stop.get()
        );
    }

    /// EXP-1162 (web `floating` twin): the Issue face's bar takes NO row —
    /// laid over a body it leaves that body's height alone, at rest and
    /// collapsed — and it reports its cluster's width only when asked.
    #[gpui::test]
    async fn the_floating_bar_takes_no_row_and_measures_its_cluster(
        cx: &mut gpui::TestAppContext,
    ) {
        use std::cell::Cell;
        use std::rc::Rc;

        struct Face {
            collapsed: bool,
            cluster: Rc<Cell<Option<f32>>>,
            height: Rc<Cell<f32>>,
        }

        impl gpui::Render for Face {
            fn render(
                &mut self,
                _window: &mut Window,
                cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                let height = self.height.clone();
                let stop = crate::session_screen::stop_session_pill(
                    "work-stop",
                    header_action_size(false),
                    cx,
                )
                .into_any_element();
                let title = self.collapsed.then(|| {
                    collapsed_title(Some("EXP-1162".into()), "Title", None, cx)
                });
                let measure = (!self.collapsed).then(|| (self.cluster.clone(), cx.entity_id()));
                div().w(px(640.)).child(
                    div()
                        .relative()
                        .w_full()
                        .child(div().h(px(300.)))
                        .child(render_floating_bar(title, vec![stop], self.collapsed, measure, None))
                        .child(
                            gpui::canvas(
                                move |bounds, _, _| height.set(f32::from(bounds.size.height)),
                                |_, _: (), _, _| {},
                            )
                            .absolute()
                            .size_full(),
                        ),
                )
            }
        }

        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        assert_eq!(BAR_H, theme::tokens::size::CONTROL_LG + 2. * BAR_PY);
        for collapsed in [false, true] {
            let (cluster, height) = (Rc::new(Cell::new(None)), Rc::new(Cell::new(0.)));
            let (_view, cx) = cx.add_window_view(|_, _| Face {
                collapsed,
                cluster: cluster.clone(),
                height: height.clone(),
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(height.get(), 300., "the bar adds no row (collapsed: {collapsed})");
            match cluster.get() {
                Some(width) => {
                    assert!(!collapsed, "only a resting bar measures its cluster");
                    let pill = header_action_size(false).height();
                    assert!(width >= pill, "the cluster is at least its pill: {width}");
                }
                None => assert!(collapsed, "a resting bar measures its cluster"),
            }
        }
    }

    /// EXP-1162: the edge strips and the break's motion are the CONTRACT's
    /// numbers, read through the domain mirror of `detail-chrome.json`.
    #[test]
    fn the_edge_strips_wear_the_contract_sizes() {
        assert_eq!(domain::detail_chrome::EDGE_TOP, 24.);
        assert_eq!(domain::detail_chrome::EDGE_BOTTOM, 32.);
        assert_eq!(domain::detail_chrome::COLLAPSE_MS, 160);
        assert_eq!(domain::detail_chrome::COLLAPSE_RISE, 4.);
    }

    /// EXP-895: a diff count prints the CONTRACT's labels through the
    /// ONE counts renderer ([`crate::diff_pane::counts`]) — `+N` and a U+2212
    /// MINUS SIGN, never an ASCII hyphen, on every desktop surface.
    #[test]
    fn the_diff_label_uses_the_minus_sign() {
        assert_eq!(domain::diff::additions_label(12), "+12");
        assert_eq!(domain::diff::deletions_label(2), "\u{2212}2");
        assert!(!domain::diff::deletions_label(2).contains('-'));
    }
}
