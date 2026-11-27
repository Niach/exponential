package at.exponential.ui.compose

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.key
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.platform.LocalDensity
import at.exponential.ui.model.NodeInfo
import at.exponential.ui.model.ScrollInfo
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.scrollTo
import at.exponential.ui.theme.PaintStyle
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.roundToInt
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull

/**
 * A container's content: its visible children nested at their frames
 * (relative to it); a scroll container (the core's `scrolls`: any
 * `overflow: scroll | auto` node, windowed lists, a Dialog body) in a
 * [ScrollContainer].
 */
@Composable
internal fun ContainerContent(index: Int, node: NodeInfo, model: SurfaceModel, style: PaintStyle, size: Size) {
    // NEW-77: the VoiceWave native paints its waves and centres its one child.
    if (node.component == "VoiceWave" && node.part == null) return VoiceWaveContent(index, node, model, size)
    val kids = (model.children.getOrNull(index) ?: emptyList()).filter { model.node(it)?.hidden == false }
    val origin = model.frame(index).topLeft
    val scroll = model.scrolls[index]
    if (scroll != null && (scroll.scrollX || scroll.scrollY)) {
        ScrollContainer(node, model, size, kids, origin, scroll, style.sides)
    } else {
        ChildrenLayout(size, kids, origin, model)
    }
}

/**
 * The children `kids` of a container whose surface-space origin is
 * [origin], each a [NodeView] at its frame relative to the container (plus
 * its sticky pin, round 2), in pre-order (= paint order = TalkBack order
 * through `traversalIndex`).
 */
@Composable
internal fun ChildrenLayout(size: Size, kids: List<Int>, origin: Offset, model: SurfaceModel) {
    val rel = HashMap<Int, Rect>(kids.size * 2)
    val pins = model.sticky
    for (k in kids) {
        var r = model.frame(k).translate(-origin.x, -origin.y)
        pins[k]?.let { r = r.translate(it.x, it.y) }
        rel[k] = r
    }
    FrameLayout(size = size, frames = rel) {
        for (k in kids) {
            key(k) { NodeView(k, Modifier.frameIndex(k)) }
        }
    }
}

/**
 * A scroll container: the children at their UNSCROLLED content offsets
 * inside Compose scrollers (vertical and / or horizontal) whose content is
 * as large as the core says. Every offset change goes back to the core
 * (`scrollTo(id, x, y, fromView)` in dp): a plain container lays nothing
 * out again (Compose moved the paint), a windowed list re-windows and
 * sticky pins move; an offset the core sets (`scrollIntoView`,
 * `scrollToIndex`, a clamp) scrolls the view ([SurfaceModel.scrollEpoch]). Nothing here guesses row heights and no lazy
 * list is involved: the core windows.
 */
@Composable
internal fun ScrollContainer(node: NodeInfo, model: SurfaceModel, size: Size, kids: List<Int>, origin: Offset, scroll: ScrollInfo, sides: List<Float>) {
    val sx = rememberScrollState()
    val sy = rememberScrollState()
    val density = LocalDensity.current.density
    val id = node.id
    LaunchedEffect(id, sx, sy, density) {
        // Only MOVES report: the scrollers' first value is where they start
        // (0, 0), not a scroll (#EXP-1804: it would unpin a `follow` CodeBlock
        // the core already put at its bottom).
        snapshotFlow { sx.value to sy.value }.drop(1).collect { (x, y) -> model.scrollTo(id, x / density, y / density, fromView = true) }
    }
    LaunchedEffect(scroll.offsetX, scroll.offsetY, model.scrollEpoch, density) {
        val ty = (scroll.offsetY * density).roundToInt()
        val tx = (scroll.offsetX * density).roundToInt()
        // #EXP-1804: an offset past the scroller's current end (the content
        // just grew: a `follow` CodeBlock's new bottom) waits for the measure
        // that makes room, else the scroller clamps it short and reports that.
        if (scroll.scrollY && abs(sy.value - ty) > 1) {
            if (sy.maxValue < ty) withTimeoutOrNull(500) { snapshotFlow { sy.maxValue }.first { it >= ty } }
            sy.scrollTo(ty)
        }
        if (scroll.scrollX && abs(sx.value - tx) > 1) {
            if (sx.maxValue < tx) withTimeoutOrNull(500) { snapshotFlow { sx.maxValue }.first { it >= tx } }
            sx.scrollTo(tx)
        }
    }
    // EXP-1495: a Carousel's thumbnail strip keeps the current page's thumbnail in view.
    if (scroll.scrollX) ThumbnailStripFollow(node, model, sx, size)
    // #EXP-2204: a CodeBlock that scrolls itself keeps its first selected line in view.
    if (scroll.scrollY) CodeSelectionFollow(node, model, kids, origin, sy, size)
    val content = Size(
        if (scroll.scrollX) max(scroll.contentWidth, size.width) else size.width,
        if (scroll.scrollY) max(scroll.contentHeight, size.height) else size.height,
    )
    // VAPP-250: a scroller clips at its padding box (CSS): what scrolls never
    // shows through its border, or beside a node pinned at the padding edge.
    var m: Modifier = Modifier.fillMaxSize().drawWithContent {
        clipRect(sides[3] * density, sides[0] * density, this.size.width - sides[1] * density, this.size.height - sides[2] * density) { this@drawWithContent.drawContent() }
    }
    if (scroll.scrollY) m = m.verticalScroll(sy)
    if (scroll.scrollX) m = m.horizontalScroll(sx)
    // #EXP-2204 (contract §6): a long uniform CodeBlock body builds only
    // the lines in view ± the overscan, re-windowed per line scrolled.
    val shown = if (scroll.scrollY && codeWindows(node, kids, model)) {
        val lh = max(1f, model.frame(kids.first()).height)
        val first by remember(lh, density) { derivedStateOf { (sy.value / density / lh).toInt() } }
        remember(kids, first, size.height, lh) { codeWindow(kids, model, origin, first * lh, size.height, lh) }
    } else {
        kids
    }
    Box(m) {
        ChildrenLayout(content, shown, origin, model)
    }
}

/** style.json `codeBlock`: a self-scrolling body past this many lines windows. */
internal const val CODE_WINDOW_THRESHOLD = 50

/** layout.json `windowOverscan`: lines built beyond the viewport, each side. */
internal const val CODE_WINDOW_OVERSCAN = 5

/** #EXP-2204: does this scroll container window its CodeBlock lines (a body, more than the threshold, `wrap: false`)? */
internal fun codeWindows(node: NodeInfo, kids: List<Int>, model: SurfaceModel): Boolean =
    node.ownerComponent == "CodeBlock" && node.part == "body" && kids.size > CODE_WINDOW_THRESHOLD && model.ownerProps(node.index)["wrap"]?.bool != true

/** The lines of a windowed CodeBlock body intersecting [offset]…[offset] + [height] ± the overscan (dp, content space). */
internal fun codeWindow(kids: List<Int>, model: SurfaceModel, origin: Offset, offset: Float, height: Float, lineHeight: Float): List<Int> {
    val lo = offset - CODE_WINDOW_OVERSCAN * lineHeight
    val hi = offset + height + CODE_WINDOW_OVERSCAN * lineHeight
    return kids.filter { k ->
        val f = model.frame(k)
        f.bottom - origin.y >= lo && f.top - origin.y <= hi
    }
}

/**
 * #EXP-2204 (contract §6): a CodeBlock body that scrolls itself scrolls its
 * first `selected` line (the `selection` anchor) into view, once per new
 * selection.
 */
@Composable
private fun CodeSelectionFollow(node: NodeInfo, model: SurfaceModel, kids: List<Int>, origin: Offset, sy: androidx.compose.foundation.ScrollState, size: Size) {
    if (node.ownerComponent != "CodeBlock" || node.part != "body") return
    val line = kids.firstOrNull { model.node(it)?.props?.get("selected")?.bool == true } ?: return
    val f = model.frame(line)
    val top = f.top - origin.y
    val density = LocalDensity.current.density
    LaunchedEffect(node.id, line, density) {
        val view = sy.value / density
        if (top >= view && top + f.height <= view + size.height) return@LaunchedEffect
        val max = snapshotFlow { sy.maxValue }.first { it > 0 }
        sy.scrollTo((top * density).roundToInt().coerceIn(0, max))
    }
}
