package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

/**
 * Board list row as a glass pill (iOS Home board row): color dot, name,
 * monospace-ish prefix badge, chevron. Replaces the three duplicated board-row
 * implementations (drawer, home, team settings).
 */
@Composable
fun BoardRow(
    board: BoardEntity,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    /** The board currently on screen, in a switcher — the shared active row
     *  paint (EXP-698 r5), never a tick beside the name. */
    active: Boolean = false,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .glassRow(active = active)
            .clickable(onClick = onClick)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // Board glyph tinted with the board color (replaces the plain dot).
        BoardIcon(board)
        Spacer(Modifier.width(12.dp))
        Text(
            board.name,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Spacer(Modifier.width(8.dp))
        Text(
            board.prefix,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Spacer(Modifier.width(8.dp))
        Icon(
            ExpIcons.uiChevronRight,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
            modifier = Modifier.size(16.dp),
        )
    }
}
