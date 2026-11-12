package com.exponential.app.data.db

import androidx.room.Database
import androidx.room.RoomDatabase
import androidx.room.TypeConverters

@Database(
    entities = [
        TeamEntity::class,
        BoardEntity::class,
        IssueEntity::class,
        LabelEntity::class,
        IssueStatusEntity::class,
        IssueLabelEntity::class,
        IssueRelationEntity::class,
        UserEntity::class,
        TeamMemberEntity::class,
        TeamInviteEntity::class,
        CommentEntity::class,
        AttachmentEntity::class,
        NotificationEntity::class,
        IssueSubscriberEntity::class,
        IssueEventEntity::class,
        DeviceEntity::class,
        DeviceWorktreeEntity::class,
        PinEntity::class,
        IssueDraftEntity::class,
        ComposerDraftEntity::class,
        SessionEntity::class,
        SessionIssueEntity::class,
        MessageEntity::class,
        PullRequestEntity::class,
        PullRequestIssueEntity::class,
        PullRequestCheckEntity::class,
        SessionReadEntity::class,
        AgentDefinitionEntity::class,
        EventEntity::class,
        PromptEntity::class,
        SkillEntity::class,
        AppEntity::class,
        AppVersionEntity::class,
        AppRecordEntity::class,
        AppRecordOutboxEntity::class,
        ElectricOffsetEntity::class,
    ],
    // v105 (EXP-1459): sessions.seal_public_key, seal_epoch, sealed_title
    //      (sealed content). Explicit [MIGRATION_104_105] adds the nullable
    //      columns and refetches only the sessions shape. v104 is the oldest
    //      schema kept in place (Android 0.14.73-0.14.77); the history before
    //      it lives in git. Older versions fall back to destructive + resync
    //      (DatabaseHolder).
    // v106 (#VAPP-186): comments.via_app_id ("via <app>"), explicit
    //      [MIGRATION_105_106]; only the comments shape refetches.
    // v107 (#EXP-1534): notifications.device_id (agent_signed_out rows),
    //      explicit [MIGRATION_106_107]; only the notifications shape refetches.
    // v108 (#EXP-1611): pull_requests.checks_state + the pull_request_checks
    //      table (CI on a PR), explicit [MIGRATION_107_108]; only the
    //      pull_requests shape refetches (the new shape starts fresh).
    // v109 (#VAPP-187): app_records.author_id + updated_by (record
    //      ownership), explicit [MIGRATION_108_109]; only the app_records
    //      shape refetches.
    // v110 (#EXP-1661): the `skills` table + agents.skill_ids, explicit
    //      [MIGRATION_109_110]; only the agents shape refetches.
    // v111 (eApps V1, #VAPP-210): apps.published_version + draft_package,
    //      explicit [MIGRATION_110_111]; only the apps shape refetches.
    // v112 (#VAPP-213): skills.builtin, explicit [MIGRATION_111_112]; only the
    //      skills shape refetches.
    // v113 (#VAPP-201): teams.time_zone (the team time zone), explicit
    //      [MIGRATION_112_113]; only the teams shape refetches.
    // v114 (#VAPP-229): issues/sessions/app_records.via_app_id ("via <app>"),
    //      explicit [MIGRATION_113_114]; only those three shapes refetch.
    // v115 (#VAPP-235): the LOCAL `app_record_outbox` table (the offline
    //      outbox, no shape), explicit [MIGRATION_114_115]; nothing refetches.
    // v116 (#VAPP-233): `app_versions.migration_*` (the record sweep's
    //      progress), explicit [MIGRATION_115_116]; app_versions refetches.
    // v117 (#EXP-1682): messages.attachment_ids (a message's clear files),
    //      explicit [MIGRATION_116_117]; only the messages shape refetches.
    // v118 (#VAPP-241): app_records.share_name (public links), explicit
    //      [MIGRATION_117_118]; only the app_records shape refetches.
    // v119 (#EXP-1710): pull_requests.merge_* + teams.merge_mode (the merge
    //      train), explicit [MIGRATION_118_119]; only those two shapes refetch.
    // v120 (#EXP-1715): sessions.scope_team_ids (the Assistant's Teams pick),
    //      explicit [MIGRATION_119_120]; only the sessions shape refetches.
    // v121 (#EXP-1730 phase A, one tree): messages.session_id (+ index),
    //      sessions.digest, agents.defaults, apps.agent_id; the local
    //      `conversations` table is dropped (nothing reads it: messages key by
    //      session_id, the list reads sessions.digest). Explicit
    //      [MIGRATION_120_121]; the messages, sessions, agents and apps shapes
    //      refetch, the conversations offset goes.
    // v122 (#EXP-1793): agents.mcp_server_ids (the agent's MCP servers),
    //      explicit [MIGRATION_121_122]; only the agents shape refetches.
    // v123 (large chats): messages index (root_id, created_at, id) replaces
    //      (root_id), explicit [MIGRATION_122_123]; nothing refetches.
    // v124 (#FEED-219): the `composer_drafts` table (every composer's synced
    //      draft), explicit [MIGRATION_123_124]; the new shape starts fresh.
    version = 124,
    exportSchema = false,
)
@TypeConverters(StringListConverters::class)
abstract class ExponentialDatabase : RoomDatabase() {
    abstract fun teamDao(): TeamDao
    abstract fun boardDao(): BoardDao
    abstract fun issueDao(): IssueDao
    abstract fun labelDao(): LabelDao
    abstract fun issueStatusDao(): IssueStatusDao
    abstract fun issueLabelDao(): IssueLabelDao
    abstract fun issueRelationDao(): IssueRelationDao
    abstract fun userDao(): UserDao
    abstract fun teamMemberDao(): TeamMemberDao
    abstract fun teamInviteDao(): TeamInviteDao
    abstract fun commentDao(): CommentDao
    abstract fun attachmentDao(): AttachmentDao
    abstract fun notificationDao(): NotificationDao
    abstract fun issueSubscriberDao(): IssueSubscriberDao
    abstract fun issueEventDao(): IssueEventDao
    abstract fun deviceDao(): DeviceDao
    abstract fun deviceWorktreeDao(): DeviceWorktreeDao
    abstract fun pinDao(): PinDao
    abstract fun issueDraftDao(): IssueDraftDao
    abstract fun composerDraftDao(): ComposerDraftDao
    abstract fun sessionDao(): SessionDao
    abstract fun sessionIssueDao(): SessionIssueDao
    abstract fun messageDao(): MessageDao
    abstract fun pullRequestDao(): PullRequestDao
    abstract fun pullRequestIssueDao(): PullRequestIssueDao
    abstract fun pullRequestCheckDao(): PullRequestCheckDao
    abstract fun sessionReadDao(): SessionReadDao
    abstract fun agentDefinitionDao(): AgentDefinitionDao
    abstract fun eventDao(): EventDao
    abstract fun promptDao(): PromptDao
    abstract fun skillDao(): SkillDao
    abstract fun appDao(): AppDao
    abstract fun appVersionDao(): AppVersionDao
    abstract fun appRecordDao(): AppRecordDao
    abstract fun appRecordOutboxDao(): AppRecordOutboxDao
    abstract fun electricOffsetDao(): ElectricOffsetDao
}
