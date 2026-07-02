//! GTK-free unified-diff parsing into a line-anchored side-by-side model
//! (masterplan §4e.5). GitHub's `issues.prFiles` returns each file's `patch`
//! as a unified diff; the desktop diff view renders it as two aligned columns
//! (old / new). Every row keeps the ORIGINAL file line number for each side so
//! review comments can later anchor to (filename, side, line) — the row model
//! is the anchor, the widgets are disposable.
//!
//! Alignment: within a hunk, a run of `-` lines followed by a run of `+` lines
//! is treated as one change block and paired index-by-index (the classic
//! side-by-side pairing); the longer side's remainder gets empty filler cells
//! on the other side. Context lines appear on both sides. Hunk headers become
//! full-width separator rows carrying the raw `@@ …` text.

const std = @import("std");

/// What one CELL (one side of a row) shows.
pub const CellKind = enum {
    /// Filler — the other side has a line here, this side doesn't.
    empty,
    /// Unchanged line (present on both sides).
    context,
    /// Line only in the new file (`+`).
    add,
    /// Line only in the old file (`-`).
    del,
    /// Hunk separator (`@@ -a,b +c,d @@ …`), spans both sides.
    hunk,
};

/// One aligned side-by-side row. `*_line` is the 1-based line number in that
/// side's file (0 = no line on that side). Texts have the +/-/space diff
/// marker stripped; hunk rows carry the raw header text on both sides.
pub const Row = struct {
    old_kind: CellKind = .empty,
    new_kind: CellKind = .empty,
    old_line: u32 = 0,
    new_line: u32 = 0,
    old_text: []const u8 = "",
    new_text: []const u8 = "",
};

const PendingLine = struct { line: u32, text: []const u8 };

/// Parse a unified `patch` into aligned side-by-side rows. All returned slices
/// reference `patch` (zero-copy) — the caller keeps `patch` alive as long as
/// the rows. Rows are allocated from `arena`.
pub fn parseSideBySide(arena: std.mem.Allocator, patch: []const u8) ![]Row {
    var rows: std.ArrayListUnmanaged(Row) = .empty;
    var dels: std.ArrayListUnmanaged(PendingLine) = .empty;
    var adds: std.ArrayListUnmanaged(PendingLine) = .empty;
    defer dels.deinit(arena);
    defer adds.deinit(arena);

    var old_no: u32 = 0;
    var new_no: u32 = 0;

    var it = std.mem.splitScalar(u8, patch, '\n');
    while (it.next()) |line| {
        if (std.mem.startsWith(u8, line, "@@")) {
            try flushChange(arena, &rows, &dels, &adds);
            const nums = parseHunkHeader(line);
            old_no = nums.old_start;
            new_no = nums.new_start;
            try rows.append(arena, .{
                .old_kind = .hunk,
                .new_kind = .hunk,
                .old_text = line,
                .new_text = line,
            });
        } else if (std.mem.startsWith(u8, line, "\\")) {
            // "\ No newline at end of file" — metadata, not a file line.
            continue;
        } else if (line.len > 0 and line[0] == '-') {
            try dels.append(arena, .{ .line = old_no, .text = line[1..] });
            old_no += 1;
        } else if (line.len > 0 and line[0] == '+') {
            try adds.append(arena, .{ .line = new_no, .text = line[1..] });
            new_no += 1;
        } else {
            // Context line (leading space) — or the trailing empty split
            // fragment after the patch's final newline, which we skip.
            if (line.len == 0 and it.peek() == null) break;
            try flushChange(arena, &rows, &dels, &adds);
            const text = if (line.len > 0 and line[0] == ' ') line[1..] else line;
            try rows.append(arena, .{
                .old_kind = .context,
                .new_kind = .context,
                .old_line = old_no,
                .new_line = new_no,
                .old_text = text,
                .new_text = text,
            });
            old_no += 1;
            new_no += 1;
        }
    }
    try flushChange(arena, &rows, &dels, &adds);
    return rows.toOwnedSlice(arena);
}

/// Pair the pending -/+ runs into aligned change rows (filler on the short side).
fn flushChange(
    arena: std.mem.Allocator,
    rows: *std.ArrayListUnmanaged(Row),
    dels: *std.ArrayListUnmanaged(PendingLine),
    adds: *std.ArrayListUnmanaged(PendingLine),
) !void {
    const n = @max(dels.items.len, adds.items.len);
    for (0..n) |i| {
        var row: Row = .{};
        if (i < dels.items.len) {
            row.old_kind = .del;
            row.old_line = dels.items[i].line;
            row.old_text = dels.items[i].text;
        }
        if (i < adds.items.len) {
            row.new_kind = .add;
            row.new_line = adds.items[i].line;
            row.new_text = adds.items[i].text;
        }
        try rows.append(arena, row);
    }
    dels.clearRetainingCapacity();
    adds.clearRetainingCapacity();
}

const HunkNums = struct { old_start: u32 = 1, new_start: u32 = 1 };

/// Parse "@@ -a[,b] +c[,d] @@ …" → starting line numbers (1-based; a `0` start
/// means an empty side, e.g. a new file's old side).
fn parseHunkHeader(line: []const u8) HunkNums {
    var nums: HunkNums = .{};
    var i: usize = 0;
    // old start: after "-"
    if (std.mem.indexOfScalarPos(u8, line, 0, '-')) |dash| {
        i = dash + 1;
        nums.old_start = parseInt(line, &i);
    }
    // new start: after "+"
    if (std.mem.indexOfScalarPos(u8, line, i, '+')) |plus| {
        i = plus + 1;
        nums.new_start = parseInt(line, &i);
    }
    return nums;
}

fn parseInt(line: []const u8, i: *usize) u32 {
    var v: u32 = 0;
    while (i.* < line.len and line[i.*] >= '0' and line[i.*] <= '9') : (i.* += 1) {
        v = v *% 10 +% (line[i.*] - '0');
    }
    return v;
}

// ---------------------------------------------------------------------------

const t = std.testing;

test "parseSideBySide: change block pairs dels with adds, filler on the short side" {
    var arena = std.heap.ArenaAllocator.init(t.allocator);
    defer arena.deinit();
    const patch =
        "@@ -10,4 +10,5 @@ fn main() {\n" ++
        " ctx\n" ++
        "-old one\n" ++
        "-old two\n" ++
        "+new one\n" ++
        "+new two\n" ++
        "+new three\n" ++
        " tail\n";
    const rows = try parseSideBySide(arena.allocator(), patch);
    try t.expectEqual(@as(usize, 6), rows.len);

    try t.expectEqual(CellKind.hunk, rows[0].old_kind);
    try t.expectEqualStrings("@@ -10,4 +10,5 @@ fn main() {", rows[0].old_text);

    try t.expectEqual(CellKind.context, rows[1].old_kind);
    try t.expectEqual(@as(u32, 10), rows[1].old_line);
    try t.expectEqual(@as(u32, 10), rows[1].new_line);
    try t.expectEqualStrings("ctx", rows[1].old_text);

    // paired change rows
    try t.expectEqual(CellKind.del, rows[2].old_kind);
    try t.expectEqual(CellKind.add, rows[2].new_kind);
    try t.expectEqual(@as(u32, 11), rows[2].old_line);
    try t.expectEqual(@as(u32, 11), rows[2].new_line);
    try t.expectEqualStrings("old one", rows[2].old_text);
    try t.expectEqualStrings("new one", rows[2].new_text);

    try t.expectEqual(CellKind.del, rows[3].old_kind);
    try t.expectEqual(CellKind.add, rows[3].new_kind);
    try t.expectEqualStrings("old two", rows[3].old_text);
    try t.expectEqualStrings("new two", rows[3].new_text);

    // surplus add gets an empty old cell
    try t.expectEqual(CellKind.empty, rows[4].old_kind);
    try t.expectEqual(@as(u32, 0), rows[4].old_line);
    try t.expectEqual(CellKind.add, rows[4].new_kind);
    try t.expectEqual(@as(u32, 13), rows[4].new_line);
    try t.expectEqualStrings("new three", rows[4].new_text);

    // trailing context re-syncs both counters
    try t.expectEqual(CellKind.context, rows[5].old_kind);
    try t.expectEqual(@as(u32, 13), rows[5].old_line);
    try t.expectEqual(@as(u32, 14), rows[5].new_line);
    try t.expectEqualStrings("tail", rows[5].old_text);
}

test "parseSideBySide: multiple hunks + no-newline marker + del-only block" {
    var arena = std.heap.ArenaAllocator.init(t.allocator);
    defer arena.deinit();
    const patch =
        "@@ -1,2 +1 @@\n" ++
        "-gone\n" ++
        " kept\n" ++
        "@@ -9,2 +8,2 @@\n" ++
        " a\n" ++
        "-b\n" ++
        "+B\n" ++
        "\\ No newline at end of file\n";
    const rows = try parseSideBySide(arena.allocator(), patch);
    try t.expectEqual(@as(usize, 6), rows.len);

    try t.expectEqual(CellKind.del, rows[1].old_kind);
    try t.expectEqual(CellKind.empty, rows[1].new_kind);
    try t.expectEqual(@as(u32, 1), rows[1].old_line);

    try t.expectEqual(CellKind.context, rows[2].old_kind);
    try t.expectEqual(@as(u32, 2), rows[2].old_line);
    try t.expectEqual(@as(u32, 1), rows[2].new_line);

    try t.expectEqual(CellKind.hunk, rows[3].old_kind);

    try t.expectEqual(@as(u32, 9), rows[4].old_line); // second hunk restarts numbering
    try t.expectEqual(@as(u32, 8), rows[4].new_line);

    try t.expectEqual(CellKind.del, rows[5].old_kind);
    try t.expectEqual(CellKind.add, rows[5].new_kind);
    try t.expectEqualStrings("b", rows[5].old_text);
    try t.expectEqualStrings("B", rows[5].new_text);
}

test "parseSideBySide: new-file patch (old side 0,0)" {
    var arena = std.heap.ArenaAllocator.init(t.allocator);
    defer arena.deinit();
    const patch =
        "@@ -0,0 +1,2 @@\n" ++
        "+hello\n" ++
        "+world";
    const rows = try parseSideBySide(arena.allocator(), patch);
    try t.expectEqual(@as(usize, 3), rows.len);
    try t.expectEqual(CellKind.empty, rows[1].old_kind);
    try t.expectEqual(CellKind.add, rows[1].new_kind);
    try t.expectEqual(@as(u32, 1), rows[1].new_line);
    try t.expectEqual(@as(u32, 2), rows[2].new_line);
    try t.expectEqualStrings("world", rows[2].new_text);
}

test "parseSideBySide: empty patch yields no rows" {
    var arena = std.heap.ArenaAllocator.init(t.allocator);
    defer arena.deinit();
    const rows = try parseSideBySide(arena.allocator(), "");
    try t.expectEqual(@as(usize, 0), rows.len);
}
