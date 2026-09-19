//! EXP-965 — painting [`domain::tree_guides`] over a nested list row.
//!
//! The facts are pure and shared ×4; only the geometry is ours. A row that
//! nests draws its connector in the gutter its INDENT already reserves — the
//! 14px band per level `row_shell` pads with — so the lines land exactly on
//! the column the parent's leading glyph sits in and nothing shifts.
//!
//! One absolutely-positioned paint-only `canvas` per row (the
//! `source_control::gutter_cell` recipe), behind the row's own children: a
//! quad for every straight vertical (cheap) and one stroked path for the
//! rounded elbow.
//!
//! GAP BRIDGING (×4): a list that SPACES its rows would otherwise show the
//! connector in dashes — every segment stops at a row edge, and the gap
//! between two rows is nobody's. The rule: a vertical that starts at the
//! row's TOP edge starts one row-gap ABOVE it instead (the layer is absolute
//! and unclipped, so it simply paints into the gap), while tees and
//! pass-throughs still end at the BOTTOM edge — so every gap is covered
//! exactly once, by the row below it.

use gpui::{
    canvas, div, point, px, size, AnyElement, Bounds, IntoElement, ParentElement, Pixels, Styled,
};

use domain::tree_guides::Guides;

/// One indent level, ×4 (`row_shell`'s `14. * depth`).
pub(crate) const LEVEL_PITCH: f32 = 14.;

/// The connector's stroke width.
const LINE: f32 = 1.;

/// The elbow's corner radius.
const RADIUS: f32 = 5.;

/// Where a row's top-edge segments START, given the row's own top and the
/// list's row gap: one gap higher, so the line bridges the space above the
/// row. Pure (and trivial) on purpose — it is the ONE place the ×4 bridging
/// rule turns into a number, and getting its SIGN wrong is a dashed line
/// nobody can explain.
fn segment_top(row_top: f32, gap: f32) -> f32 {
    row_top - gap.max(0.)
}

/// The x of level `level`'s gutter CENTRE, given the list's base left pad —
/// the middle of the 14px band between `pad + 14*level` and `pad + 14*(level
/// + 1)`, which is where that level's leading glyph sits.
fn gutter_center(pad: f32, level: usize) -> f32 {
    pad + LEVEL_PITCH * level as f32 + LEVEL_PITCH / 2.
}

/// The connector layer for one row, or `None` when the row is a root (depth 0
/// draws nothing). The caller puts it in a `relative()` row as the FIRST
/// child; it is `absolute` and paints only, so it never takes part in layout
/// and never eats a click.
///
/// `pad` is the list's base left padding (12 for the session rows and the
/// Reviews stack, 8 for the PR-graph overlay); `gap` is the vertical spacing
/// that list puts BETWEEN its rows, which the top-edge segments extend over
/// (0 for the flat lists that stack with none).
pub(crate) fn guide_layer(guides: &Guides, pad: f32, gap: f32) -> Option<AnyElement> {
    let Some(elbow) = guides.elbow_at else {
        return None;
    };
    let tee = guides.tee;
    let pass: Vec<f32> = guides
        .pass_through
        .iter()
        .map(|&level| gutter_center(pad, level))
        .collect();
    let elbow_x = gutter_center(pad, elbow);
    // The stub ends at the gutter's right edge — just before the child's own
    // leading glyph.
    let stub_end = pad + LEVEL_PITCH * (elbow + 1) as f32;
    let color = theme::tokens::glass::STROKE_STRONG.to_hsla();
    Some(
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right_0()
            .child(
                canvas(|_, _, _| (), move |bounds: Bounds<Pixels>, _, window, _| {
                    // The top-edge segments start a row-gap HIGHER, so the
                    // line carries across the space between two rows; the
                    // bottom stays the row's own edge, and the row below
                    // covers the rest.
                    let top = px(segment_top(f32::from(bounds.origin.y), gap));
                    let bottom = bounds.origin.y + bounds.size.height;
                    let mid = bounds.origin.y + bounds.size.height / 2.;
                    let vertical = |window: &mut gpui::Window, x: f32, from: Pixels, to: Pixels| {
                        if to <= from {
                            return;
                        }
                        window.paint_quad(gpui::fill(
                            Bounds::new(
                                point(bounds.origin.x + px(x - LINE / 2.), from),
                                size(px(LINE), to - from),
                            ),
                            color,
                        ));
                    };
                    // Ancestors whose subtree continues past this row.
                    for x in &pass {
                        vertical(window, *x, top, bottom);
                    }
                    // The elbow's vertical: to the row's bottom edge when
                    // another sibling follows, else into the corner.
                    let end = if tee { bottom } else { mid - px(RADIUS) };
                    vertical(window, elbow_x, top, end);
                    // The corner and the stub — one stroked path, so the
                    // quarter-turn is round rather than mitred.
                    let mut path = gpui::PathBuilder::stroke(px(LINE));
                    let x = bounds.origin.x + px(elbow_x);
                    path.move_to(point(x, mid - px(RADIUS)));
                    path.curve_to(point(x + px(RADIUS), mid), point(x, mid));
                    path.line_to(point(bounds.origin.x + px(stub_end), mid));
                    if let Ok(path) = path.build() {
                        window.paint_path(path, color);
                    }
                })
                .size_full(),
            )
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The line sits on the PARENT glyph's column: level 0's centre is half a
    /// pitch past the list's base pad, and the stub ends one whole pitch on
    /// (just before the child's own glyph).
    #[test]
    fn a_gutter_centre_is_the_parent_glyph_column() {
        assert_eq!(gutter_center(12., 0), 19.);
        assert_eq!(gutter_center(12., 1), 33.);
        assert_eq!(gutter_center(8., 0), 15.);
        // A depth-1 row pads to 26; its elbow's stub ends there.
        assert_eq!(12. + LEVEL_PITCH, 26.);
    }

    /// EXP-965 gap bridging: a top-edge segment starts one row-gap ABOVE the
    /// row, so a spaced list draws one line rather than a dashed one. A list
    /// that stacks its rows flush (gap 0) is unchanged, and a nonsense
    /// negative gap never pushes the start DOWN into the row.
    #[test]
    fn a_top_segment_bridges_the_row_gap() {
        assert_eq!(segment_top(100., 0.), 100.);
        assert_eq!(segment_top(100., 3.5), 96.5);
        assert_eq!(segment_top(100., 1.75), 98.25);
        assert_eq!(segment_top(100., -4.), 100.);
    }

    /// A root row paints nothing at all.
    #[test]
    fn a_root_row_has_no_layer() {
        assert!(guide_layer(&Guides::default(), 12., 0.).is_none());
    }
}
