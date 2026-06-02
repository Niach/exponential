//! Small reusable GTK widgets that give the tracker its web-parity look:
//! coloured status/priority icon labels, label chips (rounded pills), and the
//! one-time CSS for them. Built on the GTK-free `format.zig` option tables.

const std = @import("std");
const gtk = @import("gtk.zig");
const format = @import("format.zig");

/// Install application-wide CSS once (rounded label chips, dim group headers,
/// comment cards). Safe to call multiple times but intended for startup.
pub fn applyCss() void {
    const provider = gtk.gtk_css_provider_new();
    gtk.gtk_css_provider_load_from_string(provider,
        \\.exp-chip {
        \\  border: 1px solid alpha(currentColor, 0.18);
        \\  border-radius: 9999px;
        \\  padding: 1px 8px;
        \\  font-size: 0.85em;
        \\}
        \\.exp-group-header {
        \\  background-color: alpha(currentColor, 0.04);
        \\  border-radius: 6px;
        \\}
        \\.exp-comment {
        \\  background-color: alpha(currentColor, 0.04);
        \\  border-radius: 8px;
        \\  padding: 8px 10px;
        \\}
        \\.exp-plan {
        \\  background-color: alpha(#6366f1, 0.10);
        \\  border: 1px solid alpha(#6366f1, 0.30);
        \\  border-radius: 8px;
        \\  padding: 8px 10px;
        \\}
        \\.exp-question {
        \\  background-color: alpha(#eab308, 0.10);
        \\  border: 1px solid alpha(#eab308, 0.35);
        \\  border-radius: 8px;
        \\  padding: 8px 10px;
        \\}
        \\.exp-icon { font-size: 1.05em; }
        \\.exp-title-entry {
        \\  font-size: 1.5em;
        \\  font-weight: bold;
        \\  background: none;
        \\  box-shadow: none;
        \\  border: none;
        \\  padding-left: 0;
        \\  min-height: 0;
        \\}
        \\.exp-avatar {
        \\  background-color: alpha(currentColor, 0.14);
        \\  border-radius: 9999px;
        \\  min-width: 20px;
        \\  min-height: 20px;
        \\  font-size: 0.72em;
        \\}
    );
    const display = gtk.gdk_display_get_default();
    if (display != null)
        gtk.gtk_style_context_add_provider_for_display(display, provider, gtk.STYLE_PROVIDER_PRIORITY_APPLICATION);
    gtk.g_object_unref(provider);
}

/// A label rendering a coloured glyph (status/priority icon). The markup is
/// fixed-size (glyph ≤ 4 bytes + a #rrggbb colour), so a stack buffer suffices.
pub fn iconLabel(opt: format.Option) gtk.Object {
    const lbl = gtk.gtk_label_new(null);
    var buf: [96]u8 = undefined;
    const markup = std.fmt.bufPrintZ(&buf, "<span foreground='{s}'>{s}</span>", .{ opt.color, opt.glyph }) catch return lbl;
    gtk.gtk_label_set_markup(lbl, markup.ptr);
    gtk.gtk_widget_add_css_class(lbl, "exp-icon");
    return lbl;
}

pub fn statusIcon(value: []const u8) gtk.Object {
    return iconLabel(format.status(value));
}

pub fn priorityIcon(value: []const u8) gtk.Object {
    return iconLabel(format.priority(value));
}

/// A small coloured dot (used for label glyphs on dense list rows).
pub fn dot(color: []const u8) gtk.Object {
    const lbl = gtk.gtk_label_new(null);
    var buf: [64]u8 = undefined;
    const markup = std.fmt.bufPrintZ(&buf, "<span foreground='{s}'>●</span>", .{color}) catch return lbl;
    gtk.gtk_label_set_markup(lbl, markup.ptr);
    return lbl;
}

/// A rounded label chip: coloured dot + name. `name` need not be NUL-terminated.
pub fn chip(arena: std.mem.Allocator, name: []const u8, color: []const u8) gtk.Object {
    const box = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 4);
    gtk.gtk_widget_add_css_class(box, "exp-chip");
    gtk.gtk_box_append(box, dot(color));
    const lbl = gtk.gtk_label_new(null);
    if (arena.dupeZ(u8, name)) |z| gtk.gtk_label_set_text(lbl, z.ptr) else |_| {}
    gtk.gtk_box_append(box, lbl);
    return box;
}

/// A circular initial avatar for a user (first codepoint, uppercased if ASCII),
/// with the full name as a tooltip. `name` must be non-empty.
pub fn avatar(arena: std.mem.Allocator, name: []const u8) gtk.Object {
    const lbl = gtk.gtk_label_new(null);
    gtk.gtk_widget_add_css_class(lbl, "exp-avatar");
    const n = std.unicode.utf8ByteSequenceLength(name[0]) catch 1;
    var buf: [5]u8 = undefined;
    const first = name[0..@min(n, name.len)];
    @memcpy(buf[0..first.len], first);
    if (first.len == 1 and buf[0] >= 'a' and buf[0] <= 'z') buf[0] -= 32;
    buf[first.len] = 0;
    gtk.gtk_label_set_text(lbl, @ptrCast(&buf));
    if (arena.dupeZ(u8, name)) |z| gtk.gtk_widget_set_tooltip_text(lbl, z.ptr) else |_| {}
    return lbl;
}

/// A start-aligned section title ("Description", "Comments", …).
pub fn sectionTitle(text: [*:0]const u8) gtk.Object {
    const lbl = gtk.gtk_label_new(text);
    gtk.gtk_widget_add_css_class(lbl, "title-4");
    gtk.gtk_widget_set_halign(lbl, gtk.ALIGN_START);
    gtk.gtk_widget_set_margin_top(lbl, 8);
    return lbl;
}

/// A horizontal row: coloured icon + plain text label (used inside picker
/// popovers and the detail properties row).
pub fn iconTextRow(arena: std.mem.Allocator, icon: gtk.Object, text: []const u8) gtk.Object {
    const box = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
    gtk.gtk_box_append(box, icon);
    const lbl = gtk.gtk_label_new(null);
    if (arena.dupeZ(u8, text)) |z| gtk.gtk_label_set_text(lbl, z.ptr) else |_| {}
    gtk.gtk_widget_set_halign(lbl, gtk.ALIGN_START);
    gtk.gtk_box_append(box, lbl);
    return box;
}
