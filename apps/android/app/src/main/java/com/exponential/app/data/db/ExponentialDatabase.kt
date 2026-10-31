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
        SessionEntity::class,
        SessionIssueEntity::class,
        MessageEntity::class,
        PullRequestEntity::class,
        PullRequestIssueEntity::class,
        SessionReadEntity::class,
        AgentDefinitionEntity::class,
        EventEntity::class,
        PromptEntity::class,
        ConversationEntity::class,
        AppEntity::class,
        AppVersionEntity::class,
        AppRecordEntity::class,
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
    version = 106,
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
    abstract fun sessionDao(): SessionDao
    abstract fun sessionIssueDao(): SessionIssueDao
    abstract fun messageDao(): MessageDao
    abstract fun pullRequestDao(): PullRequestDao
    abstract fun pullRequestIssueDao(): PullRequestIssueDao
    abstract fun sessionReadDao(): SessionReadDao
    abstract fun agentDefinitionDao(): AgentDefinitionDao
    abstract fun eventDao(): EventDao
    abstract fun promptDao(): PromptDao
    abstract fun conversationDao(): ConversationDao
    abstract fun appDao(): AppDao
    abstract fun appVersionDao(): AppVersionDao
    abstract fun appRecordDao(): AppRecordDao
    abstract fun electricOffsetDao(): ElectricOffsetDao
}
