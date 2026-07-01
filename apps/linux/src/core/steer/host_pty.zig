//! Host-owned PTY for steer-enabled coding sessions (masterplan §3.3).
//!
//! When live steering is on, the launcher spawns `claude` (via `.exp-run.sh`)
//! on a PTY master WE own instead of letting libghostty exec it internally —
//! that is the seam that makes the tee possible: the reader thread hands every
//! output chunk to the caller, which forwards it to BOTH the local ghostty
//! surface (manual-IO mode, `ghostty_surface_process_output`) AND the relay
//! publisher as binary `0x01` frames. Remote steerer keystrokes are written to
//! the SAME master fd local keys use, so `claude` sees one input stream.
//!
//! The child is spawned with `forkpty(3)` (child = session leader on the PTY
//! slave), so signalling the process GROUP reaches both the `sh` wrapper and
//! `claude`. All exec inputs (argv/envp/cwd C-strings) are prepared BEFORE the
//! fork — the child performs no allocations (fork in a threaded process).

const std = @import("std");
const util = @import("util.zig");

const winsize = extern struct { row: u16, col: u16, xpixel: u16 = 0, ypixel: u16 = 0 };

extern "c" fn forkpty(amaster: *c_int, name: ?[*]u8, termp: ?*const anyopaque, winp: ?*const winsize) c_int;
extern "c" fn execve(path: [*:0]const u8, argv: [*:null]const ?[*:0]const u8, envp: [*:null]const ?[*:0]const u8) c_int;
extern "c" fn chdir(path: [*:0]const u8) c_int;
extern "c" fn _exit(code: c_int) noreturn;
extern "c" fn read(fd: c_int, buf: [*]u8, len: usize) isize;
extern "c" fn write(fd: c_int, buf: [*]const u8, len: usize) isize;
extern "c" fn close(fd: c_int) c_int;
extern "c" fn ioctl(fd: c_int, request: c_ulong, ...) c_int;
extern "c" fn killpg(pgrp: c_int, sig: c_int) c_int;
extern "c" fn waitpid(pid: c_int, status: ?*c_int, options: c_int) c_int;
extern "c" var environ: [*:null]const ?[*:0]const u8;

const TIOCSWINSZ: c_ulong = 0x5414; // Linux

pub const SIGHUP: c_int = 1;
pub const SIGKILL: c_int = 9;
pub const SIGTERM: c_int = 15;

/// PTY output chunk — fired on the READER thread.
pub const OutputFn = *const fn (ctx: ?*anyopaque, bytes: []const u8) void;
/// Child exited (fires exactly once, on the reader thread, after all output).
pub const ExitFn = *const fn (ctx: ?*anyopaque, exit_code: i32) void;

pub const Options = struct {
    /// argv[0] must be an absolute path (execve, no PATH search).
    argv: []const []const u8,
    cwd: []const u8,
    /// TERM for the child. xterm-256color renders identically in ghostty's VT
    /// and a remote xterm.js — the safe common denominator for the tee.
    term: []const u8 = "xterm-256color",
    cols: u16 = 80,
    rows: u16 = 24,
    on_output: OutputFn,
    on_exit: ExitFn,
    ctx: ?*anyopaque = null,
};

pub const HostPty = struct {
    gpa: std.mem.Allocator,
    master_fd: c_int,
    child_pid: c_int,
    on_output: OutputFn,
    on_exit: ExitFn,
    ctx: ?*anyopaque,
    write_mutex: util.Mutex = .{},
    exited: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    thread: ?std.Thread = null,

    /// Fork the child on a fresh PTY and start the reader thread.
    pub fn spawn(gpa: std.mem.Allocator, opts: Options) !*HostPty {
        if (opts.argv.len == 0) return error.BadArgv;

        // Prepare every pointer the child needs BEFORE forking.
        var arena = std.heap.ArenaAllocator.init(gpa);
        defer arena.deinit();
        const a = arena.allocator();

        const argv_z = try a.allocSentinel(?[*:0]const u8, opts.argv.len, null);
        for (opts.argv, 0..) |arg, i| argv_z[i] = (try a.dupeZ(u8, arg)).ptr;
        const cwd_z = try a.dupeZ(u8, opts.cwd);
        const term_kv = try std.fmt.allocPrintSentinel(a, "TERM={s}", .{opts.term}, 0);

        // envp = current environ with TERM replaced/added.
        var env_count: usize = 0;
        while (environ[env_count] != null) env_count += 1;
        const envp = try a.allocSentinel(?[*:0]const u8, env_count + 1, null);
        var out_i: usize = 0;
        for (0..env_count) |i| {
            const entry = environ[i].?;
            if (std.mem.startsWith(u8, std.mem.span(entry), "TERM=")) continue;
            envp[out_i] = entry;
            out_i += 1;
        }
        envp[out_i] = term_kv.ptr;
        envp[out_i + 1] = null;

        const ws = winsize{ .row = opts.rows, .col = opts.cols };
        var master: c_int = -1;
        const pid = forkpty(&master, null, null, &ws);
        if (pid < 0) return error.ForkFailed;
        if (pid == 0) {
            // Child: no allocation, no locks — chdir + execve + _exit only.
            _ = chdir(cwd_z.ptr);
            _ = execve(argv_z[0].?, argv_z.ptr, envp.ptr);
            _exit(127);
        }

        const self = gpa.create(HostPty) catch {
            _ = killpg(pid, SIGKILL);
            _ = close(master);
            return error.OutOfMemory;
        };
        self.* = .{
            .gpa = gpa,
            .master_fd = master,
            .child_pid = pid,
            .on_output = opts.on_output,
            .on_exit = opts.on_exit,
            .ctx = opts.ctx,
        };
        self.thread = std.Thread.spawn(.{}, reader, .{self}) catch {
            _ = killpg(pid, SIGKILL);
            _ = waitpid(pid, null, 0);
            _ = close(master);
            gpa.destroy(self);
            return error.ThreadFailed;
        };
        return self;
    }

    /// Kill the child's process group (if still alive) and join the reader —
    /// no more on_output/on_exit callbacks after this returns. The master fd
    /// intentionally stays OPEN so a concurrent `writeInput` from another
    /// thread (relay input) remains safe (it just gets EIO); `destroy` closes
    /// it once every writer is known to be quiesced. Safe on the GTK main
    /// thread: the reader exits as soon as the child dies (read → EIO).
    pub fn shutdown(self: *HostPty) void {
        if (!self.exited.load(.acquire)) _ = killpg(self.child_pid, SIGKILL);
        if (self.thread) |t| t.join();
        self.thread = null;
    }

    /// shutdown() + close the master + free. Only call once no other thread
    /// can still touch this pty (see the coding launcher's teardown order).
    pub fn destroy(self: *HostPty) void {
        self.shutdown();
        _ = close(self.master_fd);
        self.gpa.destroy(self);
    }

    /// Write input bytes to the PTY master — the single point where local
    /// (ghostty io_write) and remote (relay `input`) keystrokes merge. Any
    /// thread; serialized so concurrent writers can't interleave mid-sequence.
    pub fn writeInput(self: *HostPty, bytes: []const u8) void {
        if (self.exited.load(.acquire)) return;
        self.write_mutex.lock();
        defer self.write_mutex.unlock();
        var sent: usize = 0;
        while (sent < bytes.len) {
            const n = write(self.master_fd, bytes.ptr + sent, bytes.len - sent);
            if (n <= 0) return; // EIO after child exit — drop
            sent += @intCast(n);
        }
    }

    /// Update the PTY winsize (child gets SIGWINCH). Any thread.
    pub fn setWinsize(self: *HostPty, cols: u16, rows: u16) void {
        if (self.exited.load(.acquire)) return;
        const ws = winsize{ .row = rows, .col = cols };
        _ = ioctl(self.master_fd, TIOCSWINSZ, &ws);
    }

    /// Signal the child's process group (kill-switch). Guarded against pid
    /// reuse: no-op once the child was reaped.
    pub fn kill(self: *HostPty, sig: c_int) void {
        if (self.exited.load(.acquire)) return;
        _ = killpg(self.child_pid, sig);
    }

    fn reader(self: *HostPty) void {
        var buf: [8192]u8 = undefined;
        while (true) {
            const n = read(self.master_fd, &buf, buf.len);
            if (n <= 0) break; // 0/EIO ⇒ child gone (slave side closed)
            self.on_output(self.ctx, buf[0..@intCast(n)]);
        }
        var status: c_int = 0;
        _ = waitpid(self.child_pid, &status, 0);
        self.exited.store(true, .release);
        // WIFEXITED ⇒ high byte; killed-by-signal ⇒ 128+signo (shell convention).
        const code: i32 = if ((status & 0x7F) == 0) (status >> 8) & 0xFF else 128 + (status & 0x7F);
        self.on_exit(self.ctx, code);
    }
};

// ---------------------------------------------------------------------------
// tests (headless — a real PTY + /bin/sh, both present in the build VM)
// ---------------------------------------------------------------------------

const timespec = extern struct { sec: c_long, nsec: c_long };
extern "c" fn nanosleep(req: *const timespec, rem: ?*timespec) c_int;

fn sleepMs(ms: u64) void {
    var ts = timespec{ .sec = @intCast(ms / 1000), .nsec = @intCast((ms % 1000) * 1_000_000) };
    _ = nanosleep(&ts, null);
}

const TestSink = struct {
    mutex: util.Mutex = .{},
    output: std.ArrayListUnmanaged(u8) = .empty,
    exit_code: ?i32 = null,
    gpa: std.mem.Allocator,

    fn onOutput(ctx: ?*anyopaque, bytes: []const u8) void {
        const self: *TestSink = @ptrCast(@alignCast(ctx.?));
        self.mutex.lock();
        defer self.mutex.unlock();
        self.output.appendSlice(self.gpa, bytes) catch {};
    }

    fn onExit(ctx: ?*anyopaque, code: i32) void {
        const self: *TestSink = @ptrCast(@alignCast(ctx.?));
        self.mutex.lock();
        defer self.mutex.unlock();
        self.exit_code = code;
    }

    fn waitExit(self: *TestSink, deadline_ms: u64) ?i32 {
        var waited: u64 = 0;
        while (waited < deadline_ms) {
            {
                self.mutex.lock();
                defer self.mutex.unlock();
                if (self.exit_code) |code| return code;
            }
            sleepMs(20);
            waited += 20;
        }
        return null;
    }
};

test "host pty: spawn, tee output, observe exit" {
    const gpa = std.testing.allocator;
    var sink = TestSink{ .gpa = gpa };
    defer sink.output.deinit(gpa);

    const pty = try HostPty.spawn(gpa, .{
        .argv = &.{ "/bin/sh", "-c", "printf steer-tee-ok" },
        .cwd = "/tmp",
        .on_output = TestSink.onOutput,
        .on_exit = TestSink.onExit,
        .ctx = &sink,
    });
    const code = sink.waitExit(5_000);
    pty.destroy();

    try std.testing.expectEqual(@as(i32, 0), code orelse -1);
    sink.mutex.lock();
    defer sink.mutex.unlock();
    try std.testing.expect(std.mem.indexOf(u8, sink.output.items, "steer-tee-ok") != null);
}

test "host pty: remote input reaches the child via the master fd" {
    const gpa = std.testing.allocator;
    var sink = TestSink{ .gpa = gpa };
    defer sink.output.deinit(gpa);

    // `read x` consumes a line from the PTY; the child then echoes it back.
    const pty = try HostPty.spawn(gpa, .{
        .argv = &.{ "/bin/sh", "-c", "read x; printf \"got:%s\" \"$x\"" },
        .cwd = "/tmp",
        .on_output = TestSink.onOutput,
        .on_exit = TestSink.onExit,
        .ctx = &sink,
    });
    pty.setWinsize(120, 40); // exercise TIOCSWINSZ on a live master
    pty.writeInput("hello\n");
    const code = sink.waitExit(5_000);
    pty.destroy();

    try std.testing.expectEqual(@as(i32, 0), code orelse -1);
    sink.mutex.lock();
    defer sink.mutex.unlock();
    try std.testing.expect(std.mem.indexOf(u8, sink.output.items, "got:hello") != null);
}

test "host pty: kill terminates a long-running child group" {
    const gpa = std.testing.allocator;
    var sink = TestSink{ .gpa = gpa };
    defer sink.output.deinit(gpa);

    const pty = try HostPty.spawn(gpa, .{
        .argv = &.{ "/bin/sh", "-c", "sleep 30" },
        .cwd = "/tmp",
        .on_output = TestSink.onOutput,
        .on_exit = TestSink.onExit,
        .ctx = &sink,
    });
    sleepMs(100); // let it start
    pty.kill(SIGKILL);
    const code = sink.waitExit(5_000);
    pty.destroy();
    try std.testing.expectEqual(@as(i32, 128 + 9), code orelse -1);
}
