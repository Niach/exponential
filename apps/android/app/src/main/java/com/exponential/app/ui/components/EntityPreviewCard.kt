package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/**
 * EXP-920: one row a preview card lists under its facts — a board's open
 * issues, a list chip's members, the issue a comment sits on. [icon] is a
 * slot (a status glyph, a session dot, a concept glyph); [onClick] makes it
 * a row that goes somewhere.
 */
class EntityPreviewRow(
    val icon: @Composable () -> Unit,
    val primary: String,
    val secondary: String? = null,
    val onClick: (() -> Unit)? = null,
    /** Draw [primary] in the identifier style (mono, muted) — an issue row. */
    val monoPrimary: Boolean = false,
)

/**
 * EXP-920: the PRESENTATIONAL preview card every entity kind renders into —
 * the phone's twin of the web hover card / the desktop popover. A header
 * (glyph, eyebrow = the kind noun, title, subtitle), an optional excerpt
 * [body] that unfolds on tap, a wrapping row of [facts] pills, a list of
 * [rows], a trailing [more] note, and the ONE navigation: an "Open" row
 * ([onOpen]) that lands on the entity's detail screen.
 *
 * It knows nothing about refs or the database; [EntityRefPreviewSheet]
 * resolves a ref into these slots.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun EntityPreviewCard(
    icon: @Composable () -> Unit,
    eyebrow: String,
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    body: String? = null,
    facts: (@Composable () -> Unit)? = null,
    rows: List<EntityPreviewRow> = emptyList(),
    more: String? = null,
    onOpen: (() -> Unit)? = null,
) {
    Column(modifier = modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(28.dp), contentAlignment = Alignment.Center) { icon() }
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    eyebrow,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                )
                Text(
                    title,
                    style = MaterialTheme.typography.bodyLarge,
                    fontWeight = FontWeight.Medium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                if (subtitle != null) {
                    Text(
                        subtitle,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
        if (body != null) {
            var expanded by remember(body) { mutableStateOf(false) }
            Text(
                body,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                maxLines = if (expanded) Int.MAX_VALUE else 4,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 10.dp)
                    .clickable { expanded = !expanded },
            )
        }
        if (facts != null) {
            FlowRow(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                facts()
            }
        }
        if (rows.isNotEmpty() || more != null || onOpen != null) {
            Spacer(Modifier.height(4.dp))
        }
        rows.forEach { row ->
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 8.dp)
                    // EXP-818: an overlay row is a LIST row.
                    .flatRow()
                    .then(
                        if (row.onClick != null) Modifier.clickable(onClick = row.onClick) else Modifier,
                    )
                    .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(Modifier.size(16.dp), contentAlignment = Alignment.Center) { row.icon() }
                Spacer(Modifier.width(10.dp))
                if (row.monoPrimary) {
                    Text(
                        row.primary,
                        style = MaterialTheme.typography.labelMedium,
                        fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                    )
                    if (row.secondary != null) Spacer(Modifier.width(8.dp))
                } else {
                    Text(
                        row.primary,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                    if (row.secondary != null) Spacer(Modifier.width(8.dp))
                }
                if (row.secondary != null) {
                    Text(
                        row.secondary,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = if (row.monoPrimary) TextEmphasis.Primary else TextEmphasis.Tertiary,
                        ),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                }
            }
        }
        if (more != null) {
            Text(
                more,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
            )
        }
        if (onOpen != null) {
            GlassSheetRow(
                label = "Open",
                onClick = onOpen,
                leading = {
                    Icon(
                        ExpIcons.uiArrowRight,
                        contentDescription = null,
                        modifier = Modifier.size(16.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                },
                trailing = {
                    Icon(
                        ExpIcons.uiChevronRight,
                        contentDescription = null,
                        modifier = Modifier.size(16.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
                    )
                },
            )
        }
    }
}

/**
 * The card in a [GlassSheet] — the phone's hover card. [title] = the kind noun
 * capitalized, or the identifier when the entity has one; the card's own
 * scroller, since the sheet never scrolls its content itself.
 */
@Composable
fun EntityPreviewSheet(
    title: String,
    onDismiss: () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    GlassSheet(title = title, onDismiss = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState()),
        ) {
            content()
            Spacer(Modifier.height(8.dp))
        }
    }
}
