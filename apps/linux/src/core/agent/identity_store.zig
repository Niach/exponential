//! Persisted desktop-DEVICE identity (the `expk_` key + agent/device metadata
//! from `agent.register`). Account-level, so there's ONE identity per install at
//! `{configDir}/device-agent.json` (0600) — not one per workspace. Shared by the
//! CLI register command and the in-app auto-registration flow.

const std = @import("std");
const storage = @import("../storage.zig");
const AgentIdentity = @import("registration.zig").AgentIdentity;

pub fn path(gpa: std.mem.Allocator) ![]u8 {
    const dir = try storage.configDir(gpa);
    defer gpa.free(dir);
    return std.fmt.allocPrint(gpa, "{s}/device-agent.json", .{dir});
}

/// Persist the device identity (0600). Returns the file path (caller frees).
pub fn save(gpa: std.mem.Allocator, id: *const AgentIdentity) ![]u8 {
    const dir = try storage.configDir(gpa);
    defer gpa.free(dir);
    try storage.ensureDir(dir);
    const p = try path(gpa);
    errdefer gpa.free(p);

    const view = .{
        .instanceUrl = id.instance_url,
        .apiKey = id.api_key, // the expk_ key
        .agentId = id.agent_id,
        .agentUserId = id.agent_user_id,
        .agentName = id.agent_name,
        .deviceId = id.device_id,
    };
    const json = try std.json.Stringify.valueAlloc(gpa, view, .{});
    defer gpa.free(json);
    try storage.writeSecret(p, json);
    return p;
}

pub fn exists(gpa: std.mem.Allocator) bool {
    const p = path(gpa) catch return false;
    defer gpa.free(p);
    return storage.fileExists(p);
}

pub fn delete(gpa: std.mem.Allocator) void {
    const p = path(gpa) catch return;
    defer gpa.free(p);
    storage.deleteFile(p);
}

/// One stored string field, read from the on-disk identity (caller frees), or
/// null. `field` is a top-level key like "agentName" or "apiKey".
pub fn readField(gpa: std.mem.Allocator, field: []const u8) ?[]u8 {
    const p = path(gpa) catch return null;
    defer gpa.free(p);
    const bytes = storage.readFileAlloc(gpa, p) orelse return null;
    defer gpa.free(bytes);
    var parsed = std.json.parseFromSlice(std.json.Value, gpa, bytes, .{}) catch return null;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return null,
    };
    const v = obj.get(field) orelse return null;
    return switch (v) {
        .string => |s| gpa.dupe(u8, s) catch null,
        else => null,
    };
}
