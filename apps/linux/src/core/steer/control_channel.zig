//! The desktop's outbound device-presence socket (masterplan §3.2; the Zig
//! sibling of MacSteerControlChannel). While the user is signed in, one
//! background thread keeps a control WebSocket to the steer relay: it announces
//! `online{deviceId, deviceLabel}` so the phone's "Start on my desktop" picker
//! can see this machine, and routes an inbound `start_session{issueId}` to the
//! coding launcher.
//!
//! Graceful-off (masterplan §3.6): `steer.config` disabled / mintTicket
//! `{disabled:true}` / 404 ⇒ no socket is opened and the next recheck is slow
//! (~15 min) — never a hot loop. The relay is purely additive; local coding is
//! never gated on it.
//!
//! Threading: everything here runs on the channel's own thread. `on_start`
//! fires ON THAT THREAD — the UI wiring (app.zig) marshals to the GTK main
//! loop via `g_idle_add`, mirroring how SyncManager's `notify` hop works.

const std = @import("std");
const trpc = @import("../api/trpc.zig");
const protocol = @import("protocol.zig");
const ws = @import("ws_client.zig");
const util = @import("util.zig");

const timespec = extern struct { sec: c_long, nsec: c_long };
extern "c" fn nanosleep(req: *const timespec, rem: ?*timespec) c_int;

fn sleepMs(ms: u64) void {
    var ts = timespec{ .sec = @intCast(ms / 1000), .nsec = @intCast((ms % 1000) * 1_000_000) };
    _ = nanosleep(&ts, null);
}

/// Fired on the CHANNEL thread when the relay routes a remote start here.
/// `issue_id` is only valid for the duration of the call — dupe before hopping
/// to the main loop.
pub const StartSessionFn = *const fn (ctx: ?*anyopaque, issue_id: []const u8) void;

const disabled_recheck_ms: u64 = 15 * 60 * 1000; // config off → slow recheck
const max_backoff_ms: u64 = 30 * 1000;

pub const ControlChannel = struct {
    gpa: std.mem.Allocator,
    instance: []u8,
    token: ?[]u8,
    device_id: []u8,
    device_label: []u8,
    on_start: StartSessionFn,
    on_start_ctx: ?*anyopaque,

    stop_flag: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    conn_mutex: util.Mutex = .{},
    conn: ?*ws.Client = null, // guarded by conn_mutex; owned by the thread
    thread: ?std.Thread = null,

    /// Dupe the creds and start the background thread. `gpa` must be
    /// thread-safe (the app uses page_allocator).
    pub fn create(
        gpa: std.mem.Allocator,
        instance: []const u8,
        token: ?[]const u8,
        device_id: []const u8,
        device_label: []const u8,
        on_start: StartSessionFn,
        on_start_ctx: ?*anyopaque,
    ) ?*ControlChannel {
        const self = gpa.create(ControlChannel) catch return null;
        self.* = .{
            .gpa = gpa,
            .instance = gpa.dupe(u8, instance) catch {
                gpa.destroy(self);
                return null;
            },
            .token = if (token) |t| (gpa.dupe(u8, t) catch null) else null,
            .device_id = gpa.dupe(u8, device_id) catch {
                gpa.free(self.instance);
                if (self.token) |t| gpa.free(t);
                gpa.destroy(self);
                return null;
            },
            .device_label = gpa.dupe(u8, device_label) catch {
                gpa.free(self.instance);
                if (self.token) |t| gpa.free(t);
                gpa.free(self.device_id);
                gpa.destroy(self);
                return null;
            },
            .on_start = on_start,
            .on_start_ctx = on_start_ctx,
        };
        self.thread = std.Thread.spawn(.{}, run, .{self}) catch {
            self.freeFields();
            gpa.destroy(self);
            return null;
        };
        return self;
    }

    /// Stop + join the thread, then free. A live socket read unblocks via
    /// shutdown; a sleeping backoff exits within ~250ms. (A tRPC call in
    /// flight can hold the join for up to its 15s timeout — sign-out only.)
    pub fn destroy(self: *ControlChannel) void {
        self.stop_flag.store(true, .release);
        {
            self.conn_mutex.lock();
            defer self.conn_mutex.unlock();
            if (self.conn) |client| client.shutdownSocket();
        }
        if (self.thread) |t| t.join();
        const gpa = self.gpa;
        self.freeFields();
        gpa.destroy(self);
    }

    fn freeFields(self: *ControlChannel) void {
        self.gpa.free(self.instance);
        if (self.token) |t| self.gpa.free(t);
        self.gpa.free(self.device_id);
        self.gpa.free(self.device_label);
    }

    fn stopped(self: *ControlChannel) bool {
        return self.stop_flag.load(.acquire);
    }

    /// Interruptible sleep in ~250ms slices.
    fn snooze(self: *ControlChannel, total_ms: u64) void {
        var left = total_ms;
        while (left > 0 and !self.stopped()) {
            const slice = @min(left, 250);
            sleepMs(slice);
            left -= slice;
        }
    }

    fn run(self: *ControlChannel) void {
        var backoff_ms: u64 = 2_000;
        while (!self.stopped()) {
            switch (self.mintConnectUrl()) {
                .disabled => self.snooze(disabled_recheck_ms),
                .err => {
                    self.snooze(backoff_ms);
                    backoff_ms = @min(backoff_ms * 2, max_backoff_ms);
                },
                .url => |url| {
                    defer self.gpa.free(url);
                    if (self.serve(url)) backoff_ms = 2_000; // had a live session
                    if (self.stopped()) return;
                    self.snooze(backoff_ms);
                    backoff_ms = @min(backoff_ms * 2, max_backoff_ms);
                },
            }
        }
    }

    const MintResult = union(enum) { disabled, err, url: []u8 };

    /// steer.config → enabled? → steer.mintTicket(control) → dial URL.
    fn mintConnectUrl(self: *ControlChannel) MintResult {
        const gpa = self.gpa;

        // Config gate: relay unset on this instance ⇒ zero sockets.
        {
            var resp = trpc.query(gpa, self.instance, "steer.config", self.token, 15) catch return .err;
            defer resp.deinit();
            if (!resp.ok()) return if (resp.status == 404) .disabled else .err;
            const obj = dataObject(&resp) orelse return .err;
            if (!trpc.objBool(obj, "enabled")) return .disabled;
        }

        const label_json = protocol.jsonStringAlloc(gpa, self.device_label) catch return .err;
        defer gpa.free(label_json);
        const input = std.fmt.allocPrint(gpa, "{{\"kind\":\"control\",\"deviceLabel\":{s}}}", .{label_json}) catch return .err;
        defer gpa.free(input);
        var resp = trpc.call(gpa, self.instance, "steer.mintTicket", input, self.token, 15) catch return .err;
        defer resp.deinit();
        if (!resp.ok()) return if (resp.status == 404) .disabled else .err;
        const obj = dataObject(&resp) orelse return .err;
        if (trpc.objBool(obj, "disabled")) return .disabled;
        const ticket = trpc.objString(obj, "ticket") orelse return .disabled;
        const url = trpc.objString(obj, "url") orelse return .disabled;
        const dial = ws.urlWithTicket(gpa, url, ticket) catch return .err;
        return .{ .url = dial };
    }

    /// One connected control session: online announce + frame loop. Returns
    /// true when the socket connected (resets the reconnect backoff).
    fn serve(self: *ControlChannel, url: []const u8) bool {
        const gpa = self.gpa;
        const client = ws.Client.connect(gpa, url, 55) catch |e| {
            if (e == ws.ConnectError.TlsUnsupported) {
                std.log.warn("steer: wss:// relay URLs are not supported by the Linux client yet — point STEER_RELAY_URL at a plain ws:// (LAN) endpoint", .{});
            }
            return false;
        };
        {
            self.conn_mutex.lock();
            defer self.conn_mutex.unlock();
            self.conn = client;
        }
        defer {
            self.conn_mutex.lock();
            self.conn = null;
            self.conn_mutex.unlock();
            client.destroy();
        }
        if (self.stopped()) return true;

        const online = protocol.onlineFrame(gpa, self.device_id, self.device_label) catch return true;
        defer gpa.free(online);
        client.sendText(online) catch return true;

        while (!self.stopped()) {
            const msg = client.next() catch return true;
            switch (msg) {
                .text => |text| {
                    defer gpa.free(text);
                    var arena = std.heap.ArenaAllocator.init(gpa);
                    defer arena.deinit();
                    if (protocol.parseInbound(arena.allocator(), text)) |frame| {
                        switch (frame) {
                            .start_session => |issue_id| {
                                if (issue_id.len > 0) self.on_start(self.on_start_ctx, issue_id);
                            },
                            else => {},
                        }
                    }
                },
                .binary => |bytes| gpa.free(bytes), // not expected on control
                .closed => return true,
            }
        }
        return true;
    }
};

/// `result.data`, unwrapping a `{ json: … }` transformer layer (mirrors the
/// coding launcher's dataObject).
fn dataObject(resp: *const trpc.Response) ?std.json.ObjectMap {
    const dv = resp.data() orelse return null;
    var obj = trpc.asObject(dv) orelse return null;
    if (obj.get("json")) |inner| {
        if (trpc.asObject(inner)) |inner_obj| obj = inner_obj;
    }
    return obj;
}
