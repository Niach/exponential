//! Floating window placement and Mint-like edge snapping, as pure geometry.
//!
//! Nothing here touches Wayland: the compositor feeds rectangles in and maps
//! windows where these functions say. That keeps the rules testable without a
//! display (`cargo test` runs them headless).

use smithay::utils::{Logical, Point, Rectangle, Size};

pub type Rect = Rectangle<i32, Logical>;

/// Where a window dropped at a screen edge tiles to (Cinnamon's edge tiling:
/// the sides take half the work area, the corners a quarter, the top edge
/// maximizes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile {
    Maximized,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Tile {
    /// The rectangle this tile covers inside `area` (the output's work area,
    /// i.e. without exclusive layer-shell zones such as a panel).
    pub fn rect(self, area: Rect) -> Rect {
        let (x, y) = (area.loc.x, area.loc.y);
        let (w, h) = (area.size.w, area.size.h);
        let (lw, th) = (w / 2, h / 2);
        let (rw, bh) = (w - lw, h - th);
        let r = |x, y, w, h| Rectangle::new((x, y).into(), (w, h).into());
        match self {
            Tile::Maximized => area,
            Tile::Left => r(x, y, lw, h),
            Tile::Right => r(x + lw, y, rw, h),
            Tile::TopLeft => r(x, y, lw, th),
            Tile::TopRight => r(x + lw, y, rw, th),
            Tile::BottomLeft => r(x, y + th, lw, bh),
            Tile::BottomRight => r(x + lw, y + th, rw, bh),
        }
    }
}

/// The tile a pointer at `pointer` asks for when a move ends there: within
/// `edge` px of a side of `area` (the corners take `corner` px along the
/// edge). `None` = an ordinary floating drop.
pub fn tile_zone(pointer: Point<f64, Logical>, area: Rect, edge: i32, corner: i32) -> Option<Tile> {
    let (px, py) = (pointer.x.floor() as i32, pointer.y.floor() as i32);
    let left = px < area.loc.x + edge;
    let right = px >= area.loc.x + area.size.w - edge;
    let top = py < area.loc.y + edge;
    let near_top = py < area.loc.y + corner;
    let near_bottom = py >= area.loc.y + area.size.h - corner;
    let near_left = px < area.loc.x + corner;
    let near_right = px >= area.loc.x + area.size.w - corner;
    match () {
        _ if left && near_top => Some(Tile::TopLeft),
        _ if left && near_bottom => Some(Tile::BottomLeft),
        _ if right && near_top => Some(Tile::TopRight),
        _ if right && near_bottom => Some(Tile::BottomRight),
        _ if top && near_left => Some(Tile::TopLeft),
        _ if top && near_right => Some(Tile::TopRight),
        _ if left => Some(Tile::Left),
        _ if right => Some(Tile::Right),
        _ if top => Some(Tile::Maximized),
        _ => None,
    }
}

/// Magnetic edges while a window moves: if an edge of `rect` comes within
/// `threshold` px of an edge of the work area or of another window, it
/// snaps flush. Returns the corrected location.
pub fn magnetic(rect: Rect, area: Rect, others: &[Rect], threshold: i32) -> Point<i32, Logical> {
    if threshold <= 0 {
        return rect.loc;
    }
    // Candidate x positions for the window's left edge, y for its top edge.
    let mut xs = vec![area.loc.x, area.loc.x + area.size.w - rect.size.w];
    let mut ys = vec![area.loc.y, area.loc.y + area.size.h - rect.size.h];
    for o in others {
        let overlaps_y = rect.loc.y < o.loc.y + o.size.h && o.loc.y < rect.loc.y + rect.size.h;
        let overlaps_x = rect.loc.x < o.loc.x + o.size.w && o.loc.x < rect.loc.x + rect.size.w;
        if overlaps_y {
            // our left to their right, our right to their left
            xs.push(o.loc.x + o.size.w);
            xs.push(o.loc.x - rect.size.w);
        }
        if overlaps_x {
            ys.push(o.loc.y + o.size.h);
            ys.push(o.loc.y - rect.size.h);
        }
    }
    let nearest = |cur: i32, cands: &[i32]| {
        cands
            .iter()
            .copied()
            .filter(|c| (c - cur).abs() <= threshold)
            .min_by_key(|c| (c - cur).abs())
            .unwrap_or(cur)
    };
    (nearest(rect.loc.x, &xs), nearest(rect.loc.y, &ys)).into()
}

/// Where a new window of `size` opens: centred in the work area, cascaded by
/// `step` px while another window already sits at that spot, and clamped so
/// its top-left stays on screen.
pub fn place_new(size: Size<i32, Logical>, area: Rect, existing: &[Point<i32, Logical>], step: i32) -> Point<i32, Logical> {
    let cx = area.loc.x + (area.size.w - size.w).max(0) / 2;
    let cy = area.loc.y + (area.size.h - size.h).max(0) / 2;
    let mut p: Point<i32, Logical> = (cx, cy).into();
    let max_x = area.loc.x + (area.size.w - size.w).max(0);
    let max_y = area.loc.y + (area.size.h - size.h).max(0);
    for _ in 0..32 {
        if !existing.contains(&p) {
            break;
        }
        p = (p.x + step, p.y + step).into();
        if p.x > max_x || p.y > max_y {
            p = (area.loc.x, area.loc.y).into();
        }
    }
    p
}

/// A tiled window dragged away goes back to its floating size, keeping the
/// grab point at the same fraction of the title width so the window doesn't
/// jump out from under the pointer.
pub fn untile_location(pointer: Point<f64, Logical>, tiled: Rect, restore: Size<i32, Logical>) -> Point<i32, Logical> {
    let frac = if tiled.size.w > 0 {
        ((pointer.x - tiled.loc.x as f64) / tiled.size.w as f64).clamp(0.0, 1.0)
    } else {
        0.5
    };
    let x = pointer.x - frac * restore.w as f64;
    (x.round() as i32, tiled.loc.y).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rectangle::new((x, y).into(), (w, h).into())
    }

    const AREA: (i32, i32, i32, i32) = (0, 0, 1920, 1080);

    fn area() -> Rect {
        r(AREA.0, AREA.1, AREA.2, AREA.3)
    }

    #[test]
    fn tiles_split_the_work_area_without_gaps() {
        let a = r(0, 40, 1921, 1041); // odd sizes: the right/bottom halves take the extra pixel
        assert_eq!(Tile::Left.rect(a), r(0, 40, 960, 1041));
        assert_eq!(Tile::Right.rect(a), r(960, 40, 961, 1041));
        assert_eq!(Tile::TopLeft.rect(a), r(0, 40, 960, 520));
        assert_eq!(Tile::BottomRight.rect(a), r(960, 560, 961, 521));
        assert_eq!(Tile::Maximized.rect(a), a);
    }

    #[test]
    fn edges_pick_halves_corners_and_maximize() {
        let z = |x: f64, y: f64| tile_zone((x, y).into(), area(), 4, 120);
        assert_eq!(z(0.0, 500.0), Some(Tile::Left));
        assert_eq!(z(1919.0, 500.0), Some(Tile::Right));
        assert_eq!(z(900.0, 0.0), Some(Tile::Maximized));
        assert_eq!(z(0.0, 10.0), Some(Tile::TopLeft));
        assert_eq!(z(10.0, 0.0), Some(Tile::TopLeft));
        assert_eq!(z(1919.0, 1079.0), Some(Tile::BottomRight));
        assert_eq!(z(0.0, 1000.0), Some(Tile::BottomLeft));
        assert_eq!(z(900.0, 500.0), None);
        // the bottom edge alone is not a tile (Cinnamon leaves it to the panel)
        assert_eq!(z(900.0, 1079.0), None);
    }

    #[test]
    fn edges_follow_the_work_area_not_the_output() {
        let panel_free = r(0, 0, 1920, 1040);
        assert_eq!(tile_zone((900.0, 1038.0).into(), panel_free, 4, 120), None);
        assert_eq!(tile_zone((1917.0, 1038.0).into(), panel_free, 4, 120), Some(Tile::BottomRight));
    }

    #[test]
    fn windows_stick_to_screen_edges_within_the_threshold() {
        assert_eq!(magnetic(r(9, 300, 400, 300), area(), &[], 12), (0, 300).into());
        assert_eq!(magnetic(r(13, 300, 400, 300), area(), &[], 12), (13, 300).into());
        assert_eq!(magnetic(r(1515, 775, 400, 300), area(), &[], 12), (1520, 780).into());
        assert_eq!(magnetic(r(9, 9, 400, 300), area(), &[], 0), (9, 9).into());
    }

    #[test]
    fn windows_stick_to_each_other_only_when_they_overlap_on_the_other_axis() {
        let other = r(500, 200, 300, 300);
        // our left edge near their right edge, same height band → flush
        assert_eq!(magnetic(r(806, 250, 200, 200), area(), &[other], 12), (800, 250).into());
        // our right edge near their left edge
        assert_eq!(magnetic(r(295, 250, 200, 200), area(), &[other], 12), (300, 250).into());
        // far below them: no x magnetism from that window
        assert_eq!(magnetic(r(806, 700, 200, 200), area(), &[other], 12), (806, 700).into());
    }

    #[test]
    fn new_windows_open_centred_and_cascade() {
        let size = (800, 600).into();
        let first = place_new(size, area(), &[], 32);
        assert_eq!(first, (560, 240).into());
        let second = place_new(size, area(), &[first], 32);
        assert_eq!(second, (592, 272).into());
        // bigger than the screen: pinned to the work area's corner
        assert_eq!(place_new((4000, 3000).into(), r(0, 40, 1920, 1040), &[], 32), (0, 40).into());
    }

    #[test]
    fn untiling_keeps_the_grab_point_under_the_pointer() {
        let tiled = r(0, 0, 960, 1080);
        let p = untile_location((480.0, 10.0).into(), tiled, (600, 400).into());
        assert_eq!(p, (180, 0).into());
    }
}
