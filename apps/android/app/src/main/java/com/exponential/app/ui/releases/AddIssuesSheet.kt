package com.exponential.app.ui.releases

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity

/**
 * Multi-select issue picker for the release detail's "+" (EXP-56 rework):
 * the shared [IssueMultiSelectPicker] over the workspace's addable issues
 * (status not done/cancelled/duplicate, not already in this release). Rows
 * toggle membership in the selection (sheet stays open); the bottom button
 * confirms the whole batch via releases.addIssues.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AddIssuesSheet(
    candidates: List<IssueEntity>,
    onConfirm: (List<String>) -> Unit,
    onDismiss: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var selected by remember { mutableStateOf(setOf<String>()) }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = { BottomSheetDefaults.DragHandle() },
    ) {
        Column(modifier = Modifier.fillMaxWidth().padding(bottom = 12.dp)) {
            Text(
                "Add issues",
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(horizontal = 24.dp, vertical = 12.dp),
            )
            IssueMultiSelectPicker(
                candidates = candidates,
                selected = selected,
                onToggle = { id ->
                    selected = if (id in selected) selected - id else selected + id
                },
            )
            Spacer(Modifier.height(8.dp))
            Button(
                onClick = {
                    onConfirm(selected.toList())
                    onDismiss()
                },
                enabled = selected.isNotEmpty(),
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp),
            ) {
                Text(
                    when {
                        selected.isEmpty() -> "Add issues"
                        selected.size == 1 -> "Add 1 issue"
                        else -> "Add ${selected.size} issues"
                    }
                )
            }
        }
    }
}
