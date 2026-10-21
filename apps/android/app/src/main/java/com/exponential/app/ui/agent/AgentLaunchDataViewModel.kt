package com.exponential.app.ui.agent

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.routeTeamIdFlow
import com.exponential.app.data.api.PromptDto
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.api.toPromptDto
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.DeviceWorktreeEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.PullRequestIssueEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.toPickerBoard
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

// The launcher's LOOKUP data (EXP-257, renamed from StartCodingSheetViewModel
// in EXP-825 when the three-tab sheet became the Agent page composer): the
// selected team's actions LIVE from the synced actions shape (EXP-268 — the
// local Room flow, body-less by design; EXP-1291: saved prompts only, no
// builtins) plus the sources the typed input fields render from — the team
// repo registry for `repo` inputs, the synced boards for `board` inputs, the
// open pull requests for `pr` inputs — and the worktree inventory behind the
// Resume offer. Owned by a dedicated ViewModel so the composer, the action
// editor and the trigger form share one fetch; starting stays with
// AgentComposerViewModel.

/** Actions-list progress: null [actions] with null [error] = still loading. */
data class SheetActionsState(
    val actions: List<PromptDto>? = null,
    val error: String? = null,
)

/**
 * One pickable board for a `board`-typed action input. EXP-1030: it carries
 * the board's GLYPH and COLOUR too — a board is its icon+colour pair wherever
 * it is picked (EXP-449), and the shared `BoardPicker` draws that pair.
 */
data class StartBoardOption(
    val id: String,
    val name: String,
    /**
     * EXP-1021: the board's resolved glyph NAME and colour, so every surface
     * that lists these can draw the BOARD row the rest of the app draws
     * (`boardPickerItems`) instead of a bare label. Resolved by
     * [toPickerBoard], never guessed from a null here.
     */
    val icon: String? = null,
    val colorHex: String? = null,
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
    /** EXP-1291: the `pull_requests` row id — a `pr` input's value. */
    val pullRequestId: String,
    val prNumber: Int?,
    val identifiers: List<String>,
    val linkedIssueIds: List<String> = emptyList(),
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
class AgentLaunchDataViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    auth: AuthRepository,
    holder: DatabaseHolder,
    private val repositoriesApi: RepositoriesApi,
    selection: TeamSelection,
    private val json: Json,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    /**
     * EXP-1186: the route's team — the composer seeded from another team's
     * row, or the action page of another team's action ([routeTeamIdFlow]);
     * otherwise the selection.
     */
    val teamId: StateFlow<String?> =
        routeTeamIdFlow(dbFlow, savedStateHandle, selection.selectedId)
            .stateIn(viewModelScope, SharingStarted.Eagerly, selection.selectedId.value)

    private val scope = combine(auth.activeAccountId, teamId) { accountId, teamId ->
        accountId to teamId
    }

    /**
     * The selected team's saved prompts (NEW-2: the `prompts` shape), in
     * server order — "no subject" on the composer is a plain message.
     */
    val actionsState: StateFlow<SheetActionsState> = combine(dbFlow, teamId) { db, teamId ->
        db to teamId
    }.flatMapLatest { (db, teamId) ->
        if (db == null || teamId == null) {
            flowOf(SheetActionsState(actions = emptyList()))
        } else {
            db.promptDao().observeByTeam(teamId).map { rows ->
                SheetActionsState(actions = rows.map { it.toPromptDto() })
            }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SheetActionsState())

    /**
     * The synced worktree inventory (EXP-481) behind the composer's "Resume
     * previous session" offer. Owned here — like the actions/board/PR lookup
     * sources — so the composer needs no plumbing of its own.
     */
    val deviceWorktrees: StateFlow<List<DeviceWorktreeEntity>> =
        dbFlow.scopedQuery(emptyList<DeviceWorktreeEntity>()) { it.deviceWorktreeDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * The synced device rows (EXP-481) — resolves a picked machine's ROW id
     * for the worktree join when the composer holds a poll-derived
     * SteerDevice row (devices.list carries no rowId).
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
        teamId,
    ) { boards, teamId ->
        if (teamId == null) {
            emptyList()
        } else {
            boards
                .filter { it.teamId == teamId && it.deletedAt == null }
                .sortedBy { it.name.lowercase() }
                .map { board ->
                    val row = board.toPickerBoard()
                    StartBoardOption(
                        id = row.id,
                        name = row.name,
                        icon = row.icon,
                        colorHex = row.colorHex,
                    )
                }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * Live, team-scoped OPEN pull requests (`pull_requests` + the issues they
     * close) — options for `pr`-typed inputs.
     */
    val pullRequestOptions: StateFlow<List<StartPullRequestOption>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.pullRequestDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.pullRequestIssueDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        teamId,
    ) { prs, links, issues, teamId ->
        if (teamId == null) emptyList() else buildPullRequestOptions(prs, links, issues, teamId)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())
}

/**
 * One option per OPEN pull request of [teamId], naming the issues it closes.
 * Sorted by label so the list doesn't reshuffle as sync lands rows.
 */
fun buildPullRequestOptions(
    pullRequests: List<PullRequestEntity>,
    links: List<PullRequestIssueEntity>,
    issues: List<IssueEntity>,
    teamId: String,
): List<StartPullRequestOption> {
    val issueById = issues.associateBy { it.id }
    val linksByPr = links.groupBy { it.pullRequestId }
    return pullRequests
        .filter { it.teamId == teamId && it.state == DomainContract.prStateOpen }
        .map { pr ->
            val linked = linksByPr[pr.id].orEmpty().mapNotNull { issueById[it.issueId] }
            StartPullRequestOption(
                pullRequestId = pr.id,
                prNumber = pr.number,
                identifiers = linked.map { it.identifier }.sorted(),
                linkedIssueIds = linked.map { it.id },
            )
        }
        .sortedWith(compareBy({ it.label }, { it.pullRequestId }))
}

/**
 * The option a given issue id belongs to — by MEMBERSHIP (the PR closes
 * it), or the option whose PR id it is.
 */
fun List<StartPullRequestOption>.optionForIssue(issueId: String): StartPullRequestOption? =
    firstOrNull { issueId in it.linkedIssueIds || it.pullRequestId == issueId }
