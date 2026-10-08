package at.exponential.ui.primitives

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * The box a text field or a trigger sits in: [fill], a hairline [stroke]
 * that turns [focusedStroke] (the tokens' `ring`) on focus and
 * [invalidStroke] (`destructive`) when invalid, [radius] corners, an
 * optional fixed [height] (`Dp.Unspecified` = the content's). The field
 * itself is the content. Every colour defaults from [tokens].
 */
fun Modifier.fieldChrome(
    tokens: PrimitiveTokens,
    focused: Boolean = false,
    invalid: Boolean = false,
    radius: Dp = tokens.radiusMd,
    height: Dp = Dp.Unspecified,
    fill: Color = tokens.input,
    stroke: Color = tokens.border,
    focusedStroke: Color = tokens.ring,
    invalidStroke: Color = tokens.destructive,
    strokeWidth: Dp = tokens.hairline,
): Modifier {
    val shape = RoundedCornerShape(radius)
    val edge = when {
        invalid -> invalidStroke
        focused -> focusedStroke
        else -> stroke
    }
    return this
        .then(if (height != Dp.Unspecified) Modifier.height(height) else Modifier)
        .background(fill, shape)
        .border(strokeWidth, edge, shape)
}

/**
 * [fieldChrome] as a box: the chrome on the ambient tokens around
 * [content], inset by [contentPadding] horizontally and centred vertically.
 */
@Composable
fun FieldChrome(
    modifier: Modifier = Modifier,
    focused: Boolean = false,
    invalid: Boolean = false,
    tokens: PrimitiveTokens = LocalPrimitiveTokens.current,
    radius: Dp = tokens.radiusMd,
    height: Dp = tokens.inputHeight,
    fill: Color = tokens.input,
    contentPadding: Dp = 12.dp,
    content: @Composable BoxScope.() -> Unit,
) {
    Box(
        modifier = modifier
            .fieldChrome(tokens, focused = focused, invalid = invalid, radius = radius, height = height, fill = fill)
            .padding(horizontal = contentPadding),
        contentAlignment = Alignment.CenterStart,
        content = content,
    )
}
