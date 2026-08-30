package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
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
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.icons.ExpIcons

/**
 * Wraps [IssueRow] with a long-press gesture that opens a [GlassSheet]
 * action list (Mark done / Move to backlog), replacing the old Material 3
 * swipe-to-dismiss row. iOS keeps its native `.swipeActions`; Android uses a
 * long-press → action sheet, the platform-idiomatic list affordance and the
 * same chooser pattern as [IssuePickerSheet] / [LabelPickerSheet].
 *
 * A plain tap always opens the issue via [onClick]. When [canMutate] is false
 * the long-press affordance is omitted (read-only row).
 *
 * The two quick actions stay ENUM writes (EXP-314) — they mean "the builtin
 * Done / Backlog status", and the server trigger derives the matching status
 * row. This row is also used by the cross-team My Issues list, so it renders
 * the ANCHOR status glyph rather than a resolved team row.
 */
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
        GlassSheet(title = issue.identifier, onDismiss = { showActions = false }) {
            GlassSheetRow(
                label = "Mark done",
                onClick = {
                    onMarkDone()
                    showActions = false
                },
                leading = { Icon(ExpIcons.statusDone, contentDescription = null, modifier = Modifier.size(18.dp)) },
            )
            GlassSheetRow(
                label = "Move to backlog",
                onClick = {
                    onMoveToBacklog()
                    showActions = false
                },
                leading = { Icon(ExpIcons.statusBacklog, contentDescription = null, modifier = Modifier.size(18.dp)) },
            )
        }
    }
}
