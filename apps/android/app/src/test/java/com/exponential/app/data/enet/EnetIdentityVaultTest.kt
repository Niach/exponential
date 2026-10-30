package com.exponential.app.data.enet

import com.exponential.app.data.push.SecretSealer
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1457: identity generation + persistence, with the Keystore swapped for a fake sealer. */
class EnetIdentityVaultTest {
    private class MapSlots : IdentitySlots {
        val map = mutableMapOf<String, String>()
        override fun get(key: String) = map[key]
        override fun put(key: String, value: String): Boolean { map[key] = value; return true }
        override fun remove(key: String) { map.remove(key) }
    }

    /** XOR "sealing" with a key that can be rotated (= the Keystore key was lost). */
    private class FakeSealer(var key: Byte = 0x5a) : SecretSealer {
        var working = true
        override fun seal(plain: ByteArray) = if (working) byteArrayOf(key) + plain.map { (it.toInt() xor key.toInt()).toByte() } else null
        override fun open(blob: ByteArray) =
            if (blob.isEmpty() || blob[0] != key) null else blob.drop(1).map { (it.toInt() xor key.toInt()).toByte() }.toByteArray()
    }

    private var generated = 0
    private fun generate() = ByteArray(32) { (it + 1 + 32 * generated).toByte() }.also { generated++ }

    @Test
    fun generatesOnceAndPersistsSealedAcrossInstances() {
        val slots = MapSlots()
        val sealer = FakeSealer()
        val first = EnetIdentityVault(slots, sealer, ::generate).identity("acct-1")!!
        assertEquals(32, first.size)
        // The slot never holds the raw identity.
        val stored = java.util.Base64.getDecoder().decode(slots.map["identity.acct-1"]!!)
        assertFalse(stored.copyOfRange(1, stored.size).contentEquals(first))

        val reopened = EnetIdentityVault(slots, sealer, ::generate).identity("acct-1")!!
        assertArrayEquals(first, reopened)
        assertEquals(1, generated)
    }

    @Test
    fun oneIdentityPerAccount() {
        val vault = EnetIdentityVault(MapSlots(), FakeSealer(), ::generate)
        val a = vault.identity("acct-a")!!
        val b = vault.identity("acct-b")!!
        assertFalse(a.contentEquals(b))
        assertArrayEquals(a, vault.identity("acct-a"))
    }

    @Test
    fun anUnopenableSlotIsReplacedByAFreshIdentity() {
        val slots = MapSlots()
        val sealer = FakeSealer()
        val first = EnetIdentityVault(slots, sealer, ::generate).identity("acct-1")!!
        sealer.key = 0x11 // the Keystore key is gone (restored onto another device)
        val second = EnetIdentityVault(slots, sealer, ::generate).identity("acct-1")!!
        assertFalse(first.contentEquals(second))
        assertArrayEquals(second, EnetIdentityVault(slots, sealer, ::generate).identity("acct-1"))
    }

    @Test
    fun notStoredMeansNoIdentity() {
        val sealer = FakeSealer().apply { working = false }
        assertNull(EnetIdentityVault(MapSlots(), sealer, ::generate).identity("acct-1"))
    }

    @Test
    fun forgetAndPeek() {
        val slots = MapSlots()
        val vault = EnetIdentityVault(slots, FakeSealer(), ::generate)
        assertNull(vault.identity("acct-1", create = false))
        val first = vault.identity("acct-1")!!
        assertNotNull(vault.identity("acct-1", create = false))
        vault.forget("acct-1")
        assertTrue(slots.map.isEmpty())
        assertNull(vault.identity("acct-1", create = false))
        assertFalse(first.contentEquals(vault.identity("acct-1")!!))
    }
}
