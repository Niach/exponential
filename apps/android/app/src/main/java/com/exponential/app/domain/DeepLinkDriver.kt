package com.exponential.app.domain

/**
 * waiting-cards.json DEEP LINKS: THE one pending deep link a transcript holds
 * until it settles. Pure: the view feeds it its facts on every layout /
 * arrival ([step]) and acts on the answer ([WaitingCards.deepLinkStep]):
 * `page` = ONE older page (then [paging] until the loaded rows change or
 * [pageSettleMs] passes), `wait`, `scroll`, and `ring` / `missing` settle the
 * link. The reader's own scroll [drop]s it.
 */
class DeepLinkDriver(private val pageSettleMs: Long = PAGE_SETTLE_MS) {

    data class Link(val id: String, val takenAtMs: Long)

    /** The pending link; null = none (settled or dropped). */
    var link: Link? = null
        private set

    private var pagedAtMs: Long? = null
    private var pagedMark: Int = 0

    /** A new jump replaces any pending one. */
    fun take(id: String, nowMs: Long) {
        link = Link(id, nowMs)
        pagedAtMs = null
    }

    /** The reader's own scroll (or the view going away). */
    fun drop() {
        link = null
        pagedAtMs = null
    }

    /** Whether the page last asked for is still in flight: the loaded rows
     *  ([loadedMark]) did not change since and it is not stale. */
    private fun paging(loadedMark: Int, nowMs: Long): Boolean {
        val at = pagedAtMs ?: return false
        if (loadedMark != pagedMark || nowMs - at >= pageSettleMs) {
            pagedAtMs = null
            return false
        }
        return true
    }

    /** One step over the view's facts; null = no pending link. [loadedMark]
     *  = any value that changes when a page lands (the loaded row count). */
    fun step(
        found: Boolean,
        laidOut: Boolean,
        centred: Boolean,
        more: Boolean,
        loadedMark: Int,
        nowMs: Long,
    ): WaitingCards.DeepLinkStep? {
        val pending = link ?: return null
        val step = WaitingCards.deepLinkStep(
            WaitingCards.DeepLinkInput(
                found = found,
                laidOut = laidOut,
                centred = centred,
                paging = paging(loadedMark, nowMs),
                more = more,
                elapsedMs = nowMs - pending.takenAtMs,
            ),
        )
        when (step) {
            WaitingCards.DeepLinkStep.Ring, WaitingCards.DeepLinkStep.Missing -> drop()
            WaitingCards.DeepLinkStep.Page -> {
                pagedAtMs = nowMs
                pagedMark = loadedMark
            }
            else -> Unit
        }
        return step
    }

    companion object {
        /** A page that changes nothing stops counting as in flight after this. */
        const val PAGE_SETTLE_MS = 1_500L
    }
}
