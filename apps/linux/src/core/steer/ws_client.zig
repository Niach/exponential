//! Minimal RFC 6455 WebSocket CLIENT for the steer relay (masterplan §3).
//!
//! Scope: exactly what an outbound relay socket needs — the HTTP/1.1 Upgrade
//! handshake (Sec-WebSocket-Key → SHA1/base64 Accept validation), client→server
//! masking, text/binary/ping/pong/close frames, fragmented-message reassembly,
//! and 16/64-bit extended lengths. Plain `ws://` over TCP only: Zig 0.16's std
//! TLS client is tied to the new std.Io reader/writer plumbing and is not worth
//! the risk here — `wss://` returns `error.TlsUnsupported` with the guidance
//! that self-hosted relays are LAN-first (`ws://relay.lan:4002`); a public
//! `wss://` relay needs a TLS-terminating proxy on the LAN or a future TLS pass.
//!
//! Layering (the pure parts unit-test headlessly, no sockets):
//!   parseUrl / acceptKeyFromNonce / encodeFrame / decodeFrame / Reassembler
//!   → `Client` (libc TCP + blocking reads on the caller's thread).
//!
//! Thread model: ONE reader thread calls `next()`; any thread may call the
//! send fns (serialized by an internal mutex); `shutdownSocket()` unblocks a
//! blocked `next()` from another thread (used for teardown). For teardown that
//! must also reach a socket still MID-DIAL (before `connect` returns), owners
//! pass a `ConnectControl` — its `cancel()` shuts down the in-progress fd and
//! poisons future dials without touching any owner-side connection mutex.

const std = @import("std");
const util = @import("util.zig");

pub const Opcode = enum(u4) {
    continuation = 0x0,
    text = 0x1,
    binary = 0x2,
    close = 0x8,
    ping = 0x9,
    pong = 0xA,
    _,
};

/// Hard cap on a single (possibly reassembled) inbound message. The relay caps
/// payloads at 1 MiB (`maxPayloadLength`); anything bigger here is a bad peer.
pub const max_message_bytes: usize = 2 * 1024 * 1024;

const ws_guid = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

// ---------------------------------------------------------------------------
// URL parsing (pure)
// ---------------------------------------------------------------------------

pub const WsUrl = struct {
    tls: bool,
    host: []const u8,
    port: u16,
    /// Path + query, always starting with `/`.
    path: []const u8,
};

pub const UrlError = error{ BadUrl, TlsUnsupported };

/// Parse `ws://host[:port]/path?query`. `wss://` parses but is flagged so the
/// caller can surface a clean "TLS unsupported" error at connect time.
pub fn parseUrl(url: []const u8) UrlError!WsUrl {
    var rest: []const u8 = undefined;
    var tls = false;
    if (std.mem.startsWith(u8, url, "ws://")) {
        rest = url["ws://".len..];
    } else if (std.mem.startsWith(u8, url, "wss://")) {
        tls = true;
        rest = url["wss://".len..];
    } else return UrlError.BadUrl;

    const path_start = std.mem.indexOfAny(u8, rest, "/?") orelse rest.len;
    const authority = rest[0..path_start];
    if (authority.len == 0) return UrlError.BadUrl;
    const path = if (path_start == rest.len) "/" else rest[path_start..];

    var host = authority;
    var port: u16 = if (tls) 443 else 80;
    if (std.mem.lastIndexOfScalar(u8, authority, ':')) |colon| {
        // IPv6 literals ([::1]:80) are out of scope for v1 (LAN hostnames/IPv4).
        host = authority[0..colon];
        port = std.fmt.parseInt(u16, authority[colon + 1 ..], 10) catch return UrlError.BadUrl;
    }
    if (host.len == 0) return UrlError.BadUrl;
    return .{ .tls = tls, .host = host, .port = port, .path = path };
}

// ---------------------------------------------------------------------------
// Handshake (pure parts)
// ---------------------------------------------------------------------------

/// The expected `Sec-WebSocket-Accept` for a request nonce:
/// base64(SHA1(nonce_b64 ++ GUID)).
pub fn acceptKeyFromNonce(nonce_b64: []const u8) [28]u8 {
    var sha = std.crypto.hash.Sha1.init(.{});
    sha.update(nonce_b64);
    sha.update(ws_guid);
    var digest: [20]u8 = undefined;
    sha.final(&digest);
    var out: [28]u8 = undefined;
    _ = std.base64.standard.Encoder.encode(&out, &digest);
    return out;
}

/// Validate the server's 101 response headers (everything before the trailing
/// CRLFCRLF): status 101 + a matching Sec-WebSocket-Accept.
pub fn validateHandshake(head: []const u8, expected_accept: []const u8) bool {
    // Status line: "HTTP/1.1 101 ..."
    const line_end = std.mem.indexOf(u8, head, "\r\n") orelse return false;
    const status_line = head[0..line_end];
    if (std.mem.indexOf(u8, status_line, " 101") == null) return false;

    var it = std.mem.splitSequence(u8, head[line_end + 2 ..], "\r\n");
    while (it.next()) |line| {
        const colon = std.mem.indexOfScalar(u8, line, ':') orelse continue;
        const name = std.mem.trim(u8, line[0..colon], " \t");
        if (!std.ascii.eqlIgnoreCase(name, "sec-websocket-accept")) continue;
        const value = std.mem.trim(u8, line[colon + 1 ..], " \t");
        return std.mem.eql(u8, value, expected_accept);
    }
    return false;
}

// ---------------------------------------------------------------------------
// Frame codec (pure)
// ---------------------------------------------------------------------------

/// Encode one unfragmented frame (FIN set). `mask` must be present for
/// client→server frames per RFC 6455 (tests pass a fixed mask for determinism).
pub fn encodeFrame(gpa: std.mem.Allocator, opcode: Opcode, payload: []const u8, mask: ?[4]u8) ![]u8 {
    const header_len: usize = 2 +
        @as(usize, if (payload.len > 65535) 8 else if (payload.len > 125) 2 else 0) +
        @as(usize, if (mask != null) 4 else 0);
    const out = try gpa.alloc(u8, header_len + payload.len);
    out[0] = 0x80 | @as(u8, @intFromEnum(opcode));
    const mask_bit: u8 = if (mask != null) 0x80 else 0;
    var i: usize = 2;
    if (payload.len > 65535) {
        out[1] = mask_bit | 127;
        std.mem.writeInt(u64, out[2..10], payload.len, .big);
        i = 10;
    } else if (payload.len > 125) {
        out[1] = mask_bit | 126;
        std.mem.writeInt(u16, out[2..4], @intCast(payload.len), .big);
        i = 4;
    } else {
        out[1] = mask_bit | @as(u8, @intCast(payload.len));
    }
    if (mask) |m| {
        @memcpy(out[i .. i + 4], &m);
        i += 4;
        for (payload, 0..) |b, j| out[i + j] = b ^ m[j % 4];
    } else {
        @memcpy(out[i..], payload);
    }
    return out;
}

pub const Frame = struct {
    fin: bool,
    opcode: Opcode,
    /// View into the input buffer (unmasked in place when the peer masked).
    payload: []u8,
    /// Total bytes this frame consumed from the input buffer.
    consumed: usize,
};

pub const DecodeError = error{ FrameTooLong, BadFrame };

/// Decode one frame from the front of `buf`. Returns null when more bytes are
/// needed. A masked payload (unexpected from a server, but legal to handle) is
/// unmasked IN PLACE.
pub fn decodeFrame(buf: []u8) DecodeError!?Frame {
    if (buf.len < 2) return null;
    const fin = buf[0] & 0x80 != 0;
    const opcode: Opcode = @enumFromInt(@as(u4, @truncate(buf[0])));
    const masked = buf[1] & 0x80 != 0;
    const len7: u8 = buf[1] & 0x7F;

    var offset: usize = 2;
    var payload_len: usize = len7;
    if (len7 == 126) {
        if (buf.len < 4) return null;
        payload_len = std.mem.readInt(u16, buf[2..4], .big);
        offset = 4;
    } else if (len7 == 127) {
        if (buf.len < 10) return null;
        const raw = std.mem.readInt(u64, buf[2..10], .big);
        if (raw > max_message_bytes) return DecodeError.FrameTooLong;
        payload_len = @intCast(raw);
        offset = 10;
    }
    if (payload_len > max_message_bytes) return DecodeError.FrameTooLong;

    var mask: [4]u8 = undefined;
    if (masked) {
        if (buf.len < offset + 4) return null;
        @memcpy(&mask, buf[offset .. offset + 4]);
        offset += 4;
    }
    if (buf.len < offset + payload_len) return null;

    const payload = buf[offset .. offset + payload_len];
    if (masked) {
        for (payload, 0..) |*b, j| b.* ^= mask[j % 4];
    }
    return .{ .fin = fin, .opcode = opcode, .payload = payload, .consumed = offset + payload_len };
}

// ---------------------------------------------------------------------------
// Message reassembly (pure)
// ---------------------------------------------------------------------------

/// A complete inbound event after reassembly. Payload slices are gpa-owned —
/// the caller frees them.
pub const Event = union(enum) {
    none, // control frame handled / message still fragmented
    text: []u8,
    binary: []u8,
    ping: []u8, // reply with a pong carrying this payload
    pong,
    close: u16, // close code (1005 when absent)
};

/// Accumulates fragmented data frames into whole messages. Control frames may
/// interleave with fragments per RFC 6455 and are surfaced immediately.
pub const Reassembler = struct {
    gpa: std.mem.Allocator,
    partial: std.ArrayListUnmanaged(u8) = .empty,
    partial_opcode: Opcode = .continuation, // opcode of the in-flight message
    in_flight: bool = false,

    pub fn deinit(self: *Reassembler) void {
        self.partial.deinit(self.gpa);
    }

    pub fn onFrame(self: *Reassembler, frame: Frame) !Event {
        switch (frame.opcode) {
            .ping => return .{ .ping = try self.gpa.dupe(u8, frame.payload) },
            .pong => return .pong,
            .close => {
                const code: u16 = if (frame.payload.len >= 2)
                    std.mem.readInt(u16, frame.payload[0..2], .big)
                else
                    1005;
                return .{ .close = code };
            },
            .text, .binary => {
                if (self.in_flight) return error.BadFrame; // new message mid-reassembly
                if (frame.fin) {
                    // Fast path: whole message in one frame.
                    const copy = try self.gpa.dupe(u8, frame.payload);
                    return if (frame.opcode == .text) .{ .text = copy } else .{ .binary = copy };
                }
                self.in_flight = true;
                self.partial_opcode = frame.opcode;
                try self.appendPartial(frame.payload);
                return .none;
            },
            .continuation => {
                if (!self.in_flight) return error.BadFrame; // nothing in flight
                try self.appendPartial(frame.payload);
                if (!frame.fin) return .none;
                return self.finish();
            },
            _ => return .none, // reserved opcode — ignore
        }
    }

    fn appendPartial(self: *Reassembler, bytes: []const u8) !void {
        if (self.partial.items.len + bytes.len > max_message_bytes) return error.FrameTooLong;
        try self.partial.appendSlice(self.gpa, bytes);
    }

    fn finish(self: *Reassembler) !Event {
        const opcode = self.partial_opcode;
        const whole = try self.partial.toOwnedSlice(self.gpa);
        self.in_flight = false;
        self.partial_opcode = .continuation;
        return if (opcode == .text) .{ .text = whole } else .{ .binary = whole };
    }
};

// ---------------------------------------------------------------------------
// TCP socket layer (libc; Linux-only, like the rest of the app)
// ---------------------------------------------------------------------------

const c = struct {
    // glibc addrinfo layout (Linux).
    const addrinfo = extern struct {
        flags: c_int,
        family: c_int,
        socktype: c_int,
        protocol: c_int,
        addrlen: c_uint,
        addr: ?*anyopaque,
        canonname: ?[*:0]u8,
        next: ?*addrinfo,
    };
    const timeval = extern struct { sec: c_long, usec: c_long };
    const pollfd = extern struct { fd: c_int, events: c_short, revents: c_short };

    const AF_UNSPEC: c_int = 0;
    const SOCK_STREAM: c_int = 1;
    const SOCK_NONBLOCK: c_int = 0o4000;
    const IPPROTO_TCP: c_int = 6;
    const TCP_NODELAY: c_int = 1;
    const SOL_SOCKET: c_int = 1;
    const SO_ERROR: c_int = 4;
    const SO_RCVTIMEO: c_int = 20;
    const SO_SNDTIMEO: c_int = 21;
    const MSG_NOSIGNAL: c_int = 0x4000;
    const MSG_DONTWAIT: c_int = 0x40;
    const SHUT_RDWR: c_int = 2;
    const POLLOUT: c_short = 0x4;
    const F_GETFL: c_int = 3;
    const F_SETFL: c_int = 4;
    const O_NONBLOCK: c_int = 0o4000;

    extern "c" fn socket(domain: c_int, sock_type: c_int, protocol: c_int) c_int;
    extern "c" fn connect(fd: c_int, addr: *const anyopaque, len: c_uint) c_int;
    extern "c" fn send(fd: c_int, buf: [*]const u8, len: usize, flags: c_int) isize;
    extern "c" fn recv(fd: c_int, buf: [*]u8, len: usize, flags: c_int) isize;
    extern "c" fn close(fd: c_int) c_int;
    extern "c" fn shutdown(fd: c_int, how: c_int) c_int;
    extern "c" fn setsockopt(fd: c_int, level: c_int, optname: c_int, optval: *const anyopaque, optlen: c_uint) c_int;
    extern "c" fn getsockopt(fd: c_int, level: c_int, optname: c_int, optval: *anyopaque, optlen: *c_uint) c_int;
    extern "c" fn getaddrinfo(node: ?[*:0]const u8, service: ?[*:0]const u8, hints: ?*const addrinfo, res: *?*addrinfo) c_int;
    extern "c" fn freeaddrinfo(res: *addrinfo) void;
    extern "c" fn poll(fds: [*]pollfd, nfds: c_ulong, timeout: c_int) c_int;
    extern "c" fn fcntl(fd: c_int, cmd: c_int, arg: c_int) c_int;
    extern "c" fn __errno_location() *c_int;
};

fn errnoIs(codes: []const c_int) bool {
    const e = c.__errno_location().*;
    for (codes) |code| if (e == code) return true;
    return false;
}

const EAGAIN: c_int = 11;
const EINTR: c_int = 4;
const EINPROGRESS: c_int = 115;

/// Bounds every BLOCKING send (SO_SNDTIMEO): a hung peer stalls a send for at
/// most this long instead of forever.
pub const send_timeout_s: u32 = 5;
/// Total budget for the non-blocking TCP connect (SYN → established).
const connect_timeout_ms: c_int = 5000;
/// Poll slice during connect so a `ConnectControl.cancel` is honored promptly.
const connect_poll_slice_ms: c_int = 250;

pub const ConnectError = error{ TlsUnsupported, BadUrl, ResolveFailed, ConnectFailed, HandshakeFailed, OutOfMemory };
pub const ReadError = error{ Disconnected, Timeout, BadFrame, FrameTooLong, OutOfMemory };
pub const SendError = error{ Disconnected, WouldBlock, OutOfMemory };

/// Cross-thread cancel/teardown handle for one client socket. It lives with
/// the OWNER (publisher / control channel), not the `Client`, so teardown can
/// reach the fd in EVERY phase — including mid-dial, before `connect` returns
/// a `Client` — without needing any owner-side connection mutex (which a
/// sender might hold). The internal mutex only guards fd bookkeeping
/// (register/clear/shutdown), never a blocking syscall, so `cancel` is always
/// prompt. Note: `getaddrinfo` remains technically unbounded (no portable
/// cancel/timeout) — accepted; DNS on the LAN answers or fails fast.
pub const ConnectControl = struct {
    mutex: util.Mutex = .{},
    fd: c_int = -1,
    cancelled: bool = false,

    /// Cancel the current AND all future dials: poisons `register` and shuts
    /// down the live fd so blocked connect-poll/handshake/recv/send calls on
    /// the socket thread return promptly. Idempotent; any thread.
    pub fn cancel(self: *ConnectControl) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        self.cancelled = true;
        if (self.fd >= 0) _ = c.shutdown(self.fd, c.SHUT_RDWR);
    }

    pub fn isCancelled(self: *ConnectControl) bool {
        self.mutex.lock();
        defer self.mutex.unlock();
        return self.cancelled;
    }

    /// Adopt a freshly created socket so `cancel` can reach it. Returns false
    /// when already cancelled (the caller closes the fd and aborts the dial).
    fn register(self: *ConnectControl, fd: c_int) bool {
        self.mutex.lock();
        defer self.mutex.unlock();
        if (self.cancelled) return false;
        self.fd = fd;
        return true;
    }

    /// Forget the fd right before the owner closes it, so a late `cancel`
    /// can never shut down a recycled descriptor.
    fn clear(self: *ConnectControl) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        self.fd = -1;
    }
};

pub const Message = union(enum) {
    text: []u8,
    binary: []u8,
    closed: u16,
};

pub const Client = struct {
    gpa: std.mem.Allocator,
    fd: c_int,
    read_buf: std.ArrayListUnmanaged(u8) = .empty,
    reassembler: Reassembler,
    write_mutex: util.Mutex = .{},
    /// Consecutive recv timeouts with no traffic; each fires a keepalive ping.
    idle_strikes: u8 = 0,
    /// Owner's cancel handle; cleared before the fd is closed.
    control: ?*ConnectControl = null,

    /// Dial `ws://…` and complete the upgrade handshake. `recv_timeout_s` sets
    /// the blocking-read slice used for keepalive pings (0 = block forever).
    /// `control` (optional) lets the owner cancel the dial/handshake from
    /// another thread and shut the socket down without holding its own locks.
    pub fn connect(gpa: std.mem.Allocator, url: []const u8, recv_timeout_s: u32, control: ?*ConnectControl) ConnectError!*Client {
        const parsed = parseUrl(url) catch |e| switch (e) {
            UrlError.TlsUnsupported => return ConnectError.TlsUnsupported,
            UrlError.BadUrl => return ConnectError.BadUrl,
        };
        if (parsed.tls) return ConnectError.TlsUnsupported;

        const fd = try dial(gpa, parsed.host, parsed.port, control);
        errdefer {
            if (control) |ctl| ctl.clear();
            _ = c.close(fd);
        }

        // Keystrokes are latency-sensitive.
        const one: c_int = 1;
        _ = c.setsockopt(fd, c.IPPROTO_TCP, c.TCP_NODELAY, &one, @sizeOf(c_int));
        if (recv_timeout_s > 0) {
            const tv = c.timeval{ .sec = recv_timeout_s, .usec = 0 };
            _ = c.setsockopt(fd, c.SOL_SOCKET, c.SO_RCVTIMEO, &tv, @sizeOf(c.timeval));
        }
        // Bound BLOCKING sends (handshake/hello/replay/bye): a hung peer whose
        // buffer is full times the send out (EAGAIN) instead of wedging.
        const send_tv = c.timeval{ .sec = send_timeout_s, .usec = 0 };
        _ = c.setsockopt(fd, c.SOL_SOCKET, c.SO_SNDTIMEO, &send_tv, @sizeOf(c.timeval));

        const self = try gpa.create(Client);
        errdefer gpa.destroy(self);
        self.* = .{ .gpa = gpa, .fd = fd, .reassembler = .{ .gpa = gpa }, .control = control };
        try self.handshake(parsed);
        return self;
    }

    pub fn destroy(self: *Client) void {
        const gpa = self.gpa;
        if (self.control) |ctl| ctl.clear();
        _ = c.close(self.fd);
        self.read_buf.deinit(gpa);
        self.reassembler.deinit();
        gpa.destroy(self);
    }

    /// Unblock a `next()` blocked in recv from another thread (teardown). The
    /// blocked call returns `error.Disconnected`; call `destroy` after joining.
    pub fn shutdownSocket(self: *Client) void {
        _ = c.shutdown(self.fd, c.SHUT_RDWR);
    }

    /// Bounded, cancellable TCP dial: non-blocking connect + poll (~5s per
    /// candidate address, in slices, honoring `control.cancel` between
    /// slices) — never the kernel's ~2min SYN-retry default. `getaddrinfo`
    /// itself stays blocking/uncancellable (accepted; see ConnectControl).
    fn dial(gpa: std.mem.Allocator, host: []const u8, port: u16, control: ?*ConnectControl) ConnectError!c_int {
        const host_z = gpa.dupeZ(u8, host) catch return ConnectError.OutOfMemory;
        defer gpa.free(host_z);
        var port_buf: [8]u8 = undefined;
        const port_z = std.fmt.bufPrintZ(&port_buf, "{d}", .{port}) catch unreachable;

        var hints = std.mem.zeroes(c.addrinfo);
        hints.family = c.AF_UNSPEC;
        hints.socktype = c.SOCK_STREAM;
        var res: ?*c.addrinfo = null;
        if (c.getaddrinfo(host_z.ptr, port_z.ptr, &hints, &res) != 0 or res == null)
            return ConnectError.ResolveFailed;
        defer c.freeaddrinfo(res.?);

        var ai: ?*c.addrinfo = res;
        while (ai) |info| : (ai = info.next) {
            if (control) |ctl| if (ctl.isCancelled()) return ConnectError.ConnectFailed;
            const addr = info.addr orelse continue;
            const fd = c.socket(info.family, info.socktype | c.SOCK_NONBLOCK, info.protocol);
            if (fd < 0) continue;
            if (control) |ctl| {
                if (!ctl.register(fd)) {
                    _ = c.close(fd);
                    return ConnectError.ConnectFailed;
                }
            }
            if (connectBounded(fd, addr, info.addrlen, control)) {
                // Back to blocking for the normal recv/send paths.
                const fl = c.fcntl(fd, c.F_GETFL, 0);
                if (fl >= 0) _ = c.fcntl(fd, c.F_SETFL, fl & ~c.O_NONBLOCK);
                return fd;
            }
            if (control) |ctl| ctl.clear();
            _ = c.close(fd);
        }
        return ConnectError.ConnectFailed;
    }

    /// Non-blocking connect, polled for writability in short slices so a
    /// cancel is honored within one slice. True only on a fully established
    /// connection (SO_ERROR == 0).
    fn connectBounded(fd: c_int, addr: *const anyopaque, addrlen: c_uint, control: ?*ConnectControl) bool {
        if (c.connect(fd, addr, addrlen) == 0) return true;
        if (!errnoIs(&.{ EINPROGRESS, EINTR })) return false;
        var waited: c_int = 0;
        while (waited < connect_timeout_ms) : (waited += connect_poll_slice_ms) {
            if (control) |ctl| if (ctl.isCancelled()) return false;
            var pfd = [1]c.pollfd{.{ .fd = fd, .events = c.POLLOUT, .revents = 0 }};
            const n = c.poll(&pfd, 1, connect_poll_slice_ms);
            if (n < 0) {
                if (errnoIs(&.{EINTR})) continue;
                return false;
            }
            if (n == 0) continue; // slice elapsed — re-check cancel
            var so_err: c_int = 0;
            var len: c_uint = @sizeOf(c_int);
            if (c.getsockopt(fd, c.SOL_SOCKET, c.SO_ERROR, &so_err, &len) != 0) return false;
            return so_err == 0;
        }
        return false; // timed out
    }

    fn handshake(self: *Client, url: WsUrl) ConnectError!void {
        const gpa = self.gpa;

        var nonce_raw: [16]u8 = undefined;
        util.fillRandom(&nonce_raw);
        var nonce_b64: [24]u8 = undefined;
        _ = std.base64.standard.Encoder.encode(&nonce_b64, &nonce_raw);
        const expected = acceptKeyFromNonce(&nonce_b64);

        const request = std.fmt.allocPrint(gpa, "GET {s} HTTP/1.1\r\n" ++
            "Host: {s}:{d}\r\n" ++
            "Upgrade: websocket\r\n" ++
            "Connection: Upgrade\r\n" ++
            "Sec-WebSocket-Key: {s}\r\n" ++
            "Sec-WebSocket-Version: 13\r\n" ++
            "\r\n", .{ url.path, url.host, url.port, nonce_b64 }) catch return ConnectError.OutOfMemory;
        defer gpa.free(request);
        self.sendAll(request, false) catch return ConnectError.HandshakeFailed;

        // Read until the header terminator; anything after it is frame data.
        var head: std.ArrayListUnmanaged(u8) = .empty;
        defer head.deinit(gpa);
        var tmp: [2048]u8 = undefined;
        while (std.mem.indexOf(u8, head.items, "\r\n\r\n") == null) {
            if (head.items.len > 16 * 1024) return ConnectError.HandshakeFailed;
            const n = c.recv(self.fd, &tmp, tmp.len, 0);
            if (n <= 0) return ConnectError.HandshakeFailed;
            head.appendSlice(gpa, tmp[0..@intCast(n)]) catch return ConnectError.OutOfMemory;
        }
        const end = std.mem.indexOf(u8, head.items, "\r\n\r\n").?;
        if (!validateHandshake(head.items[0..end], &expected)) return ConnectError.HandshakeFailed;
        // Preserve any frame bytes that arrived with the response.
        const leftover = head.items[end + 4 ..];
        if (leftover.len > 0) {
            self.read_buf.appendSlice(gpa, leftover) catch return ConnectError.OutOfMemory;
        }
    }

    // --- sending -----------------------------------------------------------

    pub fn sendText(self: *Client, text: []const u8) SendError!void {
        try self.sendFrame(.text, text, false);
    }

    /// Best-effort text send for frames issued OFF the socket thread (resize,
    /// claim/release, bye): never blocks — a full socket buffer drops the
    /// frame (`error.WouldBlock`) instead of stalling the calling thread.
    pub fn sendTextNoWait(self: *Client, text: []const u8) SendError!void {
        try self.sendFrame(.text, text, true);
    }

    pub fn sendBinary(self: *Client, bytes: []const u8) SendError!void {
        try self.sendFrame(.binary, bytes, false);
    }

    /// Best-effort binary send for the hot output path: if the socket buffer is
    /// full (slow relay/viewer), the frame is DROPPED (`error.WouldBlock`)
    /// rather than stalling the PTY reader — a viewer recovers via `resync`.
    /// A buffer that fills MID-frame kills the connection (see `sendAll`).
    pub fn sendBinaryNoWait(self: *Client, bytes: []const u8) SendError!void {
        try self.sendFrame(.binary, bytes, true);
    }

    pub fn sendClose(self: *Client, code: u16) void {
        var payload: [2]u8 = undefined;
        std.mem.writeInt(u16, &payload, code, .big);
        self.sendFrame(.close, &payload, false) catch {};
    }

    fn sendPong(self: *Client, payload: []const u8) void {
        self.sendFrame(.pong, payload, false) catch {};
    }

    fn sendPing(self: *Client) void {
        self.sendFrame(.ping, "ka", false) catch {};
    }

    fn sendFrame(self: *Client, opcode: Opcode, payload: []const u8, no_wait: bool) SendError!void {
        var mask: [4]u8 = undefined;
        util.fillRandom(&mask);
        const frame = encodeFrame(self.gpa, opcode, payload, mask) catch return SendError.OutOfMemory;
        defer self.gpa.free(frame);
        try self.sendAll(frame, no_wait);
    }

    /// Serialized whole-frame write. In `no_wait` mode EVERY write is
    /// MSG_DONTWAIT: WouldBlock before the first byte drops the frame cleanly
    /// (`error.WouldBlock`); WouldBlock AFTER a partial write cannot be
    /// resumed later without corrupting the frame stream, so the connection
    /// is shut down and reported dead (`error.Disconnected` — callers already
    /// treat that as disconnect, and the shutdown unblocks the reader thread
    /// so teardown proceeds). Blocking sends are bounded by SO_SNDTIMEO (set
    /// at connect); a timeout there is the same mid-frame stall ⇒ same kill.
    fn sendAll(self: *Client, bytes: []const u8, no_wait: bool) SendError!void {
        self.write_mutex.lock();
        defer self.write_mutex.unlock();
        var sent: usize = 0;
        while (sent < bytes.len) {
            const flags: c_int = if (no_wait) c.MSG_NOSIGNAL | c.MSG_DONTWAIT else c.MSG_NOSIGNAL;
            const n = c.send(self.fd, bytes.ptr + sent, bytes.len - sent, flags);
            if (n < 0) {
                if (errnoIs(&.{EINTR})) continue;
                if (errnoIs(&.{EAGAIN})) {
                    if (no_wait and sent == 0) return SendError.WouldBlock;
                    // Partial frame stuck (or a blocking send timed out): the
                    // stream is unrecoverable mid-frame — kill the socket so
                    // every blocked/future call sees a dead connection
                    // instead of a wedged one.
                    self.shutdownSocket();
                    return SendError.Disconnected;
                }
                return SendError.Disconnected;
            }
            sent += @intCast(n);
        }
    }

    // --- receiving ----------------------------------------------------------

    /// Block until the next complete text/binary message or the close of the
    /// connection. Handles ping (auto-pong), pong, and interleaved fragments
    /// internally. Payloads are gpa-owned — the caller frees them.
    pub fn next(self: *Client) ReadError!Message {
        while (true) {
            // Drain complete frames already buffered.
            while (true) {
                const maybe = decodeFrame(self.read_buf.items) catch |e| return switch (e) {
                    DecodeError.FrameTooLong => ReadError.FrameTooLong,
                    DecodeError.BadFrame => ReadError.BadFrame,
                };
                const frame = maybe orelse break;
                const event = self.reassembler.onFrame(frame) catch |e| switch (e) {
                    error.BadFrame => return ReadError.BadFrame,
                    error.FrameTooLong => return ReadError.FrameTooLong,
                    error.OutOfMemory => return ReadError.OutOfMemory,
                };
                self.consume(frame.consumed);
                switch (event) {
                    .none => {},
                    .pong => {},
                    .ping => |payload| {
                        self.sendPong(payload);
                        self.gpa.free(payload);
                    },
                    .text => |t| return .{ .text = t },
                    .binary => |b| return .{ .binary = b },
                    .close => |code| {
                        self.sendClose(code);
                        return .{ .closed = code };
                    },
                }
            }
            // Need more bytes.
            var tmp: [8192]u8 = undefined;
            const n = c.recv(self.fd, &tmp, tmp.len, 0);
            if (n == 0) return ReadError.Disconnected;
            if (n < 0) {
                if (errnoIs(&.{EINTR})) continue;
                if (errnoIs(&.{EAGAIN})) {
                    // recv timeout slice: fire a keepalive ping; a dead peer
                    // yields three silent slices → give up and reconnect.
                    self.idle_strikes += 1;
                    if (self.idle_strikes >= 3) return ReadError.Timeout;
                    self.sendPing();
                    continue;
                }
                return ReadError.Disconnected;
            }
            self.idle_strikes = 0;
            self.read_buf.appendSlice(self.gpa, tmp[0..@intCast(n)]) catch return ReadError.OutOfMemory;
        }
    }

    fn consume(self: *Client, n: usize) void {
        const remaining = self.read_buf.items.len - n;
        std.mem.copyForwards(u8, self.read_buf.items[0..remaining], self.read_buf.items[n..]);
        self.read_buf.shrinkRetainingCapacity(remaining);
    }
};

/// `<url>?ticket=<ticket>` (or `&ticket=` when the url already has a query) —
/// the relay reads the ticket from the query string. Caller frees.
pub fn urlWithTicket(gpa: std.mem.Allocator, url: []const u8, ticket: []const u8) ![]u8 {
    const sep: u8 = if (std.mem.indexOfScalar(u8, url, '?') != null) '&' else '?';
    return std.fmt.allocPrint(gpa, "{s}{c}ticket={s}", .{ url, sep, ticket });
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

const testing = std.testing;

test "parseUrl handles ports, paths and wss" {
    const plain = try parseUrl("ws://relay.lan:4002/ws");
    try testing.expect(!plain.tls);
    try testing.expectEqualStrings("relay.lan", plain.host);
    try testing.expectEqual(@as(u16, 4002), plain.port);
    try testing.expectEqualStrings("/ws", plain.path);

    const defaulted = try parseUrl("ws://relay.lan");
    try testing.expectEqual(@as(u16, 80), defaulted.port);
    try testing.expectEqualStrings("/", defaulted.path);

    const with_query = try parseUrl("ws://h/ws?ticket=abc.def");
    try testing.expectEqualStrings("/ws?ticket=abc.def", with_query.path);

    const secure = try parseUrl("wss://steer.exponential.at/ws");
    try testing.expect(secure.tls);
    try testing.expectEqual(@as(u16, 443), secure.port);

    try testing.expectError(UrlError.BadUrl, parseUrl("http://nope"));
    try testing.expectError(UrlError.BadUrl, parseUrl("ws://"));
    try testing.expectError(UrlError.BadUrl, parseUrl("ws://host:notaport/ws"));
}

test "acceptKeyFromNonce matches the RFC 6455 example" {
    // RFC 6455 §1.3: key "dGhlIHNhbXBsZSBub25jZQ==" → accept "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
    const accept = acceptKeyFromNonce("dGhlIHNhbXBsZSBub25jZQ==");
    try testing.expectEqualStrings("s3pPLMBiTxaQ9kYGzzhZRbK+xOo=", &accept);
}

test "validateHandshake checks status + accept header case-insensitively" {
    const accept = acceptKeyFromNonce("dGhlIHNhbXBsZSBub25jZQ==");
    const good = "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nSEC-WEBSOCKET-ACCEPT:  s3pPLMBiTxaQ9kYGzzhZRbK+xOo=";
    try testing.expect(validateHandshake(good, &accept));
    const wrong_status = "HTTP/1.1 401 Unauthorized\r\nSec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=";
    try testing.expect(!validateHandshake(wrong_status, &accept));
    const wrong_accept = "HTTP/1.1 101 Switching Protocols\r\nSec-WebSocket-Accept: bogus=";
    try testing.expect(!validateHandshake(wrong_accept, &accept));
    const missing = "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket";
    try testing.expect(!validateHandshake(missing, &accept));
}

test "encodeFrame: small masked text frame" {
    const gpa = testing.allocator;
    const frame = try encodeFrame(gpa, .text, "hi", .{ 0x01, 0x02, 0x03, 0x04 });
    defer gpa.free(frame);
    try testing.expectEqualSlices(u8, &.{
        0x81, // FIN + text
        0x82, // masked + len 2
        0x01,       0x02,       0x03, 0x04, // mask
        'h' ^ 0x01, 'i' ^ 0x02,
    }, frame);
}

test "encodeFrame: 16-bit and 64-bit extended lengths round-trip decode" {
    const gpa = testing.allocator;

    const mid = try gpa.alloc(u8, 300);
    defer gpa.free(mid);
    @memset(mid, 'x');
    const mid_frame = try encodeFrame(gpa, .binary, mid, null);
    defer gpa.free(mid_frame);
    try testing.expectEqual(@as(u8, 126), mid_frame[1] & 0x7F);
    const mid_dec = (try decodeFrame(mid_frame)).?;
    try testing.expectEqual(@as(usize, 300), mid_dec.payload.len);
    try testing.expectEqual(mid_frame.len, mid_dec.consumed);

    const big = try gpa.alloc(u8, 70_000);
    defer gpa.free(big);
    @memset(big, 'y');
    const big_frame = try encodeFrame(gpa, .binary, big, null);
    defer gpa.free(big_frame);
    try testing.expectEqual(@as(u8, 127), big_frame[1] & 0x7F);
    const big_dec = (try decodeFrame(big_frame)).?;
    try testing.expectEqual(@as(usize, 70_000), big_dec.payload.len);
    try testing.expectEqual(Opcode.binary, big_dec.opcode);
}

test "decodeFrame: unmasks a masked frame in place and reports partials" {
    const gpa = testing.allocator;
    const frame = try encodeFrame(gpa, .text, "hello", .{ 0xAA, 0xBB, 0xCC, 0xDD });
    defer gpa.free(frame);

    // Any prefix short of the full frame → null (need more bytes).
    var i: usize = 0;
    while (i < frame.len) : (i += 1) {
        const partial = try gpa.dupe(u8, frame[0..i]);
        defer gpa.free(partial);
        try testing.expect((try decodeFrame(partial)) == null);
    }

    const whole = try gpa.dupe(u8, frame);
    defer gpa.free(whole);
    const dec = (try decodeFrame(whole)).?;
    try testing.expect(dec.fin);
    try testing.expectEqual(Opcode.text, dec.opcode);
    try testing.expectEqualStrings("hello", dec.payload);
}

test "decodeFrame rejects oversized declared lengths" {
    var buf: [10]u8 = .{ 0x82, 127, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF };
    try testing.expectError(DecodeError.FrameTooLong, decodeFrame(&buf));
}

test "reassembler: fragmented text message across three frames" {
    const gpa = testing.allocator;
    var r = Reassembler{ .gpa = gpa };
    defer r.deinit();

    var f1 = "Hel".*;
    var f2 = "lo ".*;
    var f3 = "ws".*;
    try testing.expectEqual(Event.none, try r.onFrame(.{ .fin = false, .opcode = .text, .payload = &f1, .consumed = 0 }));
    try testing.expectEqual(Event.none, try r.onFrame(.{ .fin = false, .opcode = .continuation, .payload = &f2, .consumed = 0 }));
    const done = try r.onFrame(.{ .fin = true, .opcode = .continuation, .payload = &f3, .consumed = 0 });
    try testing.expectEqualStrings("Hello ws", done.text);
    gpa.free(done.text);
}

test "reassembler: control frames interleave with fragments" {
    const gpa = testing.allocator;
    var r = Reassembler{ .gpa = gpa };
    defer r.deinit();

    var f1 = "par".*;
    var ping_payload = "p".*;
    var f2 = "t".*;
    try testing.expectEqual(Event.none, try r.onFrame(.{ .fin = false, .opcode = .binary, .payload = &f1, .consumed = 0 }));
    const ping = try r.onFrame(.{ .fin = true, .opcode = .ping, .payload = &ping_payload, .consumed = 0 });
    try testing.expectEqualStrings("p", ping.ping);
    gpa.free(ping.ping);
    const done = try r.onFrame(.{ .fin = true, .opcode = .continuation, .payload = &f2, .consumed = 0 });
    try testing.expectEqualStrings("part", done.binary);
    gpa.free(done.binary);
}

test "reassembler: close frame carries the code" {
    const gpa = testing.allocator;
    var r = Reassembler{ .gpa = gpa };
    defer r.deinit();
    var payload: [2]u8 = undefined;
    std.mem.writeInt(u16, &payload, 4002, .big);
    const ev = try r.onFrame(.{ .fin = true, .opcode = .close, .payload = &payload, .consumed = 0 });
    try testing.expectEqual(@as(u16, 4002), ev.close);
    var empty: [0]u8 = .{};
    const ev2 = try r.onFrame(.{ .fin = true, .opcode = .close, .payload = &empty, .consumed = 0 });
    try testing.expectEqual(@as(u16, 1005), ev2.close);
}

test "urlWithTicket appends with the right separator" {
    const gpa = testing.allocator;
    const bare = try urlWithTicket(gpa, "ws://relay.lan:4002/ws", "abc.def");
    defer gpa.free(bare);
    try testing.expectEqualStrings("ws://relay.lan:4002/ws?ticket=abc.def", bare);
    const with_q = try urlWithTicket(gpa, "ws://relay.lan/ws?v=1", "t");
    defer gpa.free(with_q);
    try testing.expectEqualStrings("ws://relay.lan/ws?v=1&ticket=t", with_q);
}

// --- socket-level tests (headless: AF_UNIX socketpair, no network) ----------

const test_c = struct {
    extern "c" fn socketpair(domain: c_int, socket_type: c_int, protocol: c_int, sv: *[2]c_int) c_int;
    const AF_UNIX: c_int = 1;
    const SO_SNDBUF: c_int = 7;
};

/// A Client wrapped around one end of a socketpair with a tiny send buffer.
/// The peer end never reads, so the buffer fills deterministically.
const TestConn = struct {
    client: Client,
    peer: c_int,

    fn init(gpa: std.mem.Allocator) !TestConn {
        var fds: [2]c_int = undefined;
        try testing.expect(test_c.socketpair(test_c.AF_UNIX, c.SOCK_STREAM, 0, &fds) == 0);
        const sndbuf: c_int = 4096;
        _ = c.setsockopt(fds[0], c.SOL_SOCKET, test_c.SO_SNDBUF, &sndbuf, @sizeOf(c_int));
        return .{
            .client = .{ .gpa = gpa, .fd = fds[0], .reassembler = .{ .gpa = gpa } },
            .peer = fds[1],
        };
    }

    fn deinit(self: *TestConn) void {
        _ = c.close(self.client.fd);
        _ = c.close(self.peer);
        self.client.read_buf.deinit(self.client.gpa);
        self.client.reassembler.deinit();
    }
};

test "sendAll no_wait: full buffer at frame start drops cleanly, connection stays live" {
    var conn = try TestConn.init(testing.allocator);
    defer conn.deinit();

    // Fill the send buffer to the brim with raw non-blocking writes.
    var junk: [2048]u8 = @splat('x');
    while (true) {
        const n = c.send(conn.client.fd, &junk, junk.len, c.MSG_NOSIGNAL | c.MSG_DONTWAIT);
        if (n < 0) break; // EAGAIN — full
    }

    // First byte of the frame can't go out ⇒ clean drop, NOT a dead socket.
    try testing.expectError(SendError.WouldBlock, conn.client.sendBinaryNoWait("hello"));

    // Drain the peer; the connection must still be usable afterwards.
    var sink: [8192]u8 = undefined;
    while (true) {
        const n = c.recv(conn.peer, &sink, sink.len, c.MSG_DONTWAIT);
        if (n <= 0) break;
    }
    try conn.client.sendBinaryNoWait("hi");
}

test "sendAll no_wait: WouldBlock mid-frame kills the connection instead of wedging" {
    const gpa = testing.allocator;
    var conn = try TestConn.init(gpa);
    defer conn.deinit();

    // A frame far larger than the send buffer: the first MSG_DONTWAIT send
    // takes a partial chunk, the next hits EAGAIN with sent > 0 — the frame
    // stream is unrecoverable, so the connection must be reported dead.
    const big = try gpa.alloc(u8, 512 * 1024);
    defer gpa.free(big);
    @memset(big, 'y');
    try testing.expectError(SendError.Disconnected, conn.client.sendBinaryNoWait(big));

    // The socket was shut down: further sends fail fast (no EAGAIN limbo)…
    try testing.expectError(SendError.Disconnected, conn.client.sendBinaryNoWait("more"));
    // …and a reader blocked in recv on the client fd unblocks with EOF, so
    // teardown (thread join) is reachable.
    var tmp: [16]u8 = undefined;
    try testing.expectEqual(@as(isize, 0), c.recv(conn.client.fd, &tmp, tmp.len, 0));
}

test "ConnectControl: cancel shuts down the registered fd and poisons later registers" {
    var fds: [2]c_int = undefined;
    try testing.expect(test_c.socketpair(test_c.AF_UNIX, c.SOCK_STREAM, 0, &fds) == 0);
    defer _ = c.close(fds[0]);
    defer _ = c.close(fds[1]);

    var ctl = ConnectControl{};
    try testing.expect(!ctl.isCancelled());
    try testing.expect(ctl.register(fds[0]));
    ctl.cancel();
    try testing.expect(ctl.isCancelled());

    // The registered fd got SHUT_RDWR: its recv sees EOF immediately (a
    // blocked handshake/read would unblock the same way).
    var tmp: [4]u8 = undefined;
    try testing.expectEqual(@as(isize, 0), c.recv(fds[0], &tmp, tmp.len, 0));

    // Cancelled ⇒ a later dial may not adopt a new socket.
    try testing.expect(!ctl.register(fds[1]));
    // clear/cancel with no fd registered are safe no-ops.
    ctl.clear();
    ctl.cancel();
}

test "Client.connect aborts promptly when the control is already cancelled" {
    var ctl = ConnectControl{};
    ctl.cancel();
    try testing.expectError(
        ConnectError.ConnectFailed,
        Client.connect(testing.allocator, "ws://127.0.0.1:9/ws", 1, &ctl),
    );
}

test "Client.connect: refused loopback connect fails fast through the bounded dial" {
    // Port 9 (discard) is closed on the test VM; the refusal must come back
    // through the non-blocking connect + poll path well under the 5s budget.
    try testing.expectError(
        ConnectError.ConnectFailed,
        Client.connect(testing.allocator, "ws://127.0.0.1:9/ws", 1, null),
    );
}
