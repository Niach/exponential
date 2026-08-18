package com.exponential.app.ui.issue

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.api.builtinActions
import com.exponential.app.data.api.toActionDto
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.DeviceWorktreeEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueStatusCategory
import com.exponential.app.domain.IssueStatusResolver
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.serialization.json.Json

// The unified Start-coding sheet's Actions-tab data (EXP-257): the selected
// team's actions LIVE from the synced actions shape (EXP-268 — the local Room
// flow, body-less by design; the virtual builtin rows are prepended
// client-side) plus the lookup
// sources the typed input fields render from — the team repo registry for
// `repo` inputs and the synced boards from the local DB for `board` inputs.
// Owned by a dedicated ViewModel so every host screen (Agents / issue list /
// issue detail / Actions) gets the Actions tab without fetching any of it
// itself; running stays with the HOST's ViewModel via the sheet's callback.

/** Actions-list progress: null [actions] with null [error] = still loading. */
data class SheetActionsState(
    val actions: List<ActionDto>? = null,
    val error: String? = null,
)

/** One pickable board for a `board`-typed action input. */
data class StartBoardOption(
    val id: String,
    val name: String,
)

/** One pickable label/status/priority for the EXP-530 automation filter pickers. */
data class StartFilterOption(
    val id: String,
    val name: String,
)

/**
 * One pickable pull request for a `pr`-typed action input (EXP-259, mobile
 * parity EXP-270). A batch coding run links several issues to ONE pull
 * request, so options are deduped by prUrl: [issueId] is the representative
 * issue the server resolves (team-scoped, open-state checked) and
 * [identifiers] lists every linked issue. [linkedIssueIds] carries EVERY
 * linked issue id so a caller holding some other linked id (the Reviews row
 * picks the NEWEST issue, this builder the lowest one) can resolve the option
 * — see [optionForIssue].
 */
data class StartPullRequestOption(
    val issueId: String,
    val prNumber: Int?,
    val identifiers: List<String>,
    val linkedIssueIds: List<String> = listOf(issueId),
) {
    /** `#42 · EXP-1, EXP-2` — the PR number when known, then the linked issues. */
    val label: String
        get() {
            val joined = identifiers.joinToString(", ")
            return when {
                prNumber == null -> joined
                joined.isEmpty() -> "#$prNumber"
                else -> "#$prNumber · $joined"
            }
        }
}

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class StartCodingSheetViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
    private val repositoriesApi: RepositoriesApi,
    selection: TeamSelection,
    private val json: Json,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val scope = combine(auth.activeAccountId, selection.selectedId) { accountId, teamId ->
        accountId to teamId
    }

    /** The selected team's actions (both virtual builtin rows are prepended). */
    val actionsState: StateFlow<SheetActionsState> = combine(dbFlow, selection.selectedId) { db, teamId ->
        db to teamId
    }.flatMapLatest { (db, teamId) ->
        if (db == null || teamId == null) {
            flowOf(SheetActionsState(actions = emptyList()))
        } else {
            db.actionDao().observeByTeam(teamId).map { rows ->
                SheetActionsState(
                    actions = builtinActions(teamId) +
                        rows.map { it.toActionDto(json) },
                )
            }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SheetActionsState())

    /**
     * The synced worktree inventory (EXP-481) behind the sheet's "Resume
     * previous session" offer. Owned here — like the actions/board/PR lookup
     * sources — so every host gets the toggle without new plumbing.
     */
    val deviceWorktrees: StateFlow<List<DeviceWorktreeEntity>> =
        dbFlow.scopedQuery(emptyList<DeviceWorktreeEntity>()) { it.deviceWorktreeDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * The synced device rows (EXP-481) — resolves a picked machine's ROW id
     * for the worktree join when the host handed the sheet poll-derived
     * SteerDevice rows (devices.list carries no rowId).
     */
    val deviceRows: StateFlow<List<DeviceEntity>> =
        dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The team repo registry — options for `repo`-typed inputs (failure = empty). */
    val repos: StateFlow<List<TeamRepo>> = scope.flatMapLatest { (accountId, teamId) ->
        flow {
            emit(emptyList<TeamRepo>())
            if (accountId != null && teamId != null) {
                emit(
                    runCatching { repositoriesApi.list(accountId, teamId) }.fold(
                        onSuccess = { it },
                        onFailure = {
                            if (it is CancellationException) throw it
                            emptyList()
                        },
                    ),
                )
            }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Live, team-scoped boards — options for `board`-typed inputs. */
    val boardOptions: StateFlow<List<StartBoardOption>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.boardDao().observeAll() },
        selection.selectedId,
    ) { boards, teamId ->
        if (teamId == null) {
            emptyList()
        } else {
            boards
                .filter { it.teamId == teamId && it.deletedAt == null }
                .sortedBy { it.name.lowercase() }
                .map { StartBoardOption(id = it.id, name = it.name) }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Live, team-scoped labels — the EXP-530 label-added filter picker. */
    val labelOptions: StateFlow<List<StartFilterOption>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.labelDao().observeAll() },
        selection.selectedId,
    ) { labels, teamId ->
        if (teamId == null) {
            emptyList()
        } else {
            labels
                .filter { it.teamId == teamId }
                .sortedBy { it.name.lowercase() }
                .map { StartFilterOption(id = it.id, name = it.name) }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * The team's REAL status rows in canonical display order — the EXP-530
     * status-changed filter picker. Constructed fallbacks (no row id) can't
     * be a filter target, and duplicate is never pickable (the web
     * buildStatusOptions rule).
     */
    val statusOptions: StateFlow<List<StartFilterOption>> = combine(dbFlow, selection.selectedId) { db, teamId ->
        db to teamId
    }.flatMapLatest { (db, teamId) ->
        if (db == null || teamId == null) {
            flowOf(emptyList())
        } else {
            db.issueStatusDao().observeByTeam(teamId).map { rows ->
                IssueStatusResolver.teamStatuses(rows)
                    .filter { it.category != IssueStatusCategory.Duplicate }
                    .mapNotNull { status ->
                        status.rowId?.let { StartFilterOption(id = it, name = status.name) }
                    }
            }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * Live, team-scoped OPEN pull requests — options for `pr`-typed inputs.
     * Issues don't sync `team_id`, so the scope comes from the synced boards
     * (the same derivation the web picker uses).
     */
    val pullRequestOptions: StateFlow<List<StartPullRequestOption>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.boardDao().observeAll() },
        selection.selectedId,
    ) { issues, boards, teamId ->
        if (teamId == null) {
            emptyList()
        } else {
            val teamBoardIds = boards.filter { it.teamId == teamId }.map { it.id }.toSet()
            buildPullRequestOptions(issues, teamBoardIds)
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())
}

/**
 * Collapse open-PR issue rows into one option per pull request. Rows outside
 * [teamBoardIds], rows whose PR isn't open, and rows without a `prUrl` are
 * skipped (such an id wouldn't resolve server-side anyway). Sorted by label so
 * the list doesn't reshuffle as sync lands rows; the representative issue is
 * the lowest id, so it doesn't depend on query order either.
 */
fun buildPullRequestOptions(
    issues: List<IssueEntity>,
    teamBoardIds: Set<String>,
): List<StartPullRequestOption> = issues
    .asSequence()
    .filter {
        it.boardId in teamBoardIds &&
            it.prState == DomainContract.prStateOpen &&
            !it.prUrl.isNullOrEmpty()
    }
    .sortedBy { it.id }
    .groupBy { it.prUrl!! }
    .map { (_, linked) ->
        StartPullRequestOption(
            issueId = linked.first().id,
            prNumber = linked.first().prNumber,
            identifiers = linked.mapNotNull { it.identifier?.takeIf(String::isNotEmpty) }.sorted(),
            linkedIssueIds = linked.map { it.id },
        )
    }
    .sortedWith(compareBy({ it.label }, { it.issueId }))

/**
 * The option a given issue id belongs to — by MEMBERSHIP, not by the
 * representative id. Callers seeding a `pr` input hold whatever issue their
 * surface acts on (the Reviews row's newest issue, or the batch sibling whose
 * Changes screen is open); only the representative id renders a label in the
 * picker, so the seed is normalised through this (EXP-323).
 */
fun List<StartPullRequestOption>.optionForIssue(issueId: String): StartPullRequestOption? =
    firstOrNull { issueId in it.linkedIssueIds }
