package com.exponential.app.ui.work

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.Diff
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.BarCircle
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.FloatingBarCluster
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.ChangesLoadState
import com.exponential.app.ui.issue.DiffFileCard
import com.exponential.app.ui.issue.DiffFileListSheet
import com.exponential.app.ui.issue.diffOpensByDefault
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.coroutines.launch

// EXP-893/EXP-895: the Work screen's CHANGES FACE — a full page of the ONE diff
// view: the summary row (`N files  +A −D`) over one [DiffFileCard] per file,
// and the floating bar `[file sheet]` (EXP-1150: Merge PR moved into the
// header band beside the face tabs, [MergePrHeaderPill]). Two sources,
// one look: source A is the shown session's LIVE worktree diff (`latest_diff`,
// already parsed by the host), source B is the issue's open PR's files off
// `ChangesViewModel`. GitHub is NOT on this bar — it sits in the header's
// action slot, so the leading circle can open the file list (EXP-895).

/** What the header's Merge PR pill merges — the PR, or the recovery run.
 *  EXP-1150: the host builds ONE (off the live run or the issue's PR) and the
 *  header draws it on every face ([MergePrHeaderPill]). */
data class ChangesMergeControl(
    val label: String,
    val fixConflicts: Boolean,
    val loading: Boolean,
    /** The refusal a failed merge left behind — toasted by the pill. */
    val error: String?,
    /** The confirm dialog's body (a run's own PR completes no issue). */
    val confirmText: String,
    val onConfirm: () -> Unit,
    val onFixConflicts: () -> Unit,
)

@Composable
fun ChangesFace(
    padding: PaddingValues,
    /**
     * EXP-932: the ONE list both sources land in — the shown session's live
     * diff, else the issue's PR files parsed into the shared model. The HOST
     * resolves it, so every count of the run's changes reads exactly these
     * files; two derivations meant two different numbers for one run.
     */
    files: List<Diff.File>,
    /** Source B's load state — what captions an empty [files]. */
    prLoad: ChangesLoadState?,
) {
    var sheetOpen by remember { mutableStateOf(false) }
    val expanded = remember(files) { mutableStateMapOf<String, Boolean>() }
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()

    Box(modifier = Modifier.padding(padding).fillMaxSize()) {
        LazyColumn(
            modifier = Modifier.fillMaxSize().testTag("work-changes"),
            state = listState,
            contentPadding = PaddingValues(
                start = 16.dp,
                end = 16.dp,
                top = 4.dp,
                bottom = BottomBarInset,
            ),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            item(key = "__summary__") {
                when {
                    files.isNotEmpty() -> {
                        val totals = remember(files) { Diff.totals(files) }
                        ChangesSummaryRow(totals)
                    }
                    prLoad is ChangesLoadState.Loading -> ChangesLoadingRow()
                    prLoad is ChangesLoadState.Failed -> ChangesFailureRow(prLoad.message)
                    prLoad is ChangesLoadState.Loaded -> ChangesEmptyRow()
                    else -> Spacer(Modifier.size(0.dp))
                }
            }
            items(files.size, key = { "diff_file_$it" }) { index ->
                val file = files[index]
                // EXP-916: a file card opens by DEFAULT everywhere — the
                // Changes face is a page of changes, and a column of shut
                // headers says nothing. Past `COLLAPSE_THRESHOLD` lines a file
                // still stays shut: one lockfile would otherwise bury every
                // card under it.
                val opens = diffOpensByDefault(file, defaultCollapsed = false)
                DiffFileCard(
                    file = file,
                    expanded = expanded[file.path] ?: opens,
                    onToggle = { expanded[file.path] = !(expanded[file.path] ?: opens) },
                )
            }
        }
        Column(
            modifier = Modifier.align(Alignment.BottomCenter).fillMaxWidth(),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            // EXP-916/EXP-1150: the files circle alone — Merge PR sits in
            // the header band beside the face tabs.
            FloatingBarCluster(
                left = if (files.isNotEmpty()) {
                    { FileListCircle(count = files.size, onClick = { sheetOpen = true }) }
                } else {
                    null
                },
            )
        }
    }

    // Picking a file closes the sheet, opens that card and scrolls to it — the
    // phone twin of the desktop IDE's `scroll_to_file`. +1 for the summary row.
    if (sheetOpen) {
        DiffFileListSheet(
            files = files,
            onPick = { path ->
                sheetOpen = false
                expanded[path] = true
                val index = files.indexOfFirst { it.path == path }
                if (index >= 0) scope.launch { listState.animateScrollToItem(index + 1) }
            },
            onDismiss = { sheetOpen = false },
        )
    }
}

/**
 * EXP-1150: the header's Merge PR — the [ChangesMergeControl] as a compact
 * primary pill at the face strip's end, on every face, running the confirm.
 * With a real conflict it becomes Fix conflicts (the
 * branch glyph) and opens the recovery run. A refusal toasts.
 */
@Composable
fun MergePrHeaderPill(merge: ChangesMergeControl, modifier: Modifier = Modifier) {
    var mergeConfirmOpen by remember { mutableStateOf(false) }
    val toaster = LocalToaster.current
    // The refusal toasts ONCE: the error itself stays in its model (it also
    // drives the Fix conflicts verb and the Changes screen's notice), so the
    // pill remembers what it already showed across rotation and re-toasts
    // only a NEW refusal (a retry clears the error first).
    var toastedError by rememberSaveable { mutableStateOf<String?>(null) }
    LaunchedEffect(merge.error) {
        val error = merge.error
        if (error == null) {
            toastedError = null
        } else if (error != toastedError) {
            toastedError = error
            toaster.error(error)
        }
    }
    GlassPill(
        label = merge.label,
        icon = if (merge.fixConflicts) ExpIcons.uiBranch else ExpIcons.prMerged,
        size = PillSize.Sm,
        primary = true,
        loading = merge.loading,
        enabled = !merge.loading,
        onClick = { if (merge.fixConflicts) merge.onFixConflicts() else mergeConfirmOpen = true },
        modifier = modifier.testTag("work-merge-pr"),
    )
    if (mergeConfirmOpen) {
        MergeConfirmDialog(merge = merge, onDismiss = { mergeConfirmOpen = false })
    }
}

/**
 * EXP-498: merging always closes the session too, so the merge is
 * confirm-gated — same copy as Agents and Reviews.
 */
@Composable
private fun MergeConfirmDialog(merge: ChangesMergeControl, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Merge pull request?") },
        text = { Text(merge.confirmText) },
        confirmButton = {
            TextButton(
                onClick = {
                    onDismiss()
                    merge.onConfirm()
                },
            ) { Text("Merge") }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Cancel") }
        },
    )
}

/**
 * The bar's LEADING circle (EXP-895): the files glyph over the file count,
 * opening the `Changed files` sheet. The count is the affordance — a reader
 * sees how much is in the review before opening anything.
 */
@Composable
private fun FileListCircle(count: Int, onClick: () -> Unit) {
    BarCircle(onClick = onClick, modifier = Modifier.testTag("changes-file-list-button")) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Icon(
                ExpIcons.navFiles,
                contentDescription = DomainContract.diffUiChangedFilesTitle,
                modifier = Modifier.size(18.dp),
                tint = Color.White,
            )
            Text(
                count.toString(),
                style = MaterialTheme.typography.labelSmall,
                color = Color.White.copy(alpha = TextEmphasis.Secondary),
            )
        }
    }
}

/** `N files  +A −D` — the face's first row, the shared labels ×4. */
@Composable
private fun ChangesSummaryRow(totals: Diff.Totals) {
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 4.dp).testTag("work-changes-summary"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            "${totals.files} ${if (totals.files == 1) "file" else "files"}",
            style = MaterialTheme.typography.labelMedium,
            color = secondary,
        )
        Text(
            Diff.additionsLabel(totals.additions),
            color = DesignTokens.Diff.AddFg,
            fontFamily = FontFamily.Monospace,
            style = MaterialTheme.typography.labelSmall,
        )
        Text(
            Diff.deletionsLabel(totals.deletions),
            color = DesignTokens.Diff.DelFg,
            fontFamily = FontFamily.Monospace,
            style = MaterialTheme.typography.labelSmall,
        )
    }
}

@Composable
private fun ChangesLoadingRow() {
    Row(
        modifier = Modifier.padding(vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
        Text(
            "Loading changes…",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
    }
}

@Composable
private fun ChangesFailureRow(message: String) {
    Text(
        "Couldn’t load changes: $message",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.error,
        modifier = Modifier.padding(vertical = 12.dp),
    )
}

@Composable
private fun ChangesEmptyRow() {
    Text(
        "No changed files.",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        modifier = Modifier.padding(vertical = 12.dp),
    )
}
