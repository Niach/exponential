package com.exponential.app.ui.session

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentLaunchDefaults
import com.exponential.app.data.api.DeviceLaunchDefaults
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.domain.DomainContract
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-481: the settings sheet's pure defaults-editor helpers. */
class DeviceSettingsDefaultsTest {

    private fun device(
        agents: List<String>? = listOf("claude", "codex"),
        unauthed: List<String> = listOf("pi"),
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
            listOf("claude", "codex", "pi"),
            editableAgents(device()),
        )
        // A machine the row knows nothing about stays fully editable.
        assertEquals(
            DomainContract.codingAgentValues,
            editableAgents(device(agents = null, unauthed = emptyList())),
        )
        // EXP-688: an agent the machine only reports an ACCOUNT for still gets
        // a tab — that block is where its sign-in and usage live now.
        assertEquals(
            listOf("claude", "pi"),
            editableAgents(
                device(
                    agents = listOf("claude"),
                    unauthed = emptyList(),
                    accounts = mapOf("pi" to AgentAccount(signedIn = true)),
                ),
            ),
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
            agents = listOf("claude", "codex", "pi"),
            drafts = mapOf(
                "claude" to AgentDraft("fable", "", ultracode = true, planMode = true),
                "codex" to AgentDraft("", "high", ultracode = true, planMode = true),
                "pi" to AgentDraft("", "", ultracode = false, planMode = true),
            ),
        )
        assertEquals("claude", built.defaultAgent)
        assertTrue(built.agents.getValue("claude").ultracode)
        assertTrue(built.agents.getValue("claude").planMode)
        // codex: neither ultracode nor plan mode survives.
        val codex = built.agents.getValue("codex")
        assertFalse(codex.ultracode)
        assertFalse(codex.planMode)
        // pi: plan mode only.
        val pi = built.agents.getValue("pi")
        assertFalse(pi.ultracode)
        assertTrue(pi.planMode)
    }

    /**
     * EXP-490: the sheet auto-saves and re-seeds itself from the synced row, so
     * what a save writes must read back as the very drafts it was built from —
     * otherwise the server's echo would visibly rewrite the user's picks.
     */
    @Test
    fun `saved defaults re-seed to the drafts they were built from`() {
        val agents = listOf("claude", "codex", "pi")
        val edited = mapOf(
            "claude" to AgentDraft(
                model = DomainContract.codingModelValues.last(),
                effort = DomainContract.codingEffortValues.first(),
                ultracode = true,
                planMode = true,
            ),
            "codex" to AgentDraft(
                model = DomainContract.codexModelValues.first(),
                effort = DomainContract.codexEffortValues.last(),
                ultracode = false,
                planMode = false,
            ),
            // CLI defaults ("") must survive the round trip as themselves.
            "pi" to AgentDraft("", "", ultracode = false, planMode = true),
        )
        val echoed = device(
            agents = agents,
            unauthed = emptyList(),
            defaults = buildDefaults(defaultAgent = "codex", agents = agents, drafts = edited),
        )
        assertEquals(agents, editableAgents(echoed))
        assertEquals("codex", seededDefaultAgent(echoed, agents))
        assertEquals(edited, agents.associateWith { agentDraft(echoed, it) })
    }
}
