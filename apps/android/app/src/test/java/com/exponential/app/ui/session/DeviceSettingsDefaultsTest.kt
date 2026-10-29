package com.exponential.app.ui.session

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentLaunchDefaults
import com.exponential.app.data.api.DeviceLaunchDefaults
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.setLaunchDefaultsInput
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.computerUseModelOptions
import com.exponential.app.ui.components.modelOptionsFor
import com.exponential.app.ui.components.seedComputerUseModel
import com.exponential.app.ui.components.subagentModelOptions
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
    fun `the last used agent wins when editable, else claude, else first`() {
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
        // Every model is explicit: an invalid model falls back to codex's
        // contract default (never a blank "CLI default"); ultracode never
        // survives off claude.
        assertEquals(DomainContract.deviceAgentDefaultsCodexModel, draft.model)
        assertEquals(DomainContract.deviceAgentDefaultsCodexThreadModel, draft.threadModel)
        assertEquals("high", draft.effort)
        assertFalse(draft.ultracode)
    }

    @Test
    fun `buildDefaults masks capabilities per agent and never names an agent`() {
        val built = buildDefaults(
            agents = listOf("claude", "codex", "pi"),
            drafts = mapOf(
                "claude" to AgentDraft("fable", "", ultracode = true),
                "codex" to AgentDraft("", "high", ultracode = true),
                "pi" to AgentDraft("", "", ultracode = true),
            ),
        )
        // EXP-1158: the last used agent is the device's to write.
        assertNull(built.defaultAgent)
        assertTrue(built.agents.getValue("claude").ultracode)
        // codex: ultracode does not survive.
        val codex = built.agents.getValue("codex")
        assertFalse(codex.ultracode)
        // EXP-849: a retired/unknown agent id keeps no claude capability.
        val retired = built.agents.getValue("pi")
        assertFalse(retired.ultracode)
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
                subagentModel = "sonnet",
                threadModel = "opus",
                ultracode = true,
            ),
            // The effort keeps its CLI default ("") through the round trip.
            "codex" to AgentDraft(
                model = DomainContract.codexModelValues[1],
                effort = "",
                threadModel = DomainContract.codexModelValues[0],
                ultracode = false,
            ),
        )
        val echoed = device(
            agents = agents,
            unauthed = emptyList(),
            defaults = buildDefaults(agents = agents, drafts = edited),
        )
        assertEquals(agents, editableAgents(echoed))
        assertEquals(edited, agents.associateWith { agentDraft(echoed, it) })
    }

    /**
     * EXP-1158: a settings save never names an agent or an account. The last
     * used agent is the DEVICE's to write (the server carries the stored one
     * forward when a save omits it), and the account concept is gone — so
     * neither key rides, not even as a null, whatever the DTO holds.
     */
    @Test
    fun `the setLaunchDefaults input omits both the agent and the account`() {
        val input = setLaunchDefaultsInput(
            deviceId = "dev-1",
            defaults = DeviceLaunchDefaults(
                defaultAgent = "codex",
                agents = mapOf("claude" to AgentLaunchDefaults(model = "opus")),
            ),
        )
        assertEquals(JsonPrimitive("dev-1"), input["deviceId"])
        val sent = input.getValue("launchDefaults").jsonObject
        assertFalse(sent.containsKey("defaultAgent"))
        assertFalse(sent.keys.any { it.contains("account", ignoreCase = true) })
        assertEquals(JsonPrimitive("opus"), sent.getValue("agents").jsonObject.getValue("claude").jsonObject["model"])
        // The rest of the object still drops its nulls, as it always did.
        assertFalse(sent.getValue("agents").jsonObject.getValue("claude").jsonObject.containsValue(JsonNull))
    }

    /** SLOP-3: the save never carries a `workflow` pair any more. */
    @Test
    fun `the setLaunchDefaults payload carries no workflow pair`() {
        val sent = setLaunchDefaultsInput(
            deviceId = "dev-1",
            defaults = buildDefaults(agents = listOf("claude"), drafts = emptyMap()),
        ).getValue("launchDefaults").jsonObject
        assertFalse(sent.containsKey("workflow"))
    }

    /**
     * EXP-1005: `setLaunchDefaults` REPLACES the stored object, so a save of
     * an UNTOUCHED `autoRotateAccounts` must ECHO the synced value: a stored
     * boolean rides back as itself, and a row without the key writes NO key
     * (never a literal null, which the server reads as a clear). Only a flip
     * of the "Auto-rotate accounts" row writes a value the row did not hold.
     */
    @Test
    fun `autoRotateAccounts is echoed as stored and absent when unset`() {
        val agents = listOf("claude", "codex")
        val stored = device(
            agents = agents,
            defaults = DeviceLaunchDefaults(
                defaultAgent = "claude",
                agents = mapOf(
                    "claude" to AgentLaunchDefaults(model = "opus", autoRotateAccounts = false),
                    "codex" to AgentLaunchDefaults(model = ""),
                ),
            ),
        )
        val drafts = agents.associateWith { agentDraft(stored, it) }
        assertEquals(false, drafts.getValue("claude").autoRotateAccounts)
        assertNull(drafts.getValue("codex").autoRotateAccounts)

        // An edit elsewhere on the draft keeps the echoed flag.
        val edited = drafts + ("claude" to drafts.getValue("claude").copy(ultracode = true))
        val sent = setLaunchDefaultsInput(
            deviceId = "dev-1",
            defaults = buildDefaults(
                agents = agents,
                drafts = edited,
            ),
        ).getValue("launchDefaults").jsonObject.getValue("agents").jsonObject
        assertEquals(JsonPrimitive(false), sent.getValue("claude").jsonObject["autoRotateAccounts"])
        val codex = sent.getValue("codex").jsonObject
        assertFalse(codex.containsKey("autoRotateAccounts"))
        assertFalse(codex.containsValue(JsonNull))

        // A true value echoes too, and a draft built from nothing stays keyless.
        val fresh = setLaunchDefaultsInput(
            deviceId = "dev-1",
            defaults = buildDefaults(
                agents = listOf("claude"),
                drafts = mapOf(
                    "claude" to AgentDraft("fable", "", ultracode = false, autoRotateAccounts = true),
                ),
            ),
        ).getValue("launchDefaults").jsonObject.getValue("agents").jsonObject
        assertEquals(JsonPrimitive(true), fresh.getValue("claude").jsonObject["autoRotateAccounts"])
        assertFalse(
            setLaunchDefaultsInput(
                deviceId = "dev-1",
                defaults = buildDefaults(listOf("claude"), emptyMap()),
            ).getValue("launchDefaults").jsonObject.getValue("agents").jsonObject
                .getValue("claude").jsonObject.containsKey("autoRotateAccounts"),
        )
    }

    /**
     * EXP-1196: the device-level `computerUse` switch decodes leniently
     * (absent/null = null = OFF), rides the save as an explicit boolean once
     * set, and stays ABSENT (never a literal null) while unset so the server
     * carries the stored value forward.
     */
    @Test
    fun `computerUse round-trips and stays absent when unset`() {
        val json = Json { ignoreUnknownKeys = true }
        val absent = json.decodeFromString(
            DeviceLaunchDefaults.serializer(),
            """{"defaultAgent":"claude","agents":{}}""",
        )
        assertNull(absent.computerUse)
        assertNull(
            json.decodeFromString(
                DeviceLaunchDefaults.serializer(),
                """{"computerUse":null,"agents":{}}""",
            ).computerUse,
        )
        val stored = json.decodeFromString(
            DeviceLaunchDefaults.serializer(),
            """{"computerUse":true,"agents":{"claude":{"model":"opus"}}}""",
        )
        assertEquals(true, stored.computerUse)

        for (value in listOf(true, false)) {
            val sent = setLaunchDefaultsInput(
                deviceId = "dev-1",
                defaults = buildDefaults(listOf("claude"), emptyMap(), computerUse = value),
            ).getValue("launchDefaults").jsonObject
            assertEquals(JsonPrimitive(value), sent["computerUse"])
            // And it decodes back as itself.
            assertEquals(
                value,
                json.decodeFromJsonElement(DeviceLaunchDefaults.serializer(), sent).computerUse,
            )
        }
        val unset = setLaunchDefaultsInput(
            deviceId = "dev-1",
            defaults = buildDefaults(listOf("claude"), emptyMap()),
        ).getValue("launchDefaults").jsonObject
        assertFalse(unset.containsKey("computerUse"))
    }

    /**
     * EXP-1236: the Computer use model rides beside the switch as a top-level
     * alias: decoded leniently (absent = null), sent as itself once the sheet
     * holds one, ABSENT (never a literal null) while unset; and the sheet's
     * seed clamps a stored alias to the contract, the default being Haiku.
     */
    @Test
    fun `computerUseModel rides beside the switch and seeds to the contract default`() {
        val json = Json { ignoreUnknownKeys = true }
        assertNull(
            json.decodeFromString(
                DeviceLaunchDefaults.serializer(),
                """{"computerUse":true,"agents":{}}""",
            ).computerUseModel,
        )
        val stored = json.decodeFromString(
            DeviceLaunchDefaults.serializer(),
            """{"computerUse":true,"computerUseModel":"sonnet","agents":{}}""",
        )
        assertEquals("sonnet", stored.computerUseModel)

        val sent = setLaunchDefaultsInput(
            deviceId = "dev-1",
            defaults = buildDefaults(listOf("claude"), emptyMap(), computerUse = true, computerUseModel = "opus"),
        ).getValue("launchDefaults").jsonObject
        assertEquals(JsonPrimitive(true), sent["computerUse"])
        assertEquals(JsonPrimitive("opus"), sent["computerUseModel"])
        assertEquals(
            "opus",
            json.decodeFromJsonElement(DeviceLaunchDefaults.serializer(), sent).computerUseModel,
        )
        val unset = setLaunchDefaultsInput(
            deviceId = "dev-1",
            defaults = buildDefaults(listOf("claude"), emptyMap()),
        ).getValue("launchDefaults").jsonObject
        assertFalse(unset.containsKey("computerUseModel"))
        assertFalse(unset.containsValue(JsonNull))

        // The seed: a contract alias as itself, anything else the default.
        assertEquals("haiku", DomainContract.deviceComputerUseDefaultsModel)
        // THE claude model list is the computer use vocabulary too.
        assertEquals(listOf("fable", "opus", "sonnet", "haiku"), computerUseModelOptions())
        assertEquals(DomainContract.deviceComputerUseDefaultsModel, seedComputerUseModel(null))
        assertEquals("fable", seedComputerUseModel("fable"))
        assertEquals(DomainContract.deviceComputerUseDefaultsModel, seedComputerUseModel("gpt-5.6-sol"))
        assertEquals(DomainContract.deviceComputerUseDefaultsModel, seedComputerUseModel(""))
    }

    /**
     * NEW-59: the Work screen (`computerUseDisplayId`, a display id per
     * #FEED-161, null = Auto)
     * decodes leniently, rides as itself once picked, stays ABSENT while unset
     * (the server carries it forward) and is an explicit null once Auto is
     * picked (which clears it).
     */
    @Test
    fun `computerUseDisplayId carries forward and an explicit null clears it`() {
        val json = Json { ignoreUnknownKeys = true }
        assertNull(
            json.decodeFromString(DeviceLaunchDefaults.serializer(), """{"agents":{}}""").computerUseDisplayId,
        )
        assertEquals(
            "uuid-b",
            json.decodeFromString(
                DeviceLaunchDefaults.serializer(),
                """{"computerUseDisplayId":"uuid-b","agents":{}}""",
            ).computerUseDisplayId,
        )
        // The #1295 number key is gone (no legacy): it reads as Auto.
        assertNull(
            json.decodeFromString(
                DeviceLaunchDefaults.serializer(),
                """{"computerUseDisplay":2,"agents":{}}""",
            ).computerUseDisplayId,
        )
        assertNull(
            json.decodeFromString(
                DeviceLaunchDefaults.serializer(),
                """{"computerUseDisplayId":null,"agents":{}}""",
            ).computerUseDisplayId,
        )

        val picked = setLaunchDefaultsInput(
            deviceId = "dev-1",
            defaults = buildDefaults(listOf("claude"), emptyMap(), computerUseDisplayId = "uuid-b"),
        ).getValue("launchDefaults").jsonObject
        assertEquals(JsonPrimitive("uuid-b"), picked["computerUseDisplayId"])
        assertFalse(picked.containsKey("computerUseDisplay"))

        val unset = setLaunchDefaultsInput(
            deviceId = "dev-1",
            defaults = buildDefaults(listOf("claude"), emptyMap()),
        ).getValue("launchDefaults").jsonObject
        assertFalse(unset.containsKey("computerUseDisplayId"))

        val auto = setLaunchDefaultsInput(
            deviceId = "dev-1",
            defaults = buildDefaults(listOf("claude"), emptyMap(), clearComputerUseDisplayId = true),
        ).getValue("launchDefaults").jsonObject
        assertEquals(JsonNull, auto["computerUseDisplayId"])
        assertFalse(auto.containsKey("clearComputerUseDisplayId"))
    }

    /** EXP-773 deleted the "Start in terminal" preference. A stored
     *  `launchDefaults` may still carry `startInTerminal`; the decoder is
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

    /**
     * Dispatcher roots: every model is explicit. No picker offers a
     * "Default" / "CLI default" row, a device that never set anything seeds
     * the contract defaults (work model, thread model, subagent model), and
     * the per-agent thread model rides the save beside the model.
     */
    @Test
    fun `model pickers list every model and no default entry`() {
        assertEquals(listOf("fable", "opus", "sonnet", "haiku"), modelOptionsFor("claude"))
        assertEquals(DomainContract.codexModelValues, modelOptionsFor("codex"))
        assertEquals(DomainContract.codingModelValues, subagentModelOptions())
        assertFalse("" in modelOptionsFor("codex"))

        val claude = agentDraft(device(), "claude")
        assertEquals(DomainContract.deviceAgentDefaultsModel, claude.model)
        assertEquals("haiku", claude.threadModel)
        assertEquals("opus", claude.subagentModel)
        val codex = agentDraft(device(), "codex")
        assertEquals(DomainContract.deviceAgentDefaultsCodexModel, codex.model)
        assertEquals(DomainContract.deviceAgentDefaultsCodexThreadModel, codex.threadModel)

        // A stored blank (the retired CLI default) reads as the contract default.
        val blank = agentDraft(
            device(
                defaults = DeviceLaunchDefaults(
                    agents = mapOf(
                        "claude" to AgentLaunchDefaults(model = "", subagentModel = "", threadModel = ""),
                    ),
                ),
            ),
            "claude",
        )
        assertEquals("fable", blank.model)
        assertEquals("opus", blank.subagentModel)
        assertEquals("haiku", blank.threadModel)

        val sent = setLaunchDefaultsInput(
            deviceId = "dev-1",
            defaults = buildDefaults(
                agents = listOf("claude", "codex"),
                drafts = mapOf("claude" to claude.copy(threadModel = "sonnet")),
            ),
        ).getValue("launchDefaults").jsonObject.getValue("agents").jsonObject
        assertEquals(JsonPrimitive("sonnet"), sent.getValue("claude").jsonObject["threadModel"])
        // A codex draft never stored rides codex's own thread default, and a
        // cross-agent value is clamped to it.
        assertEquals(
            JsonPrimitive(DomainContract.deviceAgentDefaultsCodexThreadModel),
            sent.getValue("codex").jsonObject["threadModel"],
        )
        assertEquals(
            DomainContract.deviceAgentDefaultsCodexThreadModel,
            threadModelOf(codex.copy(threadModel = "haiku"), "codex"),
        )
    }
}
