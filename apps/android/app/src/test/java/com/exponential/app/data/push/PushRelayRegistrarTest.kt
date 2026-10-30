package com.exponential.app.data.push

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1435: one install per relay, one capability per (relay, account), every failure = null. */
class PushRelayRegistrarTest {
    private val relay = "https://push.example"

    private class MemoryStore : RelayInstallStore {
        var installs: Map<String, RelayInstallRecord> = emptyMap()
        override fun load() = installs
        override fun save(installs: Map<String, RelayInstallRecord>) {
            this.installs = installs
        }
    }

    private class FakeOps : PushRelayOps {
        val calls = mutableListOf<String>()
        var next = 0
        var create: RelayResult<RelayInstallCreds>? = null
        var update: RelayResult<Unit> = RelayResult.Ok(Unit)
        var list: RelayResult<Set<String>>? = null
        var mint: RelayResult<RelayCapabilityRecord>? = null
        val live = mutableSetOf<String>()

        override suspend fun createInstall(relayUrl: String, token: String): RelayResult<RelayInstallCreds> {
            calls += "create $token"
            return create ?: RelayResult.Ok(RelayInstallCreds("ins_${++next}", "sec_$next"))
        }

        override suspend fun updateInstall(relayUrl: String, install: RelayInstallRecord, token: String): RelayResult<Unit> {
            calls += "update ${install.installId} $token"
            return update
        }

        override suspend fun listCapabilities(relayUrl: String, install: RelayInstallRecord): RelayResult<Set<String>> {
            calls += "list ${install.installId}"
            return list ?: RelayResult.Ok(live.toSet())
        }

        override suspend fun mintCapability(
            relayUrl: String,
            install: RelayInstallRecord,
            label: String,
        ): RelayResult<RelayCapabilityRecord> {
            calls += "mint ${install.installId} $label"
            return mint ?: RelayResult.Ok(RelayCapabilityRecord("cid_${++next}", "cap_$next").also { live += it.id })
        }

        override suspend fun revokeCapability(
            relayUrl: String,
            install: RelayInstallRecord,
            capabilityId: String,
        ): RelayResult<Unit> {
            calls += "revoke ${install.installId} $capabilityId"
            live -= capabilityId
            return RelayResult.Ok(Unit)
        }
    }

    private val ops = FakeOps()
    private val store = MemoryStore()
    private fun registrar() = PushRelayRegistrar(ops, store)

    @Test
    fun registersOneInstallAndOneCapabilityPerAccount() = runBlocking {
        val r = registrar()
        assertEquals("cap_2", r.capabilityFor(relay, "a", "tok-1", "app.exponential.at"))
        assertEquals("cap_3", r.capabilityFor(relay, "b", "tok-1", "other.host"))
        // Reused, not re-minted.
        assertEquals("cap_2", r.capabilityFor(relay, "a", "tok-1", "app.exponential.at"))
        assertEquals(
            listOf("create tok-1", "mint ins_1 app.exponential.at", "mint ins_1 other.host"),
            ops.calls,
        )
        assertEquals(setOf("a", "b"), store.installs[relay]!!.capabilities.keys)
    }

    @Test
    fun aNewTokenRotatesTheInstall() = runBlocking {
        val r = registrar()
        r.capabilityFor(relay, "a", "tok-1", "h")
        assertEquals("cap_2", r.capabilityFor(relay, "a", "tok-2", "h"))
        assertEquals("update ins_1 tok-2", ops.calls[2])
        assertEquals("tok-2", store.installs[relay]!!.token)
    }

    @Test
    fun aGoneInstallIsForgottenWithItsCapabilitiesAndRegisteredAgain() = runBlocking {
        val r = registrar()
        r.capabilityFor(relay, "a", "tok-1", "h")
        ops.update = RelayResult.Gone
        assertEquals("cap_4", r.capabilityFor(relay, "a", "tok-2", "h"))
        assertEquals(listOf("update ins_1 tok-2", "create tok-2", "mint ins_3 h"), ops.calls.drop(2))
        assertEquals("ins_3", store.installs[relay]!!.installId)
    }

    @Test
    fun theStoredInstallIsCheckedOncePerProcess() = runBlocking {
        registrar().capabilityFor(relay, "a", "tok-1", "h")
        ops.calls.clear()
        // A new process: the relay lost the capability (or the install).
        ops.live.clear()
        val r = registrar()
        assertEquals("cap_3", r.capabilityFor(relay, "a", "tok-1", "h"))
        assertEquals(listOf("list ins_1", "mint ins_1 h"), ops.calls)
        r.capabilityFor(relay, "a", "tok-1", "h")
        assertEquals(2, ops.calls.size)

        ops.calls.clear()
        ops.list = RelayResult.Gone
        assertEquals("cap_5", registrar().capabilityFor(relay, "a", "tok-1", "h"))
        assertEquals(listOf("list ins_1", "create tok-1", "mint ins_4 h"), ops.calls)
    }

    @Test
    fun relayFailuresAnswerNullAndKeepWhatWorked() = runBlocking {
        ops.create = RelayResult.Failed("relay register HTTP 503")
        assertNull(registrar().capabilityFor(relay, "a", "tok-1", "h"))
        assertTrue(store.installs.isEmpty())

        ops.create = null
        ops.mint = RelayResult.Failed("relay mint HTTP 500")
        val r = registrar()
        assertNull(r.capabilityFor(relay, "a", "tok-1", "h"))
        // The install survived; the next pass only mints.
        assertEquals("ins_1", store.installs[relay]!!.installId)
        ops.mint = null
        ops.calls.clear()
        assertEquals("cap_2", r.capabilityFor(relay, "a", "tok-1", "h"))
        assertEquals(listOf("mint ins_1 h"), ops.calls)

        ops.mint = RelayResult.Gone
        assertNull(r.capabilityFor(relay, "b", "tok-1", "h"))
        assertFalse(relay in store.installs)
    }

    @Test
    fun labelsAreCappedAt64Chars() = runBlocking {
        registrar().capabilityFor(relay, "a", "tok-1", "x".repeat(100))
        assertEquals("mint ins_1 ${"x".repeat(64)}", ops.calls[1])
    }

    @Test
    fun revokeDropsOnlyThatAccountsCapabilities() = runBlocking {
        val r = registrar()
        r.capabilityFor(relay, "a", "tok-1", "h")
        r.capabilityFor(relay, "b", "tok-1", "h")
        r.capabilityFor("https://other.relay", "a", "tok-1", "h")
        ops.calls.clear()
        r.revoke("a")
        assertEquals(listOf("revoke ins_1 cid_2", "revoke ins_4 cid_5"), ops.calls)
        assertEquals(setOf("b"), store.installs[relay]!!.capabilities.keys)
        assertTrue(store.installs["https://other.relay"]!!.capabilities.isEmpty())
        // Nothing left to revoke.
        ops.calls.clear()
        r.revoke("a")
        assertTrue(ops.calls.isEmpty())
    }
}
