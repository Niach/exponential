package com.exponential.app.data.api

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The 426 gate is keyed PER INSTANCE (REV2-18): one server demanding a version
 * this build can't reach must never block the accounts on every other server.
 * These tests pin both halves of that — the origin key both sides of the join
 * compute, and the per-origin latch semantics.
 */
class UpdateGateTest {

    private val info = UpdateGate.UpgradeInfo(min = "0.14.0", latest = "0.14.2")

    // MARK: - originKey

    @Test
    fun `strips path query and fragment`() {
        assertEquals(
            "https://app.exponential.at",
            UpdateGate.originKey("https://app.exponential.at/api/shapes/issues?offset=-1"),
        )
    }

    @Test
    fun `matches a stored instance url against a response url`() {
        assertEquals(
            UpdateGate.originKey("https://app.exponential.at"),
            UpdateGate.originKey("https://APP.Exponential.at/api/trpc/issues.list"),
        )
    }

    @Test
    fun `keeps a non-default port and drops the default one`() {
        assertEquals("http://10.0.2.2:5173", UpdateGate.originKey("http://10.0.2.2:5173/api"))
        assertEquals("https://exp.internal", UpdateGate.originKey("https://exp.internal:443/x"))
        assertEquals("http://exp.internal", UpdateGate.originKey("http://exp.internal:80"))
    }

    @Test
    fun `distinguishes servers that differ only by port or scheme`() {
        val a = UpdateGate.originKey("http://exp.internal:8080")
        val b = UpdateGate.originKey("http://exp.internal:9090")
        val c = UpdateGate.originKey("https://exp.internal:8080")
        assertTrue(a != b && a != c)
    }

    @Test
    fun `assumes https for a scheme-less host, matching AuthRepository`() {
        assertEquals(
            UpdateGate.originKey("https://exp.internal"),
            UpdateGate.originKey("exp.internal"),
        )
    }

    @Test
    fun `keeps ipv6 literals intact`() {
        assertEquals("http://[::1]:5173", UpdateGate.originKey("http://[::1]:5173/api"))
        assertEquals("https://[::1]", UpdateGate.originKey("https://[::1]/api"))
    }

    @Test
    fun `drops userinfo`() {
        assertEquals(
            "https://exp.internal",
            UpdateGate.originKey("https://user:pass@exp.internal/api"),
        )
    }

    @Test
    fun `rejects unusable input`() {
        assertNull(UpdateGate.originKey(""))
        assertNull(UpdateGate.originKey("   "))
        assertNull(UpdateGate.originKey("https:///api"))
        assertNull(UpdateGate.originKey("https://exp.internal:notaport"))
    }

    // MARK: - latch

    private fun UpdateGate.at(url: String): UpdateGate.UpgradeInfo? =
        UpdateGate.originKey(url)?.let { gated.value[it] }

    @Test
    fun `latches only the origin that answered 426`() {
        val gate = UpdateGate()
        gate.trigger("https://self.hosted/api/shapes/issues", info)

        assertEquals(info, gate.at("https://self.hosted"))
        assertNull(gate.at("https://app.exponential.at"))
    }

    @Test
    fun `keeps the first signal per origin`() {
        val gate = UpdateGate()
        gate.trigger("https://self.hosted", info)
        gate.trigger("https://self.hosted/api/trpc/x", UpdateGate.UpgradeInfo(null, null))

        assertEquals(info, gate.at("https://self.hosted"))
    }

    @Test
    fun `clear releases one origin without touching the others`() {
        val gate = UpdateGate()
        gate.trigger("https://self.hosted", info)
        gate.trigger("https://other.hosted", info)

        gate.clear("https://self.hosted/")

        assertNull(gate.at("https://self.hosted"))
        assertEquals(info, gate.at("https://other.hosted"))
    }

    @Test
    fun `ignores a 426 from an unparseable url`() {
        val gate = UpdateGate()
        gate.trigger("", info)
        assertTrue(gate.gated.value.isEmpty())
    }
}
