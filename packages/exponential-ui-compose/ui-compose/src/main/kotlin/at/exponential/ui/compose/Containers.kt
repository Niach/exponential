package at.exponential.ui.compose

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.platform.LocalDensity
import at.exponential.ui.model.NodeInfo
import at.exponential.ui.model.ScrollInfo
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.scrollTo
import at.exponential.ui.theme.PaintStyle
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.roundToInt

/**
 * A container's content: its visible children nested at their frames
 * (relative to it); a scroll container (the core's `scrolls`: any
 * `overflow: scroll | auto` node, windowed lists, a Dialog body) in a
 * [ScrollContainer].
 */
@Composable
internal fun ContainerContent(index: Int, node: NodeInfo, model: SurfaceModel, style: PaintStyle, size: Size) {
    val kids = (model.children.getOrNull(index) ?: emptyList()).filter { model.node(it)?.hidden == false }
    val origin = model.frame(index).topLeft
    val scroll = model.scrolls[index]
    if (scroll != null && (scroll.scrollX || scroll.scrollY)) {
        ScrollContainer(node, model, size, kids, origin, scroll)
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
internal fun ScrollContainer(node: NodeInfo, model: SurfaceModel, size: Size, kids: List<Int>, origin: Offset, scroll: ScrollInfo) {
    val sx = rememberScrollState()
    val sy = rememberScrollState()
    val density = LocalDensity.current.density
    val id = node.id
    LaunchedEffect(id, sx, sy, density) {
        snapshotFlow { sx.value to sy.value }.collect { (x, y) -> model.scrollTo(id, x / density, y / density, fromView = true) }
    }
    LaunchedEffect(scroll.offsetX, scroll.offsetY, model.scrollEpoch, density) {
        val ty = (scroll.offsetY * density).roundToInt()
        val tx = (scroll.offsetX * density).roundToInt()
        if (scroll.scrollY && abs(sy.value - ty) > 1) sy.scrollTo(ty)
        if (scroll.scrollX && abs(sx.value - tx) > 1) sx.scrollTo(tx)
    }
    val content = Size(
        if (scroll.scrollX) max(scroll.contentWidth, size.width) else size.width,
        if (scroll.scrollY) max(scroll.contentHeight, size.height) else size.height,
    )
    var m: Modifier = Modifier.fillMaxSize()
    if (scroll.scrollY) m = m.verticalScroll(sy)
    if (scroll.scrollX) m = m.horizontalScroll(sx)
    Box(m) {
        ChildrenLayout(content, kids, origin, model)
    }
}
