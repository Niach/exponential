package com.exponential.app.ui.session

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentLaunchDefaults
import com.exponential.app.data.api.DeviceLaunchDefaults
import com.exponential.app.data.api.DeviceWorkflowDefaults
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.setLaunchDefaultsInput
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.deviceAccountOptions
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

    /**
     * EXP-1043: the sheet's default-account row is how a machine's default
     * AGENT is changed, so an agent the machine reports no login for still
     * offers its ambient one — otherwise a machine signed into claude alone
     * could never be pointed at codex.
     */
    @Test
    fun `the device sheet offers an option for every editable agent`() {
        val signedIntoClaude = device(
            agents = listOf("claude", "codex"),
            accounts = mapOf("claude" to AgentAccount(signedIn = true, email = "me@acme.dev")),
        )
        val options = deviceAccountOptions(signedIntoClaude, listOf("claude", "codex"))
        assertEquals(listOf("claude", "codex"), options.map { it.agent })
        assertEquals("me@acme.dev", options.first().email)
        // The reported login stays THE default; the ambient one never claims it.
        assertEquals(1, options.count { it.isDeviceDefault })
        assertTrue(options.first().isDeviceDefault)
        // A machine that reports nothing at all is already one per agent.
        assertEquals(
            listOf("claude", "codex"),
            deviceAccountOptions(device(), listOf("claude", "codex")).map { it.agent },
        )
    }

    /**
     * EXP-1043: the workflow pair is resolved for the DEFAULT agent, and the
     * two vocabularies do not overlap — a stored name belonging to the other
     * agent is not something that agent can run, so it falls back.
     */
    @Test
    fun `workflow defaults take a stored pair only in the agent's own vocabulary`() {
        // Stored and valid: it wins.
        assertEquals(
            "sonnet" to "opus",
            workflowDefaults(
                "claude",
                DeviceWorkflowDefaults(model = "sonnet", strongModel = "opus"),
            ),
        )
        // Claude's names in a codex row: codex's contract pair instead.
        assertEquals(
            DomainContract.workflowLaunchCodexModel to
                DomainContract.workflowLaunchCodexStrongModel,
            workflowDefaults(
                "codex",
                DeviceWorkflowDefaults(model = "opus", strongModel = "fable"),
            ),
        )
        // Nothing stored at all (a machine from before the pair).
        assertEquals(
            DomainContract.workflowLaunchClaudeModel to
                DomainContract.workflowLaunchClaudeStrongModel,
            workflowDefaults("claude", null),
        )
        // Half a stored pair: the valid half stands, the other falls back.
        assertEquals(
            "fable" to DomainContract.workflowLaunchClaudeStrongModel,
            workflowDefaults("claude", DeviceWorkflowDefaults(model = "fable")),
        )
    }

    /**
     * EXP-1043: `setLaunchDefaults` REPLACES the stored object, so the sheet's
     * save carries the workflow pair with it — and a caller with nothing to
     * say about it writes no `workflow` key, which the server reads as an
     * older client and keeps what is stored.
     */
    @Test
    fun `the workflow pair rides the setLaunchDefaults payload`() {
        val built = buildDefaults(
            defaultAgent = "claude",
            defaultAccount = "work",
            agents = listOf("claude"),
            drafts = emptyMap(),
            workflow = DeviceWorkflowDefaults(model = "opus", strongModel = "fable"),
        )
        val sent = setLaunchDefaultsInput(deviceId = "dev-1", defaults = built)
            .getValue("launchDefaults").jsonObject
        val workflow = sent.getValue("workflow").jsonObject
        assertEquals(JsonPrimitive("opus"), workflow["model"])
        assertEquals(JsonPrimitive("fable"), workflow["strongModel"])

        val without = setLaunchDefaultsInput(
            deviceId = "dev-1",
            defaults = buildDefaults(
                defaultAgent = "claude",
                defaultAccount = "work",
                agents = listOf("claude"),
                drafts = emptyMap(),
            ),
        ).getValue("launchDefaults").jsonObject
        assertFalse(without.containsKey("workflow"))
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
