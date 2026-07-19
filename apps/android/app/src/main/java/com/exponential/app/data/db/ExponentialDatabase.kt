package com.exponential.app.data.db

import androidx.room.Database
import androidx.room.RoomDatabase

@Database(
    entities = [
        TeamEntity::class,
        BoardEntity::class,
        IssueEntity::class,
        LabelEntity::class,
        IssueLabelEntity::class,
        UserEntity::class,
        TeamMemberEntity::class,
        TeamInviteEntity::class,
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
    // v7: dropped boards.github_repo — repos now live in the server-only
    //     repositories registry (tRPC `repositories` router, never synced).
    // v8: added boards.repository_id (masterplan v4 §3 — board = repository;
    //     additive column on the existing boards shape, no shape-count change).
    // v9: added electric_offsets.is_live (live-gating: only long-poll live=true
    //     after up-to-date; catch-up polls stay non-live).
    // v10: board types — boards gained type + public_show_* toggles and
    //      repository_id went nullable; teams dropped is_public /
    //      public_write_policy. The six issue-child tables gained a denormalized
    //      board_id column server-side; ignoreUnknownKeys absorbs it (we don't
    //      store it locally). Shapes rotate once on the deploy → destructive
    //      resync repopulates.
    // v11: added boards.is_protected (server-owned dogfood-board flag that
    //      hides delete/archive affordances). Additive column; destructive
    //      fallback wipes + resyncs — acceptable here because this release
    //      already forces one full resync per account via the per-user re-key.
    // v12: caught the local schema up to the current server shape contracts —
    //      boards.deleted_at (trash marker, EXP-26) plus the denormalized
    //      board_id on issue_labels / comments / attachments / issue_events /
    //      coding_sessions and team_invites.invited_by_id (stops the
    //      tolerant-apply "dropped columns" diagnostics noise).
    // v13: team_invites.token dropped from the shape server-side
    //      (bearer-secret leak fix, REV-4/14) — entity field now nullable;
    //      destructive fallback wipes + resyncs (also purging any previously
    //      leaked plaintext tokens from the local cache).
    // v14: releases (EXP-56, the 15th shape) + issues.release_id +
    //      coding_sessions.release_id (issue_id now nullable for
    //      release-scoped orchestrator sessions).
    // v15: boards.public_show_coding removed (EXP-90 — public feedback
    //      coding sessions are gone; the column left the synced shape).
    // v16: releases removed (EXP-106 — releases table + shape dropped, back to
    //      14 shapes) + issues.release_id + coding_sessions.release_id gone
    //      (coding_sessions.issue_id stays nullable for batch multi-issue runs).
    // v17: board-type collapse (EXP-121) — boards gained is_public + icon on
    //      the existing shape (repository stays optional). `type` still syncs
    //      (dual-written server-side) but no longer gates behavior.
    // v18: recurrence removed (EXP-107) — issues.recurrence_interval /
    //      recurrence_unit dropped from the entity + shape; in_review status
    //      (EXP-120) is a plain enum-value string, no schema change.
    //      boards.type dropped entirely (EXP-129 — the column, pg enum and
    //      shape column are gone server-side); the entity field is removed. The
    //      icon fallback now derives from is_public / repository_id instead.
    //      Destructive fallback wipes + resyncs.
    // v19: public feedback boards removed (EXP-180) — boards.is_public /
    //      public_show_comments / public_show_activity dropped from the entity
    //      (and the shape server-side); the icon fallback derives from
    //      repository_id alone. Destructive fallback wipes + resyncs.
    // v20: the great rename (EXP-180) — workspace→team / project→board: local
    //      tables (teams, boards, team_members, team_invites) and wire columns
    //      (team_id / board_id on every child table) renamed to the new server
    //      contract. Destructive fallback wipes + resyncs from the renamed
    //      shapes.
    // v21: teams.helpdesk_enabled (EXP-180 helpdesk) — the synced team-level
    //      flag gating the Support inbox. Additive column on the existing teams
    //      shape; destructive fallback wipes + resyncs.
    // v22: notifications.team_id (EXP-180 helpdesk) — nullable, set on
    //      issue-less support_reply rows so the inbox can group them per team.
    //      Additive column on the existing notifications shape; destructive
    //      fallback wipes + resyncs.
    // v23: team_invites.email (EXP-188 invite-by-email) — optional invited
    //      address, synced for the pending-invite list (the bearer token stays
    //      excluded). Additive column on the existing team-invites shape;
    //      destructive fallback wipes + resyncs.
    // No Migration object — DatabaseHolder uses destructive fallback + resync,
    // so an additive shape column just wipes and re-syncs from Electric.
    version = 23,
    exportSchema = false,
)
abstract class ExponentialDatabase : RoomDatabase() {
    abstract fun teamDao(): TeamDao
    abstract fun boardDao(): BoardDao
    abstract fun issueDao(): IssueDao
    abstract fun labelDao(): LabelDao
    abstract fun issueLabelDao(): IssueLabelDao
    abstract fun userDao(): UserDao
    abstract fun teamMemberDao(): TeamMemberDao
    abstract fun teamInviteDao(): TeamInviteDao
    abstract fun commentDao(): CommentDao
    abstract fun attachmentDao(): AttachmentDao
    abstract fun notificationDao(): NotificationDao
    abstract fun issueSubscriberDao(): IssueSubscriberDao
    abstract fun issueEventDao(): IssueEventDao
    abstract fun codingSessionDao(): CodingSessionDao
    abstract fun electricOffsetDao(): ElectricOffsetDao
}
