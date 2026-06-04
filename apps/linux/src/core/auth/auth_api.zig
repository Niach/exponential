//! Better Auth sign-in — port of the iOS `AuthApi.signInWithPassword`.
//! POST {instanceUrl}/api/auth/sign-in/email with {email,password}; the bearer()
//! plugin returns {token, user:{id,email,name,isAdmin}}.

const std = @import("std");
const http = @import("../api/http.zig");
const trpc = @import("../api/trpc.zig");

pub const Error = error{ParseFailed} || http.Error;

/// Owns a parsed sign-in response; read fields then `deinit`.
pub const SignInResult = struct {
    parsed: std.json.Parsed(std.json.Value),
    status: i64,

    pub fn deinit(self: *SignInResult) void {
        self.parsed.deinit();
    }

    pub fn token(self: *const SignInResult) ?[]const u8 {
        const obj = trpc.asObject(self.parsed.value) orelse return null;
        return trpc.objString(obj, "token");
    }

    fn userObj(self: *const SignInResult) ?std.json.ObjectMap {
        const obj = trpc.asObject(self.parsed.value) orelse return null;
        return trpc.asObject(obj.get("user") orelse return null);
    }

    pub fn userId(self: *const SignInResult) ?[]const u8 {
        return trpc.objString(self.userObj() orelse return null, "id");
    }
    pub fn email(self: *const SignInResult) ?[]const u8 {
        return trpc.objString(self.userObj() orelse return null, "email");
    }
    pub fn name(self: *const SignInResult) ?[]const u8 {
        return trpc.objString(self.userObj() orelse return null, "name");
    }
    pub fn isAdmin(self: *const SignInResult) bool {
        return trpc.objBool(self.userObj() orelse return false, "isAdmin");
    }

    /// tRPC/Better-Auth error message, if any (e.g. invalid credentials).
    pub fn errorMessage(self: *const SignInResult) ?[]const u8 {
        const obj = trpc.asObject(self.parsed.value) orelse return null;
        if (trpc.objString(obj, "message")) |m| return m; // Better Auth error shape
        return null;
    }

    pub fn ok(self: *const SignInResult) bool {
        return self.status >= 200 and self.status < 300 and self.token() != null;
    }
};

pub const ConfigError = error{ ParseFailed, HttpError } || http.Error;

/// /api/auth-config — which sign-in methods an instance offers. Owns its parsed
/// body; read via the accessors then `deinit`.
pub const AuthConfig = struct {
    parsed: std.json.Parsed(std.json.Value),

    pub fn deinit(self: *AuthConfig) void {
        self.parsed.deinit();
    }
    fn root(self: *const AuthConfig) ?std.json.ObjectMap {
        return trpc.asObject(self.parsed.value);
    }
    pub fn passwordEnabled(self: *const AuthConfig) bool {
        return trpc.objBool(self.root() orelse return false, "passwordEnabled");
    }
    pub fn googleLoginEnabled(self: *const AuthConfig) bool {
        return trpc.objBool(self.root() orelse return false, "googleLoginEnabled");
    }
    /// The `oidcProviders` array value (each element `{id, name}`), or null.
    pub fn oidcProviders(self: *const AuthConfig) ?std.json.Value {
        return (self.root() orelse return null).get("oidcProviders");
    }
};

pub fn fetchAuthConfig(
    allocator: std.mem.Allocator,
    instance_url: []const u8,
    timeout_s: c_long,
) ConfigError!AuthConfig {
    var scratch = std.heap.ArenaAllocator.init(allocator);
    defer scratch.deinit();
    const trimmed = std.mem.trimEnd(u8, instance_url, "/");
    const url = try std.fmt.allocPrintSentinel(scratch.allocator(), "{s}/api/auth-config", .{trimmed}, 0);

    var resp = try http.get(allocator, url, null, timeout_s, null);
    defer resp.deinit();
    if (resp.status < 200 or resp.status >= 300) return ConfigError.HttpError;

    const parsed = std.json.parseFromSlice(std.json.Value, allocator, resp.body, .{}) catch
        return ConfigError.ParseFailed;
    return .{ .parsed = parsed };
}

pub fn signInWithPassword(
    allocator: std.mem.Allocator,
    instance_url: []const u8,
    email: []const u8,
    password: []const u8,
    timeout_s: c_long,
) Error!SignInResult {
    var scratch = std.heap.ArenaAllocator.init(allocator);
    defer scratch.deinit();
    const sa = scratch.allocator();

    const trimmed = std.mem.trimEnd(u8, instance_url, "/");
    const url = try std.fmt.allocPrintSentinel(sa, "{s}/api/auth/sign-in/email", .{trimmed}, 0);
    // Stringify for correct JSON escaping of email/password.
    const body = try std.json.Stringify.valueAlloc(sa, .{ .email = email, .password = password }, .{});

    var resp = try http.post(allocator, url, null, body, timeout_s, null);
    defer resp.deinit();

    const parsed = std.json.parseFromSlice(std.json.Value, allocator, resp.body, .{}) catch
        return Error.ParseFailed;
    return .{ .parsed = parsed, .status = resp.status };
}

/// GET {instance}/api/auth/get-session with the Bearer token → `{session, user}`.
/// OAuth/OIDC logins only hand back a token (no user object like the password
/// sign-in response), so we fetch the session to fill in the identity. Reuses
/// SignInResult's `user.*` accessors (userId/email/name/isAdmin).
pub fn fetchSession(
    allocator: std.mem.Allocator,
    instance_url: []const u8,
    token: []const u8,
    timeout_s: c_long,
) Error!SignInResult {
    var scratch = std.heap.ArenaAllocator.init(allocator);
    defer scratch.deinit();
    const trimmed = std.mem.trimEnd(u8, instance_url, "/");
    const url = try std.fmt.allocPrintSentinel(scratch.allocator(), "{s}/api/auth/get-session", .{trimmed}, 0);

    var resp = try http.get(allocator, url, token, timeout_s, null);
    defer resp.deinit();

    const parsed = std.json.parseFromSlice(std.json.Value, allocator, resp.body, .{}) catch
        return Error.ParseFailed;
    return .{ .parsed = parsed, .status = resp.status };
}
