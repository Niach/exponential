package at.exponential.ui.compose

import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Constraints
import kotlin.math.roundToInt

/**
 * The px a text may lack and still paint on one line. The core lays text
 * out at its FRACTIONAL shaped width (`TextShaper.lineWidth`, as the web and
 * gpui do), while every frame and inner box rounds to whole px and Compose
 * ceils a text's intrinsic width: a label at exactly its max-content can
 * come out ≤ 1 px short (two roundings of ≤ 0.5 px).
 */
const val TEXT_SLACK_PX = 2

/** The physical side text aligns to: -1 left, 0 centre, 1 right (start / unset follow [rtl]). */
internal fun textBias(align: TextAlign?, rtl: Boolean): Float = when (align) {
    TextAlign.Left -> -1f
    TextAlign.Right -> 1f
    TextAlign.Center -> 0f
    TextAlign.End -> if (rtl) -1f else 1f
    else -> if (rtl) 1f else -1f
}

/**
 * A text whose intrinsic width exceeds its box by at most [TEXT_SLACK_PX]
 * is laid out at that intrinsic width (no wrap, no ellipsis) and placed by
 * [bias] over the overhang; any other text measures as given.
 */
internal fun Modifier.textSlack(bias: Float): Modifier = layout { m, c ->
    if (c.hasBoundedWidth) {
        val need = m.maxIntrinsicWidth(if (c.hasBoundedHeight) c.maxHeight else Constraints.Infinity)
        val over = need - c.maxWidth
        if (over in 1..TEXT_SLACK_PX) {
            val p = m.measure(c.copy(minWidth = need, maxWidth = need))
            return@layout layout(c.maxWidth, p.height) { p.place((-over * (bias + 1f) / 2f).roundToInt(), 0) }
        }
    }
    val p = m.measure(c)
    layout(p.width, p.height) { p.place(0, 0) }
}
