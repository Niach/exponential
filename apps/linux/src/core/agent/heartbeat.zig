//! 30s `companion.heartbeat` loop (Bearer `expk_`) so a registered desktop agent
//! shows up online in the web. One detached worker thread that OWNS its struct:
//! `stop()` only flips a flag (cancellable sleep wakes within ~100ms) and the
//! worker frees itself on exit — so stopping never blocks the GTK main loop /
//! sign-out (an in-flight heartbeat just finishes in the background).

const std = @import("std");
const trpc = @import("../api/trpc.zig");

const Timespec = extern struct { sec: isize, nsec: isize };
extern fn nanosleep(req: *const Timespec, rem: ?*Timespec) c_int;

pub const Heartbeat = struct {
    gpa: std.mem.Allocator,
    base_url: []u8,
    api_key: []u8,
    workspace_id: []u8,
    stop_flag: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),

    /// Allocate + start the loop. The returned pointer is owned by the worker
    /// thread, which frees it after `stop()`. Returns null on OOM / spawn failure.
    pub fn spawn(gpa: std.mem.Allocator, base_url: []const u8, api_key: []const u8, workspace_id: []const u8) ?*Heartbeat {
        const self = gpa.create(Heartbeat) catch return null;
        self.gpa = gpa;
        self.stop_flag = std.atomic.Value(bool).init(false);
        self.base_url = gpa.dupe(u8, base_url) catch {
            gpa.destroy(self);
            return null;
        };
        self.api_key = gpa.dupe(u8, api_key) catch {
            gpa.free(self.base_url);
            gpa.destroy(self);
            return null;
        };
        self.workspace_id = gpa.dupe(u8, workspace_id) catch {
            gpa.free(self.base_url);
            gpa.free(self.api_key);
            gpa.destroy(self);
            return null;
        };
        const th = std.Thread.spawn(.{}, worker, .{self}) catch {
            self.free();
            return null;
        };
        th.detach();
        return self;
    }

    /// Ask the loop to stop; the worker frees the struct on its next wake. The
    /// caller must drop its pointer immediately (do not use it after this).
    pub fn stop(self: *Heartbeat) void {
        self.stop_flag.store(true, .release);
    }

    fn free(self: *Heartbeat) void {
        const gpa = self.gpa;
        gpa.free(self.base_url);
        gpa.free(self.api_key);
        gpa.free(self.workspace_id);
        gpa.destroy(self);
    }
};

fn worker(self: *Heartbeat) void {
    while (!self.stop_flag.load(.acquire)) {
        if (trpc.call(self.gpa, self.base_url, "agent.heartbeat", null, self.api_key, 15)) |*resp| {
            var r = resp.*;
            r.deinit();
        } else |_| {}
        sleepCancellable(self, 30_000);
    }
    self.free();
}

/// Sleep up to `ms`, waking within ~100ms of the stop flag.
fn sleepCancellable(self: *Heartbeat, ms: u64) void {
    var left = ms;
    while (left > 0 and !self.stop_flag.load(.acquire)) {
        const chunk: u64 = @min(left, 100);
        const ts = Timespec{ .sec = 0, .nsec = @intCast(chunk * std.time.ns_per_ms) };
        _ = nanosleep(&ts, null);
        left -= chunk;
    }
}
