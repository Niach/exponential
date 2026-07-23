package com.exponential.app.ui.issue

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
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
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.IssueStatus
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetSearchField
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.theme.TextEmphasis

/**
 * Issue picker for "Mark as duplicate…" (masterplan §5e): searchable list of
 * the team's other issues; picking one sets `duplicateOfId` + status
 * `duplicate` atomically via the issues.update mutation. Glass chrome (EXP-240).
 */
@Composable
fun DuplicatePickerSheet(
    candidates: List<IssueEntity>,
    onPick: (IssueEntity) -> Unit,
    onDismiss: () -> Unit,
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

    GlassSheet(title = "Duplicate of…", onDismiss = onDismiss) {
        GlassSheetSearchField(
            value = query,
            onValueChange = { query = it },
            placeholder = "Search issues",
        )
        Spacer(Modifier.height(4.dp))
        if (filtered.isEmpty()) {
            Text(
                "No matching issues",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 16.dp),
            )
        } else {
            LazyColumn(modifier = Modifier.heightIn(max = 480.dp)) {
                items(filtered, key = { it.id }) { issue ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable {
                                onPick(issue)
                                onDismiss()
                            }
                            .padding(horizontal = 20.dp, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        StatusIcon(IssueStatus.fromWire(issue.status), size = 16.dp)
                        Spacer(Modifier.width(12.dp))
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                issue.identifier,
                                style = MaterialTheme.typography.labelSmall,
                                fontFamily = FontFamily.Monospace,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            )
                            Text(
                                issue.title,
                                style = MaterialTheme.typography.bodyMedium,
                                color = Color.White.copy(alpha = 0.9f),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                }
            }
        }
    }
}
