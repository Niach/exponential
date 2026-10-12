package com.exponential.app.ui.components

import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons
import at.exponential.ui.primitives.EmptyStateSize
import at.exponential.ui.primitives.EmptyStateView
import com.exponential.app.domain.EmptyStateCopy
import com.exponential.app.ui.theme.GlassTokens
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
 * Reviews); [trailing] is pushed to the far edge. [loading] (EXP-1274, web
 * `GlassSectionHeader loading`) spins the 12dp loading glyph before it while an
 * already-loaded list refetches.
 */
@Composable
fun SectionHeader(
    title: String,
    modifier: Modifier = Modifier,
    leading: (@Composable () -> Unit)? = null,
    loading: Boolean = false,
    trailing: (@Composable () -> Unit)? = null,
) {
    SectionBand(
        // The band's 4dp breathing room over its rows (web `mb-1`) — OUTSIDE
        // the fill, so the strip itself stays tight around its title.
        modifier = modifier.padding(bottom = 4.dp),
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
            // ONE weight, filling: the title owns every dp the leading and
            // trailing slots leave, so a long title ellipsizes and `trailing`
            // sits flush at the far edge. (A `fill = false` title plus a
            // weighted spacer split the free width between the two, which
            // parked trailing content mid-row, EXP-886.)
            modifier = Modifier.weight(1f),
        )
        if (loading) SectionBandSpinner()
        trailing?.invoke()
    }
}

/**
 * The band's loading glyph (EXP-1274, web `GlassSectionHeader loading`):
 * EXP-1275's INLINE spinner, the one primitive every band and caption shares.
 */
@Composable
private fun SectionBandSpinner() {
    InlineSpinner(modifier = Modifier.testTag("section-band-loading"))
}

/**
 * THE band strip (EXP-818 `Modifier.glassSectionBand` paint + its layout): the
 * ONE composable that paints a group band. [SectionHeader] is its titled form;
 * a band whose content is not a plain title (a diff file card's header, the
 * diff tree's summary, the empty "Add sub-issues" band) fills the slot.
 * [onClick] makes the whole strip the tap target; [loading] spins the 12dp
 * loading glyph at its trailing edge (EXP-1274).
 */
@Composable
fun SectionBand(
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    loading: Boolean = false,
    contentPadding: PaddingValues = PaddingValues(horizontal = 12.dp, vertical = 6.dp),
    horizontalArrangement: Arrangement.Horizontal = Arrangement.Start,
    content: @Composable RowScope.() -> Unit,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .glassSectionBand()
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(contentPadding),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = horizontalArrangement,
    ) {
        content()
        if (loading) {
            Spacer(Modifier.weight(1f))
            SectionBandSpinner()
        }
    }
}

/**
 * EXP-1248: THE issue status group band (web `IssueGroupBand`, desktop
 * `issue_group_band`, iOS `IssueGroupBand`) — the ONE list band that keeps a
 * count: fold chevron · status glyph · name · count, the whole strip toggling
 * its group. P35: the phones ×3 draw it as THE filled band (iOS
 * `GlassSectionBand`, `glassSectionBand()` here), a plain scrolling item over
 * its rows. [gutter] is the OUTER inset that lines the band's edges up with
 * the rows below (the board list carries its gutter in the content, My Issues
 * in the list).
 */
@Composable
fun IssueGroupBand(
    glyph: @Composable () -> Unit,
    name: String,
    /** The group's full size. */
    count: Int,
    collapsed: Boolean,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier,
    gutter: Dp = 0.dp,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = gutter)
            .padding(bottom = 4.dp)
            .glassSectionBand()
            .clickable(onClick = onToggle)
            .testTag("issue-group-band")
            .padding(horizontal = 8.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            if (collapsed) ExpIcons.uiChevronRight else ExpIcons.uiChevronDown,
            contentDescription = if (collapsed) "Expand" else "Collapse",
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Spacer(Modifier.width(6.dp))
        glyph()
        Spacer(Modifier.width(8.dp))
        Text(
            name,
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
        Spacer(Modifier.width(8.dp))
        Text(
            count.toString(),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            modifier = Modifier.testTag("issue-group-band-count"),
        )
    }
}

/** The band title's emphasis — web `text-foreground/85`, desktop
 *  `foreground.opacity(0.85)`. */
private const val SectionBandTitleAlpha = 0.85f

/**
 * THE empty state (EXP-1274, `empty-state.json` ×4), drawn by the SDK's
 * `EmptyStateView`. [EmptyStateSize.Page] = a 48dp `RowFillActive` disc with a
 * 24dp secondary glyph, a `titleMedium` title, a `bodySmall` tertiary
 * description and the [action] slot (callers pass a primary `GlassPill(PillSize.Md)`
 * named by the copy's `action`), filling its container. [EmptyStateSize.List]
 * = a bare 20dp tertiary glyph over ONE `bodyMedium` secondary line, 24dp
 * vertical padding, full width — never a description or a button.
 */
@Composable
fun EmptyState(
    copy: EmptyStateCopy.State,
    modifier: Modifier = Modifier,
    size: EmptyStateSize? = null,
    /** Overrides the copy's description (an error line in its place). */
    description: String? = copy.description,
    action: (@Composable () -> Unit)? = null,
) {
    EmptyState(
        title = copy.title,
        modifier = modifier,
        description = description,
        icon = copy.icon,
        size = size ?: when (copy.size) {
            EmptyStateCopy.Size.Page -> EmptyStateSize.Page
            EmptyStateCopy.Size.List -> EmptyStateSize.List
        },
        action = action,
    )
}

/** [EmptyState] for inline copy no other client draws. */
@Composable
fun EmptyState(
    title: String,
    modifier: Modifier = Modifier,
    description: String? = null,
    icon: ImageVector? = null,
    size: EmptyStateSize = EmptyStateSize.Page,
    action: (@Composable () -> Unit)? = null,
) {
    val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    when (size) {
        EmptyStateSize.Page -> EmptyStateView(
            title = title,
            modifier = modifier.fillMaxSize(),
            size = EmptyStateSize.Page,
            description = description,
            icon = icon?.let {
                { Icon(it, contentDescription = null, tint = secondary, modifier = Modifier.size(24.dp)) }
            },
            action = action,
            iconDisc = GlassTokens.RowFillActive,
            iconDiscSize = 48.dp,
            iconColor = secondary,
            titleStyle = MaterialTheme.typography.titleMedium,
            titleColor = MaterialTheme.colorScheme.onSurface,
            descriptionStyle = MaterialTheme.typography.bodySmall,
            descriptionColor = tertiary,
            descriptionPadding = 16.dp,
            spacing = 12.dp,
            contentPadding = PaddingValues(horizontal = 24.dp, vertical = 48.dp),
        )
        EmptyStateSize.List -> EmptyStateView(
            title = title,
            modifier = modifier.fillMaxWidth(),
            size = EmptyStateSize.List,
            icon = icon?.let {
                { Icon(it, contentDescription = null, tint = tertiary, modifier = Modifier.size(20.dp)) }
            },
            iconDisc = null,
            iconColor = tertiary,
            titleStyle = MaterialTheme.typography.bodyMedium,
            titleColor = secondary,
            spacing = 6.dp,
            contentPadding = PaddingValues(horizontal = 16.dp, vertical = 24.dp),
        )
    }
}
