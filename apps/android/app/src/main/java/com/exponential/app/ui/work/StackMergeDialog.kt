package com.exponential.app.ui.work

import androidx.compose.runtime.Composable
import com.exponential.app.domain.PrStack
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction

/**
 * EXP-1145: the stack merge dialog every plain Merge control on a PR-stack
 * member opens instead of "Merge pull request?". Copy = [PrStack.stackMergeChoice],
 * byte-identical x4 (fixture `stack-merge-choice.json`). [onMergeStack] takes
 * the issue the server merges the chain THROUGH: the top member for Merge
 * stack, the merged issue for Merge this pull request above the bottom; the
 * bottom member's Merge this pull request is the plain [onMergePlain].
 * EXP-1215: the shared [GlassAlert] card, Cancel · Merge this pull request ·
 * Merge stack (primary, the default); the row stacks when it cannot fit.
 */
@Composable
fun StackMergeDialog(
    choice: PrStack.StackMergeChoice,
    issueId: String,
    onMergeStack: (throughIssueId: String) -> Unit,
    onMergePlain: () -> Unit,
    onDismiss: () -> Unit,
) {
    GlassAlert(
        title = PrStack.STACK_MERGE_CHOICE_TITLE,
        body = choice.body,
        onDismiss = onDismiss,
        trailing = listOf(
            GlassAlertAction(PrStack.STACK_MERGE_CANCEL_LABEL, onClick = onDismiss),
            GlassAlertAction(
                PrStack.MERGE_THIS_PR_LABEL,
                testTag = "merge-this-pr",
                onClick = {
                    onDismiss()
                    if (choice.position > 1) onMergeStack(issueId) else onMergePlain()
                },
            ),
            GlassAlertAction(
                PrStack.MERGE_STACK_LABEL,
                primary = true,
                testTag = "merge-stack",
                onClick = {
                    onDismiss()
                    onMergeStack(choice.topIssueId)
                },
            ),
        ),
        defaultAction = 2,
    )
}
