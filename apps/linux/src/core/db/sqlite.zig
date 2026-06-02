//! Minimal sqlite3 C-API wrapper. Thin on purpose — just enough for the sync
//! engine's WAL store and generic row UPSERT/UPDATE/DELETE.

const std = @import("std");

pub const c = @cImport({
    @cInclude("sqlite3.h");
});

pub const Error = error{ Open, Exec, Prepare, Step, Bind };

pub const Stmt = struct {
    raw: ?*c.sqlite3_stmt,

    pub fn finalize(self: *Stmt) void {
        _ = c.sqlite3_finalize(self.raw);
        self.raw = null;
    }

    /// Bind text by 1-based index. Uses SQLITE_STATIC (null destructor), so the
    /// caller must keep `text` alive until `step` completes.
    pub fn bindText(self: *Stmt, idx: c_int, text: []const u8) !void {
        if (c.sqlite3_bind_text(self.raw, idx, text.ptr, @intCast(text.len), null) != c.SQLITE_OK)
            return Error.Bind;
    }
    pub fn bindInt(self: *Stmt, idx: c_int, v: i64) !void {
        if (c.sqlite3_bind_int64(self.raw, idx, v) != c.SQLITE_OK) return Error.Bind;
    }
    pub fn bindDouble(self: *Stmt, idx: c_int, v: f64) !void {
        if (c.sqlite3_bind_double(self.raw, idx, v) != c.SQLITE_OK) return Error.Bind;
    }
    pub fn bindNull(self: *Stmt, idx: c_int) !void {
        if (c.sqlite3_bind_null(self.raw, idx) != c.SQLITE_OK) return Error.Bind;
    }

    /// Returns true if a row is available (SQLITE_ROW), false on SQLITE_DONE.
    pub fn step(self: *Stmt) !bool {
        return switch (c.sqlite3_step(self.raw)) {
            c.SQLITE_ROW => true,
            c.SQLITE_DONE => false,
            else => Error.Step,
        };
    }

    /// Column text for the current row. Valid only until the next step/finalize.
    pub fn columnText(self: *Stmt, idx: c_int) []const u8 {
        const ptr = c.sqlite3_column_text(self.raw, idx);
        if (ptr == null) return "";
        const len = c.sqlite3_column_bytes(self.raw, idx);
        return ptr[0..@intCast(len)];
    }
    pub fn columnInt(self: *Stmt, idx: c_int) i64 {
        return c.sqlite3_column_int64(self.raw, idx);
    }
};

pub const Conn = struct {
    raw: ?*c.sqlite3,

    pub fn open(path: [:0]const u8) !Conn {
        var h: ?*c.sqlite3 = null;
        if (c.sqlite3_open(path.ptr, &h) != c.SQLITE_OK) {
            _ = c.sqlite3_close(h);
            return Error.Open;
        }
        return .{ .raw = h };
    }

    pub fn close(self: *Conn) void {
        _ = c.sqlite3_close(self.raw);
        self.raw = null;
    }

    /// Execute one or more semicolon-separated statements with no bindings.
    pub fn exec(self: *Conn, sql: [:0]const u8) !void {
        if (c.sqlite3_exec(self.raw, sql.ptr, null, null, null) != c.SQLITE_OK)
            return Error.Exec;
    }

    pub fn prepare(self: *Conn, sql: []const u8) !Stmt {
        var s: ?*c.sqlite3_stmt = null;
        if (c.sqlite3_prepare_v2(self.raw, sql.ptr, @intCast(sql.len), &s, null) != c.SQLITE_OK)
            return Error.Prepare;
        return .{ .raw = s };
    }

    pub fn errMsg(self: *Conn) []const u8 {
        return std.mem.span(c.sqlite3_errmsg(self.raw));
    }
};
