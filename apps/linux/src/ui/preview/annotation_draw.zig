//! Cairo rendering of annotation shapes — the Linux twin of the web widget's
//! `packages/widget/src/annotate/draw.ts`, sharing the pure geometry in
//! `core/annotate/geometry.zig`. What the live overlay draws is exactly what the
//! flatten ships (same code path), so the uploaded screenshot matches the editor.
//!
//! Coordinates are IMAGE pixel space; the caller installs the image→widget
//! scale on the cairo context (overlay draw) or draws at native resolution
//! (flatten). The marker.io red (#ef4444 == rgb 0.937, 0.267, 0.267), round
//! cap/join, quadratic pen smoothing, and filled arrow head all mirror draw.ts.

const std = @import("std");
const gtk = @import("../gtk.zig");
const geo = @import("../../core/annotate/geometry.zig");

// #ef4444 → 0xef/255, 0x44/255, 0x44/255. Matches draw.ts `annotationColor`.
pub const color_r: f64 = 0.9372549019607843;
pub const color_g: f64 = 0.26666666666666666;
pub const color_b: f64 = 0.26666666666666666;

fn setStroke(cr: gtk.Object, stroke_width: f64) void {
    gtk.cairo_set_source_rgb(cr, color_r, color_g, color_b);
    gtk.cairo_set_line_width(cr, stroke_width);
    gtk.cairo_set_line_cap(cr, gtk.CAIRO_LINE_CAP_ROUND);
    gtk.cairo_set_line_join(cr, gtk.CAIRO_LINE_JOIN_ROUND);
}

fn drawPen(cr: gtk.Object, shape: geo.Shape, stroke_width: f64) void {
    const points = shape.points;
    if (points.len == 0) return;
    if (points.len == 1) {
        // In-progress stroke before the first move: a filled dot.
        gtk.cairo_new_path(cr);
        gtk.cairo_arc(cr, points[0].x, points[0].y, stroke_width / 2, 0, std.math.tau);
        gtk.cairo_fill(cr);
        return;
    }
    // Quadratic midpoint smoothing (draw.ts): the canvas API takes the on-curve
    // anchor (the sample point) + the midpoint to the next sample. Cairo has only
    // cubic Béziers, so convert each quadratic (P0, control=Pi, P2=mid) to a
    // cubic with C1 = P0 + 2/3·(Pi−P0), C2 = P2 + 2/3·(Pi−P2). The path is
    // mathematically identical, so the stroke matches the web pixel-for-pixel.
    gtk.cairo_new_path(cr);
    gtk.cairo_move_to(cr, points[0].x, points[0].y);
    var p0 = points[0];
    var i: usize = 1;
    while (i < points.len - 1) : (i += 1) {
        const ctrl = points[i];
        const mid = geo.Point{
            .x = (points[i].x + points[i + 1].x) / 2,
            .y = (points[i].y + points[i + 1].y) / 2,
        };
        const c1 = geo.Point{ .x = p0.x + (2.0 / 3.0) * (ctrl.x - p0.x), .y = p0.y + (2.0 / 3.0) * (ctrl.y - p0.y) };
        const c2 = geo.Point{ .x = mid.x + (2.0 / 3.0) * (ctrl.x - mid.x), .y = mid.y + (2.0 / 3.0) * (ctrl.y - mid.y) };
        gtk.cairo_curve_to(cr, c1.x, c1.y, c2.x, c2.y, mid.x, mid.y);
        p0 = mid;
    }
    const last = points[points.len - 1];
    gtk.cairo_line_to(cr, last.x, last.y);
    gtk.cairo_stroke(cr);
}

fn drawRect(cr: gtk.Object, shape: geo.Shape) void {
    if (shape.points.len < 2) return;
    const r = geo.normalizeRect(shape.points[0], shape.points[1]);
    gtk.cairo_new_path(cr);
    gtk.cairo_rectangle(cr, r.x, r.y, r.width, r.height);
    gtk.cairo_stroke(cr);
}

fn drawArrow(cr: gtk.Object, shape: geo.Shape, stroke_width: f64) void {
    if (shape.points.len < 2) return;
    const from = shape.points[0];
    const to = shape.points[1];
    const head = geo.arrowHead(from, to, stroke_width);
    // Shaft (stops short of the tip so the line doesn't poke through the head).
    gtk.cairo_new_path(cr);
    gtk.cairo_move_to(cr, from.x, from.y);
    gtk.cairo_line_to(cr, head.shaft_end.x, head.shaft_end.y);
    gtk.cairo_stroke(cr);
    // Filled triangle head.
    gtk.cairo_new_path(cr);
    gtk.cairo_move_to(cr, head.tip.x, head.tip.y);
    gtk.cairo_line_to(cr, head.left.x, head.left.y);
    gtk.cairo_line_to(cr, head.right.x, head.right.y);
    gtk.cairo_close_path(cr);
    gtk.cairo_fill(cr);
}

/// Draw one shape onto `cr` in image space. `stroke_width` is `strokeWidthFor`
/// of the base image (one width per screenshot, like the web).
pub fn drawShape(cr: gtk.Object, shape: geo.Shape, stroke_width: f64) void {
    setStroke(cr, stroke_width);
    switch (shape.tool) {
        .pen => drawPen(cr, shape, stroke_width),
        .rect => drawRect(cr, shape),
        .arrow => drawArrow(cr, shape, stroke_width),
    }
}

/// Draw every committed shape (the live overlay + the flatten both call this).
pub fn drawShapes(cr: gtk.Object, shapes: []const geo.Shape, stroke_width: f64) void {
    for (shapes) |shape| drawShape(cr, shape, stroke_width);
}

/// Flattened image bytes + the format actually produced (so the report knows the
/// content type). Owned by the caller's allocator.
pub const Encoded = struct {
    bytes: []u8,
    /// "png" or "jpeg" — the gdk-pixbuf type that succeeded.
    format: []const u8,
};

/// Composite `shapes` over the base screenshot at `base_path` (a PNG written by
/// `adb exec-out screencap -p`, etc.) and encode the result to PNG (falling back
/// to JPEG). Returns null on any failure — the caller keeps the text-only issue
/// and toasts "screenshot upload failed" (graceful degradation, mirrors draw.ts/
/// flatten.ts null-on-failure). All Cairo/GdkPixbuf work is main-thread only.
pub fn flattenFile(gpa: std.mem.Allocator, base_path: [:0]const u8, shapes: []const geo.Shape) ?Encoded {
    const base = gtk.gdk_pixbuf_new_from_file(base_path.ptr, null);
    if (base == null) return null;
    defer gtk.g_object_unref(base);
    return flattenPixbuf(gpa, base, shapes);
}

/// Flatten over an already-decoded base pixbuf. Splitting this out keeps the
/// webview snapshot path (which hands us a pixbuf directly) reusing the encoder.
pub fn flattenPixbuf(gpa: std.mem.Allocator, base: gtk.Object, shapes: []const geo.Shape) ?Encoded {
    const w = gtk.gdk_pixbuf_get_width(base);
    const h = gtk.gdk_pixbuf_get_height(base);
    if (w <= 0 or h <= 0) return null;

    const surface = gtk.cairo_image_surface_create(gtk.CAIRO_FORMAT_ARGB32, w, h);
    if (surface == null) return null;
    defer gtk.cairo_surface_destroy(surface);
    const cr = gtk.cairo_create(surface);
    if (cr == null) return null;
    defer gtk.cairo_destroy(cr);

    // Paint the base screenshot, then the strokes in native image space (no
    // scale transform — flatten is 1:1).
    gtk.gdk_cairo_set_source_pixbuf(cr, base, 0, 0);
    gtk.cairo_paint(cr);
    const stroke_width = geo.strokeWidthFor(@floatFromInt(w), @floatFromInt(h));
    drawShapes(cr, shapes, stroke_width);
    gtk.cairo_surface_flush(surface);

    // Surface → pixbuf → encoded bytes. Try PNG first (lossless, matches the
    // web's preferred ladder), then JPEG.
    const out = gtk.gdk_pixbuf_get_from_surface(surface, 0, 0, w, h);
    if (out == null) return null;
    defer gtk.g_object_unref(out);

    if (encode(gpa, out, "png")) |b| return .{ .bytes = b, .format = "png" };
    if (encode(gpa, out, "jpeg")) |b| return .{ .bytes = b, .format = "jpeg" };
    return null;
}

fn encode(gpa: std.mem.Allocator, pixbuf: gtk.Object, fmt: [*:0]const u8) ?[]u8 {
    var buf: ?[*]u8 = null;
    var size: usize = 0;
    const ok = gtk.gdk_pixbuf_save_to_bufferv(pixbuf, &buf, &size, fmt, null, null, null);
    if (ok == 0 or buf == null or size == 0) return null;
    // gdk-pixbuf allocated the buffer with g_malloc; copy into our allocator and
    // free the original so ownership is uniform for the caller.
    defer gtk.g_free(@ptrCast(buf));
    const owned = gpa.dupe(u8, buf.?[0..size]) catch return null;
    return owned;
}
