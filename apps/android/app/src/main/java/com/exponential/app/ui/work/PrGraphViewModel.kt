package com.exponential.app.ui.work

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.PrGraph
import com.exponential.app.domain.PrStack
import com.exponential.app.domain.ResolvedIssueStatus
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

/**
 * EXP-897 part 4 / SLOP-3: what the Work screen's related-work badge and its
 * sheet read, ONE [PrGraph] built from synced rows alone (the blockers off
 * the `blocks` relations, the batch off a shared `pr_url`, the stack off
 * `pr_base_branch`).
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

    /** The screen's current subject: an issue, the shown run, or both. */
    fun bind(issueId: String?, sessionId: String?) {
        val next = Subject(issueId, sessionId)
        if (subject.value != next) subject.value = next
    }

    val graph: StateFlow<PrGraph.Graph> = combine(
        subject,
        dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() },
        dbFlow.scopedQuery(emptyList<CodingSessionEntity>()) { it.codingSessionDao().observeAll() },
        dbFlow.scopedQuery(emptyList<IssueRelationEntity>()) { it.issueRelationDao().observeAll() },
        dbFlow.scopedQuery(emptyList<BoardEntity>()) { it.boardDao().observeAll() },
    ) { current, issues, sessions, relations, boards ->
        val issue = current.issueId?.let { id -> issues.firstOrNull { it.id == id } }
        val session = current.sessionId?.let { id -> sessions.firstOrNull { it.id == id } }
        PrGraph.build(
            issue = issue,
            session = session,
            // The subject's TEAM only: branch names repeat across teams.
            issues = if (issue != null) {
                PrStack.teamIssues(issue, issues, boards)
            } else {
                PrStack.teamIssues(session?.teamId, issues, boards)
            },
            relations = relations,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), PrGraph.Graph(emptyList(), null))

    /**
     * EXP-1248/1251: the subject's TEAM issues — the pool the Guide's Stack
     * card ([PrStack.stackView]) and the ONE stack merge confirm
     * ([PrStack.stackMergeConfirm]) read. Branch names repeat across teams.
     */
    val teamIssues: StateFlow<List<IssueEntity>> = combine(
        subject,
        dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() },
        dbFlow.scopedQuery(emptyList<CodingSessionEntity>()) { it.codingSessionDao().observeAll() },
        dbFlow.scopedQuery(emptyList<BoardEntity>()) { it.boardDao().observeAll() },
    ) { current, issues, sessions, boards ->
        val issue = current.issueId?.let { id -> issues.firstOrNull { it.id == id } }
        if (issue != null) {
            PrStack.teamIssues(issue, issues, boards)
        } else {
            val session = current.sessionId?.let { id -> sessions.firstOrNull { it.id == id } }
            PrStack.teamIssues(session?.teamId, issues, boards)
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** SLOP-16 r3: every synced status, resolved; the sheet's relation rows
     *  resolve their glyph by `status_id` against it. */
    val issueStatuses: StateFlow<List<ResolvedIssueStatus>> =
        dbFlow.scopedQuery(emptyList<IssueStatusEntity>()) { it.issueStatusDao().observeAll() }
            .map { IssueStatusResolver.teamStatuses(it) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** SLOP-16 r3: the relation rows' assignee avatars. */
    val users: StateFlow<List<UserEntity>> =
        dbFlow.scopedQuery(emptyList<UserEntity>()) { it.userDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())
}
