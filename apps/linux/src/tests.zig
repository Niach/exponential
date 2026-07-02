//! Test root — imports only the core (non-GUI) modules so `zig build test` does
//! not depend on GTK. The GUI lives behind `main.zig` → `ui/`.

test {
    _ = @import("core/electric/shape_message.zig");
    _ = @import("core/electric/shape_client.zig");
    _ = @import("core/electric/sync_manager.zig");
    _ = @import("core/api/http.zig");
    _ = @import("core/api/trpc.zig");
    _ = @import("core/api/mutate.zig");
    _ = @import("core/auth/server_account.zig");
    _ = @import("core/auth/auth_api.zig");
    _ = @import("core/auth/account_store.zig");
    _ = @import("core/credentials.zig");
    _ = @import("core/git_worktree.zig");
    _ = @import("core/coding_prompt.zig");
    _ = @import("core/steer/util.zig");
    _ = @import("core/steer/protocol.zig");
    _ = @import("core/steer/ws_client.zig");
    _ = @import("core/steer/host_pty.zig"); // real-PTY tests (VM has /bin/sh)
    _ = @import("core/steer/control_channel.zig");
    _ = @import("core/steer/publisher.zig");
    _ = @import("core/db/database.zig");
    _ = @import("core/db/migrations.zig");
    _ = @import("core/diff.zig"); // unified patch → side-by-side hunk model (§4e)
    _ = @import("core/annotate/geometry.zig");
    _ = @import("ui/preview/preview_config.zig"); // GTK-free parse + trust + doctor
    _ = @import("ui/oauth.zig");
    _ = @import("ui/format.zig"); // GTK-free presentation helpers
    _ = @import("ui/tab_registry.zig"); // GTK-free terminal-dock tab bookkeeping (§4d)
    _ = @import("ui/run_script.zig"); // GTK-free run-config script + history (§4c)
}
