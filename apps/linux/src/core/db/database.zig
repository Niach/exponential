//! Local sync store. Applies Electric shape batches to SQLite generically (no
//! per-table structs): column names come straight from the row JSON, normalised
//! to snake_case, and bound by JSON type. Mirrors the iOS `SyncManager.applyBatch`
//! semantics — crucially, **one transaction per long-poll batch** (never per
//! row), the write-contention rule from packages/electric-protocol/README.md.

const std = @import("std");
const sqlite = @import("sqlite.zig");
const shape = @import("../electric/shape_message.zig");
const migrations = @import("migrations.zig");

/// Minimal spinlock. 0.16's `std.Io.Mutex` requires an `Io` handle; we just need
/// to serialize short DB critical sections across the shape threads (which spend
/// ~60s in HTTP and only milliseconds in the DB), so a spin + hint is plenty.
const Spinlock = struct {
    state: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),

    fn lock(self: *Spinlock) void {
        while (self.state.swap(true, .acquire)) {
            std.atomic.spinLoopHint();
        }
    }
    fn unlock(self: *Spinlock) void {
        self.state.store(false, .release);
    }
};

pub const Database = struct {
    conn: sqlite.Conn,
    gpa: std.mem.Allocator,
    /// Serializes all connection access — the sync engine drives one connection
    /// from N shape threads, so every public DB method takes this lock. (The
    /// `gpa` passed to `open` must itself be thread-safe, e.g. page_allocator.)
    mutex: Spinlock = .{},

    pub fn open(gpa: std.mem.Allocator, path: [:0]const u8) !Database {
        var conn = try sqlite.Conn.open(path);
        errdefer conn.close();
        try conn.exec("PRAGMA journal_mode=WAL;");
        try conn.exec("PRAGMA foreign_keys=ON;");
        try migrations.run(&conn);
        return .{ .conn = conn, .gpa = gpa };
    }

    pub fn close(self: *Database) void {
        self.conn.close();
    }

    /// Apply all messages from one poll inside a single transaction.
    pub fn applyBatch(self: *Database, table: []const u8, messages: []const shape.Message) !void {
        if (messages.len == 0) return;
        self.mutex.lock();
        defer self.mutex.unlock();
        try self.conn.exec("BEGIN IMMEDIATE;");
        errdefer self.conn.exec("ROLLBACK;") catch {};
        for (messages) |m| try self.applyOne(table, m);
        try self.conn.exec("COMMIT;");
    }

    fn applyOne(self: *Database, table: []const u8, m: shape.Message) !void {
        var arena_state = std.heap.ArenaAllocator.init(self.gpa);
        defer arena_state.deinit();
        const arena = arena_state.allocator();

        switch (m.kind) {
            .up_to_date => {},
            .must_refetch => {
                const sql = try std.fmt.allocPrintSentinel(arena, "DELETE FROM \"{s}\";", .{table}, 0);
                try self.conn.exec(sql);
            },
            .insert => try self.upsert(arena, table, m),
            .update => try self.updateRow(arena, table, m),
            .delete => try self.deleteRow(arena, table, m),
        }
    }

    fn upsert(self: *Database, arena: std.mem.Allocator, table: []const u8, m: shape.Message) !void {
        const obj = switch (m.value orelse return) {
            .object => |o| o,
            else => return,
        };
        const pks = pkColumns(table);
        const known = self.tableColumnSet(arena, table);

        var cols: std.ArrayList([]const u8) = .empty;
        var vals: std.ArrayList(std.json.Value) = .empty;
        var it = obj.iterator();
        while (it.next()) |entry| {
            const name = try camelToSnake(arena, entry.key_ptr.*);
            // Skip columns the local table doesn't have. The server may carry
            // columns this client doesn't model (e.g. auth fields on `users`);
            // including them makes the INSERT fail to prepare and drops the row.
            if (!known.contains(name)) continue;
            try cols.append(arena, name);
            try vals.append(arena, entry.value_ptr.*);
        }
        if (cols.items.len == 0) return;

        // "c1", "c2", ...   and   ?, ?, ...
        var quoted = try arena.alloc([]const u8, cols.items.len);
        var holes = try arena.alloc([]const u8, cols.items.len);
        for (cols.items, 0..) |name, i| {
            quoted[i] = try std.fmt.allocPrint(arena, "\"{s}\"", .{name});
            holes[i] = "?";
        }
        const col_list = try std.mem.join(arena, ", ", quoted);
        const hole_list = try std.mem.join(arena, ", ", holes);

        // ON CONFLICT(pk...) DO UPDATE SET "c"=excluded."c" for each non-pk column.
        var sets: std.ArrayList([]const u8) = .empty;
        for (cols.items) |name| {
            if (isPk(pks, name)) continue;
            try sets.append(arena, try std.fmt.allocPrint(arena, "\"{s}\"=excluded.\"{s}\"", .{ name, name }));
        }
        const pk_list = try joinQuoted(arena, pks);

        const sql = if (sets.items.len == 0)
            try std.fmt.allocPrint(
                arena,
                "INSERT INTO \"{s}\" ({s}) VALUES ({s}) ON CONFLICT({s}) DO NOTHING;",
                .{ table, col_list, hole_list, pk_list },
            )
        else
            try std.fmt.allocPrint(
                arena,
                "INSERT INTO \"{s}\" ({s}) VALUES ({s}) ON CONFLICT({s}) DO UPDATE SET {s};",
                .{ table, col_list, hole_list, pk_list, try std.mem.join(arena, ", ", sets.items) },
            );

        var stmt = try self.conn.prepare(sql);
        defer stmt.finalize();
        for (vals.items, 0..) |v, i| try bindValue(arena, &stmt, @intCast(i + 1), v);
        _ = try stmt.step();
    }

    /// The set of column names that exist in `table`, so sync never tries to
    /// write a column the server carries but this client doesn't model (which
    /// would make the statement fail to prepare and drop the entire row).
    fn tableColumnSet(self: *Database, arena: std.mem.Allocator, table: []const u8) std.StringHashMap(void) {
        var set = std.StringHashMap(void).init(arena);
        var buf: [160]u8 = undefined;
        const sql = std.fmt.bufPrintZ(&buf, "PRAGMA table_info(\"{s}\")", .{table}) catch return set;
        var stmt = self.conn.prepare(sql) catch return set;
        defer stmt.finalize();
        while (stmt.step() catch false) {
            const name = arena.dupe(u8, stmt.columnText(1)) catch continue;
            set.put(name, {}) catch {};
        }
        return set;
    }

    fn updateRow(self: *Database, arena: std.mem.Allocator, table: []const u8, m: shape.Message) !void {
        const obj = switch (m.value orelse return) {
            .object => |o| o,
            else => return,
        };
        const pks = pkColumns(table);
        const known = self.tableColumnSet(arena, table);

        // SET only the provided non-PK columns. Updates target rows that already
        // exist (Electric sends "insert" when a row enters the shape), so a plain
        // UPDATE is correct for both full and partial-column updates.
        var set_cols: std.ArrayList([]const u8) = .empty;
        var set_vals: std.ArrayList(std.json.Value) = .empty;
        var it = obj.iterator();
        while (it.next()) |entry| {
            const name = try camelToSnake(arena, entry.key_ptr.*);
            if (isPk(pks, name)) continue;
            if (!known.contains(name)) continue; // skip unmodeled server columns
            try set_cols.append(arena, name);
            try set_vals.append(arena, entry.value_ptr.*);
        }
        if (set_cols.items.len == 0) return;

        var assigns = try arena.alloc([]const u8, set_cols.items.len);
        for (set_cols.items, 0..) |name, i|
            assigns[i] = try std.fmt.allocPrint(arena, "\"{s}\"=?", .{name});

        const where = try self.buildPkWhere(arena, pks, obj, m.id);
        if (where == null) return; // can't locate the row
        const sql = try std.fmt.allocPrint(
            arena,
            "UPDATE \"{s}\" SET {s} WHERE {s};",
            .{ table, try std.mem.join(arena, ", ", assigns), where.?.clause },
        );

        var stmt = try self.conn.prepare(sql);
        defer stmt.finalize();
        var idx: c_int = 1;
        for (set_vals.items) |v| {
            try bindValue(arena, &stmt, idx, v);
            idx += 1;
        }
        for (where.?.values) |v| {
            try bindValue(arena, &stmt, idx, v);
            idx += 1;
        }
        _ = try stmt.step();
    }

    fn deleteRow(self: *Database, arena: std.mem.Allocator, table: []const u8, m: shape.Message) !void {
        const pks = pkColumns(table);
        const obj: ?std.json.ObjectMap = if (m.value) |v| switch (v) {
            .object => |o| o,
            else => null,
        } else null;

        const where = try self.buildPkWhere(arena, pks, obj, m.id);
        if (where == null) return;
        const sql = try std.fmt.allocPrint(arena, "DELETE FROM \"{s}\" WHERE {s};", .{ table, where.?.clause });

        var stmt = try self.conn.prepare(sql);
        defer stmt.finalize();
        for (where.?.values, 0..) |v, i| try bindValue(arena, &stmt, @intCast(i + 1), v);
        _ = try stmt.step();
    }

    const PkWhere = struct { clause: []const u8, values: []std.json.Value };

    /// Build `"pk1"=? AND "pk2"=?` plus the matching values, pulled from the row
    /// object when present, falling back to the bare id parsed from the key for
    /// single-column `id` PKs.
    fn buildPkWhere(
        self: *Database,
        arena: std.mem.Allocator,
        pks: []const []const u8,
        obj: ?std.json.ObjectMap,
        id_from_key: ?[]const u8,
    ) !?PkWhere {
        _ = self;
        var parts = try arena.alloc([]const u8, pks.len);
        var values = try arena.alloc(std.json.Value, pks.len);
        for (pks, 0..) |pk, i| {
            parts[i] = try std.fmt.allocPrint(arena, "\"{s}\"=?", .{pk});
            if (obj) |o| {
                if (o.get(pk)) |v| {
                    values[i] = v;
                    continue;
                }
            }
            if (pks.len == 1 and std.mem.eql(u8, pk, "id")) {
                if (id_from_key) |id| {
                    values[i] = .{ .string = id };
                    continue;
                }
            }
            return null; // missing a PK component
        }
        return .{ .clause = try std.mem.join(arena, " AND ", parts), .values = values };
    }

    // --- helpers used by tests / callers ---

    pub fn count(self: *Database, table: []const u8) !i64 {
        self.mutex.lock();
        defer self.mutex.unlock();
        var buf: [128]u8 = undefined;
        const sql = try std.fmt.bufPrint(&buf, "SELECT COUNT(*) FROM \"{s}\";", .{table});
        var stmt = try self.conn.prepare(sql);
        defer stmt.finalize();
        _ = try stmt.step();
        return stmt.columnInt(0);
    }

    pub const IssueRow = struct {
        id: [:0]const u8,
        identifier: [:0]const u8,
        title: [:0]const u8,
        status: [:0]const u8,
        priority: [:0]const u8,
        due_date: [:0]const u8, // "" when none
        assignee: [:0]const u8, // display name/email, "" when unassigned
        recurrence_interval: i64, // 0 when not recurring
    };

    pub const ProjectRow = struct {
        id: [:0]const u8,
        name: [:0]const u8,
        workspace_id: [:0]const u8,
        github_repo: [:0]const u8, // "" when none
        color: [:0]const u8, // "#rrggbb"; defaults to indigo when unset
    };

    /// Issues for the tracker list. Filtered to one project when `project_id` is
    /// set; otherwise to one workspace's projects when `workspace_id` is set;
    /// otherwise all. Strings are arena-allocated + NUL-terminated for GTK.
    pub fn listIssues(self: *Database, arena: std.mem.Allocator, project_id: ?[]const u8, workspace_id: ?[]const u8, limit: i64) ![]IssueRow {
        self.mutex.lock();
        defer self.mutex.unlock();
        const cols = "SELECT i.id, i.identifier, i.title, i.status, i.priority, COALESCE(i.due_date,''), " ++
            "COALESCE(u.name, u.email, ''), COALESCE(i.recurrence_interval, 0) FROM issues i LEFT JOIN users u ON u.id = i.assignee_id ";
        const tail = " ORDER BY i.status, i.sort_order LIMIT ?;";
        var stmt = if (project_id != null)
            try self.conn.prepare(cols ++ "WHERE i.project_id = ?" ++ tail)
        else if (workspace_id != null)
            try self.conn.prepare(cols ++ "WHERE i.project_id IN (SELECT id FROM projects WHERE workspace_id = ?)" ++ tail)
        else
            try self.conn.prepare(cols ++ tail);
        defer stmt.finalize();
        if (project_id) |pid| {
            try stmt.bindText(1, pid);
            try stmt.bindInt(2, limit);
        } else if (workspace_id) |wid| {
            try stmt.bindText(1, wid);
            try stmt.bindInt(2, limit);
        } else {
            try stmt.bindInt(1, limit);
        }

        var rows: std.ArrayList(IssueRow) = .empty;
        while (try stmt.step()) {
            try rows.append(arena, .{
                .id = try arena.dupeZ(u8, stmt.columnText(0)),
                .identifier = try arena.dupeZ(u8, stmt.columnText(1)),
                .title = try arena.dupeZ(u8, stmt.columnText(2)),
                .status = try arena.dupeZ(u8, stmt.columnText(3)),
                .priority = try arena.dupeZ(u8, stmt.columnText(4)),
                .due_date = try arena.dupeZ(u8, stmt.columnText(5)),
                .assignee = try arena.dupeZ(u8, stmt.columnText(6)),
                .recurrence_interval = stmt.columnInt(7),
            });
        }
        return rows.toOwnedSlice(arena);
    }

    pub const LabelChip = struct {
        issue_id: [:0]const u8,
        label_id: [:0]const u8,
        name: [:0]const u8,
        color: [:0]const u8,
    };

    /// Every (issue, label) pairing in one query — callers bucket by issue_id to
    /// avoid an N+1 per visible row.
    pub fn listAllIssueLabels(self: *Database, arena: std.mem.Allocator) ![]LabelChip {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = try self.conn.prepare(
            "SELECT il.issue_id, il.label_id, l.name, l.color FROM issue_labels il " ++
                "JOIN labels l ON l.id = il.label_id ORDER BY l.sort_order, l.name;",
        );
        defer stmt.finalize();
        var rows: std.ArrayList(LabelChip) = .empty;
        while (try stmt.step()) {
            try rows.append(arena, .{
                .issue_id = try arena.dupeZ(u8, stmt.columnText(0)),
                .label_id = try arena.dupeZ(u8, stmt.columnText(1)),
                .name = try arena.dupeZ(u8, stmt.columnText(2)),
                .color = try arena.dupeZ(u8, stmt.columnText(3)),
            });
        }
        return rows.toOwnedSlice(arena);
    }

    pub const LabelRow = struct {
        id: [:0]const u8,
        name: [:0]const u8,
        color: [:0]const u8,
    };

    /// Workspace labels for the picker.
    pub fn listLabels(self: *Database, arena: std.mem.Allocator, workspace_id: []const u8) ![]LabelRow {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = try self.conn.prepare("SELECT id, name, color FROM labels WHERE workspace_id = ? ORDER BY sort_order, name;");
        defer stmt.finalize();
        try stmt.bindText(1, workspace_id);
        var rows: std.ArrayList(LabelRow) = .empty;
        while (try stmt.step()) {
            try rows.append(arena, .{
                .id = try arena.dupeZ(u8, stmt.columnText(0)),
                .name = try arena.dupeZ(u8, stmt.columnText(1)),
                .color = try arena.dupeZ(u8, stmt.columnText(2)),
            });
        }
        return rows.toOwnedSlice(arena);
    }

    /// Label ids attached to one issue (for prefilling the picker).
    pub fn listIssueLabelIds(self: *Database, arena: std.mem.Allocator, issue_id: []const u8) ![][:0]const u8 {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = try self.conn.prepare("SELECT label_id FROM issue_labels WHERE issue_id = ?;");
        defer stmt.finalize();
        try stmt.bindText(1, issue_id);
        var rows: std.ArrayList([:0]const u8) = .empty;
        while (try stmt.step()) try rows.append(arena, try arena.dupeZ(u8, stmt.columnText(0)));
        return rows.toOwnedSlice(arena);
    }

    pub const UserRow = struct {
        id: [:0]const u8,
        name: [:0]const u8,
    };

    /// Known users (for the assignee picker).
    pub fn listUsers(self: *Database, arena: std.mem.Allocator) ![]UserRow {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = try self.conn.prepare("SELECT id, COALESCE(name, email, id) FROM users ORDER BY COALESCE(name, email, id);");
        defer stmt.finalize();
        var rows: std.ArrayList(UserRow) = .empty;
        while (try stmt.step()) {
            try rows.append(arena, .{
                .id = try arena.dupeZ(u8, stmt.columnText(0)),
                .name = try arena.dupeZ(u8, stmt.columnText(1)),
            });
        }
        return rows.toOwnedSlice(arena);
    }

    pub const IssueDetail = struct {
        project_id: [:0]const u8,
        workspace_id: [:0]const u8,
        identifier: [:0]const u8,
        title: [:0]const u8,
        status: [:0]const u8,
        priority: [:0]const u8,
        due_date: [:0]const u8, // "" when none
        assignee_id: [:0]const u8, // "" when unassigned
        assignee: [:0]const u8, // display name/email, "" when unassigned
        recurrence_interval: i64, // 0 when not recurring
        recurrence_unit: [:0]const u8, // "" when not recurring
        agent_plan_state: [:0]const u8, // "" / drafting / awaiting_answer / awaiting_approval / approved
        agent_plan_approver: [:0]const u8, // display name of who approved ("" when none)
        description: [:0]const u8, // markdown text extracted from the jsonb {text}
        pr_url: [:0]const u8, // the agent's PR ("" when none) — drives the Changes button
        pr_state: [:0]const u8, // open/closed/merged/draft ("" when none)
    };

    pub fn getIssue(self: *Database, arena: std.mem.Allocator, id: []const u8) !?IssueDetail {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = try self.conn.prepare(
            "SELECT i.project_id, p.workspace_id, i.identifier, i.title, i.status, i.priority, " ++
                "COALESCE(i.due_date,''), COALESCE(i.assignee_id,''), COALESCE(u.name, u.email, ''), " ++
                "COALESCE(i.recurrence_interval, 0), COALESCE(i.recurrence_unit, ''), " ++
                "COALESCE(i.agent_plan_state, ''), COALESCE(ap.name, ap.email, ''), i.description, " ++
                "COALESCE(i.pr_url, ''), COALESCE(i.pr_state, '') " ++
                "FROM issues i JOIN projects p ON p.id = i.project_id " ++
                "LEFT JOIN users u ON u.id = i.assignee_id " ++
                "LEFT JOIN users ap ON ap.id = i.agent_plan_approved_by WHERE i.id = ?;",
        );
        defer stmt.finalize();
        try stmt.bindText(1, id);
        if (try stmt.step()) {
            return .{
                .project_id = try arena.dupeZ(u8, stmt.columnText(0)),
                .workspace_id = try arena.dupeZ(u8, stmt.columnText(1)),
                .identifier = try arena.dupeZ(u8, stmt.columnText(2)),
                .title = try arena.dupeZ(u8, stmt.columnText(3)),
                .status = try arena.dupeZ(u8, stmt.columnText(4)),
                .priority = try arena.dupeZ(u8, stmt.columnText(5)),
                .due_date = try arena.dupeZ(u8, stmt.columnText(6)),
                .assignee_id = try arena.dupeZ(u8, stmt.columnText(7)),
                .assignee = try arena.dupeZ(u8, stmt.columnText(8)),
                .recurrence_interval = stmt.columnInt(9),
                .recurrence_unit = try arena.dupeZ(u8, stmt.columnText(10)),
                .agent_plan_state = try arena.dupeZ(u8, stmt.columnText(11)),
                .agent_plan_approver = try arena.dupeZ(u8, stmt.columnText(12)),
                .description = try extractJsonText(arena, stmt.columnText(13)),
                .pr_url = try arena.dupeZ(u8, stmt.columnText(14)),
                .pr_state = try arena.dupeZ(u8, stmt.columnText(15)),
            };
        }
        return null;
    }

    pub const CommentRow = struct {
        author: [:0]const u8, // display name (joined from users), falling back to id
        body: [:0]const u8,
        kind: [:0]const u8,
        created_at: [:0]const u8,
    };

    pub fn listComments(self: *Database, arena: std.mem.Allocator, issue_id: []const u8) ![]CommentRow {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = try self.conn.prepare(
            "SELECT COALESCE(u.name, u.email, c.author_id), c.body, c.kind, c.created_at FROM comments c " ++
                "LEFT JOIN users u ON u.id = c.author_id WHERE c.issue_id = ? ORDER BY c.created_at;",
        );
        defer stmt.finalize();
        try stmt.bindText(1, issue_id);

        var rows: std.ArrayList(CommentRow) = .empty;
        while (try stmt.step()) {
            try rows.append(arena, .{
                .author = try arena.dupeZ(u8, stmt.columnText(0)),
                .body = try extractJsonText(arena, stmt.columnText(1)),
                .kind = try arena.dupeZ(u8, stmt.columnText(2)),
                .created_at = try arena.dupeZ(u8, stmt.columnText(3)),
            });
        }
        return rows.toOwnedSlice(arena);
    }

    pub const IssueEventRow = struct {
        actor: [:0]const u8, // display name (joined), "" when system/unknown
        type: [:0]const u8,
        created_at: [:0]const u8,
    };

    /// Synced activity events for an issue (status/assignee/label/PR/plan/error),
    /// for the Linear-style activity timeline merged with comments.
    pub fn listIssueEvents(self: *Database, arena: std.mem.Allocator, issue_id: []const u8) ![]IssueEventRow {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = try self.conn.prepare(
            "SELECT COALESCE(u.name, u.email, ''), e.type, e.created_at FROM issue_events e " ++
                "LEFT JOIN users u ON u.id = e.actor_user_id WHERE e.issue_id = ? ORDER BY e.created_at;",
        );
        defer stmt.finalize();
        try stmt.bindText(1, issue_id);

        var rows: std.ArrayList(IssueEventRow) = .empty;
        while (try stmt.step()) {
            try rows.append(arena, .{
                .actor = try arena.dupeZ(u8, stmt.columnText(0)),
                .type = try arena.dupeZ(u8, stmt.columnText(1)),
                .created_at = try arena.dupeZ(u8, stmt.columnText(2)),
            });
        }
        return rows.toOwnedSlice(arena);
    }

    /// Projects for the sidebar (active first, archived hidden), optionally
    /// scoped to one workspace.
    pub fn listProjects(self: *Database, arena: std.mem.Allocator, workspace_id: ?[]const u8) ![]ProjectRow {
        self.mutex.lock();
        defer self.mutex.unlock();
        const cols = "SELECT id, name, workspace_id, COALESCE(github_repo,''), COALESCE(NULLIF(color,''),'#6366f1') FROM projects WHERE archived_at IS NULL";
        var stmt = if (workspace_id != null)
            try self.conn.prepare(cols ++ " AND workspace_id = ? ORDER BY sort_order, name;")
        else
            try self.conn.prepare(cols ++ " ORDER BY sort_order, name;");
        defer stmt.finalize();
        if (workspace_id) |wid| try stmt.bindText(1, wid);

        var rows: std.ArrayList(ProjectRow) = .empty;
        while (try stmt.step()) {
            try rows.append(arena, .{
                .id = try arena.dupeZ(u8, stmt.columnText(0)),
                .name = try arena.dupeZ(u8, stmt.columnText(1)),
                .workspace_id = try arena.dupeZ(u8, stmt.columnText(2)),
                .github_repo = try arena.dupeZ(u8, stmt.columnText(3)),
                .color = try arena.dupeZ(u8, stmt.columnText(4)),
            });
        }
        return rows.toOwnedSlice(arena);
    }

    /// First workspace id seen (for create-project / label defaults when no
    /// project context is available). null when none synced yet.
    pub fn firstWorkspaceId(self: *Database, arena: std.mem.Allocator) !?[:0]const u8 {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = try self.conn.prepare("SELECT id FROM workspaces ORDER BY created_at LIMIT 1;");
        defer stmt.finalize();
        if (try stmt.step()) return try arena.dupeZ(u8, stmt.columnText(0));
        return null;
    }

    pub const WorkspaceRow = struct {
        id: [:0]const u8,
        name: [:0]const u8,
        is_public: bool,
        public_write_policy: [:0]const u8,
    };

    pub fn listWorkspaces(self: *Database, arena: std.mem.Allocator) ![]WorkspaceRow {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = try self.conn.prepare("SELECT id, name, COALESCE(is_public,0), COALESCE(public_write_policy,'members') FROM workspaces ORDER BY created_at;");
        defer stmt.finalize();
        var rows: std.ArrayList(WorkspaceRow) = .empty;
        while (try stmt.step()) {
            try rows.append(arena, .{
                .id = try arena.dupeZ(u8, stmt.columnText(0)),
                .name = try arena.dupeZ(u8, stmt.columnText(1)),
                .is_public = stmt.columnInt(2) != 0,
                .public_write_policy = try arena.dupeZ(u8, stmt.columnText(3)),
            });
        }
        return rows.toOwnedSlice(arena);
    }

    /// Whether `user_id` is a member of `workspace_id` (drives moderation gating
    /// on public workspaces, mirroring use-workspace-permissions).
    pub fn isWorkspaceMember(self: *Database, workspace_id: []const u8, user_id: []const u8) bool {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = self.conn.prepare("SELECT 1 FROM workspace_members WHERE workspace_id = ? AND user_id = ? LIMIT 1;") catch return false;
        defer stmt.finalize();
        stmt.bindText(1, workspace_id) catch return false;
        stmt.bindText(2, user_id) catch return false;
        return stmt.step() catch false;
    }

    /// Whether a workspace is public (members-only moderation when it is).
    pub fn isWorkspacePublic(self: *Database, workspace_id: []const u8) bool {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = self.conn.prepare("SELECT COALESCE(is_public,0) FROM workspaces WHERE id = ?;") catch return false;
        defer stmt.finalize();
        stmt.bindText(1, workspace_id) catch return false;
        if (stmt.step() catch false) return stmt.columnInt(0) != 0;
        return false;
    }

    pub const MemberRow = struct {
        id: [:0]const u8,
        user_id: [:0]const u8,
        role: [:0]const u8,
        name: [:0]const u8, // display name/email
        email: [:0]const u8,
    };

    pub fn listMembers(self: *Database, arena: std.mem.Allocator, workspace_id: []const u8) ![]MemberRow {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = try self.conn.prepare(
            "SELECT wm.id, wm.user_id, wm.role, COALESCE(u.name, u.email, wm.user_id), COALESCE(u.email,'') " ++
                "FROM workspace_members wm LEFT JOIN users u ON u.id = wm.user_id " ++
                "WHERE wm.workspace_id = ? ORDER BY wm.role, COALESCE(u.name, u.email, wm.user_id);",
        );
        defer stmt.finalize();
        try stmt.bindText(1, workspace_id);
        var rows: std.ArrayList(MemberRow) = .empty;
        while (try stmt.step()) {
            try rows.append(arena, .{
                .id = try arena.dupeZ(u8, stmt.columnText(0)),
                .user_id = try arena.dupeZ(u8, stmt.columnText(1)),
                .role = try arena.dupeZ(u8, stmt.columnText(2)),
                .name = try arena.dupeZ(u8, stmt.columnText(3)),
                .email = try arena.dupeZ(u8, stmt.columnText(4)),
            });
        }
        return rows.toOwnedSlice(arena);
    }

    pub const InviteRow = struct {
        id: [:0]const u8,
        role: [:0]const u8,
        token: [:0]const u8,
        expires_at: [:0]const u8,
    };

    /// Pending (not-yet-accepted) invites for a workspace.
    pub fn listInvites(self: *Database, arena: std.mem.Allocator, workspace_id: []const u8) ![]InviteRow {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = try self.conn.prepare(
            "SELECT id, role, token, COALESCE(expires_at,'') FROM workspace_invites " ++
                "WHERE workspace_id = ? AND accepted_at IS NULL ORDER BY created_at DESC;",
        );
        defer stmt.finalize();
        try stmt.bindText(1, workspace_id);
        var rows: std.ArrayList(InviteRow) = .empty;
        while (try stmt.step()) {
            try rows.append(arena, .{
                .id = try arena.dupeZ(u8, stmt.columnText(0)),
                .role = try arena.dupeZ(u8, stmt.columnText(1)),
                .token = try arena.dupeZ(u8, stmt.columnText(2)),
                .expires_at = try arena.dupeZ(u8, stmt.columnText(3)),
            });
        }
        return rows.toOwnedSlice(arena);
    }

    pub fn setOffset(self: *Database, shape_name: []const u8, handle: []const u8, offset: []const u8) !void {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = try self.conn.prepare(
            "INSERT INTO electric_offsets (shape, handle, \"offset\") VALUES (?,?,?) " ++
                "ON CONFLICT(shape) DO UPDATE SET handle=excluded.handle, \"offset\"=excluded.\"offset\";",
        );
        defer stmt.finalize();
        try stmt.bindText(1, shape_name);
        try stmt.bindText(2, handle);
        try stmt.bindText(3, offset);
        _ = try stmt.step();
    }

    pub const Cursor = struct { handle: []const u8, offset: []const u8 };

    /// Returns the persisted cursor for a shape (allocated in `arena`), or null
    /// when none exists (→ initial snapshot with offset=-1).
    pub fn getOffset(self: *Database, arena: std.mem.Allocator, shape_name: []const u8) !?Cursor {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = try self.conn.prepare("SELECT handle, \"offset\" FROM electric_offsets WHERE shape = ?;");
        defer stmt.finalize();
        try stmt.bindText(1, shape_name);
        if (try stmt.step()) {
            return .{
                .handle = try arena.dupe(u8, stmt.columnText(0)),
                .offset = try arena.dupe(u8, stmt.columnText(1)),
            };
        }
        return null;
    }

    pub fn deleteOffset(self: *Database, shape_name: []const u8) !void {
        self.mutex.lock();
        defer self.mutex.unlock();
        var stmt = try self.conn.prepare("DELETE FROM electric_offsets WHERE shape = ?;");
        defer stmt.finalize();
        try stmt.bindText(1, shape_name);
        _ = try stmt.step();
    }
};

fn pkColumns(table: []const u8) []const []const u8 {
    if (std.mem.eql(u8, table, "issue_labels")) return &.{ "issue_id", "label_id" };
    return &.{"id"};
}

fn isPk(pks: []const []const u8, name: []const u8) bool {
    for (pks) |pk| if (std.mem.eql(u8, pk, name)) return true;
    return false;
}

fn joinQuoted(arena: std.mem.Allocator, names: []const []const u8) ![]u8 {
    var quoted = try arena.alloc([]const u8, names.len);
    for (names, 0..) |name, i| quoted[i] = try std.fmt.allocPrint(arena, "\"{s}\"", .{name});
    return std.mem.join(arena, ", ", quoted);
}

/// Electric sends snake_case, but some server paths rewrite to camelCase; the
/// schema is snake_case, so normalise here. Borrow when already snake_case.
fn camelToSnake(arena: std.mem.Allocator, key: []const u8) ![]const u8 {
    var uppers: usize = 0;
    for (key) |ch| {
        if (ch >= 'A' and ch <= 'Z') uppers += 1;
    }
    if (uppers == 0) return key;
    var buf = try arena.alloc(u8, key.len + uppers);
    var j: usize = 0;
    for (key) |ch| {
        if (ch >= 'A' and ch <= 'Z') {
            buf[j] = '_';
            buf[j + 1] = ch - 'A' + 'a';
            j += 2;
        } else {
            buf[j] = ch;
            j += 1;
        }
    }
    return buf[0..j];
}

fn bindValue(arena: std.mem.Allocator, stmt: *sqlite.Stmt, idx: c_int, v: std.json.Value) !void {
    switch (v) {
        .null => try stmt.bindNull(idx),
        .bool => |b| try stmt.bindInt(idx, if (b) 1 else 0),
        .integer => |i| try stmt.bindInt(idx, i),
        .float => |f| try stmt.bindDouble(idx, f),
        .number_string => |s| try stmt.bindText(idx, s),
        .string => |s| try stmt.bindText(idx, s),
        // jsonb columns (description, body): store the serialized JSON text.
        .array, .object => try stmt.bindText(idx, try std.json.Stringify.valueAlloc(arena, v, .{})),
    }
}

/// Pull the markdown out of a jsonb-stored `{ "text": "…" }` value; falls back
/// to the raw string if it isn't that shape (or is empty).
fn extractJsonText(arena: std.mem.Allocator, stored: []const u8) ![:0]const u8 {
    if (stored.len == 0) return arena.dupeZ(u8, "");
    var parsed = std.json.parseFromSlice(std.json.Value, arena, stored, .{}) catch
        return arena.dupeZ(u8, stored);
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return arena.dupeZ(u8, stored),
    };
    const t = obj.get("text") orelse return arena.dupeZ(u8, "");
    return switch (t) {
        .string => |s| try arena.dupeZ(u8, s),
        else => try arena.dupeZ(u8, ""),
    };
}

// ---------------------------------------------------------------------------
// Tests — apply real shape batches to an in-memory DB and assert the resulting
// rows. Proves parse → persist and the one-transaction-per-batch invariant.
// ---------------------------------------------------------------------------

const insert_body =
    \\[
    \\ {"headers":{"operation":"insert"},"key":"\"issues\"/\"A\"","value":{"id":"A","project_id":"P","number":1,"identifier":"EXP-1","title":"First","description":{"text":"body"},"status":"todo","priority":"medium","creator_id":"u1","sort_order":1.0,"created_at":"t0","updated_at":"t0"}},
    \\ {"headers":{"operation":"insert"},"key":"\"issues\"/\"B\"","value":{"id":"B","project_id":"P","number":2,"identifier":"EXP-2","title":"Second","status":"in_progress","priority":"high","creator_id":"u1","sort_order":2.0,"created_at":"t0","updated_at":"t0"}},
    \\ {"headers":{"control":"up-to-date"}}
    \\]
;

const update_delete_body =
    \\[
    \\ {"headers":{"operation":"update"},"key":"\"issues\"/\"A\"","value":{"id":"A","title":"First (renamed)","status":"in_progress"}},
    \\ {"headers":{"operation":"delete"},"key":"\"issues\"/\"B\"","value":{"id":"B"}},
    \\ {"headers":{"control":"up-to-date"}}
    \\]
;

const refetch_body =
    \\[ {"headers":{"control":"must-refetch"}} ]
;

fn titleOf(db: *Database, a: std.mem.Allocator, id: []const u8) ![]u8 {
    var stmt = try db.conn.prepare("SELECT title FROM issues WHERE id = ?;");
    defer stmt.finalize();
    try stmt.bindText(1, id);
    _ = try stmt.step();
    return a.dupe(u8, stmt.columnText(0));
}

test "applyBatch: insert, update, delete, must-refetch (one txn per batch)" {
    const a = std.testing.allocator;
    var db = try Database.open(a, ":memory:");
    defer db.close();

    {
        var batch = try shape.parse(a, insert_body);
        defer batch.deinit();
        try db.applyBatch("issues", batch.messages);
    }
    try std.testing.expectEqual(@as(i64, 2), try db.count("issues"));

    {
        var batch = try shape.parse(a, update_delete_body);
        defer batch.deinit();
        try db.applyBatch("issues", batch.messages);
    }
    try std.testing.expectEqual(@as(i64, 1), try db.count("issues"));
    const title = try titleOf(&db, a, "A");
    defer a.free(title);
    try std.testing.expectEqualStrings("First (renamed)", title);

    {
        var batch = try shape.parse(a, refetch_body);
        defer batch.deinit();
        try db.applyBatch("issues", batch.messages);
    }
    try std.testing.expectEqual(@as(i64, 0), try db.count("issues"));
}

test "electric offset cursor round-trips" {
    const a = std.testing.allocator;
    var db = try Database.open(a, ":memory:");
    defer db.close();

    var arena = std.heap.ArenaAllocator.init(a);
    defer arena.deinit();

    try std.testing.expect((try db.getOffset(arena.allocator(), "issues")) == null);
    try db.setOffset("issues", "h-1", "0_0");
    try db.setOffset("issues", "h-1", "0_5"); // upsert
    const cur = (try db.getOffset(arena.allocator(), "issues")).?;
    try std.testing.expectEqualStrings("h-1", cur.handle);
    try std.testing.expectEqualStrings("0_5", cur.offset);

    try db.deleteOffset("issues");
    try std.testing.expect((try db.getOffset(arena.allocator(), "issues")) == null);
}

test "concurrent applyBatch from two threads is serialized (no corruption)" {
    // page_allocator is thread-safe; two shape threads hammer the one connection.
    const alloc = std.heap.page_allocator;
    var db = try Database.open(alloc, ":memory:");
    defer db.close();

    const Worker = struct {
        fn run(d: *Database) void {
            var i: usize = 0;
            while (i < 100) : (i += 1) {
                var batch = shape.parse(std.heap.page_allocator, insert_body) catch return;
                defer batch.deinit();
                d.applyBatch("issues", batch.messages) catch return;
            }
        }
    };

    const t1 = try std.Thread.spawn(.{}, Worker.run, .{&db});
    const t2 = try std.Thread.spawn(.{}, Worker.run, .{&db});
    t1.join();
    t2.join();

    // Idempotent upserts of ids A and B from both threads → exactly 2 rows, and
    // no "transaction within a transaction" / corruption from interleaving.
    try std.testing.expectEqual(@as(i64, 2), try db.count("issues"));
}
