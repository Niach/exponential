package com.exponential.app.ui.reviews

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.TreeGuide
import com.exponential.app.domain.TreeGuides
import com.exponential.app.ui.components.treeGuides
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/**
 * SLOP-16 r3: THE pull-request row — the Reviews list's stack row, lifted out
 * so the Work screen's "Related work" view lists its pull requests with the
 * very same row. A flat row nested [depth] levels (14dp each, EXP-897) with
 * the stack's connector in the gutter the indent leaves (EXP-965): the green
 * PR glyph, the batch mark on a batch, the mono [label] (`#n`, else the
 * identifier) and the [title]. [details] adds lines under the identity line,
 * [trailing] the row's controls, [footer] what captions the row below it.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun PrStackRow(
    isBatch: Boolean,
    label: String,
    title: String,
    depth: Int,
    guide: TreeGuide?,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    /** The list's own row spacing, which the connector spans. */
    gap: Dp = 0.dp,
    onLongClick: (() -> Unit)? = null,
    /** A tap on the batch mark (Reviews: the batch's issues); null = inert. */
    onBatchMark: (() -> Unit)? = null,
    details: @Composable ColumnScope.() -> Unit = {},
    trailing: @Composable RowScope.() -> Unit = {},
    footer: @Composable ColumnScope.() -> Unit = {},
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .treeGuides(guide, gap)
            .padding(start = (TreeGuides.INDENT_DP * depth).dp),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .flatRow()
                .combinedClickable(onClick = onClick, onLongClick = onLongClick)
                .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // PR glyph — green like the iOS/web review rows (EXP-248).
            Icon(
                ExpIcons.prOpen,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = DesignTokens.Semantic.Green,
            )
            Spacer(Modifier.width(10.dp))
            Column(modifier = Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (isBatch) {
                        // The batch mark (EXP-897), in its own hit area so the
                        // row's own tap still opens the review.
                        Box(
                            modifier = Modifier
                                .size(24.dp)
                                .then(if (onBatchMark != null) Modifier.clickable(onClick = onBatchMark) else Modifier)
                                .testTag("review-batch-glyph"),
                            contentAlignment = Alignment.Center,
                        ) {
                            Icon(
                                ExpIcons.prBatch,
                                contentDescription = "Issues in this batch",
                                modifier = Modifier.size(14.dp),
                                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            )
                        }
                        Spacer(Modifier.width(4.dp))
                    }
                    Text(
                        label,
                        style = MaterialTheme.typography.labelMedium,
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(
                        title,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                }
                details()
            }
            Spacer(Modifier.width(6.dp))
            trailing()
        }
        footer()
    }
}

/** The PR's state as one coloured word — open green, merged blue, else muted. */
@Composable
internal fun PrStatePill(state: String?) {
    val label = when (state) {
        DomainContract.prStateOpen -> "Open"
        DomainContract.prStateMerged -> "Merged"
        DomainContract.prStateClosed -> "Closed"
        DomainContract.prStateDraft -> "Draft"
        else -> return
    }
    val tint: Color = when (state) {
        DomainContract.prStateOpen -> DesignTokens.Semantic.Green
        DomainContract.prStateMerged -> DoneBlue
        else -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    }
    Text(
        label,
        style = MaterialTheme.typography.labelSmall,
        color = tint,
        maxLines = 1,
    )
}
