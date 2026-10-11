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
                    id = "me",
                    active = true,
                    signedIn = true,
                    email = "me@acme.test",
                    plan = "max",
                    health = "ok",
                ),
                AgentAccountProfile(
                    id = "work",
                    signedIn = true,
                    email = "work@acme.test",
                    health = "ok",
                ),
                AgentAccountProfile(id = "stale", signedIn = true, health = "needs_relogin"),
                AgentAccountProfile(id = "empty", signedIn = false),
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
        canSwitchAccount: Boolean = true,
        turnState: String = TURN_STATE_ENDED,
        currentAccount: String? = null,
    ) = SessionAccountSwitch.refusal(
        option = option(id),
        agent = agent,
        mine = mine,
        sessionEnded = sessionEnded,
        deviceOnline = deviceOnline,
        canResume = canResume,
        canSwitchAccount = canSwitchAccount,
        turnState = turnState,
        currentAccount = currentAccount,
    )

    @Test
    fun `options list every profile the machine reported`() {
        val options = SessionAccountSwitch.options(accounts, "claude")
        assertEquals(listOf("me", "work", "stale", "empty"), options.map { it.profileId })
        assertEquals("me@acme.test", options[0].caption)
        assertEquals("work@acme.test", options[1].caption)
        // EXP-1013: a login that names nobody reads "No email".
        assertEquals("No email", options[3].caption)
        assertEquals(AgentHealth.NeedsRelogin, options[2].health)
    }

    @Test
    fun `an agent with no profiles offers nothing, never its top-level login`() {
        assertEquals(emptyList<SessionAccountOption>(), SessionAccountSwitch.options(accounts, "codex"))
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
        assertNull(refusal("me"))
        assertEquals(
            SessionAccountSwitch.REASON_ALREADY,
            refusal("me", currentAccount = "me"),
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
        // EXP-849: `resume-run` alone is not enough — a machine that does not
        // advertise `account-switch` resumes on the RECORDED account and drops
        // the field, which the server refuses too.
        assertEquals(SessionAccountSwitch.REASON_NO_CAP, refusal("work", canSwitchAccount = false))
        assertEquals(SessionAccountSwitch.REASON_ENDED, refusal("work", sessionEnded = true))
        assertEquals(SessionAccountSwitch.REASON_NOT_MINE, refusal("work", mine = false))
    }

    @Test
    fun `an unusable account is named, never silently offered`() {
        assertEquals(SessionAccountSwitch.REASON_NEEDS_RELOGIN, refusal("stale"))
        assertEquals(SessionAccountSwitch.REASON_SIGNED_OUT, refusal("empty"))
    }

    @Test
    fun `a switch names the account it targets`() {
        // The PRESENCE of `account` is what makes the server accept a resume of
        // a LIVE run, so the last used login is named here too.
        assertEquals("me", SessionAccountSwitch.wireAccount(option("me")))
        assertEquals("work", SessionAccountSwitch.wireAccount(option("work")))
    }
}
