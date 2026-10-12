package com.exponential.app.ui.components

import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.ui.theme.DesignTokens

/**
 * EXP-846: the Exponential mark, for the surfaces that have to say "this was
 * US" — the transcript rows of our own MCP tool calls, which wear the product's
 * mark the way a file edit wears the wrench — and (EXP-1253) the Agent home's
 * headline.
 *
 * EXP-1275: painted from the ONE copy of the logo's stripe geometry
 * ([FeedbackStates.STRIPES], the same cubics the animated [FeedbackMark]
 * sweeps), the stripes CUT through to the ground behind the mark. [size] is
 * the disc's diameter. Only the splash drawable (an OS resource) keeps its own
 * copy of the paths.
 */
@Composable
fun ExponentialMark(
    size: Dp = 12.dp,
    modifier: Modifier = Modifier,
    /**
     * Paint the disc in ONE colour (the foreground for the home headline);
     * null = the logo's own light disc.
     */
    tint: Color? = null,
) {
    MarkCanvas(
        spans = FeedbackStates.STATIC_SPANS,
        color = tint ?: DesignTokens.Palette.Foreground,
        modifier = modifier.size(size),
    )
}
