//! Workspace settings dialog: General (rename / public toggle / write policy),
//! Members (change role, remove) and Invites (generate link, revoke). Reads the
//! synced local tables and mutates via tRPC. Self-contained — it depends on the
//! primitives (gpa/instance/token/db/parent) rather than the app's AppState, so
//! it stays decoupled from the tracker UI.
//!
//! For v1 it operates on the first synced workspace (the common single-workspace
//! case); a workspace switcher is a future addition. Admin mutations are blocking
//! (deliberate, low-frequency) and the dialog re-reads local state afterwards.

const std = @import("std");
const gtk = @import("gtk.zig");
const widgets = @import("widgets.zig");
const trpc = @import("../core/api/trpc.zig");
const Database = @import("../core/db/database.zig").Database;
const api_projects = @import("../core/api/projects.zig");
const credentials = @import("../core/credentials.zig");

const policy_options = [_:null]?[*:0]const u8{ "members", "everyone" };

const Ctx = struct {
    gpa: std.mem.Allocator,
    instance: []u8, // owned (dialog can outlive the caller)
    token: ?[]u8, // owned
    db: *Database,
    current_user_id: ?[:0]u8, // owned
    dialog: gtk.Object,
    content: gtk.Object, // vbox we rebuild on every change
    ws_id: [:0]u8,
    name_entry: gtk.Object = null,
    public_check: gtk.Object = null,
    policy_dd: gtk.Object = null,
    link_entry: gtk.Object = null,
    new_label_entry: gtk.Object = null,
    last_link: ?[:0]u8 = null,
    // Coding section
    coding_claude_entry: gtk.Object = null,
    coding_repos_entry: gtk.Object = null,
    coding_prefix_entry: gtk.Object = null,
    coding_doctor_label: gtk.Object = null,
    last_minted_key: ?[:0]u8 = null, // shown once after Generate; freed on close
};

pub fn open(
    gpa: std.mem.Allocator,
    instance: []const u8,
    token: ?[]const u8,
    db: *Database,
    parent: gtk.Object,
    current_user_id: ?[]const u8,
    workspace_id: ?[]const u8,
) void {
    var tmp = std.heap.ArenaAllocator.init(gpa);
    defer tmp.deinit();
    // Prefer the user's own (non-public, owned) default workspace. Falling back
    // to firstWorkspaceId can pick the synced public/shared workspace, which
    // would register the agent against the wrong workspace.
    const ws = workspace_id orelse (blk: {
        if (current_user_id) |uid| {
            if (db.defaultOwnedWorkspaceId(tmp.allocator(), uid) catch null) |w| break :blk w;
        }
        break :blk (db.firstWorkspaceId(tmp.allocator()) catch null);
    }) orelse return;

    const ctx = gpa.create(Ctx) catch return;
    ctx.gpa = gpa;
    ctx.db = db;
    ctx.dialog = undefined;
    ctx.content = undefined;
    ctx.name_entry = null;
    ctx.public_check = null;
    ctx.policy_dd = null;
    ctx.link_entry = null;
    ctx.last_link = null;
    // Own all borrowed inputs so the modeless dialog can't dangle on sign-out.
    ctx.ws_id = gpa.dupeZ(u8, ws) catch {
        gpa.destroy(ctx);
        return;
    };
    ctx.instance = gpa.dupe(u8, instance) catch {
        gpa.free(ctx.ws_id);
        gpa.destroy(ctx);
        return;
    };
    ctx.token = if (token) |t| (gpa.dupe(u8, t) catch null) else null;
    ctx.current_user_id = if (current_user_id) |u| (gpa.dupeZ(u8, u) catch null) else null;

    const dialog = gtk.adw_dialog_new();
    gtk.adw_dialog_set_title(dialog, "Workspace settings");
    gtk.adw_dialog_set_content_width(dialog, 560);
    gtk.adw_dialog_set_content_height(dialog, 680);
    ctx.dialog = dialog;

    const tv = gtk.adw_toolbar_view_new();
    const header = gtk.adw_header_bar_new();
    gtk.adw_toolbar_view_add_top_bar(tv, header);

    const scrolled = gtk.gtk_scrolled_window_new();
    gtk.gtk_widget_set_vexpand(scrolled, 1);
    const content = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 12);
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
    gpa.free(ctx.ws_id);
    gpa.free(ctx.instance);
    if (ctx.token) |t| gpa.free(t);
    if (ctx.current_user_id) |u| gpa.free(u);
    if (ctx.last_link) |l| gpa.free(l);
    if (ctx.last_minted_key) |k| gpa.free(k);
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

    // Resolve current workspace details.
    var ws_name: []const u8 = "Workspace";
    var is_public = false;
    var policy: []const u8 = "members";
    if (ctx.db.listWorkspaces(a)) |list| {
        for (list) |w| {
            if (std.mem.eql(u8, w.id, ctx.ws_id)) {
                ws_name = w.name;
                is_public = w.is_public;
                policy = w.public_write_policy;
            }
        }
    } else |_| {}

    // --- General ---
    gtk.gtk_box_append(ctx.content, widgets.sectionTitle("General"));
    const name_row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    const name = gtk.gtk_entry_new();
    gtk.gtk_widget_set_hexpand(name, 1);
    if (a.dupeZ(u8, ws_name)) |z| gtk.gtk_editable_set_text(name, z.ptr) else |_| {}
    gtk.gtk_box_append(name_row, name);
    const rename = gtk.gtk_button_new_with_label("Rename");
    _ = gtk.g_signal_connect_data(rename, "clicked", @ptrCast(&onRename), ctx, null, 0);
    gtk.gtk_box_append(name_row, rename);
    gtk.gtk_box_append(ctx.content, name_row);
    ctx.name_entry = name;

    const pub_check = gtk.gtk_check_button_new_with_label("Public workspace");
    if (is_public) gtk.gtk_check_button_set_active(pub_check, 1);
    _ = gtk.g_signal_connect_data(pub_check, "toggled", @ptrCast(&onTogglePublic), ctx, null, 0);
    gtk.gtk_box_append(ctx.content, pub_check);
    ctx.public_check = pub_check;

    const policy_row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    const policy_lbl = gtk.gtk_label_new("Who can write:");
    gtk.gtk_widget_add_css_class(policy_lbl, "dim-label");
    gtk.gtk_box_append(policy_row, policy_lbl);
    const policy_dd = gtk.gtk_drop_down_new_from_strings(&policy_options);
    gtk.gtk_drop_down_set_selected(policy_dd, if (std.mem.eql(u8, policy, "everyone")) 1 else 0);
    gtk.gtk_widget_set_sensitive(policy_dd, if (is_public) 1 else 0);
    // connect AFTER set_selected so priming doesn't fire (would loop via rebuild)
    _ = gtk.g_signal_connect_data(policy_dd, "notify::selected", @ptrCast(&onPolicyChanged), ctx, null, 0);
    gtk.gtk_box_append(policy_row, policy_dd);
    gtk.gtk_box_append(ctx.content, policy_row);
    ctx.policy_dd = policy_dd;

    // --- Members ---
    const members = ctx.db.listMembers(a, ctx.ws_id) catch &[_]Database.MemberRow{};
    if (std.fmt.allocPrintSentinel(a, "Members ({d})", .{members.len}, 0)) |t| {
        gtk.gtk_box_append(ctx.content, widgets.sectionTitle(t));
    } else |_| {}
    // A workspace must always keep at least one owner.
    var owner_count: usize = 0;
    for (members) |m| {
        if (std.mem.eql(u8, m.role, "owner")) owner_count += 1;
    }
    for (members) |m| gtk.gtk_box_append(ctx.content, memberRow(ctx, a, m, owner_count));

    // --- Invites ---
    gtk.gtk_box_append(ctx.content, widgets.sectionTitle("Invites"));
    const gen = gtk.gtk_button_new_with_label("Generate invite link");
    gtk.gtk_widget_set_halign(gen, gtk.ALIGN_START);
    _ = gtk.g_signal_connect_data(gen, "clicked", @ptrCast(&onGenerate), ctx, null, 0);
    gtk.gtk_box_append(ctx.content, gen);

    if (ctx.last_link) |link| {
        const entry = gtk.gtk_entry_new();
        gtk.gtk_editable_set_text(entry, link.ptr);
        gtk.gtk_editable_set_editable(entry, 0); // read-only; user selects + copies
        gtk.gtk_box_append(ctx.content, entry);
        ctx.link_entry = entry;
    }

    const invites = ctx.db.listInvites(a, ctx.ws_id) catch &[_]Database.InviteRow{};
    for (invites) |inv| gtk.gtk_box_append(ctx.content, inviteRow(ctx, a, inv));

    // --- Projects ---
    projectsSection(ctx, a);

    // --- Run Targets & Preview ---
    previewSection(ctx, a);

    // --- Labels ---
    labelsSection(ctx, a);

    // --- Coding (desktop launcher settings) ---
    codingSection(ctx, a);

    // --- Danger Zone (owner-only) ---
    var is_owner = false;
    for (members) |m| {
        if (ctx.current_user_id) |uid| {
            if (std.mem.eql(u8, uid, m.user_id) and std.mem.eql(u8, m.role, "owner")) is_owner = true;
        }
    }
    if (is_owner) dangerZone(ctx, ws_name);
}

// --- Projects section -------------------------------------------------------

fn projectsSection(ctx: *Ctx, a: std.mem.Allocator) void {
    const projects = ctx.db.listProjects(a, ctx.ws_id) catch &[_]Database.ProjectRow{};
    if (std.fmt.allocPrintSentinel(a, "Projects ({d})", .{projects.len}, 0)) |t| {
        gtk.gtk_box_append(ctx.content, widgets.sectionTitle(t));
    } else |_| {}
    for (projects) |p| {
        const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
        gtk.gtk_widget_set_margin_top(row, 2);
        gtk.gtk_box_append(row, widgets.dot(p.color));
        const lbl = gtk.gtk_label_new(null);
        if (a.dupeZ(u8, p.name)) |z| gtk.gtk_label_set_text(lbl, z.ptr) else |_| {}
        gtk.gtk_widget_set_halign(lbl, gtk.ALIGN_START);
        gtk.gtk_widget_set_hexpand(lbl, 1);
        gtk.gtk_box_append(row, lbl);

        const del = gtk.gtk_button_new_with_label("Delete");
        gtk.gtk_widget_add_css_class(del, "flat");
        gtk.gtk_widget_add_css_class(del, "destructive-action");
        if (makeRowDel(ctx, .project, p.id, p.name)) |rd| {
            gtk.g_object_set_data_full(del, "exp-rd", @ptrCast(rd), @ptrCast(&freeRowDel));
            _ = gtk.g_signal_connect_data(del, "clicked", @ptrCast(&onOpenConfirm), rd, null, 0);
        }
        gtk.gtk_box_append(row, del);
        gtk.gtk_box_append(ctx.content, row);
    }
}

// --- Run Targets & Preview section ------------------------------------------
//
// Edits the DB MIRROR only (display metadata + the feedback issue routing
// target). The build/run commands live in the repo's .exponential/config.json
// and are never shown/edited here — only the named targets (read-only) and the
// feedbackProjectId picker. Owner-gated server-side via projects.updatePreviewConfig.

/// Per-project preview-config edit context (the feedback-project picker), freed
/// when its row is destroyed.
const PreviewRowCtx = struct {
    ctx: *Ctx,
    gpa: std.mem.Allocator,
    project_id: [:0]u8,
    feedback_dd: gtk.Object = null,
    // Parallel to the dropdown entries: index 0 = "(this project)", then the
    // workspace's projects. gpa-owned id strings.
    option_ids: std.ArrayListUnmanaged([:0]u8) = .empty,
    // The mirror's current target list (id/name/platform), re-sent on save so we
    // don't clobber what the desktop discovered from the repo.
    target_ids: std.ArrayListUnmanaged([:0]u8) = .empty,
    target_names: std.ArrayListUnmanaged([:0]u8) = .empty,
    target_platforms: std.ArrayListUnmanaged([:0]u8) = .empty,
};

fn freePreviewRow(p: gtk.gpointer) callconv(.c) void {
    const prc: *PreviewRowCtx = @ptrCast(@alignCast(p));
    const gpa = prc.gpa;
    gpa.free(prc.project_id);
    for (prc.option_ids.items) |s| gpa.free(s);
    prc.option_ids.deinit(gpa);
    for (prc.target_ids.items) |s| gpa.free(s);
    prc.target_ids.deinit(gpa);
    for (prc.target_names.items) |s| gpa.free(s);
    prc.target_names.deinit(gpa);
    for (prc.target_platforms.items) |s| gpa.free(s);
    prc.target_platforms.deinit(gpa);
    gpa.destroy(prc);
}

fn previewSection(ctx: *Ctx, a: std.mem.Allocator) void {
    const projects = ctx.db.listProjects(a, ctx.ws_id) catch &[_]Database.ProjectRow{};
    gtk.gtk_box_append(ctx.content, widgets.sectionTitle("Run Targets & Preview"));
    const hint = gtk.gtk_label_new("Build/run commands live in .exponential/config.json in each repo. Here you only route preview feedback.");
    gtk.gtk_widget_add_css_class(hint, "dim-label");
    gtk.gtk_label_set_wrap(hint, 1);
    gtk.gtk_label_set_xalign(hint, 0);
    gtk.gtk_box_append(ctx.content, hint);

    for (projects) |p| {
        // Parse the mirror (display-only). Skip projects with no preview config.
        if (p.preview_config.len == 0) continue;
        const parsed = std.json.parseFromSliceLeaky(std.json.Value, a, p.preview_config, .{}) catch continue;
        const obj = switch (parsed) {
            .object => |o| o,
            else => continue,
        };

        const card = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 4);
        gtk.gtk_widget_set_margin_top(card, 8);
        const title = gtk.gtk_label_new(null);
        if (a.dupeZ(u8, p.name)) |z| gtk.gtk_label_set_text(title, z.ptr) else |_| {}
        gtk.gtk_widget_add_css_class(title, "heading");
        gtk.gtk_widget_set_halign(title, gtk.ALIGN_START);
        gtk.gtk_box_append(card, title);

        const prc = ctx.gpa.create(PreviewRowCtx) catch continue;
        prc.* = .{ .ctx = ctx, .gpa = ctx.gpa, .project_id = ctx.gpa.dupeZ(u8, p.id) catch {
            ctx.gpa.destroy(prc);
            continue;
        } };

        // Read-only target badges + capture the target list for re-send.
        const targets_row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
        if (obj.get("targets")) |tv| if (tv == .array) {
            for (tv.array.items) |t| {
                const to = switch (t) {
                    .object => |o| o,
                    else => continue,
                };
                const id = jsonStr(to, "id") orelse continue;
                const name = jsonStr(to, "name") orelse id;
                const platform = jsonStr(to, "platform") orelse "web";
                prc.target_ids.append(ctx.gpa, ctx.gpa.dupeZ(u8, id) catch continue) catch {};
                prc.target_names.append(ctx.gpa, ctx.gpa.dupeZ(u8, name) catch continue) catch {};
                prc.target_platforms.append(ctx.gpa, ctx.gpa.dupeZ(u8, platform) catch continue) catch {};
                const badge_txt = std.fmt.allocPrintSentinel(a, "{s} · {s}", .{ name, platform }, 0) catch continue;
                const badge = gtk.gtk_label_new(badge_txt.ptr);
                gtk.gtk_widget_add_css_class(badge, "exp-chip");
                gtk.gtk_box_append(targets_row, badge);
            }
        };
        gtk.gtk_box_append(card, targets_row);

        // Feedback project picker: "(this project)" + every project in the ws.
        const fb_row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
        const fb_lbl = gtk.gtk_label_new("File feedback into:");
        gtk.gtk_widget_add_css_class(fb_lbl, "dim-label");
        gtk.gtk_box_append(fb_row, fb_lbl);

        var names = std.ArrayListUnmanaged(?[*:0]const u8).empty;
        names.append(a, "(this project)") catch {};
        // Index 0 = "(this project)" → empty id (means "no explicit routing").
        if (ctx.gpa.dupeZ(u8, "")) |empty_id| {
            prc.option_ids.append(ctx.gpa, empty_id) catch ctx.gpa.free(empty_id);
        } else |_| {}
        const current_fb = jsonStr(obj, "feedbackProjectId");
        var selected_idx: c_uint = 0;
        for (projects, 0..) |pp, i| {
            const z = a.dupeZ(u8, pp.name) catch continue;
            names.append(a, z.ptr) catch {};
            prc.option_ids.append(ctx.gpa, ctx.gpa.dupeZ(u8, pp.id) catch continue) catch {};
            if (current_fb) |fb| if (std.mem.eql(u8, fb, pp.id)) {
                selected_idx = @intCast(i + 1);
            };
        }
        names.append(a, null) catch {};
        const fb_dd = gtk.gtk_drop_down_new_from_strings(@ptrCast(names.items.ptr));
        gtk.gtk_drop_down_set_selected(fb_dd, selected_idx);
        prc.feedback_dd = fb_dd;
        gtk.gtk_box_append(fb_row, fb_dd);

        const save = gtk.gtk_button_new_with_label("Save");
        gtk.gtk_widget_add_css_class(save, "flat");
        gtk.g_object_set_data_full(save, "exp-prc", @ptrCast(prc), @ptrCast(&freePreviewRow));
        _ = gtk.g_signal_connect_data(save, "clicked", @ptrCast(&onPreviewSave), prc, null, 0);
        gtk.gtk_box_append(fb_row, save);

        gtk.gtk_box_append(card, fb_row);
        gtk.gtk_box_append(ctx.content, card);
    }
}

fn onPreviewSave(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const prc: *PreviewRowCtx = @ptrCast(@alignCast(data orelse return));
    const ctx = prc.ctx;
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    // Resolve the chosen feedback project id (index 0 = "(this project)" → null).
    const sel: usize = @intCast(gtk.gtk_drop_down_get_selected(prc.feedback_dd));
    var feedback_id: ?[]const u8 = null;
    if (sel < prc.option_ids.items.len) {
        const id = prc.option_ids.items[sel];
        if (id.len > 0) feedback_id = id;
    }

    // Re-send the existing target list (display metadata) unchanged.
    var targets = std.ArrayList(api_projects.MirrorTarget).empty;
    const n = @min(@min(prc.target_ids.items.len, prc.target_names.items.len), prc.target_platforms.items.len);
    for (0..n) |i| {
        targets.append(a, .{
            .id = prc.target_ids.items[i],
            .name = prc.target_names.items[i],
            .platform = prc.target_platforms.items[i],
        }) catch {};
    }

    _ = api_projects.updatePreviewConfig(ctx.gpa, ctx.instance, ctx.token, prc.project_id, targets.items, feedback_id);
    rebuild(ctx);
}

fn jsonStr(o: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const v = o.get(key) orelse return null;
    return switch (v) {
        .string => |s| s,
        else => null,
    };
}

// --- Labels section ---------------------------------------------------------

/// Per-label edit context (rename entry + colour), freed when its row is destroyed.
const LabelRowCtx = struct {
    ctx: *Ctx,
    gpa: std.mem.Allocator, // long-lived; freeLabelRow can run after ctx is freed
    id: [:0]u8,
    name_entry: gtk.Object = null,
    selected_swatch: gtk.Object = null,
};

fn freeLabelRow(p: gtk.gpointer) callconv(.c) void {
    const lrc: *LabelRowCtx = @ptrCast(@alignCast(p));
    lrc.gpa.free(lrc.id);
    lrc.gpa.destroy(lrc);
}

fn labelsSection(ctx: *Ctx, a: std.mem.Allocator) void {
    const labels = ctx.db.listLabels(a, ctx.ws_id) catch &[_]Database.LabelRow{};
    if (std.fmt.allocPrintSentinel(a, "Labels ({d})", .{labels.len}, 0)) |t| {
        gtk.gtk_box_append(ctx.content, widgets.sectionTitle(t));
    } else |_| {}
    for (labels) |l| gtk.gtk_box_append(ctx.content, labelRow(ctx, a, l));

    // New-label form.
    const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
    gtk.gtk_widget_set_margin_top(row, 4);
    const entry = gtk.gtk_entry_new();
    gtk.gtk_entry_set_placeholder_text(entry, "New label name");
    gtk.gtk_widget_set_hexpand(entry, 1);
    gtk.gtk_box_append(row, entry);
    ctx.new_label_entry = entry;
    const add = gtk.gtk_button_new_with_label("Add label");
    _ = gtk.g_signal_connect_data(add, "clicked", @ptrCast(&onCreateLabel), ctx, null, 0);
    _ = gtk.g_signal_connect_data(entry, "activate", @ptrCast(&onCreateLabel), ctx, null, 0);
    gtk.gtk_box_append(row, add);
    gtk.gtk_box_append(ctx.content, row);
}

fn labelRow(ctx: *Ctx, a: std.mem.Allocator, l: Database.LabelRow) gtk.Object {
    const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    gtk.gtk_widget_set_margin_top(row, 2);

    const lrc = ctx.gpa.create(LabelRowCtx) catch return row;
    lrc.ctx = ctx;
    lrc.gpa = ctx.gpa;
    lrc.name_entry = null;
    lrc.selected_swatch = null;
    lrc.id = ctx.gpa.dupeZ(u8, l.id) catch {
        ctx.gpa.destroy(lrc);
        return row;
    };
    gtk.g_object_set_data_full(row, "exp-lrc", @ptrCast(lrc), @ptrCast(&freeLabelRow));

    // Colour menu-button (current colour dot → swatch popover).
    const color_btn = gtk.gtk_menu_button_new();
    gtk.gtk_widget_add_css_class(color_btn, "flat");
    gtk.gtk_menu_button_set_child(color_btn, widgets.dot(l.color));
    const pop = gtk.gtk_popover_new();
    const grid = widgets.swatchGrid(@ptrCast(lrc), @ptrCast(&onLabelColor), l.color, &lrc.selected_swatch);
    gtk.gtk_popover_set_child(pop, grid);
    gtk.gtk_menu_button_set_popover(color_btn, pop);
    gtk.gtk_box_append(row, color_btn);

    // Name entry (rename on Enter).
    const entry = gtk.gtk_entry_new();
    if (a.dupeZ(u8, l.name)) |z| gtk.gtk_editable_set_text(entry, z.ptr) else |_| {}
    gtk.gtk_widget_set_hexpand(entry, 1);
    lrc.name_entry = entry;
    _ = gtk.g_signal_connect_data(entry, "activate", @ptrCast(&onLabelRename), lrc, null, 0);
    gtk.gtk_box_append(row, entry);

    // Delete (direct — labels are cheap to recreate; mirrors member "Remove").
    const del = gtk.gtk_button_new_with_label("Delete");
    gtk.gtk_widget_add_css_class(del, "flat");
    if (makeAction(ctx, .delete_label, l.id)) |ac| {
        gtk.g_object_set_data_full(del, "exp-ctx", @ptrCast(ac), @ptrCast(&freeAction));
        _ = gtk.g_signal_connect_data(del, "clicked", @ptrCast(&onAction), ac, null, 0);
    }
    gtk.gtk_box_append(row, del);
    return row;
}

fn onLabelRename(entry: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const lrc: *LabelRowCtx = @ptrCast(@alignCast(data));
    const ctx = lrc.ctx;
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    const a = arena.allocator();
    const name = std.mem.trim(u8, std.mem.span(gtk.gtk_editable_get_text(entry)), " \t");
    if (name.len == 0) return;
    call(ctx, a, "labels.update", .{ .workspaceId = ctx.ws_id, .labelId = lrc.id, .name = name });
    rebuild(ctx); // frees lrc — don't touch it after
}

fn onLabelColor(btn: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const lrc: *LabelRowCtx = @ptrCast(@alignCast(data));
    const ctx = lrc.ctx;
    const raw = gtk.g_object_get_data(btn, "exp-color") orelse return;
    const color = std.mem.span(@as([*:0]const u8, @ptrCast(raw)));
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    const a = arena.allocator();
    call(ctx, a, "labels.update", .{ .workspaceId = ctx.ws_id, .labelId = lrc.id, .color = color });
    rebuild(ctx); // frees lrc — don't touch it after
}

fn onCreateLabel(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    const entry = ctx.new_label_entry orelse return;
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    const a = arena.allocator();
    const name = std.mem.trim(u8, std.mem.span(gtk.gtk_editable_get_text(entry)), " \t");
    if (name.len == 0) return;
    call(ctx, a, "labels.create", .{ .workspaceId = ctx.ws_id, .name = name, .color = "#6366f1" });
    rebuild(ctx);
}

// --- Coding section (JetBrains-SDK-style desktop launcher settings, §4b) -----
//
// Editable, prefilled defaults for the native "Start coding" launcher: the
// `claude` CLI path (+ a version doctor), the clone/worktree root, the branch
// prefix, plus a pure STATUS row for the user's personal API key — the expu_
// key is invisible plumbing, auto-minted by the launcher on the first coding
// session (EXP-2) and written into each worktree's `.mcp.json`. Regenerate is
// the only control. Values persist to `credentials.zig`'s desktop-settings
// store, NOT the workspace.

fn codingSection(ctx: *Ctx, a: std.mem.Allocator) void {
    gtk.gtk_box_append(ctx.content, widgets.sectionTitle("Coding"));

    // Read current values (dup into the rebuild arena so the store can close).
    var claude_path: []const u8 = credentials.default_claude_path;
    var repos_root: []const u8 = "";
    var branch_prefix: []const u8 = credentials.default_branch_prefix;
    var key_start: ?[]const u8 = null;
    if (credentials.Store.open(ctx.gpa)) |opened| {
        var store = opened;
        defer store.deinit();
        claude_path = a.dupe(u8, store.claudePath()) catch claude_path;
        branch_prefix = a.dupe(u8, store.branchPrefix()) catch branch_prefix;
        if (store.reposRoot(a)) |r| repos_root = r else |_| {}
        if (store.personalKeyStart()) |s| key_start = a.dupe(u8, s) catch null;
    } else |_| {}
    if (repos_root.len == 0) {
        if (credentials.defaultReposRoot(a)) |r| repos_root = r else |_| {}
    }

    const hint = gtk.gtk_label_new("The `claude` CLI and `git` power the Start-coding button. These settings live on this machine.");
    gtk.gtk_widget_add_css_class(hint, "dim-label");
    gtk.gtk_label_set_wrap(hint, 1);
    gtk.gtk_label_set_xalign(hint, 0);
    gtk.gtk_box_append(ctx.content, hint);

    ctx.coding_claude_entry = labeledEntry(ctx, "Claude CLI path", claude_path, "Save", &onCodingSaveClaude);
    // Doctor: `claude --version` + `git --version`.
    const doctor_row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    const doctor_btn = gtk.gtk_button_new_with_label("Check tools");
    gtk.gtk_widget_add_css_class(doctor_btn, "flat");
    gtk.gtk_widget_set_halign(doctor_btn, gtk.ALIGN_START);
    _ = gtk.g_signal_connect_data(doctor_btn, "clicked", @ptrCast(&onCodingDoctor), ctx, null, 0);
    gtk.gtk_box_append(doctor_row, doctor_btn);
    const doctor_lbl = gtk.gtk_label_new("");
    gtk.gtk_widget_add_css_class(doctor_lbl, "dim-label");
    gtk.gtk_label_set_xalign(doctor_lbl, 0);
    gtk.gtk_label_set_wrap(doctor_lbl, 1);
    gtk.gtk_widget_set_hexpand(doctor_lbl, 1);
    gtk.gtk_box_append(doctor_row, doctor_lbl);
    ctx.coding_doctor_label = doctor_lbl;
    gtk.gtk_box_append(ctx.content, doctor_row);

    ctx.coding_repos_entry = labeledEntry(ctx, "Repos & worktrees root", repos_root, "Save", &onCodingSaveRepos);
    ctx.coding_prefix_entry = labeledEntry(ctx, "Branch prefix", branch_prefix, "Save", &onCodingSavePrefix);

    // Personal API key — status only (EXP-2): auto-minted by the launcher on
    // the first coding session; Regenerate is the only control.
    if (key_start) |start| {
        const lbl = gtk.gtk_label_new(null);
        if (std.fmt.allocPrintSentinel(a, "Personal API key: active · {s}… — authenticates the coding agent as you, written into each worktree's .mcp.json.", .{start}, 0)) |t| {
            gtk.gtk_label_set_text(lbl, t.ptr);
        } else |_| {}
        gtk.gtk_widget_add_css_class(lbl, "dim-label");
        gtk.gtk_label_set_wrap(lbl, 1);
        gtk.gtk_widget_set_halign(lbl, gtk.ALIGN_START);
        gtk.gtk_box_append(ctx.content, lbl);
        const regen = gtk.gtk_button_new_with_label("Regenerate key");
        gtk.gtk_widget_add_css_class(regen, "flat");
        gtk.gtk_widget_set_halign(regen, gtk.ALIGN_START);
        _ = gtk.g_signal_connect_data(regen, "clicked", @ptrCast(&onCodingRegenKey), ctx, null, 0);
        gtk.gtk_box_append(ctx.content, regen);
    } else {
        const lbl = gtk.gtk_label_new("Personal API key: created automatically on your first coding session.");
        gtk.gtk_widget_add_css_class(lbl, "dim-label");
        gtk.gtk_label_set_wrap(lbl, 1);
        gtk.gtk_widget_set_halign(lbl, gtk.ALIGN_START);
        gtk.gtk_box_append(ctx.content, lbl);
    }
    // Freshly minted key shown once (read-only; select + copy).
    if (ctx.last_minted_key) |key| {
        const note = gtk.gtk_label_new("Copy your new key now — it won't be shown in full again:");
        gtk.gtk_widget_add_css_class(note, "dim-label");
        gtk.gtk_widget_set_halign(note, gtk.ALIGN_START);
        gtk.gtk_box_append(ctx.content, note);
        const entry = gtk.gtk_entry_new();
        gtk.gtk_editable_set_text(entry, key.ptr);
        gtk.gtk_editable_set_editable(entry, 0);
        gtk.gtk_box_append(ctx.content, entry);
    }
}

/// A "label: [entry] [Save]" row; returns the entry (owned by the widget tree).
fn labeledEntry(ctx: *Ctx, label: []const u8, value: []const u8, btn_label: [*:0]const u8, handler: *const fn (gtk.Object, gtk.gpointer) callconv(.c) void) gtk.Object {
    const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    gtk.gtk_widget_set_margin_top(row, 2);
    const lbl = gtk.gtk_label_new(null);
    var buf: [64]u8 = undefined;
    if (std.fmt.bufPrintZ(&buf, "{s}", .{label[0..@min(label.len, buf.len - 1)]})) |z| gtk.gtk_label_set_text(lbl, z.ptr) else |_| {}
    gtk.gtk_widget_add_css_class(lbl, "dim-label");
    gtk.gtk_box_append(row, lbl);
    const entry = gtk.gtk_entry_new();
    gtk.gtk_widget_set_hexpand(entry, 1);
    var vbuf: [512]u8 = undefined;
    if (std.fmt.bufPrintZ(&vbuf, "{s}", .{value[0..@min(value.len, vbuf.len - 1)]})) |z| gtk.gtk_editable_set_text(entry, z.ptr) else |_| {}
    gtk.gtk_box_append(row, entry);
    const save = gtk.gtk_button_new_with_label(btn_label);
    gtk.gtk_widget_add_css_class(save, "flat");
    _ = gtk.g_signal_connect_data(save, "clicked", @ptrCast(handler), ctx, null, 0);
    _ = gtk.g_signal_connect_data(entry, "activate", @ptrCast(handler), ctx, null, 0);
    gtk.gtk_box_append(row, save);
    gtk.gtk_box_append(ctx.content, row);
    return entry;
}

fn entryText(entry: gtk.Object) []const u8 {
    return std.mem.trim(u8, std.mem.span(gtk.gtk_editable_get_text(entry)), " \t\r\n");
}

fn onCodingSaveClaude(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    const e = ctx.coding_claude_entry orelse return;
    saveCodingField(ctx, .claude, entryText(e));
}
fn onCodingSaveRepos(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    const e = ctx.coding_repos_entry orelse return;
    saveCodingField(ctx, .repos, entryText(e));
}
fn onCodingSavePrefix(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    const e = ctx.coding_prefix_entry orelse return;
    saveCodingField(ctx, .prefix, entryText(e));
}

const CodingField = enum { claude, repos, prefix };

fn saveCodingField(ctx: *Ctx, field: CodingField, value: []const u8) void {
    var store = credentials.Store.open(ctx.gpa) catch return;
    defer store.deinit();
    switch (field) {
        .claude => store.setClaudePath(value),
        .repos => store.setReposRoot(value),
        .prefix => store.setBranchPrefix(value),
    }
    store.save() catch {};
}

fn onCodingDoctor(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    const lbl = ctx.coding_doctor_label orelse return;
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    const claude = if (ctx.coding_claude_entry) |e| entryText(e) else credentials.default_claude_path;
    const claude_v = versionOf(a, if (claude.len > 0) claude else credentials.default_claude_path, "--version");
    const git_v = versionOf(a, "git", "--version");
    if (std.fmt.allocPrintSentinel(a, "claude: {s}\ngit: {s}", .{ claude_v, git_v }, 0)) |t| {
        gtk.gtk_label_set_text(lbl, t.ptr);
    } else |_| {}
}

/// Run `<prog> <arg>` and return trimmed stdout (or "not found" on failure).
fn versionOf(a: std.mem.Allocator, prog: []const u8, arg: []const u8) []const u8 {
    const res = std.process.run(a, std.Io.Threaded.global_single_threaded.io(), .{
        .argv = &.{ prog, arg },
        .stdout_limit = .limited(16 * 1024),
        .stderr_limit = .limited(16 * 1024),
    }) catch return "not found";
    if (!(res.term == .exited and res.term.exited == 0)) return "not found";
    const out = std.mem.trim(u8, res.stdout, " \t\r\n");
    return if (out.len > 0) out else "ok";
}

/// Mint a replacement key, then best-effort revoke ONLY the locally-stored old
/// key id — never blanket-revoke (other devices hold their own keys).
fn onCodingRegenKey(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    var store = credentials.Store.open(ctx.gpa) catch return;
    defer store.deinit();
    const old_id = store.personalKeyId(); // arena-owned; survives setPersonalKey

    const label = credentials.hostDeviceLabel(ctx.gpa);
    defer ctx.gpa.free(label);
    const name: []const u8 = std.fmt.allocPrint(a, "Device: {s}", .{label}) catch return;
    const input = std.json.Stringify.valueAlloc(a, .{ .name = name }, .{}) catch return;
    var resp = trpc.call(ctx.gpa, ctx.instance, "users.mintPersonalApiKey", input, ctx.token, 30) catch return;
    defer resp.deinit();
    if (!resp.ok()) return;
    var obj = trpc.asObject(resp.data() orelse return) orelse return;
    if (obj.get("json")) |inner| {
        if (trpc.asObject(inner)) |io| obj = io;
    }
    const key = trpc.objString(obj, "key") orelse return;
    const key_id = trpc.objString(obj, "id") orelse "";
    const start = trpc.objString(obj, "start") orelse key[0..@min(key.len, 9)];

    // Persist to the desktop-settings store.
    store.setPersonalKey(key, key_id, start);
    store.save() catch {};

    // Best-effort revoke of the replaced key (this device's old id only).
    if (old_id) |oid| {
        if (oid.len > 0) call(ctx, a, "users.revokePersonalApiKey", .{ .id = oid });
    }

    // Show the raw key once (freed on close / next rebuild).
    if (ctx.last_minted_key) |old| ctx.gpa.free(old);
    ctx.last_minted_key = ctx.gpa.dupeZ(u8, key) catch null;
    rebuild(ctx);
}

// --- Danger Zone + delete confirmation --------------------------------------

const DeleteKind = enum { project, workspace };

/// Carried by a delete button; opens a confirmation dialog on click.
const RowDel = struct { ctx: *Ctx, gpa: std.mem.Allocator, kind: DeleteKind, id: [:0]u8, name: [:0]u8 };

fn makeRowDel(ctx: *Ctx, kind: DeleteKind, id: []const u8, name: []const u8) ?*RowDel {
    const rd = ctx.gpa.create(RowDel) catch return null;
    rd.ctx = ctx;
    rd.gpa = ctx.gpa;
    rd.kind = kind;
    rd.id = ctx.gpa.dupeZ(u8, id) catch {
        ctx.gpa.destroy(rd);
        return null;
    };
    rd.name = ctx.gpa.dupeZ(u8, name) catch {
        ctx.gpa.free(rd.id);
        ctx.gpa.destroy(rd);
        return null;
    };
    return rd;
}

fn freeRowDel(p: gtk.gpointer) callconv(.c) void {
    const rd: *RowDel = @ptrCast(@alignCast(p));
    rd.gpa.free(rd.id);
    rd.gpa.free(rd.name);
    rd.gpa.destroy(rd);
}

fn dangerZone(ctx: *Ctx, ws_name: []const u8) void {
    gtk.gtk_box_append(ctx.content, widgets.sectionTitle("Danger zone"));
    const desc = gtk.gtk_label_new("Deleting a workspace permanently removes it and all its projects and issues.");
    gtk.gtk_widget_add_css_class(desc, "dim-label");
    gtk.gtk_widget_set_halign(desc, gtk.ALIGN_START);
    gtk.gtk_label_set_wrap(desc, 1);
    gtk.gtk_box_append(ctx.content, desc);

    const del = gtk.gtk_button_new_with_label("Delete workspace");
    gtk.gtk_widget_add_css_class(del, "destructive-action");
    gtk.gtk_widget_set_halign(del, gtk.ALIGN_START);
    if (makeRowDel(ctx, .workspace, ctx.ws_id, ws_name)) |rd| {
        gtk.g_object_set_data_full(del, "exp-rd", @ptrCast(rd), @ptrCast(&freeRowDel));
        _ = gtk.g_signal_connect_data(del, "clicked", @ptrCast(&onOpenConfirm), rd, null, 0);
    }
    gtk.gtk_box_append(ctx.content, del);
}

/// Confirmation dialog state (owns its duped id/expected-name, freed on close).
const ConfirmCtx = struct {
    ctx: *Ctx,
    gpa: std.mem.Allocator, // long-lived; freeConfirm can run after ctx is freed
    kind: DeleteKind,
    id: [:0]u8,
    expect: [:0]u8, // type-to-confirm text (workspace name); "" = no match required
    dialog: gtk.Object = null,
    entry: gtk.Object = null,
    confirm_btn: gtk.Object = null,
};

fn freeConfirm(p: gtk.gpointer) callconv(.c) void {
    const cc: *ConfirmCtx = @ptrCast(@alignCast(p));
    cc.gpa.free(cc.id);
    cc.gpa.free(cc.expect);
    cc.gpa.destroy(cc);
}

fn onOpenConfirm(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const rd: *RowDel = @ptrCast(@alignCast(data));
    const ctx = rd.ctx;
    const require_match = rd.kind == .workspace;

    const cc = ctx.gpa.create(ConfirmCtx) catch return;
    cc.ctx = ctx;
    cc.gpa = ctx.gpa;
    cc.kind = rd.kind;
    cc.entry = null;
    cc.confirm_btn = null;
    cc.id = ctx.gpa.dupeZ(u8, rd.id) catch {
        ctx.gpa.destroy(cc);
        return;
    };
    cc.expect = ctx.gpa.dupeZ(u8, rd.name) catch {
        ctx.gpa.free(cc.id);
        ctx.gpa.destroy(cc);
        return;
    };

    const dialog = gtk.adw_dialog_new();
    gtk.adw_dialog_set_title(dialog, if (rd.kind == .workspace) "Delete workspace" else "Delete project");
    gtk.adw_dialog_set_content_width(dialog, 420);
    cc.dialog = dialog;

    const tv = gtk.adw_toolbar_view_new();
    const header = gtk.adw_header_bar_new();
    const cancel = gtk.gtk_button_new_with_label("Cancel");
    _ = gtk.g_signal_connect_data(cancel, "clicked", @ptrCast(&onConfirmCancel), cc, null, 0);
    gtk.adw_header_bar_pack_start(header, cancel);
    gtk.adw_toolbar_view_add_top_bar(tv, header);

    const form = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 10);
    gtk.gtk_widget_set_margin_top(form, 16);
    gtk.gtk_widget_set_margin_bottom(form, 16);
    gtk.gtk_widget_set_margin_start(form, 16);
    gtk.gtk_widget_set_margin_end(form, 16);

    const msg = gtk.gtk_label_new(null);
    var buf: [320]u8 = undefined;
    const text = if (rd.kind == .workspace)
        std.fmt.bufPrintZ(&buf, "Permanently delete “{s}” and everything in it? This cannot be undone.", .{rd.name}) catch "Delete this workspace?"
    else
        std.fmt.bufPrintZ(&buf, "Permanently delete “{s}” and all its issues?", .{rd.name}) catch "Delete this project?";
    gtk.gtk_label_set_text(msg, text.ptr);
    gtk.gtk_label_set_wrap(msg, 1);
    gtk.gtk_widget_set_halign(msg, gtk.ALIGN_START);
    gtk.gtk_box_append(form, msg);

    if (require_match) {
        const hint = gtk.gtk_label_new(null);
        var hbuf: [200]u8 = undefined;
        if (std.fmt.bufPrintZ(&hbuf, "Type the workspace name to confirm:", .{})) |z| gtk.gtk_label_set_text(hint, z.ptr) else |_| {}
        gtk.gtk_widget_add_css_class(hint, "dim-label");
        gtk.gtk_widget_set_halign(hint, gtk.ALIGN_START);
        gtk.gtk_box_append(form, hint);
        const entry = gtk.gtk_entry_new();
        gtk.gtk_entry_set_placeholder_text(entry, rd.name.ptr);
        cc.entry = entry;
        _ = gtk.g_signal_connect_data(entry, "changed", @ptrCast(&onConfirmEntryChanged), cc, null, 0);
        gtk.gtk_box_append(form, entry);
    }

    const confirm = gtk.gtk_button_new_with_label(if (rd.kind == .workspace) "Delete workspace" else "Delete project");
    gtk.gtk_widget_add_css_class(confirm, "destructive-action");
    gtk.gtk_widget_set_halign(confirm, gtk.ALIGN_END);
    if (require_match) gtk.gtk_widget_set_sensitive(confirm, 0); // enabled once the name matches
    cc.confirm_btn = confirm;
    _ = gtk.g_signal_connect_data(confirm, "clicked", @ptrCast(&onConfirmDelete), cc, null, 0);
    gtk.gtk_box_append(form, confirm);

    gtk.adw_toolbar_view_set_content(tv, form);
    gtk.adw_dialog_set_child(dialog, tv);
    _ = gtk.g_signal_connect_data(dialog, "closed", @ptrCast(&onConfirmClosed), cc, null, 0);
    gtk.adw_dialog_present(dialog, ctx.dialog);
}

fn onConfirmEntryChanged(entry: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const cc: *ConfirmCtx = @ptrCast(@alignCast(data));
    const typed = std.mem.span(gtk.gtk_editable_get_text(entry));
    if (cc.confirm_btn) |btn| {
        gtk.gtk_widget_set_sensitive(btn, if (std.mem.eql(u8, typed, cc.expect)) 1 else 0);
    }
}

fn onConfirmCancel(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const cc: *ConfirmCtx = @ptrCast(@alignCast(data));
    _ = gtk.adw_dialog_close(cc.dialog);
}

fn onConfirmClosed(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    freeConfirm(data);
}

fn onConfirmDelete(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const cc: *ConfirmCtx = @ptrCast(@alignCast(data));
    const ctx = cc.ctx;
    const kind = cc.kind;
    {
        var arena = std.heap.ArenaAllocator.init(ctx.gpa);
        defer arena.deinit();
        const a = arena.allocator();
        switch (kind) {
            .project => call(ctx, a, "projects.delete", .{ .projectId = cc.id }),
            .workspace => call(ctx, a, "workspaces.delete", .{ .workspaceId = ctx.ws_id }),
        }
    }
    _ = gtk.adw_dialog_close(cc.dialog); // frees cc via onConfirmClosed
    if (kind == .workspace) {
        _ = gtk.adw_dialog_close(ctx.dialog); // workspace gone — close settings
    } else {
        rebuild(ctx);
    }
}

fn memberRow(ctx: *Ctx, arena: std.mem.Allocator, m: Database.MemberRow, owner_count: usize) gtk.Object {
    const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    gtk.gtk_widget_set_margin_top(row, 2);
    if (m.name.len > 0) gtk.gtk_box_append(row, widgets.avatar(arena, m.name));

    const vb = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 0);
    gtk.gtk_widget_set_hexpand(vb, 1);
    const is_self = if (ctx.current_user_id) |uid| std.mem.eql(u8, uid, m.user_id) else false;
    const name_lbl = gtk.gtk_label_new(null);
    const name_txt = if (is_self)
        std.fmt.allocPrintSentinel(arena, "{s} (you)", .{m.name}, 0) catch null
    else
        arena.dupeZ(u8, m.name) catch null;
    if (name_txt) |t| gtk.gtk_label_set_text(name_lbl, t.ptr);
    gtk.gtk_widget_set_halign(name_lbl, gtk.ALIGN_START);
    gtk.gtk_box_append(vb, name_lbl);
    if (m.email.len > 0) {
        const email = gtk.gtk_label_new(null);
        if (arena.dupeZ(u8, m.email)) |z| gtk.gtk_label_set_text(email, z.ptr) else |_| {}
        gtk.gtk_widget_add_css_class(email, "dim-label");
        gtk.gtk_widget_add_css_class(email, "caption");
        gtk.gtk_widget_set_halign(email, gtk.ALIGN_START);
        gtk.gtk_box_append(vb, email);
    }
    gtk.gtk_box_append(row, vb);

    const role_lbl = gtk.gtk_label_new(m.role.ptr);
    gtk.gtk_widget_add_css_class(role_lbl, "dim-label");
    gtk.gtk_box_append(row, role_lbl);

    // The widget helpdesk bot is managed elsewhere; no actions for it. The last
    // owner of a workspace can't be demoted or leave (mirrors the server guard).
    const is_last_owner = std.mem.eql(u8, m.role, "owner") and owner_count <= 1;
    const can_make_owner = !std.mem.eql(u8, m.role, "owner");
    const can_make_member = !std.mem.eql(u8, m.role, "member") and !is_last_owner;
    const can_leave = is_self and !is_last_owner;
    const can_remove = !is_self;
    if (!m.is_agent and (can_make_owner or can_make_member or can_leave or can_remove)) {
        const menu = gtk.gtk_menu_button_new();
        gtk.gtk_menu_button_set_child(menu, gtk.gtk_label_new("⋯"));
        const pop = gtk.gtk_popover_new();
        const list = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 2);
        if (can_make_owner)
            addAction(ctx, list, .make_owner, m.id, "Make owner");
        if (can_make_member)
            addAction(ctx, list, .make_member, m.id, "Make member");
        if (can_leave)
            addAction(ctx, list, .remove_member, m.id, "Leave");
        if (can_remove)
            addAction(ctx, list, .remove_member, m.id, "Remove");
        gtk.gtk_popover_set_child(pop, list);
        gtk.gtk_menu_button_set_popover(menu, pop);
        gtk.gtk_box_append(row, menu);
    }
    return row;
}

fn inviteRow(ctx: *Ctx, arena: std.mem.Allocator, inv: Database.InviteRow) gtk.Object {
    const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    const info = gtk.gtk_label_new(null);
    const exp = if (inv.expires_at.len >= 10) inv.expires_at[0..10] else inv.expires_at;
    if (std.fmt.allocPrintSentinel(arena, "{s} · expires {s}", .{ inv.role, exp }, 0)) |t| {
        gtk.gtk_label_set_text(info, t.ptr);
    } else |_| {}
    gtk.gtk_widget_set_halign(info, gtk.ALIGN_START);
    gtk.gtk_widget_set_hexpand(info, 1);
    gtk.gtk_box_append(row, info);

    const revoke = gtk.gtk_button_new_with_label("Revoke");
    gtk.gtk_widget_add_css_class(revoke, "flat");
    const ac = makeAction(ctx, .revoke_invite, inv.id) orelse return row;
    gtk.g_object_set_data_full(revoke, "exp-ctx", @ptrCast(ac), @ptrCast(&freeAction));
    _ = gtk.g_signal_connect_data(revoke, "clicked", @ptrCast(&onAction), ac, null, 0);
    gtk.gtk_box_append(row, revoke);
    return row;
}

// --- per-action context (member role/remove, invite revoke) ---

const ActionKind = enum { make_owner, make_member, remove_member, revoke_invite, delete_label };

const ActionCtx = struct {
    ctx: *Ctx,
    gpa: std.mem.Allocator, // long-lived; freeAction can run after ctx is freed
    id: [:0]u8,
    kind: ActionKind,
};

fn makeAction(ctx: *Ctx, kind: ActionKind, id: []const u8) ?*ActionCtx {
    const ac = ctx.gpa.create(ActionCtx) catch return null;
    ac.ctx = ctx;
    ac.gpa = ctx.gpa;
    ac.kind = kind;
    ac.id = ctx.gpa.dupeZ(u8, id) catch {
        ctx.gpa.destroy(ac);
        return null;
    };
    return ac;
}

fn freeAction(p: gtk.gpointer) callconv(.c) void {
    const ac: *ActionCtx = @ptrCast(@alignCast(p));
    ac.gpa.free(ac.id);
    ac.gpa.destroy(ac);
}

fn addAction(ctx: *Ctx, list: gtk.Object, kind: ActionKind, id: []const u8, label: [*:0]const u8) void {
    const btn = gtk.gtk_button_new_with_label(label);
    gtk.gtk_widget_add_css_class(btn, "flat");
    const ac = makeAction(ctx, kind, id) orelse return;
    gtk.g_object_set_data_full(btn, "exp-ctx", @ptrCast(ac), @ptrCast(&freeAction));
    _ = gtk.g_signal_connect_data(btn, "clicked", @ptrCast(&onAction), ac, null, 0);
    gtk.gtk_box_append(list, btn);
}

fn onAction(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ac: *ActionCtx = @ptrCast(@alignCast(data));
    const ctx = ac.ctx;
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    switch (ac.kind) {
        .make_owner => updateRole(ctx, a, ac.id, "owner"),
        .make_member => updateRole(ctx, a, ac.id, "member"),
        .remove_member => call(ctx, a, "workspaceMembers.remove", .{ .memberId = ac.id }),
        .revoke_invite => call(ctx, a, "workspaceInvites.revoke", .{ .id = ac.id }),
        .delete_label => call(ctx, a, "labels.delete", .{ .workspaceId = ctx.ws_id, .labelId = ac.id }),
    }
    rebuild(ctx); // frees ac (button destroyed) — don't touch ac after
}

fn updateRole(ctx: *Ctx, a: std.mem.Allocator, member_id: []const u8, role: []const u8) void {
    call(ctx, a, "workspaceMembers.updateRole", .{ .memberId = member_id, .role = role });
}

fn onRename(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    const a = arena.allocator();
    const name = std.mem.trim(u8, std.mem.span(gtk.gtk_editable_get_text(ctx.name_entry)), " \t");
    if (name.len == 0) return;
    call(ctx, a, "workspaces.update", .{ .id = ctx.ws_id, .name = name });
    rebuild(ctx);
}

fn onTogglePublic(check: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    const a = arena.allocator();
    const is_public = gtk.gtk_check_button_get_active(check) != 0;
    call(ctx, a, "workspaces.update", .{ .id = ctx.ws_id, .isPublic = is_public });
    rebuild(ctx);
}

fn onPolicyChanged(dd: gtk.Object, _: gtk.gpointer, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    const a = arena.allocator();
    const policy: []const u8 = if (gtk.gtk_drop_down_get_selected(dd) == 1) "everyone" else "members";
    call(ctx, a, "workspaces.update", .{ .id = ctx.ws_id, .publicWritePolicy = policy });
}

fn onGenerate(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *Ctx = @ptrCast(@alignCast(data));
    var arena = std.heap.ArenaAllocator.init(ctx.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    const json = std.json.Stringify.valueAlloc(a, .{ .workspaceId = ctx.ws_id, .role = "member" }, .{}) catch return;
    var resp = trpc.call(ctx.gpa, ctx.instance, "workspaceInvites.create", json, ctx.token, 30) catch return;
    defer resp.deinit();
    if (!resp.ok()) return;
    const obj = trpc.asObject(resp.data() orelse return) orelse return;
    const tok = trpc.objString(obj, "token") orelse return;
    const link = std.fmt.allocPrintSentinel(ctx.gpa, "{s}/invite/{s}", .{ std.mem.trimEnd(u8, ctx.instance, "/"), tok }, 0) catch return;
    if (ctx.last_link) |old| ctx.gpa.free(old);
    ctx.last_link = link;
    rebuild(ctx);
}

/// Blocking tRPC mutation (deliberate admin action). Errors are ignored; the
/// dialog re-reads local state, which sync corrects shortly after.
fn call(ctx: *Ctx, a: std.mem.Allocator, proc: []const u8, input: anytype) void {
    const json = std.json.Stringify.valueAlloc(a, input, .{}) catch return;
    var resp = trpc.call(ctx.gpa, ctx.instance, proc, json, ctx.token, 30) catch return;
    resp.deinit();
}
