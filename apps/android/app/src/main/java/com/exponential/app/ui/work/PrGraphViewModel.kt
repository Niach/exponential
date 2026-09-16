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
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.PrGraph
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
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

    val graph: StateFlow<PrGraph.Graph> = combine(
        subject,
        dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() },
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
