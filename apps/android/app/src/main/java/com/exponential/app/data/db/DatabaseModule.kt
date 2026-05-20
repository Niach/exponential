package com.exponential.app.data.db

import android.content.Context
import androidx.room.Room
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object DatabaseModule {

    @Provides
    @Singleton
    fun provideDatabase(@ApplicationContext context: Context): ExponentialDatabase =
        Room.databaseBuilder(context, ExponentialDatabase::class.java, "exponential.db")
            .addMigrations(
                ExponentialDatabase.MIGRATION_2_3,
                ExponentialDatabase.MIGRATION_3_4,
            )
            .fallbackToDestructiveMigration()
            .build()

    @Provides
    fun provideWorkspaceDao(db: ExponentialDatabase): WorkspaceDao = db.workspaceDao()

    @Provides
    fun provideProjectDao(db: ExponentialDatabase): ProjectDao = db.projectDao()

    @Provides
    fun provideIssueDao(db: ExponentialDatabase): IssueDao = db.issueDao()

    @Provides
    fun provideLabelDao(db: ExponentialDatabase): LabelDao = db.labelDao()

    @Provides
    fun provideIssueLabelDao(db: ExponentialDatabase): IssueLabelDao = db.issueLabelDao()

    @Provides
    fun provideUserDao(db: ExponentialDatabase): UserDao = db.userDao()

    @Provides
    fun provideWorkspaceMemberDao(db: ExponentialDatabase): WorkspaceMemberDao = db.workspaceMemberDao()

    @Provides
    fun provideWorkspaceInviteDao(db: ExponentialDatabase): WorkspaceInviteDao = db.workspaceInviteDao()

    @Provides
    fun provideCommentDao(db: ExponentialDatabase): CommentDao = db.commentDao()

    @Provides
    fun provideElectricOffsetDao(db: ExponentialDatabase): ElectricOffsetDao = db.electricOffsetDao()
}
