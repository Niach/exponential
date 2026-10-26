package com.exponential.app.ui.work

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.PullFile
import com.exponential.app.data.api.PullRequestsApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.PullRequestIssueEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.MergeFailure
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// EXP-1291 Threads v2: ONE pull request (a `pull_requests` row) behind a
// Guide — its changed files (`pullRequests.files`) and the Merge / Close PR
// actions (`pullRequests.merge` / `.close`). No local writes: the merge /
// close echo arrives through the pull-requests shape.

sealed interface ChangesLoadState {
    data object Loading : ChangesLoadState
    data class Failed(val message: String) : ChangesLoadState
    data class Loaded(val files: List<PullFile>) : ChangesLoadState
}

@HiltViewModel(assistedFactory = ChangesViewModel.Factory::class)
class ChangesViewModel @AssistedInject constructor(
    /** Assisted: one model per pull request id. */
    @Assisted val pullRequestId: String,
    holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val pullRequestsApi: PullRequestsApi,
) : ViewModel() {

    @AssistedFactory
    interface Factory {
        fun create(pullRequestId: String): ChangesViewModel
    }

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val pullRequest: StateFlow<PullRequestEntity?> =
        dbFlow.scopedQuery<PullRequestEntity?>(null) { it.pullRequestDao().observeById(pullRequestId) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** How many issues the PR closes (the Close PR confirm's batch line). */
    val linkedIssueCount: StateFlow<Int> =
        dbFlow.scopedQuery(emptyList<PullRequestIssueEntity>()) {
            it.pullRequestIssueDao().observeByPullRequest(pullRequestId)
        }.map { it.size }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), 0)

    private val _load = MutableStateFlow<ChangesLoadState>(ChangesLoadState.Loading)
    val load: StateFlow<ChangesLoadState> = _load

    private val _merging = MutableStateFlow(false)
    val merging: StateFlow<Boolean> = _merging
    private val _closing = MutableStateFlow(false)
    val closing: StateFlow<Boolean> = _closing
    private val _actionError = MutableStateFlow<String?>(null)
    val actionError: StateFlow<String?> = _actionError

    /** Which action produced [actionError] — merge and close share the caption. */
    enum class PrAction { Merge, Close }

    private val _actionErrorFrom = MutableStateFlow<PrAction?>(null)
    val actionErrorFrom: StateFlow<PrAction?> = _actionErrorFrom

    /**
     * EXP-1378: the merge was refused by the check gate (HTTP 428) — its toast
     * offers `Merge anyway` ([retryMerge] with force) instead of Retry.
     */
    private val _actionErrorIsChecks = MutableStateFlow(false)
    val actionErrorIsChecks: StateFlow<Boolean> = _actionErrorIsChecks

    /** The last merge's [mergePr] `stack` flag, so a toast's action replays it. */
    private var lastMergeStack = false

    init {
        refresh()
    }

    fun refresh() {
        viewModelScope.launch {
            _load.value = ChangesLoadState.Loading
            try {
                val accountId = auth.activeAccountId.value
                    ?: throw IllegalStateException("No active account")
                val files = pullRequestsApi.files(accountId, pullRequestId).map { file ->
                    PullFile(
                        filename = file.path,
                        status = "modified",
                        additions = file.additions,
                        deletions = file.deletions,
                        patch = file.patch,
                    )
                }
                _load.value = ChangesLoadState.Loaded(files)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _load.value = ChangesLoadState.Failed(trpcErrorMessage(t, ""))
            }
        }
    }

    /**
     * Squash-merge the PR; [stack] = merge THROUGH it (a native stack);
     * [force] = past the check gate (EXP-1378). A refusal lands in
     * [actionError]; the server posts it into the thread too (EXP-1391).
     */
    fun mergePr(stack: Boolean = false, force: Boolean = false) {
        if (_merging.value || _closing.value) return
        lastMergeStack = stack
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _merging.value = true
            _actionError.value = null
            _actionErrorFrom.value = null
            _actionErrorIsChecks.value = false
            runCatching {
                pullRequestsApi.merge(
                    accountId,
                    pullRequestId,
                    stack = if (stack) true else null,
                    force = if (force) true else null,
                )
            }
                .onSuccess { result ->
                    val refusal = result.error
                    if (!result.merged && result.queued != true && !refusal.isNullOrBlank()) {
                        _actionError.value = refusal
                        _actionErrorFrom.value = PrAction.Merge
                    }
                }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    val failure = if (stack) {
                        MergeFailure.fromStack(t)
                    } else {
                        MergeFailure.from(t, "The pull request could not be merged")
                    }
                    // The flag first: the screen keys its toast on the message.
                    _actionErrorIsChecks.value = failure.isChecks
                    _actionError.value = failure.message
                    _actionErrorFrom.value = PrAction.Merge
                }
            _merging.value = false
        }
    }

    /** The refusal toast's action: the SAME merge again (Retry), [force] = `Merge anyway`. */
    fun retryMerge(force: Boolean) = mergePr(stack = lastMergeStack, force = force)

    /** Close the PR WITHOUT merging. */
    fun closePr() {
        if (_closing.value || _merging.value) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _closing.value = true
            _actionError.value = null
            _actionErrorFrom.value = null
            _actionErrorIsChecks.value = false
            runCatching { pullRequestsApi.close(accountId, pullRequestId) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    _actionError.value = MergeFailure.from(t, "The pull request could not be closed").message
                    _actionErrorFrom.value = PrAction.Close
                }
            _closing.value = false
        }
    }
}
