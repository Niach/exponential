package com.exponential.app.ui.drafts

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.DraftRow
import com.exponential.app.domain.IssueDraftPage
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.compactRelativeTime
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/**
 * One unsent draft (EXP-878) as a FLAT list row (EXP-818). EXP-1647: web
 * `DraftsList`'s phone row ×4 — ONE line: the resolved status glyph, the
 * draft's title (or a muted italic "Untitled draft" — a draft with only a
 * description is still worth showing) and how long ago it was touched,
 * right-aligned. No buttons on the row: Discard lives in its long-press menu,
 * as web's lives in its context Menu.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun DraftListRow(
    row: DraftRow,
    onOpen: () -> Unit,
    onDelete: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var menuOpen by remember { mutableStateOf(false) }
    val muted = DesignTokens.Palette.MutedForeground
    Box(modifier = modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .testTag("draft-row-${row.draft.id}")
                .flatRow()
                .combinedClickable(onClick = onOpen, onLongClick = { menuOpen = true })
                // web ListRow: p-3, gap-3.
                .padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            StatusIcon(row.status, size = 16.dp)
            Spacer(Modifier.width(12.dp))
            val hasTitle = row.draft.title.isNotBlank()
            Text(
                if (hasTitle) row.draft.title else IssueDraftPage.UNTITLED,
                style = MaterialTheme.typography.bodyMedium.copy(fontSize = 14.sp),
                color = if (hasTitle) {
                    MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Primary)
                } else {
                    muted
                },
                fontStyle = if (hasTitle) FontStyle.Normal else FontStyle.Italic,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(12.dp))
            // The board chip ×4 (web's drafts row): its glyph + name.
            Row(
                modifier = Modifier.widthIn(max = 140.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                BoardIcon(row.board, size = 12.dp)
                Text(
                    row.board.name,
                    style = MaterialTheme.typography.labelMedium.copy(fontSize = 12.sp),
                    color = muted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Spacer(Modifier.width(12.dp))
            Text(
                compactRelativeTime(row.draft.updatedAt),
                style = MaterialTheme.typography.labelMedium.copy(
                    fontSize = 12.sp,
                    fontFeatureSettings = "tnum",
                ),
                color = muted,
                maxLines = 1,
            )
        }
        GlassDropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
            GlassMenuItem(
                text = { Text("Discard draft") },
                onClick = {
                    menuOpen = false
                    onDelete()
                },
                leadingIcon = { Icon(ExpIcons.uiDelete, contentDescription = null) },
                destructive = true,
            )
        }
    }
}
