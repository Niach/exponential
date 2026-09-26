package com.exponential.app.ui.reviews

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.CodingSessionsApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.WorkflowsApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.WorkflowEntity
import com.exponential.app.data.db.WorkflowNodeEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.CHAT_RUN_NAME
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.PrStack
import com.exponential.app.domain.ReviewMergeInput
import com.exponential.app.domain.ReviewStackPosition
import com.exponential.app.domain.ReviewsMerge
import com.exponential.app.domain.coveredIssueIds
import com.exponential.app.domain.WorkflowFinalPr
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
 * continues the same PR) keeping the NEWEST row, newest first. Top-level and
 * pure so it can be tested without a database.
 */
fun buildRunEntries(sessions: List<CodingSessionEntity>): List<RunReviewEntry> {
    val byPrUrl = LinkedHashMap<String, CodingSessionEntity>()
    for (session in sessions) {
        val prUrl = session.prUrl
        if (prUrl.isNullOrEmpty()) continue
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
 * EXP-1072: a workflow's ONE final pull request (integration branch → the
 * default branch). The workflow row carries its url/number/state, so it is
 * the workflow's OWN PR here — never an unlinked one — and merges through
 * `workflows.mergeFinalPr`, which completes the workflow and its issues.
 */
data class WorkflowReviewEntry(
    val groupKey: String,
    val workflow: WorkflowEntity,
    val prUrl: String?,
    val prNumber: Int?,
    val branch: String?,
    /** The workflow's name — the row's title. */
    val title: String,
)

/**
 * The team's workflows → review entries: only an OPEN final pull request with
 * a url, newest workflow first. Pure so it can be tested without a database.
 */
fun buildWorkflowEntries(workflows: List<WorkflowEntity>): List<WorkflowReviewEntry> =
    workflows
        .filter { it.finalPrState == DomainContract.prStateOpen && !it.finalPrUrl.isNullOrEmpty() }
        .sortedByDescending { sortableTimestamp(it.createdAt) }
        .map { workflow ->
            WorkflowReviewEntry(
                groupKey = WorkflowFinalPr.reviewKey(workflow.id),
                workflow = workflow,
                prUrl = workflow.finalPrUrl,
                prNumber = workflow.finalPrNumber,
                branch = workflow.integrationBranch.takeIf { it.isNotBlank() },
                title = workflow.name,
            )
        }

/**
 * EXP-897: one row of the Reviews list — a pull request and where it sits in
 * its STACK. The stack edge is synced (`pr_base_branch` → the lower entry's
 * `branch`), so the nesting is pure client work like the batch collapsing
 * above it.
 */
data class ReviewRowEntry(
    val entry: ReviewEntry,
    /** 0 for the bottom of a stack (or a lone PR), +1 per level. */
    val depth: Int,
    /** Whether a stacked pull request is nested right below this row. */
    val hasChildren: Boolean,
    /** The identifier of the entry directly below — the `on top of #X` caption. */
    val stackedOn: String?,
    /** The board the ROOT of this row's stack belongs to — the group it lands in. */
    val rootBoardId: String,
    /**
     * Non-null on the BOTTOM row of a real stack: the issue id `Merge stack`
     * sends (the server resolves the chain's top from it) and how many pull
     * requests that merge would take.
     */
    val mergeStackIssueId: String?,
    val stackSize: Int,
    /**
     * EXP-1094: the status of the workflow whose node covers this PR's
     * issues (a live one first), else null. A running or paused workflow
     * merges its node PRs itself.
     */
    val workflowStatus: String? = null,
) {
    /** EXP-1094: the input of the ONE merge control this row carries. */
    val mergeInput: ReviewMergeInput
        get() = ReviewMergeInput(
            stack = when {
                mergeStackIssueId != null -> ReviewStackPosition.BOTTOM
                depth > 0 -> ReviewStackPosition.UPPER
                else -> ReviewStackPosition.NONE
            },
            workflowStatus = workflowStatus,
        )
}

/**
 * The team's review entries → rows, nested by stack: the caller's order is
 * the ROOT order, a stacked pull request follows its foundation, and every
 * row records the board of its ROOT so a whole stack groups under one board
 * even when a member was moved.
 */
fun buildReviewRows(
    entries: List<ReviewEntry>,
    /** EXP-1094: issue id → its covering workflow's status ([ReviewsMerge.workflowStatusByIssue]). */
    workflowStatusByIssue: Map<String, String> = emptyMap(),
): List<ReviewRowEntry> {
    val nested = PrStack.nestPrStacks(
        entries,
        { it.branch },
        { it.representative.prBaseBranch },
    )
    // The size of the stack each ROOT starts — the `N pull requests` count.
    val sizeOfRootAt = HashMap<Int, Int>()
    var rootIndex = -1
    nested.forEachIndexed { index, row ->
        if (row.depth == 0) rootIndex = index
        sizeOfRootAt[rootIndex] = (sizeOfRootAt[rootIndex] ?: 0) + 1
    }
    val ancestors = ArrayList<ReviewEntry>()
    var rootBoardId = ""
    rootIndex = -1
    return nested.mapIndexed { index, row ->
        while (ancestors.size > row.depth) ancestors.removeAt(ancestors.size - 1)
        val below = ancestors.lastOrNull()
        if (row.depth == 0) {
            rootIndex = index
            rootBoardId = row.entry.boardId
        }
        ancestors.add(row.entry)
        ReviewRowEntry(
            entry = row.entry,
            depth = row.depth,
            hasChildren = row.hasChildren,
            stackedOn = below?.representative?.identifier,
            rootBoardId = rootBoardId,
            mergeStackIssueId = if (row.depth == 0 && row.hasChildren) {
                row.entry.representative.id
            } else {
                null
            },
            stackSize = sizeOfRootAt[rootIndex] ?: 1,
            workflowStatus = ReviewsMerge.reviewWorkflowStatus(
                row.entry.issues.map { it.id },
                workflowStatusByIssue,
            ),
        )
    }
}

data class ReviewBoardGroup(
    val board: BoardEntity,
    val rows: List<ReviewRowEntry>,
) {
    /** The flat pull requests of this board — counts and callers that ignore nesting. */
    val entries: List<ReviewEntry> get() = rows.map { it.entry }
}

data class ReviewsState(
    val groups: List<ReviewBoardGroup> = emptyList(),
    // EXP-734: issueless runs whose OWN pull request is open — listed under
    // their own header, after the board groups.
    val runs: List<RunReviewEntry> = emptyList(),
    // EXP-1072: workflows whose FINAL pull request is open — listed under
    // their own header, between the board groups and the runs.
    val workflows: List<WorkflowReviewEntry> = emptyList(),
    val loaded: Boolean = false,
) {
    val isEmpty: Boolean get() = groups.isEmpty() && runs.isEmpty() && workflows.isEmpty()
}

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ReviewsViewModel @Inject constructor(
    holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val issuesApi: IssuesApi,
    private val codingSessionsApi: CodingSessionsApi,
    private val workflowsApi: WorkflowsApi,
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
                        db.codingSessionDao().observeOpenPrRunsByTeam(teamId),
                        db.workflowDao().observeByTeam(teamId),
                        db.workflowNodeDao().observeByTeam(teamId),
                    ) { issues, boards, runs, workflows, nodes ->
                        buildState(issues, boards, runs, workflows, nodes)
                    }
                }
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ReviewsState())

    private fun buildState(
        issues: List<IssueEntity>,
        boards: List<BoardEntity>,
        runs: List<CodingSessionEntity>,
        workflows: List<WorkflowEntity>,
        nodes: List<WorkflowNodeEntity>,
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

        // Newest entry first, then nested by stack (EXP-897): roots keep that
        // order, a stacked pull request follows its foundation. Grouped by the
        // ROOT's board so a stack never splits across two bands, and the
        // boards ordered by sortOrder (name tiebreak) — parity with
        // web/iOS/desktop, which all walk boards in board order.
        // EXP-1094: a node PR of a live workflow merges through the workflow.
        val workflowStatusByIssue = ReviewsMerge.workflowStatusByIssue(
            workflows.map { it.id to it.status },
            nodes.map { it.workflowId to it.coveredIssueIds },
        )
        val rows = buildReviewRows(
            entries.sortedByDescending { sortableTimestamp(it.representative.createdAt) },
            workflowStatusByIssue,
        )
        val groups = rows
            .groupBy { it.rootBoardId }
            .mapNotNull { (boardId, boardRows) ->
                val board = boardsById[boardId] ?: return@mapNotNull null
                ReviewBoardGroup(board = board, rows = boardRows)
            }
            .sortedWith(
                compareBy({ it.board.sortOrder }, { it.board.name.lowercase() })
            )

        return ReviewsState(
            groups = groups,
            runs = buildRunEntries(runs),
            workflows = buildWorkflowEntries(workflows),
            loaded = true,
        )
    }

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
     * EXP-1072: squash-merge a workflow's FINAL pull request via
     * `workflows.mergeFinalPr`. The server completes the workflow and moves
     * every landed issue to the team's PR-merge status; the synced row's
     * `final_pr_state` leaving `open` drops the entry. Shares the merging /
     * mergeErrors maps, keyed by [WorkflowReviewEntry.groupKey].
     */
    fun mergeWorkflow(entry: WorkflowReviewEntry) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            val key = entry.groupKey
            _mergeErrors.value = _mergeErrors.value - key
            _merging.value = _merging.value + key
            runCatching { workflowsApi.mergeFinalPr(accountId, entry.workflow.id) }
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
     */
    /**
     * EXP-897: merge the whole STACK this row starts, bottom-up. [issueId] is
     * the BOTTOM row's representative issue — the server resolves the chain's
     * top and merges every unmerged member below it, retargeting as it goes.
     * Shares the merging / mergeErrors maps with the single merge.
     */
    fun mergeStack(groupKey: String, issueId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _mergeErrors.value = _mergeErrors.value - groupKey
            _merging.value = _merging.value + groupKey
            runCatching { issuesApi.mergePr(accountId, issueId, mergeStack = true) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    _mergeErrors.value = _mergeErrors.value +
                        (groupKey to MergeFailure.from(t, "The stack could not be merged"))
                }
            _merging.value = _merging.value - groupKey
        }
    }

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
}
