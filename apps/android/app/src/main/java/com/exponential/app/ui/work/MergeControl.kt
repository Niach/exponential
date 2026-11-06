package com.exponential.app.ui.work

import com.exponential.app.ui.components.InlineSpinner
import com.exponential.app.domain.FeedbackStates
import androidx.compose.ui.Alignment
import androidx.compose.material3.LocalContentColor
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.MergeTrain
import com.exponential.app.domain.Prompts
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassPillDefaults
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens

// EXP-1154/EXP-1248 (web `session-merge-button.tsx`): the thread header's
// merge control: [MergeStatusPill] (#EXP-1710, merge-train.json STATUS PILL)
// and, for its `Needs approval`, ONE [ChangesMergeControl] the confirm runs
// (the plain confirm, or the ONE stack confirm while the PR sits in an open
// stack).

/** What Needs approval merges — the PR. Always Merge: a refused merge
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
 * #EXP-1710 (merge-train.json STATUS PILL, web `MergeStatusPill`): the thread
 * header's merge pill is a STATUS — THE Pill md, mode action, never primary
 * but for `Needs approval` (the ONLY action, approval mode). The glyph by its
 * icons.json concept (`spinner` = THE Spinner); tone `red` tints only the
 * glyph. [onClick] null = readonly (no check rows); the host maps the tap
 * (`approve` = the merge confirm, `checks` = the Checks submenu). [loading]
 * = an approval in flight.
 */
@Composable
fun MergeStatusPill(
    view: MergeTrain.StatusPillView,
    onClick: (() -> Unit)?,
    modifier: Modifier = Modifier,
    loading: Boolean = false,
) {
    val size = PillSize.Md
    val glyph = view.glyph
    GlassPill(
        view.label,
        icon = if (glyph == "spinner" || view.tone == "red") null else ExpIcons.byConcept(glyph),
        leading = when {
            loading -> null
            glyph == "spinner" -> {
                {
                    Box(Modifier.size(GlassPillDefaults.glyphSize(size)), contentAlignment = Alignment.Center) {
                        InlineSpinner(size = FeedbackStates.PENDING_SPINNER.dp, color = LocalContentColor.current)
                    }
                }
            }
            view.tone == "red" -> {
                {
                    ExpIcons.byConcept(glyph)?.let { icon ->
                        Icon(
                            icon,
                            contentDescription = null,
                            modifier = Modifier.size(GlassPillDefaults.glyphSize(size)),
                            tint = DesignTokens.Semantic.Red,
                        )
                    }
                }
            }
            else -> null
        },
        size = size,
        primary = view.primary,
        loading = loading,
        enabled = !loading,
        onClick = onClick,
        modifier = modifier.testTag(MergeTrain.TestIds.STATUS_PILL),
    )
}

/**
 * EXP-498: merging always closes the session too, so the merge is
 * confirm-gated, same copy as Agents and Reviews. EXP-1248: a member of an
 * open stack asks the ONE stack confirm instead ([StackMergeDialog]: what
 * lands, what stays open).
 */
@Composable
internal fun MergeConfirmDialog(merge: ChangesMergeControl, onDismiss: () -> Unit) {
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

