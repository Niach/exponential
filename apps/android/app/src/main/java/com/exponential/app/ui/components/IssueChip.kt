package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.MdStyle
import com.exponential.app.ui.markdown.chipTitle

/**
 * The ONE issue badge (EXP-885).
 *
 * Android drew three of them: the chip the markdown painter draws inside a
 * rendered description or feed line (`ui/markdown/IssueRefChips.kt` — the
 * canonical one), the launcher composer's [GlassPill] capsule (identifier
 * only, no title, a 32dp control rung) and the agent feed's tool-result row (a
 * glass row). Same thing, three shapes; and the capsule in particular read as
 * a control rather than as the issue it names.
 *
 * This is that chip as a composable, for every badge OUTSIDE the text painter
 * — which cannot use a composable at all, because a chip inside a paragraph is
 * spans plus a `drawBehind`. The two are kept from drifting by taking the SAME
 * [MdStyle] tokens (corner radius, fill, hairline, glyph size, token colour)
 * and by `IssueChipStyleTest`.
 *
 * The recipe, left to right: the status glyph · the identifier in monospace at
 * [MdStyle.ChipToken] · the issue title in the body colour, one line,
 * ellipsized. [onClick] opens the issue. [onRemove] adds the composer's ✕
 * INSIDE the chip, after the title — the removable variant is a composer's
 * alone; nothing else may hand a badge a control.
 */
@Composable
fun IssueChip(
    identifier: String,
    title: String?,
    status: ResolvedIssueStatus?,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    onRemove: (() -> Unit)? = null,
    /** Names the ✕ for TalkBack; defaults to the removal it performs. */
    removeContentDescription: String = "Remove $identifier",
    removeTestTag: String? = null,
    /**
     * EXP-1014: the glyph slot, for the ONE caller whose leading mark is not
     * the issue's status — a workflow node wears its node state (or its live
     * run's dot) there, so the graph's rows are THE chip rather than a second
     * badge beside one. Takes [status]'s place; never drawn beside it.
     */
    leading: (@Composable () -> Unit)? = null,
) {
    // Web parity through the markdown renderer's own cap: a chip is a badge,
    // not a place to read a sentence.
    val chipText = title?.takeIf { it.isNotBlank() }?.let { remember(it) { chipTitle(it) } }
    ChipShell(modifier = modifier, onClick = onClick) {
        // A chip whose issue has not synced (another team's, a trashed board)
        // still names it: the glyph is the one part that can be missing.
        if (leading != null) {
            leading()
        } else if (status != null) {
            StatusIcon(status, size = MdStyle.chipIconSize)
        }
        // A BLANK identifier is the agent feed's one degenerate case: a tool
        // answered with a title and no code. The chip then names the issue by
        // its title alone rather than reserving an empty mono column.
        if (identifier.isNotBlank()) {
            Text(
                identifier,
                style = IssueChipDefaults.textStyle().copy(fontFamily = FontFamily.Monospace),
                color = MdStyle.ChipToken,
                maxLines = 1,
            )
        }
        if (chipText != null) {
            Text(
                chipText,
                style = IssueChipDefaults.textStyle(),
                color = MdStyle.Text,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                // Deliberately NOT `Modifier.weight`: a Row measures its
                // unweighted children against whatever main-axis space is
                // left, so the title ellipsizes inside a bounded parent and
                // renders whole inside the composer's horizontal scroller —
                // where a weight would have collapsed it to zero width
                // (weights are ignored when the main axis is infinite).
            )
        }
        if (onRemove != null) {
            ChipRemove(
                contentDescription = removeContentDescription,
                testTag = removeTestTag,
                onClick = onRemove,
            )
        }
    }
}

/**
 * EXP-1014: ONE chip that stands for SEVERAL issues — a workflow's compound
 * node (a parent and its sub-issues, run as one batch on one branch). The
 * front chip is the ordinary [IssueChip]; behind it, offset towards the top
 * right in [IssueChipDefaults.StackStep] steps, [ghosts] empty chip outlines
 * say there is more than one issue in there.
 *
 * The ghosts are drawn INSIDE the composable's own bounds (the reserved inset
 * is real padding), so nothing is ever clipped by a scroller or a row that
 * clips its background.
 */
@Composable
fun IssueChipStack(
    modifier: Modifier = Modifier,
    ghosts: Int = 2,
    chip: @Composable () -> Unit,
) {
    val outline = MdStyle.IssueRefBorder
    val radius = MdStyle.chipCornerRadius
    val step = IssueChipDefaults.StackStep
    val inset = step * ghosts
    Box(
        modifier = modifier
            .padding(top = inset, end = inset)
            .drawBehind {
                val stepPx = step.toPx()
                val corner = CornerRadius(radius.toPx())
                // Back to front, so the nearest ghost sits over the far one.
                for (depth in ghosts downTo 1) {
                    drawRoundRect(
                        color = outline,
                        topLeft = Offset(stepPx * depth, -stepPx * depth),
                        size = size,
                        cornerRadius = corner,
                        style = Stroke(IssueChipDefaults.BorderWidth.toPx()),
                    )
                }
            },
    ) {
        chip()
    }
}

/**
 * EXP-920: the chip BOX every badge shares — [IssueChip] and [EntityChip]
 * paint through this one recipe (the painter's corner radius, fill and
 * hairline; the same insets and gap), so an entity chip under a tool row sits
 * pixel-for-pixel beside the issue chip in the line above it.
 */
@Composable
internal fun ChipShell(
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    content: @Composable RowScope.() -> Unit,
) {
    val shape = remember { RoundedCornerShape(MdStyle.chipCornerRadius) }
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(IssueChipDefaults.Spacing),
        modifier = modifier
            .clip(shape)
            .background(MdStyle.IssueRefBg, shape)
            .border(IssueChipDefaults.BorderWidth, MdStyle.IssueRefBorder, shape)
            .then(
                if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier,
            )
            .padding(
                horizontal = IssueChipDefaults.HorizontalPadding,
                vertical = IssueChipDefaults.VerticalPadding,
            ),
        content = content,
    )
}

/** The removable variant's ONE control: a round hit area around the ✕. */
@Composable
private fun ChipRemove(
    contentDescription: String,
    testTag: String?,
    onClick: () -> Unit,
) {
    val describedAs = contentDescription
    Box(
        modifier = Modifier
            .size(IssueChipDefaults.RemoveHitArea)
            .clip(CircleShape)
            .clickable(role = Role.Button, onClick = onClick)
            .then(if (testTag != null) Modifier.testTag(testTag) else Modifier)
            .semantics { this.contentDescription = describedAs },
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            ExpIcons.uiClose,
            contentDescription = null,
            modifier = Modifier.size(IssueChipDefaults.RemoveGlyph),
            tint = MdStyle.ChipToken,
        )
    }
}

/**
 * [IssueChip]'s numbers. The chrome ones are the markdown painter's tokens
 * verbatim — read them from [MdStyle], never re-type them — and the rest is
 * the padding the painter applies through the text layout (a hairline of
 * breathing room round the run) expressed as real insets.
 */
object IssueChipDefaults {
    /** The painter's hairline (`drawChip`'s `1.dp` stroke). */
    val BorderWidth: Dp = 1.dp
    val HorizontalPadding: Dp = 6.dp
    val VerticalPadding: Dp = 3.dp
    val Spacing: Dp = 4.dp

    /** [IssueChipStack]'s offset per ghost chip behind the front one. */
    val StackStep: Dp = 3.dp

    /** The ✕: a 20dp hit area around a 10dp glyph (the composer's rung). */
    val RemoveHitArea: Dp = 20.dp
    val RemoveGlyph: Dp = 10.dp

    /** Medium weight, the small label rung — a badge, never a heading. */
    @Composable
    fun textStyle(): TextStyle =
        MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium)
}
