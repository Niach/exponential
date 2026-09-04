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
    @Query("SELECT * FROM boards WHERE deleted_at IS NULL ORDER BY sort_order, name")
    fun observeAll(): Flow<List<BoardEntity>>

    @Query("SELECT * FROM boards WHERE team_id = :teamId AND deleted_at IS NULL ORDER BY sort_order, name")
    fun observeByTeam(teamId: String): Flow<List<BoardEntity>>

    @Query("SELECT * FROM boards WHERE team_id = :teamId AND slug = :slug AND deleted_at IS NULL LIMIT 1")
    fun observeBySlug(teamId: String, slug: String): Flow<BoardEntity?>

    @Query("SELECT * FROM boards WHERE id = :id AND deleted_at IS NULL LIMIT 1")
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
    @Query("SELECT * FROM issues WHERE board_id = :boardId ORDER BY sort_order, created_at")
    fun observeByBoard(boardId: String): Flow<List<IssueEntity>>

    // All issues (used by the inbox to resolve titles + the "needs review" list).
    @Query("SELECT * FROM issues")
    fun observeAll(): Flow<List<IssueEntity>>

    // Cross-board "My Issues" view (masterplan §5a): everything assigned to me.
    @Query("SELECT * FROM issues WHERE assignee_id = :userId ORDER BY sort_order, created_at")
    fun observeByAssignee(userId: String): Flow<List<IssueEntity>>

    @Query("SELECT * FROM issues WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<IssueEntity?>

    // Existence probe for the push-tap fallback's fill-a-hole guard (EXP-264);
    // suspend so it can run inside the same withTransaction as the insert.
    @Query("SELECT EXISTS(SELECT 1 FROM issues WHERE id = :id)")
    suspend fun exists(id: String): Boolean

    // Reviews (EXP-131): every issue in one team with an OPEN pull request.
    // Joins boards to scope by team and drop trashed boards;
    // a batch PR links several issues to the SAME pr_url, so the client groups
    // these rows by pr_url into one review entry. Draft and state-less prUrl
    // rows are deliberately excluded — parity with web use-reviews-data.ts
    // (open only).
    @Query(
        "SELECT i.* FROM issues i JOIN boards p ON p.id = i.board_id " +
            "WHERE p.team_id = :teamId AND i.pr_state = 'open' " +
            "AND p.deleted_at IS NULL"
    )
    fun observeOpenPrsByTeam(teamId: String): Flow<List<IssueEntity>>

    // App-link resolution (EXP-92): team SLUG + identifier → issue id.
    // Deliberately no board-slug predicate (identifiers are
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
interface IssueStatusDao {
    // Canonical ORDERING is IssueStatusResolver.teamStatuses (category display
    // order first); this query only needs a stable, deterministic feed.
    @Query("SELECT * FROM issue_statuses WHERE team_id = :teamId ORDER BY sort_order, created_at, id")
    fun observeByTeam(teamId: String): Flow<List<IssueStatusEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: IssueStatusEntity)

    @Query("DELETE FROM issue_statuses WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM issue_statuses")
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

    // EXP-487: the team's member users — assignee pickers + @-mention
    // vocabulary. The users table is account-wide (cross-team author
    // display), so scoping happens here via the team_members join.
    @Query(
        "SELECT u.* FROM users u JOIN team_members m ON m.user_id = u.id " +
            "WHERE m.team_id = :teamId ORDER BY u.name, u.email"
    )
    fun observeByTeam(teamId: String): Flow<List<UserEntity>>

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

    // Account-wide live sessions (the Agents tab + its bottom-bar dot). Takes a
    // status list so both live states (`running` + `in_review`, EXP-194) match;
    // CodingSessionLiveness.isLive still applies the staleness cut on top.
    @Query("SELECT * FROM coding_sessions WHERE status IN (:statuses) ORDER BY started_at DESC")
    fun observeByStatuses(statuses: List<String>): Flow<List<CodingSessionEntity>>

    // EXP-734: the team's runs with an OPEN pull request of their OWN — an
    // action or chat run (issue_id NULL) whose PR links no issue, so nothing
    // in the issues table can represent it in Reviews. Newest first; the
    // client still collapses by pr_url (a resumed run can share one).
    @Query(
        "SELECT * FROM coding_sessions WHERE team_id = :teamId " +
            "AND issue_id IS NULL AND pr_state = 'open' ORDER BY started_at DESC"
    )
    fun observeOpenPrRunsByTeam(teamId: String): Flow<List<CodingSessionEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: CodingSessionEntity)

    @Query("DELETE FROM coding_sessions WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM coding_sessions")
    suspend fun clear()
}

@Dao
interface ActionDao {
    @Query("SELECT * FROM actions WHERE team_id = :teamId ORDER BY sort_order, name")
    fun observeByTeam(teamId: String): Flow<List<ActionEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: ActionEntity)

    @Query("DELETE FROM actions WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM actions")
    suspend fun clear()
}

@Dao
interface AutomationDao {
    @Query("SELECT * FROM automations WHERE team_id = :teamId ORDER BY sort_order, created_at")
    fun observeByTeam(teamId: String): Flow<List<AutomationEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: AutomationEntity)

    @Query("DELETE FROM automations WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM automations")
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
interface DeviceDao {
    @Query("SELECT * FROM devices ORDER BY last_seen_at DESC")
    fun observeAll(): Flow<List<DeviceEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: DeviceEntity)

    @Query("DELETE FROM devices WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM devices")
    suspend fun clear()
}

@Dao
interface DeviceWorktreeDao {
    @Query("SELECT * FROM device_worktrees ORDER BY repo_full_name, branch")
    fun observeAll(): Flow<List<DeviceWorktreeEntity>>

    @Query("SELECT * FROM device_worktrees WHERE device_row_id = :deviceRowId ORDER BY repo_full_name, branch")
    fun observeByDevice(deviceRowId: String): Flow<List<DeviceWorktreeEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: DeviceWorktreeEntity)

    @Query("DELETE FROM device_worktrees WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM device_worktrees")
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

    // No per-shape delete: dropping the cursor is how a refetch used to start,
    // and it blanked the table until the snapshot landed. A refetch is now
    // requested by upserting the row with needs_refetch = true (EXP-264).

    @Query("DELETE FROM electric_offsets")
    suspend fun clear()
}
