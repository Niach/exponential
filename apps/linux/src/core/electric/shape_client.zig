//! Electric long-poll client for a single shape — port of the iOS
//! `ShapeClient.swift` run/pollOnce loop, per packages/electric-protocol/README.md.
//!
//! Loop: read the persisted cursor; `offset=-1` for the initial snapshot, then
//! `offset=…&handle=…&live=true` (server holds ~60s). On 409 or an inline
//! must-refetch, drop the cursor and flag a pending refetch so the *next* poll
//! prepends a synthetic must-refetch — `applyBatch` then does DELETE + INSERTs in
//! one transaction, so a reader never observes an empty table. Backoff on
//! transport error (500ms→30s), never on a normal long-poll close.

const std = @import("std");
const http = @import("../api/http.zig");
const shape = @import("shape_message.zig");
const dbmod = @import("../db/database.zig");
const Database = dbmod.Database;

// Zig 0.16 removed std.Thread.sleep (sleeps now need an Io). The sync threads
// just need a plain blocking sleep, so use libc nanosleep directly.
const timespec = extern struct { sec: isize, nsec: isize };
extern fn nanosleep(req: *const timespec, rem: ?*timespec) c_int;
fn sleepMs(ms: u64) void {
    const ts = timespec{
        .sec = @intCast(ms / 1000),
        .nsec = @intCast((ms % 1000) * std.time.ns_per_ms),
    };
    _ = nanosleep(&ts, null);
}

/// Sleep that wakes promptly when `cancel` is set — so a backed-off thread exits
/// within ~100ms of a stop request (this is what fixes the slow sign-out).
fn sleepCancellable(ms: u64, cancel: ?*std.atomic.Value(bool)) void {
    var left = ms;
    while (left > 0) {
        if (cancel) |c| if (c.load(.acquire)) return;
        const chunk = @min(left, 100);
        sleepMs(chunk);
        left -= chunk;
    }
}

pub const PollResult = struct {
    http_status: i64,
    message_count: usize,
    pending_refetch: bool,
};

pub const ShapeClient = struct {
    gpa: std.mem.Allocator,
    db: *Database,
    /// Cursor key + log label, e.g. "issues".
    shape_name: []const u8,
    /// Proxy path, e.g. "/api/shapes/issues".
    url_path: []const u8,
    /// Local SQLite table, e.g. "issues".
    table: []const u8,
    base_url: []const u8,
    token: ?[]const u8,
    /// Called (on the sync thread) after a poll that applied messages, so the UI
    /// can schedule a refresh (e.g. via g_idle_add). Optional.
    notify: ?*const fn (?*anyopaque) callconv(.c) void = null,
    notify_ctx: ?*anyopaque = null,
    /// When set true, an in-flight long-poll aborts within ~1s (for shutdown).
    cancel: ?*std.atomic.Value(bool) = null,

    /// One request + apply. Returns whether a refetch is now pending (so the
    /// caller threads it into the next call).
    pub fn pollOnce(self: *ShapeClient, pending_refetch_in: bool) !PollResult {
        var arena_state = std.heap.ArenaAllocator.init(self.gpa);
        defer arena_state.deinit();
        const arena = arena_state.allocator();

        const cursor = try self.db.getOffset(arena, self.shape_name);
        const url = try buildUrl(arena, self.base_url, self.url_path, cursor);

        var resp = try http.get(self.gpa, url, self.token, 90, self.cancel);
        defer resp.deinit();

        if (resp.status == 409) {
            // Stale handle — reset to an initial snapshot next time, but leave the
            // existing rows visible until they're replaced atomically.
            try self.db.deleteOffset(self.shape_name);
            return .{ .http_status = resp.status, .message_count = 0, .pending_refetch = true };
        }
        if (resp.status == 401) return error.Unauthorized;
        if (resp.status < 200 or resp.status >= 300) return error.HttpError;

        var batch = try shape.parse(self.gpa, resp.body);
        defer batch.deinit();

        var has_inline_refetch = false;
        for (batch.messages) |m| {
            if (m.kind == .must_refetch) {
                has_inline_refetch = true;
                break;
            }
        }

        if (has_inline_refetch) {
            try self.db.deleteOffset(self.shape_name);
            try self.applyFiltered(arena, batch.messages); // apply siblings, drop the control msg
            return .{ .http_status = resp.status, .message_count = batch.messages.len, .pending_refetch = true };
        }

        if (pending_refetch_in) {
            try self.applyWithLeadingRefetch(arena, batch.messages);
        } else {
            try self.db.applyBatch(self.table, batch.messages);
        }

        if (resp.handle) |h| {
            if (resp.offset) |o| try self.db.setOffset(self.shape_name, h, o);
        }

        return .{ .http_status = resp.status, .message_count = batch.messages.len, .pending_refetch = false };
    }

    /// Run the live loop until `stop` is set. Backoff doubles to 30s on transport
    /// error and resets on success; a normal long-poll close is the happy path.
    pub fn run(self: *ShapeClient, stop: *std.atomic.Value(bool)) void {
        var backoff_ms: u64 = 500;
        var pending = false;
        while (!stop.load(.acquire)) {
            if (self.pollOnce(pending)) |res| {
                backoff_ms = 500;
                pending = res.pending_refetch;
                if (res.message_count > 0) {
                    if (self.notify) |n| n(self.notify_ctx);
                }
                if (pending) sleepCancellable(500, self.cancel);
            } else |_| {
                sleepCancellable(backoff_ms, self.cancel);
                backoff_ms = nextBackoff(backoff_ms);
            }
        }
    }

    fn applyFiltered(self: *ShapeClient, arena: std.mem.Allocator, messages: []const shape.Message) !void {
        var filtered = try arena.alloc(shape.Message, messages.len);
        var n: usize = 0;
        for (messages) |m| {
            if (m.kind != .must_refetch) {
                filtered[n] = m;
                n += 1;
            }
        }
        try self.db.applyBatch(self.table, filtered[0..n]);
    }

    fn applyWithLeadingRefetch(self: *ShapeClient, arena: std.mem.Allocator, messages: []const shape.Message) !void {
        var combined = try arena.alloc(shape.Message, messages.len + 1);
        combined[0] = .{ .kind = .must_refetch };
        @memcpy(combined[1..], messages);
        try self.db.applyBatch(self.table, combined);
    }
};

/// Build the shape request URL (null-terminated for libcurl).
pub fn buildUrl(
    arena: std.mem.Allocator,
    base_url: []const u8,
    url_path: []const u8,
    cursor: ?Database.Cursor,
) ![:0]u8 {
    if (cursor) |cur| {
        return std.fmt.allocPrintSentinel(
            arena,
            "{s}{s}?offset={s}&handle={s}&live=true",
            .{ base_url, url_path, cur.offset, cur.handle },
            0,
        );
    }
    return std.fmt.allocPrintSentinel(arena, "{s}{s}?offset=-1", .{ base_url, url_path }, 0);
}

pub fn nextBackoff(ms: u64) u64 {
    return @min(ms * 2, 30_000);
}

// --- tests (pure logic; no network) ---

test "buildUrl: initial snapshot uses offset=-1" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const url = try buildUrl(arena.allocator(), "https://x.dev", "/api/shapes/issues", null);
    try std.testing.expectEqualStrings("https://x.dev/api/shapes/issues?offset=-1", url);
}

test "buildUrl: live loop carries offset + handle" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const url = try buildUrl(arena.allocator(), "https://x.dev", "/api/shapes/issues", .{ .handle = "h-1", .offset = "0_5" });
    try std.testing.expectEqualStrings("https://x.dev/api/shapes/issues?offset=0_5&handle=h-1&live=true", url);
}

test "nextBackoff doubles and caps at 30s" {
    try std.testing.expectEqual(@as(u64, 1000), nextBackoff(500));
    try std.testing.expectEqual(@as(u64, 30_000), nextBackoff(20_000));
    try std.testing.expectEqual(@as(u64, 30_000), nextBackoff(30_000));
}
