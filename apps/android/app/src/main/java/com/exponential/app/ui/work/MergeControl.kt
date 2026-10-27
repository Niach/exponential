package com.exponential.app.ui.work

import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.Prompts
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.BarCircle
import com.exponential.app.ui.components.BarSolidPill
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.ui.icons.ExpIcons

// EXP-1154/EXP-1248: the Work screen's ONE merge control (web
// `session-merge-button.tsx`): the host builds ONE [ChangesMergeControl] (off
// the live run or the issue's PR) and every face draws it on its floating bar
// — the white [MergeCapsule] on the Guide, the glyph-only [MergeCircle] right
// of the Issue / Run composer capsule. Labelled `Merge stack` while the PR
// sits in an open stack (one confirm listing what lands), else `Merge PR`.

/** What the Merge capsule merges — the PR. Always Merge: a refused merge
 *  toasts from the host (EXP-1391: the server hands it to the thread). */
data class ChangesMergeControl(
    /** `Merge PR`, or `Merge stack` while [stackConfirm] is non-null. */
    val label: String,
    val loading: Boolean,
    /** The refusal a failed merge left behind, toasted once by the host. */
    val error: String?,
    /** EXP-1215: the confirm prompt, `merge-issue-pr` or (a run's own PR,
     *  no issue linked) `merge-run-pr`. */
    val confirmPrompt: Prompts.Prompt,
    val onConfirm: () -> Unit,
    /** EXP-1248/EXP-1291: non-null = the PR is a member of an open stack —
     *  Merge asks the ONE stack confirm ([StackMergeDialog]) and lands the
     *  whole open chain through [onMergeStack]. */
    val stackConfirm: PullRequestStack.Confirm? = null,
    val onMergeStack: () -> Unit = {},
)

/**
 * EXP-1154: the ONE Merge PR on a phone — the SOLID WHITE capsule (the
 * EXP-916 [BarSolidPill], 52dp, hugging its label) in the Guide's bar
 * cluster, running the confirm (or the ONE stack confirm). A refusal toasts
 * from the host (the check gate's offers `Merge anyway`, EXP-1378). The host
 * builds it only while the PR is
 * open, so it self-hides with the PR. The Issue / Run bars carry the same
 * control as the glyph-only [MergeCircle] (EXP-1191).
 */
@Composable
fun MergeCapsule(
    merge: ChangesMergeControl,
    modifier: Modifier = Modifier,
    /**
     * Every pager page keeps its own capsule composed, so only the thread
     * Guide's carries the fixture's `thread-guide-merge` (EXP-1307,
     * session-thread.json `testIds.guideMerge`); the Guide's section page
     * suffixes [MergeCapsuleTag], like iOS.
     */
    tag: String = SessionTree.ThreadCopy.TEST_ID_GUIDE_MERGE,
) {
    MergeTrigger(merge) { onClick ->
        BarSolidPill(
            label = merge.label,
            icon = ExpIcons.prMerged,
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
 * composer capsule. Same control, same confirm / stack flow
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
                .semantics { contentDescription = merge.label },
        ) {
            if (merge.loading) {
                CircularProgressIndicator(
                    modifier = Modifier.size(18.dp),
                    strokeWidth = 2.dp,
                    color = Color.White,
                )
            } else {
                Icon(
                    ExpIcons.prMerged,
                    contentDescription = null,
                    modifier = Modifier.size(20.dp),
                    tint = Color.White,
                )
            }
        }
    }
}

/**
 * EXP-1291: an issue's PR row's Merge — the small glass pill trailing the
 * row (an issue can list several PRs, so the control rides each open one).
 * Same confirm / stack flow as [MergeCapsule]; spins while merging.
 */
@Composable
fun MergePill(merge: ChangesMergeControl, modifier: Modifier = Modifier) {
    MergeTrigger(merge) { onClick ->
        GlassPill(
            merge.label,
            icon = ExpIcons.prMerged,
            size = PillSize.Sm,
            loading = merge.loading,
            enabled = !merge.loading,
            onClick = onClick,
            modifier = modifier,
        )
    }
}

/**
 * The ONE tap behaviour both Merge shapes share: a merge asks first
 * ([MergeConfirmDialog], the stack choice included). A refusal toasts ONCE in the host (WorkScreen), never per
 * control: the pager keeps neighbouring faces composed.
 */
@Composable
private fun MergeTrigger(
    merge: ChangesMergeControl,
    content: @Composable (onClick: () -> Unit) -> Unit,
) {
    var mergeConfirmOpen by remember { mutableStateOf(false) }
    content { mergeConfirmOpen = true }
    if (mergeConfirmOpen) {
        MergeConfirmDialog(merge = merge, onDismiss = { mergeConfirmOpen = false })
    }
}

/** The Merge tag stem the Guide's section page suffixes (`-section`). */
const val MergeCapsuleTag = "work-merge-pr"

/**
 * EXP-498: merging always closes the session too, so the merge is
 * confirm-gated, same copy as Agents and Reviews. EXP-1248: a member of an
 * open stack asks the ONE stack confirm instead ([StackMergeDialog]: what
 * lands, what stays open).
 */
@Composable
private fun MergeConfirmDialog(merge: ChangesMergeControl, onDismiss: () -> Unit) {
    val stack = merge.stackConfirm
    if (stack != null) {
        StackMergeDialog(confirm = stack, onConfirm = merge.onMergeStack, onDismiss = onDismiss)
        return
    }
    PromptAlert(
        prompt = merge.confirmPrompt,
        onDismiss = onDismiss,
        handlers = mapOf(
            "merge" to {
                onDismiss()
                merge.onConfirm()
            },
        ),
    )
}

