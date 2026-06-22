//! Project-scoped tRPC mutations the desktop app issues directly (alongside the
//! generic `mutate.fire` fire-and-forget path). Currently just the preview-config
//! mirror writer used by the Settings "Run Targets & Preview" expander.

const std = @import("std");
const trpc = @import("trpc.zig");

/// A run-target entry in the DB mirror — display-only metadata (id/name/platform),
/// never the executable commands (those live in the repo file).
pub const MirrorTarget = struct {
    id: []const u8,
    name: []const u8,
    platform: []const u8,
};

/// Write the `projects.preview_config` DB mirror via `projects.updatePreviewConfig`
/// (owner-gated server-side; uses the Electric `generateTxId` pattern there). The
/// mirror holds ONLY display metadata — the run-target list + the feedback issue
/// routing target — and is never executed. Blocking (a deliberate settings
/// action); returns true on a 2xx, non-error tRPC response.
///
/// `targets` is typically auto-populated by the desktop after it clones + parses
/// the repo file; the Settings expander usually just edits `feedback_project_id`
/// and re-sends the current target list.
pub fn updatePreviewConfig(
    gpa: std.mem.Allocator,
    instance: []const u8,
    token: ?[]const u8,
    project_id: []const u8,
    targets: []const MirrorTarget,
    feedback_project_id: ?[]const u8,
) bool {
    var arena = std.heap.ArenaAllocator.init(gpa);
    defer arena.deinit();
    const a = arena.allocator();

    const input = std.json.Stringify.valueAlloc(a, .{
        .projectId = project_id,
        .previewConfig = .{
            .targets = targets,
            // null serializes as JSON null → the server clears the routing target.
            .feedbackProjectId = feedback_project_id,
        },
    }, .{ .emit_null_optional_fields = true }) catch return false;

    var resp = trpc.call(gpa, instance, "projects.updatePreviewConfig", input, token, 30) catch return false;
    defer resp.deinit();
    return resp.ok();
}
