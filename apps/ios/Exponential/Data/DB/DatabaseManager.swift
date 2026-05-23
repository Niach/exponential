import Foundation
import GRDB

final class DatabaseManager: Sendable {
    let dbPool: DatabasePool

    init() {
        do {
            let fileManager = FileManager.default
            let appSupportDir = try fileManager.url(
                for: .applicationSupportDirectory,
                in: .userDomainMask,
                appropriateFor: nil,
                create: true
            )
            let dbDir = appSupportDir.appendingPathComponent("Exponential", isDirectory: true)
            try fileManager.createDirectory(at: dbDir, withIntermediateDirectories: true)
            let dbPath = dbDir.appendingPathComponent("exponential.sqlite").path

            var config = Configuration()
            config.foreignKeysEnabled = true
            config.journalMode = .wal

            dbPool = try DatabasePool(path: dbPath, configuration: config)
            try runMigrations()
        } catch {
            fatalError("Failed to initialize database: \(error)")
        }
    }

    private func runMigrations() throws {
        var migrator = DatabaseMigrator()

        migrator.registerMigration("v1_initial") { db in
            try db.create(table: "electric_offset", ifNotExists: true) { t in
                t.primaryKey("shape", .text)
                t.column("handle", .text).notNull()
                t.column("offset", .text).notNull()
            }

            try db.create(table: "workspace", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("name", .text).notNull()
                t.column("slug", .text).notNull()
                t.column("icon_url", .text)
                t.column("is_public", .boolean).notNull().defaults(to: false)
                t.column("public_write_policy", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "project", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull()
                t.column("name", .text).notNull()
                t.column("slug", .text).notNull()
                t.column("prefix", .text).notNull()
                t.column("color", .text)
                t.column("sort_order", .double)
                t.column("archived_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "issue", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("project_id", .text).notNull()
                t.column("number", .integer)
                t.column("identifier", .text)
                t.column("title", .text).notNull()
                t.column("description", .text)
                t.column("status", .text).notNull()
                t.column("priority", .text).notNull()
                t.column("assignee_id", .text)
                t.column("creator_id", .text)
                t.column("due_date", .text)
                t.column("due_time", .text)
                t.column("end_time", .text)
                t.column("sort_order", .double)
                t.column("completed_at", .text)
                t.column("archived_at", .text)
                t.column("recurrence_interval", .integer)
                t.column("recurrence_unit", .text)
                t.column("google_calendar_event_id", .text)
                t.column("google_calendar_last_synced_at", .text)
                t.column("google_calendar_last_sync_error", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "label", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull()
                t.column("name", .text).notNull()
                t.column("color", .text).notNull()
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "issue_label", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("issue_id", .text).notNull()
                t.column("label_id", .text).notNull()
                t.column("created_at", .text).notNull()
            }

            try db.create(table: "user", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("name", .text)
                t.column("email", .text).notNull()
                t.column("image", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "workspace_member", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull()
                t.column("user_id", .text).notNull()
                t.column("role", .text).notNull()
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }

            try db.create(table: "workspace_invite", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull()
                t.column("role", .text).notNull()
                t.column("token", .text).notNull()
                t.column("expires_at", .text).notNull()
                t.column("accepted_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
        }

        migrator.registerMigration("v2_public_workspace") { db in
            try db.alter(table: "workspace") { t in
                t.add(column: "is_public", .boolean).notNull().defaults(to: false)
                t.add(column: "public_write_policy", .text)
            }
            // Existing rows keep the column defaults (is_public=false,
            // public_write_policy=null) until Electric streams the next
            // workspace update. Forcing a refetch here caused a flapping
            // sync loop on first launch — the workspace switcher would
            // briefly empty out between mustRefetch cycles.
        }

        migrator.registerMigration("v3_comments") { db in
            try db.create(table: "comment", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("issue_id", .text).notNull().indexed()
                t.column("workspace_id", .text).notNull().indexed()
                t.column("author_id", .text).notNull()
                t.column("body", .text)
                t.column("edited_at", .text)
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
        }

        migrator.registerMigration("v4_comment_kind") { db in
            try db.alter(table: "comment") { t in
                t.add(column: "kind", .text).notNull().defaults(to: "regular")
            }
        }

        migrator.registerMigration("v5_issue_agent_plan") { db in
            try db.alter(table: "issue") { t in
                t.add(column: "agent_plan_state", .text)
                t.add(column: "agent_plan_revision", .integer).notNull().defaults(to: 0)
                t.add(column: "agent_plan_approved_at", .text)
                t.add(column: "agent_plan_approved_by", .text)
                t.add(column: "agent_last_comment_seen_at", .text)
            }
        }

        migrator.registerMigration("v6_attachments") { db in
            try db.create(table: "attachment", ifNotExists: true) { t in
                t.primaryKey("id", .text)
                t.column("workspace_id", .text).notNull().indexed()
                t.column("issue_id", .text).notNull().indexed()
                t.column("comment_id", .text)
                t.column("uploader_id", .text).notNull()
                t.column("filename", .text).notNull()
                t.column("content_type", .text).notNull()
                t.column("size_bytes", .integer).notNull()
                t.column("storage_key", .text).notNull()
                t.column("url", .text).notNull()
                t.column("created_at", .text).notNull()
                t.column("updated_at", .text).notNull()
            }
        }

        migrator.registerMigration("v7_project_github_repo") { db in
            try db.alter(table: "project") { t in
                t.add(column: "github_repo", .text)
            }
        }

        try migrator.migrate(dbPool)
    }

    func clearAllData() throws {
        try dbPool.write { db in
            try db.execute(sql: "DELETE FROM electric_offset")
            try db.execute(sql: "DELETE FROM attachment")
            try db.execute(sql: "DELETE FROM comment")
            try db.execute(sql: "DELETE FROM issue_label")
            try db.execute(sql: "DELETE FROM issue")
            try db.execute(sql: "DELETE FROM label")
            try db.execute(sql: "DELETE FROM project")
            try db.execute(sql: "DELETE FROM workspace_member")
            try db.execute(sql: "DELETE FROM workspace_invite")
            try db.execute(sql: "DELETE FROM workspace")
            try db.execute(sql: "DELETE FROM user")
        }
    }
}
