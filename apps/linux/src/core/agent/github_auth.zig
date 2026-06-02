//! GitHub OAuth **device flow** for the desktop app — a port of the companion's
//! `github-auth.ts`. The device flow needs no redirect URI: request a device
//! code, show the user a short code + verification URL, then poll until they
//! authorize. The resulting access token (for the agent to push code + open PRs)
//! is stored 0600 at `{configDir}/github.json`. The OAuth App client id comes
//! from the registered agent identity (`claimSetup` → `oauth.githubClientId`).

const std = @import("std");
const http = @import("../api/http.zig");
const storage = @import("../storage.zig");

const Timespec = extern struct { sec: isize, nsec: isize };
extern fn nanosleep(req: *const Timespec, rem: ?*Timespec) c_int;

const DEVICE_CODE_URL = "https://github.com/login/device/code";
const TOKEN_URL = "https://github.com/login/oauth/access_token";
const USER_URL = "https://api.github.com/user";
// URLSearchParams-equivalent encoding (space→+, ':'→%3A); client_id/device_code
// are token-safe so they're interpolated raw.
const SCOPE_ENCODED = "repo+read%3Auser";
const GRANT_ENCODED = "urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code";
const FORM_CT = "application/x-www-form-urlencoded";

pub const DeviceCode = struct {
    device_code: []u8,
    user_code: []u8,
    verification_uri: []u8,
    interval_s: u64,
    expires_in_s: u64,

    pub fn deinit(self: *DeviceCode, gpa: std.mem.Allocator) void {
        gpa.free(self.device_code);
        gpa.free(self.user_code);
        gpa.free(self.verification_uri);
    }
};

/// Step 1: request a device + user code (one quick POST). Caller shows the
/// user_code + verification_uri and opens the browser.
pub fn requestDeviceCode(gpa: std.mem.Allocator, client_id: []const u8) !DeviceCode {
    const body = try std.fmt.allocPrint(gpa, "client_id={s}&scope={s}", .{ client_id, SCOPE_ENCODED });
    defer gpa.free(body);
    var resp = try http.request(gpa, .POST, DEVICE_CODE_URL, null, body, FORM_CT, 20, null);
    defer resp.deinit();
    if (resp.status < 200 or resp.status >= 300) return error.DeviceCodeRequestFailed;

    var parsed = std.json.parseFromSlice(std.json.Value, gpa, resp.body, .{}) catch return error.BadResponse;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return error.BadResponse,
    };
    return .{
        .device_code = try dupStr(gpa, obj, "device_code") orelse return error.NoDeviceCode,
        .user_code = (try dupStr(gpa, obj, "user_code")) orelse try gpa.dupe(u8, ""),
        .verification_uri = (try dupStr(gpa, obj, "verification_uri")) orelse try gpa.dupe(u8, "https://github.com/login/device"),
        .interval_s = intField(obj, "interval", 5),
        .expires_in_s = intField(obj, "expires_in", 900),
    };
}

pub const Connected = struct { token: []u8, login: []u8 };
pub const Outcome = union(enum) {
    success: Connected, // owned by gpa
    failure: []u8, // owned by gpa
};

/// Step 2: poll the token endpoint until the user authorizes, GitHub errors, or
/// the code expires. Blocking — run on a worker thread. On success the token is
/// persisted and the user login fetched.
pub fn pollUntilDone(gpa: std.mem.Allocator, client_id: []const u8, dc: *const DeviceCode) Outcome {
    var interval_ms: u64 = dc.interval_s * 1000;
    var elapsed: u64 = 0;
    const deadline_ms = dc.expires_in_s * 1000;

    const body = std.fmt.allocPrint(gpa, "client_id={s}&device_code={s}&grant_type={s}", .{ client_id, dc.device_code, GRANT_ENCODED }) catch
        return fail(gpa, "out of memory");
    defer gpa.free(body);

    while (elapsed < deadline_ms) {
        sleepMs(interval_ms);
        elapsed += interval_ms;

        var resp = http.request(gpa, .POST, TOKEN_URL, null, body, FORM_CT, 20, null) catch continue;
        defer resp.deinit();
        var parsed = std.json.parseFromSlice(std.json.Value, gpa, resp.body, .{}) catch continue;
        defer parsed.deinit();
        const obj = switch (parsed.value) {
            .object => |o| o,
            else => continue,
        };
        if (obj.get("access_token")) |t| switch (t) {
            .string => |tok| {
                const token = gpa.dupe(u8, tok) catch return fail(gpa, "out of memory");
                const login = fetchLogin(gpa, token) catch null;
                saveToken(gpa, token, login orelse "") catch {};
                return .{ .success = .{ .token = token, .login = login orelse (gpa.dupe(u8, "") catch "") } };
            },
            else => {},
        };
        const err = if (obj.get("error")) |e| switch (e) {
            .string => |s| s,
            else => "",
        } else "";
        if (std.mem.eql(u8, err, "authorization_pending")) {
            // keep polling
        } else if (std.mem.eql(u8, err, "slow_down")) {
            interval_ms += 5000;
        } else if (std.mem.eql(u8, err, "expired_token")) {
            return fail(gpa, "Device code expired before authorization completed.");
        } else if (std.mem.eql(u8, err, "access_denied")) {
            return fail(gpa, "Authorization was denied.");
        } else if (err.len > 0) {
            return fail(gpa, "GitHub device-flow error.");
        }
    }
    return fail(gpa, "Timed out waiting for GitHub authorization.");
}

/// GET /user to resolve the login for display (best-effort).
fn fetchLogin(gpa: std.mem.Allocator, token: []const u8) !?[]u8 {
    var resp = try http.get(gpa, USER_URL, token, 15, null);
    defer resp.deinit();
    if (resp.status < 200 or resp.status >= 300) return null;
    var parsed = std.json.parseFromSlice(std.json.Value, gpa, resp.body, .{}) catch return null;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return null,
    };
    return try dupStr(gpa, obj, "login");
}

// --- token storage ({configDir}/github.json, 0600) ---

fn tokenPath(gpa: std.mem.Allocator) ![]u8 {
    const dir = try storage.configDir(gpa);
    defer gpa.free(dir);
    return std.fmt.allocPrint(gpa, "{s}/github.json", .{dir});
}

pub fn saveToken(gpa: std.mem.Allocator, token: []const u8, login: []const u8) !void {
    const dir = try storage.configDir(gpa);
    defer gpa.free(dir);
    try storage.ensureDir(dir);
    const path = try tokenPath(gpa);
    defer gpa.free(path);
    const json = try std.json.Stringify.valueAlloc(gpa, .{ .token = token, .login = login }, .{});
    defer gpa.free(json);
    try storage.writeSecret(path, json);
}

/// The stored access token (caller frees), or null when not connected.
pub fn loadToken(gpa: std.mem.Allocator) ?[]u8 {
    return readField(gpa, "token");
}

/// The stored GitHub login (caller frees), or null.
pub fn loadLogin(gpa: std.mem.Allocator) ?[]u8 {
    return readField(gpa, "login");
}

pub fn connected(gpa: std.mem.Allocator) bool {
    if (loadToken(gpa)) |t| {
        defer gpa.free(t);
        return t.len > 0;
    }
    return false;
}

pub fn deleteToken(gpa: std.mem.Allocator) void {
    const path = tokenPath(gpa) catch return;
    defer gpa.free(path);
    storage.deleteFile(path);
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

// --- helpers ---

fn dupStr(gpa: std.mem.Allocator, obj: std.json.ObjectMap, key: []const u8) !?[]u8 {
    const v = obj.get(key) orelse return null;
    return switch (v) {
        .string => |s| try gpa.dupe(u8, s),
        else => null,
    };
}

fn intField(obj: std.json.ObjectMap, key: []const u8, default: u64) u64 {
    const v = obj.get(key) orelse return default;
    return switch (v) {
        .integer => |i| if (i > 0) @intCast(i) else default,
        .float => |f| if (f > 0) @intFromFloat(f) else default,
        else => default,
    };
}

// On OOM the fallback is a zero-length slice; callers free outcome strings only
// when `len > 0` (see settings.zig), so this never frees a non-heap pointer.
fn fail(gpa: std.mem.Allocator, comptime msg: []const u8) Outcome {
    return .{ .failure = gpa.dupe(u8, msg) catch @constCast(@as([]const u8, "")) };
}

fn sleepMs(ms: u64) void {
    const ts = Timespec{ .sec = @intCast(ms / 1000), .nsec = @intCast((ms % 1000) * std.time.ns_per_ms) };
    _ = nanosleep(&ts, null);
}
