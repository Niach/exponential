//! Desktop coding-flow settings + the user's personal API key, persisted as JSON
//! at `{configDir}/desktop-settings.json` (0600). This is the successor to the
//! deleted agent `identity_store` — it holds the REAL signed-in user's personal
//! Better Auth apikey (`expu_…`, auto-minted by the coding launcher on first
//! use via `users.mintPersonalApiKey`, named "Device: <hostname>" — EXP-2),
//! written into each worktree's `.mcp.json` by the coding launcher, plus the
//! JetBrains-SDK-style editable defaults for the "Start coding" flow (§4b):
//! the `claude` CLI path, the repos/worktrees root, and the branch prefix.
//!
//! All strings live in the store's arena (mirrors `account_store.zig`); the
//! effective-value helpers fold in the defaults so callers never special-case a
//! null override.

const std = @import("std");
const storage = @import("storage.zig");
const steer_util = @import("steer/util.zig");

/// The default branch prefix — the launcher builds `<prefix><ISSUE-IDENTIFIER>`.
pub const default_branch_prefix = "exp/";
/// The default `claude` binary (resolved on PATH).
pub const default_claude_path = "claude";

/// The on-disk shape. Every field is optional so an older/newer file loads
/// cleanly and an unset override falls back to its default.
pub const Data = struct {
    /// The raw personal API key (`expu_…`) — written into `.mcp.json`. Shown to
    /// the user exactly once at mint time; we persist it so the launcher can
    /// reuse it without re-minting.
    personal_api_key: ?[]const u8 = null,
    /// The key's id (for `users.revokePersonalApiKey`) and its non-secret start
    /// (for display, e.g. "expu_ab12…").
    personal_api_key_id: ?[]const u8 = null,
    personal_api_key_start: ?[]const u8 = null,
    /// Absolute path to the `claude` CLI, or null to resolve `claude` on PATH.
    claude_path: ?[]const u8 = null,
    /// Where clones + worktrees live, or null for `~/Exponential/repos`.
    repos_root: ?[]const u8 = null,
    /// Branch prefix, or null for `exp/`.
    branch_prefix: ?[]const u8 = null,
    /// Stable random id announced to the steer relay (`online{deviceId}`) so
    /// the phone's "Start on my desktop" picker can target THIS machine.
    /// Generated once on first use, then persisted.
    device_id: ?[]const u8 = null,
};

pub const Store = struct {
    gpa: std.mem.Allocator,
    arena: std.heap.ArenaAllocator,
    path: []const u8,
    data: Data = .{},

    pub fn open(gpa: std.mem.Allocator) !Store {
        var store = Store{
            .gpa = gpa,
            .arena = std.heap.ArenaAllocator.init(gpa),
            .path = "",
        };
        const a = store.arena.allocator();
        const dir = try storage.configDir(a);
        storage.ensureDir(dir) catch {}; // best-effort; surfaced on save
        store.path = try std.fs.path.join(a, &.{ dir, "desktop-settings.json" });
        store.load();
        return store;
    }

    pub fn deinit(self: *Store) void {
        self.arena.deinit();
    }

    fn load(self: *Store) void {
        const bytes = storage.readFileAlloc(self.gpa, self.path) orelse return;
        defer self.gpa.free(bytes);
        const parsed = std.json.parseFromSlice(Data, self.gpa, bytes, .{ .ignore_unknown_fields = true }) catch return;
        defer parsed.deinit();
        const a = self.arena.allocator();
        self.data = .{
            .personal_api_key = dupOpt(a, parsed.value.personal_api_key),
            .personal_api_key_id = dupOpt(a, parsed.value.personal_api_key_id),
            .personal_api_key_start = dupOpt(a, parsed.value.personal_api_key_start),
            .claude_path = dupOpt(a, parsed.value.claude_path),
            .repos_root = dupOpt(a, parsed.value.repos_root),
            .branch_prefix = dupOpt(a, parsed.value.branch_prefix),
            .device_id = dupOpt(a, parsed.value.device_id),
        };
    }

    pub fn save(self: *Store) !void {
        const json = try std.json.Stringify.valueAlloc(self.gpa, self.data, .{});
        defer self.gpa.free(json);
        try storage.writeSecret(self.path, json);
    }

    // --- setters (dup into the arena, replacing any prior value) ---

    /// Empty/whitespace-only input clears the override (falls back to default).
    pub fn setClaudePath(self: *Store, v: []const u8) void {
        self.data.claude_path = self.dupOverride(v);
    }
    pub fn setReposRoot(self: *Store, v: []const u8) void {
        self.data.repos_root = self.dupOverride(v);
    }
    pub fn setBranchPrefix(self: *Store, v: []const u8) void {
        self.data.branch_prefix = self.dupOverride(v);
    }

    /// Persist a freshly minted personal API key (raw key + id + display start).
    pub fn setPersonalKey(self: *Store, key: []const u8, id: []const u8, start: []const u8) void {
        const a = self.arena.allocator();
        self.data.personal_api_key = a.dupe(u8, key) catch null;
        self.data.personal_api_key_id = a.dupe(u8, id) catch null;
        self.data.personal_api_key_start = a.dupe(u8, start) catch null;
    }

    pub fn clearPersonalKey(self: *Store) void {
        self.data.personal_api_key = null;
        self.data.personal_api_key_id = null;
        self.data.personal_api_key_start = null;
    }

    fn dupOverride(self: *Store, v: []const u8) ?[]const u8 {
        const trimmed = std.mem.trim(u8, v, " \t\r\n");
        if (trimmed.len == 0) return null;
        return self.arena.allocator().dupe(u8, trimmed) catch null;
    }

    // --- effective values (override folded with the default) ---

    pub fn claudePath(self: *const Store) []const u8 {
        return self.data.claude_path orelse default_claude_path;
    }
    pub fn branchPrefix(self: *const Store) []const u8 {
        return self.data.branch_prefix orelse default_branch_prefix;
    }
    /// The effective repos root (caller owns the returned path). Defaults to
    /// `$HOME/Exponential/repos`.
    pub fn reposRoot(self: *const Store, gpa: std.mem.Allocator) ![]u8 {
        if (self.data.repos_root) |r| return gpa.dupe(u8, r);
        return defaultReposRoot(gpa);
    }
    pub fn personalKey(self: *const Store) ?[]const u8 {
        return self.data.personal_api_key;
    }
    pub fn personalKeyId(self: *const Store) ?[]const u8 {
        return self.data.personal_api_key_id;
    }
    pub fn personalKeyStart(self: *const Store) ?[]const u8 {
        return self.data.personal_api_key_start;
    }

    /// The stable steer-relay device id. Generated (32 hex chars) and persisted
    /// on first use; a failed save just means a fresh id next launch — the relay
    /// treats a device id as ephemeral presence, so that's harmless.
    pub fn deviceId(self: *Store) []const u8 {
        if (self.data.device_id) |id| return id;
        var raw: [16]u8 = undefined;
        steer_util.fillRandom(&raw);
        var hex: [32]u8 = undefined;
        const alphabet = "0123456789abcdef";
        for (raw, 0..) |b, i| {
            hex[i * 2] = alphabet[b >> 4];
            hex[i * 2 + 1] = alphabet[b & 0x0F];
        }
        const owned = self.arena.allocator().dupe(u8, &hex) catch return "linux-desktop";
        self.data.device_id = owned;
        self.save() catch {};
        return owned;
    }
};

/// Human device label for relay presence + `codingSessions.start`: the host
/// name when readable, else a generic fallback. Caller owns the result.
pub fn hostDeviceLabel(gpa: std.mem.Allocator) []u8 {
    if (storage.readFileAlloc(gpa, "/etc/hostname")) |bytes| {
        defer gpa.free(bytes);
        const trimmed = std.mem.trim(u8, bytes, " \t\r\n");
        if (trimmed.len > 0) return gpa.dupe(u8, trimmed) catch (gpa.dupe(u8, "Linux desktop") catch unreachable);
    }
    return gpa.dupe(u8, "Linux desktop") catch unreachable;
}

/// `$HOME/Exponential/repos` (caller owns it). Falls back to a relative path if
/// `$HOME` is unset (never happens in practice; keeps this total).
pub fn defaultReposRoot(gpa: std.mem.Allocator) ![]u8 {
    if (std.c.getenv("HOME")) |home| {
        const h = std.mem.span(home);
        if (h.len > 0) return std.fs.path.join(gpa, &.{ h, "Exponential", "repos" });
    }
    return gpa.dupe(u8, "Exponential/repos");
}

fn dupOpt(a: std.mem.Allocator, v: ?[]const u8) ?[]const u8 {
    return if (v) |s| (a.dupe(u8, s) catch null) else null;
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

test "effective defaults fold in when overrides are unset" {
    const gpa = std.testing.allocator;
    var store = Store{
        .gpa = gpa,
        .arena = std.heap.ArenaAllocator.init(gpa),
        .path = "",
    };
    defer store.deinit();

    try std.testing.expectEqualStrings("claude", store.claudePath());
    try std.testing.expectEqualStrings("exp/", store.branchPrefix());
    try std.testing.expect(store.personalKey() == null);

    store.setClaudePath("/opt/claude/bin/claude");
    store.setBranchPrefix("feat/");
    try std.testing.expectEqualStrings("/opt/claude/bin/claude", store.claudePath());
    try std.testing.expectEqualStrings("feat/", store.branchPrefix());

    // Whitespace clears the override back to the default.
    store.setBranchPrefix("   ");
    try std.testing.expectEqualStrings("exp/", store.branchPrefix());
}

test "deviceId is generated once and stays stable in-process" {
    const gpa = std.testing.allocator;
    var store = Store{
        .gpa = gpa,
        .arena = std.heap.ArenaAllocator.init(gpa),
        .path = "", // save() fails silently — id still cached in the arena
    };
    defer store.deinit();

    const first = store.deviceId();
    try std.testing.expectEqual(@as(usize, 32), first.len);
    for (first) |ch| try std.testing.expect(std.ascii.isHex(ch));
    const second = store.deviceId();
    try std.testing.expectEqualStrings(first, second);
}

test "personal key round-trips through JSON" {
    const gpa = std.testing.allocator;
    const data = Data{
        .personal_api_key = "expu_secret_raw",
        .personal_api_key_id = "key_123",
        .personal_api_key_start = "expu_secr",
        .branch_prefix = "exp/",
    };
    const json = try std.json.Stringify.valueAlloc(gpa, data, .{});
    defer gpa.free(json);
    const parsed = try std.json.parseFromSlice(Data, gpa, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expectEqualStrings("expu_secret_raw", parsed.value.personal_api_key.?);
    try std.testing.expectEqualStrings("key_123", parsed.value.personal_api_key_id.?);
}
