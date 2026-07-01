//! Pure composition helpers for the "Start coding" launcher (§4a) — the
//! `.mcp.json` document, the plan-first prompt, and the worktree launcher
//! script. GTK-free so they unit-test in the core test root; the launch pipeline
//! (git + terminal + threads) lives in `ui/coding_launcher.zig`.

const std = @import("std");

/// The `.mcp.json` that points Claude at the web MCP server with the user's
/// personal apikey. Caller owns the returned string.
pub fn mcpJson(gpa: std.mem.Allocator, base_url: []const u8, personal_key: []const u8) ![]u8 {
    var arena = std.heap.ArenaAllocator.init(gpa);
    defer arena.deinit();
    const a = arena.allocator();
    const trimmed = std.mem.trimEnd(u8, base_url, "/");
    const url = try std.fmt.allocPrint(a, "{s}/api/mcp", .{trimmed});
    const auth = try std.fmt.allocPrint(a, "Bearer {s}", .{personal_key});
    const doc = .{
        .mcpServers = .{
            .exponential = .{
                .@"type" = "http",
                .url = url,
                .headers = .{ .Authorization = auth },
            },
        },
    };
    return std.json.Stringify.valueAlloc(gpa, doc, .{});
}

/// The plan-first prefilled prompt written to `.exp-prompt.md` in the worktree.
/// Claude is told to propose a plan and WAIT, then implement, push the branch,
/// and open a PR via the MCP `open_pr` tool. Caller owns the result.
pub fn composePrompt(
    gpa: std.mem.Allocator,
    identifier: []const u8,
    title: []const u8,
    description: []const u8,
    branch: []const u8,
) ![]u8 {
    const desc = if (std.mem.trim(u8, description, " \t\r\n").len == 0) "_(no description)_" else description;
    return std.fmt.allocPrint(gpa,
        \\# {s}: {s}
        \\
        \\{s}
        \\
        \\---
        \\## How to work this issue
        \\
        \\You are in a fresh git worktree on branch `{s}`. The `exponential` MCP server
        \\is configured (see `.mcp.json`) — use its tools for issue context and to open the PR.
        \\
        \\1. **Plan first.** Propose a concise implementation plan and WAIT for the user's
        \\   go-ahead before writing any code.
        \\2. **Implement** the change after approval.
        \\3. **Ship it.** Commit your work, push the branch `{s}`, then call the `open_pr`
        \\   MCP tool to open a pull request. You may set the issue status with `update_status`.
        \\
        \\Do not force-push or touch other branches.
        \\
    , .{ identifier, title, desc, branch, branch });
}

/// The tiny launcher script (`.exp-run.sh`) run inside the worktree. Going
/// through `sh` lets us pass the whole prompt as one correctly-quoted argument
/// (`"$(cat …)"`) without depending on how ghostty tokenises its `command`
/// string. Caller owns the result.
pub fn launchScript(gpa: std.mem.Allocator, claude_path: []const u8) ![]u8 {
    return std.fmt.allocPrint(gpa,
        \\#!/bin/sh
        \\exec "{s}" --dangerously-skip-permissions "$(cat .exp-prompt.md)"
        \\
    , .{claude_path});
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

test "mcpJson points at /api/mcp with the bearer key" {
    const a = std.testing.allocator;
    const j = try mcpJson(a, "https://next.exponential.at/", "expu_abc");
    defer a.free(j);
    try std.testing.expect(std.mem.indexOf(u8, j, "\"url\":\"https://next.exponential.at/api/mcp\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, j, "Bearer expu_abc") != null);
    try std.testing.expect(std.mem.indexOf(u8, j, "\"exponential\"") != null);
}

test "composePrompt embeds identifier, branch, and a plan-first instruction" {
    const a = std.testing.allocator;
    const p = try composePrompt(a, "EXP-123", "Fix the thing", "Some **markdown**.", "exp/EXP-123");
    defer a.free(p);
    try std.testing.expect(std.mem.indexOf(u8, p, "EXP-123: Fix the thing") != null);
    try std.testing.expect(std.mem.indexOf(u8, p, "Some **markdown**.") != null);
    try std.testing.expect(std.mem.indexOf(u8, p, "exp/EXP-123") != null);
    try std.testing.expect(std.mem.indexOf(u8, p, "Plan first") != null);
    try std.testing.expect(std.mem.indexOf(u8, p, "open_pr") != null);
}

test "composePrompt tolerates an empty description" {
    const a = std.testing.allocator;
    const p = try composePrompt(a, "EXP-9", "T", "   ", "exp/EXP-9");
    defer a.free(p);
    try std.testing.expect(std.mem.indexOf(u8, p, "_(no description)_") != null);
}

test "launchScript execs claude with the prompt file" {
    const a = std.testing.allocator;
    const s = try launchScript(a, "claude");
    defer a.free(s);
    try std.testing.expect(std.mem.indexOf(u8, s, "exec \"claude\" --dangerously-skip-permissions") != null);
    try std.testing.expect(std.mem.indexOf(u8, s, "cat .exp-prompt.md") != null);
}
