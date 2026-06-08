const std = @import("std");
const Database = @import("core/db/database.zig").Database;
const ShapeClient = @import("core/electric/shape_client.zig").ShapeClient;
const sync = @import("core/electric/sync_manager.zig");
const auth_api = @import("core/auth/auth_api.zig");
const ServerAccount = @import("core/auth/server_account.zig").ServerAccount;
const AccountStore = @import("core/auth/account_store.zig").AccountStore;
const registration = @import("core/agent/registration.zig");
const identity_store = @import("core/agent/identity_store.zig");
const storage = @import("core/storage.zig");
const ui = @import("ui/app.zig");
const terminal = @import("ui/terminal.zig");
const agent_manager = @import("core/agent/agent_manager.zig");

extern fn setenv(name: [*:0]const u8, value: [*:0]const u8, overwrite: c_int) c_int;

pub fn main(init: std.process.Init) !void {
    // Make GTK4's GL backend reliable for the embedded ghostty terminal (and
    // GLArea generally) on NVIDIA/X11 — must be set before any GDK init. These
    // mirror exactly what Ghostty's own GTK apprt forces at startup: desktop GL
    // only (no GLES), no fractional GL scaling, no Vulkan renderer, no GDK color
    // management. Without these, GSK framebuffer setup fails ("fb setup not
    // supported") and GL contexts won't realize.
    _ = setenv("GDK_DEBUG", "gl-disable-gles,gl-no-fractional,vulkan-disable", 1);
    _ = setenv("GDK_DISABLE", "color-mgmt", 1);

    const args = try init.minimal.args.toSlice(init.arena.allocator());
    // Stash the process env so background workers (agent-core run handler) can
    // seed child-process environments without threading `init` everywhere.
    storage.setEnviron(init.environ_map);
    // Shape threads allocate concurrently outside the DB lock, so use a
    // thread-safe allocator (not the debug gpa).
    const gpa = std.heap.page_allocator;

    if (args.len >= 2) {
        const cmd = args[1];
        if (std.mem.eql(u8, cmd, "login")) return login(gpa, args[2..]);
        if (std.mem.eql(u8, cmd, "register")) return register(gpa, args[2..]);
        if (std.mem.eql(u8, cmd, "accounts")) return listAccounts(gpa);
        if (std.mem.eql(u8, cmd, "shape-smoke")) return shapeSmoke(gpa, args[2..]);
        if (std.mem.eql(u8, cmd, "sync-smoke")) return syncSmoke(gpa, args[2..]);
        if (std.mem.eql(u8, cmd, "term-smoke")) std.process.exit(terminal.runSmoke(gpa));
        if (std.mem.eql(u8, cmd, "run-smoke")) return runSmoke(gpa, args[2..]);
        if (std.mem.eql(u8, cmd, "--help") or std.mem.eql(u8, cmd, "-h")) {
            printUsage();
            return;
        }
    }

    // Default action: launch the GTK GUI.
    std.process.exit(ui.run(gpa, args));
}

/// Dev smoke: launch a wrapped command in an embedded ghostty terminal and log
/// the captured output + exit code (exercises the Stage-2 agent-run plumbing).
fn runSmoke(gpa: std.mem.Allocator, args: []const [:0]const u8) !void {
    if (args.len < 1) {
        std.debug.print("usage: exponential run-smoke <program> [args...]\n", .{});
        return;
    }
    const program: []const u8 = args[0];
    var list = std.ArrayListUnmanaged([]const u8).empty;
    defer list.deinit(gpa);
    for (args[1..]) |a| list.append(gpa, a) catch {};
    std.process.exit(agent_manager.runSmoke(gpa, program, list.items));
}

fn printUsage() void {
    std.debug.print(
        \\usage:
        \\  exponential login       <baseUrl> <email> <password>
        \\  exponential register    <baseUrl> <setupToken>
        \\  exponential accounts
        \\  exponential shape-smoke <baseUrl> <shape> [bearerToken]
        \\  exponential sync-smoke  <baseUrl> [bearerToken]
        \\
    , .{});
}

fn previewSecret(s: []const u8) []const u8 {
    return s[0..@min(s.len, 10)];
}

/// Sign in with email/password and persist the account.
fn login(gpa: std.mem.Allocator, args: []const [:0]const u8) !void {
    if (args.len < 3) {
        std.debug.print("usage: exponential login <baseUrl> <email> <password>\n", .{});
        return;
    }
    const base_url = args[0];
    var res = try auth_api.signInWithPassword(gpa, base_url, args[1], args[2], 30);
    defer res.deinit();

    if (!res.ok()) {
        std.debug.print("login failed (HTTP {d}): {s}\n", .{ res.status, res.errorMessage() orelse "unknown error" });
        return;
    }

    const id = try ServerAccount.makeId(gpa, base_url);
    defer gpa.free(id);

    var store = try AccountStore.open(gpa);
    defer store.deinit();
    try store.upsert(.{
        .id = id,
        .instance_url = base_url,
        .token = res.token(),
        .user_id = res.userId(),
        .user_email = res.email(),
        .user_name = res.name(),
        .is_admin = res.isAdmin(),
    });
    try store.save();

    std.debug.print(
        "login OK: account={s} user={s} <{s}> admin={} → saved to {s}\n",
        .{ id, res.name() orelse "?", res.email() orelse "?", res.isAdmin(), store.path },
    );
}

/// Register this machine as a desktop device and persist the identity.
fn register(gpa: std.mem.Allocator, args: []const [:0]const u8) !void {
    if (args.len < 2) {
        std.debug.print("usage: exponential register <baseUrl> <sessionToken> [deviceName]\n", .{});
        return;
    }
    const name = if (args.len >= 3) args[2] else "Desktop";
    const did = try registration.deviceId(gpa);
    defer gpa.free(did);
    var outcome = try registration.registerDevice(gpa, args[0], args[1], did, name, 30);
    switch (outcome) {
        .success => |*id| {
            defer id.deinit();
            std.debug.print(
                "registered: agent={s} ({s}) device={s} apiKey={s}…\n",
                .{ id.agent_name, id.agent_user_id, id.device_id, previewSecret(id.api_key) },
            );
            if (identity_store.save(gpa, id)) |path| {
                defer gpa.free(path);
                std.debug.print("  identity saved to {s}\n", .{path});
            } else |e| {
                std.debug.print("  warning: could not persist identity: {s}\n", .{@errorName(e)});
            }
        },
        .failure => |msg| {
            defer gpa.free(msg);
            std.debug.print("register failed: {s}\n", .{msg});
        },
    }
}

fn listAccounts(gpa: std.mem.Allocator) !void {
    var store = try AccountStore.open(gpa);
    defer store.deinit();
    const accts = store.list();
    std.debug.print("{d} account(s) — {s}\n", .{ accts.len, store.path });
    for (accts) |acc| {
        const active = if (store.active_id) |aid| std.mem.eql(u8, aid, acc.id) else false;
        std.debug.print(
            "  {s}{s}  {s}  <{s}>  admin={}  token={s}\n",
            .{
                if (active) "* " else "  ",
                acc.id,
                acc.instance_url,
                acc.user_email orelse "?",
                acc.is_admin,
                if (acc.token != null) "yes" else "no",
            },
        );
    }
}

/// Dev smoke: one Electric poll against a real backend, applied to an in-memory DB.
fn shapeSmoke(gpa: std.mem.Allocator, args: []const [:0]const u8) !void {
    if (args.len < 2) {
        std.debug.print("usage: exponential shape-smoke <baseUrl> <shape> [bearerToken]\n", .{});
        return;
    }
    const base_url = args[0];
    const shape_name = args[1];
    const token: ?[]const u8 = if (args.len >= 3) args[2] else null;

    const path = try std.fmt.allocPrint(gpa, "/api/shapes/{s}", .{shape_name});
    defer gpa.free(path);

    var db = try Database.open(gpa, ":memory:");
    defer db.close();

    var client = ShapeClient{
        .gpa = gpa,
        .db = &db,
        .shape_name = shape_name,
        .url_path = path,
        .table = shape_name,
        .base_url = base_url,
        .token = token,
    };

    const res = try client.pollOnce(false);
    const rows = db.count(shape_name) catch -1;
    std.debug.print(
        "shape-smoke {s}{s}: HTTP {d}, {d} messages, {s} rows = {d}\n",
        .{ base_url, path, res.http_status, res.message_count, shape_name, rows },
    );
}

/// Dev smoke: one concurrent initial poll across all 10 shapes.
fn syncSmoke(gpa: std.mem.Allocator, args: []const [:0]const u8) !void {
    if (args.len < 1) {
        std.debug.print("usage: exponential sync-smoke <baseUrl> [bearerToken]\n", .{});
        return;
    }
    const base_url = args[0];
    const token: ?[]const u8 = if (args.len >= 2) args[1] else null;

    var db = try Database.open(gpa, ":memory:");
    defer db.close();

    var mgr = sync.SyncManager{ .gpa = gpa, .db = &db, .base_url = base_url, .token = token };
    var outcomes: [sync.specs.len]sync.PollOutcome = undefined;
    mgr.pollAllOnce(&outcomes);

    var total_rows: i64 = 0;
    for (outcomes) |o| {
        const rows = db.count(tableFor(o.name)) catch -1;
        if (rows > 0) total_rows += rows;
        std.debug.print(
            "  {s:<18} HTTP {d} {d} msgs  rows={d}{s}\n",
            .{ o.name, o.status, o.messages, rows, if (o.err) "  ERR" else "" },
        );
    }
    std.debug.print("sync-smoke {s}: {d} total rows synced\n", .{ base_url, total_rows });
}

fn tableFor(shape_name: []const u8) []const u8 {
    for (sync.specs) |s| {
        if (std.mem.eql(u8, s.name, shape_name)) return s.table;
    }
    return shape_name;
}
