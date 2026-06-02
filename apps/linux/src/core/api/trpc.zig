//! tRPC-over-HTTP client — matches the companion's `exponential-api.ts` exactly:
//! POST {baseUrl}/api/trpc/{path} with the raw input as the JSON body (NO
//! superjson/transformer), response envelope `{result:{data}}` or
//! `{error:{message}}`. Auth via `Authorization: Bearer <token>`.

const std = @import("std");
const http = @import("http.zig");

pub const Error = error{ParseFailed} || http.Error;

pub const Response = struct {
    parsed: std.json.Parsed(std.json.Value),
    status: i64,

    pub fn deinit(self: *Response) void {
        self.parsed.deinit();
    }

    /// `result.data` subtree, or null if absent.
    pub fn data(self: *const Response) ?std.json.Value {
        const obj = asObject(self.parsed.value) orelse return null;
        const result = asObject(obj.get("result") orelse return null) orelse return null;
        return result.get("data");
    }

    /// `error.message` if the server returned a tRPC error.
    pub fn errorMessage(self: *const Response) ?[]const u8 {
        const obj = asObject(self.parsed.value) orelse return null;
        const err = asObject(obj.get("error") orelse return null) orelse return null;
        return asString(err.get("message") orelse return null);
    }

    pub fn ok(self: *const Response) bool {
        return self.status >= 200 and self.status < 300 and self.errorMessage() == null;
    }
};

/// Call a procedure. `input_json` is the pre-serialized input object (or null for
/// no input). Caller owns the returned Response (`deinit`).
pub fn call(
    allocator: std.mem.Allocator,
    base_url: []const u8,
    path: []const u8,
    input_json: ?[]const u8,
    token: ?[]const u8,
    timeout_s: c_long,
) Error!Response {
    var scratch = std.heap.ArenaAllocator.init(allocator);
    defer scratch.deinit();
    const trimmed = std.mem.trimEnd(u8, base_url, "/");
    const url = try std.fmt.allocPrintSentinel(scratch.allocator(), "{s}/api/trpc/{s}", .{ trimmed, path }, 0);

    var resp = try http.post(allocator, url, token, input_json, timeout_s, null);
    defer resp.deinit();

    const parsed = std.json.parseFromSlice(std.json.Value, allocator, resp.body, .{}) catch
        return Error.ParseFailed;
    return .{ .parsed = parsed, .status = resp.status };
}

// --- small JSON helpers shared by the auth/agent layers ---

pub fn asObject(v: std.json.Value) ?std.json.ObjectMap {
    return switch (v) {
        .object => |o| o,
        else => null,
    };
}

pub fn asString(v: std.json.Value) ?[]const u8 {
    return switch (v) {
        .string => |s| s,
        else => null,
    };
}

pub fn objString(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    return asString(obj.get(key) orelse return null);
}

pub fn objBool(obj: std.json.ObjectMap, key: []const u8) bool {
    const v = obj.get(key) orelse return false;
    return switch (v) {
        .bool => |b| b,
        else => false,
    };
}
