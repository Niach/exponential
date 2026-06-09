//! Desktop-DEVICE registration — the in-app replacement for the companion's
//! `curl | bash` installer. `registerDevice` is ONE human-session-authorized
//! call to `agent.register` (the logged-in human registers this machine, keyed
//! by a stable hardware id). The server creates the device's agent sub-identity,
//! mints a single long-lived `expk_` API key, and fans the device out as an
//! `agent` member of every workspace the owner belongs to. `api_key` is that
//! expk_ key — a valid bearer for every agent call (MCP, shapes, agent.*). No
//! OAuth, no refresh token, no setup token.

const std = @import("std");
const trpc = @import("../api/trpc.zig");
const storage = @import("../storage.zig");

pub const AgentIdentity = struct {
    allocator: std.mem.Allocator,
    instance_url: []const u8,
    api_key: []const u8, // the expk_ key (Bearer for every agent call)
    agent_id: []const u8,
    agent_user_id: []const u8,
    agent_name: []const u8,
    device_id: []const u8,

    pub fn deinit(self: *AgentIdentity) void {
        const a = self.allocator;
        a.free(self.instance_url);
        a.free(self.api_key);
        a.free(self.agent_id);
        a.free(self.agent_user_id);
        a.free(self.agent_name);
        a.free(self.device_id);
    }
};

pub const Outcome = union(enum) {
    success: AgentIdentity,
    /// owned by the allocator passed to registerDevice
    failure: []const u8,
};

pub const Error = trpc.Error || std.mem.Allocator.Error;

/// Register this machine as a desktop device: one `agent.register` call, authed
/// with the logged-in human's `session_token`. Account-level — the server fans
/// the device into every workspace the owner belongs to. Returns the device's
/// agent sub-identity + its single expk_ API key.
pub fn registerDevice(
    allocator: std.mem.Allocator,
    base_url: []const u8,
    session_token: ?[]const u8,
    device_id: []const u8,
    name: []const u8,
    timeout_s: c_long,
) Error!Outcome {
    if (session_token == null) return fail(allocator, "sign in first to register this device");
    var scratch = std.heap.ArenaAllocator.init(allocator);
    defer scratch.deinit();
    const input = try std.json.Stringify.valueAlloc(scratch.allocator(), .{ .deviceId = device_id, .name = name }, .{});

    var resp = try trpc.call(allocator, base_url, "agent.register", input, session_token, timeout_s);
    defer resp.deinit();

    if (resp.errorMessage()) |msg| return .{ .failure = try allocator.dupe(u8, msg) };
    const data = resp.data() orelse return fail(allocator, "register: no data in response");
    const obj = trpc.asObject(data) orelse return fail(allocator, "register: malformed response");

    const agent = trpc.asObject(obj.get("agent") orelse return fail(allocator, "missing agent")) orelse
        return fail(allocator, "missing agent");

    const api_key = trpc.objString(obj, "apiKey") orelse return fail(allocator, "missing apiKey");
    const agent_id = trpc.objString(agent, "id") orelse return fail(allocator, "missing agent.id");
    const agent_user_id = trpc.objString(agent, "userId") orelse return fail(allocator, "missing agent.userId");
    const agent_name = trpc.objString(agent, "name") orelse return fail(allocator, "missing agent.name");

    return .{ .success = .{
        .allocator = allocator,
        .instance_url = try allocator.dupe(u8, base_url),
        .api_key = try allocator.dupe(u8, api_key),
        .agent_id = try allocator.dupe(u8, agent_id),
        .agent_user_id = try allocator.dupe(u8, agent_user_id),
        .agent_name = try allocator.dupe(u8, agent_name),
        .device_id = try allocator.dupe(u8, device_id),
    } };
}

fn fail(allocator: std.mem.Allocator, comptime msg: []const u8) Error!Outcome {
    return .{ .failure = try allocator.dupe(u8, msg) };
}

/// Stable per-machine id: the systemd/D-Bus machine id, else a random UUID
/// persisted in the config dir, so re-launch and re-install are idempotent
/// (the server keys a device on (owner, deviceId)). Caller frees.
pub fn deviceId(gpa: std.mem.Allocator) ![]u8 {
    const candidates = [_][]const u8{ "/etc/machine-id", "/var/lib/dbus/machine-id" };
    for (candidates) |path| {
        if (storage.readFileAlloc(gpa, path)) |bytes| {
            defer gpa.free(bytes);
            const trimmed = std.mem.trim(u8, bytes, " \t\r\n");
            if (trimmed.len > 0) return try gpa.dupe(u8, trimmed);
        }
    }
    const dir = try storage.configDir(gpa);
    defer gpa.free(dir);
    const fpath = try std.fmt.allocPrint(gpa, "{s}/device-id", .{dir});
    defer gpa.free(fpath);
    if (storage.readFileAlloc(gpa, fpath)) |bytes| {
        defer gpa.free(bytes);
        const trimmed = std.mem.trim(u8, bytes, " \t\r\n");
        if (trimmed.len > 0) return try gpa.dupe(u8, trimmed);
    }
    // Zig 0.16 moved std.crypto.random behind the std.Io interface.
    var raw: [16]u8 = undefined;
    std.Io.Threaded.global_single_threaded.io().random(&raw);
    const hex = std.fmt.bytesToHex(raw, .lower);
    const id = try gpa.dupe(u8, &hex);
    storage.writeSecret(fpath, id) catch {};
    return id;
}

/// Agent-initiated self-revoke (`agent.uninstallSelf`, Bearer expk_ key).
/// Returns true on success.
pub fn uninstall(allocator: std.mem.Allocator, base_url: []const u8, api_key: []const u8, timeout_s: c_long) bool {
    var resp = trpc.call(allocator, base_url, "agent.uninstallSelf", null, api_key, timeout_s) catch return false;
    defer resp.deinit();
    return resp.ok();
}
