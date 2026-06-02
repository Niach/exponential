//! XDG config location + secret-file helpers. Tokens are written 0600 (matching
//! the companion's bot.token). A libsecret backend can replace the file store
//! later; the call sites only use these functions.

const std = @import("std");

extern "c" fn chmod(path: [*:0]const u8, mode: c_uint) c_int;

fn io() std.Io {
    return std.Io.Threaded.global_single_threaded.io();
}

/// The process environment, stashed from `main` so background workers (e.g. the
/// agent-core run handler) can seed a child process's env without threading
/// `std.process.Init` through every call site. Null until `setEnviron` runs.
var g_environ: ?*std.process.Environ.Map = null;

pub fn setEnviron(map: *std.process.Environ.Map) void {
    g_environ = map;
}

pub fn environ() ?*std.process.Environ.Map {
    return g_environ;
}

/// `$XDG_CONFIG_HOME/exponential-desktop` or `$HOME/.config/exponential-desktop`
/// (allocated with `gpa`).
pub fn configDir(gpa: std.mem.Allocator) ![]u8 {
    if (std.c.getenv("XDG_CONFIG_HOME")) |x| {
        const base = std.mem.span(x);
        if (base.len > 0) return std.fs.path.join(gpa, &.{ base, "exponential-desktop" });
    }
    const home = std.c.getenv("HOME") orelse return error.NoHomeDir;
    return std.fs.path.join(gpa, &.{ std.mem.span(home), ".config", "exponential-desktop" });
}

/// mkdir -p the directory (no error if it already exists).
pub fn ensureDir(path: []const u8) !void {
    try std.Io.Dir.cwd().createDirPath(io(), path);
}

/// Write a file owner-readable only (0600).
pub fn writeSecret(abs_path: []const u8, data: []const u8) !void {
    try std.Io.Dir.cwd().writeFile(io(), .{ .sub_path = abs_path, .data = data });
    var buf: [std.fs.max_path_bytes]u8 = undefined;
    if (std.fmt.bufPrintZ(&buf, "{s}", .{abs_path})) |z| {
        _ = chmod(z.ptr, 0o600);
    } else |_| {}
}

/// Read a whole file, or null if it doesn't exist / can't be read (treated as an
/// empty store on load).
pub fn readFileAlloc(gpa: std.mem.Allocator, abs_path: []const u8) ?[]u8 {
    return std.Io.Dir.cwd().readFileAlloc(io(), abs_path, gpa, .unlimited) catch null;
}

/// Whether a file exists (best-effort).
pub fn fileExists(abs_path: []const u8) bool {
    std.Io.Dir.cwd().access(io(), abs_path, .{}) catch return false;
    return true;
}

/// Delete a file (no error if missing).
pub fn deleteFile(abs_path: []const u8) void {
    std.Io.Dir.cwd().deleteFile(io(), abs_path) catch {};
}
