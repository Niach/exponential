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
        ElectricOffsetEntity::class,
    ],
    version = 2,
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
    abstract fun electricOffsetDao(): ElectricOffsetDao
}
