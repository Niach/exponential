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
    // v8: added projects.repository_id (masterplan v4 §3 — project = repository;
    //     additive column on the existing projects shape, no shape-count change).
    // v9: added electric_offsets.is_live (live-gating: only long-poll live=true
    //     after up-to-date; catch-up polls stay non-live).
    // v10: project types — projects gained type + public_show_* toggles and
    //      repository_id went nullable; workspaces dropped is_public /
    //      public_write_policy. The six issue-child tables gained a denormalized
    //      project_id column server-side; ignoreUnknownKeys absorbs it (we don't
    //      store it locally). Shapes rotate once on the deploy → destructive
    //      resync repopulates.
    // No Migration object — DatabaseHolder uses destructive fallback + resync,
    // so an additive shape column just wipes and re-syncs from Electric.
    version = 10,
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
