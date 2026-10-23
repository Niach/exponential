package com.exponential.app.data.db

import kotlinx.serialization.json.Json
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The sessions shape row: NEW-8's dropped legacy columns are tolerated
 *  while today's server still sends them, and never needed. */
class SessionEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `the legacy columns decode present or absent`() {
        val today = json.decodeFromString(
            SessionEntity.serializer(),
            """{"id":"s-1","root_id":"s-1","kind":"agent","dispatch":"t","plan_mode":"f","caption":"Reading",""" +
                """"aside":"f","move_to_device_id":"mac","handoff_restored_at":null,"action_id":"a-1",""" +
                """"action_name":"Nightly","action_inputs":{"repo":"r-1"},"spawned_by_session_id":"s-0",""" +
                """"name":"Scout","icon":"bot","color":"blue","triggers":[],"sort_order":2}""",
        )
        assertFalse(today.planMode)
        org.junit.Assert.assertEquals("s-1", today.rootId)
        val afterDrop = json.decodeFromString(SessionEntity.serializer(), """{"id":"s-2","root_id":"s-2"}""")
        org.junit.Assert.assertEquals(today.copy(id = "s-2", rootId = "s-2"), afterDrop)
    }

    /** NEW-2: `agent_id` = the agent whose conversation the root is; the kind
     *  derives from it ([com.exponential.app.domain.SessionKind.sessionKindOf]). */
    @Test
    fun `agent_id decodes and drives isAgent, absent = a thread`() {
        val conversation = json.decodeFromString(
            SessionEntity.serializer(),
            """{"id":"s-1","root_id":"s-1","kind":"agent","agent_id":"ag-1"}""",
        )
        assertTrue(conversation.isAgent)
        org.junit.Assert.assertEquals("ag-1", conversation.agentId)
        // The legacy `kind` shim alone never makes an agent.
        val legacy = json.decodeFromString(SessionEntity.serializer(), """{"id":"s-2","kind":"agent"}""")
        assertFalse(legacy.isAgent)
        val camel = json.decodeFromString(SessionEntity.serializer(), """{"id":"s-3","agentId":"ag-2"}""")
        org.junit.Assert.assertEquals("ag-2", camel.agentId)
    }
}
