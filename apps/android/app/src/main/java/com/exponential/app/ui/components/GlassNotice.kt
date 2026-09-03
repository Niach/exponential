package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

/**
 * A transient message floating over scrolling content — "No desktop online to
 * start this on", a failed remote start, the issue list's outcome captions
 * (EXP-698).
 *
 * It is deliberately NOT a [GlassPill]: a pill is a fixed-height capsule for a
 * short one-line label, and these sentences are written by the server and run
 * to two or three lines on a phone. Putting them in a capsule clipped them.
 * So a notice is the ROW rung — 10dp corners, [glassRow]'s fill and hairline —
 * and its text wraps.
 *
 * [opaque] (the default) lays the solid card fill under the glass tint, since
 * every host overlays this on a list or a diff that scrolls beneath it.
 */
@Composable
fun GlassNotice(
    text: String,
    modifier: Modifier = Modifier,
    leading: (@Composable () -> Unit)? = null,
    contentColor: Color? = null,
    opaque: Boolean = true,
    onClick: (() -> Unit)? = null,
) {
    val fg = contentColor
        ?: MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(GlassNoticeDefaults.Spacing),
        modifier = modifier
            .glassRow(opaque = opaque)
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(
                horizontal = GlassNoticeDefaults.HorizontalPadding,
                vertical = GlassNoticeDefaults.VerticalPadding,
            ),
    ) {
        if (leading != null) {
            CompositionLocalProvider(LocalContentColor provides fg) { leading() }
        }
        Text(
            text,
            style = MaterialTheme.typography.labelMedium,
            color = fg,
        )
    }
}

/** The notice's own numbers, so its two hosts cannot drift apart. */
object GlassNoticeDefaults {
    val HorizontalPadding: Dp = 14.dp
    val VerticalPadding: Dp = 8.dp
    val Spacing: Dp = 8.dp

    /** The leading spinner / glyph — small enough to sit on the first line. */
    val GlyphSize: Dp = 12.dp
}
