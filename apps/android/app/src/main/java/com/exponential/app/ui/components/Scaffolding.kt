package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassSectionBand

/**
 * THE section header (EXP-698) — every list, sheet and settings section in the
 * app renders this one: sentence-case title at secondary emphasis, an optional
 * [count] right after it, and an optional [trailing] control pushed to the far
 * edge (a "New action" pill). Three near-identical private copies (a
 * `labelLarge` in AgentsScreen, a `bodyMedium` row in ActionsScreen, a padded
 * `SectionLabel` in the sheets) collapsed into it. Signature and layout are
 * iOS's `GlassSectionHeader(title, trailing:)` — no count slot anywhere (EXP-698
 * retired the header counts on every client). The emoji
 * picker's uppercase category headers are the one documented exception (a
 * cross-client convention, not this app's section language).
 *
 * EXP-818: it is a filled BAND now (`Modifier.glassSectionBand`, the web
 * `GlassSectionHeader` / desktop `glass_section_band` twin) rather than a bare
 * label in a 4dp gutter: a group reads as a highlighted strip with its flat
 * rows ([com.exponential.app.ui.theme.flatRow]) under it, which is what turned
 * every list from a stack of cards into a table. A caller INSIDE a sheet insets
 * the band itself (the 16dp it passes) so it lines up with `OptionGroup`'s edge.
 *
 * An optional [leading] glyph sits before the title (the board icon on
 * Reviews); [trailing] is pushed to the far edge.
 */
@Composable
fun SectionHeader(
    title: String,
    modifier: Modifier = Modifier,
    leading: (@Composable () -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            // The band's 4dp breathing room over its rows (web `mb-1`) — OUTSIDE
            // the fill, so the strip itself stays tight around its title.
            .padding(bottom = 4.dp)
            .glassSectionBand()
            .padding(horizontal = 12.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        leading?.invoke()
        Text(
            title,
            style = MaterialTheme.typography.labelLarge,
            // The band's own fill carries the emphasis, so the title reads at
            // the web band's 85% rather than the old bare label's 70%.
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = SectionBandTitleAlpha),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
        Spacer(Modifier.weight(1f))
        trailing?.invoke()
    }
}

/** The band title's emphasis — web `text-foreground/85`, desktop
 *  `foreground.opacity(0.85)`. */
private const val SectionBandTitleAlpha = 0.85f

/** Centered empty-state with optional icon + message + detail line (replaces 4 ad-hoc copies). */
@Composable
fun EmptyState(
    message: String,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    detail: String? = null,
    action: (@Composable () -> Unit)? = null,
) {
    Box(
        modifier = modifier
            .fillMaxSize()
            .padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (icon != null) {
                Icon(
                    icon,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier.size(28.dp),
                )
            }
            Text(
                message,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                textAlign = TextAlign.Center,
            )
            if (detail != null) {
                Text(
                    detail,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    textAlign = TextAlign.Center,
                    modifier = Modifier.padding(horizontal = 16.dp),
                )
            }
            action?.invoke()
        }
    }
}

/** Centered spinner loading state. */
@Composable
fun LoadingState(modifier: Modifier = Modifier) {
    Box(
        modifier = modifier.fillMaxSize(),
        contentAlignment = Alignment.Center,
    ) {
        CircularProgressIndicator(color = MaterialTheme.colorScheme.onSurface)
    }
}
