package com.exponential.app.data.db

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.ExperimentalCoroutinesApi

/// DAO facades that delegate to the current DB held by DatabaseHolder.
/// Flow methods use flatMapLatest so consumers receive new data automatically
/// when the active server account switches the underlying database.
///
/// Suspend methods read holder.current() at call time; if no DB is open they
/// throw — but in practice the auth flow guarantees a DB is open before any
/// repository call fires (DatabaseHolder.switchTo is called before sync starts).

@OptIn(ExperimentalCoroutinesApi::class)
private fun <T> Flow<ExponentialDatabase?>.flatMapDao(transform: (ExponentialDatabase) -> Flow<T>): Flow<T> =
    flatMapLatest { db -> if (db == null) emptyFlow() else transform(db) }

class WorkspaceDaoFacade(private val holder: DatabaseHolder) : WorkspaceDao {
    override fun observeAll(): Flow<List<WorkspaceEntity>> =
        holder.database.flatMapDao { it.workspaceDao().observeAll() }

    override fun observeById(id: String): Flow<WorkspaceEntity?> =
        holder.database.flatMapDao { it.workspaceDao().observeById(id) }

    override fun observeBySlug(slug: String): Flow<WorkspaceEntity?> =
        holder.database.flatMapDao { it.workspaceDao().observeBySlug(slug) }

    override suspend fun upsert(item: WorkspaceEntity) = holder.current().workspaceDao().upsert(item)
    override suspend fun deleteById(id: String) = holder.current().workspaceDao().deleteById(id)
    override suspend fun clear() = holder.current().workspaceDao().clear()
}

class ProjectDaoFacade(private val holder: DatabaseHolder) : ProjectDao {
    override fun observeAll(): Flow<List<ProjectEntity>> =
        holder.database.flatMapDao { it.projectDao().observeAll() }

    override fun observeByWorkspace(workspaceId: String): Flow<List<ProjectEntity>> =
        holder.database.flatMapDao { it.projectDao().observeByWorkspace(workspaceId) }

    override fun observeBySlug(workspaceId: String, slug: String): Flow<ProjectEntity?> =
        holder.database.flatMapDao { it.projectDao().observeBySlug(workspaceId, slug) }

    override suspend fun upsert(item: ProjectEntity) = holder.current().projectDao().upsert(item)
    override suspend fun deleteById(id: String) = holder.current().projectDao().deleteById(id)
    override suspend fun clear() = holder.current().projectDao().clear()
}

class IssueDaoFacade(private val holder: DatabaseHolder) : IssueDao {
    override fun observeByProject(projectId: String): Flow<List<IssueEntity>> =
        holder.database.flatMapDao { it.issueDao().observeByProject(projectId) }

    override fun observeById(id: String): Flow<IssueEntity?> =
        holder.database.flatMapDao { it.issueDao().observeById(id) }

    override suspend fun upsert(item: IssueEntity) = holder.current().issueDao().upsert(item)
    override suspend fun deleteById(id: String) = holder.current().issueDao().deleteById(id)
    override suspend fun clear() = holder.current().issueDao().clear()
}

class LabelDaoFacade(private val holder: DatabaseHolder) : LabelDao {
    override fun observeByWorkspace(workspaceId: String): Flow<List<LabelEntity>> =
        holder.database.flatMapDao { it.labelDao().observeByWorkspace(workspaceId) }

    override suspend fun upsert(item: LabelEntity) = holder.current().labelDao().upsert(item)
    override suspend fun deleteById(id: String) = holder.current().labelDao().deleteById(id)
    override suspend fun clear() = holder.current().labelDao().clear()
}

class IssueLabelDaoFacade(private val holder: DatabaseHolder) : IssueLabelDao {
    override fun observeByIssue(issueId: String): Flow<List<IssueLabelEntity>> =
        holder.database.flatMapDao { it.issueLabelDao().observeByIssue(issueId) }

    override fun observeByWorkspace(workspaceId: String): Flow<List<IssueLabelEntity>> =
        holder.database.flatMapDao { it.issueLabelDao().observeByWorkspace(workspaceId) }

    override suspend fun upsert(item: IssueLabelEntity) = holder.current().issueLabelDao().upsert(item)
    override suspend fun delete(issueId: String, labelId: String) =
        holder.current().issueLabelDao().delete(issueId, labelId)
    override suspend fun clear() = holder.current().issueLabelDao().clear()
}

class UserDaoFacade(private val holder: DatabaseHolder) : UserDao {
    override fun observeAll(): Flow<List<UserEntity>> =
        holder.database.flatMapDao { it.userDao().observeAll() }

    override fun observeById(id: String): Flow<UserEntity?> =
        holder.database.flatMapDao { it.userDao().observeById(id) }

    override suspend fun upsert(item: UserEntity) = holder.current().userDao().upsert(item)
    override suspend fun deleteById(id: String) = holder.current().userDao().deleteById(id)
    override suspend fun clear() = holder.current().userDao().clear()
}

class WorkspaceMemberDaoFacade(private val holder: DatabaseHolder) : WorkspaceMemberDao {
    override fun observeByWorkspace(workspaceId: String): Flow<List<WorkspaceMemberEntity>> =
        holder.database.flatMapDao { it.workspaceMemberDao().observeByWorkspace(workspaceId) }

    override suspend fun upsert(item: WorkspaceMemberEntity) =
        holder.current().workspaceMemberDao().upsert(item)
    override suspend fun deleteById(id: String) = holder.current().workspaceMemberDao().deleteById(id)
    override suspend fun clear() = holder.current().workspaceMemberDao().clear()
}

class CommentDaoFacade(private val holder: DatabaseHolder) : CommentDao {
    override fun observeByIssue(issueId: String): Flow<List<CommentEntity>> =
        holder.database.flatMapDao { it.commentDao().observeByIssue(issueId) }

    override suspend fun upsert(item: CommentEntity) = holder.current().commentDao().upsert(item)
    override suspend fun deleteById(id: String) = holder.current().commentDao().deleteById(id)
    override suspend fun clear() = holder.current().commentDao().clear()
}

class AttachmentDaoFacade(private val holder: DatabaseHolder) : AttachmentDao {
    override fun observeByIssue(issueId: String): Flow<List<AttachmentEntity>> =
        holder.database.flatMapDao { it.attachmentDao().observeByIssue(issueId) }

    override suspend fun upsert(item: AttachmentEntity) = holder.current().attachmentDao().upsert(item)
    override suspend fun deleteById(id: String) = holder.current().attachmentDao().deleteById(id)
    override suspend fun clear() = holder.current().attachmentDao().clear()
}

class WorkspaceInviteDaoFacade(private val holder: DatabaseHolder) : WorkspaceInviteDao {
    override fun observeByWorkspace(workspaceId: String): Flow<List<WorkspaceInviteEntity>> =
        holder.database.flatMapDao { it.workspaceInviteDao().observeByWorkspace(workspaceId) }

    override fun observeByToken(token: String): Flow<WorkspaceInviteEntity?> =
        holder.database.flatMapDao { it.workspaceInviteDao().observeByToken(token) }

    override suspend fun upsert(item: WorkspaceInviteEntity) =
        holder.current().workspaceInviteDao().upsert(item)
    override suspend fun deleteById(id: String) = holder.current().workspaceInviteDao().deleteById(id)
    override suspend fun clear() = holder.current().workspaceInviteDao().clear()
}

class ElectricOffsetDaoFacade(private val holder: DatabaseHolder) : ElectricOffsetDao {
    override suspend fun get(shape: String): ElectricOffsetEntity? =
        holder.current().electricOffsetDao().get(shape)
    override suspend fun upsert(item: ElectricOffsetEntity) =
        holder.current().electricOffsetDao().upsert(item)
    override suspend fun deleteShape(shape: String) =
        holder.current().electricOffsetDao().deleteShape(shape)
    override suspend fun clear() = holder.current().electricOffsetDao().clear()
}
