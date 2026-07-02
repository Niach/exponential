//! GTK-free helpers for the host-side run-config launcher (masterplan §4c):
//! shell-quoting + launcher-script composition for a `command` run target, a
//! tab-key/script-name sanitizer, and the in-memory per-target run-history
//! ring the play menu reads ("last run: exit 0 · 2m ago"). Pure logic so the
//! headless test root can exercise it (`zig build test` links no GTK); the
//! spawn/widget glue lives in `run_launcher.zig`.

const std = @import("std");

/// POSIX-shell single-quote `s` (embedded `'` becomes `'\''`). The result is
/// always quoted, so any byte content passes through the shell verbatim.
pub fn shellQuote(a: std.mem.Allocator, s: []const u8) ![]u8 {
    var out = std.ArrayList(u8).empty;
    errdefer out.deinit(a);
    try out.append(a, '\'');
    for (s) |ch| {
        if (ch == '\'') {
            try out.appendSlice(a, "'\\''");
        } else {
            try out.append(a, ch);
        }
    }
    try out.append(a, '\'');
    return out.toOwnedSlice(a);
}

/// Compose the launcher script for a command target. The terminal spawns it as
/// `sh <script>` with cwd = the repo clone root, so argv elements with spaces
/// or quotes survive regardless of how ghostty tokenises its `command` string
/// (mirrors the coding launcher's `.exp-run.sh` trick). `cwd_rel` is the
/// repo-relative working directory (already validated — no `..`, not absolute).
pub fn buildRunScript(a: std.mem.Allocator, cwd_rel: ?[]const u8, argv: []const []const u8) ![]u8 {
    var out = std.ArrayList(u8).empty;
    errdefer out.deinit(a);
    try out.appendSlice(a, "#!/bin/sh\n");
    if (cwd_rel) |rel| {
        const q = try shellQuote(a, rel);
        defer a.free(q);
        try out.appendSlice(a, "cd ");
        try out.appendSlice(a, q);
        try out.appendSlice(a, " || exit 127\n");
    }
    try out.appendSlice(a, "exec");
    for (argv) |arg| {
        const q = try shellQuote(a, arg);
        defer a.free(q);
        try out.append(a, ' ');
        try out.appendSlice(a, q);
    }
    try out.append(a, '\n');
    return out.toOwnedSlice(a);
}

/// Map a run-target id onto a filesystem/token-safe name: `[A-Za-z0-9._-]`
/// pass through, everything else becomes `-`; output is capped at `buf.len`.
/// Never empty (an empty id yields "run").
pub fn sanitizeId(buf: []u8, id: []const u8) []const u8 {
    if (id.len == 0 or buf.len == 0) return "run";
    const n = @min(id.len, buf.len);
    for (id[0..n], 0..) |ch, i| {
        const ok = std.ascii.isAlphanumeric(ch) or ch == '.' or ch == '_' or ch == '-';
        buf[i] = if (ok) ch else '-';
    }
    return buf[0..n];
}

// libc time(2) — std.time.timestamp()/milliTimestamp() were removed in Zig
// 0.16 (same note as format.zig). Second precision is plenty for "2m ago".
extern "c" fn time(tloc: ?*i64) i64;

/// Wall-clock now in milliseconds (libc time(2) × 1000).
pub fn nowMs() i64 {
    return time(null) * std.time.ms_per_s;
}

// ---------------------------------------------------------------------------
// Run history — host-side state only (not synced, §4c). A bounded ring of
// finished runs; the play menu shows the newest record per target id.
// ---------------------------------------------------------------------------

pub const Record = struct {
    /// Owned by the history.
    target_id: []u8,
    exit_code: i32,
    ended_at_ms: i64,
};

pub const RunHistory = struct {
    pub const capacity = 32;

    gpa: std.mem.Allocator,
    records: std.ArrayListUnmanaged(Record) = .empty,

    pub fn init(gpa: std.mem.Allocator) RunHistory {
        return .{ .gpa = gpa };
    }

    pub fn deinit(self: *RunHistory) void {
        for (self.records.items) |r| self.gpa.free(r.target_id);
        self.records.deinit(self.gpa);
    }

    /// Append a finished run, evicting the oldest record past capacity.
    pub fn record(self: *RunHistory, target_id: []const u8, exit_code: i32, ended_at_ms: i64) void {
        const owned = self.gpa.dupe(u8, target_id) catch return;
        self.records.append(self.gpa, .{
            .target_id = owned,
            .exit_code = exit_code,
            .ended_at_ms = ended_at_ms,
        }) catch {
            self.gpa.free(owned);
            return;
        };
        if (self.records.items.len > capacity) {
            const evicted = self.records.orderedRemove(0);
            self.gpa.free(evicted.target_id);
        }
    }

    /// The most recent finished run of `target_id` (borrowed; valid until the
    /// next mutation).
    pub fn lastFor(self: *const RunHistory, target_id: []const u8) ?Record {
        var i = self.records.items.len;
        while (i > 0) {
            i -= 1;
            const r = self.records.items[i];
            if (std.mem.eql(u8, r.target_id, target_id)) return r;
        }
        return null;
    }
};

/// "just now" / "37s ago" / "2m ago" / "3h ago" / "5d ago" for the play menu.
pub fn ageLabel(buf: []u8, now_ms: i64, then_ms: i64) []const u8 {
    const delta_s = @divTrunc(@max(now_ms - then_ms, 0), std.time.ms_per_s);
    if (delta_s < 5) return "just now";
    const result = if (delta_s < 60)
        std.fmt.bufPrint(buf, "{d}s ago", .{delta_s})
    else if (delta_s < 60 * 60)
        std.fmt.bufPrint(buf, "{d}m ago", .{@divTrunc(delta_s, 60)})
    else if (delta_s < 24 * 60 * 60)
        std.fmt.bufPrint(buf, "{d}h ago", .{@divTrunc(delta_s, 60 * 60)})
    else
        std.fmt.bufPrint(buf, "{d}d ago", .{@divTrunc(delta_s, 24 * 60 * 60)});
    return result catch "earlier";
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

const testing = std.testing;

test "shellQuote wraps and escapes single quotes" {
    const a = testing.allocator;
    const plain = try shellQuote(a, "hello");
    defer a.free(plain);
    try testing.expectEqualStrings("'hello'", plain);

    const tricky = try shellQuote(a, "it's; rm -rf $HOME `x`");
    defer a.free(tricky);
    try testing.expectEqualStrings("'it'\\''s; rm -rf $HOME `x`'", tricky);
}

test "buildRunScript cds into the relative dir and execs quoted argv" {
    const a = testing.allocator;
    const script = try buildRunScript(a, "apps/web", &.{ "bun", "run", "dev --port 5173" });
    defer a.free(script);
    try testing.expectEqualStrings(
        "#!/bin/sh\ncd 'apps/web' || exit 127\nexec 'bun' 'run' 'dev --port 5173'\n",
        script,
    );

    const rootless = try buildRunScript(a, null, &.{"make"});
    defer a.free(rootless);
    try testing.expectEqualStrings("#!/bin/sh\nexec 'make'\n", rootless);
}

test "sanitizeId keeps safe chars and never returns empty" {
    var buf: [40]u8 = undefined;
    try testing.expectEqualStrings("web-dev", sanitizeId(&buf, "web-dev"));
    try testing.expectEqualStrings("a-b_c.1-", sanitizeId(&buf, "a b_c.1/"));
    try testing.expectEqualStrings("run", sanitizeId(&buf, ""));
    var tiny: [4]u8 = undefined;
    try testing.expectEqualStrings("long", sanitizeId(&tiny, "longer-than-four"));
}

test "RunHistory records, finds newest per target, and evicts past capacity" {
    var h = RunHistory.init(testing.allocator);
    defer h.deinit();

    try testing.expect(h.lastFor("t1") == null);
    h.record("t1", 0, 1000);
    h.record("t2", 1, 2000);
    h.record("t1", 2, 3000);
    const last = h.lastFor("t1").?;
    try testing.expectEqual(@as(i32, 2), last.exit_code);
    try testing.expectEqual(@as(i64, 3000), last.ended_at_ms);
    try testing.expectEqual(@as(i32, 1), h.lastFor("t2").?.exit_code);

    // Push far past capacity — the ring stays bounded and keeps the newest.
    var i: i64 = 0;
    while (i < RunHistory.capacity + 10) : (i += 1) {
        h.record("bulk", @intCast(@mod(i, 7)), 4000 + i);
    }
    try testing.expectEqual(@as(usize, RunHistory.capacity), h.records.items.len);
    try testing.expectEqual(@as(i64, 4000 + RunHistory.capacity + 9), h.lastFor("bulk").?.ended_at_ms);
    // The early t1/t2 records were evicted.
    try testing.expect(h.lastFor("t1") == null);
}

test "ageLabel buckets" {
    var buf: [32]u8 = undefined;
    try testing.expectEqualStrings("just now", ageLabel(&buf, 10_000, 9_000));
    try testing.expectEqualStrings("30s ago", ageLabel(&buf, 40_000, 10_000));
    try testing.expectEqualStrings("2m ago", ageLabel(&buf, 130_000, 0));
    try testing.expectEqualStrings("3h ago", ageLabel(&buf, 3 * 3600 * 1000 + 60_000, 0));
    try testing.expectEqualStrings("2d ago", ageLabel(&buf, 2 * 86_400_000 + 3_600_000, 0));
}
