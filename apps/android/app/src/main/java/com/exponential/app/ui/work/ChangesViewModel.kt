package com.exponential.app.ui.work

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.CodingSessionsApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.PrDescription
import com.exponential.app.data.api.PrFilesApi
import com.exponential.app.data.api.PullFile
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.PrStack
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// EXP-893/EXP-1154: the issue's PR (or pushed branch) behind the Work screen —
// the Changes face's files (issues.prFiles, else repositories.branchDiff), the
// Merge / Close PR actions and the Results face's PR-body fallback
// (issues.prDescription). The standalone review page it used to back is gone:
// a review IS the issue's Work screen on its Changes face.
// EXP-1194: a RUN's own issue-less PR (Reviews → Agent runs) loads through the
// same model off `codingSessions.prFiles` — no branch-diff fallback, no
// PR-body fallback, no Close PR, no stack (none of them exist for a run).

/** What the Changes model reads — an issue's PR, or a run's own PR. */
sealed interface ChangesSource {
    data class Issue(val id: String) : ChangesSource
    data class Session(val id: String) : ChangesSource
}

sealed interface ChangesLoadState {
    data object Loading : ChangesLoadState
    data class Failed(val message: String) : ChangesLoadState
    data class Loaded(val files: List<PullFile>) : ChangesLoadState
}

/** EXP-1154: the Results face's fallback — the open PR's GitHub title + body. */
sealed interface PrDescriptionState {
    data object Loading : PrDescriptionState
    data class Failed(val message: String) : PrDescriptionState
    data class Loaded(val description: PrDescription) : PrDescriptionState
}

@HiltViewModel(assistedFactory = ChangesViewModel.Factory::class)
class ChangesViewModel @AssistedInject constructor(
    /** EXP-893: assisted — the Work screen keys one per issue; EXP-1194 one per run. */
    @Assisted val source: ChangesSource,
    holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val prFilesApi: PrFilesApi,
    private val repositoriesApi: RepositoriesApi,
    private val issuesApi: IssuesApi,
    private val codingSessionsApi: CodingSessionsApi,
) : ViewModel() {

    @AssistedFactory
    interface Factory {
        fun create(source: ChangesSource): ChangesViewModel
    }

    /** The issue subject's id; null for a run's own PR. */
    val issueId: String? = (source as? ChangesSource.Issue)?.id

    /** EXP-1194: the run subject's id; null for an issue. */
    val sessionId: String? = (source as? ChangesSource.Session)?.id

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val issue: StateFlow<IssueEntity?> =
        (issueId?.let { id -> dbFlow.scopedQuery<IssueEntity?>(null) { it.issueDao().observeById(id) } }
            ?: flowOf(null))
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** EXP-1194: the run whose own PR this reviews (title, PR url + state). */
    val session: StateFlow<CodingSessionEntity?> =
        (sessionId?.let { id -> dbFlow.scopedQuery<CodingSessionEntity?>(null) { it.codingSessionDao().observeById(id) } }
            ?: flowOf(null))
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // EXP-1145: the synced issues and boards, so a plain Merge on a stack
    // member can ask first (Merge stack / Merge this pull request / Cancel).
    // The stack is read from this issue's TEAM only ([PrStack.teamIssues]).
    private val allIssues: Flow<List<IssueEntity>> =
        dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() }
    private val allBoards: Flow<List<BoardEntity>> =
        dbFlow.scopedQuery(emptyList<BoardEntity>()) { it.boardDao().observeAll() }

    /** EXP-1145: non-null when merging this issue's PR must ask first. */
    val stackMergeChoice: StateFlow<PrStack.StackMergeChoice?> =
        combine(issue, allIssues, allBoards) { iss, all, boards ->
            iss?.let { PrStack.stackMergeChoice(it, PrStack.teamIssues(it, all, boards)) }
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * EXP-1154: how many OTHER synced issues share this issue's PR (a batch
     * PR) — the Close PR confirm's batch line names them.
     */
    val linkedIssueCount: StateFlow<Int> =
        combine(issue, allIssues) { iss, all ->
            val url = iss?.prUrl?.takeIf { it.isNotBlank() } ?: return@combine 0
            all.count { it.id != iss.id && it.prUrl == url }
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), 0)

    private val _load = MutableStateFlow<ChangesLoadState>(ChangesLoadState.Loading)
    val load: StateFlow<ChangesLoadState> = _load

    private val _prDescription = MutableStateFlow<PrDescriptionState?>(null)
    /** EXP-1154: null until [loadDescription] first runs. */
    val prDescription: StateFlow<PrDescriptionState?> = _prDescription

    // PR review actions (EXP-156). No local writes: the Electric echo flips
    // prState and the controls vanish.
    private val _merging = MutableStateFlow(false)
    val merging: StateFlow<Boolean> = _merging
    private val _closing = MutableStateFlow(false)
    val closing: StateFlow<Boolean> = _closing
    private val _actionError = MutableStateFlow<String?>(null)
    val actionError: StateFlow<String?> = _actionError

    /** Which action produced [actionError] — merge and close share the caption. */
    enum class PrAction { Merge, Close }

    // The "Fix conflicts" run rebases, force-pushes and then MERGES the PR, so
    // it may only be offered after a failed MERGE: a user who asked to CLOSE a
    // pull request must never be handed a button that merges it.
    private val _actionErrorFrom = MutableStateFlow<PrAction?>(null)
    val actionErrorFrom: StateFlow<PrAction?> = _actionErrorFrom

    // EXP-533: and only for a REAL conflict — a merge refused by branch
    // protection, a stale base or a dead connection is not something a rebase
    // run can resolve, so the button must stay away.
    private val _actionErrorIsConflict = MutableStateFlow(false)
    val actionErrorIsConflict: StateFlow<Boolean> = _actionErrorIsConflict

    init {
        // Re-fetch when the diff source flips (a PR opens on a watched branch).
        viewModelScope.launch {
            val prMissing: Flow<Boolean> = if (sessionId != null) {
                session.filterNotNull().map { it.prUrl.isNullOrBlank() }
            } else {
                issue.filterNotNull().map { it.prUrl.isNullOrBlank() }
            }
            prMissing.distinctUntilChanged().collectLatest { refresh() }
        }
    }

    fun refresh() {
        viewModelScope.launch {
            _load.value = ChangesLoadState.Loading
            try {
                val accountId = auth.activeAccountId.value
                    ?: throw IllegalStateException("No active account")
                val files = when (source) {
                    // EXP-1194: the run's own PR only — no branch fallback.
                    is ChangesSource.Session -> prFilesApi.forSession(accountId, source.id).files
                    is ChangesSource.Issue -> {
                        val hasPr = !issue.value?.prUrl.isNullOrBlank()
                        if (hasPr) {
                            prFilesApi.get(accountId, source.id).files
                        } else {
                            repositoriesApi.branchDiff(accountId, source.id)?.files ?: emptyList()
                        }
                    }
                }
                _load.value = ChangesLoadState.Loaded(files)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _load.value = ChangesLoadState.Failed(trpcErrorMessage(t, "Failed to load changes"))
            }
        }
    }

    /**
     * EXP-1154: fetch the open PR's GitHub title + body once (the Results
     * face's fallback when no run filed a report). A failure stays until a
     * later call retries it.
     */
    fun loadDescription() {
        val issueId = issueId ?: return
        val current = _prDescription.value
        if (current is PrDescriptionState.Loading || current is PrDescriptionState.Loaded) return
        viewModelScope.launch {
            _prDescription.value = PrDescriptionState.Loading
            try {
                val accountId = auth.activeAccountId.value
                    ?: throw IllegalStateException("No active account")
                _prDescription.value = PrDescriptionState.Loaded(prFilesApi.description(accountId, issueId))
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _prDescription.value = PrDescriptionState.Failed(
                    trpcErrorMessage(t, "Failed to load the pull request"),
                )
            }
        }
    }

    /**
     * Squash-merge the issue's open PR via the GitHub App (batch PRs complete
     * all linked issues); EXP-1194: a run's own PR via `codingSessions.mergePr`.
     */
    fun mergePr() {
        when (source) {
            is ChangesSource.Issue -> merge(source.id, mergeStack = false)
            is ChangesSource.Session -> mergeSession(source.id)
        }
    }

    /** EXP-734/EXP-1194: a run's own PR — nothing is completed. */
    private fun mergeSession(sessionId: String) {
        if (_merging.value || _closing.value) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _merging.value = true
            _actionError.value = null
            _actionErrorFrom.value = null
            _actionErrorIsConflict.value = false
            runCatching { codingSessionsApi.mergePr(accountId, sessionId) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    val failure = MergeFailure.from(t, "The pull request could not be merged")
                    _actionError.value = failure.message
                    _actionErrorFrom.value = PrAction.Merge
                    _actionErrorIsConflict.value = failure.isConflict
                }
            _merging.value = false
        }
    }

    /**
     * EXP-1145: merge the open stack bottom-up THROUGH [targetIssueId]
     * (the top member = the whole stack, this issue = it and those below).
     */
    fun mergeStack(targetIssueId: String) = merge(targetIssueId, mergeStack = true)

    private fun merge(targetIssueId: String, mergeStack: Boolean) {
        if (_merging.value || _closing.value) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _merging.value = true
            _actionError.value = null
            _actionErrorFrom.value = null
            _actionErrorIsConflict.value = false
            runCatching { issuesApi.mergePr(accountId, targetIssueId, mergeStack = mergeStack) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    // A stack merge's refusal may be about another member:
                    // the message shows, the conflict offer does not.
                    val failure = if (mergeStack) {
                        MergeFailure.fromStack(t)
                    } else {
                        MergeFailure.from(t, "The pull request could not be merged")
                    }
                    _actionError.value = failure.message
                    _actionErrorFrom.value = PrAction.Merge
                    _actionErrorIsConflict.value = failure.isConflict
                }
            _merging.value = false
        }
    }

    /** Close the issue's open PR WITHOUT merging (EXP-100 reject path). */
    fun closePr() {
        val issueId = issueId ?: return
        if (_closing.value || _merging.value) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _closing.value = true
            _actionError.value = null
            _actionErrorFrom.value = null
            // Never a conflict offer: the recovery run ends in a MERGE, the
            // opposite of what a failed close asked for.
            _actionErrorIsConflict.value = false
            runCatching { issuesApi.closePr(accountId, issueId) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    _actionError.value = MergeFailure.from(t, "The pull request could not be closed").message
                    _actionErrorFrom.value = PrAction.Close
                }
            _closing.value = false
        }
    }
}
