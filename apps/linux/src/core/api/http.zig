//! Minimal libcurl wrapper for the Electric long-poll loop, tRPC, and auth.
//! Blocking GET/POST that collect the body and the `electric-handle` /
//! `electric-offset` response headers the shape protocol depends on.

const std = @import("std");

pub const c = @cImport({
    @cInclude("curl/curl.h");
});

pub const Error = error{ CurlInit, CurlPerform } || std.mem.Allocator.Error;

pub const Method = enum { GET, POST };

// curl_global_init is not thread-safe; call it once before any easy handle.
var global_init_done: bool = false;
fn ensureGlobalInit() void {
    if (!global_init_done) {
        _ = c.curl_global_init(c.CURL_GLOBAL_DEFAULT);
        global_init_done = true;
    }
}

pub const Response = struct {
    status: i64,
    body: []u8,
    handle: ?[]u8,
    offset: ?[]u8,
    allocator: std.mem.Allocator,

    pub fn deinit(self: *Response) void {
        self.allocator.free(self.body);
        if (self.handle) |h| self.allocator.free(h);
        if (self.offset) |o| self.allocator.free(o);
    }
};

const Ctx = struct {
    allocator: std.mem.Allocator,
    body: std.ArrayList(u8) = .empty,
    handle: ?[]u8 = null,
    offset: ?[]u8 = null,
    oom: bool = false,
};

fn writeCb(ptr: [*c]u8, size: usize, nmemb: usize, userdata: ?*anyopaque) callconv(.c) usize {
    const ctx: *Ctx = @ptrCast(@alignCast(userdata.?));
    const n = size * nmemb;
    ctx.body.appendSlice(ctx.allocator, ptr[0..n]) catch {
        ctx.oom = true;
        return 0; // signal write error → aborts the transfer
    };
    return n;
}

fn captureHeader(ctx: *Ctx, line: []const u8) !void {
    const colon = std.mem.indexOfScalar(u8, line, ':') orelse return;
    const name = std.mem.trim(u8, line[0..colon], " \t\r\n");
    const value = std.mem.trim(u8, line[colon + 1 ..], " \t\r\n");
    if (std.ascii.eqlIgnoreCase(name, "electric-handle")) {
        if (ctx.handle == null) ctx.handle = try ctx.allocator.dupe(u8, value);
    } else if (std.ascii.eqlIgnoreCase(name, "electric-offset")) {
        if (ctx.offset == null) ctx.offset = try ctx.allocator.dupe(u8, value);
    }
}

fn headerCb(buffer: [*c]u8, size: usize, nitems: usize, userdata: ?*anyopaque) callconv(.c) usize {
    const ctx: *Ctx = @ptrCast(@alignCast(userdata.?));
    const n = size * nitems;
    captureHeader(ctx, buffer[0..n]) catch {
        ctx.oom = true;
    };
    return n;
}

/// libcurl progress callback (fires ~once/sec, even while a long-poll idles).
/// Returning non-zero aborts the transfer — so a set `cancel` flag stops an
/// in-flight long-poll within ~1s (used for sign-out / shutdown).
fn progressCb(clientp: ?*anyopaque, dltotal: c.curl_off_t, dlnow: c.curl_off_t, ultotal: c.curl_off_t, ulnow: c.curl_off_t) callconv(.c) c_int {
    _ = dltotal;
    _ = dlnow;
    _ = ultotal;
    _ = ulnow;
    const flag: *std.atomic.Value(bool) = @ptrCast(@alignCast(clientp orelse return 0));
    return if (flag.load(.acquire)) 1 else 0;
}

/// Core request. `body` is sent only for POST (not copied by curl, so it must
/// outlive the call — it does). `timeout_s` should exceed the ~60s long-poll
/// window for live GETs (iOS/Android use 90s).
pub fn request(
    allocator: std.mem.Allocator,
    method: Method,
    url: [:0]const u8,
    bearer: ?[]const u8,
    body: ?[]const u8,
    content_type: ?[]const u8,
    timeout_s: c_long,
    cancel: ?*std.atomic.Value(bool),
) Error!Response {
    ensureGlobalInit();
    const handle = c.curl_easy_init() orelse return Error.CurlInit;
    defer c.curl_easy_cleanup(handle);

    var ctx = Ctx{ .allocator = allocator };
    errdefer {
        ctx.body.deinit(allocator);
        if (ctx.handle) |h| allocator.free(h);
        if (ctx.offset) |o| allocator.free(o);
    }

    var headers: ?*c.curl_slist = null;
    defer if (headers) |h| c.curl_slist_free_all(h);
    headers = c.curl_slist_append(headers, "Accept: application/json");

    var auth_buf: ?[:0]u8 = null;
    defer if (auth_buf) |b| allocator.free(b);
    if (bearer) |tok| {
        auth_buf = try std.fmt.allocPrintSentinel(allocator, "Authorization: Bearer {s}", .{tok}, 0);
        headers = c.curl_slist_append(headers, auth_buf.?.ptr);
    }

    var ct_buf: ?[:0]u8 = null;
    defer if (ct_buf) |b| allocator.free(b);
    if (method == .POST) {
        if (content_type) |ct| {
            ct_buf = std.fmt.allocPrintSentinel(allocator, "Content-Type: {s}", .{ct}, 0) catch null;
            if (ct_buf) |b| headers = c.curl_slist_append(headers, b.ptr);
        }
        _ = c.curl_easy_setopt(handle, c.CURLOPT_POST, @as(c_long, 1));
        const b = body orelse "";
        _ = c.curl_easy_setopt(handle, c.CURLOPT_POSTFIELDSIZE, @as(c_long, @intCast(b.len)));
        _ = c.curl_easy_setopt(handle, c.CURLOPT_POSTFIELDS, b.ptr);
    }

    _ = c.curl_easy_setopt(handle, c.CURLOPT_URL, url.ptr);
    _ = c.curl_easy_setopt(handle, c.CURLOPT_HTTPHEADER, headers);
    _ = c.curl_easy_setopt(handle, c.CURLOPT_WRITEFUNCTION, writeCb);
    _ = c.curl_easy_setopt(handle, c.CURLOPT_WRITEDATA, &ctx);
    _ = c.curl_easy_setopt(handle, c.CURLOPT_HEADERFUNCTION, headerCb);
    _ = c.curl_easy_setopt(handle, c.CURLOPT_HEADERDATA, &ctx);
    _ = c.curl_easy_setopt(handle, c.CURLOPT_TIMEOUT, timeout_s);
    _ = c.curl_easy_setopt(handle, c.CURLOPT_FOLLOWLOCATION, @as(c_long, 1));
    _ = c.curl_easy_setopt(handle, c.CURLOPT_USERAGENT, "exponential-desktop/0.1");

    // Cancellation: progress callback aborts the transfer when `cancel` is set
    // (null clientp → never aborts).
    _ = c.curl_easy_setopt(handle, c.CURLOPT_NOPROGRESS, @as(c_long, 0));
    _ = c.curl_easy_setopt(handle, c.CURLOPT_XFERINFOFUNCTION, progressCb);
    _ = c.curl_easy_setopt(handle, c.CURLOPT_XFERINFODATA, @as(?*anyopaque, @ptrCast(cancel)));

    if (c.curl_easy_perform(handle) != c.CURLE_OK) return Error.CurlPerform;
    if (ctx.oom) return Error.OutOfMemory;

    var status: c_long = 0;
    _ = c.curl_easy_getinfo(handle, c.CURLINFO_RESPONSE_CODE, &status);

    const owned_body = try ctx.body.toOwnedSlice(allocator);
    return .{
        .status = @intCast(status),
        .body = owned_body,
        .handle = ctx.handle,
        .offset = ctx.offset,
        .allocator = allocator,
    };
}

pub fn get(allocator: std.mem.Allocator, url: [:0]const u8, bearer: ?[]const u8, timeout_s: c_long, cancel: ?*std.atomic.Value(bool)) Error!Response {
    return request(allocator, .GET, url, bearer, null, null, timeout_s, cancel);
}

pub fn post(
    allocator: std.mem.Allocator,
    url: [:0]const u8,
    bearer: ?[]const u8,
    body: ?[]const u8,
    timeout_s: c_long,
    cancel: ?*std.atomic.Value(bool),
) Error!Response {
    return request(allocator, .POST, url, bearer, body, "application/json", timeout_s, cancel);
}

/// POST a pre-built multipart/form-data body (for image upload).
pub fn postMultipart(
    allocator: std.mem.Allocator,
    url: [:0]const u8,
    bearer: ?[]const u8,
    body: []const u8,
    content_type: []const u8,
    timeout_s: c_long,
) Error!Response {
    return request(allocator, .POST, url, bearer, body, content_type, timeout_s, null);
}

test "libcurl links and reports a version" {
    const v = c.curl_version();
    try std.testing.expect(v != null);
    try std.testing.expect(std.mem.startsWith(u8, std.mem.span(v), "libcurl"));
}
