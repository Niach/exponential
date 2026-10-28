package at.exponential.ui.primitives

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** S-3: the catalog `Kbd` geometry (web `KBD_CLASSES`): `$control.badge`, `$spacing.xs`. */
object KbdMetrics {
    /** `h-5` / `min-w-5`. */
    val Height = 20.dp
    /** `px-1`. */
    val HorizontalPadding = 4.dp
    /** `gap-1` between the caps of a [KbdGroup]. */
    val Gap = 4.dp
}

/**
 * THE keycap outside a surface (the painter draws the `Kbd` macro inside
 * one): 20 dp tall, at least 20 dp wide, 4 dp side padding, the `radius.sm`
 * corner, the muted fill, 12 sp medium muted text.
 */
@Composable
fun Kbd(label: String, modifier: Modifier = Modifier) {
    val tokens = LocalPrimitiveTokens.current
    val shape = RoundedCornerShape(tokens.radiusSm)
    Box(
        contentAlignment = Alignment.Center,
        modifier = modifier
            .height(KbdMetrics.Height)
            .defaultMinSize(minWidth = KbdMetrics.Height)
            .background(tokens.muted, shape)
            .padding(horizontal = KbdMetrics.HorizontalPadding),
    ) {
        BasicText(
            label,
            style = TextStyle(fontSize = 12.sp, lineHeight = 16.sp, fontWeight = FontWeight.Medium, color = tokens.mutedForeground),
            maxLines = 1,
        )
    }
}

/** `KbdGroup`: the caps of a chord in a row, 4 dp apart. */
@Composable
fun KbdGroup(modifier: Modifier = Modifier, content: @Composable RowScope.() -> Unit) {
    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(KbdMetrics.Gap),
        verticalAlignment = Alignment.CenterVertically,
        content = content,
    )
}
