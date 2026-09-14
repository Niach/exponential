package com.exponential.app.ui.drafts

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.DraftRow
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/**
 * One unsent draft (EXP-878) as a FLAT list row (EXP-818): the resolved status
 * glyph, the draft's title (or a muted "Untitled draft" — a draft with only a
 * description is still worth showing), and the board it will be filed on beside
 * how long ago it was touched. Delete is always visible: there is no detail
 * screen to remove it from, and a draft is the one row a swipe would be wrong
 * for (it is not an issue yet).
 */
@Composable
fun DraftListRow(
    row: DraftRow,
    onOpen: () -> Unit,
    onDelete: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .flatRow()
            .clickable(onClick = onOpen)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        StatusIcon(row.status, size = 14.dp)
        Spacer(Modifier.width(10.dp))
        Column(modifier = Modifier.weight(1f)) {
            val hasTitle = row.draft.title.isNotBlank()
            Text(
                if (hasTitle) row.draft.title else "Untitled draft",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(
                    alpha = if (hasTitle) TextEmphasis.Primary else TextEmphasis.Tertiary,
                ),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                "${row.board.name} · ${relativeTime(row.draft.updatedAt)}",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        IconButton(onClick = onDelete) {
            Icon(
                ExpIcons.uiDelete,
                contentDescription = "Delete draft",
                modifier = Modifier.size(16.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        }
    }
}
