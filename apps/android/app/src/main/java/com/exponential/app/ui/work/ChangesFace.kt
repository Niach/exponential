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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.ui.platform.LocalLayoutDirection
import com.exponential.app.ui.components.FloatingBarEdge
import com.exponential.app.ui.components.detailHazeSource
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.Diff
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.PrStack
import com.exponential.app.ui.components.BarCircle
import com.exponential.app.ui.components.BarSolidPill
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.FloatingBarCluster
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DiffFileCard
import com.exponential.app.ui.issue.DiffFileListSheet
import com.exponential.app.ui.issue.diffOpensByDefault
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.coroutines.launch

// EXP-893/EXP-895: the Work screen's CHANGES FACE — a full page of the ONE diff
// view: the summary row (`N files  +A −D`) over one [DiffFileCard] per file,
// and the floating bar's centred cluster `[files circle] [Merge PR]`
// (EXP-1154: the white [MergeCapsule] came back from the header band). Two
// sources, one look: source A is the shown session's LIVE worktree diff
// (`latest_diff`, already parsed by the host), source B is the issue's open
// PR's (or pushed branch's) files off [ChangesViewModel]. GitHub is NOT on
// this bar — it sits in the header's action slot (EXP-895). EXP-1154: this
// face IS the review of a PR; the standalone review page is gone.

/** What the Merge capsule merges — the PR, or the recovery run.
 *  The host builds ONE (off the live run or the issue's PR) and every face
 *  draws it on its floating bar ([MergeCapsule] / [MergeCircle], EXP-1154/1191). */
data class ChangesMergeControl(
    val label: String,
    val fixConflicts: Boolean,
    val loading: Boolean,
    /** The refusal a failed merge left behind, toasted once by the host. */
    val error: String?,
    /** The confirm dialog's body (a run's own PR completes no issue). */
    val confirmText: String,
    val onConfirm: () -> Unit,
    val onFixConflicts: () -> Unit,
    /** EXP-1145: non-null = the merged PR is a stack member, so Merge asks first. */
    val stackChoice: PrStack.StackMergeChoice? = null,
    /** EXP-1145: the issue whose PR the plain merge lands (the stack dialog's "this one"). */
    val stackIssueId: String? = null,
    /** EXP-1145: merge the open stack bottom-up THROUGH the given issue. */
    val onMergeStack: (throughIssueId: String) -> Unit = {},
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
    /** EXP-1154: the bar's white Merge PR beside the files circle; null hides it. */
    merge: ChangesMergeControl? = null,
    /**
     * EXP-1154: a file a Results row asked for — opened and scrolled to once
     * it is in [files], then [onFocusConsumed] clears it.
     */
    focusPath: String? = null,
    onFocusConsumed: () -> Unit = {},
) {
    var sheetOpen by remember { mutableStateOf(false) }
    val expanded = remember(files) { mutableStateMapOf<String, Boolean>() }
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()
    // EXP-1154: a Guide file row's tap lands here — open that card and bring
    // it up (+1 for the summary row), the file sheet's pick without the sheet.
    LaunchedEffect(focusPath, files) {
        val path = focusPath ?: return@LaunchedEffect
        val index = files.indexOfFirst { it.path == path }
        if (index < 0) {
            // Not loaded yet: wait for the files, unless they settled without it.
            if (files.isNotEmpty() || prLoad !is ChangesLoadState.Loading) onFocusConsumed()
            return@LaunchedEffect
        }
        expanded[path] = true
        listState.animateScrollToItem(index + 1)
        onFocusConsumed()
    }

    // EXP-1162: the page runs under the header band and the floating bar —
    // the host's insets are the list's CONTENT padding, not the face's.
    val layoutDirection = LocalLayoutDirection.current
    val bottomInset = padding.calculateBottomPadding()
    Box(
        modifier = Modifier
            .padding(
                start = padding.calculateStartPadding(layoutDirection),
                end = padding.calculateEndPadding(layoutDirection),
            )
            .fillMaxSize(),
    ) {
        LazyColumn(
            modifier = Modifier.fillMaxSize().detailHazeSource().testTag("work-changes"),
            state = listState,
            contentPadding = PaddingValues(
                start = 16.dp,
                end = 16.dp,
                top = padding.calculateTopPadding() + 4.dp,
                bottom = BottomBarInset + bottomInset,
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
        // EXP-1162: the bottom edge strip fades the list out under the bar.
        FloatingBarEdge(
            bottomInset = bottomInset,
            visible = files.isNotEmpty() || merge != null,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .padding(bottom = bottomInset),
        ) {
            // EXP-916/EXP-1154: `[files circle] [Merge PR]`, centred.
            FloatingBarCluster(
                left = if (files.isNotEmpty()) {
                    { FileListCircle(count = files.size, onClick = { sheetOpen = true }) }
                } else {
                    null
                },
                centre = merge?.let { control -> { MergeCapsule(control) } },
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
 * EXP-1154: the ONE Merge PR on a phone — the SOLID WHITE capsule (the
 * EXP-916 [BarSolidPill], 52dp, hugging its label) in the Changes / Results
 * bar cluster, running the confirm (or the stack dialog). With a real
 * conflict it becomes Fix conflicts (the branch glyph) and opens the
 * recovery run. A refusal toasts. The host builds it only while the PR is
 * open, so it self-hides with the PR. The Issue / Run bars carry the same
 * control as the glyph-only [MergeCircle] (EXP-1191).
 */
@Composable
fun MergeCapsule(
    merge: ChangesMergeControl,
    modifier: Modifier = Modifier,
    /**
     * Every pager page keeps its own capsule composed, so only the Changes
     * face's carries the bare `work-merge-pr` (the store slide's pop-out rect
     * reads the first match); the others suffix their face, like iOS.
     */
    tag: String = MergeCapsuleTag,
) {
    MergeTrigger(merge) { onClick ->
        BarSolidPill(
            label = merge.label,
            icon = mergeGlyph(merge),
            loading = merge.loading,
            enabled = !merge.loading,
            onClick = onClick,
            // EXP-627: the store slide's pop-out rect is measured off this tag.
            modifier = modifier.testTag(tag),
        )
    }
}

/**
 * EXP-1191: the Issue / Run faces' Merge — the bar's own 52dp glass
 * [BarCircle] carrying only the merge glyph (white), directly right of the
 * composer capsule. Same control, same confirm / stack / fix-conflicts flow
 * as [MergeCapsule]; a merging circle spins in place of its glyph.
 */
@Composable
fun MergeCircle(merge: ChangesMergeControl, tag: String, modifier: Modifier = Modifier) {
    MergeTrigger(merge) { onClick ->
        BarCircle(
            onClick = onClick,
            enabled = !merge.loading,
            modifier = modifier
                .testTag(tag)
                .semantics { contentDescription = if (merge.fixConflicts) merge.label else MERGE_CIRCLE_LABEL },
        ) {
            if (merge.loading) {
                CircularProgressIndicator(
                    modifier = Modifier.size(18.dp),
                    strokeWidth = 2.dp,
                    color = Color.White,
                )
            } else {
                Icon(
                    mergeGlyph(merge),
                    contentDescription = null,
                    modifier = Modifier.size(20.dp),
                    tint = Color.White,
                )
            }
        }
    }
}

private const val MERGE_CIRCLE_LABEL = "Merge PR"

private fun mergeGlyph(merge: ChangesMergeControl) =
    if (merge.fixConflicts) ExpIcons.uiBranch else ExpIcons.prMerged

/**
 * The ONE tap behaviour both Merge shapes share: Fix conflicts opens the
 * recovery run, a merge asks first ([MergeConfirmDialog], the stack choice
 * included). A refusal toasts ONCE in the host (WorkScreen), never per
 * control: the pager keeps neighbouring faces composed.
 */
@Composable
private fun MergeTrigger(
    merge: ChangesMergeControl,
    content: @Composable (onClick: () -> Unit) -> Unit,
) {
    var mergeConfirmOpen by remember { mutableStateOf(false) }
    content { if (merge.fixConflicts) merge.onFixConflicts() else mergeConfirmOpen = true }
    if (mergeConfirmOpen) {
        MergeConfirmDialog(merge = merge, onDismiss = { mergeConfirmOpen = false })
    }
}

/** The Changes face's capsule tag; the other faces append `-issue`/`-run`/`-results`. */
const val MergeCapsuleTag = "work-merge-pr"

/**
 * EXP-498: merging always closes the session too, so the merge is
 * confirm-gated, same copy as Agents and Reviews. EXP-1145: a stack member
 * asks which merge it means.
 */
@Composable
private fun MergeConfirmDialog(merge: ChangesMergeControl, onDismiss: () -> Unit) {
    val stackChoice = merge.stackChoice
    val stackIssueId = merge.stackIssueId
    if (stackChoice != null && stackIssueId != null) {
        StackMergeDialog(
            choice = stackChoice,
            issueId = stackIssueId,
            onMergeStack = merge.onMergeStack,
            onMergePlain = merge.onConfirm,
            onDismiss = onDismiss,
        )
        return
    }
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
