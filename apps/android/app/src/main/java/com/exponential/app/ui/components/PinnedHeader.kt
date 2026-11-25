package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import com.exponential.app.domain.DetailChrome

// #EXP-2109 (chats.json THE HEADER IS PINNED): the page header stays put and
// the list scrolls under it: the header stands OUTSIDE the scroller and THE
// detail edge fade ([EdgeFade], the page's ground) hangs below it while the
// list is scrolled, so a row dissolves under the title.

/** How far the list under the header is scrolled (px), read off nested scroll. */
private class PinnedEdge(private val get: () -> Float, private val set: (Float) -> Unit) {
    val offset: Float get() = get()

    val connection = object : NestedScrollConnection {
        override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
            // A pull left over at the top = the list IS at its top.
            set(if (available.y > 0f) 0f else (get() - consumed.y).coerceAtLeast(0f))
            return Offset.Zero
        }
    }
}

@Composable
private fun rememberPinnedEdge(): PinnedEdge {
    // Saved: coming Back to a scrolled list keeps its fade.
    var offset by rememberSaveable { mutableFloatStateOf(0f) }
    return remember { PinnedEdge({ offset }, { offset = it }) }
}

/** [header] with the fade below it, outside its reported size (no touch). */
@Composable
private fun PinnedHeaderEdge(edge: PinnedEdge, header: @Composable () -> Unit) {
    Layout(
        contents = listOf(
            {
                EdgeFade(
                    FadeEdge.Top,
                    modifier = Modifier.graphicsLayer { alpha = (edge.offset / DetailChrome.EDGE_TOP.dp.toPx()).coerceIn(0f, 1f) },
                    ground = true,
                )
            },
            { Box { header() } },
        ),
    ) { (strips, bands), constraints ->
        val band = bands.first().measure(constraints)
        val strip = strips.first().measure(Constraints.fixed(band.width, DetailChrome.EDGE_TOP.dp.roundToPx()))
        layout(band.width, band.height) {
            strip.place(0, band.height)
            band.place(0, 0)
        }
    }
}

/** A page whose header is content: [header], then [content] scrolling under it. */
@Composable
fun PinnedHeaderColumn(
    header: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    val edge = rememberPinnedEdge()
    Column(modifier.nestedScroll(edge.connection)) {
        Box(Modifier.zIndex(1f)) { PinnedHeaderEdge(edge, header) }
        Box(Modifier.weight(1f)) { content() }
    }
}

/** A pushed page: the pinned [topBar] (a [PageHeader]) over its body. */
@Composable
fun PinnedScaffold(
    topBar: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable (PaddingValues) -> Unit,
) {
    val edge = rememberPinnedEdge()
    Scaffold(
        modifier = modifier.nestedScroll(edge.connection),
        containerColor = Color.Transparent,
        topBar = { PinnedHeaderEdge(edge, topBar) },
        content = content,
    )
}
