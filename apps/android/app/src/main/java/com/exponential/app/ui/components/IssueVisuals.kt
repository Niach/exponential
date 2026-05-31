package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.priorityIcon
import com.exponential.app.domain.statusIcon
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.priorityColor
import com.exponential.app.ui.theme.statusColor

/** Status glyph tinted by its semantic color (iOS colored status icon). */
@Composable
fun StatusIcon(status: IssueStatus, modifier: Modifier = Modifier, size: Dp = 16.dp) {
    Icon(
        statusIcon(status),
        contentDescription = status.label,
        tint = statusColor(status),
        modifier = modifier.size(size),
    )
}

/** Priority glyph tinted by its semantic color (iOS colored priority icon). */
@Composable
fun PriorityIcon(priority: IssuePriority, modifier: Modifier = Modifier, size: Dp = 16.dp) {
    Icon(
        priorityIcon(priority),
        contentDescription = priority.label,
        tint = priorityColor(priority),
        modifier = modifier.size(size),
    )
}

/** Bare colored label dot used in list rows (iOS shows up to three dots). */
@Composable
fun LabelDot(color: Color, modifier: Modifier = Modifier, size: Dp = 8.dp) {
    androidx.compose.foundation.layout.Box(modifier.size(size).background(color, CircleShape))
}

/** Dot + name pill used in the filter sheet and issue detail. */
@Composable
fun LabelChip(label: LabelEntity, modifier: Modifier = Modifier) {
    val color = remember(label.color) { parseColor(label.color) }
    Row(
        modifier = modifier
            .background(color.copy(alpha = 0.18f), RoundedCornerShape(6.dp))
            .padding(horizontal = 6.dp, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        LabelDot(color)
        Spacer(Modifier.width(4.dp))
        Text(
            label.name,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface,
        )
    }
}
