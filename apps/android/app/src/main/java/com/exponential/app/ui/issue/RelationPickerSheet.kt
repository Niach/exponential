package com.exponential.app.ui.issue

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import com.exponential.app.data.api.SearchIssueHit
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.RelationPick
import com.exponential.app.domain.relationPicks
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.components.picker.issuePickerItems
import com.exponential.app.ui.components.toPickerIssue

/**
 * "Add relation" (EXP-736), in two stages inside ONE sheet: pick the KIND of
 * relation, then the issue it points at. Both stages share the sheet so the
 * back-and-forth doesn't remount the bottom sheet — the title is what changes.
 *
 * The pick carries its own direction ("Blocked by" is the inverse side of
 * `blocks`); the ViewModel turns it into the canonical create input, or into
 * the duplicate lockstep write.
 *
 * EXP-1021: both stages are the shared [Picker] at the SAME call site, which
 * is what keeps the one sheet across the stage change — a second `Picker`
 * would be a second `ModalBottomSheet` and the sheet would blink.
 */
@Composable
fun RelationPickerSheet(
    candidates: List<IssueEntity>,
    onPick: (RelationPick, IssueEntity) -> Unit,
    onDismiss: () -> Unit,
    searchServer: (suspend (String) -> List<SearchIssueHit>)? = null,
) {
    var stage by remember { mutableStateOf<RelationPick?>(null) }
    var query by remember { mutableStateOf("") }
    // A stage change closes the primitive's single-mode sheet; the sheet
    // itself is ours (`open = true`), so this flag is how the close that
    // ADVANCES is told apart from the swipe that dismisses.
    var advancing by remember { mutableStateOf(false) }
    val pick = stage
    val ranked = rememberIssueCandidates(candidates, query, searchServer)

    val items = if (pick == null) {
        // The kinds are titled uniquely, so the title IS the row's identity.
        remember { relationPicks.map { PickerItem(value = it.title, label = it.title, icon = it.icon) } }
    } else {
        issuePickerItems(ranked.map { it.toPickerIssue() })
    }

    Picker(
        items = items,
        mode = PickerMode.Single,
        value = emptySet(),
        onChange = { picked ->
            val value = picked.firstOrNull() ?: return@Picker
            if (pick == null) {
                advancing = true
                stage = relationPicks.firstOrNull { it.title == value }
            } else {
                ranked.firstOrNull { it.id == value }?.let { onPick(pick, it) }
            }
        },
        search = pick != null,
        searchPlaceholder = "Search issues",
        query = query,
        onQueryChange = { query = it },
        filter = false,
        emptyText = "No matching issues",
        title = pick?.title ?: "Add relation",
        open = true,
        onOpenChange = { open ->
            if (open) return@Picker
            if (advancing) advancing = false else onDismiss()
        },
    )
}
