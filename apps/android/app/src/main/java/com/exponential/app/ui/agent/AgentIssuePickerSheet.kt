package com.exponential.app.ui.agent

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetSearchField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

// Loose batch caps (desktop parity): a hard 30-issue ceiling, and a soft note
// past 6 that a single Claude session across that many issues burns tokens.
internal const val MAX_BATCH_ISSUES = 30
internal const val LARGE_BATCH_HINT_THRESHOLD = 6

/**
 * EXP-825: the composer's `#` tool — the multi-select issue picker the
 * Start-coding sheet's Issues tab used to be (search row + hairline-divided
 * check rows in one grouped card, EXP-211). Nothing is submitted here: every
 * toggle chips the issue on the composer at once, and Done just closes. The
 * checked rows stay put on toggle (EXP-241 — re-sorting teleported the tapped
 * row out from under the finger); the validation captions read the same as
 * under the composer so the reason a batch is blocked is visible while picking.
 */
@Composable
internal fun AgentIssuePickerSheet(
    issues: List<IssueOption>,
    checkedIds: Set<String>,
    onToggle: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var query by remember { mutableStateOf("") }
    val rows = remember(issues, query) {
        val q = query.trim()
        issues.asSequence()
            .filter {
                q.isEmpty() ||
                    it.identifier.contains(q, ignoreCase = true) ||
                    it.title.contains(q, ignoreCase = true)
            }
            .take(50)
            .toList()
    }
    val checked = issues.filter { it.id in checkedIds }
    val multiRepo = checked.map { it.repositoryId }.toSet().size > 1
    val tooMany = checked.size > MAX_BATCH_ISSUES

    GlassSheet(
        title = "Issues",
        onDismiss = onDismiss,
        modifier = Modifier.testTag("agent-composer-issues-picker"),
        height = SheetHeight.Full,
        primaryAction = SheetPrimaryAction(label = "Done", onClick = onDismiss),
    ) {
        Column(modifier = Modifier.fillMaxSize()) {
            OptionGroup {
                // EXP-698: the shared field, chrome-less inside the group.
                GlassSheetSearchField(
                    value = query,
                    onValueChange = { query = it },
                    placeholder = "Search issues",
                )
                GroupDivider()
                if (rows.isEmpty()) {
                    Text(
                        // One wording per state across the clients (iOS
                        // AgentIssuePickerSheet).
                        if (issues.isEmpty()) "No eligible issues to code." else "No matching issues.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                    )
                } else {
                    LazyColumn(modifier = Modifier.fillMaxWidth().weight(1f, fill = false)) {
                        itemsIndexed(rows, key = { _, option -> option.id }) { index, option ->
                            Column(modifier = Modifier.animateItem()) {
                                if (index > 0) GroupDivider()
                                IssueCheckRow(
                                    option = option,
                                    checked = option.id in checkedIds,
                                    onToggle = { onToggle(option.id) },
                                )
                            }
                        }
                    }
                }
            }
            // Validation captions (blocking) + the large-batch soft note.
            val caption = when {
                multiRepo -> "Pick issues from a single repository per run."
                tooMany -> "At most $MAX_BATCH_ISSUES issues per run. Split the batch."
                checked.size > LARGE_BATCH_HINT_THRESHOLD -> "Large batches are token-expensive."
                else -> null
            }
            if (caption != null) {
                Spacer(Modifier.height(4.dp))
                Text(
                    caption,
                    style = MaterialTheme.typography.labelSmall,
                    color = if (multiRepo || tooMany) {
                        MaterialTheme.colorScheme.error
                    } else {
                        MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
                    },
                    modifier = Modifier.padding(horizontal = 32.dp),
                )
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

// One checkable issue, styled like the regular issue-list row (EXP-173):
// circle toggle icon (RepoRow's selection affordance — EXP-208), priority
// icon, mono identifier column, status icon, title.
@Composable
private fun IssueCheckRow(
    option: IssueOption,
    checked: Boolean,
    onToggle: () -> Unit,
) {
    val status = IssueStatus.fromWire(option.status)
    val priority = IssuePriority.fromWire(option.priority)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onToggle)
            // Selected rows tint so the state is unmissable (EXP-241) —
            // the icon swap alone was easy to overlook.
            .background(
                if (checked) MaterialTheme.colorScheme.primary.copy(alpha = 0.12f) else Color.Transparent,
            )
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            if (checked) ExpIcons.uiSelected else ExpIcons.uiUnselected,
            contentDescription = null,
            modifier = Modifier.size(20.dp),
            tint = if (checked) {
                MaterialTheme.colorScheme.primary
            } else {
                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
            },
        )
        Spacer(Modifier.width(10.dp))
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
}
