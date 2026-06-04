//! Reads the desktop agent's stored GitHub token. GitHub is connected in the web
//! app now (Better Auth `linkSocial`) and the token is fetched from the server at
//! runtime (`integrations.github.token`); this only reads a legacy
//! `{configDir}/github.json` left by the old device flow as a fallback (see
//! `app.zig` reconcileAgent). The device-flow itself has been removed.

const std = @import("std");
const storage = @import("../storage.zig");

fn tokenPath(gpa: std.mem.Allocator) ![]u8 {
    const dir = try storage.configDir(gpa);
    defer gpa.free(dir);
    return std.fmt.allocPrint(gpa, "{s}/github.json", .{dir});
}

/// The stored access token (caller frees), or null when not connected.
pub fn loadToken(gpa: std.mem.Allocator) ?[]u8 {
    return readField(gpa, "token");
}

fn readField(gpa: std.mem.Allocator, field: []const u8) ?[]u8 {
    const path = tokenPath(gpa) catch return null;
    defer gpa.free(path);
    const bytes = storage.readFileAlloc(gpa, path) orelse return null;
    defer gpa.free(bytes);
    var parsed = std.json.parseFromSlice(std.json.Value, gpa, bytes, .{}) catch return null;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return null,
    };
    return (dupStr(gpa, obj, field) catch null) orelse null;
}

fn dupStr(gpa: std.mem.Allocator, obj: std.json.ObjectMap, key: []const u8) !?[]u8 {
    const v = obj.get(key) orelse return null;
    return switch (v) {
        .string => |s| try gpa.dupe(u8, s),
        else => null,
    };
}
