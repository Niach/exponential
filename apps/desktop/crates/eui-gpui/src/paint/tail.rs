//! #EXP-1876: the Box style key `tail` (style.json): a speech-bubble tail
//! painted with the box as ONE outline. The numbers are every renderer's:
//! sideways 6 px out × 9 px down at the TOP corner of the inline-start
//! (`start`) or inline-end (`end`) side, that corner square; `down` 9 px
//! down, 6 px wide at the base, its straight edge (the tip) 22 px in from
//! the inline-end edge; a ~2 px rounded tip. Paint-only: it sits outside the
//! frame. gpui fills the outline in the border colour, then the same outline
//! inset by the border width in the background: one fill, one hairline, no
//! seam. [`TailCanvas`] is that paint for any element (the genUI Box and the
//! IDE's own chat bubble).

use gpui::{canvas, point, prelude::*, px, AnyElement, Bounds, Hsla, PathBuilder, Pixels, Window};

/// Which tail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tail {
    Start,
    End,
    Down,
}

impl Tail {
    /// `start | end | down`; anything else (`none`) = no tail.
    pub fn parse(s: &str) -> Option<Tail> {
        match s {
            "start" => Some(Tail::Start),
            "end" => Some(Tail::End),
            "down" => Some(Tail::Down),
            _ => None,
        }
    }

    /// The corner a sideways tail squares, `[tl, tr, br, bl]` index, under
    /// the node's direction.
    pub fn square_corner(self, rtl: bool) -> Option<usize> {
        match (self, rtl) {
            (Tail::Start, false) | (Tail::End, true) => Some(0),
            (Tail::End, false) | (Tail::Start, true) => Some(1),
            (Tail::Down, _) => None,
        }
    }
}

/// How far out (sideways) / the base (down).
pub const OUT: f32 = 6.0;
/// How far down (sideways) / how tall (down).
pub const DROP: f32 = 9.0;
/// `down`: the straight edge's distance from the inline-end edge.
pub const INSET: f32 = 22.0;
/// The tip's rounding, along each edge.
pub const TIP: f32 = 2.0;
/// The room every tail needs outside the frame (a canvas margin).
pub const MARGIN: f32 = 10.0;

/// One path op, in the box's own coordinates (the frame's top-left = 0,0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Move(f32, f32),
    Line(f32, f32),
    /// A quadratic: control, then the end point.
    Quad(f32, f32, f32, f32),
    /// A cubic: two controls, then the end point.
    Cubic(f32, f32, f32, f32, f32, f32),
}

const K: f32 = 0.552_284_8;

/// The unit hypotenuse (6 × 9).
fn hyp() -> (f32, f32) {
    let len = (OUT * OUT + DROP * DROP).sqrt();
    (OUT / len, DROP / len)
}

struct Pen(Vec<Op>);

impl Pen {
    fn corner(&mut self, from: (f32, f32), c: (f32, f32), to: (f32, f32)) {
        if from == to {
            return;
        }
        let a = (from.0 + (c.0 - from.0) * K, from.1 + (c.1 - from.1) * K);
        let b = (to.0 + (c.0 - to.0) * K, to.1 + (c.1 - to.1) * K);
        self.0.push(Op::Cubic(a.0, a.1, b.0, b.1, to.0, to.1));
    }
    fn line(&mut self, p: (f32, f32)) {
        self.0.push(Op::Line(p.0, p.1));
    }
}

/// The outline of a `w × h` box with `radii` `[tl, tr, br, bl]` and `tail`,
/// inset by `d` (0 = the outer edge, the border width = the fill's edge),
/// mirrored for rtl.
pub fn outline(tail: Tail, w: f32, h: f32, radii: [f32; 4], d: f32, rtl: bool) -> Vec<Op> {
    // Build in ltr as `start` or `down`, then mirror: `end` = a mirrored
    // `start` (its radii swapped first).
    let sideways = tail != Tail::Down;
    let mirror = match tail {
        Tail::Start => rtl,
        Tail::End => !rtl,
        Tail::Down => rtl,
    };
    let mut r = if mirror { [radii[1], radii[0], radii[3], radii[2]] } else { radii };
    if sideways {
        r[0] = 0.0;
    }
    let cap = (w.min(h) / 2.0 - d).max(0.0);
    let r = r.map(|x| (x - d).clamp(0.0, cap));
    let (dx, dy) = hyp();
    let shift = d / dy;
    let tip = (TIP - d * 0.5).max(0.5);
    let (l, t, rt, b) = (d, d, w - d, h - d);
    let mut pen = Pen(Vec::new());
    if sideways {
        // Tip at the top-left, the inset hypotenuse x(y) = -6 + y·6/9 + shift.
        let tx = -OUT + d * OUT / DROP + shift;
        let ys = ((l + OUT - shift) * DROP / OUT).min(b - r[3]);
        pen.0.push(Op::Move(tx + tip, t));
        pen.line((rt - r[1], t));
        pen.corner((rt - r[1], t), (rt, t), (rt, t + r[1]));
        pen.line((rt, b - r[2]));
        pen.corner((rt, b - r[2]), (rt, b), (rt - r[2], b));
        pen.line((l + r[3], b));
        pen.corner((l + r[3], b), (l, b), (l, b - r[3]));
        pen.line((l, ys));
        pen.line((tx + tip * dx, t + tip * dy));
        pen.0.push(Op::Quad(tx, t, tx + tip, t));
    } else {
        // The straight edge at x = w - 22 (inset by d), the hypotenuse from
        // (w - 28, h) to the tip, inset: x(y) = w - 28 + (y - h)·6/9 + shift.
        let ex = w - INSET - d;
        let hx = |y: f32| w - INSET - OUT + (y - h) * OUT / DROP + shift;
        let ty = h + (ex - (w - INSET - OUT) - shift) * DROP / OUT;
        pen.0.push(Op::Move(l + r[0], t));
        pen.line((rt - r[1], t));
        pen.corner((rt - r[1], t), (rt, t), (rt, t + r[1]));
        pen.line((rt, b - r[2]));
        pen.corner((rt, b - r[2]), (rt, b), (rt - r[2], b));
        pen.line((ex, b));
        pen.line((ex, ty - tip));
        pen.0.push(Op::Quad(ex, ty, ex - tip * dx, ty - tip * dy));
        pen.line((hx(b), b));
        pen.line((l + r[3], b));
        pen.corner((l + r[3], b), (l, b), (l, b - r[3]));
        pen.line((l, t + r[0]));
        pen.corner((l, t + r[0]), (l, t), (l + r[0], t));
    }
    if mirror {
        let m = |x: f32| w - x;
        pen.0 = pen
            .0
            .into_iter()
            .map(|op| match op {
                Op::Move(x, y) => Op::Move(m(x), y),
                Op::Line(x, y) => Op::Line(m(x), y),
                Op::Quad(cx, cy, x, y) => Op::Quad(m(cx), cy, m(x), y),
                Op::Cubic(ax, ay, bx, by, x, y) => Op::Cubic(m(ax), ay, m(bx), by, m(x), y),
            })
            .collect();
    }
    pen.0
}

fn fill(window: &mut Window, origin: (f32, f32), ops: &[Op], color: Hsla) {
    let p = |x: f32, y: f32| point(px(origin.0 + x), px(origin.1 + y));
    let mut b = PathBuilder::fill();
    for op in ops {
        match *op {
            Op::Move(x, y) => b.move_to(p(x, y)),
            Op::Line(x, y) => b.line_to(p(x, y)),
            Op::Quad(cx, cy, x, y) => b.curve_to(p(x, y), p(cx, cy)),
            Op::Cubic(ax, ay, bx, by, x, y) => b.cubic_bezier_to(p(x, y), p(ax, ay), p(bx, by)),
        }
    }
    b.close();
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

/// Paint a tailed box whose frame starts at `origin` (window px).
#[allow(clippy::too_many_arguments)]
pub fn paint(window: &mut Window, origin: (f32, f32), w: f32, h: f32, radii: [f32; 4], tail: Tail, rtl: bool, bg: Option<Hsla>, border: Option<(f32, Hsla)>) {
    match border.filter(|(b, c)| *b > 0.0 && c.a > 0.0) {
        Some((b, color)) => {
            fill(window, origin, &outline(tail, w, h, radii, 0.0, rtl), color);
            if let Some(bg) = bg {
                fill(window, origin, &outline(tail, w, h, radii, b, rtl), bg);
            }
        }
        None => {
            if let Some(bg) = bg {
                fill(window, origin, &outline(tail, w, h, radii, 0.0, rtl), bg);
            }
        }
    }
}

/// The box + tail as an absolutely placed canvas child of the box it paints
/// (it reaches [`MARGIN`] past each edge; the box itself paints no fill and
/// no border). Its size follows the box at paint time.
pub fn tail_canvas(tail: Tail, rtl: bool, radii: [f32; 4], bg: Option<Hsla>, border: Option<(f32, Hsla)>) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let (x, y) = (f32::from(bounds.origin.x) + MARGIN, f32::from(bounds.origin.y) + MARGIN);
            let (w, h) = (f32::from(bounds.size.width) - 2.0 * MARGIN, f32::from(bounds.size.height) - 2.0 * MARGIN);
            if w > 0.0 && h > 0.0 {
                paint(window, (x, y), w, h, radii, tail, rtl, bg, border);
            }
        },
    )
    .absolute()
    .top(px(-MARGIN))
    .left(px(-MARGIN))
    .right(px(-MARGIN))
    .bottom(px(-MARGIN))
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points(ops: &[Op]) -> Vec<(f32, f32)> {
        ops.iter()
            .map(|op| match *op {
                Op::Move(x, y) | Op::Line(x, y) | Op::Quad(_, _, x, y) | Op::Cubic(_, _, _, _, x, y) => (x, y),
            })
            .collect()
    }

    fn near(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01
    }

    #[test]
    fn start_tip_sits_six_out_on_the_top_edge_and_joins_the_side_nine_down() {
        let ops = outline(Tail::Start, 120.0, 40.0, [16.0; 4], 0.0, false);
        assert_eq!(ops[0], Op::Move(-4.0, 0.0));
        let Op::Quad(cx, cy, x, y) = *ops.last().unwrap() else { panic!("ends on the tip") };
        assert!(near((cx, cy), (-6.0, 0.0)) && near((x, y), (-4.0, 0.0)));
        assert!(points(&ops).iter().any(|p| near(*p, (0.0, 9.0))), "the side join");
        // The square corner: the top edge runs straight from the tip.
        assert!(points(&ops).iter().all(|p| p.0 >= -6.0 && p.1 >= 0.0));
    }

    #[test]
    fn end_mirrors_start_and_rtl_mirrors_both() {
        let w = 120.0;
        let start = points(&outline(Tail::Start, w, 40.0, [16.0; 4], 0.0, false));
        let end = points(&outline(Tail::End, w, 40.0, [16.0; 4], 0.0, false));
        for (a, b) in start.iter().zip(&end) {
            assert!(near((w - a.0, a.1), *b));
        }
        assert_eq!(outline(Tail::Start, w, 40.0, [16.0; 4], 0.0, true), outline(Tail::End, w, 40.0, [16.0; 4], 0.0, false));
        assert_eq!(Tail::Start.square_corner(false), Some(0));
        assert_eq!(Tail::End.square_corner(false), Some(1));
        assert_eq!(Tail::Start.square_corner(true), Some(1));
        assert_eq!(Tail::Down.square_corner(false), None);
    }

    #[test]
    fn down_tip_is_nine_below_twenty_two_in_from_the_end() {
        let (w, h) = (200.0, 40.0);
        let ops = outline(Tail::Down, w, h, [16.0; 4], 0.0, false);
        let tip = ops.iter().find_map(|op| if let Op::Quad(cx, cy, _, _) = *op { Some((cx, cy)) } else { None }).unwrap();
        assert!(near(tip, (w - 22.0, h + 9.0)));
        assert!(points(&ops).iter().any(|p| near(*p, (w - 28.0, h))), "the base, 6 wide");
    }

    #[test]
    fn the_inset_outline_stays_inside_the_outer_one() {
        let ops = outline(Tail::Start, 120.0, 40.0, [16.0; 4], 1.0, false);
        assert_eq!(ops[0], Op::Move(-6.0 + 1.0 * 6.0 / 9.0 + (6f32.hypot(9.0) / 9.0) + 1.5, 1.0));
        assert!(points(&ops).iter().all(|p| p.1 >= 1.0 && p.0 > -6.0));
    }
}
