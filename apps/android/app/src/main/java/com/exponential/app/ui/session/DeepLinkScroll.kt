package com.exponential.app.ui.session

import android.os.SystemClock
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import com.exponential.app.domain.DeepLinkDriver
import com.exponential.app.domain.WaitingCards
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.abs

/** A transcript's pending deep link: [pending] = the list must not follow its
 *  newest row; [ringing] = the card wearing the highlight ring. */
internal class DeepLinkState(val pending: Boolean, val ringing: String?)

/**
 * waiting-cards.json DEEP LINKS ×4: THE one pending deep link of a
 * transcript ([LazyListState]), driven by [DeepLinkDriver] once per frame
 * while it is pending: `page` = ONE older page ([page]), `wait` = until its
 * item is laid out (an off-screen loaded row is scrolled into the layout
 * first), `scroll` = to the viewport's centre, `ring` = the highlight for
 * `constants.highlightMs`; `missing` = drop. The reader's own drag drops it.
 * [indexOf] = the target's item index in the list (null = not loaded),
 * [loadedMark] = a value that changes when a page lands.
 */
@Composable
internal fun rememberDeepLink(
    listState: LazyListState,
    request: CardScrollRequest?,
    indexOf: (id: String) -> Int?,
    more: () -> Boolean,
    loadedMark: () -> Int,
    page: () -> Unit,
    onTake: () -> Unit = {},
    /** false = a link of the transcript's own (find in chat's landing): the
     *  host's card link ([LocalCardScrollSettled]) is not told. */
    reportSettled: Boolean = true,
    /** #EXP-2051: the target's LazyColumn item KEY (null = look it up by
     *  [indexOf]'s index): the laid-out item is found by its key, never by
     *  an index a page or a widened tail may have shifted. */
    itemKeyOf: (id: String) -> Any? = { null },
    /** #EXP-2051: the target is ONE part of its item (an own-tail bubble
     *  inside the own-tail item): [spanOf] places it; until it does the
     *  target is not laid out (never the item's centre). */
    innerOf: (id: String) -> Boolean = { false },
    spanOf: (id: String) -> ItemSpan? = { null },
): DeepLinkState {
    val settle = if (reportSettled) LocalCardScrollSettled.current else null
    val latestIndexOf by rememberUpdatedState(indexOf)
    val latestMore by rememberUpdatedState(more)
    val latestMark by rememberUpdatedState(loadedMark)
    val latestPage by rememberUpdatedState(page)
    val latestTake by rememberUpdatedState(onTake)
    val latestSettle by rememberUpdatedState(settle)
    val latestKeyOf by rememberUpdatedState(itemKeyOf)
    val latestInnerOf by rememberUpdatedState(innerOf)
    val latestSpanOf by rememberUpdatedState(spanOf)
    var settled by remember { mutableStateOf<CardScrollRequest?>(null) }
    var ringing by remember { mutableStateOf<Pair<String, Long>?>(null) }
    LaunchedEffect(request) {
        val req = request ?: return@LaunchedEffect
        val driver = DeepLinkDriver()
        driver.take(req.id, req.takenAtMs)
        latestTake()
        // The reader's OWN drag (never a programmatic scroll) drops the link.
        val drags = launch {
            listState.interactionSource.interactions.collect { if (it is DragInteraction.Start) driver.drop() }
        }
        var nudged: Int? = null
        try {
            while (driver.link != null) {
                withFrameNanos { }
                val index = latestIndexOf(req.id)
                val item = layoutInfoOf(listState, index, if (index == null) null else latestKeyOf(req.id))
                val span = if (item != null && latestInnerOf(req.id)) latestSpanOf(req.id) else null
                val info = item?.takeIf { !latestInnerOf(req.id) || span != null }
                val delta = info?.let {
                    val layout = listState.layoutInfo
                    deepLinkCentreDelta(it.offset, it.size, span, layout.viewportStartOffset, layout.viewportEndOffset)
                } ?: 0
                val centred = info != null && (
                    abs(delta) <= 2 ||
                        (delta > 0 && !listState.canScrollForward) ||
                        (delta < 0 && !listState.canScrollBackward)
                    )
                val step = driver.step(
                    found = index != null,
                    laidOut = info != null,
                    centred = centred,
                    more = latestMore(),
                    loadedMark = latestMark(),
                    nowMs = SystemClock.uptimeMillis(),
                ) ?: break
                when (step) {
                    WaitingCards.DeepLinkStep.Ring -> ringing = req.id to SystemClock.uptimeMillis()
                    WaitingCards.DeepLinkStep.Scroll -> listState.animateScrollBy(delta.toFloat())
                    // Loaded but off screen: bring it into the layout once,
                    // the next frame measures it at its real height.
                    WaitingCards.DeepLinkStep.Wait -> if (index != null && item == null && nudged != index) {
                        nudged = index
                        listState.scrollToItem(index)
                    }
                    WaitingCards.DeepLinkStep.Page -> latestPage()
                    WaitingCards.DeepLinkStep.Missing -> Unit
                }
            }
        } finally {
            drags.cancel()
            settled = req
            latestSettle?.invoke(req)
        }
    }
    val ring = ringing
    LaunchedEffect(ring) {
        if (ring == null) return@LaunchedEffect
        delay(WaitingCards.Constants.HIGHLIGHT_MS)
        ringing = null
    }
    return DeepLinkState(pending = request != null && request != settled, ringing = ring?.first)
}

/** The target's laid-out item: by its [key] when it has one, else by [index]. */
private fun layoutInfoOf(state: LazyListState, index: Int?, key: Any?) = when {
    index == null -> null
    key != null -> state.layoutInfo.visibleItemsInfo.firstOrNull { it.key == key }
    else -> state.layoutInfo.visibleItemsInfo.firstOrNull { it.index == index }
}

/** #EXP-2051: a deep-link target that is one part of its list item: its top
 *  below the item's top and its height (px). */
internal data class ItemSpan(val top: Int, val size: Int)

/** How far the target's centre (the item's, or its [span]'s inside it) sits
 *  below the viewport's centre (px). */
internal fun deepLinkCentreDelta(itemOffset: Int, itemSize: Int, span: ItemSpan?, viewportStart: Int, viewportEnd: Int): Int {
    val centre = span?.let { it.top + it.size / 2 } ?: (itemSize / 2)
    return itemOffset + centre - (viewportStart + viewportEnd) / 2
}
