package com.exponential.app.ui.issue

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import com.exponential.app.data.api.SearchIssueHit
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.ui.components.picker.IssuePicker
import com.exponential.app.ui.components.toPickerIssue

/**
 * Issue picker for "Mark as duplicate…" (masterplan §5e): the team's other
 * issues, ranked by the shared engine; picking one sets `duplicateOfId` +
 * status `duplicate` atomically via the issues.update mutation.
 *
 * EXP-1021: the sheet IS the shared `IssuePicker` — the caller ranks (the
 * rows arrive ordered, so `filter = false`) and the primitive renders.
 */
@Composable
fun DuplicatePickerSheet(
    candidates: List<IssueEntity>,
    onPick: (IssueEntity) -> Unit,
    onDismiss: () -> Unit,
    searchServer: (suspend (String) -> List<SearchIssueHit>)? = null,
) {
    var query by remember { mutableStateOf("") }
    val ranked = rememberIssueCandidates(candidates, query, searchServer)
    IssuePicker(
        issues = ranked.map { it.toPickerIssue() },
        // EXP-892: while a query is being typed the best match — the top row —
        // reads as picked, the same contract the `#` menu keeps. The picker's
        // own single mark is what draws it (EXP-1021).
        value = bestMatch(ranked, query),
        onChange = { picked ->
            val id = picked.firstOrNull() ?: return@IssuePicker
            ranked.firstOrNull { it.id == id }?.let(onPick)
        },
        title = "Duplicate of…",
        query = query,
        onQueryChange = { query = it },
        filter = false,
        emptyText = "No matching issues",
        open = true,
        onOpenChange = { open -> if (!open) onDismiss() },
    )
}
