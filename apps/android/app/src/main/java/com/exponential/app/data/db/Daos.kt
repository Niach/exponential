package com.exponential.app.data.db

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import com.exponential.app.domain.SessionTree
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine

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

    // #EXP-1382: the ACTIVE team's issues (the global search's local pool),
    // over its live boards only.
    @Query(
        "SELECT issues.* FROM issues INNER JOIN boards ON boards.id = issues.board_id " +
            "WHERE boards.team_id = :teamId AND boards.deleted_at IS NULL",
    )
    fun observeByTeam(teamId: String): Flow<List<IssueEntity>>

    // Cross-board "My Issues" view (masterplan §5a): everything assigned to me.
    @Query("SELECT * FROM issues WHERE assignee_id = :userId ORDER BY sort_order, created_at")
    fun observeByAssignee(userId: String): Flow<List<IssueEntity>>

    // VAPP-119: a team's issues (through their boards), the app host's `exp:issues`.
    @Query("SELECT i.* FROM issues i JOIN boards b ON b.id = i.board_id WHERE b.team_id = :teamId")
    fun observeByTeamForApps(teamId: String): Flow<List<IssueEntity>>

    @Query("SELECT * FROM issues WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<IssueEntity?>

    // EXP-1231: the issue created from a draft (`issues.draft_id`), the open
    // draft page's proof it was created elsewhere.
    @Query("SELECT * FROM issues WHERE draft_id = :draftId LIMIT 1")
    fun observeByDraftId(draftId: String): Flow<IssueEntity?>

    // Existence probe for the push-tap fallback's fill-a-hole guard (EXP-264);
    // suspend so it can run inside the same withTransaction as the insert.
    @Query("SELECT EXISTS(SELECT 1 FROM issues WHERE id = :id)")
    suspend fun exists(id: String): Boolean

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

    // Account-wide (EXP-878): the drafts list spans every team, so it resolves
    // each row's status against ITS team's rows rather than the active team's.
    @Query("SELECT * FROM issue_statuses ORDER BY sort_order, created_at, id")
    fun observeAll(): Flow<List<IssueStatusEntity>>

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
interface IssueRelationDao {
    // BOTH sides of the edge (EXP-736): the shape syncs rows scoped by their
    // SOURCE issue, so an issue that is only the TARGET of a relation still
    // has to find it. The screen resolves the other issue and drops rows
    // whose counterpart hasn't synced.
    @Query("SELECT * FROM issue_relations WHERE issue_id = :issueId OR related_issue_id = :issueId")
    fun observeForIssue(issueId: String): Flow<List<IssueRelationEntity>>

    // EXP-897: every synced edge — the Work screen's stack/batch badge
    // resolves blockers across issues, not for one at a time.
    @Query("SELECT * FROM issue_relations")
    fun observeAll(): Flow<List<IssueRelationEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: IssueRelationEntity)

    @Query("DELETE FROM issue_relations WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM issue_relations")
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
    // EXP-1267 ×4: join order (`membersInJoinOrder` re-sorts across the mixed
    // Postgres-text/ISO stamp forms; this keeps the DB emission stable).
    @Query("SELECT * FROM team_members WHERE team_id = :teamId ORDER BY created_at, id")
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

    // VAPP-119: a team's comments, the app host's `exp:comments`.
    @Query("SELECT * FROM comments WHERE team_id = :teamId")
    fun observeByTeam(teamId: String): Flow<List<CommentEntity>>

    /** EXP-920: the one row an entity preview names. */
    @Query("SELECT * FROM comments WHERE id = :id")
    fun observeById(id: String): Flow<CommentEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: CommentEntity)

    @Query("DELETE FROM comments WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM comments")
    suspend fun clear()
}

// EXP-778: the caller's personal pins. The per-account DB only ever holds
// the signed-in user's rows (the shape is `user_id = me`), so no user filter.
// Sync-only since EXP-976: the phone has no sidebar, so nothing reads pins
// (EXP-858 dropped the toggles, EXP-976 the switcher's Pinned list).
@Dao
interface PinDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: PinEntity)

    @Query("DELETE FROM pins WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM pins")
    suspend fun clear()
}

// EXP-878: the caller's unsent issue drafts. Like pins, the per-account DB
// only ever holds the signed-in user's rows (the shape is `user_id = me`), so
// there is no user filter — and the list is account-wide (every team), because
// a draft is personal, not board state.
@Dao
interface IssueDraftDao {
    @Query("SELECT * FROM issue_drafts ORDER BY updated_at DESC, created_at DESC")
    fun observeAll(): Flow<List<IssueDraftEntity>>

    @Query("SELECT * FROM issue_drafts WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<IssueDraftEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: IssueDraftEntity)

    @Query("DELETE FROM issue_drafts WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM issue_drafts")
    suspend fun clear()
}

// EXP-1291 Threads v2: the session tree. Roots (`parent_id IS NULL`) are
// threads (`kind = thread`) and team agents (`kind = agent`); every session
// of one tree shares `root_id`.
@Dao
interface SessionDao {
    @Query("SELECT * FROM sessions WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<SessionEntity?>

    @Query("SELECT * FROM sessions WHERE id = :id LIMIT 1")
    suspend fun getById(id: String): SessionEntity?

    // Every visible session (the Threads list derives groups per tree off it).
    @Query("SELECT * FROM sessions ORDER BY created_at, id")
    fun observeAll(): Flow<List<SessionEntity>>

    @Query("SELECT * FROM sessions WHERE team_id = :teamId ORDER BY created_at, id")
    fun observeByTeam(teamId: String): Flow<List<SessionEntity>>

    // One tree: the root and every descendant, creation order.
    @Query("SELECT * FROM sessions WHERE root_id = :rootId OR id = :rootId ORDER BY created_at, id")
    fun observeTree(rootId: String): Flow<List<SessionEntity>>

    // #EXP-1380: the sessions a chat's cards name (another tree's root).
    @Query("SELECT * FROM sessions WHERE id IN (:ids) ORDER BY created_at, id")
    fun observeByIds(ids: List<String>): Flow<List<SessionEntity>>

    // NEW-2: the agents' CONVERSATION sessions (roots with `agent_id`; the
    // definitions live in `agents`), creation order.
    @Query("SELECT * FROM sessions WHERE parent_id IS NULL AND agent_id IS NOT NULL ORDER BY created_at, id")
    fun observeAgents(): Flow<List<SessionEntity>>

    // Every session not ended (the bar's live dot, the switcher's per-team dots).
    @Query("SELECT * FROM sessions WHERE status != 'ended' ORDER BY created_at, id")
    fun observeLive(): Flow<List<SessionEntity>>

    // The roots (threads) an issue sits in, newest activity first.
    @Query(
        "SELECT s.* FROM sessions s INNER JOIN session_issues si ON si.session_id = s.id " +
            "WHERE si.issue_id = :issueId ORDER BY s.updated_at DESC, s.created_at DESC, s.id",
    )
    fun observeRootsOfIssue(issueId: String): Flow<List<SessionEntity>>

    // App-link resolution (EXP-1288): team SLUG + root id → the id, only when
    // the synced root sits in that team.
    @Query(
        "SELECT s.id FROM sessions s JOIN teams w ON w.id = s.team_id " +
            "WHERE s.id = :sessionId AND w.slug = :teamSlug LIMIT 1",
    )
    suspend fun findIdByTeamRef(teamSlug: String, sessionId: String): String?

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: SessionEntity)

    @Query("DELETE FROM sessions WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM sessions")
    suspend fun clear()
}

// EXP-1291: a thread's issues (always on the ROOT), display order.
@Dao
interface SessionIssueDao {
    @Query("SELECT * FROM session_issues ORDER BY session_id, sort_order, created_at, id")
    fun observeAll(): Flow<List<SessionIssueEntity>>

    @Query("SELECT * FROM session_issues WHERE session_id = :rootId ORDER BY sort_order, created_at, id")
    fun observeByRoot(rootId: String): Flow<List<SessionIssueEntity>>

    @Query("SELECT * FROM session_issues WHERE issue_id = :issueId ORDER BY created_at, id")
    fun observeByIssue(issueId: String): Flow<List<SessionIssueEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: SessionIssueEntity)

    @Query("DELETE FROM session_issues WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM session_issues")
    suspend fun clear()
}

// EXP-1291: the Conversation, oldest first.
@Dao
interface MessageDao {
    @Query("SELECT * FROM messages ORDER BY created_at, id")
    fun observeAll(): Flow<List<MessageEntity>>

    // NEW-8: a session's thread = the messages of its own conversation (the
    // `conversations` row with that `session_id`); a message whose
    // conversation has not synced yet is held back until it does.
    @Query(
        "SELECT * FROM messages WHERE conversation_id IN " +
            "(SELECT id FROM conversations WHERE session_id = :sessionId) ORDER BY created_at, id",
    )
    fun observeBySession(sessionId: String): Flow<List<MessageEntity>>

    // Every message of one tree (the root's conversation + its children's).
    @Query("SELECT * FROM messages WHERE root_id = :rootId ORDER BY created_at, id")
    fun observeByRoot(rootId: String): Flow<List<MessageEntity>>

    // #EXP-1380: the card-less rows of the given sessions' conversations —
    // all a session TITLE reads (its first prompt), never a whole transcript.
    @Query(
        "SELECT * FROM messages WHERE (card IS NULL OR card = 'null') AND conversation_id IN " +
            "(SELECT id FROM conversations WHERE session_id IN (:sessionIds)) " +
            "AND (author_kind = 'user' OR author_session_id IS NOT NULL) ORDER BY created_at, id",
    )
    fun observeTitleRows(sessionIds: List<String>): Flow<List<MessageEntity>>

    // #EXP-1399: the given messages by id (an `ask` mirror's ORIGIN card,
    // which lives in another tree than the conversation drawing it).
    @Query("SELECT * FROM messages WHERE id IN (:ids) ORDER BY created_at, id")
    fun observeByIds(ids: List<String>): Flow<List<MessageEntity>>

    // A session's person messages the device has not consumed (the queue bar).
    @Query(
        "SELECT * FROM messages WHERE conversation_id IN " +
            "(SELECT id FROM conversations WHERE session_id = :sessionId) AND author_kind = 'user' " +
            "AND consumed_at IS NULL ORDER BY created_at, id",
    )
    fun observeQueued(sessionId: String): Flow<List<MessageEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: MessageEntity)

    @Query("DELETE FROM messages WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM messages")
    suspend fun clear()
}

/**
 * NEW-8: [rows] keyed through their conversation ([SessionTree.messages]):
 * each message carries the session its conversation belongs to, and the
 * result waits on the conversations shape too (re-emits as it syncs).
 */
fun ExponentialDatabase.keyedMessages(rows: Flow<List<MessageEntity>>): Flow<List<SessionTree.Message>> =
    combine(rows, conversationDao().observeAll(), SessionTree::messages)

// EXP-1323: the viewer's read marks (the per-account DB only holds the
// signed-in user's rows, like pins).
@Dao
interface SessionReadDao {
    @Query("SELECT * FROM session_reads")
    fun observeAll(): Flow<List<SessionReadEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: SessionReadEntity)

    @Query("DELETE FROM session_reads WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM session_reads")
    suspend fun clear()
}

// NEW-3: conversations (one per session; NEW-8: dm/channel rows carry no session).
@Dao
interface ConversationDao {
    // NEW-8: every message is keyed through these ([com.exponential.app.domain.MessageConversation]).
    @Query("SELECT * FROM conversations")
    fun observeAll(): Flow<List<ConversationEntity>>

    // Thread e4a04d22: one team's conversations (the Chats list reads their digests).
    @Query("SELECT * FROM conversations WHERE team_id = :teamId")
    fun observeByTeam(teamId: String): Flow<List<ConversationEntity>>

    @Query("SELECT * FROM conversations WHERE session_id = :sessionId LIMIT 1")
    fun observeBySession(sessionId: String): Flow<ConversationEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: ConversationEntity)

    @Query("DELETE FROM conversations WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM conversations")
    suspend fun clear()
}

// NEW-2: the team agents (definitions), rail order.
@Dao
interface AgentDefinitionDao {
    @Query("SELECT * FROM agents ORDER BY sort_order, name, id")
    fun observeAll(): Flow<List<AgentDefinitionEntity>>

    @Query("SELECT * FROM agents WHERE team_id = :teamId ORDER BY sort_order, name, id")
    fun observeByTeam(teamId: String): Flow<List<AgentDefinitionEntity>>

    @Query("SELECT * FROM agents WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<AgentDefinitionEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: AgentDefinitionEntity)

    @Query("DELETE FROM agents WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM agents")
    suspend fun clear()
}

// VAPP-110: the team's apps (trashed rows hidden) and their versions.
@Dao
interface AppDao {
    @Query("SELECT * FROM apps WHERE team_id = :teamId AND deleted_at IS NULL ORDER BY name COLLATE NOCASE, id")
    fun observeByTeam(teamId: String): Flow<List<AppEntity>>

    @Query("SELECT * FROM apps WHERE id = :id AND deleted_at IS NULL LIMIT 1")
    fun observeById(id: String): Flow<AppEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: AppEntity)

    @Query("DELETE FROM apps WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM apps")
    suspend fun clear()
}

@Dao
interface AppVersionDao {
    @Query("SELECT * FROM app_versions WHERE app_id = :appId ORDER BY number DESC")
    fun observeByApp(appId: String): Flow<List<AppVersionEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: AppVersionEntity)

    @Query("DELETE FROM app_versions WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM app_versions")
    suspend fun clear()
}

// VAPP-122: an app's records (the `exp:records` binding derives over them).
@Dao
interface AppRecordDao {
    @Query("SELECT * FROM app_records WHERE app_id = :appId ORDER BY created_at DESC, id")
    fun observeByApp(appId: String): Flow<List<AppRecordEntity>>

    @Query("SELECT * FROM app_records WHERE team_id = :teamId ORDER BY created_at DESC, id")
    fun observeByTeam(teamId: String): Flow<List<AppRecordEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: AppRecordEntity)

    @Query("DELETE FROM app_records WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM app_records")
    suspend fun clear()
}

// NEW-2: the member teams' events, newest first.
@Dao
interface EventDao {
    @Query("SELECT * FROM events ORDER BY created_at DESC, id")
    fun observeAll(): Flow<List<EventEntity>>

    @Query("SELECT * FROM events WHERE team_id = :teamId ORDER BY created_at DESC, id")
    fun observeByTeam(teamId: String): Flow<List<EventEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: EventEntity)

    @Query("DELETE FROM events WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM events")
    suspend fun clear()
}

// NEW-2: the saved prompts (the composer's `/` menu, Settings → Prompts).
@Dao
interface PromptDao {
    @Query("SELECT * FROM prompts ORDER BY sort_order, name")
    fun observeAll(): Flow<List<PromptEntity>>

    @Query("SELECT * FROM prompts WHERE team_id = :teamId ORDER BY sort_order, name")
    fun observeByTeam(teamId: String): Flow<List<PromptEntity>>

    @Query("SELECT * FROM prompts WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<PromptEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: PromptEntity)

    @Query("DELETE FROM prompts WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM prompts")
    suspend fun clear()
}

// EXP-1291: pull requests (all PR + branch state).
@Dao
interface PullRequestDao {
    @Query("SELECT * FROM pull_requests ORDER BY created_at, id")
    fun observeAll(): Flow<List<PullRequestEntity>>

    @Query("SELECT * FROM pull_requests WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<PullRequestEntity?>

    @Query("SELECT * FROM pull_requests WHERE team_id = :teamId ORDER BY created_at, id")
    fun observeByTeam(teamId: String): Flow<List<PullRequestEntity>>

    // A thread's PRs (`root_session_id`), opening order.
    @Query("SELECT * FROM pull_requests WHERE root_session_id = :rootId ORDER BY created_at, id")
    fun observeByRoot(rootId: String): Flow<List<PullRequestEntity>>

    // An ISSUE's PRs (pull_requests ⋈ pull_request_issues), newest first —
    // what replaced the dropped issues.pr_* / branch columns.
    @Query(
        "SELECT pr.* FROM pull_requests pr INNER JOIN pull_request_issues pri " +
            "ON pri.pull_request_id = pr.id WHERE pri.issue_id = :issueId " +
            "ORDER BY pr.created_at DESC, pr.id",
    )
    fun observeByIssue(issueId: String): Flow<List<PullRequestEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: PullRequestEntity)

    @Query("DELETE FROM pull_requests WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM pull_requests")
    suspend fun clear()
}

// EXP-1291: which issues a PR closes.
@Dao
interface PullRequestIssueDao {
    @Query("SELECT * FROM pull_request_issues ORDER BY created_at, id")
    fun observeAll(): Flow<List<PullRequestIssueEntity>>

    @Query("SELECT * FROM pull_request_issues WHERE pull_request_id = :pullRequestId ORDER BY created_at, id")
    fun observeByPullRequest(pullRequestId: String): Flow<List<PullRequestIssueEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: PullRequestIssueEntity)

    @Query("DELETE FROM pull_request_issues WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM pull_request_issues")
    suspend fun clear()
}

@Dao
interface AttachmentDao {
    @Query("SELECT * FROM attachments WHERE issue_id = :issueId ORDER BY created_at ASC")
    fun observeByIssue(issueId: String): Flow<List<AttachmentEntity>>

    // VAPP-119: a team's attachments, the app host's `exp:attachments`.
    @Query("SELECT * FROM attachments WHERE team_id = :teamId")
    fun observeByTeam(teamId: String): Flow<List<AttachmentEntity>>

    /** EXP-920: the one row an entity preview names. */
    @Query("SELECT * FROM attachments WHERE id = :id")
    fun observeById(id: String): Flow<AttachmentEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: AttachmentEntity)

    @Query("DELETE FROM attachments WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM attachments")
    suspend fun clear()
}

@Dao
interface TeamInviteDao {
    /**
     * The team's UNACCEPTED invites — NOT its pending ones. EXP-630 (#810):
     * a revoked placeholder invite stays a row with `expires_at = now` (the
     * member row wears "Invite expired" off it), so this set holds revoked and
     * lapsed links too. Readers wanting the open links take
     * `pendingInvites()` (domain) over it; no SQL expiry filter because
     * `expires_at` mixes Postgres text and ISO forms.
     */
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
