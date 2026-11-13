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
): DeepLinkState {
    val settle = LocalCardScrollSettled.current
    val latestIndexOf by rememberUpdatedState(indexOf)
    val latestMore by rememberUpdatedState(more)
    val latestMark by rememberUpdatedState(loadedMark)
    val latestPage by rememberUpdatedState(page)
    val latestTake by rememberUpdatedState(onTake)
    val latestSettle by rememberUpdatedState(settle)
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
                val info = layoutInfoOf(listState, index)
                val delta = info?.let { centreDelta(listState, it.offset, it.size) } ?: 0
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
                    WaitingCards.DeepLinkStep.Wait -> if (index != null && info == null && nudged != index) {
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

private fun layoutInfoOf(state: LazyListState, index: Int?) =
    index?.let { at -> state.layoutInfo.visibleItemsInfo.firstOrNull { it.index == at } }

/** How far the item's centre sits below the viewport's centre (px). */
private fun centreDelta(state: LazyListState, offset: Int, size: Int): Int {
    val info = state.layoutInfo
    val viewportCentre = (info.viewportStartOffset + info.viewportEndOffset) / 2
    return offset + size / 2 - viewportCentre
}
