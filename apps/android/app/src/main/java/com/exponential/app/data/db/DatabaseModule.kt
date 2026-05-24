package com.exponential.app.data.db

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object DatabaseModule {

    // DAOs are provided as @Singleton facades that read the current DB from
    // DatabaseHolder at every call. This lets us swap the underlying SQLite
    // file (per-account, see DatabaseHolder.switchTo) without re-injecting
    // anything. Flow methods use flatMapLatest internally so consumers see
    // new data after a swap.

    @Provides
    @Singleton
    fun provideWorkspaceDao(holder: DatabaseHolder): WorkspaceDao = WorkspaceDaoFacade(holder)

    @Provides
    @Singleton
    fun provideProjectDao(holder: DatabaseHolder): ProjectDao = ProjectDaoFacade(holder)

    @Provides
    @Singleton
    fun provideIssueDao(holder: DatabaseHolder): IssueDao = IssueDaoFacade(holder)

    @Provides
    @Singleton
    fun provideLabelDao(holder: DatabaseHolder): LabelDao = LabelDaoFacade(holder)

    @Provides
    @Singleton
    fun provideIssueLabelDao(holder: DatabaseHolder): IssueLabelDao = IssueLabelDaoFacade(holder)

    @Provides
    @Singleton
    fun provideUserDao(holder: DatabaseHolder): UserDao = UserDaoFacade(holder)

    @Provides
    @Singleton
    fun provideWorkspaceMemberDao(holder: DatabaseHolder): WorkspaceMemberDao = WorkspaceMemberDaoFacade(holder)

    @Provides
    @Singleton
    fun provideWorkspaceInviteDao(holder: DatabaseHolder): WorkspaceInviteDao = WorkspaceInviteDaoFacade(holder)

    @Provides
    @Singleton
    fun provideCommentDao(holder: DatabaseHolder): CommentDao = CommentDaoFacade(holder)

    @Provides
    @Singleton
    fun provideAttachmentDao(holder: DatabaseHolder): AttachmentDao = AttachmentDaoFacade(holder)

    @Provides
    @Singleton
    fun provideElectricOffsetDao(holder: DatabaseHolder): ElectricOffsetDao = ElectricOffsetDaoFacade(holder)
}
