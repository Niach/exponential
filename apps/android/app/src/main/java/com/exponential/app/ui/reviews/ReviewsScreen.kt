package com.exponential.app.ui.reviews

import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.domain.ReviewsQueue
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.LoadingState
import com.exponential.app.ui.components.PrList
import com.exponential.app.ui.components.PrListRow
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.StackRail
import com.exponential.app.ui.components.StackRailMember
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/**
 * "Reviews" (EXP-131): the open pull requests across every member team
 * (EXP-1186), grouped by board — its own bottom-bar destination.
 *
 * EXP-1248: every PR is ONE [com.exponential.app.ui.components.PrRow] line ×4
 * and the page only OPENS things: no Merge pill, no swipe, no long-press
 * sheet, no dialogs. Inside a board band a PR TREE nests with tree guides and
 * a linear STACK hangs on its rail down to the base branch ("stack" on the top
 * row); singles stay flat. Bands carry no count. A row opens the issue's Guide
 * (a run PR the run's), an unlinked PR opens GitHub (muted external-link glyph).
 */
@Composable
fun ReviewsScreen(
    /** An issue PR row: the issue's Guide face. */
    onOpenChanges: (String) -> Unit,
    /** EXP-1194: an Agent runs row — the run's own PR on its Guide. */
    onOpenRunChanges: (sessionId: String) -> Unit,
    viewModel: ReviewsViewModel = hiltViewModel(),
) {
    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            Text(
                "Reviews",
                style = MaterialTheme.typography.headlineLarge,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
            )
            ReviewsListContent(
                onOpenIssueGuide = onOpenChanges,
                onOpenRunGuide = onOpenRunChanges,
                viewModel = viewModel,
            )
        }
    }
}

/** A stack-rail member carrying the entry it opens. */
private class ReviewStackMember(val entry: ReviewEntry, label: ReviewRowLabel) :
    StackRailMember(key = entry.groupKey, identifier = label.identifier, title = label.title)

@Composable
private fun ReviewsListContent(
    onOpenIssueGuide: (String) -> Unit,
    onOpenRunGuide: (sessionId: String) -> Unit,
    viewModel: ReviewsViewModel,
    modifier: Modifier = Modifier,
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    // EXP-1244: every entry refetches the unlinked pull requests (no sync).
    LaunchedEffect(viewModel) { viewModel.onScreenEntered() }

    when {
        !state.loaded -> LoadingState(modifier = modifier)
        state.isEmpty -> EmptyState(
            message = "No open pull requests",
            icon = ExpIcons.navReviews,
            modifier = modifier,
        )
        else -> LazyColumn(
            modifier = modifier.fillMaxSize().testTag("reviews-list"),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
        ) {
            state.groups.forEach { group ->
                item(key = "header-${group.board.id}") {
                    BoardHeader(board = group.board, teamName = group.teamName)
                }
                val blocks = reviewBlocks(group.items)
                blocks.forEachIndexed { index, block ->
                    // The band's breathing room sits under its LAST block only.
                    val gap = Modifier.padding(bottom = if (index == blocks.lastIndex) BAND_GAP else 0.dp)
                    when (block) {
                        is ReviewBlock.Stack -> item(key = "stack-${block.entries.firstOrNull()?.groupKey ?: index}") {
                            StackRail(
                                members = block.entries.map { ReviewStackMember(it, reviewRowLabel(it)) },
                                baseBranch = block.baseBranch ?: group.board.defaultBranch ?: "default branch",
                                word = STACK_WORD,
                                onOpen = { member -> onOpenIssueGuide(member.entry.representative.id) },
                                modifier = gap,
                            )
                        }
                        is ReviewBlock.Rows -> item(key = "list-${block.rows.firstOrNull()?.entry?.groupKey ?: index}") {
                            PrList(
                                rows = block.rows.map { row ->
                                    val label = reviewRowLabel(row.entry)
                                    PrListRow(
                                        key = row.entry.groupKey,
                                        identifier = label.identifier,
                                        title = label.title,
                                        depth = row.depth,
                                        onOpen = { onOpenIssueGuide(row.entry.representative.id) },
                                        testTag = "review-row",
                                    )
                                },
                                modifier = gap,
                            )
                        }
                    }
                }
            }
            // EXP-734: the runs that opened a pull request of their OWN — a
            // batch, action or chat run whose PR links no issue. EXP-1186: one
            // band per team once the user is in more than one.
            state.runGroups.forEach { runGroup ->
                item(key = "header-runs-${runGroup.team?.id.orEmpty()}") {
                    RunsHeader(teamName = runGroup.team?.name)
                }
                item(key = "runs-${runGroup.team?.id.orEmpty()}") {
                    PrList(
                        rows = runGroup.entries.map { entry ->
                            PrListRow(
                                key = entry.groupKey,
                                identifier = entry.prNumber?.let { "#$it" },
                                title = entry.title,
                                onOpen = { onOpenRunGuide(entry.session.id) },
                                testTag = "review-run-row",
                            )
                        },
                        modifier = Modifier.padding(bottom = BAND_GAP),
                    )
                }
            }
            // EXP-1244: the open pull requests NO issue or run links, one band
            // per repository; a row opens it on GitHub.
            state.repoGroups.forEach { group ->
                val repo = group.repo
                item(key = "header-repo-${repo.teamId}-${repo.repositoryId}") {
                    RepoHeader(fullName = repo.fullName, teamName = group.teamName)
                }
                item(key = "repo-${repo.teamId}-${repo.repositoryId}") {
                    PrList(
                        rows = repo.pulls.map { pull ->
                            PrListRow(
                                key = "${repo.repositoryId}#${pull.number}",
                                identifier = "#${pull.number}",
                                title = pull.title,
                                word = if (pull.draft) DRAFT_WORD else null,
                                onOpen = {
                                    CustomTabsIntent.Builder().build()
                                        .launchUrl(context, android.net.Uri.parse(pull.url))
                                },
                                testTag = "review-pull-row",
                                trailing = { ExternalLinkGlyph() },
                            )
                        },
                        modifier = Modifier.padding(bottom = BAND_GAP),
                    )
                }
            }
        }
    }
}

/** The muted external-link glyph an unlinked PR wears: it opens GitHub. */
@Composable
private fun ExternalLinkGlyph() {
    Icon(
        ExpIcons.uiExternalLink,
        contentDescription = "Open on GitHub",
        modifier = Modifier.size(14.dp),
        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
    )
}

// EXP-698/EXP-818: the board band over its PR rows — THE section header every
// list renders, with the board icon as its leading glyph and NO count
// (EXP-1248). EXP-1186: with more than one team, the band names the board's
// team as quiet secondary text.
@Composable
private fun BoardHeader(board: BoardEntity, teamName: String?) {
    SectionHeader(
        board.name,
        leading = { BoardIcon(board, size = 14.dp) },
        trailing = teamName?.let { name -> @Composable { BandTeamName(name) } },
    )
}

/** EXP-1186: the band's muted team name (multi-team only). */
@Composable
private fun BandTeamName(name: String) {
    Text(
        name,
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.widthIn(max = 160.dp),
    )
}

/** EXP-734: the band over the issueless runs' pull requests. */
@Composable
private fun RunsHeader(teamName: String?) {
    SectionHeader(
        "Agent runs",
        trailing = { BandTeamName(teamName ?: ReviewsQueue.RUN_BAND_CAPTION) },
        leading = {
            Icon(
                ExpIcons.prOpen,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        },
    )
}

/**
 * EXP-1244: the band over a repository's unlinked pull requests — the PR-open
 * glyph, the repo's full name, and as its quiet trailing text the team (a
 * multi-team list) or [ReviewsQueue.REPO_BAND_CAPTION].
 */
@Composable
private fun RepoHeader(fullName: String, teamName: String?) {
    SectionHeader(
        fullName,
        trailing = { BandTeamName(teamName ?: ReviewsQueue.REPO_BAND_CAPTION) },
        leading = {
            Icon(
                ExpIcons.prOpen,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        },
    )
}

/** The breathing room under a band's rows, before the next band. */
private val BAND_GAP = 12.dp
