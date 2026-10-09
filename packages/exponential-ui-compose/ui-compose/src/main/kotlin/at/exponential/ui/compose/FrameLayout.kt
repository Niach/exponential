package at.exponential.ui.compose

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.layoutId
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.constrainHeight
import androidx.compose.ui.unit.constrainWidth
import kotlin.math.max
import kotlin.math.roundToInt

/**
 * The ONE layout: every child (tagged `Modifier.layoutId(index)`) is
 * measured with exactly the size the core computed and placed at its frame
 * (dp, relative to this container). Compose only places; it never measures
 * through here (the measurer answered the core in batches before the pass)
 * and never asks a child's intrinsics. x / y / width / height each round
 * `dp × density` to the nearest px (the VAPP-4 rule). Coordinates are
 * PHYSICAL: children go through `place`, never `placeRelative`, and the
 * surface pins `LocalLayoutDirection` to Ltr, so an RTL surface (already
 * mirrored by the core) is not mirrored a second time. A child with no
 * frame is placed at the origin at 0×0. The container is [size] (dp).
 */
@Composable
fun FrameLayout(
    size: Size,
    frames: Map<Int, Rect>,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    Layout(content = content, modifier = modifier) { measurables, constraints ->
        val d = density
        val placed = measurables.map { m ->
            val f = frames[m.layoutId as? Int]
            if (f == null) {
                Triple(m.measure(Constraints.fixed(0, 0)), 0, 0)
            } else {
                val w = max(0, (f.width * d).roundToInt())
                val h = max(0, (f.height * d).roundToInt())
                Triple(m.measure(Constraints.fixed(w, h)), (f.left * d).roundToInt(), (f.top * d).roundToInt())
            }
        }
        val width = constraints.constrainWidth(max(0, (size.width * d).roundToInt()))
        val height = constraints.constrainHeight(max(0, (size.height * d).roundToInt()))
        layout(width, height) {
            for ((p, x, y) in placed) p.place(x, y)
        }
    }
}

/** The tag [FrameLayout] reads: the node index of a child. */
fun Modifier.frameIndex(index: Int): Modifier = layoutId(index)
