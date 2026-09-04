package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.MetaRow
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The issue's relations (EXP-736) inside the properties sheet — mobile's ONLY
 * relations surface, unlike web/desktop where they get a card of their own
 * under the properties band. An "Add relation" row over the current edges,
 * each reading from THIS issue's side ("blocked by EXP-12", not "blocks").
 */
@Composable
fun RelationsSection(
    relations: List<RelationRow>,
    onOpenRelations: () -> Unit,
    onRemoveRelation: (RelationRow) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxWidth()) {
        Text(
            "Relations",
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            // 16dp group gutter + the group's own 4dp inset, so the heading
            // sits over the rows the way the labels heading does.
            modifier = Modifier.padding(horizontal = 20.dp),
        )
        Spacer(Modifier.height(8.dp))
        OptionGroup {
            MetaRow(label = "Add relation", enabled = true, onClick = onOpenRelations) {
                Icon(
                    ExpIcons.relationSection,
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
                Spacer(Modifier.width(6.dp))
                Text(
                    "Link an issue",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
            relations.forEach { relation ->
                GroupDivider()
                RelatedIssueRow(relation = relation, onRemove = { onRemoveRelation(relation) })
            }
        }
    }
}

/** One related issue: its status glyph, the side's label + identifier, the
 *  title, and the remove button. */
@Composable
private fun RelatedIssueRow(
    relation: RelationRow,
    onRemove: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = 16.dp, end = 6.dp, top = 10.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        StatusIcon(relation.otherStatus, size = 16.dp)
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                "${relation.label} · ${relation.otherIdentifier}",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                relation.otherTitle,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Spacer(Modifier.width(8.dp))
        // The 32dp box is the hit target; M3's 48dp minimum is suppressed so
        // the row stays the height of its two lines (RegularCommentRow parity).
        CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides Dp.Unspecified) {
            IconButton(onClick = onRemove, modifier = Modifier.size(32.dp)) {
                Icon(
                    ExpIcons.eventRelationRemoved,
                    contentDescription = "Remove relation to ${relation.otherIdentifier}",
                    modifier = Modifier.size(16.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
            }
        }
    }
}
