package com.exponential.app.data.db

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import kotlinx.coroutines.flow.Flow

@Dao
interface TeamDao {
    @Query("SELECT * FROM teams ORDER BY name")
    fun observeAll(): Flow<List<TeamEntity>>

    @Query("SELECT * FROM teams WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<TeamEntity?>

    @Query("SELECT * FROM teams WHERE slug = :slug LIMIT 1")
    fun observeBySlug(slug: String): Flow<TeamEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: TeamEntity)

    @Query("DELETE FROM teams WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM teams")
    suspend fun clear()
}

@Dao
interface BoardDao {
    // deleted_at IS NULL everywhere: trashed boards leave the shape as delete
    // messages, but filter defensively so a stale pre-trash row never resurfaces.
    @Query("SELECT * FROM boards WHERE archived_at IS NULL AND deleted_at IS NULL ORDER BY sort_order, name")
    fun observeAll(): Flow<List<BoardEntity>>

    @Query("SELECT * FROM boards WHERE team_id = :teamId AND archived_at IS NULL AND deleted_at IS NULL ORDER BY sort_order, name")
    fun observeByTeam(teamId: String): Flow<List<BoardEntity>>

    @Query("SELECT * FROM boards WHERE team_id = :teamId AND slug = :slug AND deleted_at IS NULL LIMIT 1")
    fun observeBySlug(teamId: String, slug: String): Flow<BoardEntity?>

    @Query("SELECT * FROM boards WHERE id = :id AND archived_at IS NULL AND deleted_at IS NULL LIMIT 1")
    suspend fun getActiveById(id: String): BoardEntity?

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: BoardEntity)

    @Query("DELETE FROM boards WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM boards")
    suspend fun clear()
}

@Dao
interface IssueDao {
    @Query("SELECT * FROM issues WHERE board_id = :boardId AND archived_at IS NULL ORDER BY sort_order, created_at")
    fun observeByBoard(boardId: String): Flow<List<IssueEntity>>

    // All issues (used by the inbox to resolve titles + the "needs review" list).
    @Query("SELECT * FROM issues")
    fun observeAll(): Flow<List<IssueEntity>>

    // Cross-board "My Issues" view (masterplan §5a): everything assigned to me.
    @Query("SELECT * FROM issues WHERE assignee_id = :userId AND archived_at IS NULL ORDER BY sort_order, created_at")
    fun observeByAssignee(userId: String): Flow<List<IssueEntity>>

    @Query("SELECT * FROM issues WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<IssueEntity?>

    // Reviews (EXP-131): every issue in one team with an OPEN pull request.
    // Joins boards to scope by team and drop trashed/archived boards;
    // a batch PR links several issues to the SAME pr_url, so the client groups
    // these rows by pr_url into one review entry. Draft and state-less prUrl
    // rows are deliberately excluded — parity with web use-reviews-data.ts
    // (open only).
    @Query(
        "SELECT i.* FROM issues i JOIN boards p ON p.id = i.board_id " +
            "WHERE p.team_id = :teamId AND i.pr_state = 'open' " +
            "AND i.archived_at IS NULL AND p.deleted_at IS NULL AND p.archived_at IS NULL"
    )
    fun observeOpenPrsByTeam(teamId: String): Flow<List<IssueEntity>>

    // App-link resolution (EXP-92): team SLUG + identifier → issue id.
    // Deliberately no archived filter (an emailed link to an archived issue
    // should still open) and no board-slug predicate (identifiers are
    // team-unique; the board slug in an old link goes stale when an
    // issue moves — the web route also keys on the identifier alone).
    @Query(
        "SELECT i.id FROM issues i JOIN boards p ON p.id = i.board_id " +
            "JOIN teams w ON w.id = p.team_id " +
            "WHERE upper(i.identifier) = upper(:identifier) AND w.slug = :teamSlug LIMIT 1"
    )
    suspend fun findIdByTeamRef(teamSlug: String, identifier: String): String?

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: IssueEntity)

    @Query("DELETE FROM issues WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM issues")
    suspend fun clear()
}

@Dao
interface LabelDao {
    @Query("SELECT * FROM labels WHERE team_id = :teamId ORDER BY sort_order, name")
    fun observeByTeam(teamId: String): Flow<List<LabelEntity>>

    // Cross-team list for the "My Issues" rows (labels span boards there).
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

    @Query("SELECT * FROM issue_labels WHERE team_id = :teamId")
    fun observeByTeam(teamId: String): Flow<List<IssueLabelEntity>>

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
interface TeamMemberDao {
    @Query("SELECT * FROM team_members WHERE team_id = :teamId")
    fun observeByTeam(teamId: String): Flow<List<TeamMemberEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: TeamMemberEntity)

    @Query("DELETE FROM team_members WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM team_members")
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

    @Query("SELECT * FROM coding_sessions WHERE team_id = :teamId")
    fun observeByTeam(teamId: String): Flow<List<CodingSessionEntity>>

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
interface TeamInviteDao {
    @Query("SELECT * FROM team_invites WHERE team_id = :teamId AND accepted_at IS NULL")
    fun observeByTeam(teamId: String): Flow<List<TeamInviteEntity>>

    @Query("SELECT * FROM team_invites WHERE token = :token LIMIT 1")
    fun observeByToken(token: String): Flow<TeamInviteEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: TeamInviteEntity)

    @Query("DELETE FROM team_invites WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM team_invites")
    suspend fun clear()
}

@Dao
interface NotificationDao {
    @Query("SELECT * FROM notifications WHERE user_id = :userId ORDER BY created_at DESC")
    fun observeByUser(userId: String): Flow<List<NotificationEntity>>

    @Query("SELECT COUNT(*) FROM notifications WHERE user_id = :userId AND read_at IS NULL")
    fun observeUnreadCount(userId: String): Flow<Int>

    // Unread helpdesk activity in one team: issue-less support_reply rows
    // carry a synced team_id (the inbox's per-team Support groups use the
    // same rule). :type is always DomainContract.notificationTypeSupportReply.
    @Query(
        "SELECT COUNT(*) FROM notifications WHERE user_id = :userId AND read_at IS NULL " +
            "AND type = :type AND issue_id IS NULL AND team_id = :teamId"
    )
    fun observeUnreadSupportCount(userId: String, teamId: String, type: String): Flow<Int>

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

    @Query("SELECT * FROM issue_subscribers WHERE team_id = :teamId")
    fun observeByTeam(teamId: String): Flow<List<IssueSubscriberEntity>>

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
