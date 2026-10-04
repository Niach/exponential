package com.exponential.app.ui.work

import androidx.compose.foundation.layout.Row
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import com.exponential.app.domain.PrStack

/**
 * EXP-1145: the stack merge dialog every plain Merge control on a PR-stack
 * member opens instead of "Merge pull request?". Copy = [PrStack.stackMergeChoice],
 * byte-identical x4 (fixture `stack-merge-choice.json`). [onMergeStack] takes
 * the issue the server merges the chain THROUGH: the top member for Merge
 * stack, the merged issue for Merge this pull request above the bottom; the
 * bottom member's Merge this pull request is the plain [onMergePlain].
 */
@Composable
fun StackMergeDialog(
    choice: PrStack.StackMergeChoice,
    issueId: String,
    onMergeStack: (throughIssueId: String) -> Unit,
    onMergePlain: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(PrStack.STACK_MERGE_CHOICE_TITLE) },
        text = { Text(choice.body) },
        confirmButton = {
            TextButton(
                onClick = {
                    onDismiss()
                    onMergeStack(choice.topIssueId)
                },
                modifier = Modifier.testTag("merge-stack"),
            ) { Text(PrStack.MERGE_STACK_LABEL) }
        },
        dismissButton = {
            Row {
                TextButton(
                    onClick = {
                        onDismiss()
                        if (choice.position > 1) onMergeStack(issueId) else onMergePlain()
                    },
                    modifier = Modifier.testTag("merge-this-pr"),
                ) { Text(PrStack.MERGE_THIS_PR_LABEL) }
                TextButton(onClick = onDismiss) { Text(PrStack.STACK_MERGE_CANCEL_LABEL) }
            }
        },
    )
}
