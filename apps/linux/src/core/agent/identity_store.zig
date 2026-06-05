//! Persisted desktop-agent identity (the `expk_` key + agent/workspace metadata
//! from `companion.register`), keyed by workspace id at
//! `{configDir}/agent-{workspaceId}.json` (0600). Shared by the CLI register
//! command and the in-app registration flow.

const std = @import("std");
const storage = @import("../storage.zig");
const AgentIdentity = @import("registration.zig").AgentIdentity;

pub fn pathFor(gpa: std.mem.Allocator, workspace_id: []const u8) ![]u8 {
    const dir = try storage.configDir(gpa);
    defer gpa.free(dir);
    return std.fmt.allocPrint(gpa, "{s}/agent-{s}.json", .{ dir, workspace_id });
}

/// Persist an identity (0600). Returns the file path (caller frees).
pub fn save(gpa: std.mem.Allocator, id: *const AgentIdentity) ![]u8 {
    const dir = try storage.configDir(gpa);
    defer gpa.free(dir);
    try storage.ensureDir(dir);
    const path = try pathFor(gpa, id.workspace_id);
    errdefer gpa.free(path);

    const view = .{
        .instanceUrl = id.instance_url,
        .apiKey = id.api_key, // the OAuth access token
        .refreshToken = id.refresh_token,
        .tokenEndpoint = id.token_endpoint,
        .oauthClientId = id.oauth_client_id,
        .agentId = id.agent_id,
        .agentUserId = id.agent_user_id,
        .agentName = id.agent_name,
        .workspaceId = id.workspace_id,
        .workspaceSlug = id.workspace_slug,
        .workspaceName = id.workspace_name,
    };
    const json = try std.json.Stringify.valueAlloc(gpa, view, .{});
    defer gpa.free(json);
    try storage.writeSecret(path, json);
    return path;
}

pub fn existsFor(gpa: std.mem.Allocator, workspace_id: []const u8) bool {
    const path = pathFor(gpa, workspace_id) catch return false;
    defer gpa.free(path);
    return storage.fileExists(path);
}

pub fn delete(gpa: std.mem.Allocator, workspace_id: []const u8) void {
    const path = pathFor(gpa, workspace_id) catch return;
    defer gpa.free(path);
    storage.deleteFile(path);
}

/// One stored string field, read from the on-disk identity (caller frees), or
/// null. `field` is a top-level key like "agentName" or "apiKey".
pub fn readField(gpa: std.mem.Allocator, workspace_id: []const u8, field: []const u8) ?[]u8 {
    const path = pathFor(gpa, workspace_id) catch return null;
    defer gpa.free(path);
    const bytes = storage.readFileAlloc(gpa, path) orelse return null;
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
