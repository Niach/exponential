package com.exponential.app.data.db

import androidx.room.Database
import androidx.room.RoomDatabase

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
        AttachmentEntity::class,
        NotificationEntity::class,
        IssueSubscriberEntity::class,
        IssueEventEntity::class,
        CodingSessionEntity::class,
        ElectricOffsetEntity::class,
    ],
    // v2: added attachments.width / attachments.height (parity with iOS).
    // v3: added 8 agent/PR fields on issues + notifications / issue_subscribers
    //     / issue_events tables (parity with web; the 11th/12th/13th shapes).
    // v4: added users.is_agent (widget helpdesk bot marker).
    // v5: agent_runs table (14th shape) — removed in v6.
    // v6: hard cut — dropped agent_runs + agent/google-calendar issue columns,
    //     added coding_sessions (the new 14th shape) + issues.duplicate_of_id,
    //     issue_subscribers.user_id nullable + email.
    // v7: dropped projects.github_repo — repos now live in the server-only
    //     repositories registry (tRPC `repositories` router, never synced).
    // No Migration object — DatabaseHolder uses destructive fallback + resync.
    version = 7,
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
    abstract fun attachmentDao(): AttachmentDao
    abstract fun notificationDao(): NotificationDao
    abstract fun issueSubscriberDao(): IssueSubscriberDao
    abstract fun issueEventDao(): IssueEventDao
    abstract fun codingSessionDao(): CodingSessionDao
    abstract fun electricOffsetDao(): ElectricOffsetDao
}
