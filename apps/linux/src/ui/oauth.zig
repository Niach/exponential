//! OAuth deep-link helpers (pure; no GTK so the core tests can cover it).
//!
//! The server's /api/mobile-oauth-return redirects to
//! `exp://oauth-return#token=<urlencoded session token>`. We register `exp://`
//! as a scheme handler; the browser hands us that URI as argv, and we extract
//! and percent-decode the token.

const std = @import("std");

/// Extract + percent-decode the `token=` value from an `exp://…` deep link.
/// Returns an owned slice, or null if no token is present.
pub fn parseDeepLinkToken(allocator: std.mem.Allocator, uri: []const u8) !?[]u8 {
    if (!std.mem.startsWith(u8, uri, "exp://")) return null;
    const marker = "token=";
    const at = std.mem.indexOf(u8, uri, marker) orelse return null;
    var rest = uri[at + marker.len ..];
    if (std.mem.indexOfAny(u8, rest, "&\r\n \t")) |end| rest = rest[0..end];
    if (rest.len == 0) return null;
    return try percentDecode(allocator, rest);
}

fn percentDecode(allocator: std.mem.Allocator, s: []const u8) ![]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(allocator);
    var i: usize = 0;
    while (i < s.len) {
        const ch = s[i];
        if (ch == '%' and i + 2 < s.len) {
            const hi = std.fmt.charToDigit(s[i + 1], 16) catch {
                try out.append(allocator, ch);
                i += 1;
                continue;
            };
            const lo = std.fmt.charToDigit(s[i + 2], 16) catch {
                try out.append(allocator, ch);
                i += 1;
                continue;
            };
            try out.append(allocator, @intCast(hi * 16 + lo));
            i += 3;
        } else if (ch == '+') {
            try out.append(allocator, ' ');
            i += 1;
        } else {
            try out.append(allocator, ch);
            i += 1;
        }
    }
    return out.toOwnedSlice(allocator);
}

test "parseDeepLinkToken extracts and percent-decodes" {
    const a = std.testing.allocator;
    {
        const t = (try parseDeepLinkToken(a, "exp://oauth-return#token=abc.def-123")).?;
        defer a.free(t);
        try std.testing.expectEqualStrings("abc.def-123", t);
    }
    {
        const t = (try parseDeepLinkToken(a, "exp://oauth-return#token=a%2Eb%2Dc")).?;
        defer a.free(t);
        try std.testing.expectEqualStrings("a.b-c", t);
    }
    {
        // stops at an ampersand
        const t = (try parseDeepLinkToken(a, "exp://oauth-return#token=xyz&foo=1")).?;
        defer a.free(t);
        try std.testing.expectEqualStrings("xyz", t);
    }
    try std.testing.expect((try parseDeepLinkToken(a, "https://example.com")) == null);
    try std.testing.expect((try parseDeepLinkToken(a, "exp://oauth-return")) == null);
}
