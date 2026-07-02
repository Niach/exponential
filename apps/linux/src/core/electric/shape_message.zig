//! Electric shape-protocol message parsing.
//!
//! Faithful port of the decode logic in the iOS `ShapeClient.swift`
//! (`mapRawDict` / `parseIdFromKey`) and the Android `ShapeMessage.kt`, against
//! the cross-client contract in `packages/electric-protocol/` (README + fixtures).
//!
//! Unlike the iOS client — which decodes each row into a typed `Codable` entity
//! and therefore must distinguish `update` from `partialUpdate` — the Linux
//! client applies rows generically to SQLite (UPSERT of whatever columns are
//! present). So `insert` and `update` collapse into "upsert the given columns";
//! the semantically distinct cases are `delete`, `up_to_date`, and
//! `must_refetch`. Column-name casing (snake vs camel) is normalised at bind
//! time in the db layer, not here — we keep the raw JSON value.

const std = @import("std");

pub const MessageKind = enum { insert, update, delete, up_to_date, must_refetch };

/// One parsed shape message. `key`/`id`/`value` borrow from the JSON arena that
/// produced them (see `Batch`), so they are valid only while that arena lives.
pub const Message = struct {
    kind: MessageKind,
    /// Raw Electric key, e.g. `"issues"/"01J..."`. Absent for control messages.
    key: ?[]const u8 = null,
    /// Bare primary key parsed out of `key` (quotes + table segment stripped).
    id: ?[]const u8 = null,
    /// Row object for insert/update (and sometimes delete). Always a JSON object.
    value: ?std.json.Value = null,
};

/// A parsed response body: owns the JSON arena the messages borrow from.
pub const Batch = struct {
    parsed: std.json.Parsed(std.json.Value),
    messages: []Message,
    allocator: std.mem.Allocator,

    pub fn deinit(self: *Batch) void {
        self.allocator.free(self.messages);
        self.parsed.deinit();
    }
};

fn objGetString(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const v = obj.get(key) orelse return null;
    return switch (v) {
        .string => |s| s,
        else => null,
    };
}

/// Electric keys arrive as `"table"/"id"` (quoted). Strip the table segment and
/// the surrounding quotes to recover the bare primary key.
pub fn parseIdFromKey(key: []const u8) ?[]const u8 {
    var it = std.mem.splitScalar(u8, key, '/');
    var last: ?[]const u8 = null;
    while (it.next()) |seg| last = seg;
    const seg = last orelse return null;
    return std.mem.trim(u8, seg, "\"");
}

/// Decode a single array element into a `Message`. Returns null for elements
/// that aren't valid messages (e.g. unknown control values).
pub fn messageFromItem(item: std.json.Value) ?Message {
    const obj = switch (item) {
        .object => |o| o,
        else => return null,
    };
    const headers = switch (obj.get("headers") orelse return null) {
        .object => |o| o,
        else => return null,
    };

    if (objGetString(headers, "control")) |control| {
        if (std.mem.eql(u8, control, "up-to-date")) return .{ .kind = .up_to_date };
        if (std.mem.eql(u8, control, "must-refetch")) return .{ .kind = .must_refetch };
        // Chunk boundary of a multi-response snapshot — recognized but carries
        // no data; liveness is gated on up-to-date, never on snapshot-end
        // (mirrors the iOS ShapeClient).
        if (std.mem.eql(u8, control, "snapshot-end")) return null;
        return null; // unknown control message — ignore
    }

    const op = objGetString(headers, "operation") orelse return null;
    const key = objGetString(obj, "key");
    const value: ?std.json.Value = blk: {
        const v = obj.get("value") orelse break :blk null;
        break :blk switch (v) {
            .object => v,
            else => null,
        };
    };
    const id = if (key) |k| parseIdFromKey(k) else null;

    if (std.mem.eql(u8, op, "insert")) return .{ .kind = .insert, .key = key, .id = id, .value = value };
    if (std.mem.eql(u8, op, "update")) return .{ .kind = .update, .key = key, .id = id, .value = value };
    if (std.mem.eql(u8, op, "delete")) return .{ .kind = .delete, .key = key, .id = id, .value = value };
    return null; // unknown operation
}

/// Build the message slice from a JSON array value. Caller owns the returned
/// slice (free with the same allocator); elements borrow from the array's arena.
pub fn messagesFromArray(allocator: std.mem.Allocator, arr: std.json.Value) ![]Message {
    const items = switch (arr) {
        .array => |a| a.items,
        else => return error.NotAnArray,
    };
    var out = try allocator.alloc(Message, items.len);
    errdefer allocator.free(out);
    var n: usize = 0;
    for (items) |item| {
        if (messageFromItem(item)) |m| {
            out[n] = m;
            n += 1;
        }
    }
    if (n != items.len) out = try allocator.realloc(out, n);
    return out;
}

/// Parse a raw response body (the JSON array the shape proxy returns) into a
/// `Batch`. An empty body is treated as an empty array.
pub fn parse(allocator: std.mem.Allocator, body: []const u8) !Batch {
    const slice = if (body.len == 0) "[]" else body;
    const parsed = try std.json.parseFromSlice(std.json.Value, allocator, slice, .{});
    errdefer parsed.deinit();
    const messages = try messagesFromArray(allocator, parsed.value);
    return .{ .parsed = parsed, .messages = messages, .allocator = allocator };
}

// ---------------------------------------------------------------------------
// Tests — run against the shared fixtures in packages/electric-protocol so the
// Linux client provably speaks the same wire format as web/iOS/Android.
// ---------------------------------------------------------------------------

const build_options = @import("build_options");

fn loadFixture(a: std.mem.Allocator, name: []const u8) ![]u8 {
    const path = try std.fs.path.join(a, &.{ build_options.fixtures_dir, name });
    defer a.free(path);
    // Zig 0.16 I/O model: file reads go through an explicit `Io`. The global
    // single-threaded instance is fine for a synchronous test read.
    const io = std.Io.Threaded.global_single_threaded.io();
    return std.Io.Dir.cwd().readFileAlloc(io, path, a, .unlimited);
}

/// The wrapped fixtures (initial-snapshot etc.) are `{request, response:{body}}`;
/// the real wire body is `response.body`.
fn responseBody(root: std.json.Value) ?std.json.Value {
    const obj = switch (root) {
        .object => |o| o,
        else => return null,
    };
    const resp = switch (obj.get("response") orelse return null) {
        .object => |o| o,
        else => return null,
    };
    return resp.get("body");
}

test "parseIdFromKey strips table segment and quotes" {
    try std.testing.expectEqualStrings(
        "01J9K0A0X3CB4E5F6G7H8J9K0L",
        parseIdFromKey("\"issues\"/\"01J9K0A0X3CB4E5F6G7H8J9K0L\"").?,
    );
}

test "initial-snapshot: two inserts then up-to-date" {
    const a = std.testing.allocator;
    const text = try loadFixture(a, "initial-snapshot.json");
    defer a.free(text);
    const parsed = try std.json.parseFromSlice(std.json.Value, a, text, .{});
    defer parsed.deinit();
    const body = responseBody(parsed.value) orelse return error.NoBody;
    const msgs = try messagesFromArray(a, body);
    defer a.free(msgs);

    try std.testing.expectEqual(@as(usize, 3), msgs.len);
    try std.testing.expectEqual(MessageKind.insert, msgs[0].kind);
    try std.testing.expectEqual(MessageKind.insert, msgs[1].kind);
    try std.testing.expectEqual(MessageKind.up_to_date, msgs[2].kind);
    try std.testing.expectEqualStrings("01J9K0A0X3CB4E5F6G7H8J9K0L", msgs[0].id.?);
    try std.testing.expectEqualStrings("01J9K0A0X3CB4E5F6G7H8J9K0N", msgs[1].id.?);
}

test "live-update: update + delete + up-to-date" {
    const a = std.testing.allocator;
    const text = try loadFixture(a, "live-update.json");
    defer a.free(text);
    const parsed = try std.json.parseFromSlice(std.json.Value, a, text, .{});
    defer parsed.deinit();
    const body = responseBody(parsed.value) orelse return error.NoBody;
    const msgs = try messagesFromArray(a, body);
    defer a.free(msgs);

    try std.testing.expectEqual(@as(usize, 3), msgs.len);
    try std.testing.expectEqual(MessageKind.update, msgs[0].kind);
    try std.testing.expectEqual(MessageKind.delete, msgs[1].kind);
    try std.testing.expectEqual(MessageKind.up_to_date, msgs[2].kind);
    try std.testing.expectEqualStrings("01J9K0A0X3CB4E5F6G7H8J9K0N", msgs[1].id.?);
}

test "up-to-date: lone control message" {
    const a = std.testing.allocator;
    const text = try loadFixture(a, "up-to-date.json");
    defer a.free(text);
    const parsed = try std.json.parseFromSlice(std.json.Value, a, text, .{});
    defer parsed.deinit();
    const body = responseBody(parsed.value) orelse return error.NoBody;
    const msgs = try messagesFromArray(a, body);
    defer a.free(msgs);

    try std.testing.expectEqual(@as(usize, 1), msgs.len);
    try std.testing.expectEqual(MessageKind.up_to_date, msgs[0].kind);
}

test "must-refetch: lone control message" {
    const a = std.testing.allocator;
    const text = try loadFixture(a, "must-refetch.json");
    defer a.free(text);
    const parsed = try std.json.parseFromSlice(std.json.Value, a, text, .{});
    defer parsed.deinit();
    const body = responseBody(parsed.value) orelse return error.NoBody;
    const msgs = try messagesFromArray(a, body);
    defer a.free(msgs);

    try std.testing.expectEqual(@as(usize, 1), msgs.len);
    try std.testing.expectEqual(MessageKind.must_refetch, msgs[0].kind);
}

test "snake_case row: insert with id and value object" {
    const a = std.testing.allocator;
    const text = try loadFixture(a, "snake-case.json");
    defer a.free(text);
    const parsed = try std.json.parseFromSlice(std.json.Value, a, text, .{});
    defer parsed.deinit();
    const m = messageFromItem(parsed.value) orelse return error.NotAMessage;

    try std.testing.expectEqual(MessageKind.insert, m.kind);
    try std.testing.expectEqualStrings("01J9K0A0X3CB4E5F6G7H8J9K0L", m.id.?);
    try std.testing.expect(m.value != null);
    const value_obj = m.value.?.object;
    try std.testing.expectEqualStrings("EXP-1", objGetString(value_obj, "identifier").?);
    try std.testing.expect(value_obj.get("project_id") != null);
}

test "camel_case row: same row accepted in camelCase form" {
    const a = std.testing.allocator;
    const text = try loadFixture(a, "camel-case.json");
    defer a.free(text);
    const parsed = try std.json.parseFromSlice(std.json.Value, a, text, .{});
    defer parsed.deinit();
    const m = messageFromItem(parsed.value) orelse return error.NotAMessage;

    try std.testing.expectEqual(MessageKind.insert, m.kind);
    try std.testing.expectEqualStrings("01J9K0A0X3CB4E5F6G7H8J9K0L", m.id.?);
    const value_obj = m.value.?.object;
    // camelCase variant carries projectId rather than project_id.
    try std.testing.expect(value_obj.get("projectId") != null);
}

test "snapshot-end: recognized chunk boundary, dropped without dropping siblings" {
    const a = std.testing.allocator;
    const body =
        \\[
        \\ {"headers":{"operation":"insert"},"key":"\"issues\"/\"A\"","value":{"id":"A"}},
        \\ {"headers":{"control":"snapshot-end"}}
        \\]
    ;
    var batch = try parse(a, body);
    defer batch.deinit();
    // The insert survives; snapshot-end contributes no message (and crucially is
    // NOT up-to-date — the client stays non-live until Electric says head).
    try std.testing.expectEqual(@as(usize, 1), batch.messages.len);
    try std.testing.expectEqual(MessageKind.insert, batch.messages[0].kind);
}

test "parse handles empty body as zero messages" {
    const a = std.testing.allocator;
    var batch = try parse(a, "");
    defer batch.deinit();
    try std.testing.expectEqual(@as(usize, 0), batch.messages.len);
}
