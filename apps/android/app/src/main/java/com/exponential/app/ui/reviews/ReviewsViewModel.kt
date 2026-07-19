package com.exponential.app.ui.reviews

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.sortableTimestamp
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
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
    selection: TeamSelection,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

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
     * representative issue id — for a batch PR the server resolves it to ALL
     * linked issues and completes them together; the `done` flips arrive via
     * Electric sync, dropping the entry off this list.
     */
    fun mergePr(issueId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { issuesApi.mergePr(accountId, issueId) }
        }
    }
}
