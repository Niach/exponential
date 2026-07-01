//! Multi-shape sync orchestrator — port of the iOS `SyncManager`.
//!
//! Runs one `ShapeClient` per synced table on its own thread, all writing to the
//! single (mutex-guarded) `Database`. The 14 shapes match every other client
//! (web/iOS/Android). `gpa` MUST be thread-safe (page_allocator) since the shape
//! threads allocate concurrently outside the DB lock.

const std = @import("std");
const Database = @import("../db/database.zig").Database;
const ShapeClient = @import("shape_client.zig").ShapeClient;

pub const ShapeSpec = struct {
    name: []const u8,
    url_path: []const u8,
    table: []const u8,
};

/// The 14 Electric shapes mirrored by all clients (see CLAUDE.md "Mobile parity").
pub const specs = [_]ShapeSpec{
    .{ .name = "workspaces", .url_path = "/api/shapes/workspaces", .table = "workspaces" },
    .{ .name = "projects", .url_path = "/api/shapes/projects", .table = "projects" },
    .{ .name = "issues", .url_path = "/api/shapes/issues", .table = "issues" },
    .{ .name = "labels", .url_path = "/api/shapes/labels", .table = "labels" },
    .{ .name = "issue-labels", .url_path = "/api/shapes/issue-labels", .table = "issue_labels" },
    .{ .name = "users", .url_path = "/api/shapes/users", .table = "users" },
    .{ .name = "workspace-members", .url_path = "/api/shapes/workspace-members", .table = "workspace_members" },
    .{ .name = "workspace-invites", .url_path = "/api/shapes/workspace-invites", .table = "workspace_invites" },
    .{ .name = "comments", .url_path = "/api/shapes/comments", .table = "comments" },
    .{ .name = "attachments", .url_path = "/api/shapes/attachments", .table = "attachments" },
    .{ .name = "notifications", .url_path = "/api/shapes/notifications", .table = "notifications" },
    .{ .name = "issue-events", .url_path = "/api/shapes/issue-events", .table = "issue_events" },
    .{ .name = "issue-subscribers", .url_path = "/api/shapes/issue-subscribers", .table = "issue_subscribers" },
    .{ .name = "coding-sessions", .url_path = "/api/shapes/coding-sessions", .table = "coding_sessions" },
};

pub const PollOutcome = struct {
    name: []const u8 = "",
    status: i64 = 0,
    messages: usize = 0,
    err: bool = false,
};

pub const SyncManager = struct {
    gpa: std.mem.Allocator,
    db: *Database,
    base_url: []const u8,
    token: ?[]const u8,
    /// Forwarded to each ShapeClient — fired (on a sync thread) after any shape
    /// applies messages, so the UI can schedule a refresh.
    notify: ?*const fn (?*anyopaque) callconv(.c) void = null,
    notify_ctx: ?*anyopaque = null,
    stop_flag: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    threads: [specs.len]?std.Thread = [_]?std.Thread{null} ** specs.len,

    fn clientFor(self: *SyncManager, spec: ShapeSpec) ShapeClient {
        return .{
            .gpa = self.gpa,
            .db = self.db,
            .shape_name = spec.name,
            .url_path = spec.url_path,
            .table = spec.table,
            .base_url = self.base_url,
            .token = self.token,
            .notify = self.notify,
            .notify_ctx = self.notify_ctx,
            .cancel = &self.stop_flag,
        };
    }

    fn runWorker(self: *SyncManager, spec: ShapeSpec) void {
        var client = self.clientFor(spec);
        client.run(&self.stop_flag);
    }

    /// Launch the live loop for all shapes. Call `stop` to tear down.
    /// NOTE: a thread blocked in a ~60s `live=true` poll only exits once that
    /// request returns, so `stop` can take up to the long-poll window. Responsive
    /// cancellation (curl multi / abort) is a follow-up.
    pub fn start(self: *SyncManager) !void {
        for (specs, 0..) |spec, i| {
            self.threads[i] = try std.Thread.spawn(.{}, runWorker, .{ self, spec });
        }
    }

    pub fn stop(self: *SyncManager) void {
        self.stop_flag.store(true, .release);
        for (&self.threads) |*t| {
            if (t.*) |thread| {
                thread.join();
                t.* = null;
            }
        }
    }

    // --- one-shot helpers (used by the dev smoke; fast because initial snapshots
    //     return immediately rather than holding open like live=true) ---

    fn pollWorker(self: *SyncManager, spec: ShapeSpec, out: *PollOutcome) void {
        var client = self.clientFor(spec);
        if (client.pollOnce(false)) |res| {
            out.* = .{ .name = spec.name, .status = res.http_status, .messages = res.message_count, .err = false };
        } else |_| {
            out.* = .{ .name = spec.name, .status = 0, .messages = 0, .err = true };
        }
    }

    /// One concurrent initial poll across all shapes; fills `out` per shape.
    pub fn pollAllOnce(self: *SyncManager, out: *[specs.len]PollOutcome) void {
        var threads: [specs.len]?std.Thread = [_]?std.Thread{null} ** specs.len;
        for (specs, 0..) |spec, i| {
            threads[i] = std.Thread.spawn(.{}, pollWorker, .{ self, spec, &out[i] }) catch null;
            if (threads[i] == null) out[i] = .{ .name = spec.name, .err = true };
        }
        for (&threads) |*t| {
            if (t.*) |thread| thread.join();
        }
    }
};

test "shape registry: 14 shapes with matching tables" {
    try std.testing.expectEqual(@as(usize, 14), specs.len);
    // dashed shape name maps to the underscored SQLite table.
    for (specs) |s| {
        if (std.mem.eql(u8, s.name, "issue-labels")) {
            try std.testing.expectEqualStrings("issue_labels", s.table);
        }
        // every url_path is /api/shapes/<name>
        try std.testing.expect(std.mem.startsWith(u8, s.url_path, "/api/shapes/"));
    }
}
