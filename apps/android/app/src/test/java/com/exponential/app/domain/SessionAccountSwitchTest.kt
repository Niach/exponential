package com.exponential.app.domain

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentAccountProfile
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-849 phase 3: the mid-session account switch gate — claude only, the
 * owner only, idle only, and only onto a login the machine can actually use.
 */
class SessionAccountSwitchTest {

    private val accounts = mapOf(
        "claude" to AgentAccount(
            signedIn = true,
            email = "me@acme.test",
            plan = "max",
            health = "ok",
            profiles = listOf(
                AgentAccountProfile(
                    id = "system",
                    active = true,
                    signedIn = true,
                    email = "me@acme.test",
                    plan = "max",
                    health = "ok",
                ),
                AgentAccountProfile(
                    id = "work",
                    label = "Work",
                    signedIn = true,
                    email = "work@acme.test",
                    health = "ok",
                ),
                AgentAccountProfile(id = "stale", label = "Old", signedIn = true, health = "needs_relogin"),
                AgentAccountProfile(id = "empty", label = "Spare", signedIn = false),
            ),
        ),
        "codex" to AgentAccount(signedIn = true, email = "cx@acme.test"),
    )

    private fun option(id: String) =
        SessionAccountSwitch.options(accounts, "claude").first { it.profileId == id }

    private fun refusal(
        id: String,
        agent: String? = "claude",
        mine: Boolean = true,
        sessionEnded: Boolean = false,
        deviceOnline: Boolean = true,
        canResume: Boolean = true,
        turnState: String = TURN_STATE_ENDED,
        currentAccount: String? = null,
    ) = SessionAccountSwitch.refusal(
        option = option(id),
        agent = agent,
        mine = mine,
        sessionEnded = sessionEnded,
        deviceOnline = deviceOnline,
        canResume = canResume,
        turnState = turnState,
        currentAccount = currentAccount,
    )

    @Test
    fun `options list every profile the machine reported`() {
        val options = SessionAccountSwitch.options(accounts, "claude")
        assertEquals(listOf("system", "work", "stale", "empty"), options.map { it.profileId })
        assertEquals("Default", options[0].label)
        assertEquals("Work", options[1].label)
        assertEquals("work@acme.test", options[1].caption)
        // The label is the last resort when a login names nobody.
        assertEquals("Spare", options[3].caption)
        assertEquals(AgentHealth.NeedsRelogin, options[2].health)
    }

    @Test
    fun `a pre-profile machine offers its one ambient account`() {
        val options = SessionAccountSwitch.options(accounts, "codex")
        assertEquals(1, options.size)
        assertEquals("system", options[0].profileId)
        assertEquals("cx@acme.test", options[0].caption)
        assertEquals(AgentHealth.Ok, options[0].health)
        // Nothing reported for the agent = nothing to switch between.
        assertEquals(emptyList<SessionAccountOption>(), SessionAccountSwitch.options(accounts, "gemini"))
        assertEquals(emptyList<SessionAccountOption>(), SessionAccountSwitch.options(null, "claude"))
        assertEquals(emptyList<SessionAccountOption>(), SessionAccountSwitch.options(accounts, null))
    }

    @Test
    fun `an idle claude run owned by me may switch`() {
        assertNull(refusal("work"))
        // The machine's own active login is still a valid continuation target —
        // only a run KNOWN to be on it is refused.
        assertNull(refusal("system"))
        assertEquals(
            SessionAccountSwitch.REASON_ALREADY,
            refusal("system", currentAccount = "system"),
        )
    }

    @Test
    fun `codex never switches mid-run`() {
        assertEquals(SessionAccountSwitch.REASON_AGENT, refusal("work", agent = "codex"))
        assertEquals(SessionAccountSwitch.REASON_AGENT, refusal("work", agent = null))
    }

    @Test
    fun `a switch waits for the turn and for the machine`() {
        assertEquals(SessionAccountSwitch.REASON_BUSY, refusal("work", turnState = TURN_STATE_STARTED))
        assertEquals(SessionAccountSwitch.REASON_OFFLINE, refusal("work", deviceOnline = false))
        assertEquals(SessionAccountSwitch.REASON_NO_CAP, refusal("work", canResume = false))
        assertEquals(SessionAccountSwitch.REASON_ENDED, refusal("work", sessionEnded = true))
        assertEquals(SessionAccountSwitch.REASON_NOT_MINE, refusal("work", mine = false))
    }

    @Test
    fun `an unusable account is named, never silently offered`() {
        assertEquals(SessionAccountSwitch.REASON_NEEDS_RELOGIN, refusal("stale"))
        assertEquals(SessionAccountSwitch.REASON_SIGNED_OUT, refusal("empty"))
    }

    @Test
    fun `the ambient login is never named on the wire`() {
        assertNull(SessionAccountSwitch.wireAccount(option("system")))
        assertEquals("work", SessionAccountSwitch.wireAccount(option("work")))
    }
}
