//! Pure, GTK-free presentation helpers.
//!
//! Status/priority option tables (value, label, glyph, color) mirror
//! apps/web/src/lib/domain.ts and the iOS `IssueStatus`/`IssuePriority` enums —
//! the single visual contract across clients. Due-date formatting matches the
//! iOS list ("Today"/"Tomorrow"/"MMM d", red/orange/dim). No GTK here so it can
//! be unit-tested in isolation (pulled into src/tests.zig).

const std = @import("std");

pub const Option = struct {
    value: []const u8,
    label: []const u8,
    glyph: []const u8, // one display glyph (UTF-8)
    color: []const u8, // "#rrggbb"
};

// Glyphs/colors approximate the lucide icons (web) and SF Symbols (iOS):
// backlog dashed circle, todo empty circle, in_progress half circle, done
// filled, cancelled cross, duplicate copy glyph; priority none dash, urgent
// warning, high/low arrows, medium equals — coloured to match the
// zinc/yellow/green/red/orange palette.
pub const statuses = [_]Option{
    .{ .value = "backlog", .label = "Backlog", .glyph = "◌", .color = "#9aa0aa" },
    .{ .value = "todo", .label = "Todo", .glyph = "○", .color = "#d4d4d8" },
    .{ .value = "in_progress", .label = "In Progress", .glyph = "◑", .color = "#eab308" },
    .{ .value = "done", .label = "Done", .glyph = "●", .color = "#22c55e" },
    .{ .value = "cancelled", .label = "Cancelled", .glyph = "✕", .color = "#9aa0aa" },
    .{ .value = "duplicate", .label = "Duplicate", .glyph = "⧉", .color = "#9aa0aa" },
};

pub const priorities = [_]Option{
    .{ .value = "none", .label = "No priority", .glyph = "–", .color = "#9aa0aa" },
    .{ .value = "urgent", .label = "Urgent", .glyph = "⚠", .color = "#ef4444" },
    .{ .value = "high", .label = "High", .glyph = "↑", .color = "#f97316" },
    .{ .value = "medium", .label = "Medium", .glyph = "=", .color = "#eab308" },
    .{ .value = "low", .label = "Low", .glyph = "↓", .color = "#3b82f6" },
};

/// Group display order (matches contract.generated.zig display orders).
pub const status_display_order = [_][]const u8{ "in_progress", "todo", "backlog", "done", "cancelled", "duplicate" };
pub const priority_display_order = [_][]const u8{ "urgent", "high", "medium", "low", "none" };

/// Project/label colour palette — mirrors apps/web/src/lib/label-colors.ts.
pub const label_colors = [_][:0]const u8{
    "#ef4444", "#dc2626", "#f97316", "#f59e0b", "#eab308",
    "#84cc16", "#22c55e", "#10b981", "#14b8a6", "#06b6d4",
    "#0ea5e9", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7",
    "#ec4899", "#f43f5e", "#78716c", "#64748b", "#a3a3a3",
};

pub fn status(value: []const u8) Option {
    for (statuses) |o| if (std.mem.eql(u8, o.value, value)) return o;
    return statuses[0];
}

pub fn priority(value: []const u8) Option {
    for (priorities) |o| if (std.mem.eql(u8, o.value, value)) return o;
    return priorities[0];
}

/// Tab presets → the statuses they include (mirrors lib/filters.ts).
pub const Tab = enum { all, active, backlog };

pub fn tabIncludesStatus(tab: Tab, value: []const u8) bool {
    return switch (tab) {
        .all => true,
        .active => std.mem.eql(u8, value, "in_progress") or std.mem.eql(u8, value, "todo"),
        .backlog => std.mem.eql(u8, value, "backlog"),
    };
}

// --- due date ---

pub const due_red = "#ef4444"; // overdue
pub const due_orange = "#f97316"; // today
pub const due_dim = "#9aa0aa"; // upcoming

pub const Due = struct {
    /// Short human label allocated in the caller's arena.
    text: [:0]u8,
    color: []const u8,
};

const month_names = [_][]const u8{ "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec" };

/// Format a "YYYY-MM-DD" due date relative to today. Returns null if the string
/// isn't a valid date.
pub fn formatDue(arena: std.mem.Allocator, ymd: []const u8) !?Due {
    return formatDueOn(arena, ymd, todayDays());
}

/// Testable seam: `today_days` is days since the Unix epoch for "today".
pub fn formatDueOn(arena: std.mem.Allocator, ymd: []const u8, today_days: i64) !?Due {
    const parsed = parseYmd(ymd) orelse return null;
    const due_days = daysFromCivil(parsed.y, parsed.m, parsed.d);

    const color: []const u8 = if (due_days < today_days)
        due_red
    else if (due_days == today_days)
        due_orange
    else
        due_dim;

    const text: [:0]u8 = if (due_days == today_days)
        try arena.dupeZ(u8, "Today")
    else if (due_days == today_days + 1)
        try arena.dupeZ(u8, "Tomorrow")
    else
        try std.fmt.allocPrintSentinel(arena, "{s} {d}", .{ month_names[@intCast(parsed.m - 1)], parsed.d }, 0);

    return .{ .text = text, .color = color };
}

const Ymd = struct { y: i64, m: i64, d: i64 };

fn parseYmd(s: []const u8) ?Ymd {
    if (s.len < 10) return null;
    if (s[4] != '-' or s[7] != '-') return null;
    const y = std.fmt.parseInt(i64, s[0..4], 10) catch return null;
    const m = std.fmt.parseInt(i64, s[5..7], 10) catch return null;
    const d = std.fmt.parseInt(i64, s[8..10], 10) catch return null;
    if (m < 1 or m > 12 or d < 1 or d > 31) return null;
    return .{ .y = y, .m = m, .d = d };
}

// libc time(2) — std.time.timestamp() was removed in 0.16; format.zig is only
// ever compiled with libc linked (GUI + tests both link sqlite/curl).
extern fn time(tloc: ?*i64) i64;

fn todayDays() i64 {
    return @divFloor(time(null), 86400);
}

/// Days since 1970-01-01 (Howard Hinnant's days_from_civil).
fn daysFromCivil(y: i64, m: i64, d: i64) i64 {
    const yy = if (m <= 2) y - 1 else y;
    const era = @divFloor(if (yy >= 0) yy else yy - 399, 400);
    const yoe = yy - era * 400; // [0, 399]
    const mp = if (m > 2) m - 3 else m + 9; // [0, 11]
    const doy = @divFloor(153 * mp + 2, 5) + d - 1; // [0, 365]
    const doe = yoe * 365 + @divFloor(yoe, 4) - @divFloor(yoe, 100) + doy; // [0, 146096]
    return era * 146097 + doe - 719468;
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

test "status/priority lookup falls back to the first option" {
    try std.testing.expectEqualStrings("In Progress", status("in_progress").label);
    try std.testing.expectEqualStrings("●", status("done").glyph);
    try std.testing.expectEqualStrings("Backlog", status("bogus").label); // fallback
    try std.testing.expectEqualStrings("Urgent", priority("urgent").label);
    try std.testing.expectEqualStrings("No priority", priority("bogus").label);
}

test "tab presets include the right statuses" {
    try std.testing.expect(format_tab(.active, "todo"));
    try std.testing.expect(format_tab(.active, "in_progress"));
    try std.testing.expect(!format_tab(.active, "backlog"));
    try std.testing.expect(format_tab(.backlog, "backlog"));
    try std.testing.expect(!format_tab(.backlog, "done"));
    try std.testing.expect(format_tab(.all, "cancelled"));
}
fn format_tab(t: Tab, v: []const u8) bool {
    return tabIncludesStatus(t, v);
}

test "daysFromCivil epoch anchors" {
    try std.testing.expectEqual(@as(i64, 0), daysFromCivil(1970, 1, 1));
    try std.testing.expectEqual(@as(i64, 18262), daysFromCivil(2020, 1, 1));
}

test "formatDue: today / tomorrow / past / future" {
    const a = std.testing.allocator;
    var arena = std.heap.ArenaAllocator.init(a);
    defer arena.deinit();
    const al = arena.allocator();

    const today = daysFromCivil(2026, 6, 1);

    const t = (try formatDueOn(al, "2026-06-01", today)).?;
    try std.testing.expectEqualStrings("Today", t.text);
    try std.testing.expectEqualStrings(due_orange, t.color);

    const tm = (try formatDueOn(al, "2026-06-02", today)).?;
    try std.testing.expectEqualStrings("Tomorrow", tm.text);

    const past = (try formatDueOn(al, "2020-01-15", today)).?;
    try std.testing.expectEqualStrings("Jan 15", past.text);
    try std.testing.expectEqualStrings(due_red, past.color);

    const fut = (try formatDueOn(al, "2099-12-31", today)).?;
    try std.testing.expectEqualStrings("Dec 31", fut.text);
    try std.testing.expectEqualStrings(due_dim, fut.color);

    try std.testing.expect((try formatDueOn(al, "not-a-date", today)) == null);
    try std.testing.expect((try formatDueOn(al, "", today)) == null);
}
