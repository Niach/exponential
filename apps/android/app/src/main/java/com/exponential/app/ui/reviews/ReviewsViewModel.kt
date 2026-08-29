package com.exponential.app.ui.reviews

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.sortableTimestamp
import com.exponential.app.ui.issue.StartIssueOption
import com.exponential.app.ui.steer.ActionRunState
import com.exponential.app.ui.steer.SteerLaunchDelegate
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
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// Reviews (EXP-131): every open pull request in the CURRENT team, grouped
// by board. A batch coding run links N issues to ONE pr_url, so the list
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

data class ReviewBoardGroup(
    val board: BoardEntity,
    val entries: List<ReviewEntry>,
)

data class ReviewsState(
    val groups: List<ReviewBoardGroup> = emptyList(),
    val loaded: Boolean = false,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ReviewsViewModel @Inject constructor(
    holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val issuesApi: IssuesApi,
    private val steerLaunch: SteerLaunchDelegate,
    selection: TeamSelection,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    init {
        steerLaunch.attach(viewModelScope)
    }

    val state: StateFlow<ReviewsState> =
        combine(dbFlow, selection.selectedId) { db, teamId -> db to teamId }
            .flatMapLatest { (db, teamId) ->
                if (db == null || teamId == null) {
                    flowOf(ReviewsState(loaded = true))
                } else {
                    combine(
                        db.issueDao().observeOpenPrsByTeam(teamId),
                        db.boardDao().observeByTeam(teamId),
                    ) { issues, boards ->
                        buildState(issues, boards)
                    }
                }
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ReviewsState())

    private fun buildState(
        issues: List<IssueEntity>,
        boards: List<BoardEntity>,
    ): ReviewsState {
        val boardsById = boards.associateBy { it.id }

        // Group by pr_url so a batch PR (N issues, one url) becomes ONE entry;
        // an issue without a url (defensive — the query only selects pr_state
        // 'open', which normally implies a url) keys on its own id so it stays
        // a distinct single-issue row.
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

        // Group entries by board, newest entry first within each board, and
        // order the boards by their sortOrder (name tiebreak) — parity with
        // web/iOS/desktop, which all walk boards in board order.
        val groups = entries
            .groupBy { it.boardId }
            .mapNotNull { (boardId, boardEntries) ->
                val board = boardsById[boardId] ?: return@mapNotNull null
                ReviewBoardGroup(
                    board = board,
                    entries = boardEntries.sortedByDescending {
                        sortableTimestamp(it.representative.createdAt)
                    },
                )
            }
            .sortedWith(
                compareBy({ it.board.sortOrder }, { it.board.name.lowercase() })
            )

        return ReviewsState(groups = groups, loaded = true)
    }

    /**
     * Squash-merge a review's PR via the GitHub App (EXP-131). Pass the
     * entry's [groupKey] plus the representative issue id — for a batch PR the
     * server resolves it to ALL linked issues and completes them together; the
     * `done` flips arrive via Electric sync, dropping the entry off this list.
     */
    fun mergePr(groupKey: String, issueId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _mergeErrors.value = _mergeErrors.value - groupKey
            _merging.value = _merging.value + groupKey
            runCatching { issuesApi.mergePr(accountId, issueId) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    // Conflicts, branch protection and GitHub App errors are the
                    // COMMON, persistent failures of a squash merge — a silent
                    // drop left the row sitting there unexplained (REV2-50).
                    // Same copy as the issue Changes tab's merge.
                    _mergeErrors.value = _mergeErrors.value +
                        (groupKey to MergeFailure.from(t, "The pull request could not be merged"))
                }
            _merging.value = _merging.value - groupKey
        }
    }

    // Rendered INLINE on the failing row, keyed by its groupKey (EXP-323 — a
    // Scaffold snackbar landed behind the floating bottom nav pill, which is
    // drawn over the whole NavHost, so the reason a merge failed was
    // unreadable). Cleared by the next attempt on that row.
    private val _mergeErrors = MutableStateFlow<Map<String, MergeFailure>>(emptyMap())
    val mergeErrors: StateFlow<Map<String, MergeFailure>> = _mergeErrors

    private val _merging = MutableStateFlow<Set<String>>(emptySet())
    val merging: StateFlow<Set<String>> = _merging

    // ── Remote start (EXP-323) ───────────────────────────────────────────────
    // A merge refused for a REAL conflict (EXP-533) offers the builtin "Fix
    // merge conflicts" run — desktop parity (its Reviews list has the same
    // button). The launcher plumbing is the shared delegate's.
    val steerDevices: StateFlow<List<SteerDevice>?> get() = steerLaunch.devices
    val startCandidates: StateFlow<List<StartIssueOption>> get() = steerLaunch.startCandidates
    val runState: StateFlow<ActionRunState> get() = steerLaunch.runState
    val startedSessionId: StateFlow<String?> get() = steerLaunch.startedSessionId

    fun consumeStartedSession() = steerLaunch.consumeStartedSession()
    fun runAction(
        device: SteerDevice,
        action: ActionDto,
        options: SteerStartOptions,
        inputs: Map<String, String>,
    ) = steerLaunch.runAction(device, action, options, inputs)
    fun startCoding(device: SteerDevice, issueIds: List<String>, options: SteerStartOptions) =
        steerLaunch.startCoding(device, issueIds, options)
}
