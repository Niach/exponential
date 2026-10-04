package com.exponential.app.ui.reviews

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.CodingSessionsApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.CHAT_RUN_NAME
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.chatRunSubject
import com.exponential.app.domain.sortableTimestamp
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// Reviews (EXP-131): every open pull request across EVERY member team
// (EXP-1186, like the Inbox), grouped by board. A batch coding run links N issues to ONE pr_url, so the list
// collapses those rows into a single entry (never N). Pure client work over the
// already-synced issues shape — no new shape, no server round-trip to list.

/**
 * One reviewable pull request. A single-issue PR carries one issue; a batch PR
 * carries several (all sharing [prUrl]). [issues] is newest-first, so
 * [representative] is the newest issue — the one merge/navigation acts on.
 */
data class ReviewEntry(
    val groupKey: String,
    val prUrl: String?,
    val prNumber: Int?,
    val branch: String?,
    val boardId: String,
    val issues: List<IssueEntity>,
) {
    val representative: IssueEntity get() = issues.first()
    val isBatch: Boolean get() = issues.size > 1
    val identifiers: List<String> get() = issues.map { it.identifier }
}

/**
 * EXP-734: one reviewable pull request that belongs to a RUN, not an issue —
 * an action or chat run that opened a chore PR via
 * `exponential_pr_open({repositoryId, head})`. Nothing links it to a board, so
 * these list under their own header and merge through
 * `codingSessions.mergePr`.
 */
data class RunReviewEntry(
    val groupKey: String,
    val session: CodingSessionEntity,
    val prUrl: String?,
    val prNumber: Int?,
    val branch: String?,
    /** The run's own name — an action run's snapshot, or plain "Chat". */
    val title: String,
)

/**
 * The open-PR runs → review entries: collapsed by `pr_url` (a resumed run
 * continues the same PR) keeping the NEWEST row, newest first. Every run
 * stamps its own PR on its row — a batch run's combined PR too — so a run
 * whose `pr_url` an issue already carries ([issuePrUrls]) stays with that
 * issue's entry instead. Top-level and pure so it can be tested without a
 * database.
 */
fun buildRunEntries(
    sessions: List<CodingSessionEntity>,
    issuePrUrls: Set<String> = emptySet(),
): List<RunReviewEntry> {
    val byPrUrl = LinkedHashMap<String, CodingSessionEntity>()
    for (session in sessions) {
        val prUrl = session.prUrl
        if (prUrl.isNullOrEmpty() || prUrl in issuePrUrls) continue
        val current = byPrUrl[prUrl]
        if (current == null ||
            sortableTimestamp(session.startedAt) > sortableTimestamp(current.startedAt)
        ) {
            byPrUrl[prUrl] = session
        }
    }
    return byPrUrl.values
        .sortedByDescending { sortableTimestamp(it.startedAt) }
        .map { session ->
            RunReviewEntry(
                groupKey = "session:${session.id}",
                session = session,
                prUrl = session.prUrl,
                prNumber = session.prNumber,
                branch = session.branch,
                title = chatRunSubject(session) ?: session.actionName ?: CHAT_RUN_NAME,
            )
        }
}

/**
 * The open-PR issues, boards and open-PR runs → the Reviews state.
 * Issues group by `pr_url` so a batch PR (N issues, one url) is ONE entry;
 * entries are newest first and grouped by the representative issue's board.
 * EXP-1186: the list spans every member team ([teams], name order). Boards
 * order by team (its position in [teams]) then board order (sortOrder, name
 * tiebreak); with MORE than one team each board band names its team and the
 * run PRs band once per team. One team = exactly the single-team list. Pure
 * so it can be tested without a database.
 */
fun buildReviewsState(
    issues: List<IssueEntity>,
    boards: List<BoardEntity>,
    runs: List<CodingSessionEntity>,
    teams: List<TeamEntity> = emptyList(),
): ReviewsState {
    val boardsById = boards.associateBy { it.id }
    val multiTeam = teams.size > 1
    val teamsById = teams.associateBy { it.id }
    val teamOrder = teams.withIndex().associate { (index, team) -> team.id to index }
    // An issue without a url (defensive — the query only selects pr_state
    // 'open', which normally implies a url) keys on its own id so it stays a
    // distinct single-issue row.
    val entries = issues
        .filter { it.boardId in boardsById }
        .groupBy { it.prUrl ?: "issue:${it.id}" }
        .map { (groupKey, rows) ->
            val ordered = rows.sortedByDescending { sortableTimestamp(it.createdAt) }
            val representative = ordered.first()
            ReviewEntry(
                groupKey = groupKey,
                prUrl = representative.prUrl,
                prNumber = representative.prNumber,
                branch = representative.branch,
                boardId = representative.boardId,
                issues = ordered,
            )
        }
        .sortedByDescending { sortableTimestamp(it.representative.createdAt) }
    val groups = entries
        .groupBy { it.boardId }
        .mapNotNull { (boardId, boardEntries) ->
            val board = boardsById[boardId] ?: return@mapNotNull null
            ReviewBoardGroup(
                board = board,
                entries = boardEntries,
                teamName = if (multiTeam) teamsById[board.teamId]?.name else null,
            )
        }
        .sortedWith(
            compareBy(
                { teamOrder[it.board.teamId] ?: Int.MAX_VALUE },
                { it.board.sortOrder },
                { it.board.name.lowercase() },
            ),
        )
    val issuePrUrls = issues.mapNotNullTo(HashSet()) { it.prUrl?.takeIf(String::isNotEmpty) }
    val runEntries = buildRunEntries(runs, issuePrUrls)
    val runGroups = if (runEntries.isEmpty()) {
        emptyList()
    } else if (!multiTeam) {
        listOf(ReviewRunGroup(team = null, entries = runEntries))
    } else {
        runEntries
            .groupBy { it.session.teamId }
            .map { (teamId, rows) -> ReviewRunGroup(team = teamsById[teamId], entries = rows) }
            .sortedBy { group -> group.team?.let { teamOrder[it.id] } ?: Int.MAX_VALUE }
    }
    return ReviewsState(
        groups = groups,
        runs = runEntries,
        runGroups = runGroups,
        loaded = true,
    )
}

data class ReviewBoardGroup(
    val board: BoardEntity,
    /** One entry per open pull request, newest first — a FLAT list. */
    val entries: List<ReviewEntry>,
    /** EXP-1186: the board's team, named only when the user is in >1 team. */
    val teamName: String? = null,
)

/**
 * EXP-1186: one "Agent runs" band. [team] is null with a single team (one
 * band, exactly as before), else the band's team.
 */
data class ReviewRunGroup(
    val team: TeamEntity?,
    val entries: List<RunReviewEntry>,
)

data class ReviewsState(
    val groups: List<ReviewBoardGroup> = emptyList(),
    // EXP-734: issueless runs whose OWN pull request is open — listed under
    // their own header, after the board groups.
    val runs: List<RunReviewEntry> = emptyList(),
    /** EXP-1186: [runs] banded per team (one band with a single team). */
    val runGroups: List<ReviewRunGroup> = emptyList(),
    val loaded: Boolean = false,
) {
    val isEmpty: Boolean get() = groups.isEmpty() && runs.isEmpty()
}

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ReviewsViewModel @Inject constructor(
    holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val issuesApi: IssuesApi,
    private val codingSessionsApi: CodingSessionsApi,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    // EXP-1186: cross-team like the Inbox — never the selected team.
    val state: StateFlow<ReviewsState> =
        dbFlow
            .flatMapLatest { db ->
                if (db == null) {
                    flowOf(ReviewsState(loaded = true))
                } else {
                    combine(
                        db.issueDao().observeOpenPrs(),
                        db.boardDao().observeAll(),
                        db.codingSessionDao().observeOpenPrRuns(),
                        db.teamDao().observeAll(),
                    ) { issues, boards, runs, teams ->
                        buildReviewsState(issues, boards, runs, teams)
                    }
                }
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ReviewsState())

    /**
     * Squash-merge a RUN's own pull request (EXP-734). No issue is linked, so
     * nothing is completed: the server merges, flips the session row's
     * `pr_state` and (unless the team keeps sessions on merge) ends the run —
     * all of it arriving through Electric, which drops the entry off this
     * list. Shares the merging / mergeErrors maps, keyed by [RunReviewEntry.groupKey].
     */
    fun mergeRun(entry: RunReviewEntry) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            val key = entry.groupKey
            _mergeErrors.value = _mergeErrors.value - key
            _merging.value = _merging.value + key
            runCatching { codingSessionsApi.mergePr(accountId, entry.session.id) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    _mergeErrors.value = _mergeErrors.value +
                        (key to MergeFailure.from(t, "The pull request could not be merged"))
                }
            _merging.value = _merging.value - key
        }
    }

    /**
     * Squash-merge a review's PR via the GitHub App (EXP-131). Pass the
     * entry's [groupKey] plus the representative issue id — for a batch PR the
     * server resolves it to ALL linked issues and completes them together; the
     * `done` flips arrive via Electric sync, dropping the entry off this list.
     * EXP-1145: [mergeStack] merges the open stack bottom-up THROUGH [issueId];
     * a failure captions the row with the server's message and never offers
     * Fix conflicts: the conflicting pull request may be another member.
     */
    fun mergePr(groupKey: String, issueId: String, mergeStack: Boolean = false) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _mergeErrors.value = _mergeErrors.value - groupKey
            _merging.value = _merging.value + groupKey
            runCatching { issuesApi.mergePr(accountId, issueId, mergeStack = mergeStack) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    // Conflicts, branch protection and GitHub App errors are the
                    // COMMON, persistent failures of a squash merge — a silent
                    // drop left the row sitting there unexplained (REV2-50).
                    // Same copy as the issue Changes tab's merge.
                    val failure = if (mergeStack) {
                        MergeFailure.fromStack(t)
                    } else {
                        MergeFailure.from(t, "The pull request could not be merged")
                    }
                    _mergeErrors.value = _mergeErrors.value + (groupKey to failure)
                }
            _merging.value = _merging.value - groupKey
        }
    }

    // Rendered INLINE on the failing row, keyed by its groupKey (EXP-323 — a
    // Scaffold snackbar landed behind the floating bottom nav pill, which is
    // drawn over the whole NavHost, so the reason a merge failed was
    // unreadable). Cleared by the next attempt on that row.
    /** SLOP-16 r3: every synced status, resolved — the batch sheet's relation
     *  rows resolve their glyph by `status_id` against it. */
    val issueStatuses: StateFlow<List<ResolvedIssueStatus>> =
        dbFlow.scopedQuery(emptyList<IssueStatusEntity>()) { it.issueStatusDao().observeAll() }
            .map { IssueStatusResolver.teamStatuses(it) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * EXP-1145: open pull requests PER TEAM, so a row's Merge on a PR-stack
     * member asks first ([com.exponential.app.domain.PrStack.stackMergeChoice])
     * against ITS OWN team's PRs (EXP-1186: the row's team, not the
     * selection). Never the whole account: branch names repeat across teams.
     */
    val openPrIssuesByTeam: StateFlow<Map<String, List<IssueEntity>>> =
        dbFlow
            .flatMapLatest { db ->
                if (db == null) flowOf(emptyMap())
                else combine(
                    db.issueDao().observeOpenPrs(),
                    db.boardDao().observeAll(),
                ) { issues, boards ->
                    val teamByBoard = boards.associate { it.id to it.teamId }
                    issues.groupBy { teamByBoard[it.boardId].orEmpty() }
                }
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    /** SLOP-16 r3: the batch sheet's assignee avatars. */
    val users: StateFlow<List<UserEntity>> =
        dbFlow.scopedQuery(emptyList<UserEntity>()) { it.userDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val _mergeErrors = MutableStateFlow<Map<String, MergeFailure>>(emptyMap())
    val mergeErrors: StateFlow<Map<String, MergeFailure>> = _mergeErrors

    private val _merging = MutableStateFlow<Set<String>>(emptySet())
    val merging: StateFlow<Set<String>> = _merging
}
