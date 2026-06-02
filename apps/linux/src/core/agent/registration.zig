//! Desktop-agent registration — the in-app replacement for the companion's
//! `curl | bash` installer. Calls `companion.claimSetup(setupToken)` and yields
//! the persistent `expk_` API key + agent/workspace metadata. The owner-only
//! `companion.create` (which mints the setup token) is driven from the UI with
//! the human session; here we consume the token, exactly like the daemon's
//! `commands/setup.ts` → `exponential-api.ts:claimSetup`.

const std = @import("std");
const trpc = @import("../api/trpc.zig");

pub const AgentIdentity = struct {
    allocator: std.mem.Allocator,
    instance_url: []const u8,
    api_key: []const u8, // expk_…
    agent_id: []const u8,
    agent_user_id: []const u8,
    agent_name: []const u8,
    workspace_id: []const u8,
    workspace_slug: []const u8,
    workspace_name: []const u8,
    github_client_id: ?[]const u8 = null,

    pub fn deinit(self: *AgentIdentity) void {
        const a = self.allocator;
        a.free(self.instance_url);
        a.free(self.api_key);
        a.free(self.agent_id);
        a.free(self.agent_user_id);
        a.free(self.agent_name);
        a.free(self.workspace_id);
        a.free(self.workspace_slug);
        a.free(self.workspace_name);
        if (self.github_client_id) |g| a.free(g);
    }
};

pub const Outcome = union(enum) {
    success: AgentIdentity,
    /// owned by the allocator passed to claimSetup
    failure: []const u8,
};

pub const Error = trpc.Error || std.mem.Allocator.Error;

pub fn claimSetup(
    allocator: std.mem.Allocator,
    base_url: []const u8,
    setup_token: []const u8,
    timeout_s: c_long,
) Error!Outcome {
    var scratch = std.heap.ArenaAllocator.init(allocator);
    defer scratch.deinit();
    const input = try std.json.Stringify.valueAlloc(scratch.allocator(), .{ .setupToken = setup_token }, .{});

    var resp = try trpc.call(allocator, base_url, "companion.claimSetup", input, null, timeout_s);
    defer resp.deinit();

    if (resp.errorMessage()) |msg| return .{ .failure = try allocator.dupe(u8, msg) };
    const data = resp.data() orelse return fail(allocator, "claimSetup: no data in response");
    const obj = trpc.asObject(data) orelse return fail(allocator, "claimSetup: malformed response");

    // Read all required fields (borrowed) before duping, so a missing field
    // fails cleanly without partial allocations.
    const agent = trpc.asObject(obj.get("agent") orelse return fail(allocator, "missing agent")) orelse
        return fail(allocator, "missing agent");
    const workspace = trpc.asObject(obj.get("workspace") orelse return fail(allocator, "missing workspace")) orelse
        return fail(allocator, "missing workspace");

    // claimSetup returns apiKey as a bare string ("expk_…") — confirmed against
    // setup.ts (`apiKey: apiKey.key`) and the daemon's exponential-api.ts.
    const api_key = trpc.objString(obj, "apiKey") orelse return fail(allocator, "missing apiKey");
    const agent_id = trpc.objString(agent, "id") orelse return fail(allocator, "missing agent.id");
    const agent_user_id = trpc.objString(agent, "userId") orelse return fail(allocator, "missing agent.userId");
    const agent_name = trpc.objString(agent, "name") orelse return fail(allocator, "missing agent.name");
    const ws_id = trpc.objString(workspace, "id") orelse return fail(allocator, "missing workspace.id");
    const ws_slug = trpc.objString(workspace, "slug") orelse return fail(allocator, "missing workspace.slug");
    const ws_name = trpc.objString(workspace, "name") orelse return fail(allocator, "missing workspace.name");

    var github_client_id: ?[]const u8 = null;
    if (trpc.asObject(obj.get("oauth") orelse std.json.Value{ .null = {} })) |oauth| {
        if (trpc.objString(oauth, "githubClientId")) |g| github_client_id = try allocator.dupe(u8, g);
    }

    return .{ .success = .{
        .allocator = allocator,
        .instance_url = try allocator.dupe(u8, base_url),
        .api_key = try allocator.dupe(u8, api_key),
        .agent_id = try allocator.dupe(u8, agent_id),
        .agent_user_id = try allocator.dupe(u8, agent_user_id),
        .agent_name = try allocator.dupe(u8, agent_name),
        .workspace_id = try allocator.dupe(u8, ws_id),
        .workspace_slug = try allocator.dupe(u8, ws_slug),
        .workspace_name = try allocator.dupe(u8, ws_name),
        .github_client_id = github_client_id,
    } };
}

fn fail(allocator: std.mem.Allocator, comptime msg: []const u8) Error!Outcome {
    return .{ .failure = try allocator.dupe(u8, msg) };
}

/// Owner-only: mint a setup token for a new desktop agent in `workspace_id`
/// (`companion.create`, authed with the human session `token`). Returns the
/// `expc_…` setup token (caller frees) or an error message.
pub const TokenOutcome = union(enum) { token: []const u8, failure: []const u8 };

pub fn createSetup(
    allocator: std.mem.Allocator,
    base_url: []const u8,
    session_token: ?[]const u8,
    workspace_id: []const u8,
    name: []const u8,
    timeout_s: c_long,
) Error!TokenOutcome {
    var scratch = std.heap.ArenaAllocator.init(allocator);
    defer scratch.deinit();
    const input = try std.json.Stringify.valueAlloc(scratch.allocator(), .{ .workspaceId = workspace_id, .name = name }, .{});

    var resp = try trpc.call(allocator, base_url, "companion.create", input, session_token, timeout_s);
    defer resp.deinit();
    if (resp.errorMessage()) |msg| return .{ .failure = try allocator.dupe(u8, msg) };
    const obj = trpc.asObject(resp.data() orelse return .{ .failure = try allocator.dupe(u8, "create: no data") }) orelse
        return .{ .failure = try allocator.dupe(u8, "create: malformed response") };
    const tok = trpc.objString(obj, "setupToken") orelse return .{ .failure = try allocator.dupe(u8, "create: missing setupToken") };
    return .{ .token = try allocator.dupe(u8, tok) };
}

/// Agent-initiated self-revoke (`companion.uninstallSelf`, Bearer `expk_`).
/// Returns true on success.
pub fn uninstall(allocator: std.mem.Allocator, base_url: []const u8, api_key: []const u8, timeout_s: c_long) bool {
    var resp = trpc.call(allocator, base_url, "companion.uninstallSelf", null, api_key, timeout_s) catch return false;
    defer resp.deinit();
    return resp.ok();
}
