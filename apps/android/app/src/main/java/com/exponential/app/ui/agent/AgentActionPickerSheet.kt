package com.exponential.app.ui.agent

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.ActionDto
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetSearchField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.components.actionGlyph
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-825: the composer's ▶ tool — a SINGLE-select action list (the
 * Start-coding sheet's Actions tab, EXP-257): "Fix merge conflicts" first,
 * then "Create action" (listed here now that the create sheet is gone — the
 * creator run is just another action to pick), then the team's rows in server
 * order; the hidden Chat row never (it is what "no subject" means). A tap
 * picks and closes; picking an action while issues are chipped SWAPS them out.
 */
@Composable
internal fun AgentActionPickerSheet(
    actions: List<ActionDto>?,
    error: String?,
    selectedId: String?,
    onPick: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var query by remember { mutableStateOf("") }
    val rows = remember(actions, query) {
        val q = query.trim()
        actions?.filter {
            q.isEmpty() ||
                it.name.contains(q, ignoreCase = true) ||
                it.description?.contains(q, ignoreCase = true) == true
        }
    }
    GlassSheet(
        title = "Actions",
        onDismiss = onDismiss,
        modifier = Modifier.testTag("agent-composer-actions-picker"),
        height = SheetHeight.Full,
    ) {
        Column(modifier = Modifier.fillMaxSize()) {
            OptionGroup {
                GlassSheetSearchField(
                    value = query,
                    onValueChange = { query = it },
                    placeholder = "Search actions",
                )
                GroupDivider()
                when {
                    rows == null && error != null -> Text(
                        error,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.error,
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                    )
                    rows == null -> Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                    ) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(14.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        Text(
                            "Loading actions…",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        )
                    }
                    rows.isEmpty() -> Text(
                        "No matching actions.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                    )
                    else -> LazyColumn(modifier = Modifier.fillMaxWidth().weight(1f, fill = false)) {
                        itemsIndexed(rows, key = { _, action -> action.id }) { index, action ->
                            Column {
                                if (index > 0) GroupDivider()
                                ActionSelectRow(
                                    action = action,
                                    selected = action.id == selectedId,
                                    onSelect = {
                                        onPick(action.id)
                                        onDismiss()
                                    },
                                )
                            }
                        }
                    }
                }
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

// One selectable action (single-select, the issue row's affordances): the
// circle/check indicator, the action's own curated glyph (EXP-721 — one
// resolver on every surface), name (+ a small repo indicator when the action
// clones a repository), and the optional description.
@Composable
private fun ActionSelectRow(
    action: ActionDto,
    selected: Boolean,
    onSelect: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onSelect)
            .background(
                if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.12f) else Color.Transparent,
            )
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            if (selected) ExpIcons.uiSelected else ExpIcons.uiUnselected,
            contentDescription = null,
            modifier = Modifier.size(20.dp),
            tint = if (selected) {
                MaterialTheme.colorScheme.primary
            } else {
                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
            },
        )
        Spacer(Modifier.width(10.dp))
        Icon(
            actionGlyph(action),
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(10.dp))
        Column(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    action.name,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (action.repositoryId != null) {
                    Spacer(Modifier.width(6.dp))
                    Icon(
                        ExpIcons.actionRepository,
                        contentDescription = "Runs in a repository",
                        modifier = Modifier.size(12.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            }
            val description = action.description
            if (!description.isNullOrBlank()) {
                Text(
                    description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}
