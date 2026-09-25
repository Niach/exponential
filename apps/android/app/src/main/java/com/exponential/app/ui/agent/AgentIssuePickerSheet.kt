package com.exponential.app.ui.agent

import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueSearch
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.statusIcon
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.picker.IssuePickerIssue
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItemBody
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.components.picker.issuePickerItems
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.statusColor

// Loose batch caps (desktop parity): a hard 30-issue ceiling, and a soft note
// past 6 that a single Claude session across that many issues burns tokens.
internal const val MAX_BATCH_ISSUES = 30
internal const val LARGE_BATCH_HINT_THRESHOLD = 6

/** Ranked matches the picker lists at most, on top of the checked rows. */
private const val MAX_PICKER_ROWS = 50

/**
 * EXP-825: the composer's `#` tool — the multi-select issue picker the
 * Start-coding sheet's Issues tab used to be. Nothing is submitted here: every
 * toggle chips the issue on the composer at once, and Done just closes. The
 * checked rows stay put on toggle (EXP-241 — re-sorting teleported the tapped
 * row out from under the finger); the validation captions read the same as
 * under the composer so the reason a batch is blocked is visible while picking.
 *
 * EXP-1030: the sheet, the search field and the "this one is picked" paint are
 * the shared [Picker]'s (multi mode), so the batch picker and the relations
 * linker are ONE surface — the circle toggle is gone, a checked row is the
 * primitive's highlight. The row BODY stays this sheet's (priority, mono
 * identifier, status, title: the issue-list anatomy, web parity), which is
 * what `renderItem` exists for.
 */
@Composable
internal fun AgentIssuePickerSheet(
    issues: List<IssueOption>,
    checkedIds: Set<String>,
    onToggle: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var query by remember { mutableStateOf("") }
    // EXP-892: the unchecked pool is ranked by the shared engine; the checked
    // rows pin above it so the current batch is always in view. EXP-241:
    // `checkedIds` is deliberately NOT a key of this remember — the pinning is
    // recomputed when the QUERY changes, never on a toggle, because re-sorting
    // under the finger teleported the tapped row out from under it.
    val rows = remember(issues, query) {
        val pinned = issues.filter { it.id in checkedIds }
        pinned + IssueSearch.rank(issues, query, limit = MAX_PICKER_ROWS, exclude = checkedIds)
    }
    val rowsById = remember(rows) { rows.associateBy { it.id } }
    // The rows come ranked (`filter = false` below), and the contract's own
    // mapper carries the status glyph + tone the body draws.
    val items = remember(rows) {
        issuePickerItems(
            rows.map { option ->
                val anchor = IssueStatus.fromWire(option.status)
                IssuePickerIssue(
                    id = option.id,
                    identifier = option.identifier,
                    title = option.title,
                    icon = statusIcon(anchor),
                    color = statusColor(anchor),
                )
            },
        )
    }
    val checked = issues.filter { it.id in checkedIds }
    val multiRepo = checked.map { it.repositoryId }.toSet().size > 1
    val tooMany = checked.size > MAX_BATCH_ISSUES
    // Validation captions (blocking) + the large-batch soft note.
    val caption = when {
        multiRepo -> "Pick issues from a single repository per run."
        tooMany -> "At most $MAX_BATCH_ISSUES issues per run. Split the batch."
        checked.size > LARGE_BATCH_HINT_THRESHOLD -> "Large batches are token-expensive."
        else -> null
    }

    Picker(
        items = items,
        mode = PickerMode.Multi,
        value = checkedIds,
        // The primitive reports the WHOLE new set; the composer chips one
        // issue at a time, so the row that changed is the difference.
        onChange = { next ->
            ((next - checkedIds) + (checkedIds - next)).firstOrNull()?.let(onToggle)
        },
        search = true,
        searchPlaceholder = "Search issues",
        query = query,
        onQueryChange = { query = it },
        // The caller ranks (EXP-892), so the rows render verbatim.
        filter = false,
        // One wording per state across the clients (iOS AgentIssuePickerSheet).
        emptyText = if (issues.isEmpty()) "No eligible issues to code." else "No matching issues.",
        title = "Issues",
        sheetModifier = Modifier.testTag("agent-composer-issues-picker"),
        // A multi picker never closes on a pick — Done is how a phone leaves it.
        primaryAction = SheetPrimaryAction(label = "Done", onClick = onDismiss),
        caption = if (caption == null) {
            null
        } else {
            {
                Text(
                    caption,
                    style = MaterialTheme.typography.labelSmall,
                    color = if (multiRepo || tooMany) {
                        MaterialTheme.colorScheme.error
                    } else {
                        MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
                    },
                    modifier = Modifier.padding(horizontal = 32.dp, vertical = 4.dp),
                )
            }
        },
        renderItem = { item ->
            val option = rowsById[item.value]
            if (option == null) PickerItemBody(item) else IssueRowBody(option)
        },
        open = true,
        onOpenChange = { open -> if (!open) onDismiss() },
    )
}

// One issue row's body, over the primitive's own highlight and click: the
// issue-list anatomy (EXP-173) — priority icon, mono identifier column, status
// icon, title.
@Composable
private fun RowScope.IssueRowBody(option: IssueOption) {
    val status = IssueStatus.fromWire(option.status)
    val priority = IssuePriority.fromWire(option.priority)
    PriorityIcon(priority, size = 16.dp)
    Spacer(Modifier.width(10.dp))
    Text(
        option.identifier,
        style = MaterialTheme.typography.labelMedium,
        fontFamily = FontFamily.Monospace,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.widthIn(min = 60.dp),
    )
    Spacer(Modifier.width(10.dp))
    StatusIcon(status, size = 16.dp)
    Spacer(Modifier.width(10.dp))
    Text(
        option.title,
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurface,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.weight(1f),
    )
}
