package com.exponential.app.data.sealed

import com.exponential.app.data.api.HeldTreeKey
import com.exponential.app.data.api.PendingTreeKey
import com.exponential.app.data.api.PendingWrap
import com.exponential.app.data.enet.IdentitySlots
import com.exponential.app.data.push.SecretSealer
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1459: the holder pass (mine → unwrap → vault; pending → wrap) with the real eNet crypto. */
class TreeKeyHolderTest {
    private class MapSlots : IdentitySlots {
        val map = mutableMapOf<String, String>()
        override fun get(key: String) = map[key]
        override fun put(key: String, value: String): Boolean {
            map[key] = value
            return true
        }
        override fun remove(key: String) {
            map.remove(key)
        }
    }

    /** Reversible, so the vault round-trips without a Keystore. */
    private object XorSealer : SecretSealer {
        override fun seal(plain: ByteArray) = plain.map { (it.toInt() xor 0x5a).toByte() }.toByteArray()
        override fun open(blob: ByteArray) = seal(blob)
    }

    private class FakeApi : HolderApi {
        var held = listOf<HeldTreeKey>()
        var pendingPairs = listOf<PendingTreeKey>()
        val wrapped = mutableListOf<PendingWrap>()
        var failMine = false
        override suspend fun mine(installId: String): List<HeldTreeKey> = if (failMine) error("offline") else held
        override suspend fun pending(installId: String) = pendingPairs
        override suspend fun wrap(installId: String, wraps: List<PendingWrap>) {
            wrapped += wraps
        }
    }

    private val identity = ByteArray(32) { (it + 1).toByte() }
    private val installSecret by lazy { SealedFfi.installSecret(identity) }
    private val installPublic by lazy { com.exponential.app.data.enet.base64Url(com.exponential.app.data.enet.EnetFfi.x25519PublicOf(identity)) }

    @Test
    fun aPassUnwrapsWhatMineListsKeepsItAndWrapsForPendingReaders() = runBlocking {
        val tree = SealedFfi.generateTreeKey()
        val api = FakeApi()
        api.held = listOf(
            HeldTreeKey("root-1", 1, SealedFfi.wrapTreeKey(installPublic, tree.secret)),
            // Not for this install: skipped, never fatal.
            HeldTreeKey("root-2", 1, SealedFfi.wrapTreeKey(SealedFfi.generateTreeKey().publicKey, tree.secret)),
        )
        val reader = ByteArray(32) { 9 }
        val readerPublic = com.exponential.app.data.enet.base64Url(com.exponential.app.data.enet.EnetFfi.x25519PublicOf(reader))
        api.pendingPairs = listOf(
            PendingTreeKey("root-1", 1, tree.publicKey, "install-b", readerPublic),
            // A tree this install does not hold: nothing to wrap.
            PendingTreeKey("root-9", 1, tree.publicKey, "install-c", readerPublic),
        )
        val vault = TreeKeyVault(MapSlots(), XorSealer)
        val published = mutableListOf<TreeKeys>()
        val holder = TreeKeyHolder("acct", "install-a", installSecret, SealedFfi, vault, api) { published += it }

        assertTrue(holder.pass())
        assertArrayEquals(tree.secret, vault.all("acct")["root-1"]!![1])
        assertFalse(vault.all("acct").containsKey("root-2"))
        assertEquals(listOf(setOf("root-1")), published.map { it.keys })
        assertEquals(1, api.wrapped.size)
        val wrap = api.wrapped.single()
        assertEquals("install-b", wrap.targetInstallId)
        assertArrayEquals(tree.secret, SealedFfi.unwrapTreeKey(SealedFfi.installSecret(reader), wrap.wrapped))

        // A second pass unwraps nothing new.
        published.clear()
        assertTrue(holder.pass())
        assertTrue(published.isEmpty())
    }

    @Test
    fun aFailedStepReportsFalseAndTheVaultSurvivesARestart() = runBlocking {
        val slots = MapSlots()
        val api = FakeApi().apply { failMine = true }
        val holder = TreeKeyHolder("acct", "install-a", installSecret, SealedFfi, TreeKeyVault(slots, XorSealer), api) {}
        assertFalse(holder.pass())

        val tree = SealedFfi.generateTreeKey()
        TreeKeyVault(slots, XorSealer).put("acct", mapOf("root-1" to mapOf(1 to tree.secret)))
        assertArrayEquals(tree.secret, TreeKeyVault(slots, XorSealer).all("acct")["root-1"]!![1])
        TreeKeyVault(slots, XorSealer).forget("acct")
        assertTrue(TreeKeyVault(slots, XorSealer).all("acct").isEmpty())
    }
}
