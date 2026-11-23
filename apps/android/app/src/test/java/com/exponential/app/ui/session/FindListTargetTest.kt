package com.exponential.app.ui.session

import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentFeedRow
import com.exponential.app.domain.MessageDelivery
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * #EXP-2051 (find-in-chat.json LANDING + AFTER AN OLDER LANDING): a stop
 * lands on ITS row, found by key: a transcript row is its own list item, an
 * own bubble is one bubble inside the own-tail item and is centred by its
 * own place there, never the item's centre.
 */
class FindListTargetTest {

    private val feed = listOf(
        AgentFeedItem.Narration(id = 10, text = "filler 1", seq = 1),
        AgentFeedItem.Narration(id = 11, text = "filler 3 wombatword", seq = 2),
        // The synced tail a not-live transcript draws (`m:` keys).
        AgentFeedItem.Narration(id = 12, text = "filler 5 wombatword", messageId = "synced:a5"),
        AgentFeedItem.Narration(id = 13, text = "filler 9 wombatword", messageId = "synced:a9"),
    )
    private val keys = transcriptFindKeys(feed, "s1")
    private val rows = feed.map { AgentFeedRow.Single(it) }

    private fun own(n: Int) = MessageDelivery.Bubble(
        key = "u$n",
        text = "filler $n",
        createdAt = "2026-11-20T10:00:00.000Z",
        state = MessageDelivery.State.Sending,
        stored = true,
        clientId = null,
        messageId = "u$n",
    )

    @Test
    fun `a row stop is its row's item, by the row's key, below Load earlier and the head cards`() {
        val target = findListTarget("m:a9", rows, keys, lead = 2, ownKeys = emptySet(), ownTailIndex = null)
        assertEquals(FindListTarget(index = 5, itemKey = 13L, inner = false), target)
        assertEquals(FindListTarget(3, 11L, false), findListTarget("e:s1:2", rows, keys, lead = 2, ownKeys = emptySet(), ownTailIndex = null))
        // Not drawn here (yet): nothing to land on, the landing pages.
        assertNull(findListTarget("m:older", rows, keys, lead = 2, ownKeys = emptySet(), ownTailIndex = null))
    }

    @Test
    fun `every own bubble stop is the own-tail item with the bubble inside it`() {
        val ownTail = listOf(own(4), own(10), own(120))
        val ownKeys = ownTail.mapTo(HashSet(), ::ownBubbleFindKey)
        for (bubble in ownTail) {
            val target = findListTarget(ownBubbleFindKey(bubble), rows, keys, lead = 0, ownKeys = ownKeys, ownTailIndex = 4)
            assertEquals(FindListTarget(4, OWN_TAIL_ITEM_KEY, inner = true), target)
        }
        // No list yet: nothing to land on.
        assertNull(findListTarget("m:u4", rows, keys, lead = 0, ownKeys = ownKeys, ownTailIndex = null))
    }

    @Test
    fun `an inner stop centres its own span, a row its item`() {
        // A 6,000 px own-tail item at the viewport's top (0..2,000): the
        // item's centre would always be its middle (the #EXP-2051 symptom:
        // every stop "landed" on the same stretch).
        assertEquals(2_000, deepLinkCentreDelta(itemOffset = 0, itemSize = 6_000, span = null, viewportStart = 0, viewportEnd = 2_000))
        // Bubble "filler 4" near the item's top, "filler 120" near its bottom.
        assertEquals(-900, deepLinkCentreDelta(0, 6_000, ItemSpan(top = 40, size = 120), 0, 2_000))
        assertEquals(4_780, deepLinkCentreDelta(0, 6_000, ItemSpan(top = 5_720, size = 120), 0, 2_000))
        // Scrolled so the bubble sits at the centre: nothing left to scroll.
        assertEquals(0, deepLinkCentreDelta(-4_780, 6_000, ItemSpan(top = 5_720, size = 120), 0, 2_000))
        // A plain row: its own centre.
        assertEquals(-850, deepLinkCentreDelta(100, 100, null, 0, 2_000))
    }
}
