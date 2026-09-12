package com.exponential.app.domain

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentAccountProfile
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-849: the account-health rule, on the same names and the same fallback as
 * web (`agent-usage.test.ts` — `agentHealth` / `healthBadgeLabel` /
 * `worstHealth` / `deviceWorstHealth`) and the desktop
 * (`coding::agent_accounts::Health`).
 */
class AgentHealthTest {

    @Test
    fun `parses the four wire tokens and nothing else`() {
        assertEquals(AgentHealth.Ok, AgentHealthRules.parse("ok"))
        assertEquals(AgentHealth.NeedsRelogin, AgentHealthRules.parse("needs_relogin"))
        assertEquals(AgentHealth.SignedOut, AgentHealthRules.parse("signed_out"))
        assertEquals(AgentHealth.Unknown, AgentHealthRules.parse("unknown"))
        // A newer device's value, junk, and absence all degrade to unknown.
        assertEquals(AgentHealth.Unknown, AgentHealthRules.parse("expired_soon"))
        assertEquals(AgentHealth.Unknown, AgentHealthRules.parse(""))
        assertEquals(AgentHealth.Unknown, AgentHealthRules.parse(null))
    }

    @Test
    fun `absent health derives from signedIn`() {
        assertEquals(AgentHealth.Ok, AgentHealthRules.derived(true))
        assertEquals(AgentHealth.SignedOut, AgentHealthRules.derived(false))
        assertEquals(
            AgentHealth.Ok,
            AgentHealthRules.of(AgentAccount(signedIn = true)),
        )
        assertEquals(
            AgentHealth.SignedOut,
            AgentHealthRules.of(AgentAccount(signedIn = false)),
        )
        // No account at all claims nothing.
        assertEquals(AgentHealth.Unknown, AgentHealthRules.of(null))
    }

    @Test
    fun `a reported health wins over signedIn`() {
        // The device is the authority on its own credential: a signed-IN CLI
        // whose probe came back 401 needs a re-login.
        assertEquals(
            AgentHealth.NeedsRelogin,
            AgentHealthRules.of(AgentAccount(signedIn = true, health = "needs_relogin")),
        )
        assertEquals(
            AgentHealth.SignedOut,
            AgentHealthRules.of(AgentAccount(signedIn = true, health = "signed_out")),
        )
        assertEquals(
            AgentHealth.NeedsRelogin,
            AgentHealthRules.of(
                AgentAccountProfile(id = "work", signedIn = true, health = "needs_relogin"),
            ),
        )
        assertEquals(
            AgentHealth.Ok,
            AgentHealthRules.of(AgentAccountProfile(id = "work", signedIn = true)),
        )
    }

    @Test
    fun `badge names only the two negatives`() {
        assertEquals("Needs re-login", AgentHealthRules.badgeLabel(AgentHealth.NeedsRelogin))
        assertEquals("Signed out", AgentHealthRules.badgeLabel(AgentHealth.SignedOut))
        assertNull(AgentHealthRules.badgeLabel(AgentHealth.Ok))
        // "Signed in, never probed" is not a problem — nothing is claimed.
        assertNull(AgentHealthRules.badgeLabel(AgentHealth.Unknown))
    }

    @Test
    fun `worst is re-login then signed out then unknown then ok`() {
        assertEquals(
            AgentHealth.NeedsRelogin,
            AgentHealthRules.worst(listOf(AgentHealth.Ok, AgentHealth.NeedsRelogin, AgentHealth.SignedOut)),
        )
        assertEquals(
            AgentHealth.SignedOut,
            AgentHealthRules.worst(listOf(AgentHealth.Unknown, AgentHealth.SignedOut, AgentHealth.Ok)),
        )
        assertEquals(
            AgentHealth.Unknown,
            AgentHealthRules.worst(listOf(AgentHealth.Ok, AgentHealth.Unknown)),
        )
        assertEquals(AgentHealth.Ok, AgentHealthRules.worst(listOf(AgentHealth.Ok)))
        // Nothing reported claims nothing.
        assertNull(AgentHealthRules.worst(emptyList()))
    }

    @Test
    fun `device health is the worst of every account it reported`() {
        assertNull(AgentHealthRules.deviceWorst(null))
        assertNull(AgentHealthRules.deviceWorst(emptyMap()))
        assertEquals(
            AgentHealth.NeedsRelogin,
            AgentHealthRules.deviceWorst(
                mapOf(
                    "claude" to AgentAccount(signedIn = true, health = "ok"),
                    "codex" to AgentAccount(signedIn = true, health = "needs_relogin"),
                ),
            ),
        )
        // With profiles the TOP-LEVEL account is a duplicate of the active
        // one — the profiles are the whole truth.
        assertEquals(
            AgentHealth.NeedsRelogin,
            AgentHealthRules.deviceWorst(
                mapOf(
                    "claude" to AgentAccount(
                        signedIn = true,
                        health = "ok",
                        profiles = listOf(
                            AgentAccountProfile(id = "system", active = true, signedIn = true, health = "ok"),
                            AgentAccountProfile(id = "work", signedIn = true, health = "needs_relogin"),
                        ),
                    ),
                ),
            ),
        )
        // A pre-EXP-849 machine: derived, so a signed-out agent still badges.
        assertEquals(
            AgentHealth.SignedOut,
            AgentHealthRules.deviceWorst(
                mapOf(
                    "claude" to AgentAccount(signedIn = true),
                    "codex" to AgentAccount(signedIn = false),
                ),
            ),
        )
    }
}
