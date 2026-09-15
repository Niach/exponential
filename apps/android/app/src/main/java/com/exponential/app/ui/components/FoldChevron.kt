package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-897: the fold control every nested list wears — a 12dp chevron inside
 * its own 20dp hit area, so a tap on it folds the children while a tap
 * anywhere else on the row still opens what the row is about. The session
 * lists (Running and Recent) and the Reviews stacks share it, and the glyphs
 * plus the two TalkBack names are the same on all four clients.
 */
@Composable
fun FoldChevron(
    expanded: Boolean,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier
            .size(20.dp)
            .clickable(onClick = onToggle)
            .testTag("fold-chevron"),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            if (expanded) ExpIcons.uiChevronDown else ExpIcons.uiChevronRight,
            contentDescription = if (expanded) "Collapse child runs" else "Expand child runs",
            modifier = Modifier.size(12.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}
