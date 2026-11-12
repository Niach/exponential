package com.exponential.app.data.sealed

import com.exponential.app.data.db.MessageEntity
import com.exponential.app.domain.SessionTree
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * Large chats: a sealed row is opened ONCE per message id whatever the
 * outcome: a keyless (`noKey`) row too, which used to re-run the native open
 * on every re-emitted query, twice per row (the DAO, then
 * [SessionTree.message]). New keys for its tree, or a changed body, open it
 * again.
 */
class SealedRowCacheTest {
    private class CountingCrypto : SealedCrypto {
        var bodies = 0
        var cards = 0
        var key: ByteArray? = null
        override fun generateTreeKey(): TreeKey = error("unused")
        override fun wrapTreeKey(installPublic: String, treeSecret: ByteArray): String = error("unused")
        override fun unwrapTreeKey(installSecret: ByteArray, wrapped: String): ByteArray = error("unused")
        override fun sealBody(treePublicKey: String, epoch: Int, text: String): String = error("unused")
        override fun sealCard(treePublicKey: String, epoch: Int, cardJson: String): String = error("unused")
        override fun installSecret(identity: ByteArray): ByteArray = error("unused")

        override fun openBody(body: String, keys: Map<Int, ByteArray>): OpenedBody {
            bodies++
            return if (keys[1] != null) {
                OpenedBody(SealStatus.Opened, 1, "opened:" + body.removePrefix(SealedContent.BODY_PREFIX))
            } else {
                OpenedBody(SealStatus.NoKey, 1, null)
            }
        }

        override fun openCard(cardJson: String, keys: Map<Int, ByteArray>): OpenedCard {
            cards++
            return if (keys[1] != null) {
                OpenedCard(SealStatus.Opened, 1, """{"kind":"surface","title":"open"}""")
            } else {
                OpenedCard(SealStatus.NoKey, 1, """{"kind":"surface"}""")
            }
        }
    }

    private val crypto = CountingCrypto()
    private val root = "root-cache"

    private fun row(id: String, body: String, card: String? = null) =
        MessageEntity(id = id, rootId = root, sessionId = root, authorKind = "agent", body = body, card = card, createdAt = "2026-11-12T00:00:00.000Z")

    @Before
    fun setUp() {
        SealedContent.crypto = crypto
        SealedContent.clearCache()
        SealedKeys.drop(SealedKeys.keys.value.keys)
    }

    @After
    fun tearDown() {
        SealedContent.crypto = SealedFfi
        SealedContent.clearCache()
        SealedKeys.drop(SealedKeys.keys.value.keys)
    }

    @Test
    fun `a keyless row opens once across re-emissions and the second open`() {
        val sealed = row("m1", SealedContent.BODY_PREFIX + "abc", """{"sealed":"x","kind":"surface"}""")
        repeat(3) {
            val dao = SealedContent.message(sealed)
            assertTrue(SealedContent.isSealedBody(dao.body))
            val message = SessionTree.message(dao, root)
            assertEquals(SealedContent.LOCKED_NO_KEY, message.locked)
            assertEquals("", message.body)
        }
        assertEquals(1, crypto.bodies)
        assertEquals(1, crypto.cards)
    }

    @Test
    fun `new keys for the tree open the row again`() {
        val sealed = row("m2", SealedContent.BODY_PREFIX + "def")
        assertTrue(SealedContent.isSealedBody(SealedContent.message(sealed).body))
        SealedKeys.merge(mapOf(root to mapOf(1 to byteArrayOf(1))))
        assertEquals("opened:def", SealedContent.message(sealed).body)
        assertEquals(2, crypto.bodies)
        // Opened: the DAO's row is plain to the second open.
        val message = SessionTree.message(SealedContent.message(sealed), root)
        assertEquals("opened:def", message.body)
        assertEquals(null, message.locked)
        assertEquals(2, crypto.bodies)
    }

    @Test
    fun `a changed body under the same id opens again`() {
        SealedContent.message(row("m3", SealedContent.BODY_PREFIX + "one"))
        SealedContent.message(row("m3", SealedContent.BODY_PREFIX + "two"))
        assertEquals(2, crypto.bodies)
    }

    @Test
    fun `a plain row never reaches the crypto`() {
        val plain = row("m4", "hello", """{"kind":"surface"}""")
        assertTrue(plain === SealedContent.message(plain))
        assertEquals("hello", SessionTree.message(plain, root).body)
        assertEquals(0, crypto.bodies + crypto.cards)
    }
}
