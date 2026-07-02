//! Electric long-poll client for a single shape — port of the iOS
//! `ShapeClient.swift` run/pollOnce loop, per packages/electric-protocol/README.md.
//!
//! Loop: read the persisted cursor; `offset=-1` for the initial snapshot, then
//! `offset=…&handle=…`, long-polling `live=true` only once up-to-date has been
//! seen (catch-up polls stay non-live). On 409 Electric sends the replacement
//! handle in the `electric-handle` response header — it is persisted at the
//! sentinel offset `-1` (a crash-safe needs-refetch marker), so the refetch
//! targets the new handle and a quit between the 409 and the completed refetch
//! resumes the refetch instead of a poisoned cursor. The refetch response is
//! applied with a leading synthetic must-refetch — `applyBatch` then does
//! DELETE + INSERTs in one transaction, so a reader never observes an empty
//! table. Backoff on transport error (500ms→30s), never on a normal long-poll
//! close; a 401 (dead credentials) additionally fires `on_auth_error` once so
//! the app can prompt a re-login instead of silently retrying forever.

const std = @import("std");
const http = @import("../api/http.zig");
const shape = @import("shape_message.zig");
const dbmod = @import("../db/database.zig");
const Database = dbmod.Database;

/// Electric's initial-snapshot offset. A cursor persisted at this offset is the
/// needs-refetch marker (its handle is the replacement handle from a 409, or
/// empty after an inline must-refetch).
pub const initial_offset = "-1";

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
    /// Pace the next poll (500ms): a refetch is pending after a 409 / inline
    /// must-refetch, or a non-live poll made no progress — so a response that
    /// never reaches up-to-date can't spin-request.
    pace: bool,
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
    /// Called (on the sync thread) when a poll comes back 401 — the credentials
    /// are dead server-side, so the app should surface a re-login prompt.
    /// De-duplicated across shape threads via `auth_error_once`.
    on_auth_error: ?*const fn (?*anyopaque) callconv(.c) void = null,
    auth_error_ctx: ?*anyopaque = null,
    /// Shared across all shape clients of one SyncManager so 14 threads hitting
    /// the same dead session produce ONE prompt, not fourteen. Null = fire on
    /// every 401.
    auth_error_once: ?*std.atomic.Value(bool) = null,
    /// When set true, an in-flight long-poll aborts within ~1s (for shutdown).
    cancel: ?*std.atomic.Value(bool) = null,
    /// Whether this shape has reached head (up-to-date seen). Only then do
    /// polls long-poll with live=true; catch-up polls stay non-live per the
    /// Electric protocol. Per-run state — a fresh client starts non-live, and
    /// the first poll returns immediately with up-to-date before going live.
    live: bool = false,

    /// One request + apply. Refetch state is read from (and persisted to) the
    /// cursor table, so a crash never loses a pending refetch.
    pub fn pollOnce(self: *ShapeClient) !PollResult {
        var arena_state = std.heap.ArenaAllocator.init(self.gpa);
        defer arena_state.deinit();
        const arena = arena_state.allocator();

        const cursor = try self.db.getOffset(arena, self.shape_name);
        // A cursor parked at offset=-1 is the persisted needs-refetch marker
        // (written on 409 / inline must-refetch) — it survives a quit between
        // the 409 and the refetch, so the atomic DELETE+reinsert still happens
        // after relaunch instead of resuming a poisoned handle.
        const refetching = if (cursor) |cur| std.mem.eql(u8, cur.offset, initial_offset) else false;
        const was_live = self.live and !refetching;
        const url = try buildUrl(arena, self.base_url, self.url_path, cursor, was_live);

        var resp = try http.get(self.gpa, url, self.token, 90, self.cancel);
        defer resp.deinit();

        if (resp.status == 409) {
            // The shape rotated. Electric sends the replacement handle in the
            // response header — persist it at the sentinel offset instead of
            // deleting the cursor, so the refetch targets the new handle. Don't
            // delete table data yet — leave stale rows visible until the next
            // poll re-fetches and replaces them atomically.
            const new_handle: []const u8 = resp.handle orelse "";
            try self.db.setOffset(self.shape_name, new_handle, initial_offset);
            self.live = false;
            return .{ .http_status = resp.status, .message_count = 0, .pace = true };
        }
        if (resp.status == 401) return error.Unauthorized;
        if (resp.status < 200 or resp.status >= 300) return error.HttpError;

        var batch = try shape.parse(self.gpa, resp.body);
        defer batch.deinit();

        var has_inline_refetch = false;
        var saw_up_to_date = false;
        for (batch.messages) |m| {
            switch (m.kind) {
                .must_refetch => has_inline_refetch = true,
                // Only up-to-date flips the shape live — snapshot-end merely
                // closes a snapshot chunk (dropped at parse time).
                .up_to_date => saw_up_to_date = true,
                else => {},
            }
        }

        if (has_inline_refetch) {
            // The old handle is dead (no replacement comes inline) — persist a
            // bare needs-refetch marker, apply the sibling messages, and let the
            // next poll do the atomic replacement.
            try self.db.setOffset(self.shape_name, "", initial_offset);
            self.live = false;
            try self.applyFiltered(arena, batch.messages); // apply siblings, drop the control msg
            return .{ .http_status = resp.status, .message_count = batch.messages.len, .pace = true };
        }

        if (refetching) {
            try self.applyWithLeadingRefetch(arena, batch.messages);
        } else {
            try self.db.applyBatch(self.table, batch.messages);
        }

        if (resp.handle) |h| {
            if (resp.offset) |o| try self.db.setOffset(self.shape_name, h, o);
        }
        self.live = saw_up_to_date or was_live;

        return .{
            .http_status = resp.status,
            .message_count = batch.messages.len,
            .pace = !was_live and !saw_up_to_date and batch.messages.len == 0,
        };
    }

    /// Run the live loop until `stop` is set. Backoff doubles to 30s on transport
    /// error and resets on success; a normal long-poll close is the happy path.
    pub fn run(self: *ShapeClient, stop: *std.atomic.Value(bool)) void {
        var backoff_ms: u64 = 500;
        while (!stop.load(.acquire)) {
            if (self.pollOnce()) |res| {
                backoff_ms = 500;
                if (res.message_count > 0) {
                    if (self.notify) |n| n(self.notify_ctx);
                }
                if (res.pace) sleepCancellable(500, self.cancel);
            } else |err| {
                // 401 = dead credentials (the server 401s rather than silently
                // serving the anonymous view) — surface it so the app can show
                // a re-login prompt; keep backing off in case it recovers.
                if (err == error.Unauthorized) self.reportAuthError();
                sleepCancellable(backoff_ms, self.cancel);
                backoff_ms = nextBackoff(backoff_ms);
            }
        }
    }

    fn reportAuthError(self: *ShapeClient) void {
        const cb = self.on_auth_error orelse return;
        if (self.auth_error_once) |flag| {
            if (flag.swap(true, .acq_rel)) return; // another shape already reported
        }
        cb(self.auth_error_ctx);
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
/// - no cursor → initial snapshot: `offset=-1`
/// - cursor parked at offset=-1 → post-409 refetch: `offset=-1` plus the
///   replacement handle Electric sent on the 409 (when we have one)
/// - normal cursor → `offset=…&handle=…`, with `live=true` only once the
///   snapshot completed (up-to-date seen).
pub fn buildUrl(
    arena: std.mem.Allocator,
    base_url: []const u8,
    url_path: []const u8,
    cursor: ?Database.Cursor,
    live: bool,
) ![:0]u8 {
    if (cursor) |cur| {
        if (std.mem.eql(u8, cur.offset, initial_offset)) {
            if (cur.handle.len == 0) {
                return std.fmt.allocPrintSentinel(arena, "{s}{s}?offset=-1", .{ base_url, url_path }, 0);
            }
            return std.fmt.allocPrintSentinel(
                arena,
                "{s}{s}?offset=-1&handle={s}",
                .{ base_url, url_path, cur.handle },
                0,
            );
        }
        if (live) {
            return std.fmt.allocPrintSentinel(
                arena,
                "{s}{s}?offset={s}&handle={s}&live=true",
                .{ base_url, url_path, cur.offset, cur.handle },
                0,
            );
        }
        return std.fmt.allocPrintSentinel(
            arena,
            "{s}{s}?offset={s}&handle={s}",
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
    const url = try buildUrl(arena.allocator(), "https://x.dev", "/api/shapes/issues", null, false);
    try std.testing.expectEqualStrings("https://x.dev/api/shapes/issues?offset=-1", url);
}

test "buildUrl: live loop carries offset + handle once up-to-date" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const url = try buildUrl(arena.allocator(), "https://x.dev", "/api/shapes/issues", .{ .handle = "h-1", .offset = "0_5" }, true);
    try std.testing.expectEqualStrings("https://x.dev/api/shapes/issues?offset=0_5&handle=h-1&live=true", url);
}

test "buildUrl: catch-up poll (no up-to-date yet) stays non-live" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const url = try buildUrl(arena.allocator(), "https://x.dev", "/api/shapes/issues", .{ .handle = "h-1", .offset = "0_5" }, false);
    try std.testing.expectEqualStrings("https://x.dev/api/shapes/issues?offset=0_5&handle=h-1", url);
}

test "buildUrl: post-409 refetch targets the replacement handle" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const url = try buildUrl(arena.allocator(), "https://x.dev", "/api/shapes/issues", .{ .handle = "h-2", .offset = initial_offset }, false);
    try std.testing.expectEqualStrings("https://x.dev/api/shapes/issues?offset=-1&handle=h-2", url);
}

test "buildUrl: refetch marker without a handle falls back to a bare snapshot" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const url = try buildUrl(arena.allocator(), "https://x.dev", "/api/shapes/issues", .{ .handle = "", .offset = initial_offset }, true);
    try std.testing.expectEqualStrings("https://x.dev/api/shapes/issues?offset=-1", url);
}

test "nextBackoff doubles and caps at 30s" {
    try std.testing.expectEqual(@as(u64, 1000), nextBackoff(500));
    try std.testing.expectEqual(@as(u64, 30_000), nextBackoff(20_000));
    try std.testing.expectEqual(@as(u64, 30_000), nextBackoff(30_000));
}
