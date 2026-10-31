package at.exponential.ui.compose

import androidx.compose.foundation.ScrollState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.platform.LocalDensity
import at.exponential.ui.json.num
import at.exponential.ui.model.NodeInfo
import at.exponential.ui.model.SurfaceModel
import kotlinx.coroutines.flow.first
import kotlin.math.abs
import kotlin.math.roundToInt

// EXP-1495 (docs/round-6-contract.md): the round-6 pieces of the Compose
// painter. Outline and Mosaic are macros (Box / Text / Image, nothing of
// their own here); the Carousel `counter` and `thumbnails` strip are
// the core's frames: the counter a plain Text part, the root's first flow
// line above the track (EXP-1493: start-aligned, no fill, hidden from
// TalkBack), the strip a row of pressable
// 52 x 52 boxes whose press goes through the Carousel's press path (the
// core reads the thumbnail's `target` and fires `change {page}`). What is
// painter-side: the counter's tabular figures
// (`ResolvedTextStyle.forPart`) and keeping the current thumbnail in view.

/**
 * The offset (dp) the Carousel `thumbnails` strip [strip] scrolls to so the
 * thumbnail of [page] shows inside a [width] wide viewport now at [current]
 * (`nearest`: the start edge when it is before the viewport, the end edge
 * when after); null = it already shows (or there is no such thumbnail).
 */
internal fun thumbnailReveal(model: SurfaceModel, strip: NodeInfo, page: Int, current: Float, width: Float): Float? {
    val owner = strip.owner ?: return null
    val i = model.indexOf("$owner.thumbnail.$page") ?: return null
    val origin = model.frame(strip.index).left
    val f = model.frame(i)
    val left = f.left - origin
    val right = f.right - origin
    return when {
        width <= 0f -> null
        left < current - 0.5f -> left
        right > current + width + 0.5f -> right - width
        else -> null
    }
}

/**
 * Round-6 §3: the current thumbnail scrolled into view (the strip's own
 * horizontal scroller only, never an ancestor's) whenever the page moves
 * or the strip resizes; animated unless motion is reduced. The view's new
 * offset reaches the core through the scroller's usual `scrollTo`.
 */
@Composable
internal fun ThumbnailStripFollow(node: NodeInfo, model: SurfaceModel, sx: ScrollState, size: Size) {
    if (node.ownerComponent != "Carousel" || node.part != "thumbnails") return
    val page = node.props.num("page")?.roundToInt() ?: return
    val density = LocalDensity.current.density
    val reduced = model.reducedMotion
    LaunchedEffect(node.id, page, size.width, density) {
        val to = thumbnailReveal(model, node, page, sx.value / density, size.width) ?: return@LaunchedEffect
        // The scroller learns its range in its first layout.
        val max = snapshotFlow { sx.maxValue }.first { it > 0 }
        val px = (to * density).roundToInt().coerceIn(0, max)
        if (abs(px - sx.value) <= 1) return@LaunchedEffect
        if (reduced) sx.scrollTo(px) else sx.animateScrollTo(px)
    }
}
