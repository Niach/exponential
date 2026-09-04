package com.exponential.app.ui.issue

import androidx.compose.runtime.Composable
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.ui.components.GlassSheet

/**
 * Issue picker for "Mark as duplicate…" (masterplan §5e): searchable list of
 * the team's other issues; picking one sets `duplicateOfId` + status
 * `duplicate` atomically via the issues.update mutation. Glass chrome (EXP-240);
 * the list itself is the shared [IssueCandidateList] (EXP-736).
 */
@Composable
fun DuplicatePickerSheet(
    candidates: List<IssueEntity>,
    onPick: (IssueEntity) -> Unit,
    onDismiss: () -> Unit,
) {
    GlassSheet(title = "Duplicate of…", onDismiss = onDismiss) {
        IssueCandidateList(
            candidates = candidates,
            onPick = {
                onPick(it)
                onDismiss()
            },
        )
    }
}
