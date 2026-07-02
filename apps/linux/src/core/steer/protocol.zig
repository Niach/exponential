//! Steer relay wire protocol — the Zig mirror of apps/steer-relay/src/protocol.ts
//! (masterplan §3.2; the relay hub tests are the executable spec).
//!
//! Two frame kinds on every socket:
//!   - TEXT frames: JSON control messages `{ t, … }` (built/parsed here with the
//!     EXACT field names from protocol.ts — the relay zod-validates them, and
//!     `.optional()` fields must be OMITTED when absent, never `null`).
//!   - BINARY frames: terminal output — one opcode byte `0x01` followed by
//!     verbatim PTY bytes (publisher → relay → viewers). Never JSON/base64.
//!
//! Also home to the publisher's bounded replay ring buffer (`resync` payload).

const std = @import("std");

/// Opcode byte prefixing a binary terminal-output frame (OUTPUT_OPCODE).
pub const output_opcode: u8 = 0x01;

// Close codes (informational; the client just reconnects or stops).
pub const close_session_ended: u16 = 4001;
pub const close_replaced: u16 = 4002;
pub const close_unauthorized: u16 = 4003;
pub const close_slow_consumer: u16 = 4008;

// ---------------------------------------------------------------------------
// Outbound (desktop → relay) builders
// ---------------------------------------------------------------------------

/// Control socket presence announce. `device_label` omitted when null.
pub fn onlineFrame(gpa: std.mem.Allocator, device_id: []const u8, device_label: ?[]const u8) ![]u8 {
    var out: std.ArrayListUnmanaged(u8) = .empty;
    errdefer out.deinit(gpa);
    try out.appendSlice(gpa, "{\"t\":\"online\",\"deviceId\":");
    try appendJsonString(gpa, &out, device_id);
    if (device_label) |label| {
        try out.appendSlice(gpa, ",\"deviceLabel\":");
        try appendJsonString(gpa, &out, label);
    }
    try out.append(gpa, '}');
    return out.toOwnedSlice(gpa);
}

/// Publisher socket registration for a session's room.
pub fn helloFrame(gpa: std.mem.Allocator, session_id: []const u8, issue_id: ?[]const u8, cols: u16, rows: u16) ![]u8 {
    var out: std.ArrayListUnmanaged(u8) = .empty;
    errdefer out.deinit(gpa);
    try out.appendSlice(gpa, "{\"t\":\"hello\",\"sessionId\":");
    try appendJsonString(gpa, &out, session_id);
    if (issue_id) |iid| {
        try out.appendSlice(gpa, ",\"issueId\":");
        try appendJsonString(gpa, &out, iid);
    }
    var buf: [64]u8 = undefined;
    const tail = try std.fmt.bufPrint(&buf, ",\"cols\":{d},\"rows\":{d}}}", .{ cols, rows });
    try out.appendSlice(gpa, tail);
    return out.toOwnedSlice(gpa);
}

/// Publisher: local terminal geometry changed (viewers reflow).
pub fn resizeFrame(gpa: std.mem.Allocator, cols: u16, rows: u16) ![]u8 {
    return std.fmt.allocPrint(gpa, "{{\"t\":\"resize\",\"cols\":{d},\"rows\":{d}}}", .{ cols, rows });
}

/// Session ended / socket going away. `outcome` omitted when null.
pub fn byeFrame(gpa: std.mem.Allocator, outcome: ?[]const u8) ![]u8 {
    var out: std.ArrayListUnmanaged(u8) = .empty;
    errdefer out.deinit(gpa);
    try out.appendSlice(gpa, "{\"t\":\"bye\"");
    if (outcome) |o| {
        try out.appendSlice(gpa, ",\"outcome\":");
        try appendJsonString(gpa, &out, o);
    }
    try out.append(gpa, '}');
    return out.toOwnedSlice(gpa);
}

/// §3.4 local take-over, sent on the PUBLISHER socket: release the remote
/// steerer's claim, then claim for the local user (their machine wins).
/// Constant payloads — match protocol.ts `releaseFrame` / `claimFrame`.
pub const release_frame: []const u8 = "{\"t\":\"release\"}";
pub const claim_frame: []const u8 = "{\"t\":\"claim\"}";

/// A binary terminal-output frame: opcode `0x01` + verbatim PTY bytes.
pub fn outputFrame(gpa: std.mem.Allocator, bytes: []const u8) ![]u8 {
    const frame = try gpa.alloc(u8, bytes.len + 1);
    frame[0] = output_opcode;
    @memcpy(frame[1..], bytes);
    return frame;
}

/// A standalone quoted + escaped JSON string literal (for hand-built tRPC
/// inputs like the mintTicket deviceLabel). Caller frees.
pub fn jsonStringAlloc(gpa: std.mem.Allocator, s: []const u8) ![]u8 {
    var out: std.ArrayListUnmanaged(u8) = .empty;
    errdefer out.deinit(gpa);
    try appendJsonString(gpa, &out, s);
    return out.toOwnedSlice(gpa);
}

/// Append a JSON string literal (quoted + escaped) to `out`.
fn appendJsonString(gpa: std.mem.Allocator, out: *std.ArrayListUnmanaged(u8), s: []const u8) !void {
    try out.append(gpa, '"');
    for (s) |c| {
        switch (c) {
            '"' => try out.appendSlice(gpa, "\\\""),
            '\\' => try out.appendSlice(gpa, "\\\\"),
            '\n' => try out.appendSlice(gpa, "\\n"),
            '\r' => try out.appendSlice(gpa, "\\r"),
            '\t' => try out.appendSlice(gpa, "\\t"),
            else => {
                if (c < 0x20) {
                    var buf: [8]u8 = undefined;
                    const esc = std.fmt.bufPrint(&buf, "\\u{x:0>4}", .{c}) catch unreachable;
                    try out.appendSlice(gpa, esc);
                } else {
                    try out.append(gpa, c);
                }
            },
        }
    }
    try out.append(gpa, '"');
}

// ---------------------------------------------------------------------------
// Inbound (relay → desktop) parsing
// ---------------------------------------------------------------------------

/// Server control frames the desktop reacts to. String payloads are slices into
/// the arena passed to `parseInbound` — copy before the arena dies.
pub const Inbound = union(enum) {
    /// Control socket: remote "Start on my desktop".
    start_session: []const u8, // issueId
    /// Publisher socket: keystrokes from the steering viewer (utf8).
    input: []const u8, // data
    /// Publisher socket: a viewer joined / needs a repaint → replay the ring.
    resync,
    /// Publisher socket: kill-switch → tear the session down.
    kill,
    /// Publisher socket: the steerer resized their view → update PTY winsize.
    resize: struct { cols: u16, rows: u16 },
    /// Who's watching/steering. `steerer_id` null ⇒ nobody holds the claim;
    /// `steerer_name` is the steerer's display name resolved from the frame's
    /// viewers list (null when the steerer isn't listed) — drives the local
    /// "Remote steering — <name>" banner (§3.4).
    presence: struct { steerer_id: ?[]const u8, steerer_name: ?[]const u8 },
    bye,
    err,
    unknown,
};

/// Parse a JSON text control frame. Returns null for non-JSON / missing `t`.
/// All returned slices live in `arena`.
pub fn parseInbound(arena: std.mem.Allocator, text: []const u8) ?Inbound {
    const parsed = std.json.parseFromSliceLeaky(std.json.Value, arena, text, .{}) catch return null;
    const obj = switch (parsed) {
        .object => |o| o,
        else => return null,
    };
    const t = objString(obj, "t") orelse return null;

    if (std.mem.eql(u8, t, "start_session")) {
        return .{ .start_session = objString(obj, "issueId") orelse "" };
    }
    if (std.mem.eql(u8, t, "input")) {
        return .{ .input = objString(obj, "data") orelse "" };
    }
    if (std.mem.eql(u8, t, "resync")) return .resync;
    if (std.mem.eql(u8, t, "kill")) return .kill;
    if (std.mem.eql(u8, t, "resize")) {
        const cols = objU16(obj, "cols") orelse return .unknown;
        const rows = objU16(obj, "rows") orelse return .unknown;
        return .{ .resize = .{ .cols = cols, .rows = rows } };
    }
    if (std.mem.eql(u8, t, "presence")) {
        const sid = objString(obj, "steererId");
        var name: ?[]const u8 = null;
        if (sid) |id| {
            if (obj.get("viewers")) |vv| switch (vv) {
                .array => |arr| for (arr.items) |item| {
                    const vo = switch (item) {
                        .object => |o| o,
                        else => continue,
                    };
                    const uid = objString(vo, "userId") orelse continue;
                    if (std.mem.eql(u8, uid, id)) {
                        name = objString(vo, "name");
                        break;
                    }
                },
                else => {},
            };
        }
        return .{ .presence = .{ .steerer_id = sid, .steerer_name = name } };
    }
    if (std.mem.eql(u8, t, "bye")) return .bye;
    if (std.mem.eql(u8, t, "error")) return .err;
    return .unknown;
}

fn objString(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const v = obj.get(key) orelse return null;
    return switch (v) {
        .string => |s| s,
        else => null,
    };
}

fn objU16(obj: std.json.ObjectMap, key: []const u8) ?u16 {
    const v = obj.get(key) orelse return null;
    return switch (v) {
        .integer => |i| if (i > 0 and i <= 1000) @intCast(i) else null,
        else => null,
    };
}

// ---------------------------------------------------------------------------
// Replay ring buffer (publisher `resync` payload)
// ---------------------------------------------------------------------------

/// A bounded byte ring: keeps the most recent `capacity` bytes of PTY output.
/// Claude's full-screen redraws repaint the screen for a mid-session viewer, so
/// "recent bytes" is the pragmatic resync payload (mirrors MacSteerPublisher).
/// NOT thread-safe — callers guard with their own mutex.
pub const RingBuffer = struct {
    buf: []u8,
    start: usize = 0, // index of the oldest byte
    len: usize = 0,

    pub fn init(gpa: std.mem.Allocator, capacity: usize) !RingBuffer {
        return .{ .buf = try gpa.alloc(u8, capacity) };
    }

    pub fn deinit(self: *RingBuffer, gpa: std.mem.Allocator) void {
        gpa.free(self.buf);
        self.* = undefined;
    }

    pub fn append(self: *RingBuffer, bytes: []const u8) void {
        const cap = self.buf.len;
        if (cap == 0) return;
        // Oversized append: only the last `cap` bytes survive anyway.
        const src = if (bytes.len > cap) bytes[bytes.len - cap ..] else bytes;
        var write_at = (self.start + self.len) % cap;
        for (src) |b| {
            self.buf[write_at] = b;
            write_at = (write_at + 1) % cap;
            if (self.len < cap) {
                self.len += 1;
            } else {
                self.start = (self.start + 1) % cap; // evict the oldest
            }
        }
    }

    /// Linearized copy of the buffered bytes (oldest → newest). Caller frees.
    pub fn snapshot(self: *const RingBuffer, gpa: std.mem.Allocator) ![]u8 {
        const out = try gpa.alloc(u8, self.len);
        for (out, 0..) |*b, i| b.* = self.buf[(self.start + i) % self.buf.len];
        return out;
    }
};

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

const testing = std.testing;

test "onlineFrame matches protocol.ts field names" {
    const gpa = testing.allocator;
    const with_label = try onlineFrame(gpa, "dev-1", "My Desktop");
    defer gpa.free(with_label);
    try testing.expectEqualStrings(
        "{\"t\":\"online\",\"deviceId\":\"dev-1\",\"deviceLabel\":\"My Desktop\"}",
        with_label,
    );
    // zod `.optional()` ⇒ the key must be omitted (a JSON null fails validation).
    const without = try onlineFrame(gpa, "dev-1", null);
    defer gpa.free(without);
    try testing.expectEqualStrings("{\"t\":\"online\",\"deviceId\":\"dev-1\"}", without);
}

test "helloFrame carries sessionId/issueId/cols/rows" {
    const gpa = testing.allocator;
    const frame = try helloFrame(gpa, "sess-1", "issue-9", 120, 32);
    defer gpa.free(frame);
    try testing.expectEqualStrings(
        "{\"t\":\"hello\",\"sessionId\":\"sess-1\",\"issueId\":\"issue-9\",\"cols\":120,\"rows\":32}",
        frame,
    );
}

test "resize + bye frames" {
    const gpa = testing.allocator;
    const rs = try resizeFrame(gpa, 80, 24);
    defer gpa.free(rs);
    try testing.expectEqualStrings("{\"t\":\"resize\",\"cols\":80,\"rows\":24}", rs);

    const bye = try byeFrame(gpa, "ended");
    defer gpa.free(bye);
    try testing.expectEqualStrings("{\"t\":\"bye\",\"outcome\":\"ended\"}", bye);

    const bare = try byeFrame(gpa, null);
    defer gpa.free(bare);
    try testing.expectEqualStrings("{\"t\":\"bye\"}", bare);
}

test "json string escaping" {
    const gpa = testing.allocator;
    const frame = try onlineFrame(gpa, "a\"b\\c\nd", null);
    defer gpa.free(frame);
    try testing.expectEqualStrings("{\"t\":\"online\",\"deviceId\":\"a\\\"b\\\\c\\nd\"}", frame);
}

test "outputFrame prefixes the 0x01 opcode" {
    const gpa = testing.allocator;
    const frame = try outputFrame(gpa, "hi");
    defer gpa.free(frame);
    try testing.expectEqual(@as(usize, 3), frame.len);
    try testing.expectEqual(output_opcode, frame[0]);
    try testing.expectEqualStrings("hi", frame[1..]);
}

test "parseInbound round-trips every server frame kind" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const start = parseInbound(a, "{\"t\":\"start_session\",\"issueId\":\"i-1\"}").?;
    try testing.expectEqualStrings("i-1", start.start_session);

    const input = parseInbound(a, "{\"t\":\"input\",\"data\":\"ls\\n\"}").?;
    try testing.expectEqualStrings("ls\n", input.input);

    try testing.expectEqual(Inbound.resync, parseInbound(a, "{\"t\":\"resync\"}").?);
    try testing.expectEqual(Inbound.kill, parseInbound(a, "{\"t\":\"kill\"}").?);
    try testing.expectEqual(Inbound.bye, parseInbound(a, "{\"t\":\"bye\",\"outcome\":\"ended\"}").?);
    try testing.expectEqual(Inbound.err, parseInbound(a, "{\"t\":\"error\",\"code\":\"no_room\"}").?);

    const rz = parseInbound(a, "{\"t\":\"resize\",\"cols\":100,\"rows\":40}").?;
    try testing.expectEqual(@as(u16, 100), rz.resize.cols);
    try testing.expectEqual(@as(u16, 40), rz.resize.rows);

    const pres = parseInbound(a, "{\"t\":\"presence\",\"viewers\":[],\"steererId\":\"u-2\"}").?;
    try testing.expectEqualStrings("u-2", pres.presence.steerer_id.?);
    try testing.expect(pres.presence.steerer_name == null); // not in viewers
    const pres_none = parseInbound(a, "{\"t\":\"presence\",\"viewers\":[],\"steererId\":null}").?;
    try testing.expect(pres_none.presence.steerer_id == null);

    try testing.expectEqual(Inbound.unknown, parseInbound(a, "{\"t\":\"wat\"}").?);
    try testing.expect(parseInbound(a, "not json") == null);
    try testing.expect(parseInbound(a, "{\"nope\":1}") == null);
}

test "presence resolves the steerer's display name from the viewers list" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const pres = parseInbound(a,
        \\{"t":"presence","viewers":[{"userId":"u-1","name":"Ada","perm":"steer"},{"userId":"u-2","name":"Bob","perm":"view"}],"steererId":"u-1"}
    ).?;
    try testing.expectEqualStrings("u-1", pres.presence.steerer_id.?);
    try testing.expectEqualStrings("Ada", pres.presence.steerer_name.?);

    // Steerer not among the viewers (e.g. just disconnected) ⇒ id only.
    const unlisted = parseInbound(a,
        \\{"t":"presence","viewers":[{"userId":"u-2","name":"Bob","perm":"view"}],"steererId":"u-9"}
    ).?;
    try testing.expectEqualStrings("u-9", unlisted.presence.steerer_id.?);
    try testing.expect(unlisted.presence.steerer_name == null);
}

test "take-over release/claim frames match protocol.ts field names" {
    try testing.expectEqualStrings("{\"t\":\"release\"}", release_frame);
    try testing.expectEqualStrings("{\"t\":\"claim\"}", claim_frame);
}

test "ring buffer keeps the most recent bytes across wraps" {
    const gpa = testing.allocator;
    var ring = try RingBuffer.init(gpa, 8);
    defer ring.deinit(gpa);

    ring.append("abc");
    var snap = try ring.snapshot(gpa);
    try testing.expectEqualStrings("abc", snap);
    gpa.free(snap);

    ring.append("defgh"); // exactly full
    snap = try ring.snapshot(gpa);
    try testing.expectEqualStrings("abcdefgh", snap);
    gpa.free(snap);

    ring.append("XY"); // evicts "ab"
    snap = try ring.snapshot(gpa);
    try testing.expectEqualStrings("cdefghXY", snap);
    gpa.free(snap);

    ring.append("0123456789AB"); // oversized: only the last 8 survive
    snap = try ring.snapshot(gpa);
    try testing.expectEqualStrings("456789AB", snap);
    gpa.free(snap);
}
