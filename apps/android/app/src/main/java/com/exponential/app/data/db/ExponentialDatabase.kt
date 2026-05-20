package com.exponential.app.data.db

import androidx.room.Database
import androidx.room.RoomDatabase
import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

@Database(
    entities = [
        WorkspaceEntity::class,
        ProjectEntity::class,
        IssueEntity::class,
        LabelEntity::class,
        IssueLabelEntity::class,
        UserEntity::class,
        WorkspaceMemberEntity::class,
        WorkspaceInviteEntity::class,
        CommentEntity::class,
        ElectricOffsetEntity::class,
    ],
    version = 4,
    exportSchema = false,
)
abstract class ExponentialDatabase : RoomDatabase() {
    abstract fun workspaceDao(): WorkspaceDao
    abstract fun projectDao(): ProjectDao
    abstract fun issueDao(): IssueDao
    abstract fun labelDao(): LabelDao
    abstract fun issueLabelDao(): IssueLabelDao
    abstract fun userDao(): UserDao
    abstract fun workspaceMemberDao(): WorkspaceMemberDao
    abstract fun workspaceInviteDao(): WorkspaceInviteDao
    abstract fun commentDao(): CommentDao
    abstract fun electricOffsetDao(): ElectricOffsetDao

    companion object {
        val MIGRATION_2_3: Migration = object : Migration(2, 3) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL(
                    "ALTER TABLE workspaces ADD COLUMN is_public INTEGER NOT NULL DEFAULT 0"
                )
                db.execSQL(
                    "ALTER TABLE workspaces ADD COLUMN public_write_policy TEXT"
                )
                // Force a workspace shape resync so existing rows pick up new columns.
                db.execSQL("DELETE FROM electric_offsets WHERE shape = 'workspaces'")
            }
        }

        val MIGRATION_3_4: Migration = object : Migration(3, 4) {
            override fun migrate(db: SupportSQLiteDatabase) {
                db.execSQL(
                    """
                    CREATE TABLE IF NOT EXISTS comments (
                        id TEXT NOT NULL PRIMARY KEY,
                        issue_id TEXT NOT NULL,
                        workspace_id TEXT NOT NULL,
                        author_id TEXT NOT NULL,
                        body TEXT,
                        edited_at TEXT,
                        created_at TEXT NOT NULL,
                        updated_at TEXT NOT NULL
                    )
                    """.trimIndent()
                )
                db.execSQL("CREATE INDEX IF NOT EXISTS index_comments_issue_id ON comments(issue_id)")
                db.execSQL("CREATE INDEX IF NOT EXISTS index_comments_workspace_id ON comments(workspace_id)")
            }
        }
    }
}
