package com.exponential.app.ui.work

import android.content.Intent
import android.net.Uri
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
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
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
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.PullFile
import com.exponential.app.ui.components.BarCapsule
import com.exponential.app.ui.components.BarCircle
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.FloatingBottomBar
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.ChangesLoadState
import com.exponential.app.ui.issue.ChangesRefusalNotice
import com.exponential.app.ui.issue.ChangesViewModel
import com.exponential.app.ui.issue.DiffAddColor
import com.exponential.app.ui.issue.DiffDelColor
import com.exponential.app.ui.issue.PatchLines
import com.exponential.app.ui.issue.ChangesFileList
import com.exponential.app.ui.issue.splitUnifiedDiff
import com.exponential.app.ui.issue.unifiedDiffStats
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup

// EXP-893: the Work screen's CHANGES FACE — a full page of the diff: the
// summary row (`N files  +A -D`, ASCII hyphen) over per-file blocks, and the
// floating bar (GitHub circle when a PR exists · `Merge PR` / `Fix conflicts`
// capsule when mergeable · the host's switcher). Two sources, one look:
// source A is the shown session's LIVE worktree diff (`latest_diff`, raw
// `git diff` split per file, every block open); source B is the issue's open
// PR's files off `ChangesViewModel` (the Reviews page's list, collapsed rows
// that expand on tap, `changes-file-row`).

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
    diff: String?,
    /** Source B: the issue's PR files — keyed per issue by the host. */
    changesViewModel: ChangesViewModel?,
    prUrl: String?,
    merge: ChangesMergeControl?,
    trailingBarSlot: @Composable () -> Unit,
) {
    val context = LocalContext.current
    var mergeConfirmOpen by remember { mutableStateOf(false) }
    Box(modifier = Modifier.padding(padding).fillMaxSize()) {
        LazyColumn(
            modifier = Modifier.fillMaxSize().testTag("work-changes"),
            contentPadding = PaddingValues(
                start = 16.dp,
                end = 16.dp,
                top = 4.dp,
                bottom = BottomBarInset,
            ),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            if (diff != null) {
                unifiedDiffList(diff)
            } else if (changesViewModel != null) {
                prFileList(changesViewModel)
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
                left = if (!prUrl.isNullOrBlank()) {
                    {
                        BarCircle(onClick = {
                            runCatching {
                                val intent = Intent(Intent.ACTION_VIEW, Uri.parse(prUrl))
                                intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                                context.startActivity(intent)
                            }
                        }) {
                            Icon(
                                ExpIcons.uiGithub,
                                contentDescription = "Open PR on GitHub",
                                modifier = Modifier.size(20.dp),
                                tint = Color.White,
                            )
                        }
                    }
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

/** `N files  +A -D` — the face's first row, ASCII hyphen ×4. */
@Composable
private fun ChangesSummaryRow(files: Int, additions: Int, deletions: Int) {
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 4.dp).testTag("work-changes-summary"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            "$files ${if (files == 1) "file" else "files"}",
            style = MaterialTheme.typography.labelMedium,
            color = secondary,
        )
        Text(
            "+$additions",
            color = DiffAddColor,
            fontFamily = FontFamily.Monospace,
            style = MaterialTheme.typography.labelSmall,
        )
        Text(
            "-$deletions",
            color = DiffDelColor,
            fontFamily = FontFamily.Monospace,
            style = MaterialTheme.typography.labelSmall,
        )
    }
}

/**
 * Source A: the latest worktree diff (raw `git diff` output) split on
 * `diff --git` into per-file blocks with the shared +/−/@@ coloring;
 * horizontal scrolling lives inside each file's code block only. Extracted
 * from the session screen's old "Latest changes" sheet (EXP-893).
 */
private fun LazyListScope.unifiedDiffList(diff: String) {
    val sections = splitUnifiedDiff(diff)
    val stats = unifiedDiffStats(diff)
    item(key = "__summary__") {
        ChangesSummaryRow(files = sections.size, additions = stats.additions, deletions = stats.deletions)
    }
    items(sections.size, key = { "diff_$it" }) { index ->
        val section = sections[index]
        val contextColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
        Column(modifier = Modifier.fillMaxWidth().glassGroup()) {
            if (section.filename.isNotBlank()) {
                Text(
                    section.filename,
                    style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                )
            }
            PatchLines(
                lines = section.lines,
                contextColor = contextColor,
                modifier = Modifier.padding(bottom = 8.dp),
            )
        }
    }
}

/** Source B: the PR's files off the Reviews page's model, same rows. */
private fun LazyListScope.prFileList(viewModel: ChangesViewModel) {
    item(key = "__pr_files__") {
        val load by viewModel.load.collectAsStateWithLifecycle()
        val files: List<PullFile>? = (load as? ChangesLoadState.Loaded)?.files
        if (files != null) {
            ChangesSummaryRow(
                files = files.size,
                additions = files.sumOf { it.additions },
                deletions = files.sumOf { it.deletions },
            )
        }
    }
    item(key = "__pr_rows__") {
        val load by viewModel.load.collectAsStateWithLifecycle()
        // Every file starts collapsed (EXP-248) — uniform with the Reviews
        // page. Held per loaded list, so a refresh folds them again.
        val loadedFiles = (load as? ChangesLoadState.Loaded)?.files
        val expanded = remember(loadedFiles) { mutableStateMapOf<String, Boolean>() }
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            ChangesFileList(load = load, expanded = expanded)
        }
    }
}
