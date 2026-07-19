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
    // v11: added projects.is_protected (server-owned dogfood-board flag that
    //      hides delete/archive affordances). Additive column; destructive
    //      fallback wipes + resyncs — acceptable here because this release
    //      already forces one full resync per account via the per-user re-key.
    // v12: caught the local schema up to the current server shape contracts —
    //      projects.deleted_at (trash marker, EXP-26) plus the denormalized
    //      project_id on issue_labels / comments / attachments / issue_events /
    //      coding_sessions and workspace_invites.invited_by_id (stops the
    //      tolerant-apply "dropped columns" diagnostics noise).
    // v13: workspace_invites.token dropped from the shape server-side
    //      (bearer-secret leak fix, REV-4/14) — entity field now nullable;
    //      destructive fallback wipes + resyncs (also purging any previously
    //      leaked plaintext tokens from the local cache).
    // v14: releases (EXP-56, the 15th shape) + issues.release_id +
    //      coding_sessions.release_id (issue_id now nullable for
    //      release-scoped orchestrator sessions).
    // v15: projects.public_show_coding removed (EXP-90 — public feedback
    //      coding sessions are gone; the column left the synced shape).
    // v16: releases removed (EXP-106 — releases table + shape dropped, back to
    //      14 shapes) + issues.release_id + coding_sessions.release_id gone
    //      (coding_sessions.issue_id stays nullable for batch multi-issue runs).
    // v17: project-type collapse (EXP-121) — projects gained is_public + icon on
    //      the existing shape (repository stays optional). `type` still syncs
    //      (dual-written server-side) but no longer gates behavior.
    // v18: recurrence removed (EXP-107) — issues.recurrence_interval /
    //      recurrence_unit dropped from the entity + shape; in_review status
    //      (EXP-120) is a plain enum-value string, no schema change.
    //      projects.type dropped entirely (EXP-129 — the column, pg enum and
    //      shape column are gone server-side); the entity field is removed. The
    //      icon fallback now derives from is_public / repository_id instead.
    //      Destructive fallback wipes + resyncs.
    // v19: public feedback boards removed (EXP-180) — projects.is_public /
    //      public_show_comments / public_show_activity dropped from the entity
    //      (and the shape server-side); the icon fallback derives from
    //      repository_id alone. Destructive fallback wipes + resyncs.
    // No Migration object — DatabaseHolder uses destructive fallback + resync,
    // so an additive shape column just wipes and re-syncs from Electric.
    version = 19,
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
