//! Desktop-agent registration — the in-app replacement for the companion's
//! `curl | bash` installer. `registerMachine` is ONE human-session-authorized
//! call to `companion.register` (the logged-in human registers this machine);
//! the server creates the agent sub-identity and mints a refreshable OAuth
//! credential. `api_key` holds the OAuth ACCESS TOKEN — a valid bearer for
//! every agent call (MCP, shapes, companion.*), resolved server-side via the
//! widened getSession chokepoint. No setup token, no public claim step.

const std = @import("std");
const trpc = @import("../api/trpc.zig");

pub const AgentIdentity = struct {
    allocator: std.mem.Allocator,
    instance_url: []const u8,
    api_key: []const u8, // the OAuth access token (Bearer for every agent call)
    refresh_token: []const u8,
    token_endpoint: []const u8, // POST grant_type=refresh_token here to rotate
    oauth_client_id: []const u8,
    agent_id: []const u8,
    agent_user_id: []const u8,
    agent_name: []const u8,
    workspace_id: []const u8,
    workspace_slug: []const u8,
    workspace_name: []const u8,

    pub fn deinit(self: *AgentIdentity) void {
        const a = self.allocator;
        a.free(self.instance_url);
        a.free(self.api_key);
        a.free(self.refresh_token);
        a.free(self.token_endpoint);
        a.free(self.oauth_client_id);
        a.free(self.agent_id);
        a.free(self.agent_user_id);
        a.free(self.agent_name);
        a.free(self.workspace_id);
        a.free(self.workspace_slug);
        a.free(self.workspace_name);
    }
};

pub const Outcome = union(enum) {
    success: AgentIdentity,
    /// owned by the allocator passed to registerMachine
    failure: []const u8,
};

pub const Error = trpc.Error || std.mem.Allocator.Error;

/// Register this machine as a desktop agent: one `companion.register` call,
/// authed with the logged-in human's `session_token`. Returns the agent
/// sub-identity + a refreshable OAuth credential.
pub fn registerMachine(
    allocator: std.mem.Allocator,
    base_url: []const u8,
    session_token: ?[]const u8,
    workspace_id: []const u8,
    name: []const u8,
    timeout_s: c_long,
) Error!Outcome {
    if (session_token == null) return fail(allocator, "sign in first to register this machine");
    var scratch = std.heap.ArenaAllocator.init(allocator);
    defer scratch.deinit();
    const input = try std.json.Stringify.valueAlloc(scratch.allocator(), .{ .workspaceId = workspace_id, .name = name }, .{});

    var resp = try trpc.call(allocator, base_url, "companion.register", input, session_token, timeout_s);
    defer resp.deinit();

    if (resp.errorMessage()) |msg| return .{ .failure = try allocator.dupe(u8, msg) };
    const data = resp.data() orelse return fail(allocator, "register: no data in response");
    const obj = trpc.asObject(data) orelse return fail(allocator, "register: malformed response");

    const agent = trpc.asObject(obj.get("agent") orelse return fail(allocator, "missing agent")) orelse
        return fail(allocator, "missing agent");
    const workspace = trpc.asObject(obj.get("workspace") orelse return fail(allocator, "missing workspace")) orelse
        return fail(allocator, "missing workspace");
    const credential = trpc.asObject(obj.get("credential") orelse return fail(allocator, "missing credential")) orelse
        return fail(allocator, "missing credential");

    const access_token = trpc.objString(credential, "accessToken") orelse return fail(allocator, "missing accessToken");
    const refresh_token = trpc.objString(credential, "refreshToken") orelse return fail(allocator, "missing refreshToken");
    const token_endpoint = trpc.objString(credential, "tokenEndpoint") orelse return fail(allocator, "missing tokenEndpoint");
    const client_id = trpc.objString(credential, "clientId") orelse return fail(allocator, "missing clientId");
    const agent_id = trpc.objString(agent, "id") orelse return fail(allocator, "missing agent.id");
    const agent_user_id = trpc.objString(agent, "userId") orelse return fail(allocator, "missing agent.userId");
    const agent_name = trpc.objString(agent, "name") orelse return fail(allocator, "missing agent.name");
    const ws_id = trpc.objString(workspace, "id") orelse return fail(allocator, "missing workspace.id");
    const ws_slug = trpc.objString(workspace, "slug") orelse return fail(allocator, "missing workspace.slug");
    const ws_name = trpc.objString(workspace, "name") orelse return fail(allocator, "missing workspace.name");

    return .{ .success = .{
        .allocator = allocator,
        .instance_url = try allocator.dupe(u8, base_url),
        .api_key = try allocator.dupe(u8, access_token),
        .refresh_token = try allocator.dupe(u8, refresh_token),
        .token_endpoint = try allocator.dupe(u8, token_endpoint),
        .oauth_client_id = try allocator.dupe(u8, client_id),
        .agent_id = try allocator.dupe(u8, agent_id),
        .agent_user_id = try allocator.dupe(u8, agent_user_id),
        .agent_name = try allocator.dupe(u8, agent_name),
        .workspace_id = try allocator.dupe(u8, ws_id),
        .workspace_slug = try allocator.dupe(u8, ws_slug),
        .workspace_name = try allocator.dupe(u8, ws_name),
    } };
}

fn fail(allocator: std.mem.Allocator, comptime msg: []const u8) Error!Outcome {
    return .{ .failure = try allocator.dupe(u8, msg) };
}

/// Agent-initiated self-revoke (`companion.uninstallSelf`, Bearer access token).
/// Returns true on success.
pub fn uninstall(allocator: std.mem.Allocator, base_url: []const u8, api_key: []const u8, timeout_s: c_long) bool {
    var resp = trpc.call(allocator, base_url, "companion.uninstallSelf", null, api_key, timeout_s) catch return false;
    defer resp.deinit();
    return resp.ok();
}
