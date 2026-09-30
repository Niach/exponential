package com.exponential.app.ui.work

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.IssueRelationEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.PrGraph
import com.exponential.app.domain.batchRunIssues
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.flow.stateIn

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
    auth: AuthRepository,
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

    /** SLOP-16 r3: every synced status, resolved — the "Related work"
     *  view's relation rows resolve their glyph by `status_id` against it. */
    val issueStatuses: StateFlow<List<ResolvedIssueStatus>> =
        dbFlow.scopedQuery(emptyList<IssueStatusEntity>()) { it.issueStatusDao().observeAll() }
            .map { IssueStatusResolver.teamStatuses(it) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** SLOP-16 r3: the relation rows' assignee avatars. */
    val users: StateFlow<List<UserEntity>> =
        dbFlow.scopedQuery(emptyList<UserEntity>()) { it.userDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

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
}
