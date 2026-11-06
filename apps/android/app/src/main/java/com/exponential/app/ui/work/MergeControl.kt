package com.exponential.app.ui.work

import com.exponential.app.ui.components.InlineSpinner
import com.exponential.app.domain.FeedbackStates
import androidx.compose.ui.Alignment
import androidx.compose.material3.LocalContentColor
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.failedAction
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


/**
 * #EXP-1710 (merge-train.json STATUS PILL `Needs approval`, needs-you.json
 * #EXP-1710): THE pill's approve, shared by the thread header pill and a
 * Needs you `approval` ready row. [newest] = the PR; while it sits in an
 * open stack of [stackPool] the ONE stack confirm asks and Merge lands
 * through the stack's top, else the plain confirm (`merge-issue-pr` /
 * `merge-run-pr`). Merge = `pullRequests.merge`, which queues it under the
 * train. [confirmOpen] shows the confirm; Cancel = [onDismiss]. A refused
 * merge toasts with Retry. Returns whether a merge is in flight (the pill's
 * loading).
 */
@Composable
internal fun NeedsApprovalConfirm(
    newest: PullRequestEntity,
    stackPool: List<PullRequestEntity>,
    confirmOpen: Boolean,
    onDismiss: () -> Unit,
): Boolean {
    val stack = remember(newest, stackPool) {
        if (newest.state == DomainContract.prStateOpen) PullRequestStack.stackConfirm(newest, stackPool) else null
    }
    val pr = stack?.throughId?.let { id -> stackPool.firstOrNull { it.id == id } } ?: newest
    val changesVm = hiltViewModel<ChangesViewModel, ChangesViewModel.Factory>(key = "changes:${pr.id}") { it.create(pr.id) }
    val merging by changesVm.merging.collectAsStateWithLifecycle()
    val actionError by changesVm.actionError.collectAsStateWithLifecycle()
    val linkedCount by changesVm.linkedIssueCount.collectAsStateWithLifecycle()
    val toaster = LocalToaster.current
    LaunchedEffect(actionError) {
        val message = actionError ?: return@LaunchedEffect
        toaster.failedAction("merge", "the PR", message, retry = changesVm::retryMerge)
    }
    if (confirmOpen) {
        MergeConfirmDialog(
            merge = ChangesMergeControl(
                label = if (stack != null) DomainContract.diffUiMergeStack else DomainContract.diffUiMergePr,
                loading = merging,
                error = actionError,
                confirmPrompt = if (linkedCount > 0) {
                    Prompts.MergeIssuePr.prompt(pr.number, linkedCount)
                } else {
                    Prompts.MergeRunPr.prompt(pr.number)
                },
                onConfirm = { changesVm.mergePr() },
                stackConfirm = stack,
                onMergeStack = { changesVm.mergePr(stack = true) },
            ),
            onDismiss = onDismiss,
        )
    }
    return merging
}

/**
 * #EXP-1710 (merge-train.json `mergeNowPrompt`): THE Merge now confirm, the
 * thread `…` menu's and a Needs you stuck row's: Merge destructive, Cancel
 * focused, `titleStack` while [pr] sits in an open stack of [stackPool]
 * (Merge now lands the whole stack). Merge = [onMerge].
 */
@Composable
internal fun MergeNowConfirm(
    pr: PullRequestEntity,
    stackPool: List<PullRequestEntity>,
    onDismiss: () -> Unit,
    onMerge: () -> Unit,
) {
    val inStack = remember(pr, stackPool) { PullRequestStack.stackConfirm(pr, stackPool) != null }
    PromptAlert(
        prompt = Prompts.MergeNowPr.prompt(pr.number, stack = inStack),
        onDismiss = onDismiss,
        handlers = mapOf(
            "merge" to {
                onDismiss()
                onMerge()
            },
        ),
    )
}
