package com.exponential.app.ui.myissues

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.UpdateIssueInput
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueLabelEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.issueStatusOrder
import com.exponential.app.domain.sortIssuesForGroup
import com.exponential.app.ui.issue.IssueGroup
import com.exponential.app.ui.issue.IssueWithLabels
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

// "My Issues" (masterplan §5a): a fixed, built-in cross-board view of
// everything assigned to the signed-in user on the active account, grouped by
// status like the board board. No new column, no new shape, no filter
// machinery — pure client work over the already-synced issues shape.

data class MyIssuesState(
    val groups: List<IssueGroup> = emptyList(),
    val boardsById: Map<String, BoardEntity> = emptyMap(),
    val loaded: Boolean = false,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class MyIssuesViewModel @Inject constructor(
    holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val issuesApi: IssuesApi,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val state: StateFlow<MyIssuesState> =
        combine(dbFlow, auth.userId) { db, userId -> db to userId }
            .flatMapLatest { (db, userId) ->
                if (db == null || userId == null) {
                    flowOf(MyIssuesState(loaded = true))
                } else {
                    combine(
                        db.issueDao().observeByAssignee(userId),
                        db.boardDao().observeAll(),
                        db.labelDao().observeAll(),
                        db.issueLabelDao().observeAllJoins(),
                    ) { issues, boards, labels, joins ->
                        buildState(issues, boards, labels, joins)
                    }
                }
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), MyIssuesState())

    private fun buildState(
        issues: List<IssueEntity>,
        boards: List<BoardEntity>,
        labels: List<LabelEntity>,
        joins: List<IssueLabelEntity>,
    ): MyIssuesState {
        val boardsById = boards.associateBy { it.id }
        val labelsById = labels.associateBy { it.id }
        val joinsByIssue = joins.groupBy { it.issueId }

        // Only issues in live (non-archived) boards; the DAO already
        // filtered archived issues and scoped to assignee = me.
        val decorated = issues
            .filter { it.boardId in boardsById }
            .map { issue ->
                IssueWithLabels(
                    issue = issue,
                    labels = joinsByIssue[issue.id]
                        ?.mapNotNull { labelsById[it.labelId] }
                        ?: emptyList(),
                )
            }

        // Canonical in-group order (EXP-38) — shared with the board board and
        // the other clients; see sortIssuesForGroup in domain/IssueDomain.kt.
        val groups = issueStatusOrder.map { status ->
            IssueGroup(
                status = status,
                issues = sortIssuesForGroup(
                    status = status,
                    issues = decorated.filter { IssueStatus.fromWire(it.issue.status) == status },
                ) { it.issue },
            )
        }.filter { it.issues.isNotEmpty() }

        return MyIssuesState(groups = groups, boardsById = boardsById, loaded = true)
    }

    fun updateIssueStatus(issueId: String, status: IssueStatus) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, status = status.wire))
            }
        }
    }
}
