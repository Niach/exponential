//! Per-session steer publisher (masterplan §3.3; the Zig sibling of
//! MacSteerPublisher). One publisher per terminal tab, keyed by its
//! `coding_sessions.id`: it mints a short-lived publisher ticket over tRPC,
//! dials the relay OUTBOUND, registers the session's room (`hello`), tees the
//! host PTY's output as binary `0x01` frames, and routes the steering viewer's
//! keystrokes back to the same PTY master local keys use.
//!
//! Backpressure: output frames are sent best-effort (`sendBinaryNoWait`) — a
//! slow relay/viewer drops frames rather than stalling the PTY reader (which
//! also feeds the LOCAL terminal); a viewer recovers via `resync`, replayed
//! from a bounded 256 KiB ring of recent output.
//!
//! v1 does not auto-reconnect a dropped publisher socket (mirrors macOS): the
//! session keeps running locally; the relay marks the room stale.
//!
//! Threading: the socket loop runs on its own thread. `feed` is called from
//! the PTY reader thread; `sendResize`/`stop` from the GTK main thread; the
//! `on_input`/`on_kill`/`on_remote_resize` callbacks fire on the SOCKET thread
//! (callers marshal to the main loop themselves where needed).

const std = @import("std");
const trpc = @import("../api/trpc.zig");
const protocol = @import("protocol.zig");
const ws = @import("ws_client.zig");
const util = @import("util.zig");

const ring_capacity: usize = 256 * 1024;

/// Steering viewer keystrokes (utf8) — SOCKET thread. Write them to the PTY.
pub const InputFn = *const fn (ctx: ?*anyopaque, bytes: []const u8) void;
/// Kill-switch — SOCKET thread. Tear the session down (marshal to main).
pub const KillFn = *const fn (ctx: ?*anyopaque) void;
/// Remote resize — SOCKET thread. Update the PTY winsize.
pub const ResizeFn = *const fn (ctx: ?*anyopaque, cols: u16, rows: u16) void;

pub const Options = struct {
    instance: []const u8,
    token: ?[]const u8,
    session_id: []const u8, // coding_sessions.id == the relay room key
    issue_id: []const u8,
    cols: u16 = 80,
    rows: u16 = 24,
    on_input: InputFn,
    on_kill: KillFn,
    on_remote_resize: ?ResizeFn = null,
    ctx: ?*anyopaque = null,
};

pub const Publisher = struct {
    gpa: std.mem.Allocator,
    instance: []u8,
    token: ?[]u8,
    session_id: []u8,
    issue_id: []u8,
    on_input: InputFn,
    on_kill: KillFn,
    on_remote_resize: ?ResizeFn,
    ctx: ?*anyopaque,

    ring: protocol.RingBuffer,
    ring_mutex: util.Mutex = .{},

    conn_mutex: util.Mutex = .{},
    conn: ?*ws.Client = null, // guarded by conn_mutex; owned by the thread
    connected: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    stop_flag: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    bye_sent: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    cols: std.atomic.Value(u16),
    rows: std.atomic.Value(u16),
    thread: ?std.Thread = null,

    /// Dupe everything and start the connect/read thread. Returns null on OOM
    /// (steering is purely additive — the caller just skips it). Dupes are
    /// built as locals first so a mid-way failure frees exactly what succeeded
    /// (a struct-literal `catch` can't free fields the struct hasn't taken).
    pub fn create(gpa: std.mem.Allocator, opts: Options) ?*Publisher {
        const instance = gpa.dupe(u8, opts.instance) catch return null;
        const token: ?[]u8 = if (opts.token) |t| (gpa.dupe(u8, t) catch null) else null;
        const session_id = gpa.dupe(u8, opts.session_id) catch {
            freeStrings(gpa, instance, token, null, null);
            return null;
        };
        const issue_id = gpa.dupe(u8, opts.issue_id) catch {
            freeStrings(gpa, instance, token, session_id, null);
            return null;
        };
        var ring = protocol.RingBuffer.init(gpa, ring_capacity) catch {
            freeStrings(gpa, instance, token, session_id, issue_id);
            return null;
        };
        const self = gpa.create(Publisher) catch {
            ring.deinit(gpa);
            freeStrings(gpa, instance, token, session_id, issue_id);
            return null;
        };
        self.* = .{
            .gpa = gpa,
            .instance = instance,
            .token = token,
            .session_id = session_id,
            .issue_id = issue_id,
            .on_input = opts.on_input,
            .on_kill = opts.on_kill,
            .on_remote_resize = opts.on_remote_resize,
            .ctx = opts.ctx,
            .ring = ring,
            .cols = std.atomic.Value(u16).init(opts.cols),
            .rows = std.atomic.Value(u16).init(opts.rows),
        };
        self.thread = std.Thread.spawn(.{}, run, .{self}) catch {
            self.ring.deinit(gpa);
            freeStrings(gpa, instance, token, session_id, issue_id);
            gpa.destroy(self);
            return null;
        };
        return self;
    }

    fn freeStrings(gpa: std.mem.Allocator, instance: []u8, token: ?[]u8, session_id: ?[]u8, issue_id: ?[]u8) void {
        gpa.free(instance);
        if (token) |t| gpa.free(t);
        if (session_id) |s| gpa.free(s);
        if (issue_id) |i| gpa.free(i);
    }

    /// End the room (bye + close). Idempotent; GTK main thread.
    pub fn stop(self: *Publisher, outcome: []const u8) void {
        if (self.stop_flag.swap(true, .acq_rel)) return;
        self.sendByeLocked(outcome);
        self.conn_mutex.lock();
        defer self.conn_mutex.unlock();
        if (self.conn) |client| client.shutdownSocket();
    }

    /// Stop (if still running), join the socket thread, free. Main thread.
    pub fn destroy(self: *Publisher) void {
        self.stop("closed");
        if (self.thread) |t| t.join();
        const gpa = self.gpa;
        self.ring.deinit(gpa);
        gpa.free(self.instance);
        if (self.token) |t| gpa.free(t);
        gpa.free(self.session_id);
        gpa.free(self.issue_id);
        gpa.destroy(self);
    }

    /// Tee a PTY output chunk: always buffered for resync; forwarded to the
    /// relay best-effort when connected. PTY reader thread.
    pub fn feed(self: *Publisher, bytes: []const u8) void {
        if (bytes.len == 0) return;
        {
            self.ring_mutex.lock();
            defer self.ring_mutex.unlock();
            self.ring.append(bytes);
        }
        if (!self.connected.load(.acquire)) return;
        const frame = protocol.outputFrame(self.gpa, bytes) catch return;
        defer self.gpa.free(frame);
        self.conn_mutex.lock();
        defer self.conn_mutex.unlock();
        const client = self.conn orelse return;
        client.sendBinaryNoWait(frame) catch |e| {
            if (e != ws.SendError.WouldBlock) self.connected.store(false, .release);
        };
    }

    /// The local terminal grid changed — remember it (for hello) and tell
    /// viewers to reflow. Main thread.
    pub fn sendResize(self: *Publisher, cols: u16, rows: u16) void {
        self.cols.store(cols, .release);
        self.rows.store(rows, .release);
        if (!self.connected.load(.acquire)) return;
        const frame = protocol.resizeFrame(self.gpa, cols, rows) catch return;
        defer self.gpa.free(frame);
        self.sendTextLocked(frame);
    }

    // --- socket thread ------------------------------------------------------

    fn run(self: *Publisher) void {
        const gpa = self.gpa;
        const url = self.mintUrl() orelse return; // disabled/unreachable ⇒ no socket
        defer gpa.free(url);
        if (self.stop_flag.load(.acquire)) return;

        const client = ws.Client.connect(gpa, url, 55) catch return;
        self.conn_mutex.lock();
        if (self.stop_flag.load(.acquire)) {
            self.conn_mutex.unlock();
            client.destroy();
            return;
        }
        self.conn = client;
        self.conn_mutex.unlock();
        defer {
            self.connected.store(false, .release);
            self.conn_mutex.lock();
            self.conn = null;
            self.conn_mutex.unlock();
            client.destroy();
        }

        const hello = protocol.helloFrame(gpa, self.session_id, self.issue_id, self.cols.load(.acquire), self.rows.load(.acquire)) catch return;
        defer gpa.free(hello);
        client.sendText(hello) catch return;
        self.connected.store(true, .release);
        self.replay(client);

        while (!self.stop_flag.load(.acquire)) {
            const msg = client.next() catch return;
            switch (msg) {
                .text => |text| {
                    defer gpa.free(text);
                    var arena = std.heap.ArenaAllocator.init(gpa);
                    defer arena.deinit();
                    const frame = protocol.parseInbound(arena.allocator(), text) orelse continue;
                    switch (frame) {
                        .input => |data| self.on_input(self.ctx, data),
                        .resync => self.replay(client),
                        .resize => |sz| if (self.on_remote_resize) |cb| cb(self.ctx, sz.cols, sz.rows),
                        .kill => {
                            self.on_kill(self.ctx);
                            // The main thread runs the end path (bye + stop);
                            // keep serving until it does.
                        },
                        else => {},
                    }
                },
                .binary => |bytes| gpa.free(bytes), // inbound binary not expected
                .closed => return,
            }
        }
    }

    /// steer.mintTicket({kind:"publisher", codingSessionId}) → dial URL, or
    /// null when the subsystem is disabled/unreachable.
    fn mintUrl(self: *Publisher) ?[]u8 {
        const gpa = self.gpa;
        const sid_json = protocol.jsonStringAlloc(gpa, self.session_id) catch return null;
        defer gpa.free(sid_json);
        const input = std.fmt.allocPrint(gpa, "{{\"kind\":\"publisher\",\"codingSessionId\":{s}}}", .{sid_json}) catch return null;
        defer gpa.free(input);
        var resp = trpc.call(gpa, self.instance, "steer.mintTicket", input, self.token, 15) catch return null;
        defer resp.deinit();
        if (!resp.ok()) return null;
        const obj = dataObject(&resp) orelse return null;
        if (trpc.objBool(obj, "disabled")) return null;
        const ticket = trpc.objString(obj, "ticket") orelse return null;
        const url = trpc.objString(obj, "url") orelse return null;
        return ws.urlWithTicket(gpa, url, ticket) catch null;
    }

    /// Replay the ring (recent output) as one binary frame — the pragmatic
    /// full-screen resync (claude's own redraws repaint for late joiners).
    fn replay(self: *Publisher, client: *ws.Client) void {
        const snap = blk: {
            self.ring_mutex.lock();
            defer self.ring_mutex.unlock();
            break :blk self.ring.snapshot(self.gpa) catch return;
        };
        defer self.gpa.free(snap);
        if (snap.len == 0) return;
        const frame = protocol.outputFrame(self.gpa, snap) catch return;
        defer self.gpa.free(frame);
        client.sendBinary(frame) catch {};
    }

    fn sendTextLocked(self: *Publisher, frame: []const u8) void {
        self.conn_mutex.lock();
        defer self.conn_mutex.unlock();
        const client = self.conn orelse return;
        client.sendText(frame) catch {};
    }

    fn sendByeLocked(self: *Publisher, outcome: []const u8) void {
        if (self.bye_sent.swap(true, .acq_rel)) return;
        if (!self.connected.load(.acquire)) return;
        const frame = protocol.byeFrame(self.gpa, outcome) catch return;
        defer self.gpa.free(frame);
        self.sendTextLocked(frame);
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
