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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.Diff
import com.exponential.app.ui.components.BarCapsule
import com.exponential.app.ui.components.BarCircle
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.FloatingBottomBar
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.ChangesLoadState
import com.exponential.app.ui.issue.ChangesRefusalNotice
import com.exponential.app.ui.issue.ChangesViewModel
import com.exponential.app.ui.issue.DiffFileCard
import com.exponential.app.ui.issue.DiffFileListSheet
import com.exponential.app.ui.issue.diffOpensByDefault
import com.exponential.app.ui.issue.toDiffFile
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.coroutines.launch

// EXP-893/EXP-895: the Work screen's CHANGES FACE — a full page of the ONE diff
// view: the summary row (`N files  +A −D`) over one [DiffFileCard] per file,
// and the floating bar `[file sheet][merge capsule][switcher]`. Two sources,
// one look: source A is the shown session's LIVE worktree diff (`latest_diff`,
// already parsed by the host), source B is the issue's open PR's files off
// `ChangesViewModel`. GitHub is NOT on this bar — it sits in the header's
// action slot, so the leading circle can open the file list (EXP-895).

/** What the bar's centre capsule merges — nothing, the PR, or the recovery run. */
data class ChangesMergeControl(
    val label: String,
    val fixConflicts: Boolean,
    val loading: Boolean,
    /** The refusal a failed merge left behind — captions the bar. */
    val error: String?,
    /** The confirm dialog's body (a run's own PR completes no issue). */
    val confirmText: String,
    val onConfirm: () -> Unit,
    val onFixConflicts: () -> Unit,
)

@Composable
fun ChangesFace(
    padding: PaddingValues,
    /** Source A: the shown session's live diff, wins when present. */
    diff: Diff.Parsed?,
    /** Source B: the issue's PR files — keyed per issue by the host. */
    changesViewModel: ChangesViewModel?,
    merge: ChangesMergeControl?,
    trailingBarSlot: @Composable () -> Unit,
) {
    var mergeConfirmOpen by remember { mutableStateOf(false) }
    var sheetOpen by remember { mutableStateOf(false) }
    val prLoad: ChangesLoadState? = if (changesViewModel != null) {
        val state by changesViewModel.load.collectAsStateWithLifecycle()
        state
    } else {
        null
    }
    // The ONE list both sources land in — GitHub's PullFile is parsed into the
    // shared model here, exactly like the Review page does.
    val files: List<Diff.File> = remember(diff, prLoad) {
        when {
            diff != null -> diff.files
            prLoad is ChangesLoadState.Loaded -> prLoad.files.map { it.toDiffFile() }
            else -> emptyList()
        }
    }
    // A live worktree diff is the run's own output and opens; a PR's files are
    // a review queue and start collapsed (EXP-248), uniform with the web. Past
    // `COLLAPSE_THRESHOLD` lines a file stays shut either way — one lockfile
    // would otherwise bury every card under it.
    val defaultCollapsed = diff == null
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
                val opens = diffOpensByDefault(file, defaultCollapsed)
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
            // A refused merge captions the bar that produced it (EXP-559) —
            // the MESSAGE only; the recovery run takes the capsule's place.
            merge?.error?.let { ChangesRefusalNotice(message = it, modifier = Modifier.padding(horizontal = 16.dp)) }
            FloatingBottomBar(
                left = if (files.isNotEmpty()) {
                    { FileListCircle(count = files.size, onClick = { sheetOpen = true }) }
                } else {
                    null
                },
                right = trailingBarSlot,
            ) {
                if (merge != null) {
                    BarCapsule(
                        label = merge.label,
                        icon = if (merge.fixConflicts) ExpIcons.uiBranch else ExpIcons.prMerged,
                        emphatic = true,
                        loading = merge.loading,
                        onClick = { if (merge.fixConflicts) merge.onFixConflicts() else mergeConfirmOpen = true },
                        modifier = Modifier.testTag("pr-merge-bar"),
                    )
                } else {
                    Spacer(Modifier.weight(1f))
                }
            }
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

    // EXP-498: merging always closes the session too, so the merge is
    // confirm-gated — same copy as Agents and Reviews.
    if (mergeConfirmOpen && merge != null) {
        AlertDialog(
            onDismissRequest = { mergeConfirmOpen = false },
            title = { Text("Merge pull request?") },
            text = { Text(merge.confirmText) },
            confirmButton = {
                TextButton(
                    onClick = {
                        mergeConfirmOpen = false
                        merge.onConfirm()
                    },
                ) { Text("Merge") }
            },
            dismissButton = {
                TextButton(onClick = { mergeConfirmOpen = false }) { Text("Cancel") }
            },
        )
    }
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
                contentDescription = "Changed files",
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
