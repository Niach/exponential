package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

/**
 * Project list row as a glass pill (iOS Home project row): color dot, name,
 * monospace-ish prefix badge, chevron. Replaces the three duplicated project-row
 * implementations (drawer, home, workspace settings).
 */
@Composable
fun ProjectRow(
    project: ProjectEntity,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val color = remember(project.color) { parseColor(project.color) }
    val icon = remember(project.icon, project.repositoryId) { projectIcon(project) }
    Row(
        modifier = modifier
            .fillMaxWidth()
            .glassRow()
            .clickable(onClick = onClick)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // Project glyph tinted with the project color (replaces the plain dot).
        Icon(
            icon,
            contentDescription = null,
            tint = color,
            modifier = Modifier.size(16.dp),
        )
        Spacer(Modifier.width(12.dp))
        Text(
            project.name,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Spacer(Modifier.width(8.dp))
        Text(
            project.prefix,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Spacer(Modifier.width(8.dp))
        Icon(
            Icons.AutoMirrored.Filled.KeyboardArrowRight,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
            modifier = Modifier.size(16.dp),
        )
    }
}
