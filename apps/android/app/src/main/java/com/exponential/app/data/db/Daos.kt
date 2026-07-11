package com.exponential.app.data.db

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import kotlinx.coroutines.flow.Flow

@Dao
interface WorkspaceDao {
    @Query("SELECT * FROM workspaces ORDER BY name")
    fun observeAll(): Flow<List<WorkspaceEntity>>

    @Query("SELECT * FROM workspaces WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<WorkspaceEntity?>

    @Query("SELECT * FROM workspaces WHERE slug = :slug LIMIT 1")
    fun observeBySlug(slug: String): Flow<WorkspaceEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: WorkspaceEntity)

    @Query("DELETE FROM workspaces WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM workspaces")
    suspend fun clear()
}

@Dao
interface ProjectDao {
    // deleted_at IS NULL everywhere: trashed projects leave the shape as delete
    // messages, but filter defensively so a stale pre-trash row never resurfaces.
    @Query("SELECT * FROM projects WHERE archived_at IS NULL AND deleted_at IS NULL ORDER BY sort_order, name")
    fun observeAll(): Flow<List<ProjectEntity>>

    @Query("SELECT * FROM projects WHERE workspace_id = :workspaceId AND archived_at IS NULL AND deleted_at IS NULL ORDER BY sort_order, name")
    fun observeByWorkspace(workspaceId: String): Flow<List<ProjectEntity>>

    @Query("SELECT * FROM projects WHERE workspace_id = :workspaceId AND slug = :slug AND deleted_at IS NULL LIMIT 1")
    fun observeBySlug(workspaceId: String, slug: String): Flow<ProjectEntity?>

    @Query("SELECT * FROM projects WHERE id = :id AND archived_at IS NULL AND deleted_at IS NULL LIMIT 1")
    suspend fun getActiveById(id: String): ProjectEntity?

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: ProjectEntity)

    @Query("DELETE FROM projects WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM projects")
    suspend fun clear()
}

@Dao
interface IssueDao {
    @Query("SELECT * FROM issues WHERE project_id = :projectId AND archived_at IS NULL ORDER BY sort_order, created_at")
    fun observeByProject(projectId: String): Flow<List<IssueEntity>>

    // All issues (used by the inbox to resolve titles + the "needs review" list).
    @Query("SELECT * FROM issues")
    fun observeAll(): Flow<List<IssueEntity>>

    // Cross-project "My Issues" view (masterplan §5a): everything assigned to me.
    @Query("SELECT * FROM issues WHERE assignee_id = :userId AND archived_at IS NULL ORDER BY sort_order, created_at")
    fun observeByAssignee(userId: String): Flow<List<IssueEntity>>

    @Query("SELECT * FROM issues WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<IssueEntity?>

    // The issues bundled into one release (EXP-56) — drives the release
    // detail's status-grouped list.
    @Query("SELECT * FROM issues WHERE release_id = :releaseId AND archived_at IS NULL ORDER BY sort_order, created_at")
    fun observeByRelease(releaseId: String): Flow<List<IssueEntity>>

    // Every release-bundled issue — grouped client-side per release for the
    // releases list's progress bars.
    @Query("SELECT * FROM issues WHERE release_id IS NOT NULL AND archived_at IS NULL")
    fun observeAllInReleases(): Flow<List<IssueEntity>>

    // Candidates for the release detail's add-issues sheet: workspace issues
    // that are still actionable (not done/cancelled/duplicate) and not already
    // in THIS release — issues in another release stay offered (the server
    // records both timeline sides on the move).
    @Query(
        "SELECT i.* FROM issues i JOIN projects p ON p.id = i.project_id " +
            "WHERE p.workspace_id = :workspaceId " +
            "AND i.archived_at IS NULL " +
            "AND i.status NOT IN ('done', 'cancelled', 'duplicate') " +
            "AND (i.release_id IS NULL OR i.release_id != :releaseId) " +
            "ORDER BY i.sort_order, i.created_at"
    )
    fun observeAddableForRelease(workspaceId: String, releaseId: String): Flow<List<IssueEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: IssueEntity)

    @Query("DELETE FROM issues WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM issues")
    suspend fun clear()
}

@Dao
interface LabelDao {
    @Query("SELECT * FROM labels WHERE workspace_id = :workspaceId ORDER BY sort_order, name")
    fun observeByWorkspace(workspaceId: String): Flow<List<LabelEntity>>

    // Cross-workspace list for the "My Issues" rows (labels span projects there).
    @Query("SELECT * FROM labels ORDER BY sort_order, name")
    fun observeAll(): Flow<List<LabelEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: LabelEntity)

    @Query("DELETE FROM labels WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM labels")
    suspend fun clear()
}

@Dao
interface IssueLabelDao {
    @Query("SELECT * FROM issue_labels WHERE issue_id = :issueId")
    fun observeByIssue(issueId: String): Flow<List<IssueLabelEntity>>

    @Query("SELECT * FROM issue_labels WHERE workspace_id = :workspaceId")
    fun observeByWorkspace(workspaceId: String): Flow<List<IssueLabelEntity>>

    @Query("SELECT * FROM issue_labels")
    fun observeAllJoins(): Flow<List<IssueLabelEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: IssueLabelEntity)

    @Query("DELETE FROM issue_labels WHERE issue_id = :issueId AND label_id = :labelId")
    suspend fun delete(issueId: String, labelId: String)

    @Query("DELETE FROM issue_labels")
    suspend fun clear()
}

@Dao
interface UserDao {
    @Query("SELECT * FROM users ORDER BY name, email")
    fun observeAll(): Flow<List<UserEntity>>

    @Query("SELECT * FROM users WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<UserEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: UserEntity)

    @Query("DELETE FROM users WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM users")
    suspend fun clear()
}

@Dao
interface WorkspaceMemberDao {
    @Query("SELECT * FROM workspace_members WHERE workspace_id = :workspaceId")
    fun observeByWorkspace(workspaceId: String): Flow<List<WorkspaceMemberEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: WorkspaceMemberEntity)

    @Query("DELETE FROM workspace_members WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM workspace_members")
    suspend fun clear()
}

@Dao
interface CommentDao {
    @Query("SELECT * FROM comments WHERE issue_id = :issueId ORDER BY created_at ASC")
    fun observeByIssue(issueId: String): Flow<List<CommentEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: CommentEntity)

    @Query("DELETE FROM comments WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM comments")
    suspend fun clear()
}

@Dao
interface CodingSessionDao {
    @Query("SELECT * FROM coding_sessions WHERE issue_id = :issueId ORDER BY started_at DESC")
    fun observeByIssue(issueId: String): Flow<List<CodingSessionEntity>>

    @Query("SELECT * FROM coding_sessions WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<CodingSessionEntity?>

    @Query("SELECT * FROM coding_sessions WHERE workspace_id = :workspaceId")
    fun observeByWorkspace(workspaceId: String): Flow<List<CodingSessionEntity>>

    // Account-wide live sessions (the Agents tab + its bottom-bar dot).
    @Query("SELECT * FROM coding_sessions WHERE status = :status ORDER BY started_at DESC")
    fun observeByStatus(status: String): Flow<List<CodingSessionEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: CodingSessionEntity)

    @Query("DELETE FROM coding_sessions WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM coding_sessions")
    suspend fun clear()
}

@Dao
interface ReleaseDao {
    @Query("SELECT * FROM releases WHERE workspace_id = :workspaceId")
    fun observeByWorkspace(workspaceId: String): Flow<List<ReleaseEntity>>

    // Cross-workspace map for timeline release-name resolution.
    @Query("SELECT * FROM releases")
    fun observeAll(): Flow<List<ReleaseEntity>>

    @Query("SELECT * FROM releases WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<ReleaseEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: ReleaseEntity)

    @Query("DELETE FROM releases WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM releases")
    suspend fun clear()
}

@Dao
interface AttachmentDao {
    @Query("SELECT * FROM attachments WHERE issue_id = :issueId ORDER BY created_at ASC")
    fun observeByIssue(issueId: String): Flow<List<AttachmentEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: AttachmentEntity)

    @Query("DELETE FROM attachments WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM attachments")
    suspend fun clear()
}

@Dao
interface WorkspaceInviteDao {
    @Query("SELECT * FROM workspace_invites WHERE workspace_id = :workspaceId AND accepted_at IS NULL")
    fun observeByWorkspace(workspaceId: String): Flow<List<WorkspaceInviteEntity>>

    @Query("SELECT * FROM workspace_invites WHERE token = :token LIMIT 1")
    fun observeByToken(token: String): Flow<WorkspaceInviteEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: WorkspaceInviteEntity)

    @Query("DELETE FROM workspace_invites WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM workspace_invites")
    suspend fun clear()
}

@Dao
interface NotificationDao {
    @Query("SELECT * FROM notifications WHERE user_id = :userId ORDER BY created_at DESC")
    fun observeByUser(userId: String): Flow<List<NotificationEntity>>

    @Query("SELECT COUNT(*) FROM notifications WHERE user_id = :userId AND read_at IS NULL")
    fun observeUnreadCount(userId: String): Flow<Int>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: NotificationEntity)

    @Query("DELETE FROM notifications WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM notifications")
    suspend fun clear()
}

@Dao
interface IssueSubscriberDao {
    @Query("SELECT * FROM issue_subscribers WHERE issue_id = :issueId")
    fun observeByIssue(issueId: String): Flow<List<IssueSubscriberEntity>>

    @Query("SELECT * FROM issue_subscribers WHERE workspace_id = :workspaceId")
    fun observeByWorkspace(workspaceId: String): Flow<List<IssueSubscriberEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: IssueSubscriberEntity)

    @Query("DELETE FROM issue_subscribers WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM issue_subscribers")
    suspend fun clear()
}

@Dao
interface IssueEventDao {
    @Query("SELECT * FROM issue_events WHERE issue_id = :issueId ORDER BY created_at ASC")
    fun observeByIssue(issueId: String): Flow<List<IssueEventEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: IssueEventEntity)

    @Query("DELETE FROM issue_events WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM issue_events")
    suspend fun clear()
}

@Dao
interface ElectricOffsetDao {
    @Query("SELECT * FROM electric_offsets WHERE shape = :shape LIMIT 1")
    suspend fun get(shape: String): ElectricOffsetEntity?

    // Reactive "has this shape reached up-to-date at least once" — is_live flips
    // true when the initial snapshot completes (even for a zero-row shape). Null
    // until the first offset row is written. Lets the UI tell "still doing the
    // initial sync" apart from "genuinely empty account".
    @Query("SELECT is_live FROM electric_offsets WHERE shape = :shape LIMIT 1")
    fun observeIsLive(shape: String): Flow<Boolean?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: ElectricOffsetEntity)

    @Query("DELETE FROM electric_offsets WHERE shape = :shape")
    suspend fun deleteShape(shape: String)

    @Query("DELETE FROM electric_offsets")
    suspend fun clear()
}
