package com.exponential.app.ui.session

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentLaunchDefaults
import com.exponential.app.data.api.DeviceLaunchDefaults
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.setLaunchDefaultsInput
import com.exponential.app.domain.DomainContract
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-481: the settings sheet's pure defaults-editor helpers. */
class DeviceSettingsDefaultsTest {

    private fun device(
        agents: List<String>? = listOf("claude", "codex"),
        unauthed: List<String> = emptyList(),
        defaults: DeviceLaunchDefaults? = null,
        accounts: Map<String, AgentAccount>? = null,
    ) = SteerDevice(
        deviceId = "dev-1",
        agents = agents,
        unauthedAgents = unauthed,
        launchDefaults = defaults,
        agentAccounts = accounts,
    )

    @Test
    fun `editable agents cover runnable + signed-out + stored, contract order`() {
        assertEquals(
            listOf("claude", "codex"),
            editableAgents(device(agents = listOf("codex"), unauthed = listOf("claude"))),
        )
        // A machine the row knows nothing about stays fully editable.
        assertEquals(
            DomainContract.codingAgentValues,
            editableAgents(device(agents = null, unauthed = emptyList())),
        )
        // EXP-688: an agent the machine only reports an ACCOUNT for still gets
        // a tab — that block is where its sign-in and usage live now.
        assertEquals(
            listOf("claude", "codex"),
            editableAgents(
                device(
                    agents = listOf("claude"),
                    unauthed = emptyList(),
                    accounts = mapOf("codex" to AgentAccount(signedIn = true)),
                ),
            ),
        )
        // EXP-849: an agent id the CONTRACT no longer names (a machine still
        // reporting `pi`, an external binary) is not editable at all.
        assertEquals(
            listOf("claude"),
            editableAgents(device(agents = listOf("claude", "pi"), unauthed = emptyList())),
        )
    }

    @Test
    fun `stored default agent wins when editable, else claude, else first`() {
        assertEquals(
            "codex",
            seededDefaultAgent(
                device(defaults = DeviceLaunchDefaults(defaultAgent = "codex")),
                listOf("claude", "codex"),
            ),
        )
        assertEquals(
            "claude",
            seededDefaultAgent(device(), listOf("claude", "codex")),
        )
        assertEquals(
            "codex",
            seededDefaultAgent(device(), listOf("codex")),
        )
    }

    @Test
    fun `drafts clamp stored vocabulary and capabilities`() {
        val stored = DeviceLaunchDefaults(
            agents = mapOf(
                "codex" to AgentLaunchDefaults(
                    model = "not-a-model",
                    effort = "high",
                    ultracode = true,
                ),
            ),
        )
        val draft = agentDraft(device(defaults = stored), "codex")
        // Invalid model falls back to codex's CLI default; ultracode never
        // survives off claude.
        assertEquals("", draft.model)
        assertEquals("high", draft.effort)
        assertFalse(draft.ultracode)
    }

    @Test
    fun `buildDefaults masks capabilities per agent and carries the default`() {
        val built = buildDefaults(
            defaultAgent = "claude",
            defaultAccount = "work",
            agents = listOf("claude", "codex", "pi"),
            drafts = mapOf(
                "claude" to AgentDraft("fable", "", ultracode = true, planMode = true),
                "codex" to AgentDraft("", "high", ultracode = true, planMode = true),
                "pi" to AgentDraft("", "", ultracode = false, planMode = true),
            ),
        )
        assertEquals("claude", built.defaultAgent)
        // EXP-872: the default ACCOUNT rides beside the agent it belongs to.
        assertEquals("work", built.defaultAccount)
        assertTrue(built.agents.getValue("claude").ultracode)
        assertTrue(built.agents.getValue("claude").planMode)
        // codex: neither ultracode nor plan mode survives.
        val codex = built.agents.getValue("codex")
        assertFalse(codex.ultracode)
        assertFalse(codex.planMode)
        // EXP-849: plan mode is claude's alone now — a retired/unknown agent
        // id keeps neither capability.
        val retired = built.agents.getValue("pi")
        assertFalse(retired.ultracode)
        assertFalse(retired.planMode)
    }

    /**
     * EXP-490: the sheet auto-saves and re-seeds itself from the synced row, so
     * what a save writes must read back as the very drafts it was built from —
     * otherwise the server's echo would visibly rewrite the user's picks.
     */
    @Test
    fun `saved defaults re-seed to the drafts they were built from`() {
        val agents = listOf("claude", "codex")
        val edited = mapOf(
            "claude" to AgentDraft(
                model = DomainContract.codingModelValues.last(),
                effort = DomainContract.codingEffortValues.first(),
                ultracode = true,
                planMode = true,
            ),
            // CLI defaults ("") must survive the round trip as themselves.
            "codex" to AgentDraft("", "", ultracode = false, planMode = false),
        )
        val echoed = device(
            agents = agents,
            unauthed = emptyList(),
            defaults = buildDefaults(
                defaultAgent = "codex",
                defaultAccount = "main",
                agents = agents,
                drafts = edited,
            ),
        )
        assertEquals(agents, editableAgents(echoed))
        assertEquals("codex", seededDefaultAgent(echoed, agents))
        assertEquals("main", echoed.launchDefaults?.defaultAccount)
        assertEquals(edited, agents.associateWith { agentDraft(echoed, it) })
    }

    /**
     * EXP-872: nothing picked (the agent's ACTIVE login) is an unset
     * `defaultAccount`, and the REQUEST spells it as an explicit null: the
     * server reads an absent key as an older client and keeps the stored pin.
     */
    @Test
    fun `an unpicked default account clears with an explicit null`() {
        val built = buildDefaults(
            defaultAgent = "claude",
            defaultAccount = "",
            agents = listOf("claude"),
            drafts = emptyMap(),
        )
        assertNull(built.defaultAccount)
        val input = setLaunchDefaultsInput(deviceId = "dev-1", defaults = built)
        assertEquals(JsonPrimitive("dev-1"), input["deviceId"])
        val sent = input.getValue("launchDefaults").jsonObject
        assertEquals(JsonPrimitive("claude"), sent["defaultAgent"])
        assertTrue(sent.containsKey("defaultAccount"))
        assertEquals(JsonNull, sent["defaultAccount"])
        // The rest of the object still drops its nulls, as it always did.
        assertFalse(sent.getValue("agents").jsonObject.getValue("claude").jsonObject.containsValue(JsonNull))
    }

    @Test
    fun `a picked default account rides as itself, and none without an agent`() {
        val pinned = setLaunchDefaultsInput(
            deviceId = "dev-1",
            defaults = DeviceLaunchDefaults(defaultAgent = "claude", defaultAccount = "work"),
        ).getValue("launchDefaults").jsonObject
        assertEquals(JsonPrimitive("work"), pinned["defaultAccount"])
        val agentless = setLaunchDefaultsInput(
            deviceId = "dev-1",
            defaults = DeviceLaunchDefaults(),
        ).getValue("launchDefaults").jsonObject
        assertFalse(agentless.containsKey("defaultAccount"))
    }

    /** EXP-773 deleted the "Start in terminal" preference. An older server
     *  still stamps `startInTerminal` onto `launchDefaults`; the decoder is
     *  `ignoreUnknownKeys`, so the key is skipped instead of failing the whole
     *  object and dropping the machine out of every picker. */
    @Test
    fun `a retired startInTerminal key still decodes`() {
        val json = Json { ignoreUnknownKeys = true }
        val defaults = json.decodeFromString(
            DeviceLaunchDefaults.serializer(),
            """{"defaultAgent":"claude","startInTerminal":true,"agents":{}}""",
        )
        assertEquals("claude", defaults.defaultAgent)
    }
}
