package com.exponential.app.ui.work

import androidx.compose.runtime.Composable
import com.exponential.app.domain.PrStack
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction

/** The confirm's dismiss (`stack-merge-choice.json` `confirm.labels.cancel`). */
internal const val STACK_CONFIRM_CANCEL_LABEL = "Cancel"

/**
 * EXP-1248: the ONE stack merge confirm (it replaces the EXP-1145 3-way
 * dialog), web `StackMergeConfirmDialog`. Copy = [PrStack.stackMergeConfirm],
 * byte-identical x4 (fixture `stack-merge-choice.json` `confirm`): the title
 * IS the primary button (Merge stack / Merge through here), the body lists
 * what lands and what stays open. [onConfirm] gets the issue
 * `issues.mergePr({ mergeStack: true })` takes ([PrStack.StackMergeConfirm.issueId]).
 * The shared [GlassAlert] card, Cancel · the primary (the default).
 */
@Composable
fun StackMergeDialog(
    confirm: PrStack.StackMergeConfirm,
    onConfirm: (issueId: String) -> Unit,
    onDismiss: () -> Unit,
) {
    GlassAlert(
        title = confirm.title,
        body = confirm.body,
        onDismiss = onDismiss,
        trailing = listOf(
            GlassAlertAction(STACK_CONFIRM_CANCEL_LABEL, onClick = onDismiss),
            GlassAlertAction(
                confirm.title,
                primary = true,
                testTag = "merge-stack",
                onClick = {
                    onDismiss()
                    onConfirm(confirm.issueId)
                },
            ),
        ),
        defaultAction = 1,
    )
}
