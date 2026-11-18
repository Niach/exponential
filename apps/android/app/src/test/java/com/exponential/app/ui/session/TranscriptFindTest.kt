package com.exponential.app.ui.session

import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentFeedRow
import com.exponential.app.domain.FindInChat
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
}
