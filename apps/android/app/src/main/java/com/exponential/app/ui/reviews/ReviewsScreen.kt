package com.exponential.app.ui.reviews

import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.canOfferFixConflicts
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.components.LoadingState
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.StartCodingSheet
import com.exponential.app.ui.steer.SteerRunCaptionRow
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassRow
import com.exponential.app.ui.theme.glassSection

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
    onOpenSteer: (String) -> Unit,
    viewModel: ReviewsViewModel = hiltViewModel(),
) {
    // Remote-start feedback for a "Fix conflicts" run, and the jump into the
    // session once the desktop picks it up (EXP-323).
    val runState by viewModel.runState.collectAsStateWithLifecycle()
    val startedSessionId by viewModel.startedSessionId.collectAsStateWithLifecycle()
    LaunchedEffect(startedSessionId) {
        startedSessionId?.let {
            viewModel.consumeStartedSession()
            onOpenSteer(it)
        }
    }

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            Text(
                "Reviews",
                style = MaterialTheme.typography.headlineLarge,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
            )
            // Start feedback rides the TOP of the screen: the floating bottom
            // nav pill paints over everything the NavHost renders, so nothing
            // that must be read may sit at the bottom edge (EXP-323).
            SteerRunCaptionRow(
                runState,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 6.dp),
            )
            ReviewsListContent(onOpenIssue = onOpenIssue, onOpenChanges = onOpenChanges)
        }
    }
}

/** The bare list — reusable content with no chrome of its own. */
@Composable
private fun ReviewsListContent(
    onOpenIssue: (String) -> Unit,
    onOpenChanges: (String) -> Unit,
    modifier: Modifier = Modifier,
    viewModel: ReviewsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val mergeErrors by viewModel.mergeErrors.collectAsStateWithLifecycle()
    val merging by viewModel.merging.collectAsStateWithLifecycle()
    val devices by viewModel.steerDevices.collectAsStateWithLifecycle()
    val startCandidates by viewModel.startCandidates.collectAsStateWithLifecycle()
    var mergeTarget by remember { mutableStateOf<ReviewEntry?>(null) }
    var fixTarget by remember { mutableStateOf<ReviewEntry?>(null) }

    when {
        !state.loaded -> LoadingState(modifier = modifier)
        state.groups.isEmpty() -> EmptyState(
            message = "No open pull requests",
            icon = ExpIcons.navReviews,
            modifier = modifier,
        )
        else -> LazyColumn(
            modifier = modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
            verticalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            state.groups.forEach { group ->
                item(key = "header-${group.board.id}") {
                    BoardHeader(board = group.board, count = group.entries.size)
                }
                items(group.entries, key = { it.groupKey }) { entry ->
                    ReviewRow(
                        entry = entry,
                        failure = mergeErrors[entry.groupKey],
                        merging = entry.groupKey in merging,
                        onClick = { onOpenChanges(entry.representative.id) },
                        onOpenIssue = { onOpenIssue(entry.representative.id) },
                        onMerge = { mergeTarget = entry },
                        onFixConflicts = { fixTarget = entry },
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

    // "Fix conflicts" (EXP-323, desktop parity): the unified sheet opened on
    // the builtin action with THIS pull request already picked.
    fixTarget?.let { entry ->
        StartCodingSheet(
            devices = devices ?: emptyList(),
            issues = startCandidates,
            preselectedIds = emptySet(),
            preselectedActionId = DomainContract.builtinFixConflictsId,
            preselectedPrIssueId = entry.representative.id,
            onStart = viewModel::startCoding,
            onRunAction = viewModel::runAction,
            onDismiss = { fixTarget = null },
        )
    }
}

@Composable
private fun BoardHeader(board: BoardEntity, count: Int) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 8.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        BoardIcon(board, size = 14.dp)
        Spacer(Modifier.width(8.dp))
        Text(
            board.name,
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Spacer(Modifier.width(8.dp))
        Text(
            count.toString(),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun ReviewRow(
    entry: ReviewEntry,
    failure: MergeFailure?,
    merging: Boolean,
    onClick: () -> Unit,
    onOpenIssue: () -> Unit,
    onMerge: () -> Unit,
    onFixConflicts: () -> Unit,
) {
    val context = LocalContext.current
    var showActions by remember { mutableStateOf(false) }
    // Only a REAL conflict is something the recovery run can fix (EXP-533),
    // and it rebases the PR's branch, so it needs one recorded — desktop
    // applies the same guard on its Reviews rows.
    val canFixConflicts = canOfferFixConflicts(failure, entry.branch)

    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .glassRow()
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
            // (EXP-248: uniform with the web/iOS review rows).
            Row(
                modifier = Modifier
                    .glassButton()
                    .clickable(enabled = !merging, onClick = onMerge)
                    .padding(horizontal = 10.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (merging) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(14.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                } else {
                    Icon(
                        ExpIcons.prMerged,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                        tint = MaterialTheme.colorScheme.onSurface,
                    )
                }
                Spacer(Modifier.width(4.dp))
                Text(
                    "Merge",
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                )
            }
            Spacer(Modifier.width(6.dp))
            Icon(
                ExpIcons.uiChevronRight,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }

        // A refused merge (conflicts, branch protection, GitHub App errors, an
        // unreachable server) captions THIS row (EXP-323) — inside the list,
        // which already clears the floating nav pill, so the reason is always
        // readable. The recovery run sits next to it for a conflict only.
        if (failure != null) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 3.dp)
                    .glassSection()
                    .padding(horizontal = 12.dp, vertical = 8.dp),
            ) {
                Text(
                    failure.message,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                )
                if (canFixConflicts) {
                    Spacer(Modifier.height(8.dp))
                    Row(
                        modifier = Modifier
                            .glassButton()
                            .clickable(onClick = onFixConflicts)
                            .padding(horizontal = 10.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(
                            ExpIcons.uiBranch,
                            contentDescription = null,
                            modifier = Modifier.size(14.dp),
                            tint = MaterialTheme.colorScheme.onSurface,
                        )
                        Spacer(Modifier.width(4.dp))
                        Text(
                            "Fix conflicts",
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                        )
                    }
                }
            }
        }
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
