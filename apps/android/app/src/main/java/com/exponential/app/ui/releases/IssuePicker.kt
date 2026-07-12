package com.exponential.app.ui.releases

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.IssueStatus
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * Shared searchable multi-select issue list for the release sheets (EXP-62):
 * the creation sheet and the detail's add-issues sheet render the same
 * search field + status/identifier/title rows with a trailing check.
 * Selection is hoisted; the search query is internal. A [ColumnScope]
 * extension so a long list can weight-scroll inside the sheet while the
 * confirm button stays pinned below.
 */
@Composable
internal fun ColumnScope.IssueMultiSelectPicker(
    candidates: List<IssueEntity>,
    selected: Set<String>,
    onToggle: (String) -> Unit,
) {
    var query by remember { mutableStateOf("") }

    val filtered = remember(candidates, query) {
        val q = query.trim()
        if (q.isEmpty()) {
            candidates
        } else {
            candidates.filter {
                it.title.contains(q, ignoreCase = true) ||
                    it.identifier.contains(q, ignoreCase = true)
            }
        }
    }

    TextField(
        value = query,
        onValueChange = { query = it },
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp),
        placeholder = {
            Text(
                "Search issues",
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        },
        leadingIcon = {
            Icon(
                Icons.Filled.Search,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        },
        singleLine = true,
        shape = RoundedCornerShape(12.dp),
        colors = TextFieldDefaults.colors(
            focusedContainerColor = GlassTokens.RowFill,
            unfocusedContainerColor = GlassTokens.RowFill,
            disabledContainerColor = GlassTokens.RowFill,
            focusedIndicatorColor = Color.Transparent,
            unfocusedIndicatorColor = Color.Transparent,
            disabledIndicatorColor = Color.Transparent,
        ),
    )
    Spacer(Modifier.height(4.dp))
    if (filtered.isEmpty()) {
        Text(
            "No matching issues",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.padding(horizontal = 24.dp, vertical = 16.dp),
        )
    } else {
        // fill=false: few candidates keep the sheet short; many scroll
        // within the sheet with the confirm button pinned below.
        LazyColumn(modifier = Modifier.weight(1f, fill = false)) {
            items(filtered, key = { it.id }) { issue ->
                val isSelected = issue.id in selected
                ListItem(
                    leadingContent = {
                        StatusIcon(IssueStatus.fromWire(issue.status), size = 16.dp)
                    },
                    overlineContent = {
                        Text(
                            issue.identifier,
                            fontFamily = FontFamily.Monospace,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    },
                    headlineContent = {
                        Text(issue.title, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    },
                    trailingContent = {
                        if (isSelected) {
                            Icon(
                                Icons.Filled.Check,
                                contentDescription = null,
                                tint = MaterialTheme.colorScheme.primary,
                            )
                        }
                    },
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { onToggle(issue.id) },
                )
            }
        }
    }
}
