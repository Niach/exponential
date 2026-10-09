package com.exponential.app.ui.myissues

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.UpdateIssueInput
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueLabelEntity
import com.exponential.app.data.db.IssueRelationEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.issueStatusCategoryDisplayOrder
import com.exponential.app.domain.sortIssuesForCategory
import com.exponential.app.ui.issue.IssueWithLabels
import com.exponential.app.ui.issue.nestListRows
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

// P14: groups are the issues' RESOLVED team status rows (statusId, then the
// anchor — IssueStatusResolver), named and drawn like the board lists, so a
// custom "In QA" gets its own group and the started clocks follow the team's
// own count. My Issues spans TEAMS, so rows that read the same (category +
// name, e.g. every team's builtin "In Progress") share ONE group rather than
// splitting per team; the first issue's row lends the group its glyph.
data class MyIssuesGroup(
    val key: String,
    val status: ResolvedIssueStatus,
    val issues: List<IssueWithLabels>,
)

/** The group a resolved row lands in: same category + same name = one group. */
internal fun myIssuesGroupKey(status: ResolvedIssueStatus): String =
    "${status.category.wire}|${status.name}"

/**
 * Group my issues by their resolved team status rows. [resolved] pairs each
 * issue with its row and that row's index in its team's ordered list; groups
 * order by category (contract display order), then that index.
 */
internal fun <T> groupByResolvedStatus(
    resolved: List<Triple<T, ResolvedIssueStatus, Int>>,
): List<Pair<ResolvedIssueStatus, List<T>>> =
    resolved
        .groupBy { myIssuesGroupKey(it.second) }
        .values
        .sortedWith(
            compareBy<List<Triple<T, ResolvedIssueStatus, Int>>> { group ->
                issueStatusCategoryDisplayOrder.indexOf(group.first().second.category)
                    .takeIf { it >= 0 } ?: issueStatusCategoryDisplayOrder.size
            }.thenBy { group -> group.minOf { it.third } }
                .thenBy { group -> group.first().second.name },
        )
        .map { group -> group.first().second to group.map { it.first } }

data class MyIssuesState(
    val groups: List<MyIssuesGroup> = emptyList(),
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

    /**
     * EXP-980: every synced issue and relation — the nesting rows, the blocks
     * badges and the mini-graph the badge opens. A parent or a blocker that is
     * NOT assigned to me still has to resolve, so neither may be scoped to the
     * assignee.
     */
    val allIssues: StateFlow<List<IssueEntity>> =
        dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val relations: StateFlow<List<IssueRelationEntity>> =
        dbFlow.scopedQuery(emptyList<IssueRelationEntity>()) { it.issueRelationDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val state: StateFlow<MyIssuesState> =
        combine(dbFlow, auth.userId) { db, userId -> db to userId }
            .flatMapLatest { (db, userId) ->
                if (db == null || userId == null) {
                    flowOf(MyIssuesState(loaded = true))
                } else {
                    combine(
                        db.issueDao().observeByAssignee(userId),
                        db.boardDao().observeAll(),
                        combine(db.labelDao().observeAll(), db.issueLabelDao().observeAllJoins()) { l, j -> l to j },
                        db.issueStatusDao().observeAll(),
                        combine(relations, allIssues) { rows, synced -> rows to synced },
                    ) { issues, boards, labelJoins, statuses, graph ->
                        buildState(
                            issues, boards, labelJoins.first, labelJoins.second,
                            statuses, graph.first, graph.second,
                        )
                    }
                }
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), MyIssuesState())

    private fun buildState(
        issues: List<IssueEntity>,
        boards: List<BoardEntity>,
        labels: List<LabelEntity>,
        joins: List<IssueLabelEntity>,
        statusRows: List<IssueStatusEntity>,
        relations: List<IssueRelationEntity>,
        syncedIssues: List<IssueEntity>,
    ): MyIssuesState {
        val boardsById = boards.associateBy { it.id }
        val labelsById = labels.associateBy { it.id }
        val joinsByIssue = joins.groupBy { it.issueId }

        // Only issues in live (non-trashed) boards; the DAO already
        // scoped to assignee = me.
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
        val statusesByTeam = statusRows
            .groupBy { it.teamId }
            .mapValues { (_, rows) -> IssueStatusResolver.teamStatuses(rows) }
        val resolved = decorated.map { entry ->
            val team = boardsById[entry.issue.boardId]?.teamId?.let { statusesByTeam[it] }.orEmpty()
            val status = IssueStatusResolver.resolve(entry.issue, team)
            val index = team.indexOfFirst { it.id == status.id }.takeIf { it >= 0 } ?: Int.MAX_VALUE
            Triple(entry, status, index)
        }
        val groups = groupByResolvedStatus(resolved).map { (status, entries) ->
            MyIssuesGroup(
                key = myIssuesGroupKey(status),
                status = status,
                issues = sortIssuesForCategory(category = status.category, issues = entries) { it.issue },
            )
        }

        // EXP-980: the same nesting + blocks badges the board list runs, over
        // ALL groups at once — the root decides the group here too.
        val nested = nestListRows(groups.map { it.issues }, relations, syncedIssues)
        val nestedGroups = groups.mapIndexedNotNull { index, group ->
            nested[index].takeIf { it.isNotEmpty() }?.let { group.copy(issues = it) }
        }

        return MyIssuesState(groups = nestedGroups, boardsById = boardsById, loaded = true)
    }

    /**
     * The long-press quick actions (Mark done / Move to backlog) stay ENUM
     * writes (EXP-314): the server trigger derives status_id, so they land on
     * the team's builtin Done / Backlog row. An issue sitting in a CUSTOM
     * status therefore leaves it — the intended, explicit meaning of both
     * actions.
     */
    fun updateIssueStatus(issueId: String, status: IssueStatus) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                issuesApi.update(accountId, UpdateIssueInput(id = issueId, status = status.wire))
            }
        }
    }
}
