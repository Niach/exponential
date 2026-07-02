//! GTK-free bookkeeping for the terminal dock's tabs (masterplan §4d): one
//! entry per terminal tab, keyed by the coding session id (`coding_sessions.id`)
//! or a run-target id. Tracks which top-level each tab currently lives in
//! (docked vs. a detached window) across AdwTabView reparents. Pure data —
//! the GTK wiring lives in `terminal_dock.zig` — so keying + reparent
//! bookkeeping stay headless-testable (`zig build test` links no GTK).

const std = @import("std");

pub const Kind = enum { coding, run };
pub const Location = enum { dock, window };

pub const Entry = struct {
    /// Owned. Unique within the registry (empty/duplicate requests get a
    /// generated `tab-N` key — two concurrent runs of the same run target must
    /// both keep their tab).
    key: []u8,
    kind: Kind,
    /// The tab's child widget (opaque here; a GtkWidget* in the dock). The
    /// SAME pointer survives detach/reattach — reparent, never recreate.
    widget: ?*anyopaque,
    location: Location = .dock,
    /// The detached top-level when `location == .window` (else null).
    window: ?*anyopaque = null,
    /// GObject signal handler id of the dock's widget-destroy hook (0 = none).
    /// Stored so the dock can disarm it before its own teardown.
    destroy_handler: c_ulong = 0,
};

pub const TabRegistry = struct {
    gpa: std.mem.Allocator,
    entries: std.ArrayListUnmanaged(*Entry) = .empty,
    auto_seq: usize = 0,

    pub fn init(gpa: std.mem.Allocator) TabRegistry {
        return .{ .gpa = gpa };
    }

    pub fn deinit(self: *TabRegistry) void {
        for (self.entries.items) |e| self.freeEntry(e);
        self.entries.deinit(self.gpa);
    }

    fn freeEntry(self: *TabRegistry, e: *Entry) void {
        self.gpa.free(e.key);
        self.gpa.destroy(e);
    }

    /// Register a tab. An empty or already-taken key is replaced by a
    /// generated unique one. Returns null only on OOM.
    pub fn add(self: *TabRegistry, key: []const u8, kind: Kind, widget: ?*anyopaque) ?*Entry {
        const owned = self.ownedUniqueKey(key) orelse return null;
        const e = self.gpa.create(Entry) catch {
            self.gpa.free(owned);
            return null;
        };
        e.* = .{ .key = owned, .kind = kind, .widget = widget };
        self.entries.append(self.gpa, e) catch {
            self.freeEntry(e);
            return null;
        };
        return e;
    }

    fn ownedUniqueKey(self: *TabRegistry, key: []const u8) ?[]u8 {
        if (key.len > 0 and self.get(key) == null) return self.gpa.dupe(u8, key) catch null;
        while (true) {
            self.auto_seq += 1;
            var buf: [32]u8 = undefined;
            const candidate = std.fmt.bufPrint(&buf, "tab-{d}", .{self.auto_seq}) catch return null;
            if (self.get(candidate) == null) return self.gpa.dupe(u8, candidate) catch null;
        }
    }

    pub fn get(self: *const TabRegistry, key: []const u8) ?*Entry {
        for (self.entries.items) |e| if (std.mem.eql(u8, e.key, key)) return e;
        return null;
    }

    pub fn byWidget(self: *const TabRegistry, widget: ?*anyopaque) ?*Entry {
        if (widget == null) return null;
        for (self.entries.items) |e| if (e.widget == widget) return e;
        return null;
    }

    /// A reparent landed the tab in a detached window.
    pub fn markWindow(self: *TabRegistry, widget: ?*anyopaque, window: ?*anyopaque) bool {
        const e = self.byWidget(widget) orelse return false;
        e.location = .window;
        e.window = window;
        return true;
    }

    /// A reparent landed the tab back in the main dock.
    pub fn markDocked(self: *TabRegistry, widget: ?*anyopaque) bool {
        const e = self.byWidget(widget) orelse return false;
        e.location = .dock;
        e.window = null;
        return true;
    }

    pub fn remove(self: *TabRegistry, key: []const u8) bool {
        for (self.entries.items, 0..) |e, i| {
            if (std.mem.eql(u8, e.key, key)) {
                _ = self.entries.swapRemove(i);
                self.freeEntry(e);
                return true;
            }
        }
        return false;
    }

    pub fn removeByWidget(self: *TabRegistry, widget: ?*anyopaque) bool {
        if (widget == null) return false;
        for (self.entries.items, 0..) |e, i| {
            if (e.widget == widget) {
                _ = self.entries.swapRemove(i);
                self.freeEntry(e);
                return true;
            }
        }
        return false;
    }

    pub fn count(self: *const TabRegistry) usize {
        return self.entries.items.len;
    }

    pub fn countIn(self: *const TabRegistry, loc: Location) usize {
        var n: usize = 0;
        for (self.entries.items) |e| {
            if (e.location == loc) n += 1;
        }
        return n;
    }
};

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

const testing = std.testing;

// Opaque stand-ins for widget/window pointers.
var w1: u8 = 0;
var w2: u8 = 0;
var win1: u8 = 0;

test "keyed add + get + remove" {
    var reg = TabRegistry.init(testing.allocator);
    defer reg.deinit();

    const e = reg.add("session-1", .coding, &w1).?;
    try testing.expectEqualStrings("session-1", e.key);
    try testing.expectEqual(Kind.coding, e.kind);
    try testing.expectEqual(Location.dock, e.location);
    try testing.expectEqual(@as(usize, 1), reg.count());
    try testing.expect(reg.get("session-1") == e);
    try testing.expect(reg.get("nope") == null);

    try testing.expect(reg.remove("session-1"));
    try testing.expect(!reg.remove("session-1"));
    try testing.expectEqual(@as(usize, 0), reg.count());
}

test "empty key gets a generated unique key" {
    var reg = TabRegistry.init(testing.allocator);
    defer reg.deinit();

    const a = reg.add("", .run, &w1).?;
    const b = reg.add("", .run, &w2).?;
    try testing.expect(a.key.len > 0);
    try testing.expect(b.key.len > 0);
    try testing.expect(!std.mem.eql(u8, a.key, b.key));
    try testing.expectEqual(@as(usize, 2), reg.count());
}

test "duplicate key keeps both tabs under distinct keys" {
    var reg = TabRegistry.init(testing.allocator);
    defer reg.deinit();

    const a = reg.add("web-dev", .run, &w1).?;
    const b = reg.add("web-dev", .run, &w2).?;
    try testing.expectEqualStrings("web-dev", a.key);
    try testing.expect(!std.mem.eql(u8, b.key, "web-dev"));
    // Both stay addressable.
    try testing.expect(reg.get(a.key) == a);
    try testing.expect(reg.get(b.key) == b);
}

test "detach/reattach bookkeeping (reparent, never recreate)" {
    var reg = TabRegistry.init(testing.allocator);
    defer reg.deinit();

    const e = reg.add("session-9", .coding, &w1).?;
    try testing.expectEqual(@as(usize, 1), reg.countIn(.dock));
    try testing.expectEqual(@as(usize, 0), reg.countIn(.window));

    // Pop out: same widget pointer, new location + window.
    try testing.expect(reg.markWindow(&w1, &win1));
    try testing.expectEqual(Location.window, e.location);
    try testing.expect(e.window == @as(?*anyopaque, &win1));
    try testing.expect(e.widget == @as(?*anyopaque, &w1)); // NEVER swapped
    try testing.expectEqual(@as(usize, 1), reg.countIn(.window));

    // Drag back into the dock.
    try testing.expect(reg.markDocked(&w1));
    try testing.expectEqual(Location.dock, e.location);
    try testing.expect(e.window == null);
    try testing.expectEqual(@as(usize, 1), reg.countIn(.dock));

    // Unknown widgets don't corrupt anything.
    try testing.expect(!reg.markWindow(&w2, &win1));
    try testing.expect(!reg.markDocked(&w2));
    try testing.expect(!reg.markWindow(null, &win1));
}

test "byWidget + removeByWidget" {
    var reg = TabRegistry.init(testing.allocator);
    defer reg.deinit();

    const a = reg.add("a", .coding, &w1).?;
    _ = reg.add("b", .run, &w2).?;
    try testing.expect(reg.byWidget(&w1) == a);
    try testing.expect(reg.byWidget(null) == null);

    try testing.expect(reg.removeByWidget(&w1));
    try testing.expect(!reg.removeByWidget(&w1));
    try testing.expect(reg.byWidget(&w1) == null);
    try testing.expectEqual(@as(usize, 1), reg.count());
}
