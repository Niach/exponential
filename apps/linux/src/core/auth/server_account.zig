//! A signed-in instance account — mirrors the iOS `ServerAccount`. The stable id
//! is the first 4 bytes of sha256(instanceUrl) as hex (8 chars), so every client
//! derives the same per-instance id.

const std = @import("std");

pub const ServerAccount = struct {
    id: []const u8,
    instance_url: []const u8,
    token: ?[]const u8 = null,
    user_id: ?[]const u8 = null,
    user_email: ?[]const u8 = null,
    user_name: ?[]const u8 = null,
    is_admin: bool = false,

    /// sha256(instanceUrl) → first 4 bytes → lowercase hex (8 chars).
    pub fn makeId(allocator: std.mem.Allocator, instance_url: []const u8) ![]u8 {
        var digest: [32]u8 = undefined;
        std.crypto.hash.sha2.Sha256.hash(instance_url, &digest, .{});
        const hex = "0123456789abcdef";
        var out: [8]u8 = undefined;
        for (digest[0..4], 0..) |b, i| {
            out[i * 2] = hex[b >> 4];
            out[i * 2 + 1] = hex[b & 0x0f];
        }
        return allocator.dupe(u8, &out);
    }
};

test "makeId matches the iOS derivation (sha256 prefix, 8 hex chars)" {
    const a = std.testing.allocator;
    const id = try ServerAccount.makeId(a, "https://next.exponential.at");
    defer a.free(id);
    try std.testing.expectEqual(@as(usize, 8), id.len);
    // Stable + deterministic for the same URL.
    const id2 = try ServerAccount.makeId(a, "https://next.exponential.at");
    defer a.free(id2);
    try std.testing.expectEqualStrings(id, id2);
    // Different URL → different id.
    const other = try ServerAccount.makeId(a, "http://localhost:5173");
    defer a.free(other);
    try std.testing.expect(!std.mem.eql(u8, id, other));
    // All hex.
    for (id) |ch| try std.testing.expect(std.ascii.isHex(ch));
}
