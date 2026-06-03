//! Admin console dialog (admin-only): Users + Workspaces tabs, mirroring the web
//! /admin pages. Lists are read with blocking tRPC GET queries (low-frequency
//! admin actions; deliberate), and mutations re-fetch afterwards. Self-contained
//! like settings.zig — depends on primitives, not the tracker's AppState.

const std = @import("std");
const gtk = @import("gtk.zig");
const widgets = @import("widgets.zig");
const trpc = @import("../core/api/trpc.zig");

const Tab = enum { users, workspaces };

const Ctx = struct {
    gpa: std.mem.Allocator,
    instance: []u8, // owned
    token: ?[]u8, // owned
    current_user_id: ?[:0]u8, // owned ("(you)" marker + self-guards)
    dialog: gtk.Object,
    content: gtk.Object, // rebuilt on tab change / after mutations
    tab: Tab = .users,
    users_btn: gtk.Object = null,
    workspaces_btn: gtk.Object = null,
    status: gtk.Object = null,
};

pub fn open(
    gpa: std.mem.Allocator,
    instance: []const u8,
    token: ?[]const u8,
    parent: gtk.Object,
    current_user_id: ?[]const u8,
) void {
    const ctx = gpa.create(Ctx) catch return;
    ctx.gpa = gpa;
    ctx.tab = .users;
    ctx.users_btn = null;
    ctx.workspaces_btn = null;
    ctx.status = null;
    ctx.instance = gpa.dupe(u8, instance) catch {
        gpa.destroy(ctx);
        return;
    };
    ctx.token = if (token) |t| (gpa.dupe(u8, t) catch null) else null;
    ctx.current_user_id = if (current_user_id) |u| (gpa.dupeZ(u8, u) catch null) else null;

    const dialog = gtk.adw_dialog_new();
    gtk.adw_dialog_set_title(dialog, "Admin");
    gtk.adw_dialog_set_content_width(dialog, 620);
    gtk.adw_dialog_set_content_height(dialog, 700);
    ctx.dialog = dialog;

    const tv = gtk.adw_toolbar_view_new();
    const header = gtk.adw_header_bar_new();
    // Users / Workspaces toggle (linked segmented buttons).
    const tabs = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 0);
    gtk.gtk_widget_add_css_class(tabs, "linked");
    const users_btn = gtk.gtk_button_new_with_label("Users");
    const ws_btn = gtk.gtk_button_new_with_label("Workspaces");
    _ = gtk.g_signal_connect_data(users_btn, "clicked", @ptrCast(&onTabUsers), ctx, null, 0);
    _ = gtk.g_signal_connect_data(ws_btn, "clicked", @ptrCast(&onTabWorkspaces), ctx, null, 0);
    gtk.gtk_box_append(tabs, users_btn);
    gtk.gtk_box_append(tabs, ws_btn);
    ctx.users_btn = users_btn;
    ctx.workspaces_btn = ws_btn;
    gtk.adw_header_bar_set_title_widget(header, tabs);
    gtk.adw_toolbar_view_add_top_bar(tv, header);

    const scrolled = gtk.gtk_scrolled_window_new();
    gtk.gtk_widget_set_vexpand(scrolled, 1);
    const content = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 8);
    gtk.gtk_widget_set_margin_top(content, 16);
    gtk.gtk_widget_set_margin_bottom(content, 16);
    gtk.gtk_widget_set_margin_start(content, 16);
    gtk.gtk_widget_set_margin_end(content, 16);
    gtk.gtk_scrolled_window_set_child(scrolled, content);
    ctx.content = content;

    gtk.adw_toolbar_view_set_content(tv, scrolled);
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
    if (ctx.current_user_id) |u| gpa.free(u);
    gpa.destroy(ctx);
}

fn onTabUsers(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    ctx.tab = .users;
    rebuild(ctx);
}
fn onTabWorkspaces(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    ctx.tab = .workspaces;
    rebuild(ctx);
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
    // Reflect the active tab on the segmented buttons.
    if (ctx.users_btn) |b| {
        if (ctx.tab == .users) gtk.gtk_widget_add_css_class(b, "suggested-action") else gtk.gtk_widget_remove_css_class(b, "suggested-action");
    }
    if (ctx.workspaces_btn) |b| {
        if (ctx.tab == .workspaces) gtk.gtk_widget_add_css_class(b, "suggested-action") else gtk.gtk_widget_remove_css_class(b, "suggested-action");
    }
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    const a = arena.allocator();
    switch (ctx.tab) {
        .users => usersSection(ctx, a),
        .workspaces => workspacesSection(ctx, a),
    }
}

fn usersSection(ctx: *Ctx, a: std.mem.Allocator) void {
    var resp = trpc.query(ctx.gpa, ctx.instance, "admin.listUsers", ctx.token, 30) catch {
        appendError(ctx, "Couldn't load users.");
        return;
    };
    defer resp.deinit();
    const rows = arrayData(&resp) orelse {
        appendError(ctx, "Couldn't load users.");
        return;
    };
    for (rows) |row| {
        const obj = trpc.asObject(row) orelse continue;
        gtk.gtk_box_append(ctx.content, userRow(ctx, a, obj));
    }
}

fn userRow(ctx: *Ctx, a: std.mem.Allocator, obj: std.json.ObjectMap) gtk.Object {
    const id = trpc.objString(obj, "id") orelse "";
    const name = trpc.objString(obj, "name") orelse "";
    const email = trpc.objString(obj, "email") orelse "";
    const is_admin = trpc.objBool(obj, "isAdmin");
    const ws_count = objInt(obj, "workspaceCount");
    const is_self = if (ctx.current_user_id) |uid| std.mem.eql(u8, uid, id) else false;

    const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    gtk.gtk_widget_set_margin_top(row, 2);
    if (name.len > 0) gtk.gtk_box_append(row, widgets.avatar(a, name));

    const vb = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 0);
    gtk.gtk_widget_set_hexpand(vb, 1);
    const name_lbl = gtk.gtk_label_new(null);
    const name_txt = if (is_self)
        std.fmt.allocPrintSentinel(a, "{s} (you)", .{name}, 0) catch null
    else
        a.dupeZ(u8, name) catch null;
    if (name_txt) |t| gtk.gtk_label_set_text(name_lbl, t.ptr);
    gtk.gtk_widget_set_halign(name_lbl, gtk.ALIGN_START);
    gtk.gtk_box_append(vb, name_lbl);
    const meta = gtk.gtk_label_new(null);
    if (std.fmt.allocPrintSentinel(a, "{s} · {d} workspace(s)", .{ email, ws_count }, 0)) |z| gtk.gtk_label_set_text(meta, z.ptr) else |_| {}
    gtk.gtk_widget_add_css_class(meta, "dim-label");
    gtk.gtk_widget_add_css_class(meta, "caption");
    gtk.gtk_widget_set_halign(meta, gtk.ALIGN_START);
    gtk.gtk_box_append(vb, meta);
    gtk.gtk_box_append(row, vb);

    // Admin toggle.
    const admin_chk = gtk.gtk_check_button_new_with_label("Admin");
    if (is_admin) gtk.gtk_check_button_set_active(admin_chk, 1);
    if (makeAction(ctx, .set_admin, id)) |ac| {
        ac.flag = !is_admin; // clicking flips the current value
        gtk.g_object_set_data_full(admin_chk, "exp-ctx", @ptrCast(ac), @ptrCast(&freeAction));
        _ = gtk.g_signal_connect_data(admin_chk, "toggled", @ptrCast(&onToggleAdmin), ac, null, 0);
    }
    gtk.gtk_box_append(row, admin_chk);

    // Delete (disabled for self).
    const del = gtk.gtk_button_new_with_label("Delete");
    gtk.gtk_widget_add_css_class(del, "flat");
    gtk.gtk_widget_add_css_class(del, "destructive-action");
    gtk.gtk_widget_set_sensitive(del, if (is_self) 0 else 1);
    if (!is_self) {
        if (makeAction(ctx, .delete_user, id)) |ac| {
            gtk.g_object_set_data_full(del, "exp-ctx", @ptrCast(ac), @ptrCast(&freeAction));
            _ = gtk.g_signal_connect_data(del, "clicked", @ptrCast(&onActionClicked), ac, null, 0);
        }
    }
    gtk.gtk_box_append(row, del);
    return row;
}

fn workspacesSection(ctx: *Ctx, a: std.mem.Allocator) void {
    var resp = trpc.query(ctx.gpa, ctx.instance, "admin.listWorkspaces", ctx.token, 30) catch {
        appendError(ctx, "Couldn't load workspaces.");
        return;
    };
    defer resp.deinit();
    const rows = arrayData(&resp) orelse {
        appendError(ctx, "Couldn't load workspaces.");
        return;
    };
    for (rows) |row| {
        const obj = trpc.asObject(row) orelse continue;
        gtk.gtk_box_append(ctx.content, workspaceRow(ctx, a, obj));
    }
}

fn workspaceRow(ctx: *Ctx, a: std.mem.Allocator, obj: std.json.ObjectMap) gtk.Object {
    const id = trpc.objString(obj, "id") orelse "";
    const name = trpc.objString(obj, "name") orelse "";
    const plan = trpc.objString(obj, "plan") orelse "free";
    const members = objInt(obj, "memberCount");
    const projects = objInt(obj, "projectCount");

    var owner: []const u8 = "";
    if (obj.get("owners")) |ov| switch (ov) {
        .array => |owners| {
            if (owners.items.len > 0) {
                if (trpc.asObject(owners.items[0])) |o0| {
                    owner = trpc.objString(o0, "email") orelse (trpc.objString(o0, "name") orelse "");
                }
            }
        },
        else => {},
    };

    const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    gtk.gtk_widget_set_margin_top(row, 2);
    const vb = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 0);
    gtk.gtk_widget_set_hexpand(vb, 1);
    const name_lbl = gtk.gtk_label_new(null);
    if (a.dupeZ(u8, name)) |z| gtk.gtk_label_set_text(name_lbl, z.ptr) else |_| {}
    gtk.gtk_widget_set_halign(name_lbl, gtk.ALIGN_START);
    gtk.gtk_box_append(vb, name_lbl);
    const meta = gtk.gtk_label_new(null);
    if (std.fmt.allocPrintSentinel(a, "{s} · {d} member(s) · {d} project(s) · {s}", .{ plan, members, projects, owner }, 0)) |z| {
        gtk.gtk_label_set_text(meta, z.ptr);
    } else |_| {}
    gtk.gtk_widget_add_css_class(meta, "dim-label");
    gtk.gtk_widget_add_css_class(meta, "caption");
    gtk.gtk_widget_set_halign(meta, gtk.ALIGN_START);
    gtk.gtk_box_append(vb, meta);
    gtk.gtk_box_append(row, vb);

    const del = gtk.gtk_button_new_with_label("Delete");
    gtk.gtk_widget_add_css_class(del, "flat");
    gtk.gtk_widget_add_css_class(del, "destructive-action");
    if (makeAction(ctx, .delete_workspace, id)) |ac| {
        gtk.g_object_set_data_full(del, "exp-ctx", @ptrCast(ac), @ptrCast(&freeAction));
        _ = gtk.g_signal_connect_data(del, "clicked", @ptrCast(&onActionClicked), ac, null, 0);
    }
    gtk.gtk_box_append(row, del);
    return row;
}

fn appendError(ctx: *Ctx, msg: [*:0]const u8) void {
    const lbl = gtk.gtk_label_new(msg);
    gtk.gtk_widget_add_css_class(lbl, "dim-label");
    gtk.gtk_widget_set_halign(lbl, gtk.ALIGN_START);
    gtk.gtk_box_append(ctx.content, lbl);
}

/// Extract the `result.data` array, if the response is one.
fn arrayData(resp: *trpc.Response) ?[]std.json.Value {
    if (!resp.ok()) return null;
    return switch (resp.data() orelse return null) {
        .array => |arr| arr.items,
        else => null,
    };
}

fn objInt(obj: std.json.ObjectMap, key: []const u8) i64 {
    const v = obj.get(key) orelse return 0;
    return switch (v) {
        .integer => |i| i,
        .float => |f| @intFromFloat(f),
        else => 0,
    };
}

// --- per-row actions (blocking mutation, then re-fetch) ---

const ActionKind = enum { set_admin, delete_user, delete_workspace };

const ActionCtx = struct {
    ctx: *Ctx,
    kind: ActionKind,
    id: [:0]u8,
    flag: bool = false, // desired isAdmin for set_admin
};

fn makeAction(ctx: *Ctx, kind: ActionKind, id: []const u8) ?*ActionCtx {
    const ac = ctx.gpa.create(ActionCtx) catch return null;
    ac.ctx = ctx;
    ac.kind = kind;
    ac.flag = false;
    ac.id = ctx.gpa.dupeZ(u8, id) catch {
        ctx.gpa.destroy(ac);
        return null;
    };
    return ac;
}

fn freeAction(p: gtk.gpointer) callconv(.c) void {
    const ac: *ActionCtx = @ptrCast(@alignCast(p));
    ac.ctx.gpa.free(ac.id);
    ac.ctx.gpa.destroy(ac);
}

fn onToggleAdmin(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ac: *ActionCtx = @ptrCast(@alignCast(data));
    const ctx = ac.ctx;
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    const a = arena.allocator();
    call(ctx, a, "admin.setUserAdmin", .{ .userId = ac.id, .isAdmin = ac.flag });
    rebuild(ctx); // re-fetch (also frees ac via the destroyed checkbutton)
}

fn onActionClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ac: *ActionCtx = @ptrCast(@alignCast(data));
    const ctx = ac.ctx;
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    const a = arena.allocator();
    switch (ac.kind) {
        .delete_user => call(ctx, a, "admin.deleteUser", .{ .userId = ac.id }),
        .delete_workspace => call(ctx, a, "admin.deleteWorkspace", .{ .workspaceId = ac.id }),
        .set_admin => {},
    }
    rebuild(ctx);
}

/// Blocking tRPC mutation (deliberate admin action); errors are dropped and the
/// re-fetch reflects the true state.
fn call(ctx: *Ctx, a: std.mem.Allocator, proc: []const u8, input: anytype) void {
    const json = std.json.Stringify.valueAlloc(a, input, .{}) catch return;
    var resp = trpc.call(ctx.gpa, ctx.instance, proc, json, ctx.token, 30) catch return;
    resp.deinit();
}
