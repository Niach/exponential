package com.exponential.app.ui.reviews

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.CHAT_RUN_NAME
import com.exponential.app.domain.chatRunSubject
import com.exponential.app.domain.PullRepo
import com.exponential.app.domain.ReviewsQueue
import com.exponential.app.data.OpenPullsStore
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// Reviews (EXP-131): every open pull request across EVERY member team
// (EXP-1186, like the Inbox), grouped by board. A batch coding run links N issues to ONE pr_url, so the list
// collapses those rows into a single entry (never N). Pure client work over the
// already-synced issues shape — no new shape, no server round-trip to list.
// EXP-1248: the page only OPENS things (no merge, swipe or long-press ×4);
// inside a board band a PR TREE nests and a linear STACK rails over its base.

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
 * EXP-1248: a review row's label ×4 (web `reviewRowLabel`): the issue's
 * identifier and title; a batch PR = `<first identifier> +<n-1>` beside the
 * first issue's title.
 */
data class ReviewRowLabel(val identifier: String, val title: String)

fun reviewRowLabel(entry: ReviewEntry): ReviewRowLabel {
    val first = entry.representative
    val more = entry.issues.size - 1
    return ReviewRowLabel(
        identifier = if (more > 0) "${first.identifier} +$more" else first.identifier,
        title = first.title,
    )
}

/** EXP-1248: one board-band item as drawn (`ReviewsQueue` rule 9). */
sealed class ReviewItem {
    /** A lone PR (depth 0) or a member of a PR TREE, pre-order. */
    data class Pr(val entry: ReviewEntry, val depth: Int) : ReviewItem()

    /** A linear STACK: TOP first, then the base-branch row. */
    data class Stack(val entries: List<ReviewEntry>, val baseBranch: String?) : ReviewItem()
}

/** EXP-1248: a band's draw blocks ×4 (web `reviewBlocks`): consecutive [ReviewItem.Pr]
 *  items share ONE PR list (a tree's guides span its rows), each stack is its own rail. */
sealed class ReviewBlock {
    data class Rows(val rows: List<ReviewItem.Pr>) : ReviewBlock()
    data class Stack(val entries: List<ReviewEntry>, val baseBranch: String?) : ReviewBlock()
}

fun reviewBlocks(items: List<ReviewItem>): List<ReviewBlock> {
    val blocks = ArrayList<ReviewBlock>()
    for (item in items) {
        when (item) {
            is ReviewItem.Stack -> blocks.add(ReviewBlock.Stack(item.entries, item.baseBranch))
            is ReviewItem.Pr -> {
                val last = blocks.lastOrNull()
                if (last is ReviewBlock.Rows) {
                    blocks[blocks.size - 1] = ReviewBlock.Rows(last.rows + item)
                } else {
                    blocks.add(ReviewBlock.Rows(listOf(item)))
                }
            }
        }
    }
    return blocks
}

/** The word on a stack's top row ×4. */
const val STACK_WORD = "stack"

/** The word on an unlinked draft PR ×4. */
const val DRAFT_WORD = "draft"

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
 * The open-PR runs → review entries: [ReviewsQueue.runs] (collapsed by
 * `pr_url`, the NEWEST row kept, newest first; a run whose `pr_url` an issue
 * already carries ([issuePrUrls]) stays with that issue's entry) mapped to the
 * screen's rows.
 */
fun buildRunEntries(
    sessions: List<CodingSessionEntity>,
    issuePrUrls: Set<String> = emptySet(),
): List<RunReviewEntry> = ReviewsQueue.runs(sessions, issuePrUrls).map(::runEntry)

private fun runEntry(session: CodingSessionEntity) = RunReviewEntry(
    groupKey = "session:${session.id}",
    session = session,
    prUrl = session.prUrl,
    prNumber = session.prNumber,
    branch = session.branch,
    title = chatRunSubject(session) ?: session.actionName ?: CHAT_RUN_NAME,
)

/**
 * The Reviews state: a thin adapter over [ReviewsQueue.build] (EXP-1244, the
 * ONE queue ×4, fixture-locked). [teams] in display order (name order, every
 * member team — EXP-1186); with MORE than one team each band names its team.
 */
fun buildReviewsState(
    issues: List<IssueEntity>,
    boards: List<BoardEntity>,
    runs: List<CodingSessionEntity>,
    teams: List<TeamEntity>,
    pulls: List<PullRepo> = emptyList(),
): ReviewsState {
    val queue = ReviewsQueue.build(teams, boards, issues, runs, pulls)
    val multiTeam = teams.size > 1
    val teamsById = teams.associateBy { it.id }
    fun reviewEntry(entry: ReviewsQueue.Entry): ReviewEntry {
        val representative = entry.representative
        return ReviewEntry(
            groupKey = entry.key,
            prUrl = representative.prUrl,
            prNumber = representative.prNumber,
            branch = representative.branch,
            boardId = representative.boardId,
            issues = entry.issues,
        )
    }
    val groups = queue.boardGroups.map { group ->
        ReviewBoardGroup(
            board = group.board,
            entries = group.entries.map(::reviewEntry),
            items = group.items.map { item ->
                when (item) {
                    is ReviewsQueue.Item.Pr -> ReviewItem.Pr(reviewEntry(item.entry), item.depth)
                    is ReviewsQueue.Item.Stack ->
                        ReviewItem.Stack(item.entries.map(::reviewEntry), item.baseBranch)
                }
            },
            teamName = if (multiTeam) teamsById[group.board.teamId]?.name else null,
        )
    }
    val runGroups = queue.runGroups.map { group ->
        ReviewRunGroup(
            team = if (multiTeam) teamsById[group.teamId] else null,
            entries = group.sessions.map(::runEntry),
        )
    }
    val repoGroups = queue.repoGroups.map { repo ->
        ReviewRepoGroup(
            repo = repo,
            teamName = if (multiTeam) teamsById[repo.teamId]?.name else null,
        )
    }
    return ReviewsState(
        groups = groups,
        runs = runGroups.flatMap { it.entries },
        runGroups = runGroups,
        repoGroups = repoGroups,
        count = queue.count,
        loaded = true,
    )
}

data class ReviewBoardGroup(
    val board: BoardEntity,
    /** One entry per open pull request, newest first — a FLAT list. */
    val entries: List<ReviewEntry>,
    /** EXP-1248: the same entries as drawn: trees nest, stacks rail. */
    val items: List<ReviewItem> = entries.map { ReviewItem.Pr(it, 0) },
    /** EXP-1186: the board's team, named only when the user is in >1 team. */
    val teamName: String? = null,
)

/**
 * EXP-1186: one "Agent runs" band. [team] is null with a single team (one
 * band, exactly as before), else the band's team.
 */
data class ReviewRunGroup(
    val team: TeamEntity?,
    val entries: List<RunReviewEntry>,
)

/**
 * EXP-1244: one repository band of open pull requests NO issue or run links
 * (`repositories.openPulls`), after the "Agent runs" bands. [teamName] names
 * the team on a multi-team list, else the band reads
 * [ReviewsQueue.REPO_BAND_CAPTION].
 */
data class ReviewRepoGroup(
    val repo: PullRepo,
    val teamName: String? = null,
)

data class ReviewsState(
    val groups: List<ReviewBoardGroup> = emptyList(),
    // EXP-734: issueless runs whose OWN pull request is open — listed under
    // their own header, after the board groups.
    val runs: List<RunReviewEntry> = emptyList(),
    /** EXP-1186: [runs] banded per team (one band with a single team). */
    val runGroups: List<ReviewRunGroup> = emptyList(),
    /** EXP-1244: the unlinked pull requests, one band per repository. */
    val repoGroups: List<ReviewRepoGroup> = emptyList(),
    /** Entries + runs + repo pulls (`ReviewsQueue` rule 8). */
    val count: Int = 0,
    val loaded: Boolean = false,
) {
    val isEmpty: Boolean get() = groups.isEmpty() && runs.isEmpty() && repoGroups.isEmpty()
}

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ReviewsViewModel @Inject constructor(
    holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val openPulls: OpenPullsStore,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** EXP-1244: the member team ids, in display order. */
    private val teamIds: Flow<List<String>> =
        dbFlow.flatMapLatest { db ->
            db?.teamDao()?.observeAll()?.map { teams -> teams.map { it.id } } ?: flowOf(emptyList())
        }.distinctUntilChanged()

    /**
     * EXP-1244: the app-wide [OpenPullsStore]'s pulls for every member team —
     * the SAME entries the Reviews tab's dot reads. A team whose fetch fails
     * lists nothing; the synced queue renders regardless.
     */
    private val pulls: Flow<List<PullRepo>> =
        combine(auth.activeAccountId, teamIds) { accountId, ids -> accountId to ids }
            .flatMapLatest { (accountId, ids) -> openPulls.pulls(accountId, ids) }

    /**
     * EXP-1244: the screen calls it on every entry — a forced refetch of every
     * member team's unlinked pull requests.
     */
    fun onScreenEntered() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            openPulls.refreshAll(accountId, teamIds.first(), force = true)
        }
    }

    // EXP-1186: cross-team like the Inbox — never the selected team.
    val state: StateFlow<ReviewsState> =
        dbFlow
            .flatMapLatest { db ->
                if (db == null) {
                    flowOf(ReviewsState(loaded = true))
                } else {
                    // EXP-1244: every PR-carrying issue and run, ANY state —
                    // the queue's linked set must be complete.
                    combine(
                        db.issueDao().observeReviewQueueIssues(),
                        db.boardDao().observeAll(),
                        db.codingSessionDao().observeWithPrUrl(),
                        db.teamDao().observeAll(),
                        pulls,
                    ) { issues, boards, runs, teams, pulls ->
                        buildReviewsState(issues, boards, runs, teams, pulls)
                    }
                }
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ReviewsState())
}
