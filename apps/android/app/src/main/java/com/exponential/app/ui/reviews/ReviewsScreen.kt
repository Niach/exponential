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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.CallMerge
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
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
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.LoadingState
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

/**
 * "Reviews" (EXP-131): the open pull requests in the current workspace, grouped
 * by project. Its own bottom-bar destination beside My Work (EXP-147 — it used
 * to be a PersonalScreen segment). A batch coding run's combined PR shows as
 * ONE entry ("N issues"), never one row per linked issue. Rows open the Review
 * detail (EXP-168 — web parity: the reviews queue reviews PRs, not issues);
 * the long-press sheet keeps an "Open issue" path.
 */
@Composable
fun ReviewsScreen(
    onOpenIssue: (String) -> Unit,
    onOpenChanges: (String) -> Unit,
) {
    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            Text(
                "Reviews",
                style = MaterialTheme.typography.headlineLarge,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
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
    var mergeTarget by remember { mutableStateOf<ReviewEntry?>(null) }

    when {
        !state.loaded -> LoadingState(modifier = modifier)
        state.groups.isEmpty() -> EmptyState(
            message = "No open pull requests",
            icon = Icons.AutoMirrored.Filled.CallMerge,
            modifier = modifier,
        )
        else -> LazyColumn(
            modifier = modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
            verticalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            state.groups.forEach { group ->
                item(key = "header-${group.project.id}") {
                    ProjectHeader(name = group.project.name, count = group.entries.size)
                }
                items(group.entries, key = { it.groupKey }) { entry ->
                    ReviewRow(
                        entry = entry,
                        onClick = { onOpenChanges(entry.representative.id) },
                        onOpenIssue = { onOpenIssue(entry.representative.id) },
                        onMerge = { mergeTarget = entry },
                    )
                }
            }
        }
    }

    mergeTarget?.let { entry ->
        MergeConfirmDialog(
            entry = entry,
            onConfirm = {
                viewModel.mergePr(entry.representative.id)
                mergeTarget = null
            },
            onDismiss = { mergeTarget = null },
        )
    }
}

@Composable
private fun ProjectHeader(name: String, count: Int) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 8.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            name,
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

@OptIn(ExperimentalFoundationApi::class, ExperimentalMaterial3Api::class)
@Composable
private fun ReviewRow(
    entry: ReviewEntry,
    onClick: () -> Unit,
    onOpenIssue: () -> Unit,
    onMerge: () -> Unit,
) {
    val context = LocalContext.current
    var showActions by remember { mutableStateOf(false) }

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .combinedClickable(onClick = onClick, onLongClick = { showActions = true })
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            Icons.AutoMirrored.Filled.CallMerge,
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
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
        Icon(
            Icons.AutoMirrored.Filled.KeyboardArrowRight,
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }

    if (showActions) {
        val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
        ModalBottomSheet(
            onDismissRequest = { showActions = false },
            sheetState = sheetState,
            dragHandle = { BottomSheetDefaults.DragHandle() },
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(bottom = 12.dp),
            ) {
                Text(
                    text = if (entry.isBatch) {
                        entry.prNumber?.let { "PR #$it" } ?: "Batch PR"
                    } else {
                        entry.representative.identifier
                    },
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.padding(horizontal = 24.dp, vertical = 12.dp),
                )
                // Row taps open the Review detail (EXP-168), so issue access
                // moves here — the representative issue for a batch entry.
                ListItem(
                    headlineContent = { Text("Open issue") },
                    leadingContent = { Icon(Icons.AutoMirrored.Filled.List, contentDescription = null) },
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable {
                            showActions = false
                            onOpenIssue()
                        },
                )
                ListItem(
                    headlineContent = { Text("Merge pull request") },
                    leadingContent = { Icon(Icons.AutoMirrored.Filled.CallMerge, contentDescription = null) },
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable {
                            showActions = false
                            onMerge()
                        },
                )
                if (entry.prUrl != null) {
                    ListItem(
                        headlineContent = { Text("Open PR") },
                        leadingContent = { Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = null) },
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable {
                                showActions = false
                                CustomTabsIntent.Builder().build()
                                    .launchUrl(context, android.net.Uri.parse(entry.prUrl))
                            },
                    )
                }
                Spacer(Modifier.height(8.dp))
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
        append("Squash-merges $prLabel via the GitHub App.")
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
