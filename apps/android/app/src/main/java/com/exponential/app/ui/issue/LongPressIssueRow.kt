package com.exponential.app.ui.issue

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.outlined.Circle
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
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
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity

/**
 * Wraps [IssueRow] with a long-press gesture that opens a [ModalBottomSheet]
 * action list (Mark done / Move to backlog), replacing the old Material 3
 * swipe-to-dismiss row. iOS keeps its native `.swipeActions`; Android uses a
 * long-press → action sheet, the platform-idiomatic list affordance and the
 * same chooser pattern as [IssuePickerSheet] / [LabelPickerSheet].
 *
 * A plain tap always opens the issue via [onClick]. When [canMutate] is false
 * the long-press affordance is omitted (read-only row).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LongPressIssueRow(
    issue: IssueEntity,
    labels: List<LabelEntity>,
    assignee: UserEntity?,
    canMutate: Boolean,
    onMarkDone: () -> Unit,
    onMoveToBacklog: () -> Unit,
    onClick: () -> Unit,
) {
    var showActions by remember { mutableStateOf(false) }

    IssueRow(
        issue = issue,
        labels = labels,
        assignee = assignee,
        onClick = onClick,
        onLongClick = if (canMutate) ({ showActions = true }) else null,
    )

    if (showActions) {
        val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
        ModalBottomSheet(
            onDismissRequest = { showActions = false },
            sheetState = sheetState,
            dragHandle = { BottomSheetDefaults.DragHandle() },
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(bottom = 12.dp),
            ) {
                Text(
                    text = issue.identifier,
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.padding(horizontal = 24.dp, vertical = 12.dp),
                )
                ListItem(
                    headlineContent = { Text("Mark done") },
                    leadingContent = { Icon(Icons.Filled.Check, contentDescription = null) },
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable {
                            onMarkDone()
                            showActions = false
                        },
                )
                ListItem(
                    headlineContent = { Text("Move to backlog") },
                    leadingContent = { Icon(Icons.Outlined.Circle, contentDescription = null) },
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable {
                            onMoveToBacklog()
                            showActions = false
                        },
                )
                Spacer(Modifier.height(8.dp))
            }
        }
    }
}
