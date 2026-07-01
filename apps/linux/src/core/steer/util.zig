//! Tiny concurrency/entropy shims for the steer stack on Zig 0.16.
//!
//! 0.16 removed `std.Thread.Mutex` (its `std.Io.Mutex` successor needs an `Io`
//! handle) and `std.crypto.random`. The DB layer's swap-spinlock idiom is wrong
//! here — steer critical sections include blocking socket sends, and spinning
//! the GTK main thread against a stalled send would freeze the UI — so this
//! wraps the libc pthread mutex (the app links libc everywhere) and getrandom.

const std = @import("std");

// glibc pthread_mutex_t is 40 bytes on 64-bit Linux (x86_64 + aarch64) and its
// static initializer is all-zeroes; 64 bytes of zeroed storage is a safe
// superset for both ABIs this app targets.
const pthread_mutex_t = extern struct { data: [64]u8 align(8) = @splat(0) };

extern "c" fn pthread_mutex_lock(mutex: *pthread_mutex_t) c_int;
extern "c" fn pthread_mutex_unlock(mutex: *pthread_mutex_t) c_int;

/// A plain blocking mutex (parks the thread, never spins).
pub const Mutex = struct {
    inner: pthread_mutex_t = .{},

    pub fn lock(self: *Mutex) void {
        _ = pthread_mutex_lock(&self.inner);
    }

    pub fn unlock(self: *Mutex) void {
        _ = pthread_mutex_unlock(&self.inner);
    }
};

extern "c" fn getrandom(buf: [*]u8, len: usize, flags: c_uint) isize;

const timespec = extern struct { sec: c_long, nsec: c_long };
extern "c" fn clock_gettime(clock_id: c_int, tp: *timespec) c_int;

/// Fill `buf` with OS entropy (WS masks/nonces, device ids). Falls back to a
/// clock-seeded PRNG if getrandom ever fails (pre-3.17 kernels only) — none of
/// these uses require cryptographic strength (the relay ticket is the
/// credential).
pub fn fillRandom(buf: []u8) void {
    var done: usize = 0;
    while (done < buf.len) {
        const n = getrandom(buf.ptr + done, buf.len - done, 0);
        if (n <= 0) break;
        done += @intCast(n);
    }
    if (done >= buf.len) return;
    var ts = timespec{ .sec = 0, .nsec = 0 };
    _ = clock_gettime(1, &ts); // CLOCK_MONOTONIC
    const seed: u64 = @as(u64, @bitCast(@as(i64, ts.sec))) ^
        (@as(u64, @bitCast(@as(i64, ts.nsec))) << 20) ^ @intFromPtr(buf.ptr);
    var prng = std.Random.DefaultPrng.init(seed);
    prng.random().bytes(buf[done..]);
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

test "mutex round-trips lock/unlock across threads" {
    var mutex = Mutex{};
    var counter: i64 = 0;

    const Worker = struct {
        fn run(m: *Mutex, c: *i64) void {
            for (0..1000) |_| {
                m.lock();
                c.* += 1;
                m.unlock();
            }
        }
    };
    const t1 = try std.Thread.spawn(.{}, Worker.run, .{ &mutex, &counter });
    const t2 = try std.Thread.spawn(.{}, Worker.run, .{ &mutex, &counter });
    t1.join();
    t2.join();
    try std.testing.expectEqual(@as(i64, 2000), counter);
}

test "fillRandom fills every byte region" {
    var buf: [64]u8 = @splat(0);
    fillRandom(&buf);
    var nonzero: usize = 0;
    for (buf) |b| {
        if (b != 0) nonzero += 1;
    }
    // 64 zero bytes from a random source is a 2^-512 event.
    try std.testing.expect(nonzero > 0);
}
