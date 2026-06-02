//! Fire-and-forget tRPC mutation on a detached worker thread, so the GTK main
//! loop never blocks on the network. The UI updates optimistically at the call
//! site; Electric sync delivers the authoritative state a moment later and the
//! list/detail re-render reconciles any divergence. Failures are dropped (sync
//! re-renders the true state) — use the blocking `trpc.call` directly where a
//! success/error result must gate the UI (e.g. dialog submit).

const std = @import("std");
const trpc = @import("trpc.zig");

const Job = struct {
    gpa: std.mem.Allocator,
    instance: []u8,
    token: ?[]u8,
    proc: []const u8, // static literal — not freed
    json: []u8,
};

/// Spawn a detached worker that POSTs `proc(json)` and frees its copies. `gpa`
/// must be thread-safe (the app uses `std.heap.page_allocator`). `json` is
/// copied, so the caller's arena can be freed immediately after this returns.
pub fn fire(
    gpa: std.mem.Allocator,
    instance: []const u8,
    token: ?[]const u8,
    proc: []const u8,
    json: []const u8,
) void {
    const job = gpa.create(Job) catch return;
    job.gpa = gpa;
    job.proc = proc;
    job.instance = gpa.dupe(u8, instance) catch {
        gpa.destroy(job);
        return;
    };
    job.json = gpa.dupe(u8, json) catch {
        gpa.free(job.instance);
        gpa.destroy(job);
        return;
    };
    job.token = if (token) |t| (gpa.dupe(u8, t) catch null) else null;

    const th = std.Thread.spawn(.{}, worker, .{job}) catch {
        worker(job); // fallback: run inline (still correct, just blocks once)
        return;
    };
    th.detach();
}

fn worker(job: *Job) void {
    defer freeJob(job);
    var resp = trpc.call(job.gpa, job.instance, job.proc, job.json, job.token, 30) catch return;
    resp.deinit();
}

fn freeJob(job: *Job) void {
    job.gpa.free(job.instance);
    job.gpa.free(job.json);
    if (job.token) |t| job.gpa.free(t);
    job.gpa.destroy(job);
}
