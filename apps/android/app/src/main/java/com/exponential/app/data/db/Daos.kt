package com.exponential.app.data.db

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import com.exponential.app.data.sealed.SealedContent
import com.exponential.app.data.sealed.SealedKeys
import com.exponential.app.domain.SessionTree
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map

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
    // EXP-1459: `#IDENT` refs of a sealed text → issue ids (`refIssueIds`), case-insensitive.
    @Query("SELECT * FROM issues WHERE upper(identifier) IN (:identifiers)")
    suspend fun byIdentifiers(identifiers: List<String>): List<IssueEntity>

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

    @Query("SELECT * FROM issues WHERE id IN (:ids)")
    suspend fun byIds(ids: List<String>): List<IssueEntity>

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

    // EXP-1459: `@email` mentions → user ids (`mentionUserIds`), case-insensitive.
    @Query("SELECT * FROM users WHERE lower(email) IN (:emails)")
    suspend fun byEmails(emails: List<String>): List<UserEntity>

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

// #FEED-219: the caller's composer drafts (the shape is `user_id = me`), one
// per target key; a composer reads only its own key.
@Dao
interface ComposerDraftDao {
    @Query("SELECT * FROM composer_drafts WHERE `key` = :key ORDER BY updated_at DESC LIMIT 1")
    fun observeByKey(key: String): Flow<ComposerDraftEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: ComposerDraftEntity)

    @Query("DELETE FROM composer_drafts WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM composer_drafts WHERE `key` = :key")
    suspend fun deleteByKey(key: String)

    @Query("DELETE FROM composer_drafts")
    suspend fun clear()
}

// EXP-1291: the session tree. Roots (`parent_id IS NULL`) are
// chats (`kind = thread`) and team agents (`kind = agent`); every session
// of one tree shares `root_id`.
@Dao
abstract class SessionDao {
    @Query("SELECT * FROM sessions WHERE id = :id LIMIT 1")
    abstract fun rawObserveById(id: String): Flow<SessionEntity?>
    fun observeById(id: String): Flow<SessionEntity?> = rawObserveById(id).openedSession()

    // RAW (sealed as synced): writers and link resolution, never a screen.
    @Query("SELECT * FROM sessions WHERE id = :id LIMIT 1")
    abstract suspend fun getById(id: String): SessionEntity?

    // EXP-1459: the caller's conversation root with an agent / app (raw).
    @Query("SELECT * FROM sessions WHERE parent_id IS NULL AND agent_id = :agentId AND user_id = :userId ORDER BY created_at DESC LIMIT 1")
    abstract suspend fun agentConversation(agentId: String, userId: String): SessionEntity?

    @Query("SELECT * FROM sessions WHERE parent_id IS NULL AND app_id = :appId AND user_id = :userId ORDER BY created_at DESC LIMIT 1")
    abstract suspend fun appConversation(appId: String, userId: String): SessionEntity?

    // Every visible session (the Chats list derives groups per tree off it).
    // EXP-1635: a team's newest chats (the sharing shortcuts): roots that are
    // neither an agent's conversation nor an app's editor nor encrypted, unarchived.
    @Query(
        """SELECT * FROM sessions WHERE team_id = :teamId AND parent_id IS NULL
        AND agent_id IS NULL AND app_id IS NULL AND seal_public_key IS NULL AND sealed_title IS NULL
        AND id NOT IN (SELECT session_id FROM session_reads WHERE archived_at IS NOT NULL)
        ORDER BY updated_at DESC LIMIT :limit""",
    )
    abstract fun observeRecentChats(teamId: String, limit: Int): Flow<List<SessionEntity>>

    // #EXP-1642 (system-search.json): every team's newest plain chats, the same
    // filter (never an agent's conversation, an app's editor or an encrypted chat).
    @Query(
        """SELECT * FROM sessions WHERE parent_id IS NULL
        AND agent_id IS NULL AND app_id IS NULL AND seal_public_key IS NULL AND sealed_title IS NULL
        AND id NOT IN (SELECT session_id FROM session_reads WHERE archived_at IS NOT NULL)
        ORDER BY updated_at DESC LIMIT :limit""",
    )
    abstract fun observeSearchableChats(limit: Int): Flow<List<SessionEntity>>

    @Query("SELECT * FROM sessions ORDER BY created_at, id")
    abstract fun rawObserveAll(): Flow<List<SessionEntity>>
    fun observeAll(): Flow<List<SessionEntity>> = rawObserveAll().openedSessions()

    @Query("SELECT * FROM sessions WHERE team_id = :teamId ORDER BY created_at, id")
    abstract fun rawObserveByTeam(teamId: String): Flow<List<SessionEntity>>
    fun observeByTeam(teamId: String): Flow<List<SessionEntity>> = rawObserveByTeam(teamId).openedSessions()

    // One tree: the root and every descendant, creation order.
    @Query("SELECT * FROM sessions WHERE root_id = :rootId OR id = :rootId ORDER BY created_at, id")
    abstract fun rawObserveTree(rootId: String): Flow<List<SessionEntity>>
    fun observeTree(rootId: String): Flow<List<SessionEntity>> = rawObserveTree(rootId).openedSessions()

    // #EXP-1380: the sessions a chat's cards name (another tree's root).
    @Query("SELECT * FROM sessions WHERE id IN (:ids) ORDER BY created_at, id")
    abstract fun rawObserveByIds(ids: List<String>): Flow<List<SessionEntity>>
    fun observeByIds(ids: List<String>): Flow<List<SessionEntity>> = rawObserveByIds(ids).openedSessions()

    // NEW-2: the agents' CONVERSATION sessions (roots with `agent_id`; the
    // definitions live in `agents`), creation order.
    // #ERR-19: [observeTree] / [observeAgents] / [observeLive] WITHOUT the
    // digest, for readers that never read it (SESSION_COLUMNS_NO_DIGEST).
    @Query("SELECT $SESSION_COLUMNS_NO_DIGEST FROM sessions WHERE root_id = :rootId OR id = :rootId ORDER BY created_at, id")
    abstract fun rawObserveTreeNoDigest(rootId: String): Flow<List<SessionEntity>>
    fun observeTreeNoDigest(rootId: String): Flow<List<SessionEntity>> = rawObserveTreeNoDigest(rootId).openedSessions()

    @Query("SELECT $SESSION_COLUMNS_NO_DIGEST FROM sessions WHERE parent_id IS NULL AND agent_id IS NOT NULL ORDER BY created_at, id")
    abstract fun rawObserveAgentsNoDigest(): Flow<List<SessionEntity>>
    fun observeAgentsNoDigest(): Flow<List<SessionEntity>> = rawObserveAgentsNoDigest().openedSessions()

    @Query("SELECT $SESSION_COLUMNS_NO_DIGEST FROM sessions WHERE status != 'ended' ORDER BY created_at, id")
    abstract fun rawObserveLiveNoDigest(): Flow<List<SessionEntity>>
    fun observeLiveNoDigest(): Flow<List<SessionEntity>> = rawObserveLiveNoDigest().openedSessions()

    @Query("SELECT * FROM sessions WHERE parent_id IS NULL AND agent_id IS NOT NULL ORDER BY created_at, id")
    abstract fun rawObserveAgents(): Flow<List<SessionEntity>>
    fun observeAgents(): Flow<List<SessionEntity>> = rawObserveAgents().openedSessions()

    // Every session not ended (the bar's live dot, the switcher's per-team dots).
    @Query("SELECT * FROM sessions WHERE status != 'ended' ORDER BY created_at, id")
    abstract fun rawObserveLive(): Flow<List<SessionEntity>>
    fun observeLive(): Flow<List<SessionEntity>> = rawObserveLive().openedSessions()

    // The roots (chats) an issue sits in, newest activity first.
    @Query(
        "SELECT s.* FROM sessions s INNER JOIN session_issues si ON si.session_id = s.id " +
            "WHERE si.issue_id = :issueId ORDER BY s.updated_at DESC, s.created_at DESC, s.id",
    )
    abstract fun rawObserveRootsOfIssue(issueId: String): Flow<List<SessionEntity>>
    fun observeRootsOfIssue(issueId: String): Flow<List<SessionEntity>> = rawObserveRootsOfIssue(issueId).openedSessions()

    // App-link resolution (EXP-1288): team SLUG + root id → the id, only when
    // the synced root sits in that team.
    @Query(
        "SELECT s.id FROM sessions s JOIN teams w ON w.id = s.team_id " +
            "WHERE s.id = :sessionId AND w.slug = :teamSlug LIMIT 1",
    )
    abstract suspend fun findIdByTeamRef(teamSlug: String, sessionId: String): String?

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    abstract suspend fun upsert(item: SessionEntity)

    @Query("DELETE FROM sessions WHERE id = :id")
    abstract suspend fun deleteById(id: String)

    @Query("DELETE FROM sessions")
    abstract suspend fun clear()
}

// EXP-1291: a chat's issues (always on the ROOT), display order.
@Dao
interface SessionIssueDao {
    @Query("SELECT * FROM session_issues ORDER BY session_id, sort_order, created_at, id")
    fun observeAll(): Flow<List<SessionIssueEntity>>

    @Query("SELECT * FROM session_issues WHERE session_id = :rootId ORDER BY sort_order, created_at, id")
    fun observeByRoot(rootId: String): Flow<List<SessionIssueEntity>>

    @Query("SELECT * FROM session_issues WHERE issue_id = :issueId ORDER BY created_at, id")
    fun observeByIssue(issueId: String): Flow<List<SessionIssueEntity>>

    // FEED-243: a team's links (the app host's `exp:issues?touched=`).
    @Query("SELECT * FROM session_issues WHERE team_id = :teamId ORDER BY session_id, issue_id, id")
    fun observeByTeam(teamId: String): Flow<List<SessionIssueEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: SessionIssueEntity)

    @Query("DELETE FROM session_issues WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM session_issues")
    suspend fun clear()
}

// EXP-1291: the Conversation, oldest first.
@Dao
abstract class MessageDao {
    @Query("SELECT * FROM messages ORDER BY created_at, id")
    abstract fun rawObserveAll(): Flow<List<MessageEntity>>
    fun observeAll(): Flow<List<MessageEntity>> = rawObserveAll().openedMessages()

    // #EXP-1730: a session's chat = the messages keyed to it
    // (`messages.session_id`).
    @Query("SELECT * FROM messages WHERE session_id = :sessionId ORDER BY created_at, id")
    abstract fun rawObserveBySession(sessionId: String): Flow<List<MessageEntity>>
    fun observeBySession(sessionId: String): Flow<List<MessageEntity>> = rawObserveBySession(sessionId).openedMessages()

    // Every message of one tree (the root's conversation + its children's).
    @Query("SELECT * FROM messages WHERE root_id = :rootId ORDER BY created_at, id")
    abstract fun rawObserveByRoot(rootId: String): Flow<List<MessageEntity>>
    fun observeByRoot(rootId: String): Flow<List<MessageEntity>> = rawObserveByRoot(rootId).openedMessages()

    // #EXP-1380 (TS `useNewestMessages`): a chat's NEWEST [limit] messages of
    // the tree, oldest first; Load earlier widens [limit].
    @Query(
        "SELECT * FROM (SELECT * FROM messages WHERE root_id = :rootId ORDER BY created_at DESC, id DESC LIMIT :limit) " +
            "ORDER BY created_at, id",
    )
    abstract fun rawObserveNewestByRoot(rootId: String, limit: Int): Flow<List<MessageEntity>>
    fun observeNewestByRoot(rootId: String, limit: Int): Flow<List<MessageEntity>> =
        rawObserveNewestByRoot(rootId, limit).openedMessages()

    // results-tab.json: a chat's result CANDIDATES = the tree's sessions' OWN
    // card rows (author_session_id = session_id), never the loaded window
    // (the Results tab + its count). #ERR-19: read as STAMPS (no body, no
    // card), the rows themselves only when new or changed ([observeResultCards]).
    @Query(
        "SELECT id, (updated_at || '|' || IFNULL(answered_at, '') || '|' || IFNULL(consumed_at, '') || '|' || " +
            "length(card) || '|' || length(body)) AS stamp FROM messages WHERE root_id = :rootId " +
            "AND author_session_id IS NOT NULL AND author_session_id = session_id " +
            "AND card IS NOT NULL AND card != 'null' ORDER BY created_at, id",
    )
    abstract fun observeResultCardStamps(rootId: String): Flow<List<RowStamp>>

    @Query("SELECT * FROM messages WHERE id IN (:ids) AND card IS NOT NULL AND card != 'null'")
    abstract suspend fun cardRowsByIds(ids: List<String>): List<MessageEntity>

    // Large chats: the newest createdAt of a session's rows by one author kind
    // (the session view's "news arrived" stamp) — the max the whole session
    // read used to fold, without reading (or opening) a single row.
    @Query(
        "SELECT created_at FROM messages WHERE session_id = :sessionId AND author_kind = :authorKind " +
            "ORDER BY created_at DESC LIMIT 1",
    )
    abstract fun observeNewestCreatedAt(sessionId: String, authorKind: String): Flow<String?>

    // Large chats: a session's CARD rows (replies, plans, questions, surfaces):
    // everything the composer's quick replies and the session's own cards
    // read; a card-less row can never carry a card once opened. #ERR-19: as
    // STAMPS, the rows loaded only when new or changed
    // ([observeCardRowsBySession] with the kinds a reader keeps).
    @Query(
        "SELECT id, (updated_at || '|' || IFNULL(answered_at, '') || '|' || IFNULL(consumed_at, '') || '|' || " +
            "length(card) || '|' || length(body)) AS stamp FROM messages WHERE session_id = :sessionId " +
            "AND card IS NOT NULL AND card != 'null' ORDER BY created_at, id",
    )
    abstract fun observeCardRowStampsBySession(sessionId: String): Flow<List<RowStamp>>

    // Large chats (session-title.json `firstPrompt`): the rows a session TITLE
    // can read — card-less, a person's or ANOTHER session's input. #ERR-19:
    // summarized per session (count + newest stamp, no bodies), and only each
    // session's OLDEST rows up to its first usable prompt are ever loaded
    // ([observeFirstTitleRows] / [observeFirstTitleRowsByRoot]), never whole
    // transcripts.
    @Query(
        "SELECT session_id, (COUNT(*) || '|' || MAX(updated_at) || '|' || MIN(created_at)) AS stamp FROM messages " +
            "WHERE session_id IN (:sessionIds) AND $TITLE_ROW GROUP BY session_id ORDER BY session_id",
    )
    abstract fun observeTitleStamps(sessionIds: List<String>): Flow<List<TitleStamp>>

    @Query(
        "SELECT session_id, (COUNT(*) || '|' || MAX(updated_at) || '|' || MIN(created_at)) AS stamp FROM messages " +
            "WHERE root_id = :rootId AND session_id IS NOT NULL AND session_id != '' AND $TITLE_ROW " +
            "GROUP BY session_id ORDER BY session_id",
    )
    abstract fun observeTitleStampsByRoot(rootId: String): Flow<List<TitleStamp>>

    @Query("SELECT * FROM messages WHERE session_id = :sessionId AND $TITLE_ROW ORDER BY created_at, id LIMIT :limit OFFSET :offset")
    abstract suspend fun titleRowsPage(sessionId: String, limit: Int, offset: Int): List<MessageEntity>

    // #EXP-1399: the given messages by id (an `ask` mirror's ORIGIN card,
    // which lives in another tree than the conversation drawing it).
    @Query("SELECT * FROM messages WHERE id IN (:ids) ORDER BY created_at, id")
    abstract fun rawObserveByIds(ids: List<String>): Flow<List<MessageEntity>>
    fun observeByIds(ids: List<String>): Flow<List<MessageEntity>> = rawObserveByIds(ids).openedMessages()

    // A session's person messages the device has not consumed (the queue bar).
    @Query(
        "SELECT * FROM messages WHERE session_id = :sessionId AND author_kind = 'user' " +
            "AND consumed_at IS NULL ORDER BY created_at, id",
    )
    abstract fun rawObserveQueued(sessionId: String): Flow<List<MessageEntity>>
    fun observeQueued(sessionId: String): Flow<List<MessageEntity>> = rawObserveQueued(sessionId).openedMessages()

    // #FEED-237 (message-delivery.json `outboxCases`): the viewer's newest own
    // rows of a session (an outbox entry's synced row, matched by id or text).
    @Query(
        "SELECT * FROM messages WHERE session_id = :sessionId AND author_kind = 'user' " +
            "AND author_id = :authorId ORDER BY created_at DESC, id DESC LIMIT :limit",
    )
    abstract fun rawObserveNewestOwn(sessionId: String, authorId: String, limit: Int): Flow<List<MessageEntity>>
    fun observeNewestOwn(sessionId: String, authorId: String, limit: Int): Flow<List<MessageEntity>> =
        rawObserveNewestOwn(sessionId, authorId, limit).openedMessages()

    // #FEED-237: the rows the given outbox entries were stored as.
    @Query("SELECT id FROM messages WHERE id IN (:ids)")
    abstract suspend fun existingIds(ids: List<String>): List<String>

    // Android 0.14.134 (steer-enet.json SENT INPUTS): a session's NEWEST
    // [limit] card-less person inputs the device consumed, oldest first:
    // what a no-feed placeholder shows as the viewer's own bubbles.
    @Query(
        "SELECT * FROM (SELECT * FROM messages WHERE session_id = :sessionId AND author_kind = 'user' " +
            "AND consumed_at IS NOT NULL AND (card IS NULL OR card = 'null') " +
            "ORDER BY created_at DESC, id DESC LIMIT :limit) ORDER BY created_at, id",
    )
    abstract fun rawObserveConsumedInputs(sessionId: String, limit: Int): Flow<List<MessageEntity>>
    fun observeConsumedInputs(sessionId: String, limit: Int): Flow<List<MessageEntity>> =
        rawObserveConsumedInputs(sessionId, limit).openedMessages()

    // EXP-1459: one message RAW (sealed as synced): the answer re-seals its card.
    @Query("SELECT * FROM messages WHERE id = :id LIMIT 1")
    abstract suspend fun getById(id: String): MessageEntity?

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    abstract suspend fun upsert(item: MessageEntity)

    @Query("DELETE FROM messages WHERE id = :id")
    abstract suspend fun deleteById(id: String)

    @Query("DELETE FROM messages")
    abstract suspend fun clear()
}

/**
 * #EXP-1730: [rows] as tree messages keyed by their own `session_id`
 * ([SessionTree.messages]); a row without one (dm/channel) is left out.
 */
@Suppress("UnusedReceiverParameter")
fun ExponentialDatabase.keyedMessages(rows: Flow<List<MessageEntity>>): Flow<List<SessionTree.Message>> =
    rows.map { SessionTree.messages(it) }

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

// #ERR-19: every `sessions` column but the list digest (NULL): the readers
// that never show a preview. A sealed digest keeps its bodies whole (~24 KB
// a child of a busy tree), and Room re-runs every sessions query on any row's
// write (a message's digest update): a digest-less reader no longer re-reads
// them all per write.
private const val SESSION_COLUMNS_NO_DIGEST =
    "id, team_id, parent_id, root_id, user_id, device_id, cli, account, model, effort, plan_mode, computer_use, ultracode, repository_id, branch, status, busy, background, needs_input, blocked, started_at, last_turn_at, ended_at, shared, title, guide, created_at, updated_at, agent_id, app_id, agent_instructions_at, started_by_agent_id, seal_public_key, seal_epoch, sealed_title, via_app_id, scope_team_ids, NULL AS digest"

// session-title.json `firstPrompt`: a row a session title can read.
private const val TITLE_ROW = "(card IS NULL OR card = 'null') AND (author_kind = 'user' OR " +
    "(author_session_id IS NOT NULL AND author_session_id != '' AND author_session_id != session_id))"

// EXP-1459 (sealed-content.json): the screens read OPENED rows; Room keeps
// them as synced. Re-emits when this install gains a tree key; off the main
// thread (the first open loads the eNet library).
private fun Flow<List<MessageEntity>>.openedMessages(): Flow<List<MessageEntity>> =
    combine(this, SealedKeys.keys) { rows, keys -> rows.map { SealedContent.message(it, keys) } }.flowOn(Dispatchers.Default)

private fun Flow<List<SessionEntity>>.openedSessions(): Flow<List<SessionEntity>> =
    combine(this, SealedKeys.keys) { rows, keys -> rows.map { SealedContent.session(it, keys) } }.flowOn(Dispatchers.Default)

private fun Flow<SessionEntity?>.openedSession(): Flow<SessionEntity?> =
    combine(this, SealedKeys.keys) { row, keys -> row?.let { SealedContent.session(it, keys) } }.flowOn(Dispatchers.Default)

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

    /** #VAPP-186: every live app, the names a comment's "via <app>" reads. */
    @Query("SELECT * FROM apps WHERE deleted_at IS NULL")
    fun observeAll(): Flow<List<AppEntity>>

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

// VAPP-122: an app's records (the `records:<kind>` bindings derive over them).
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

// VAPP-235: the offline outbox (oldest first).
@Dao
interface AppRecordOutboxDao {
    @Query("SELECT * FROM app_record_outbox ORDER BY seq")
    fun observeAll(): Flow<List<AppRecordOutboxEntity>>

    @Query("SELECT * FROM app_record_outbox WHERE app_id = :appId ORDER BY seq")
    fun observeByApp(appId: String): Flow<List<AppRecordOutboxEntity>>

    @Query("SELECT * FROM app_record_outbox ORDER BY seq")
    suspend fun all(): List<AppRecordOutboxEntity>

    @Query("SELECT COALESCE(MAX(seq), 0) FROM app_record_outbox")
    suspend fun maxSeq(): Long

    @Insert(onConflict = OnConflictStrategy.ABORT)
    suspend fun insert(item: AppRecordOutboxEntity)

    @Query("UPDATE app_record_outbox SET state = 'acked', acked_at = :ackedAt WHERE write_id = :writeId")
    suspend fun ack(writeId: String, ackedAt: String)

    @Query("DELETE FROM app_record_outbox WHERE write_id IN (:writeIds)")
    suspend fun delete(writeIds: List<String>)
}

// #FEED-237: the session outbox (message-delivery.json), oldest first.
@Dao
interface OutboxDao {
    @Query("SELECT * FROM outbox WHERE session_id = :sessionId ORDER BY created_at, client_id")
    fun observeBySession(sessionId: String): Flow<List<OutboxEntity>>

    @Query("SELECT * FROM outbox WHERE client_id = :clientId LIMIT 1")
    suspend fun get(clientId: String): OutboxEntity?

    // On start: a `sending` entry the server never answered is sent again.
    @Query("SELECT * FROM outbox WHERE state = 'sending' AND message_id IS NULL ORDER BY created_at, client_id")
    suspend fun unanswered(): List<OutboxEntity>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: OutboxEntity)

    @Query("UPDATE outbox SET state = :state WHERE client_id = :clientId")
    suspend fun setState(clientId: String, state: String)

    @Query("UPDATE outbox SET message_id = :messageId WHERE client_id = :clientId")
    suspend fun setMessageId(clientId: String, messageId: String)

    // One file landed: its id joins the landed ones, it leaves the pending list.
    @Query("UPDATE outbox SET attachment_ids = :attachmentIds, pending_files = :pendingFiles WHERE client_id = :clientId")
    suspend fun setUploads(clientId: String, attachmentIds: String?, pendingFiles: String?)

    @Query("DELETE FROM outbox WHERE client_id IN (:clientIds)")
    suspend fun delete(clientIds: List<String>)
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

// #EXP-1661: the team skills (Settings › Skills, the + menu pick, the agent
// page), sorted by name (skills.json `sortSkills`; names are [a-z0-9-]).
@Dao
interface SkillDao {
    @Query("SELECT * FROM skills WHERE team_id = :teamId ORDER BY name")
    fun observeByTeam(teamId: String): Flow<List<SkillEntity>>

    @Query("SELECT * FROM skills WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<SkillEntity?>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: SkillEntity)

    @Query("DELETE FROM skills WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM skills")
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

    // A chat's PRs (`root_session_id`), opening order.
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

// #EXP-1611: the CI checks of every PR (one row per check name at its head).
@Dao
interface PullRequestCheckDao {
    @Query("SELECT * FROM pull_request_checks WHERE pull_request_id = :pullRequestId ORDER BY name, id")
    fun observeByPullRequest(pullRequestId: String): Flow<List<PullRequestCheckEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: PullRequestCheckEntity)

    @Query("DELETE FROM pull_request_checks WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM pull_request_checks")
    suspend fun clear()
}

// #EXP-1617 / #EXP-1804: our own CI's runs (the run page).
@Dao
interface CiRunDao {
    @Query("SELECT * FROM ci_runs WHERE id = :id LIMIT 1")
    fun observeById(id: String): Flow<CiRunEntity?>

    @Query("SELECT * FROM ci_runs WHERE id = :id LIMIT 1")
    suspend fun getById(id: String): CiRunEntity?

    @Query("SELECT * FROM ci_runs WHERE id IN (:ids)")
    fun observeByIds(ids: List<String>): Flow<List<CiRunEntity>>

    /** #EXP-1617 (repo-ci.json): a repository's runs (the Runs tab, the CI section). */
    @Query("SELECT * FROM ci_runs WHERE repository_id = :repositoryId ORDER BY number DESC")
    fun observeByRepository(repositoryId: String): Flow<List<CiRunEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: CiRunEntity)

    @Query("DELETE FROM ci_runs WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM ci_runs")
    suspend fun clear()
}

// #EXP-1617 / #EXP-1804: the jobs of every run (the run page, a runner's
// recent jobs).
@Dao
interface CiJobDao {
    @Query("SELECT * FROM ci_jobs WHERE run_id = :runId ORDER BY position, id")
    fun observeByRun(runId: String): Flow<List<CiJobEntity>>

    /** #EXP-1617 (repo-ci.json): the jobs of a repository's runs. */
    @Query("SELECT * FROM ci_jobs WHERE run_id IN (SELECT id FROM ci_runs WHERE repository_id = :repositoryId) ORDER BY position, id")
    fun observeByRepository(repositoryId: String): Flow<List<CiJobEntity>>

    @Query("SELECT * FROM ci_jobs WHERE runner_device_id = :deviceRowId ORDER BY started_at DESC LIMIT :limit")
    fun observeByRunner(deviceRowId: String, limit: Int): Flow<List<CiJobEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(item: CiJobEntity)

    @Query("DELETE FROM ci_jobs WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("DELETE FROM ci_jobs")
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

    // FEED-243: a team's timeline (the app host's `exp:issues?touched=`).
    @Query("SELECT * FROM issue_events WHERE team_id = :teamId ORDER BY created_at ASC, id")
    fun observeByTeam(teamId: String): Flow<List<IssueEventEntity>>

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
