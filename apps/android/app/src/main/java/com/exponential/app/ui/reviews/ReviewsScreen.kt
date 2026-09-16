package com.exponential.app.ui.reviews

import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.PrStack
import com.exponential.app.domain.canOfferFixConflicts
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.components.LoadingState
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import com.exponential.app.ui.theme.glassCard

/**
 * "Reviews" (EXP-131): the open pull requests in the current team, grouped
 * by board. Its own bottom-bar destination beside My Work (EXP-147 — it used
 * to be a PersonalScreen segment). A batch coding run's combined PR shows as
 * ONE entry ("N issues"), never one row per linked issue. Rows open the Review
 * detail (EXP-168 — web parity: the reviews queue reviews PRs, not issues);
 * the long-press sheet keeps an "Open issue" path.
 */
@Composable
fun ReviewsScreen(
    onOpenIssue: (String) -> Unit,
    onOpenChanges: (String) -> Unit,
    // EXP-825: a row's "Fix conflicts" navigates to the Agent page composer
    // on the builtin action with this PR pre-picked (EXP-323).
    onOpenAgent: (AgentComposerSeed) -> Unit,
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
                onOpenIssue = onOpenIssue,
                onOpenChanges = onOpenChanges,
                onOpenAgent = onOpenAgent,
            )
        }
    }
}

/** The bare list — reusable content with no chrome of its own. */
@Composable
private fun ReviewsListContent(
    onOpenIssue: (String) -> Unit,
    onOpenChanges: (String) -> Unit,
    onOpenAgent: (AgentComposerSeed) -> Unit,
    modifier: Modifier = Modifier,
    viewModel: ReviewsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val mergeErrors by viewModel.mergeErrors.collectAsStateWithLifecycle()
    val merging by viewModel.merging.collectAsStateWithLifecycle()
    var mergeTarget by remember { mutableStateOf<ReviewEntry?>(null) }
    // EXP-734: an issueless run's own PR — merged through the session, so it
    // gets its own confirm target.
    var mergeRunTarget by remember { mutableStateOf<RunReviewEntry?>(null) }
    // EXP-897: the bottom row whose whole stack is about to be merged.
    var mergeStackTarget by remember { mutableStateOf<ReviewRowEntry?>(null) }

    when {
        !state.loaded -> LoadingState(modifier = modifier)
        state.isEmpty -> EmptyState(
            message = "No open pull requests",
            icon = ExpIcons.navReviews,
            modifier = modifier,
        )
        else -> LazyColumn(
            modifier = modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
            // EXP-818: flat rows under a band — the 6dp every converted list
            // uses, not the 3dp gap the carded rows needed.
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            state.groups.forEach { group ->
                item(key = "header-${group.board.id}") {
                    BoardHeader(board = group.board)
                }
                items(group.rows, key = { it.entry.groupKey }) { row ->
                    val entry = row.entry
                    ReviewRow(
                        row = row,
                        failure = mergeErrors[entry.groupKey],
                        merging = entry.groupKey in merging,
                        onClick = { onOpenChanges(entry.representative.id) },
                        onOpenIssue = { onOpenIssue(entry.representative.id) },
                        onMerge = { mergeTarget = entry },
                        // EXP-897: only the BOTTOM row of a real stack offers it.
                        onMergeStack = { mergeStackTarget = row },
                        // EXP-323/EXP-825: the composer opens on the builtin
                        // with THIS pull request already picked.
                        onFixConflicts = {
                            onOpenAgent(
                                AgentComposerSeed(
                                    actionId = DomainContract.builtinFixConflictsId,
                                    prIssueId = entry.representative.id,
                                ),
                            )
                        },
                    )
                }
            }
            // EXP-734: the runs that opened a pull request of their OWN — an
            // action or chat run whose PR links no issue, so no board group
            // can hold it. Listed last, under one header.
            if (state.runs.isNotEmpty()) {
                item(key = "header-runs") { RunsHeader() }
                items(state.runs, key = { it.groupKey }) { entry ->
                    RunReviewRow(
                        entry = entry,
                        failure = mergeErrors[entry.groupKey],
                        merging = entry.groupKey in merging,
                        onMerge = { mergeRunTarget = entry },
                    )
                }
            }
        }
    }

    mergeTarget?.let { entry ->
        MergeConfirmDialog(
            entry = entry,
            onConfirm = {
                viewModel.mergePr(entry.groupKey, entry.representative.id)
                mergeTarget = null
            },
            onDismiss = { mergeTarget = null },
        )
    }

    mergeStackTarget?.let { row ->
        MergeStackConfirmDialog(
            count = row.stackSize,
            onConfirm = {
                row.mergeStackIssueId?.let { viewModel.mergeStack(row.entry.groupKey, it) }
                mergeStackTarget = null
            },
            onDismiss = { mergeStackTarget = null },
        )
    }

    mergeRunTarget?.let { entry ->
        // EXP-734: no issue is linked, so nothing is completed — say so.
        val prLabel = entry.prNumber?.let { "PR #$it" } ?: "the pull request"
        AlertDialog(
            onDismissRequest = { mergeRunTarget = null },
            title = { Text("Merge pull request?") },
            text = {
                Text(
                    "Squash-merges $prLabel via the GitHub App. " +
                        "Any live coding session for it closes.",
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        viewModel.mergeRun(entry)
                        mergeRunTarget = null
                    },
                ) { Text("Merge") }
            },
            dismissButton = {
                TextButton(onClick = { mergeRunTarget = null }) { Text("Cancel") }
            },
        )
    }

}

// EXP-698/EXP-818: the board band over its PR rows — THE section header every
// list renders, with the board icon as its leading glyph and no count (the
// header counts are gone on every client).
@Composable
private fun BoardHeader(board: BoardEntity) {
    SectionHeader(
        board.name,
        leading = { BoardIcon(board, size = 14.dp) },
    )
}

/**
 * EXP-734: the band over the issueless runs' pull requests. Its glyph is the
 * actions one, not a board icon — these PRs belong to a RUN, not to a board.
 */
@Composable
private fun RunsHeader() {
    SectionHeader(
        "Agent runs",
        leading = {
            Icon(
                ExpIcons.navActions,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        },
    )
}

/**
 * One run's OWN pull request (EXP-734). There is no issue and no Review detail
 * to open, so the row opens the PR on GitHub; merging goes through
 * `codingSessions.mergePr` and completes nothing. No "Fix conflicts" here —
 * the recovery run takes an issue-linked PR as its input.
 */
@Composable
private fun RunReviewRow(
    entry: RunReviewEntry,
    failure: MergeFailure?,
    merging: Boolean,
    onMerge: () -> Unit,
) {
    val context = LocalContext.current
    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .flatRow()
                .clickable(enabled = entry.prUrl != null) {
                    entry.prUrl?.let {
                        CustomTabsIntent.Builder().build()
                            .launchUrl(context, android.net.Uri.parse(it))
                    }
                }
                .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Same green PR glyph the issue-backed review rows wear (EXP-248).
            Icon(
                ExpIcons.prOpen,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = DesignTokens.Semantic.Green,
            )
            Spacer(Modifier.width(10.dp))
            Column(modifier = Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        entry.prNumber?.let { "#$it" } ?: "PR",
                        style = MaterialTheme.typography.labelMedium,
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(
                        entry.title,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                }
                if (!entry.branch.isNullOrBlank()) {
                    Spacer(Modifier.height(2.dp))
                    Text(
                        entry.branch,
                        style = MaterialTheme.typography.labelSmall,
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            Spacer(Modifier.width(6.dp))
            GlassPill(
                "Merge",
                onClick = onMerge,
                icon = ExpIcons.prMerged,
                enabled = !merging,
                loading = merging,
            )
        }

        // A refused merge captions THIS row, like every other merge surface.
        if (failure != null) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 3.dp)
                    .glassCard()
                    .padding(horizontal = 12.dp, vertical = 8.dp),
            ) {
                Text(
                    failure.message,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun ReviewRow(
    row: ReviewRowEntry,
    failure: MergeFailure?,
    merging: Boolean,
    onClick: () -> Unit,
    onOpenIssue: () -> Unit,
    onMerge: () -> Unit,
    onMergeStack: () -> Unit,
    onFixConflicts: () -> Unit,
) {
    val entry = row.entry
    val context = LocalContext.current
    var showActions by remember { mutableStateOf(false) }
    // EXP-897: the batch glyph opens the issues its ONE pull request spans —
    // the same overlay the Work screen's badge shows.
    var showBatch by remember { mutableStateOf(false) }
    // Only a REAL conflict is something the recovery run can fix (EXP-533),
    // and it rebases the PR's branch, so it needs one recorded — desktop
    // applies the same guard on its Reviews rows.
    val canFixConflicts = canOfferFixConflicts(failure, entry.branch)

    // EXP-897: one stack level is 14dp of indent, on every client.
    Column(modifier = Modifier.fillMaxWidth().padding(start = (STACK_INDENT_DP * row.depth).dp)) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .flatRow()
                .combinedClickable(onClick = onClick, onLongClick = { showActions = true })
                .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // PR glyph — green like the iOS/web review rows (EXP-248).
            Icon(
                ExpIcons.prOpen,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = DesignTokens.Semantic.Green,
            )
            Spacer(Modifier.width(10.dp))
            Column(modifier = Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (entry.isBatch) {
                        // The batch mark (EXP-897): a tap lists the issues, in
                        // its own hit area so the row's own tap still opens the
                        // review.
                        Box(
                            modifier = Modifier
                                .size(24.dp)
                                .clickable { showBatch = true }
                                .testTag("review-batch-glyph"),
                            contentAlignment = Alignment.Center,
                        ) {
                            Icon(
                                ExpIcons.prBatch,
                                contentDescription = "Issues in this batch",
                                modifier = Modifier.size(14.dp),
                                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            )
                        }
                        Spacer(Modifier.width(4.dp))
                        Text(
                            entry.prNumber?.let { "#$it" } ?: "Batch",
                            style = MaterialTheme.typography.labelMedium,
                            fontFamily = FontFamily.Monospace,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            maxLines = 1,
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(
                            "${entry.issues.size} issues",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    } else {
                        Text(
                            entry.representative.identifier,
                            style = MaterialTheme.typography.labelMedium,
                            fontFamily = FontFamily.Monospace,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            maxLines = 1,
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(
                            entry.representative.title,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f, fill = false),
                        )
                    }
                }
                // EXP-897: an upper stack member names its foundation, in the
                // same words on every client.
                row.stackedOn?.let { below ->
                    Spacer(Modifier.height(2.dp))
                    Text(
                        "on top of #$below",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.testTag("review-stacked-on"),
                    )
                }
                // Secondary line: the batch entry lists its issue identifiers; every
                // entry shows its branch (parity with the web/desktop Reviews rows).
                val subtitle = buildString {
                    if (entry.isBatch) append(entry.identifiers.joinToString(", "))
                    if (entry.branch != null) {
                        if (isNotEmpty()) append(" · ")
                        append(entry.branch)
                    }
                }
                if (subtitle.isNotEmpty()) {
                    Spacer(Modifier.height(2.dp))
                    Text(
                        subtitle,
                        style = MaterialTheme.typography.labelSmall,
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            Spacer(Modifier.width(6.dp))
            // Inline merge — same confirm-gated flow as the long-press sheet
            // (EXP-248: uniform with the web/iOS review rows). EXP-706: a
            // conflict-refused merge REPLACES the pill with the recovery run
            // rather than stacking a second button under the message — the
            // one thing that can move this PR forward sits where the user
            // just tapped.
            // EXP-698: the shared pill, not a second hand-rolled copy of it
            // (the steer screen's Merge control is the same component).
            GlassPill(
                if (canFixConflicts) "Fix conflicts" else "Merge",
                onClick = if (canFixConflicts) onFixConflicts else onMerge,
                icon = if (canFixConflicts) ExpIcons.uiBranch else ExpIcons.prMerged,
                enabled = !merging,
                loading = merging,
            )
        }

        // A refused merge (conflicts, branch protection, GitHub App errors, an
        // unreachable server) captions THIS row (EXP-323) — inside the list,
        // which already clears the floating nav pill, so the reason is always
        // readable. EXP-706: the message ONLY; the recovery run took the merge
        // pill's place in the row above.
        if (failure != null) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 3.dp)
                    .glassCard()
                    .padding(horizontal = 12.dp, vertical = 8.dp),
            ) {
                Text(
                    failure.message,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }
    }

    if (showBatch) {
        BatchIssuesSheet(entry = entry, onDismiss = { showBatch = false })
    }

    if (showActions) {
        GlassSheet(
            title = if (entry.isBatch) {
                entry.prNumber?.let { "PR #$it" } ?: "Batch PR"
            } else {
                entry.representative.identifier
            },
            onDismiss = { showActions = false },
        ) {
            // Row taps open the Review detail (EXP-168), so issue access
            // moves here — the representative issue for a batch entry.
            GlassSheetRow(
                label = "Open issue",
                onClick = {
                    showActions = false
                    onOpenIssue()
                },
                leading = { Icon(ExpIcons.navMyIssues, contentDescription = null, modifier = Modifier.size(18.dp)) },
            )
            GlassSheetRow(
                label = "Merge pull request",
                onClick = {
                    showActions = false
                    onMerge()
                },
                leading = { Icon(ExpIcons.prOpen, contentDescription = null, modifier = Modifier.size(18.dp)) },
            )
            // EXP-897: the BOTTOM of a stack can take the whole chain in one
            // go — merging it merges every pull request above it, bottom-up.
            if (row.mergeStackIssueId != null) {
                GlassSheetRow(
                    label = PrStack.MERGE_STACK_LABEL,
                    onClick = {
                        showActions = false
                        onMergeStack()
                    },
                    leading = { Icon(ExpIcons.prStack, contentDescription = null, modifier = Modifier.size(18.dp)) },
                )
            }
            // The recovery run needs the PR's branch to rebase (EXP-323).
            if (!entry.branch.isNullOrBlank()) {
                GlassSheetRow(
                    label = "Fix merge conflicts",
                    onClick = {
                        showActions = false
                        onFixConflicts()
                    },
                    leading = { Icon(ExpIcons.uiBranch, contentDescription = null, modifier = Modifier.size(18.dp)) },
                )
            }
            if (entry.prUrl != null) {
                GlassSheetRow(
                    label = "Open PR",
                    onClick = {
                        showActions = false
                        CustomTabsIntent.Builder().build()
                            .launchUrl(context, android.net.Uri.parse(entry.prUrl))
                    },
                    leading = { Icon(ExpIcons.uiExternalLink, contentDescription = null, modifier = Modifier.size(18.dp)) },
                )
            }
        }
    }
}

@Composable
private fun MergeConfirmDialog(
    entry: ReviewEntry,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val prLabel = entry.prNumber?.let { "PR #$it" } ?: "the pull request"
    val message = buildString {
        append("Squash-merges $prLabel via the GitHub App. Any live coding session for it closes.")
        if (entry.isBatch) append(" Completes all ${entry.issues.size} linked issues.")
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Merge pull request?") },
        text = { Text(message) },
        confirmButton = { TextButton(onClick = onConfirm) { Text("Merge") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

/** EXP-897: one stack level of indent, the ×4 number. */
private const val STACK_INDENT_DP = 14

/**
 * EXP-897: the issues a batch pull request spans — the Reviews list's half of
 * the Work screen badge's "In batch with" section, same rows, same words.
 */
@Composable
private fun BatchIssuesSheet(entry: ReviewEntry, onDismiss: () -> Unit) {
    GlassSheet(
        title = entry.prNumber?.let { "PR #$it" } ?: "Batch PR",
        onDismiss = onDismiss,
    ) {
        entry.issues.forEach { issue ->
            GlassSheetRow(
                label = "${issue.identifier} · ${issue.title}",
                onClick = onDismiss,
                leading = {
                    Icon(
                        ExpIcons.navMyIssues,
                        contentDescription = null,
                        modifier = Modifier.size(18.dp),
                    )
                },
            )
        }
    }
}

/**
 * EXP-897: merging the bottom of a stack merges everything above it. Same
 * title and same sentence on all four clients.
 */
@Composable
private fun MergeStackConfirmDialog(count: Int, onConfirm: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Merge the whole stack?") },
        text = { Text("$count pull requests, bottom-up.") },
        confirmButton = { TextButton(onClick = onConfirm) { Text(PrStack.MERGE_STACK_LABEL) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
