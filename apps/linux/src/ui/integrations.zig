//! Account integrations dialog: Google Calendar status / disconnect / backfill,
//! mirroring the web /account/integrations page. Connecting (OAuth scope upgrade
//! via Better Auth linkSocial) has no native flow — like iOS, we surface status +
//! disconnect + backfill, and link out to the web page to connect.

const std = @import("std");
const gtk = @import("gtk.zig");
const widgets = @import("widgets.zig");
const trpc = @import("../core/api/trpc.zig");

const Ctx = struct {
    gpa: std.mem.Allocator,
    instance: []u8, // owned
    token: ?[]u8, // owned
    dialog: gtk.Object,
    content: gtk.Object, // rebuilt after mutations
    status: gtk.Object = null, // transient result line (backfill count, errors)
};

pub fn open(
    gpa: std.mem.Allocator,
    instance: []const u8,
    token: ?[]const u8,
    parent: gtk.Object,
) void {
    const ctx = gpa.create(Ctx) catch return;
    ctx.gpa = gpa;
    ctx.status = null;
    ctx.instance = gpa.dupe(u8, instance) catch {
        gpa.destroy(ctx);
        return;
    };
    ctx.token = if (token) |t| (gpa.dupe(u8, t) catch null) else null;

    const dialog = gtk.adw_dialog_new();
    gtk.adw_dialog_set_title(dialog, "Integrations");
    gtk.adw_dialog_set_content_width(dialog, 480);
    ctx.dialog = dialog;

    const tv = gtk.adw_toolbar_view_new();
    const header = gtk.adw_header_bar_new();
    gtk.adw_toolbar_view_add_top_bar(tv, header);

    const content = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 10);
    gtk.gtk_widget_set_margin_top(content, 16);
    gtk.gtk_widget_set_margin_bottom(content, 16);
    gtk.gtk_widget_set_margin_start(content, 16);
    gtk.gtk_widget_set_margin_end(content, 16);
    ctx.content = content;

    gtk.adw_toolbar_view_set_content(tv, content);
    gtk.adw_dialog_set_child(dialog, tv);
    _ = gtk.g_signal_connect_data(dialog, "closed", @ptrCast(&onClosed), ctx, null, 0);
    rebuild(ctx);
    gtk.adw_dialog_present(dialog, parent);
}

fn onClosed(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    const gpa = ctx.gpa;
    gpa.free(ctx.instance);
    if (ctx.token) |t| gpa.free(t);
    gpa.destroy(ctx);
}

fn clearBox(box: gtk.Object) void {
    var child = gtk.gtk_widget_get_first_child(box);
    while (child != null) {
        const next = gtk.gtk_widget_get_next_sibling(child);
        gtk.gtk_box_remove(box, child);
        child = next;
    }
}

fn rebuild(ctx: *Ctx) void {
    clearBox(ctx.content);
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    gtk.gtk_box_append(ctx.content, widgets.sectionTitle("Google Calendar"));
    const desc = gtk.gtk_label_new("One-way sync: issues with a due date appear as all-day events on your primary calendar.");
    gtk.gtk_widget_add_css_class(desc, "dim-label");
    gtk.gtk_widget_set_halign(desc, gtk.ALIGN_START);
    gtk.gtk_label_set_wrap(desc, 1);
    gtk.gtk_box_append(ctx.content, desc);

    // Status (GET query).
    var connected = false;
    var since: []const u8 = "";
    var status_resp = trpc.query(ctx.gpa, ctx.instance, "integrations.google.status", ctx.token, 30) catch null;
    if (status_resp) |*resp| {
        defer resp.deinit();
        if (resp.data()) |d| {
            if (trpc.asObject(d)) |obj| {
                connected = trpc.objBool(obj, "connected");
                if (connected) {
                    if (trpc.objString(obj, "connectedAt")) |c| since = if (c.len >= 10) c[0..10] else c;
                }
            }
        }
    }

    const status_lbl = gtk.gtk_label_new(null);
    if (connected) {
        if (since.len > 0) {
            if (std.fmt.allocPrintSentinel(a, "<span foreground='#22c55e'>✓ Connected</span> · since {s}", .{since}, 0)) |z| {
                gtk.gtk_label_set_markup(status_lbl, z.ptr);
            } else |_| {}
        } else {
            gtk.gtk_label_set_markup(status_lbl, "<span foreground='#22c55e'>✓ Connected</span>");
        }
    } else {
        gtk.gtk_label_set_text(status_lbl, "Not connected");
        gtk.gtk_widget_add_css_class(status_lbl, "dim-label");
    }
    gtk.gtk_widget_set_halign(status_lbl, gtk.ALIGN_START);
    gtk.gtk_box_append(ctx.content, status_lbl);

    const actions = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    gtk.gtk_widget_set_margin_top(actions, 4);
    if (connected) {
        const backfill = gtk.gtk_button_new_with_label("Backfill existing issues");
        _ = gtk.g_signal_connect_data(backfill, "clicked", @ptrCast(&onBackfill), ctx, null, 0);
        gtk.gtk_box_append(actions, backfill);
        const disconnect = gtk.gtk_button_new_with_label("Disconnect");
        gtk.gtk_widget_add_css_class(disconnect, "destructive-action");
        _ = gtk.g_signal_connect_data(disconnect, "clicked", @ptrCast(&onDisconnect), ctx, null, 0);
        gtk.gtk_box_append(actions, disconnect);
    } else {
        // Connecting upgrades OAuth scopes via the web linkSocial flow — no native
        // equivalent, so we link out to the web integrations page (matches iOS).
        const connect = gtk.gtk_button_new_with_label("Connect on the web");
        gtk.gtk_widget_add_css_class(connect, "suggested-action");
        _ = gtk.g_signal_connect_data(connect, "clicked", @ptrCast(&onConnectWeb), ctx, null, 0);
        gtk.gtk_box_append(actions, connect);
    }
    gtk.gtk_box_append(ctx.content, actions);

    const status = gtk.gtk_label_new("");
    gtk.gtk_widget_add_css_class(status, "dim-label");
    gtk.gtk_widget_set_halign(status, gtk.ALIGN_START);
    gtk.gtk_label_set_wrap(status, 1);
    gtk.gtk_box_append(ctx.content, status);
    ctx.status = status;
}

fn setStatus(ctx: *Ctx, msg: []const u8) void {
    const lbl = ctx.status orelse return;
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    if (arena.allocator().dupeZ(u8, msg)) |z| gtk.gtk_label_set_text(lbl, z.ptr) else |_| {}
}

fn onConnectWeb(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    var buf: [512]u8 = undefined;
    const url = std.fmt.bufPrintZ(&buf, "{s}/account/integrations", .{std.mem.trimEnd(u8, ctx.instance, "/")}) catch return;
    _ = gtk.g_app_info_launch_default_for_uri(url.ptr, null, null);
}

fn onDisconnect(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    var resp = trpc.call(ctx.gpa, ctx.instance, "integrations.google.disconnect", null, ctx.token, 30) catch {
        setStatus(ctx, "Couldn't disconnect.");
        return;
    };
    resp.deinit();
    rebuild(ctx);
}

fn onBackfill(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    var resp = trpc.call(ctx.gpa, ctx.instance, "integrations.google.backfill", null, ctx.token, 60) catch {
        setStatus(ctx, "Backfill failed.");
        return;
    };
    defer resp.deinit();
    var scheduled: i64 = 0;
    if (resp.data()) |d| {
        if (trpc.asObject(d)) |obj| {
            if (obj.get("scheduled")) |v| switch (v) {
                .integer => |i| scheduled = i,
                .float => |f| scheduled = @intFromFloat(f),
                else => {},
            };
        }
    }
    var buf: [96]u8 = undefined;
    if (std.fmt.bufPrint(&buf, "Scheduled {d} issue(s) for sync.", .{scheduled})) |m| setStatus(ctx, m) else |_| {}
}
