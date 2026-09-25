package com.exponential.app.ui.work

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.domain.IssueGraph
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.PrGraph
import com.exponential.app.domain.batchRunIssues
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * EXP-897 part 4: what the Work screen's stack/batch badge and its overlay
 * read — ONE [PrGraph] built from synced rows alone (the stack off
 * `pr_base_branch`, the batch off a shared `pr_url`, the run family off
 * `parent_session_id`, the blockers off the `blocks` relations).
 *
 * Not assisted: the Work screen shows one subject at a time and rebinds
 * ([bind]) when the reader switches the run it is watching.
 */
@HiltViewModel
class PrGraphViewModel @Inject constructor(
    holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val issuesApi: IssuesApi,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    private data class Subject(val issueId: String?, val sessionId: String?)

    private val subject = MutableStateFlow(Subject(null, null))

    /** The screen's current subject — an issue, the shown run, or both. */
    fun bind(issueId: String?, sessionId: String?) {
        val next = Subject(issueId, sessionId)
        if (subject.value != next) subject.value = next
    }

    /** The ONE full-table issue observation every derived flow below reads —
     *  three of them used to open three. */
    private val allIssues = dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() }
        .shareIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), replay = 1)

    val graph: StateFlow<PrGraph.Graph> = combine(
        subject,
        allIssues,
        dbFlow.scopedQuery(emptyList<CodingSessionEntity>()) { it.codingSessionDao().observeAll() },
        dbFlow.scopedQuery(emptyList<IssueRelationEntity>()) { it.issueRelationDao().observeAll() },
    ) { current, issues, sessions, relations ->
        PrGraph.build(
            issue = current.issueId?.let { id -> issues.firstOrNull { it.id == id } },
            session = current.sessionId?.let { id -> sessions.firstOrNull { it.id == id } },
            issues = issues,
            sessions = sessions,
            relations = relations,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), PrGraph.Graph(emptyList(), null, emptyList()))

    /**
     * EXP-1058: the header chip's front issue ([PrGraph.Graph.lead]) resolved
     * against ITS team's statuses (its board's team) — the glyph the stacked
     * chip leads with. Null when there is no front issue.
     */
    val leadStatus: StateFlow<ResolvedIssueStatus?> = combine(
        graph,
        dbFlow.scopedQuery(emptyList<BoardEntity>()) { it.boardDao().observeAll() },
        dbFlow.scopedQuery(emptyList<IssueStatusEntity>()) { it.issueStatusDao().observeAll() },
    ) { current, boards, statuses ->
        val lead = current.lead ?: return@combine null
        val teamId = boards.firstOrNull { it.id == lead.boardId }?.teamId
        val team = statuses.filter { it.teamId == teamId }
        IssueStatusResolver.resolve(lead, IssueStatusResolver.teamStatuses(team))
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * EXP-980: the subject's BLOCKS graph — the transitive chain the overlay's
     * Issue face draws where it used to list a flat row of blocker chips. The
     * same rule and the same view the list badges and the blocked-start dialog
     * use; empty whenever the subject is an issue-less run.
     */
    val blocksGraph: StateFlow<IssueGraph.Graph> = combine(
        subject,
        allIssues,
        dbFlow.scopedQuery(emptyList<IssueRelationEntity>()) { it.issueRelationDao().observeAll() },
    ) { current, issues, relations ->
        val id = current.issueId
        if (id == null) EMPTY_GRAPH else IssueGraph.blockGraph(listOf(id), relations, issues)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), EMPTY_GRAPH)

    /** The pool the graph's nodes resolve against. */
    val issuesById: StateFlow<Map<String, IssueEntity>> = allIssues
        .map { issues -> issues.associateBy { it.id } }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    /**
     * EXP-876: the issues the SHOWN run covers when it is a BATCH — what names
     * an issue-less run in the Work screen's header, where "Batch run" told
     * two batches apart no better than it did in the list. Empty for every
     * other subject. Rides this model because it already observes every synced
     * issue for the graph.
     */
    val batchIssues: StateFlow<List<IssueEntity>> = combine(
        subject,
        allIssues,
        dbFlow.scopedQuery(emptyList<CodingSessionEntity>()) { it.codingSessionDao().observeAll() },
    ) { current, issues, sessions ->
        val session = current.sessionId?.let { id -> sessions.firstOrNull { it.id == id } }
        session?.let { batchRunIssues(it, issues) } ?: emptyList()
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val _merging = MutableStateFlow(false)
    val merging: StateFlow<Boolean> = _merging

    private val _mergeError = MutableStateFlow<MergeFailure?>(null)
    val mergeError: StateFlow<MergeFailure?> = _mergeError

    /**
     * Merge the whole stack from its BOTTOM entry (the server resolves the
     * top). The flips arrive through Electric, which is what closes the
     * overlay's rows out.
     */
    fun mergeStack(issueId: String) {
        if (_merging.value) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _mergeError.value = null
            _merging.value = true
            runCatching { issuesApi.mergePr(accountId, issueId, mergeStack = true) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    _mergeError.value = MergeFailure.from(t, "The stack could not be merged")
                }
            _merging.value = false
        }
    }
}

private val EMPTY_GRAPH = IssueGraph.Graph(
    nodes = emptyList(),
    edges = emptyList(),
    hasCycle = false,
    truncated = false,
)
