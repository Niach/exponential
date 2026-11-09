//! #LOGS-6: a column of rows whose first frame lays out only the rows that
//! can reach its scroll viewport, then [`GROW_ROWS`] more per frame until it
//! holds them all.
//!
//! Rows are only ever left off at the END, as one spacer at the rows' lower
//! bound heights: every mounted row lays out in the real tree exactly as in
//! the whole column (same taffy pass, same pixel snapping), and the whole
//! column is what settles a few frames later. A row left out starts below
//! the viewport, because its top at the rows' lower bounds already does,
//! and so does everything after it.

use gpui::{div, px, AnyElement, IntoElement, Pixels, Styled as _, Window};

/// Rows a column mounts per frame after its first.
pub(crate) const GROW_ROWS: usize = 2;

pub(crate) struct LazyRow {
    pub element: AnyElement,
    /// A height the row never goes below.
    pub lower: Pixels,
    /// It, and every row above it, lays out now.
    pub force: bool,
}

/// What a column mounted last frame, kept by its view.
#[derive(Default)]
pub(crate) struct LazyRowsState {
    /// The row set it belongs to (an issue id); another key starts over.
    key: Option<String>,
    mounted: usize,
}

/// The rows to put in the column this frame: a prefix of `rows`, then one
/// spacer for the rest. `reach` = the viewport's bottom measured from the
/// column's top, at least (the column's top in its scroll content is taken
/// as the content's top).
pub(crate) fn lazy_rows(
    state: &mut LazyRowsState,
    key: Option<&str>,
    rows: Vec<LazyRow>,
    reach: Pixels,
    window: &Window,
) -> Vec<AnyElement> {
    let n = rows.len();
    let lower: Vec<Pixels> = rows.iter().map(|row| row.lower).collect();
    let mut mounted = if state.key.as_deref() == key {
        (state.mounted + GROW_ROWS).min(n)
    } else {
        first_rows(&lower, reach)
    };
    if let Some(forced) = rows.iter().rposition(|row| row.force) {
        mounted = mounted.max(forced + 1);
    }
    state.key = key.map(str::to_string);
    state.mounted = mounted;
    if mounted < n {
        window.request_animation_frame();
    }
    let spacer: Pixels = lower[mounted..].iter().copied().sum();
    let mut out: Vec<AnyElement> = rows.into_iter().take(mounted).map(|row| row.element).collect();
    if spacer > px(0.) {
        out.push(div().w_full().flex_shrink_0().h(spacer).into_any_element());
    }
    out
}

/// How many rows a first frame mounts: every row whose top, at the rows'
/// `lower` heights, is above `reach` (at least one).
fn first_rows(lower: &[Pixels], reach: Pixels) -> usize {
    let mut top = px(0.);
    lower
        .iter()
        .take_while(|height| {
            let shows = top < reach;
            top += **height;
            shows
        })
        .count()
        .max(1)
        .min(lower.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_first_frame_mounts_the_rows_whose_lower_bound_top_is_in_reach() {
        let lower = vec![px(30.); 60];
        assert_eq!(first_rows(&lower, px(900.)), 30);
        assert_eq!(first_rows(&lower, px(10_000.)), 60);
        // Rows of no known height never cut the column.
        assert_eq!(first_rows(&[px(0.); 5], px(1.)), 5);
        assert_eq!(first_rows(&[], px(900.)), 0);
    }
}
