package at.exponential.ui.compose

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
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.scroll
import at.exponential.ui.theme.PaintStyle
import kotlin.math.max

/**
 * A container's content: its visible children nested at their frames
 * (relative to it); a scrolling one (`overflow: scroll`, or a List the
 * core windowed) in a [ScrollContainer].
 */
@Composable
internal fun ContainerContent(index: Int, node: NodeInfo, model: SurfaceModel, style: PaintStyle, size: Size) {
    val kids = (model.children.getOrNull(index) ?: emptyList()).filter { model.node(it)?.hidden == false }
    val origin = model.frame(index).topLeft
    val scrolls = style.overflowScroll || (model.lists[node.id]?.windowed ?: false)
    if (scrolls) {
        ScrollContainer(index, node, model, size, kids, origin)
    } else {
        ChildrenLayout(size, kids, origin, model)
    }
}

/**
 * The children `kids` of a container whose surface-space origin is
 * [origin], each a [NodeView] at its frame relative to the container, in
 * pre-order (= paint order = TalkBack order through `traversalIndex`).
 */
@Composable
internal fun ChildrenLayout(size: Size, kids: List<Int>, origin: Offset, model: SurfaceModel) {
    val rel = HashMap<Int, Rect>(kids.size * 2)
    for (k in kids) rel[k] = model.frame(k).translate(-origin.x, -origin.y)
    FrameLayout(size = size, frames = rel) {
        for (k in kids) {
            key(k) { NodeView(k, Modifier.frameIndex(k)) }
        }
    }
}

/**
 * A scrolling container: the children at their content offsets inside a
 * vertical scroller whose content is as tall as the core says
 * (`contentHeight`, never below the container). A windowed list reports
 * its visible offset back (`scroll(list, offset)` in dp), which moves the
 * window in constant work (the model relayouts only when the window
 * moved); nothing here guesses row heights and no lazy list is involved.
 */
@Composable
internal fun ScrollContainer(index: Int, node: NodeInfo, model: SurfaceModel, size: Size, kids: List<Int>, origin: Offset) {
    val contentHeight = max(model.contentHeight(index), size.height)
    val listId = node.id
    val windowed = model.lists[listId]?.windowed ?: false
    val state = rememberScrollState()
    val density = LocalDensity.current.density
    if (windowed) {
        LaunchedEffect(listId, state, density) {
            snapshotFlow { state.value }.collect { px ->
                model.scroll(listId, max(0f, px / density))
            }
        }
    }
    Box(Modifier.fillMaxSize().verticalScroll(state)) {
        ChildrenLayout(Size(size.width, contentHeight), kids, origin, model)
    }
}
