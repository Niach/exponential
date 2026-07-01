//! Pure annotation geometry — a faithful Zig port of the web widget's
//! `packages/widget/src/annotate/shapes.ts` (the single source of truth, also
//! ported to Swift `ExpCore/Sources/Annotate/AnnotationGeometry.swift`). The
//! test block at the bottom mirrors `shapes.test.ts` so CI fails on divergence
//! across TS ↔ Swift ↔ Zig.
//!
//! All coordinates live in IMAGE pixel space (the screenshot's native raster):
//! flattening is exact and the editor's display scale is a pure draw-time
//! transform. GTK-free so it links into the core test root (`src/tests.zig`).

const std = @import("std");

/// `AnnotationTool` in shapes.ts — rect/arrow draw from two corners; pen is the
/// sampled polyline.
pub const Tool = enum { rect, pen, arrow };

/// A point in image-pixel space.
pub const Point = struct {
    x: f64,
    y: f64,
};

/// A drawn shape: rect/arrow use `points[0..2]` (start, end); pen carries the
/// whole sampled polyline (≥1 point while drafting).
pub const Shape = struct {
    tool: Tool,
    points: []const Point,
};

/// Origin + positive extent (whatever the drag direction).
pub const NormalizedRect = struct {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
};

/// Corner pair → origin + positive extent, whatever the drag direction.
pub fn normalizeRect(a: Point, b: Point) NormalizedRect {
    return .{
        .x = @min(a.x, b.x),
        .y = @min(a.y, b.y),
        .width = @abs(a.x - b.x),
        .height = @abs(a.y - b.y),
    };
}

/// One stroke width per image (~0.35% of the long edge, min 3px) so all shapes
/// on a screenshot read consistently at both full and thumbnail size. Mirrors
/// `strokeWidthFor` exactly: `max(3, round(max(w,h,1) * 0.0035))`.
pub fn strokeWidthFor(image_width: f64, image_height: f64) f64 {
    const longest = @max(image_width, image_height, 1);
    return @max(3, @round(longest * 0.0035));
}

/// Geometry of an arrow's filled-triangle head.
pub const ArrowHead = struct {
    /// Where the shaft stroke should stop so it doesn't poke through the head.
    shaft_end: Point,
    tip: Point,
    left: Point,
    right: Point,
};

const arrow_spread = std.math.pi / 7.0;

pub fn arrowHeadLength(stroke_width: f64) f64 {
    return @max(10, stroke_width * 4);
}

/// Filled-triangle head for an arrow from `from` to `to`.
pub fn arrowHead(from: Point, to: Point, stroke_width: f64) ArrowHead {
    const angle = std.math.atan2(to.y - from.y, to.x - from.x);
    const length = arrowHeadLength(stroke_width);
    const wing = struct {
        fn at(t: Point, a: f64, len: f64, spread: f64) Point {
            return .{
                .x = t.x - len * @cos(a + spread),
                .y = t.y - len * @sin(a + spread),
            };
        }
    }.at;
    return .{
        .tip = to,
        .left = wing(to, angle, length, -arrow_spread),
        .right = wing(to, angle, length, arrow_spread),
        .shaft_end = .{
            .x = to.x - length * 0.6 * @cos(angle),
            .y = to.y - length * 0.6 * @sin(angle),
        },
    };
}

pub fn distance(a: Point, b: Point) f64 {
    return std.math.hypot(a.x - b.x, a.y - b.y);
}

/// Accidental clicks shouldn't commit invisible shapes: rect/arrow need a real
/// drag; a pen stroke needs at least two samples (a tiny scribble is a valid dot
/// marker, so no length threshold there). `min_drag_px` defaults to 4 to mirror
/// `isDegenerate`'s default arg.
pub fn isDegenerate(shape: Shape, min_drag_px: f64) bool {
    if (shape.tool == .pen) return shape.points.len < 2;
    if (shape.points.len < 2) return true;
    return distance(shape.points[0], shape.points[1]) < min_drag_px;
}

/// The shapes.ts default-arg form (`minDragPx = 4`).
pub fn isDegenerateDefault(shape: Shape) bool {
    return isDegenerate(shape, 4);
}

pub fn clampPoint(point: Point, width: f64, height: f64) Point {
    return .{
        .x = @min(@max(point.x, 0), width),
        .y = @min(@max(point.y, 0), height),
    };
}

// ---------------------------------------------------------------------------
// Parity tests — a direct port of packages/widget/src/annotate/shapes.test.ts.
// Keep these byte-aligned with the TS/Swift suites: a divergence here is a
// cross-client rendering bug.
// ---------------------------------------------------------------------------

const expect = std.testing.expect;

fn approx(a: f64, b: f64, tol: f64) bool {
    return @abs(a - b) <= tol;
}

test "normalizeRect keeps an already-normalized pair" {
    const r = normalizeRect(.{ .x = 10, .y = 20 }, .{ .x = 110, .y = 70 });
    try expect(r.x == 10 and r.y == 20 and r.width == 100 and r.height == 50);
}

test "normalizeRect normalizes a drag in any direction" {
    const r1 = normalizeRect(.{ .x = 110, .y = 70 }, .{ .x = 10, .y = 20 });
    try expect(r1.x == 10 and r1.y == 20 and r1.width == 100 and r1.height == 50);
    const r2 = normalizeRect(.{ .x = 10, .y = 70 }, .{ .x = 110, .y = 20 });
    try expect(r2.x == 10 and r2.y == 20 and r2.width == 100 and r2.height == 50);
}

test "strokeWidthFor enforces the 3px minimum on small images" {
    try expect(strokeWidthFor(320, 240) == 3);
}

test "strokeWidthFor scales with the long edge" {
    try expect(strokeWidthFor(1920, 1080) == 7);
    try expect(strokeWidthFor(1080, 1920) == 7);
}

test "strokeWidthFor survives degenerate dimensions" {
    try expect(strokeWidthFor(0, 0) == 3);
}

test "arrowHead puts the tip at the drag end" {
    const from = Point{ .x = 0, .y = 0 };
    const to = Point{ .x = 100, .y = 0 };
    const head = arrowHead(from, to, 5);
    try expect(head.tip.x == to.x and head.tip.y == to.y);
}

test "arrowHead places both wings one head-length from the tip" {
    const from = Point{ .x = 0, .y = 0 };
    const to = Point{ .x = 100, .y = 0 };
    const sw: f64 = 5;
    const head = arrowHead(from, to, sw);
    const length = arrowHeadLength(sw);
    try expect(approx(distance(head.left, head.tip), length, 1e-6));
    try expect(approx(distance(head.right, head.tip), length, 1e-6));
}

test "arrowHead keeps the wings symmetric about the shaft" {
    const from = Point{ .x = 0, .y = 0 };
    const to = Point{ .x = 100, .y = 0 };
    const head = arrowHead(from, to, 5);
    // Shaft lies on the x-axis: wings mirror in y, share x.
    try expect(approx(head.left.x, head.right.x, 1e-6));
    try expect(approx(head.left.y, -head.right.y, 1e-6));
    try expect(@abs(head.left.y) > 0.1);
}

test "arrowHead pulls the shaft end back inside the head" {
    const from = Point{ .x = 0, .y = 0 };
    const to = Point{ .x = 100, .y = 0 };
    const sw: f64 = 5;
    const head = arrowHead(from, to, sw);
    const length = arrowHeadLength(sw);
    try expect(approx(head.shaft_end.y, 0, 1e-6));
    try expect(head.shaft_end.x < to.x);
    try expect(head.shaft_end.x > to.x - length);
}

test "arrowHeadLength scales with stroke width but never below 10px" {
    try expect(arrowHeadLength(1) == 10);
    try expect(arrowHeadLength(8) == 32);
}

test "isDegenerate drops click-without-drag rects and arrows" {
    const pts = [_]Point{ .{ .x = 50, .y = 50 }, .{ .x = 52, .y = 51 } };
    try expect(isDegenerateDefault(.{ .tool = .rect, .points = &pts }));
    try expect(isDegenerateDefault(.{ .tool = .arrow, .points = &pts }));
}

test "isDegenerate keeps real drags" {
    const pts = [_]Point{ .{ .x = 0, .y = 0 }, .{ .x = 30, .y = 10 } };
    try expect(!isDegenerateDefault(.{ .tool = .arrow, .points = &pts }));
}

test "isDegenerate treats single-point shapes as degenerate" {
    const pts = [_]Point{.{ .x = 1, .y = 1 }};
    try expect(isDegenerateDefault(.{ .tool = .rect, .points = &pts }));
}

test "isDegenerate keeps tiny pen scribbles but drops single samples" {
    const one = [_]Point{.{ .x = 5, .y = 5 }};
    try expect(isDegenerateDefault(.{ .tool = .pen, .points = &one }));
    const two = [_]Point{ .{ .x = 5, .y = 5 }, .{ .x = 6, .y = 5 } };
    try expect(!isDegenerateDefault(.{ .tool = .pen, .points = &two }));
}

test "clampPoint clamps to the image bounds" {
    const a = clampPoint(.{ .x = -4, .y = 30 }, 100, 50);
    try expect(a.x == 0 and a.y == 30);
    const b = clampPoint(.{ .x = 120, .y = 60 }, 100, 50);
    try expect(b.x == 100 and b.y == 50);
    const c = clampPoint(.{ .x = 40, .y = 20 }, 100, 50);
    try expect(c.x == 40 and c.y == 20);
}
