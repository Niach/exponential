//! The transparent annotation canvas that floats over the live preview: a
//! GtkDrawingArea driven by a GtkGestureDrag that builds shapes in IMAGE-pixel
//! space, rendered via annotation_draw.zig (Cairo). A small toolbar
//! (rect/pen/arrow + undo/clear) picks the tool; an AdwDialog "send" sheet
//! collects a title/description and files the annotated screenshot through
//! feedback_report.zig (authenticated-direct).
//!
//! Coordinate spaces: the gesture delivers widget pixels; we map them to image
//! space with the same scale the draw func uses (image fills the widget,
//! preserving aspect via a uniform scale + centering), so what you draw lands
//! exactly where it flattens. All shapes live in image space — the display scale
//! is a draw-time-only transform (the rule from the web widget).
//!
//! Main-thread only. The flatten (Cairo/GdkPixbuf) runs here on the main thread;
//! the network report runs on feedback_report's worker.

const std = @import("std");
const gtk = @import("../gtk.zig");
const geo = @import("../../core/annotate/geometry.zig");
const draw = @import("annotation_draw.zig");
const report = @import("feedback_report.zig");

/// A committed shape with a gpa-owned point buffer.
const OwnedShape = struct {
    tool: geo.Tool,
    points: std.ArrayListUnmanaged(geo.Point) = .empty,
};

pub const AnnotationOverlay = struct {
    gpa: std.mem.Allocator,
    area: gtk.Object, // the transparent GtkDrawingArea (the overlay child)
    parent_window: gtk.Object, // anchors the send sheet

    tool: geo.Tool = .rect,
    shapes: std.ArrayListUnmanaged(OwnedShape) = .empty,
    // In-progress shape while dragging (committed on drag-end if non-degenerate).
    drafting: ?OwnedShape = null,
    drag_start: geo.Point = .{ .x = 0, .y = 0 },

    // The base screenshot: dimensions (image space) + a decoded pixbuf used for
    // the flatten. Set via setBase before drawing. Null → nothing to annotate.
    image_w: f64 = 0,
    image_h: f64 = 0,
    base_pixbuf: gtk.Object = null, // owned ref (unref on clear)

    // Report context, set by the host (instance/token/project from AppState).
    instance: ?[]u8 = null,
    token: ?[]u8 = null,
    project_id: ?[]u8 = null,

    // The send sheet's entries (valid only while the sheet is open).
    title_entry: gtk.Object = null,
    desc_view: gtk.Object = null,

    pub fn create(gpa: std.mem.Allocator, parent_window: gtk.Object) ?*AnnotationOverlay {
        const self = gpa.create(AnnotationOverlay) catch return null;
        const area = gtk.gtk_drawing_area_new();
        gtk.gtk_widget_set_hexpand(area, 1);
        gtk.gtk_widget_set_vexpand(area, 1);
        self.* = .{ .gpa = gpa, .area = area, .parent_window = parent_window };
        gtk.gtk_drawing_area_set_draw_func(area, drawFunc, self, null);

        // A drag gesture builds shapes. We also keep a separate toolbar widget the
        // host can place; here we install the gesture on the canvas itself.
        const drag = gtk.gtk_gesture_drag_new();
        _ = gtk.g_signal_connect_data(drag, "drag-begin", @ptrCast(&onDragBegin), self, null, 0);
        _ = gtk.g_signal_connect_data(drag, "drag-update", @ptrCast(&onDragUpdate), self, null, 0);
        _ = gtk.g_signal_connect_data(drag, "drag-end", @ptrCast(&onDragEnd), self, null, 0);
        gtk.gtk_widget_add_controller(area, drag);
        return self;
    }

    pub fn destroy(self: *AnnotationOverlay) void {
        self.clear();
        if (self.instance) |s| self.gpa.free(s);
        if (self.token) |s| self.gpa.free(s);
        if (self.project_id) |s| self.gpa.free(s);
        self.gpa.destroy(self);
    }

    /// Build a toolbar (rect/pen/arrow toggles + undo/clear + Send) the host can
    /// place above the canvas. Returns a GtkBox.
    pub fn buildToolbar(self: *AnnotationOverlay) gtk.Object {
        const bar = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 4);
        gtk.gtk_widget_add_css_class(bar, "toolbar");
        gtk.gtk_widget_set_margin_start(bar, 6);
        gtk.gtk_widget_set_margin_end(bar, 6);
        gtk.gtk_widget_set_margin_top(bar, 4);
        gtk.gtk_widget_set_margin_bottom(bar, 4);

        const rect = gtk.gtk_toggle_button_new_with_label("▭");
        gtk.gtk_widget_set_tooltip_text(rect, "Rectangle");
        gtk.gtk_toggle_button_set_active(rect, 1);
        _ = gtk.g_signal_connect_data(rect, "toggled", @ptrCast(&onToolRect), self, null, 0);
        const pen = gtk.gtk_toggle_button_new_with_label("✎");
        gtk.gtk_widget_set_tooltip_text(pen, "Pen");
        gtk.gtk_toggle_button_set_group(pen, rect);
        _ = gtk.g_signal_connect_data(pen, "toggled", @ptrCast(&onToolPen), self, null, 0);
        const arrow = gtk.gtk_toggle_button_new_with_label("↗");
        gtk.gtk_widget_set_tooltip_text(arrow, "Arrow");
        gtk.gtk_toggle_button_set_group(arrow, rect);
        _ = gtk.g_signal_connect_data(arrow, "toggled", @ptrCast(&onToolArrow), self, null, 0);
        gtk.gtk_box_append(bar, rect);
        gtk.gtk_box_append(bar, pen);
        gtk.gtk_box_append(bar, arrow);

        const sep = gtk.gtk_separator_new(gtk.ORIENTATION_HORIZONTAL);
        gtk.gtk_box_append(bar, sep);

        const undo = gtk.gtk_button_new_with_label("Undo");
        gtk.gtk_widget_add_css_class(undo, "flat");
        _ = gtk.g_signal_connect_data(undo, "clicked", @ptrCast(&onUndo), self, null, 0);
        const clear_btn = gtk.gtk_button_new_with_label("Clear");
        gtk.gtk_widget_add_css_class(clear_btn, "flat");
        _ = gtk.g_signal_connect_data(clear_btn, "clicked", @ptrCast(&onClearClicked), self, null, 0);
        gtk.gtk_box_append(bar, undo);
        gtk.gtk_box_append(bar, clear_btn);

        const send = gtk.gtk_button_new_with_label("Send feedback");
        gtk.gtk_widget_add_css_class(send, "suggested-action");
        gtk.gtk_widget_set_hexpand(send, 1);
        gtk.gtk_widget_set_halign(send, gtk.ALIGN_END);
        _ = gtk.g_signal_connect_data(send, "clicked", @ptrCast(&onOpenSendSheet), self, null, 0);
        gtk.gtk_box_append(bar, send);
        return bar;
    }

    /// Point the report at a project, with the developer's auth.
    pub fn setReportContext(self: *AnnotationOverlay, instance: []const u8, token: ?[]const u8, project_id: []const u8) void {
        if (self.instance) |s| self.gpa.free(s);
        if (self.token) |s| self.gpa.free(s);
        if (self.project_id) |s| self.gpa.free(s);
        self.instance = self.gpa.dupe(u8, instance) catch null;
        self.token = if (token) |t| (self.gpa.dupe(u8, t) catch null) else null;
        self.project_id = self.gpa.dupe(u8, project_id) catch null;
    }

    /// Set the base screenshot (an owned pixbuf ref + its image-space size). The
    /// overlay takes a ref; the previous base is unref'd.
    pub fn setBase(self: *AnnotationOverlay, pixbuf: gtk.Object) void {
        if (self.base_pixbuf != null) gtk.g_object_unref(self.base_pixbuf);
        self.base_pixbuf = if (pixbuf != null) gtk.g_object_ref(pixbuf) else null;
        if (pixbuf != null) {
            self.image_w = @floatFromInt(gtk.gdk_pixbuf_get_width(pixbuf));
            self.image_h = @floatFromInt(gtk.gdk_pixbuf_get_height(pixbuf));
        }
        gtk.gtk_widget_queue_draw(self.area);
    }

    /// Start a fresh annotation session (the host has set the base frame). Just
    /// clears any stale strokes and redraws.
    pub fn beginSession(self: *AnnotationOverlay) void {
        self.clearShapes();
        gtk.gtk_widget_queue_draw(self.area);
    }

    /// Drop all shapes + the base. Idempotent.
    pub fn clear(self: *AnnotationOverlay) void {
        self.clearShapes();
        if (self.base_pixbuf != null) {
            gtk.g_object_unref(self.base_pixbuf);
            self.base_pixbuf = null;
        }
        self.image_w = 0;
        self.image_h = 0;
        gtk.gtk_widget_queue_draw(self.area);
    }

    fn clearShapes(self: *AnnotationOverlay) void {
        for (self.shapes.items) |*s| s.points.deinit(self.gpa);
        self.shapes.clearRetainingCapacity();
        if (self.drafting) |*d| {
            d.points.deinit(self.gpa);
            self.drafting = null;
        }
    }

    // --- coordinate mapping (widget ↔ image space) ---

    /// Uniform fit of the image into the widget (aspect-preserving, centered),
    /// matching the draw func. Returns scale + the top-left offset of the image
    /// inside the widget.
    fn fit(self: *AnnotationOverlay) struct { scale: f64, ox: f64, oy: f64 } {
        const ww: f64 = @floatFromInt(gtk.gtk_widget_get_width(self.area));
        const wh: f64 = @floatFromInt(gtk.gtk_widget_get_height(self.area));
        if (self.image_w <= 0 or self.image_h <= 0 or ww <= 0 or wh <= 0)
            return .{ .scale = 1, .ox = 0, .oy = 0 };
        const scale = @min(ww / self.image_w, wh / self.image_h);
        const ox = (ww - self.image_w * scale) / 2;
        const oy = (wh - self.image_h * scale) / 2;
        return .{ .scale = scale, .ox = ox, .oy = oy };
    }

    fn toImage(self: *AnnotationOverlay, wx: f64, wy: f64) geo.Point {
        const f = self.fit();
        if (f.scale == 0) return .{ .x = wx, .y = wy };
        const p = geo.Point{ .x = (wx - f.ox) / f.scale, .y = (wy - f.oy) / f.scale };
        return geo.clampPoint(p, self.image_w, self.image_h);
    }

    // --- drag gesture → shapes ---

    fn onDragBegin(_: gtk.Object, sx: f64, sy: f64, data: gtk.gpointer) callconv(.c) void {
        const self: *AnnotationOverlay = @ptrCast(@alignCast(data orelse return));
        if (self.image_w <= 0) return;
        self.drag_start = self.toImage(sx, sy);
        var d = OwnedShape{ .tool = self.tool };
        d.points.append(self.gpa, self.drag_start) catch {};
        if (self.tool != .pen) {
            // rect/arrow track a second point updated on drag.
            d.points.append(self.gpa, self.drag_start) catch {};
        }
        self.drafting = d;
    }

    fn onDragUpdate(_: gtk.Object, ox: f64, oy: f64, data: gtk.gpointer) callconv(.c) void {
        const self: *AnnotationOverlay = @ptrCast(@alignCast(data orelse return));
        const d = if (self.drafting) |*x| x else return;
        const cur = self.toImage(self.drag_startWidget().x + ox, self.drag_startWidget().y + oy);
        if (self.tool == .pen) {
            d.points.append(self.gpa, cur) catch {};
        } else {
            if (d.points.items.len >= 2) d.points.items[1] = cur;
        }
        gtk.gtk_widget_queue_draw(self.area);
    }

    fn onDragEnd(_: gtk.Object, ox: f64, oy: f64, data: gtk.gpointer) callconv(.c) void {
        const self: *AnnotationOverlay = @ptrCast(@alignCast(data orelse return));
        var d = if (self.drafting) |x| x else return;
        self.drafting = null;
        // Final point.
        const cur = self.toImage(self.drag_startWidget().x + ox, self.drag_startWidget().y + oy);
        if (self.tool == .pen) {
            d.points.append(self.gpa, cur) catch {};
        } else if (d.points.items.len >= 2) {
            d.points.items[1] = cur;
        }
        // Drop degenerate (click-without-drag) shapes, matching the web.
        const shape = geo.Shape{ .tool = d.tool, .points = d.points.items };
        if (geo.isDegenerate(shape, 4)) {
            d.points.deinit(self.gpa);
        } else {
            self.shapes.append(self.gpa, d) catch d.points.deinit(self.gpa);
        }
        gtk.gtk_widget_queue_draw(self.area);
    }

    /// The drag offsets are relative to the gesture start; we kept the start in
    /// image space, so reconstruct the start in widget space for the offset math.
    fn drag_startWidget(self: *AnnotationOverlay) geo.Point {
        const f = self.fit();
        return .{ .x = self.drag_start.x * f.scale + f.ox, .y = self.drag_start.y * f.scale + f.oy };
    }

    // --- draw func ---

    fn drawFunc(area: gtk.Object, cr: gtk.Object, _: c_int, _: c_int, data: gtk.gpointer) callconv(.c) void {
        const self: *AnnotationOverlay = @ptrCast(@alignCast(data orelse return));
        _ = area;
        if (self.image_w <= 0) return;
        const f = self.fit();
        // Map image space → widget space so shapes (in image px) land correctly.
        gtk.cairo_save(cr);
        gtk.cairo_translate(cr, f.ox, f.oy);
        gtk.cairo_scale(cr, f.scale, f.scale);
        const stroke_width = geo.strokeWidthFor(self.image_w, self.image_h);
        // Committed shapes.
        for (self.shapes.items) |s| {
            draw.drawShape(cr, .{ .tool = s.tool, .points = s.points.items }, stroke_width);
        }
        // The in-progress shape.
        if (self.drafting) |d| {
            draw.drawShape(cr, .{ .tool = d.tool, .points = d.points.items }, stroke_width);
        }
        gtk.cairo_restore(cr);
    }

    // --- toolbar handlers ---

    fn onToolRect(btn: gtk.Object, data: gtk.gpointer) callconv(.c) void {
        if (gtk.gtk_toggle_button_get_active(btn) != 0) setTool(data, .rect);
    }
    fn onToolPen(btn: gtk.Object, data: gtk.gpointer) callconv(.c) void {
        if (gtk.gtk_toggle_button_get_active(btn) != 0) setTool(data, .pen);
    }
    fn onToolArrow(btn: gtk.Object, data: gtk.gpointer) callconv(.c) void {
        if (gtk.gtk_toggle_button_get_active(btn) != 0) setTool(data, .arrow);
    }
    fn setTool(data: gtk.gpointer, tool: geo.Tool) void {
        const self: *AnnotationOverlay = @ptrCast(@alignCast(data orelse return));
        self.tool = tool;
    }

    fn onUndo(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
        const self: *AnnotationOverlay = @ptrCast(@alignCast(data orelse return));
        if (self.shapes.items.len > 0) {
            var s = self.shapes.pop().?;
            s.points.deinit(self.gpa);
            gtk.gtk_widget_queue_draw(self.area);
        }
    }
    fn onClearClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
        const self: *AnnotationOverlay = @ptrCast(@alignCast(data orelse return));
        self.clearShapes();
        gtk.gtk_widget_queue_draw(self.area);
    }

    // --- send sheet (AdwDialog) → feedback_report ---

    fn onOpenSendSheet(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
        const self: *AnnotationOverlay = @ptrCast(@alignCast(data orelse return));
        if (self.project_id == null or self.instance == null) return;

        const dialog = gtk.adw_dialog_new();
        gtk.adw_dialog_set_title(dialog, "Send feedback");
        gtk.adw_dialog_set_content_width(dialog, 420);

        const tv = gtk.adw_toolbar_view_new();
        const header = gtk.adw_header_bar_new();
        gtk.adw_toolbar_view_add_top_bar(tv, header);

        const box = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 8);
        gtk.gtk_widget_set_margin_top(box, 12);
        gtk.gtk_widget_set_margin_bottom(box, 12);
        gtk.gtk_widget_set_margin_start(box, 12);
        gtk.gtk_widget_set_margin_end(box, 12);

        const title = gtk.gtk_entry_new();
        gtk.gtk_entry_set_placeholder_text(title, "Title");
        gtk.gtk_box_append(box, title);
        self.title_entry = title;

        const desc = gtk.gtk_text_view_new();
        gtk.gtk_text_view_set_wrap_mode(desc, gtk.WRAP_WORD_CHAR);
        gtk.gtk_widget_set_size_request(desc, -1, 100);
        const desc_scroll = gtk.gtk_scrolled_window_new();
        gtk.gtk_scrolled_window_set_child(desc_scroll, desc);
        gtk.gtk_box_append(box, desc_scroll);
        self.desc_view = desc;

        const send = gtk.gtk_button_new_with_label("File issue");
        gtk.gtk_widget_add_css_class(send, "suggested-action");
        _ = gtk.g_signal_connect_data(send, "clicked", @ptrCast(&onSubmit), self, null, 0);
        gtk.adw_header_bar_pack_end(header, send);

        gtk.adw_toolbar_view_set_content(tv, box);
        gtk.adw_dialog_set_child(dialog, tv);
        // Stash the dialog so onSubmit can close it.
        gtk.g_object_set_data_full(send, "exp-dialog", dialog, null);
        gtk.adw_dialog_present(dialog, self.parent_window);
    }

    fn onSubmit(btn: gtk.Object, data: gtk.gpointer) callconv(.c) void {
        const self: *AnnotationOverlay = @ptrCast(@alignCast(data orelse return));
        const gpa = self.gpa;

        const title_c = gtk.gtk_editable_get_text(self.title_entry);
        const title = std.mem.trim(u8, std.mem.span(title_c), " \t\r\n");
        const desc = self.descText(gpa);
        defer if (desc) |d| gpa.free(d);

        // Flatten the annotated screenshot (Cairo/GdkPixbuf, main thread).
        var image: ?[]u8 = null;
        var image_format: []const u8 = "png";
        if (self.base_pixbuf != null and self.image_w > 0) {
            const shapes = self.snapshotShapes(gpa);
            defer freeSnapshot(gpa, shapes);
            if (draw.flattenPixbuf(gpa, self.base_pixbuf, shapes)) |enc| {
                image = enc.bytes;
                image_format = enc.format;
            }
        }
        defer if (image) |b| gpa.free(b);

        const effective_title = if (title.len > 0) title else "Preview feedback";
        _ = report.fire(
            gpa,
            self.instance.?,
            if (self.token) |t| t else null,
            self.project_id.?,
            effective_title,
            desc orelse "",
            image,
            image_format,
            self.parent_window,
        );

        // Close the sheet.
        const dialog = gtk.g_object_get_data(btn, "exp-dialog");
        if (dialog != null) _ = gtk.adw_dialog_close(dialog);
    }

    fn descText(self: *AnnotationOverlay, gpa: std.mem.Allocator) ?[]u8 {
        const buffer = gtk.gtk_text_view_get_buffer(self.desc_view);
        var start: [128]u8 align(8) = undefined;
        var end: [128]u8 align(8) = undefined;
        gtk.gtk_text_buffer_get_bounds(buffer, @ptrCast(&start), @ptrCast(&end));
        const text_c = gtk.gtk_text_buffer_get_text(buffer, @ptrCast(&start), @ptrCast(&end), 0) orelse return null;
        defer gtk.g_free(@ptrCast(text_c));
        const text = std.mem.span(text_c);
        if (text.len == 0) return null;
        return gpa.dupe(u8, text) catch null;
    }

    /// Snapshot the committed shapes into a plain `[]geo.Shape` for the flatten
    /// (the point buffers are borrowed from `self.shapes` for the call's life).
    fn snapshotShapes(self: *AnnotationOverlay, gpa: std.mem.Allocator) []geo.Shape {
        var out = gpa.alloc(geo.Shape, self.shapes.items.len) catch return &.{};
        for (self.shapes.items, 0..) |s, i| out[i] = .{ .tool = s.tool, .points = s.points.items };
        return out;
    }
};

fn freeSnapshot(gpa: std.mem.Allocator, shapes: []geo.Shape) void {
    if (shapes.len > 0) gpa.free(shapes);
}
