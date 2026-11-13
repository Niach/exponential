package com.exponential.app.data.db

import com.exponential.app.data.sealed.CharBoundedLru
import com.exponential.app.data.sealed.OpenedBody
import com.exponential.app.data.sealed.OpenedCard
import com.exponential.app.data.sealed.SealStatus
import com.exponential.app.data.sealed.SealedContent
import com.exponential.app.data.sealed.SealedCrypto
import com.exponential.app.data.sealed.SealedFfi
import com.exponential.app.data.sealed.SealedKeys
import com.exponential.app.data.sealed.TreeKey
import com.exponential.app.data.steer.detachedIdsToClose
import com.exponential.app.domain.SessionTitle
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * #ERR-19 heap guard: a production Assistant chat (one root receiving every
 * child's report, ask and surface card, hundreds of `thread_started` cards
 * naming other roots) ran a Pixel 7 out of its 256 MB heap seconds after the
 * chat opened. These drive a synthetic tree of that shape through the fixed
 * paths and pin that what they KEEP is bounded by what the screen shows, not
 * by the tree's history.
 */
class LargeTreeHeapGuardTest {

    /** Opens `exp-sealed:<text>` to `<text>` (and a sealed card to its `sealed` text) while key 1 is held. */
    private class EchoCrypto : SealedCrypto {
        var opens = 0
        override fun generateTreeKey(): TreeKey = error("unused")
        override fun wrapTreeKey(installPublic: String, treeSecret: ByteArray): String = error("unused")
        override fun unwrapTreeKey(installSecret: ByteArray, wrapped: String): ByteArray = error("unused")
        override fun sealBody(treePublicKey: String, epoch: Int, text: String): String = error("unused")
        override fun sealCard(treePublicKey: String, epoch: Int, cardJson: String): String = error("unused")
        override fun installSecret(identity: ByteArray): ByteArray = error("unused")
        override fun openBody(body: String, keys: Map<Int, ByteArray>): OpenedBody {
            opens++
            return if (keys[1] != null) OpenedBody(SealStatus.Opened, 1, body.removePrefix(SealedContent.BODY_PREFIX)) else OpenedBody(SealStatus.NoKey, 1, null)
        }
        override fun openCard(cardJson: String, keys: Map<Int, ByteArray>): OpenedCard {
            opens++
            return if (keys[1] != null) {
                OpenedCard(SealStatus.Opened, 1, cardJson.replace("\"sealed\":\"", "\"plan\":\""))
            } else {
                OpenedCard(SealStatus.NoKey, 1, """{"kind":"plan"}""")
            }
        }
    }

    private val crypto = EchoCrypto()
    private val root = "root-assistant"

    @Before
    fun setUp() {
        SealedContent.crypto = crypto
        SealedContent.clearCache()
        SealedKeys.drop(SealedKeys.keys.value.keys)
        SealedKeys.merge(mapOf(root to mapOf(1 to byteArrayOf(1))))
    }

    @After
    fun tearDown() {
        SealedContent.crypto = SealedFfi
        SealedContent.clearCache()
        SealedKeys.drop(SealedKeys.keys.value.keys)
    }

    private fun ts(i: Int) = "2026-11-12T%02d:%02d:%02d.000Z".format(i / 3600 % 24, i / 60 % 60, i % 60)

    private val plan = "x".repeat(8_000)

    /** ~3k own cards of the tree: plans + asks (8 KB, sealed) and 1 in 10 a result surface. */
    private fun ownCards(n: Int): List<MessageEntity> = (0 until n).map { i ->
        val session = "child-${i % 300}"
        val card = if (i % 10 == 0) {
            """{"kind":"surface","result":true,"title":"Result $i","surface":{"root":{"id":"s","component":"Stack"}}}"""
        } else {
            """{"kind":"plan","sealed":"$plan"}"""
        }
        MessageEntity(
            id = "card-$i", sessionId = session, rootId = root, authorKind = "agent", authorSessionId = session,
            body = SealedContent.BODY_PREFIX + plan, card = card, createdAt = ts(i), updatedAt = ts(i),
        )
    }

    @Test
    fun `the results fold keeps only result cards and loads only what changed`() = runBlocking {
        val rows = ownCards(3_000)
        val byId = rows.associateBy { it.id }
        val fold = ResultCardsFold()
        val keys = SealedKeys.keys.value
        var stamps = rows.map { RowStamp(it.id, it.updatedAt) }
        var loads = 0
        val load: suspend (List<String>) -> List<MessageEntity> = { ids ->
            assertTrue("loaded in pages", ids.size <= ResultCardsFold.LOAD_PAGE)
            loads += ids.size
            ids.map { byId.getValue(it) }
        }
        val first = fold.update(stamps, keys, load)
        assertEquals(300, first.size)
        assertTrue(first.all { it.card?.get("kind").toString() == "\"surface\"" })
        assertEquals(3_000, loads)

        // A trickle: one new row (a plan) is the only one read.
        val extra = ownCards(3_002).last()
        val all = byId + (extra.id to extra)
        loads = 0
        stamps = stamps + RowStamp(extra.id, extra.updatedAt)
        val second = fold.update(stamps, keys) { ids -> loads += ids.size; ids.map { all.getValue(it) } }
        assertEquals(1, loads)
        assertEquals(300, second.size)
        // Unchanged results keep their instance (no re-parse).
        assertTrue(first.zip(second).all { (a, b) -> a === b })

        // New keys re-open every row once.
        SealedKeys.merge(mapOf("other-root" to mapOf(1 to byteArrayOf(2))))
        loads = 0
        fold.update(stamps, SealedKeys.keys.value) { ids -> loads += ids.size; ids.map { all.getValue(it) } }
        assertEquals(3_001, loads)
    }

    @Test
    fun `titles read only each session's first usable prompt`() = runBlocking {
        // 800 other roots, 50 inbound briefs of 2 KB each.
        val brief = "brief line\n" + "y".repeat(2_000)
        val sessions = (0 until 800).map { "other-$it" }
        val rowsBySession = sessions.associateWith { s ->
            (0 until 50).map { i ->
                // other-0 opens with image-only rows: its title is its 4th row.
                val body = if (s == "other-0" && i < 3) "![shot](/api/attachments/a$i)" else brief
                MessageEntity(id = "$s-$i", sessionId = s, rootId = s, authorKind = "user", body = body, createdAt = ts(i), updatedAt = ts(i))
            }
        }
        var read = 0
        val fold = FirstTitleRows()
        val stamps = sessions.map { TitleStamp(it, "50|x") }
        val page: suspend (String, Int, Int) -> List<MessageEntity> = { s, limit, offset ->
            rowsBySession.getValue(s).drop(offset).take(limit).also { read += it.size }
        }
        val out = fold.update(stamps, SealedKeys.keys.value, page)
        // One row per session, + the three image rows before other-0's prompt.
        assertEquals(803, out.size)
        assertTrue("read ${read} rows", read <= 800 * FirstTitleRows.FIRST_PAGE)
        assertEquals(listOf("other-0-0", "other-0-1", "other-0-2", "other-0-3"), out.filter { it.sessionId == "other-0" }.map { it.id })
        // The title is the same one the whole transcript gives.
        val whole = rowsBySession.getValue("other-0").map { titleMessage(it) }
        val kept = out.filter { it.sessionId == "other-0" }.map { titleMessage(it) }
        assertEquals(SessionTitle.firstPrompt("other-0", whole), SessionTitle.firstPrompt("other-0", kept))

        // An unchanged stamp reads nothing again.
        read = 0
        fold.update(stamps, SealedKeys.keys.value, page)
        assertEquals(0, read)
    }

    private fun titleMessage(row: MessageEntity) = SessionTitle.TitleMessage(
        id = row.id, sessionId = row.sessionId!!, authorKind = row.authorKind, authorSessionId = row.authorSessionId,
        hasCard = row.card != null, body = row.body, createdAt = row.createdAt,
    )

    @Test
    fun `the sealed caches stay character-bounded over a long tree`() {
        // 15k sealed rows of 2–10 KB opened one after another (a long tree read
        // by the chat, its results, its titles).
        for (i in 0 until 15_000) {
            val text = "z".repeat(2_000 + (i % 5) * 2_000) + i
            SealedContent.message(
                MessageEntity(id = "m$i", sessionId = root, rootId = root, authorKind = "agent", body = SealedContent.BODY_PREFIX + text, createdAt = ts(i)),
            )
        }
        val (content, rows, sessions) = SealedContent.cachedChars()
        assertTrue("content cache $content chars", content <= SealedContent.CACHE_MAX_CHARS)
        assertTrue("row cache $rows chars", rows <= SealedContent.ROWS_MAX_CHARS)
        assertTrue(sessions <= SealedContent.SESSIONS_MAX_CHARS)
        // A cache key never copies the ciphertext.
        assertTrue(SealedContent.contentKey('b', root, SealedContent.BODY_PREFIX + plan).length < 120)
    }

    @Test
    fun `the char-bounded LRU evicts the eldest and skips an oversized value`() {
        val lru = CharBoundedLru<String>(maxEntries = 100, maxChars = 1_000) { it.length }
        repeat(20) { lru["k$it"] = "v".repeat(100) }
        assertTrue(lru.chars <= 1_000)
        assertEquals(10, lru.size)
        assertEquals(null, lru["k0"])
        lru["huge"] = "h".repeat(600)
        assertEquals(null, lru["huge"])
        assertTrue(lru.chars <= 1_000)
    }

    @Test
    fun `only the newest detached steer connections stay open`() {
        val detached = (1..6).associate { "s$it" to it * 1_000L }
        assertEquals(listOf("s4", "s3", "s2", "s1"), detachedIdsToClose(detached, keep = 2))
        assertEquals(emptyList<String>(), detachedIdsToClose(mapOf("a" to 1L), keep = 2))
    }
}
