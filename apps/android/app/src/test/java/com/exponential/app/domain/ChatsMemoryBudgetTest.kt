package com.exponential.app.domain

import com.exponential.app.data.db.MappedRows
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #ERR-28 memory budget: the Chats list's inputs on a large account (the
 * #ERR-23 seed's shape: ~2k sessions, ~600 roots, ~50k digest rows) re-run on
 * every synced write. What one pass may allocate is bounded here, so churn
 * that ran a 256 MB phone heap out fails CI first.
 */
class ChatsMemoryBudgetTest {

    private val sessionCount = 2_000
    private val roots = 600
    private val agentTimes = 24

    private fun stamp(i: Int) = "2026-11-%02d %02d:%02d:%02d.%06d+00".format(1 + i / 86_400 % 28, i / 3600 % 24, i / 60 % 60, i % 60, i % 1_000_000)

    private fun digest(id: String, i: Int): String {
        val times = (0 until agentTimes).joinToString(",") { "\"${stamp(i * 50 + it)}\"" }
        return """{"first":{"id":"$id-m0","authorKind":"user","body":"Fix the flaky test number $i\nwith some more words","createdAt":"${stamp(i * 50)}"},""" +
            """"preview":{"id":"$id-m1","authorKind":"agent","body":"Done: opened the PR for $i","createdAt":"${stamp(i * 50 + 49)}"},""" +
            """"waits":[],"agentTimes":[$times],"agentAt":"${stamp(i * 50 + 49)}"}"""
    }

    private val ids = List(sessionCount) { "s$it" }
    private val rootOf = List(sessionCount) { if (it < roots) ids[it] else ids[it % roots] }
    private val digests = ids.mapIndexed { i, id -> id to digest(id, i) }

    private val sessions = ids.mapIndexed { i, id ->
        SessionTree.Session(
            id = id,
            teamId = "team-${i % 7}",
            parentId = if (i < roots) null else rootOf[i],
            rootId = rootOf[i],
            userId = "me",
            status = if (i % 5 == 0) "ended" else "idle",
            busy = false,
            needsInput = false,
            blocked = null,
            shared = true,
            title = if (i % 2 == 0) null else "Named $i",
            guide = null,
            startedAt = stamp(i * 50),
            endedAt = null,
            createdAt = stamp(i * 50),
            updatedAt = stamp(i * 50 + 49),
        )
    }

    @Test
    fun anUnchangedDigestIsNeverParsedAgain() {
        val cache = Chats.DigestMessagesCache()
        val first = cache.read(digests)
        assertEquals(sessionCount, cache.parsed)
        val changed = digests.mapIndexed { i, (id, d) -> if (i == 7) id to d.replace("Done:", "Still done:") else id to d }
        val (bytes, second) = allocatedBytes { cache.read(changed) }
        assertEquals("one write = one row parsed again", sessionCount + 1, cache.parsed)
        assertSame(first.getValue("s0"), second.getValue("s0"))
        assertTrue(first.getValue("s7") !== second.getValue("s7"))
        // Was a full re-parse of every digest (~2k JSON trees + ~50k rows): tens of MB.
        assertTrue("a one-row re-read allocated $bytes bytes", bytes < 1_500_000L)
    }

    @Test
    fun unchangedRowsKeepTheirMappedInstance() {
        val cache = MappedRows<Pair<String, String>, String>({ it.first }) { it.second.uppercase() }
        val first = cache.read(digests)
        val second = cache.read(digests.map { (id, d) -> id to String(d.toCharArray()) })
        assertEquals(sessionCount, cache.mapped)
        first.indices.forEach { assertSame(first[it], second[it]) }
    }

    @Test
    fun titlingEveryChatIndexesTheRowsOnce() {
        val messages = Chats.DigestMessagesCache().read(digests).values.flatten()
        assertTrue(messages.size > sessionCount * agentTimes)
        val links = List(400) { SessionTree.SessionIssue(sessionId = ids[it * 2], issueId = "i$it", sortOrder = it.toDouble()) }
        val issues = List(3_400) { SessionTree.Issue(id = "i$it", identifier = "EXP-$it", title = "Issue $it") }
        // Warm up (class loading, regexes).
        SessionTree.sessionTitles(sessions, links, issues, messages)
        val (bytes, titles) = allocatedBytes { SessionTree.sessionTitles(sessions, links, issues, messages) }
        assertEquals("Fix the flaky test number 1000", titles.getValue("s1000"))
        assertEquals("#EXP-1 Issue 1", titles.getValue("s2"))
        assertEquals("#EXP-0 Issue 0", titles.getValue("s0"))
        assertEquals("Named 1", titles.getValue("s1"))
        // Was every row converted per call (and, per chat row, re-scanned): ~50k
        // TitleMessage + every issue mapped, several MB per pass.
        assertTrue("titling $sessionCount sessions allocated $bytes bytes", bytes < 3_000_000L)
        println("ERR-28 budget: sessionTitles over ${messages.size} digest rows allocated $bytes bytes")
    }
}
