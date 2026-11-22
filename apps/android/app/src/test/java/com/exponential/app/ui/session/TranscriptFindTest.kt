package com.exponential.app.ui.session

import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentFeedRow
import com.exponential.app.domain.FindInChat
import com.exponential.app.domain.MessageDelivery
import com.exponential.app.domain.groupFeedRows
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** #EDE-130: find in chat's transcript half (the rows it counts, the keys). */
class TranscriptFindTest {

    private fun tool(id: Long, seq: Long, detail: String, output: String? = null) = AgentFeedItem.Tool(
        id = id,
        name = "Bash",
        detail = detail,
        seq = seq,
        callId = "c$id",
        toolKind = "execute",
        settled = true,
        output = output,
    )

    private val feed = listOf(
        AgentFeedItem.UserMessage(id = 1, text = "Profile the cold start", seq = 1),
        tool(2, 2, "rg \"snapshot cache\" apps/android"),
        tool(3, 3, "./gradlew :app:benchmark", output = "cache warmed\nCache hits: 4"),
        tool(4, 4, "ls"),
        AgentFeedItem.Narration(id = 5, text = "Added a board snapshot **cache**. The cache is off.", seq = 5),
        AgentFeedItem.UserMessage(id = 6, text = "Keep the cache behind a flag please.", seq = 6),
        AgentFeedItem.Thought(id = 7, thoughtId = "t7", ended = true, seq = 7),
    )

    @Test
    fun `keys are the events' and a seq-less or repeated item gets its own`() {
        val keys = transcriptFindKeys(
            listOf(
                AgentFeedItem.Narration(id = 1, text = "a", seq = 9),
                AgentFeedItem.Narration(id = 2, text = "b", seq = 9),
                AgentFeedItem.Narration(id = 3, text = "c"),
            ),
            "s1",
        )
        assertEquals("e:s1:9", keys[1])
        assertEquals("e:s1:9#2", keys[2])
        assertEquals("i:s1:3", keys[3])
        assertEquals(9L, FindInChat.eventSeq(keys.getValue(2)))
    }

    @Test
    fun `rows count bubbles and tool titles, an output stop, a fold's group, never thoughts`() {
        val rows = transcriptFindRows(feed, "s1", "cache", countMarkdown = { text -> Regex("cache", RegexOption.IGNORE_CASE).findAll(text).count() })
        val byKey = rows.associateBy { it.row.key }
        assertEquals(0, byKey.getValue("e:s1:1").row.count)
        assertEquals(1, byKey.getValue("e:s1:2").row.count)
        val bench = byKey.getValue("e:s1:3")
        assertEquals(0, bench.row.count)
        assertTrue(bench.row.outputHit)
        assertEquals(2, bench.outputCount)
        assertEquals(2, byKey.getValue("e:s1:5").row.count)
        assertEquals(1, byKey.getValue("e:s1:6").row.count)
        assertFalse(byKey.containsKey("e:s1:7"))
        // The tool calls fold the way the transcript folds them.
        val run = groupFeedRows(feed).filterIsInstance<AgentFeedRow.ToolRun>().firstOrNull()
        if (run != null) {
            assertEquals(findGroupKey(run.items), byKey.getValue("e:s1:2").row.group)
        }
        // Top to bottom: the stops of the whole loaded feed.
        val stops = FindInChat.stops(rows.map { it.row }, emptyList())
        assertEquals(listOf("e:s1:2", "e:s1:3", "e:s1:5", "e:s1:5", "e:s1:6"), stops.map { it.key })
        assertTrue(stops[1].output)
    }

    @Test
    fun `the genUI markdown count reads the painted text`() {
        assertEquals(2, findMarkdownCount("Added a **cache**. The Cáche is off.", "cache"))
        assertEquals(0, findMarkdownCount("```\ncache\n```", "cache"))
        assertEquals(0, findMarkdownCount("   ", "cache"))
    }

    @Test
    fun `a blank query counts nothing`() {
        assertTrue(transcriptFindRows(feed, "s1", "").isEmpty())
    }

    @Test
    fun `a person's bubble searches its prose, never image lines`() {
        assertEquals("hello", findUserMessageProse("hello"))
        assertTrue(findToolTitle(tool(9, 9, "ls -la"), live = false).orEmpty().contains("ls -la"))
    }

    @Test
    fun `a row holds its items' keys`() {
        val keys = transcriptFindKeys(feed, "s1")
        assertTrue(findRowHolds(AgentFeedRow.Single(feed[0]), "e:s1:1", keys))
        assertFalse(findRowHolds(AgentFeedRow.Single(feed[0]), "e:s1:2", keys))
        val run = AgentFeedRow.ToolRun(listOf(feed[1] as AgentFeedItem.Tool, feed[2] as AgentFeedItem.Tool))
        assertTrue(findRowHolds(run, "e:s1:3", keys))
    }

    @Test
    fun `the paint marks the current stop's row only`() {
        val paint = FindPaint("cache", currentKey = "e:s1:5", currentLocal = 1, outputHits = mapOf("e:s1:3" to 2), keys = transcriptFindKeys(feed, "s1"))
        assertEquals(1, paint.highlightOf(5)?.current)
        assertEquals(-1, paint.highlightOf(6)?.current)
        assertEquals("cache", paint.highlightOf(6)?.query)
        assertEquals("2 matches in output", paint.outputHint(3))
        assertNull(paint.outputHint(2))
        assertNull(paint.highlight(null))
    }

    private fun bubble(text: String, messageId: String? = null, clientId: String? = null) = MessageDelivery.Bubble(
        key = messageId?.let { "own-row-$it" } ?: "own-entry-$clientId",
        text = text,
        createdAt = "2026-11-19T10:00:00.000Z",
        state = if (messageId != null) MessageDelivery.State.Sent else MessageDelivery.State.Sending,
        stored = messageId != null,
        clientId = clientId,
        messageId = messageId,
    )

    private val countCache: (String) -> Int = { text -> Regex("cache", RegexOption.IGNORE_CASE).findAll(text).count() }

    @Test
    fun `EXP-1975 synced tail rows are keyed by their message`() {
        val keys = transcriptFindKeys(
            listOf(
                AgentFeedItem.Narration(id = 1, text = "a", seq = 4),
                AgentFeedItem.Narration(id = 2, text = "b", messageId = "synced:a1"),
                AgentFeedItem.UserMessage(id = 3, text = "c", messageId = "synced:u1"),
                AgentFeedItem.Narration(id = 4, text = "d", messageId = "live-msg"),
            ),
            "s1",
        )
        assertEquals("e:s1:4", keys[1])
        assertEquals("m:a1", keys[2])
        assertEquals("m:u1", keys[3])
        assertEquals("i:s1:4", keys[4])
    }

    @Test
    fun `EXP-1975 the own tail counts after the feed's rows, outbox entries as o keys`() {
        val synced = listOf(
            AgentFeedItem.Narration(id = 10, text = "The cache is warm.", messageId = "synced:a1"),
        )
        val own = listOf(bubble("Drop the cache", messageId = "own1"), bubble("cache cache", clientId = "c1"), bubble("nothing", clientId = "c2"))
        val rows = transcriptFindRows(synced, "s1", "cache", ownBubbles = own, countMarkdown = countCache)
        assertEquals(listOf("m:a1", "m:own1", "o:c1", "o:c2"), rows.map { it.row.key })
        assertEquals(listOf(1, 1, 2, 0), rows.map { it.row.count })
        assertNull(rows[1].itemId)
        val stops = FindInChat.stops(rows.map { it.row }, emptyList())
        assertEquals(listOf("m:a1", "m:own1", "o:c1", "o:c1"), stops.map { it.key })
        assertEquals(listOf(0, 0, 0, 1), stops.map { it.local })
    }

    @Test
    fun `EXP-1975 a placeholder counts the sent inputs then the own tail`() {
        val inputs = listOf(bubble("Warm the cache", messageId = "in1"), bubble("Ship it", messageId = "in2"))
        val own = listOf(bubble("cache now?", messageId = "own1"), bubble("the cache", clientId = "c1"))
        val rows = transcriptFindRows(emptyList(), "s1", "cache", ownBubbles = inputs + own, countMarkdown = countCache)
        assertEquals(listOf("m:in1", "m:in2", "m:own1", "o:c1"), rows.map { it.row.key })
        assertEquals(listOf(1, 0, 1, 1), rows.map { it.row.count })
        // A bubble repeating a key already counted is one row.
        assertEquals(1, ownBubbleFindRows(listOf(inputs[0], inputs[0]), "cache", countMarkdown = countCache).size)
        assertTrue(ownBubbleFindRows(own, "").isEmpty())
    }

    @Test
    fun `EXP-1975 an own bubble paints by its key`() {
        val paint = FindPaint("cache", currentKey = "o:c1", currentLocal = 0, outputHits = emptyMap(), keys = emptyMap())
        assertEquals(0, paint.highlight(ownBubbleFindKey(bubble("cache", clientId = "c1")))?.current)
        assertEquals(-1, paint.highlight(ownBubbleFindKey(bubble("cache", messageId = "own1")))?.current)
        assertEquals("m:own1", ownBubbleFindKey(bubble("x", messageId = "own1", clientId = "c9")))
    }

    @Test
    fun `EXP-1998 the drawn synced messages widen to the landed one`() {
        val all = listOf("a", "b", "c", "d", "e")
        assertEquals(3, findWidenedSkip(all, 2, null))
        // Further back than the cap: every one from it on.
        assertEquals(1, findWidenedSkip(all, 2, "b"))
        // Inside the cap, or not a drawable one: the newest `max`.
        assertEquals(3, findWidenedSkip(all, 2, "e"))
        assertEquals(3, findWidenedSkip(all, 2, "gone"))
        assertEquals(0, findWidenedSkip(all, 9, null))
        assertEquals("u2", findMessageId("m:u2"))
        assertNull(findMessageId("e:s1:4"))
        assertNull(findMessageId("m:"))
    }

    @Test
    fun `EXP-1998 an older message stop pages in by widening, then by loading older ones`() {
        val calls = ArrayList<String>()
        fun host(synced: FindInChat.Synced?, shown: Set<String>) =
            TranscriptFindSynced(synced, shown, widen = { calls += "widen:$it" }, loadOlder = { calls += "older" })
        val more = host(FindInChat.Synced(loaded = listOf("u2", "sys1", "a9"), drawable = listOf("u2", "a9"), more = true), setOf("a9"))
        // Drawable but not shown: widen to it; shown already: nothing to page.
        assertTrue(more.canPage("u2"))
        assertFalse(more.canPage("a9"))
        more.page("u2")
        // Not loaded: older messages load while more exist.
        assertTrue(more.canPage("a0"))
        more.page("a0")
        // Loaded but never drawn (a card, a system line): no page reaches it.
        assertFalse(more.canPage("sys1"))
        assertEquals(listOf("widen:u2", "older"), calls)
        val done = host(FindInChat.Synced(loaded = listOf("u2"), drawable = listOf("u2"), more = false), setOf("u2"))
        assertFalse(done.canPage("gone"))
        done.page("gone")
        // A live transcript draws no synced messages.
        assertFalse(host(null, emptySet()).canPage("u2"))
        assertEquals(listOf("widen:u2", "older"), calls)
    }

    @Test
    fun `EXP-1998 every loaded sent input, oldest first, once each`() {
        fun message(id: String, author: String, at: String, consumed: Boolean = true, body: String = "text $id") =
            com.exponential.app.data.steer.SyncedTailMessage(id, author, if (consumed) at else null, at, null, body)
        val newest = listOf(bubble("text in8", messageId = "in8").copy(key = "in8", createdAt = "2026-01-01T00:08:00Z"))
        val loaded = listOf(
            message("in2", "user", "2026-01-01T00:02:00Z"),
            message("a3", "agent", "2026-01-01T00:03:00Z"),
            message("in4", "user", "2026-01-01T00:04:00Z", consumed = false),
            message("in5", "user", "2026-01-01T00:05:00Z", body = " "),
            message("in8", "user", "2026-01-01T00:08:00Z"),
        )
        assertEquals(listOf("in2", "in8"), sentInputBubbles(newest, loaded).map { it.key })
        assertEquals(listOf("m:in2", "m:in8"), sentInputBubbles(newest, loaded).map(::ownBubbleFindKey))
        assertEquals(newest, sentInputBubbles(newest, emptyList()))
    }
}
