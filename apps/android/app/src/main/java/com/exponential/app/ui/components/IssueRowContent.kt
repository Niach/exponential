package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.TextEmphasis

/**
 * THE issue row's content, in slots (wave D, M31): every list that shows an
 * issue (or a PR standing for one) lays its row out here — the board list
 * ([com.exponential.app.ui.issue.IssueRow]), search, the relation and PR
 * relation rows, the inbox rows and the composer's issue picker. The CALLER
 * owns the chrome (glass/flat row, click, padding, height floor, test tag,
 * read alpha); this owns the order, the gaps and the type:
 *
 * `[leading] gap [identifier] gap [status] gap [title][titleAccessory] gap [trailing]`
 *
 * With a [subLine] the identifier + title (+ accessory) line and the sub-line
 * stack in a weighted column after [leading] (search, inbox). The identifier
 * is the mono tertiary label every client uses; the title is one ellipsized
 * line of bodyMedium. Absent slots take their gap with them.
 */
@Composable
fun RowScope.IssueRowContent(
    title: String,
    leading: (@Composable RowScope.() -> Unit)? = null,
    leadingGap: Dp = 12.dp,
    identifier: String? = null,
    /** A min (not fixed) identifier column, so typical ids line up. */
    identifierMinWidth: Dp = Dp.Unspecified,
    identifierEllipsis: Boolean = false,
    identifierGap: Dp = 12.dp,
    /** A glyph between the identifier and the title (the status column). */
    status: (@Composable RowScope.() -> Unit)? = null,
    statusGap: Dp = 10.dp,
    titleColor: Color = MaterialTheme.colorScheme.onSurface,
    titleWeight: FontWeight? = null,
    /** On the title's line, after it (a blocks badge, labels, a team name). */
    titleAccessory: (@Composable RowScope.() -> Unit)? = null,
    subLine: (@Composable () -> Unit)? = null,
    trailingGap: Dp = 12.dp,
    trailing: (@Composable RowScope.() -> Unit)? = null,
) {
    if (leading != null) {
        leading()
        if (leadingGap > 0.dp) Spacer(Modifier.width(leadingGap))
    }
    if (subLine == null) {
        TitleLine(
            identifier, identifierMinWidth, identifierEllipsis, identifierGap, status, statusGap,
            title, titleColor, titleWeight, titleAccessory, fillTitle = true,
        )
    } else {
        Column(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                TitleLine(
                    identifier, identifierMinWidth, identifierEllipsis, identifierGap, status, statusGap,
                    title, titleColor, titleWeight, titleAccessory, fillTitle = false,
                )
            }
            subLine()
        }
    }
    if (trailing != null) {
        if (trailingGap > 0.dp) Spacer(Modifier.width(trailingGap))
        trailing()
    }
}

@Composable
private fun RowScope.TitleLine(
    identifier: String?,
    identifierMinWidth: Dp,
    identifierEllipsis: Boolean,
    identifierGap: Dp,
    status: (@Composable RowScope.() -> Unit)?,
    statusGap: Dp,
    title: String,
    titleColor: Color,
    titleWeight: FontWeight?,
    titleAccessory: (@Composable RowScope.() -> Unit)?,
    /** Single-line rows fill the title so trailing glyphs pin to the edge;
     *  a stacked line lets an accessory sit right after the words. */
    fillTitle: Boolean,
) {
    if (identifier != null) {
        Text(
            identifier,
            style = MaterialTheme.typography.labelMedium,
            fontFamily = FontFamily.Monospace,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            maxLines = 1,
            overflow = if (identifierEllipsis) TextOverflow.Ellipsis else TextOverflow.Clip,
            modifier = if (identifierMinWidth != Dp.Unspecified) Modifier.widthIn(min = identifierMinWidth) else Modifier,
        )
        if (identifierGap > 0.dp) Spacer(Modifier.width(identifierGap))
    }
    if (status != null) {
        status()
        if (statusGap > 0.dp) Spacer(Modifier.width(statusGap))
    }
    Text(
        title,
        style = MaterialTheme.typography.bodyMedium,
        color = titleColor,
        fontWeight = titleWeight,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.weight(1f, fill = fillTitle),
    )
    titleAccessory?.invoke(this)
}
