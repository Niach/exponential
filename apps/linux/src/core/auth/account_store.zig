//! Persistent multi-account store (the human's instance logins) — mirrors the
//! iOS `AccountStore`. Serialized as JSON at `{configDir}/accounts.json` (0600).
//! All account strings live in the store's arena; the ArrayList nodes in `gpa`.

const std = @import("std");
const storage = @import("../storage.zig");
const ServerAccount = @import("server_account.zig").ServerAccount;

const StoreData = struct {
    active_id: ?[]const u8 = null,
    accounts: []ServerAccount = &.{},
};

pub const AccountStore = struct {
    gpa: std.mem.Allocator,
    arena: std.heap.ArenaAllocator,
    path: []const u8,
    active_id: ?[]const u8 = null,
    accounts: std.ArrayList(ServerAccount) = .empty,

    pub fn open(gpa: std.mem.Allocator) !AccountStore {
        var store = AccountStore{
            .gpa = gpa,
            .arena = std.heap.ArenaAllocator.init(gpa),
            .path = "",
        };
        const a = store.arena.allocator();
        const dir = try storage.configDir(a);
        storage.ensureDir(dir) catch {}; // best-effort; surfaced on save
        store.path = try std.fs.path.join(a, &.{ dir, "accounts.json" });
        try store.load();
        return store;
    }

    pub fn deinit(self: *AccountStore) void {
        self.accounts.deinit(self.gpa);
        self.arena.deinit();
    }

    fn load(self: *AccountStore) !void {
        const bytes = storage.readFileAlloc(self.gpa, self.path) orelse return;
        defer self.gpa.free(bytes);
        const parsed = std.json.parseFromSlice(StoreData, self.gpa, bytes, .{ .ignore_unknown_fields = true }) catch return;
        defer parsed.deinit();

        const a = self.arena.allocator();
        if (parsed.value.active_id) |id| self.active_id = try a.dupe(u8, id);
        for (parsed.value.accounts) |acc| {
            try self.accounts.append(self.gpa, try dupAccount(a, acc));
        }
    }

    pub fn save(self: *AccountStore) !void {
        const data = StoreData{ .active_id = self.active_id, .accounts = self.accounts.items };
        const json = try std.json.Stringify.valueAlloc(self.gpa, data, .{});
        defer self.gpa.free(json);
        try storage.writeSecret(self.path, json);
    }

    /// Insert or replace by id (id = sha256(instanceUrl)[..8]); becomes active.
    pub fn upsert(self: *AccountStore, account: ServerAccount) !void {
        const a = self.arena.allocator();
        const duped = try dupAccount(a, account);
        self.active_id = duped.id;
        for (self.accounts.items) |*existing| {
            if (std.mem.eql(u8, existing.id, duped.id)) {
                existing.* = duped;
                return;
            }
        }
        try self.accounts.append(self.gpa, duped);
    }

    /// Remove the active account (sign out). Active becomes the first remaining
    /// account, or none.
    pub fn removeActive(self: *AccountStore) !void {
        if (self.active_id) |aid| {
            var i: usize = 0;
            while (i < self.accounts.items.len) {
                if (std.mem.eql(u8, self.accounts.items[i].id, aid)) {
                    _ = self.accounts.orderedRemove(i);
                } else i += 1;
            }
        } else {
            self.accounts.clearRetainingCapacity();
        }
        self.active_id = if (self.accounts.items.len > 0) self.accounts.items[0].id else null;
        try self.save();
    }

    pub fn get(self: *AccountStore, id: []const u8) ?ServerAccount {
        for (self.accounts.items) |acc| {
            if (std.mem.eql(u8, acc.id, id)) return acc;
        }
        return null;
    }

    pub fn list(self: *AccountStore) []const ServerAccount {
        return self.accounts.items;
    }
};

fn dupAccount(a: std.mem.Allocator, acc: ServerAccount) !ServerAccount {
    return .{
        .id = try a.dupe(u8, acc.id),
        .instance_url = try a.dupe(u8, acc.instance_url),
        .token = if (acc.token) |t| try a.dupe(u8, t) else null,
        .user_id = if (acc.user_id) |t| try a.dupe(u8, t) else null,
        .user_email = if (acc.user_email) |t| try a.dupe(u8, t) else null,
        .user_name = if (acc.user_name) |t| try a.dupe(u8, t) else null,
        .is_admin = acc.is_admin,
    };
}

test "account store JSON round-trips" {
    const a = std.testing.allocator;
    const accts = [_]ServerAccount{.{
        .id = "ab12cd34",
        .instance_url = "https://x.dev",
        .token = "sess_tok",
        .user_email = "e@x.dev",
        .user_name = "Ann",
        .is_admin = true,
    }};
    const data = StoreData{ .active_id = "ab12cd34", .accounts = @constCast(accts[0..]) };

    const json = try std.json.Stringify.valueAlloc(a, data, .{});
    defer a.free(json);

    const parsed = try std.json.parseFromSlice(StoreData, a, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();

    try std.testing.expectEqualStrings("ab12cd34", parsed.value.active_id.?);
    try std.testing.expectEqual(@as(usize, 1), parsed.value.accounts.len);
    try std.testing.expectEqualStrings("https://x.dev", parsed.value.accounts[0].instance_url);
    try std.testing.expectEqualStrings("sess_tok", parsed.value.accounts[0].token.?);
    try std.testing.expect(parsed.value.accounts[0].is_admin);
}
