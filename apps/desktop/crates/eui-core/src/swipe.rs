//! FEED-200 (fixtures/swipe.json): the pure rules of a horizontal swipe
//! pager, the twin of packages/exponential-ui/src/swipe.ts. `Tabs.swipeable`
//! pages its panels with them (`surface::swipe`); the FFI exposes them to
//! Swift and Kotlin; FEED-201 reuses them for app screens. Offsets and
//! velocities are SCREEN px (px/ms), positive = rightwards.

/// px a touch travels before it commits to an axis.
pub const SWIPE_SLOP: f64 = 8.0;
/// Fraction of the page width a slow drag must pass to page.
pub const SWIPE_DISTANCE: f64 = 0.5;
/// px/ms a release must reach (towards the drag) to page on a fling.
pub const SWIPE_VELOCITY: f64 = 0.4;
/// How much of the finger's travel shows past the first/last page.
pub const SWIPE_RESISTANCE: f64 = 0.3;

/// Which axis a touch that moved (`dx`, `dy`) commits to: `pending` inside
/// the slop, `horizontal` when it moved more across than down, else
/// `vertical`.
pub fn swipe_lock(dx: f64, dy: f64) -> &'static str {
    let (ax, ay) = (dx.abs(), dy.abs());
    if ax.max(ay) < SWIPE_SLOP {
        "pending"
    } else if ax > ay {
        "horizontal"
    } else {
        "vertical"
    }
}

/// The index step a screen offset points at: dragging left reveals the
/// next page (the previous one under RTL); 0 = none.
pub fn swipe_step(offset: f64, rtl: bool) -> i64 {
    if offset == 0.0 {
        return 0;
    }
    let forward = if offset < 0.0 { 1 } else { -1 };
    if rtl {
        -forward
    } else {
        forward
    }
}

/// The offset the pages are DRAWN at while the finger is at `offset`
/// (resisted past an end, never more than one page).
pub fn swipe_offset(index: usize, count: usize, offset: f64, width: f64, rtl: bool) -> f64 {
    let target = index as i64 + swipe_step(offset, rtl);
    let damped = if target < 0 || target >= count as i64 { offset * SWIPE_RESISTANCE } else { offset };
    let room = width.max(0.0);
    damped.clamp(-room, room)
}

/// Which page a released swipe settles on: one step towards the drag on a
/// fling past the slop or a drag past [`SWIPE_DISTANCE`] of the width that
/// was not flung back; else the page it started on. Never past an end.
pub fn swipe_settle(index: usize, count: usize, offset: f64, velocity: f64, width: f64, rtl: bool) -> usize {
    if count == 0 {
        return 0;
    }
    let from = index.min(count - 1);
    let target = from as i64 + swipe_step(offset, rtl);
    if target == from as i64 || target < 0 || target >= count as i64 {
        return from;
    }
    let fast = velocity.abs() >= SWIPE_VELOCITY;
    let along = sign(velocity) == sign(offset);
    if fast && !along {
        return from;
    }
    if fast && offset.abs() >= SWIPE_SLOP {
        return target as usize;
    }
    if width > 0.0 && offset.abs() >= width * SWIPE_DISTANCE {
        target as usize
    } else {
        from
    }
}

fn sign(v: f64) -> i8 {
    if v > 0.0 {
        1
    } else if v < 0.0 {
        -1
    } else {
        0
    }
}

/// How far (0…1) a drawn offset carried the pager towards its neighbour.
pub fn swipe_progress(offset: f64, width: f64) -> f64 {
    if width > 0.0 {
        (offset.abs() / width).min(1.0)
    } else {
        0.0
    }
}

/// The header indicator's `(x, w)` between the active tab's and the
/// neighbour's at `progress`.
pub fn swipe_indicator(from: (f64, f64), to: (f64, f64), progress: f64) -> (f64, f64) {
    let t = progress.clamp(0.0, 1.0);
    (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)
}

/// Whether a touch starting inside a node of `component` (its `direction`
/// prop, its resolved `overflowX`) is that node's, not an enclosing
/// pager's: anything that scrolls, pans or drags sideways.
pub fn swipe_blocks(component: &str, direction: Option<&str>, overflow_x: Option<&str>) -> bool {
    if matches!(component, "Table" | "Chart" | "Carousel" | "ZoomableImage" | "Slider" | "Resizable") {
        return true;
    }
    if matches!(component, "List" | "ScrollArea") && matches!(direction, Some("horizontal" | "both")) {
        return true;
    }
    matches!(overflow_x, Some("scroll" | "auto"))
}
