package com.exponential.app.domain

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentAccountProfile
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.data.api.AgentUsageWindow
import com.exponential.app.data.api.DeviceLaunchDefaults
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-988 contract tests for [AccountOptions.flatten] (web
 * `lib/accounts/account-option.ts`), implemented by EXP-872. The SAME table is
 * mirrored in the TypeScript, Rust and Swift copies — same fixture, same test
 * names (bar the characters Kotlin does not allow in one).
 */
class AccountOptionTest {

    /**
     * The fixture ×4: two claude logins (work = active, home = a dead
     * credential), one codex login, and a `system` codex profile that is
     * signed out. The default agent is codex.
     */
    private val accounts = mapOf(
        "claude" to AgentAccount(
            signedIn = true,
            email = "work@x.test",
            profiles = listOf(
                AgentAccountProfile(
                    id = "work",
                    label = "Work laptop",
                    signedIn = true,
                    email = "work@x.test",
                    active = true,
                    health = "ok",
                    usage = AgentUsage(
                        fetchedAt = "2026-09-19T10:00:00Z",
                        windows = listOf(
                            AgentUsageWindow(key = "session", label = "5h", percent = 40.0),
                            AgentUsageWindow(key = "weekly", label = "Week", percent = 85.0),
                            AgentUsageWindow(key = "model:opus", label = "Opus", percent = 10.0),
                        ),
                    ),
                ),
                AgentAccountProfile(
                    id = "home",
                    label = "Default",
                    signedIn = true,
                    email = "home@x.test",
                    health = "needs_relogin",
                ),
            ),
        ),
        "codex" to AgentAccount(
            signedIn = true,
            profiles = listOf(
                AgentAccountProfile(
                    id = "main",
                    label = "Main",
                    signedIn = true,
                    email = "codex@x.test",
                    active = true,
                    usage = AgentUsage(
                        windows = listOf(
                            AgentUsageWindow(key = "session", label = "5h", percent = 5.0),
                            AgentUsageWindow(key = "weekly", label = "Week", percent = 50.0),
                        ),
                    ),
                ),
                AgentAccountProfile(id = "system", signedIn = false, active = false),
            ),
        ),
        // A retired agent still sitting in a synced row (EXP-849).
        "pi" to AgentAccount(signedIn = true, email = "pi@x.test"),
    )

    private val defaults = DeviceLaunchDefaults(defaultAgent = "codex")

    private fun flatten(
        launchDefaults: DeviceLaunchDefaults? = defaults,
        usage: Map<String, AgentUsage>? = null,
    ) = AccountOptions.flatten(accounts, usage, launchDefaults)

    @Test
    fun `yields one option per signed-in login across both agents`() {
        assertEquals(
            listOf("codex:main", "claude:work", "claude:home"),
            flatten().map { it.key },
        )
    }

    @Test
    fun `labels every option by email, never by profile name and never 'default'`() {
        // A profile { id: 'work', label: 'Work laptop', email: 'a@x.test' }
        // yields email 'a@x.test'; no option's email is 'Work laptop' or
        // contains the word 'default'.
        val options = flatten()
        assertEquals(
            listOf("codex@x.test", "work@x.test", "home@x.test"),
            options.map { it.email },
        )
        options.forEach { option ->
            assertTrue(option.email != "Work laptop")
            assertTrue(!option.email.lowercase().contains("default"))
        }
    }

    @Test
    fun `puts the device default first and marks exactly one option`() {
        // launchDefaults.defaultAgent = 'codex' with an active codex login →
        // that login is options[0] and the only isDeviceDefault: true.
        val options = flatten()
        assertEquals("codex", options.first().agent)
        assertEquals("main", options.first().id)
        assertTrue(options.first().isDeviceDefault)
        assertEquals(1, options.count { it.isDeviceDefault })
        assertEquals(options.first(), AccountOptions.default(options))
    }

    @Test
    fun `prefers the stored default account of the default agent`() {
        // EXP-872: "default agent" became "default account" — the device
        // stores the profile id, and it wins over the agent's ACTIVE login.
        val options = flatten(
            DeviceLaunchDefaults(defaultAgent = "claude", defaultAccount = "home"),
        )
        assertEquals("claude:home", options.first().key)
        assertTrue(options.first().isDeviceDefault)
        assertEquals(1, options.count { it.isDeviceDefault })
        // A stored profile the device no longer reports falls back to the
        // active login.
        val gone = flatten(
            DeviceLaunchDefaults(defaultAgent = "claude", defaultAccount = "retired"),
        )
        assertEquals("claude:work", gone.first().key)
    }

    @Test
    fun `falls back to the first contract agent's active login when no default agent is set`() {
        val options = flatten(launchDefaults = null)
        assertEquals("claude:work", options.first().key)
        assertEquals(1, options.count { it.isDeviceDefault })
        // A default agent with no active login falls back the same way.
        val stale = flatten(DeviceLaunchDefaults(defaultAgent = "pi"))
        assertEquals("claude:work", stale.first().key)
    }

    @Test
    fun `carries the agent on the option so a pick implies it`() {
        val options = flatten()
        assertEquals("codex", options.first { it.id == "main" }.agent)
        assertEquals("claude", options.first { it.id == "home" }.agent)
        assertEquals(AgentHealth.NeedsRelogin, options.first { it.id == "home" }.health)
        assertEquals(
            AccountOptionKey(agent = "codex", id = "main"),
            AccountOptions.parseKey(options.first().key),
        )
        assertNull(AccountOptions.parseKey("nope"))
    }

    @Test
    fun `derives limits as 0-1 fractions from the session, weekly and first model windows`() {
        // windows [{key:'session', percent: 40}, {key:'weekly', percent: 85},
        // {key:'model:opus', label:'Opus', percent: 10}] →
        // limits { fiveHour: 0.4, week: 0.85, model: { label: 'Opus', used: 0.1 } }.
        val options = flatten()
        assertEquals(
            AccountLimits(
                fiveHour = 0.4,
                week = 0.85,
                model = AccountModelLimit(label = "Opus", used = 0.1),
            ),
            options.first { it.id == "work" }.limits,
        )
        // Codex reports no per-model window: no `model` at all.
        assertEquals(
            AccountLimits(fiveHour = 0.05, week = 0.5),
            options.first { it.id == "main" }.limits,
        )
    }

    @Test
    fun `omits limits for a login with no usage report`() {
        assertNull(flatten().first { it.id == "home" }.limits)
    }

    @Test
    fun `shows the plan for a login the device reports without an address`() {
        val options = AccountOptions.flatten(
            accounts = mapOf(
                "claude" to AgentAccount(
                    signedIn = true,
                    profiles = listOf(
                        AgentAccountProfile(
                            id = "p1",
                            label = "Plan only",
                            signedIn = true,
                            plan = "Max",
                            active = true,
                        ),
                        AgentAccountProfile(id = "p2", label = "Bare", signedIn = true),
                    ),
                ),
            ),
            usage = null,
            launchDefaults = null,
        )
        assertEquals(listOf("Max", "p2"), options.map { it.email })
    }

    @Test
    fun `yields the ambient system login for a device that reports no profiles`() {
        val options = AccountOptions.flatten(
            accounts = mapOf("claude" to AgentAccount(signedIn = true, email = "solo@x.test")),
            usage = mapOf(
                "claude" to AgentUsage(
                    windows = listOf(
                        AgentUsageWindow(key = "session", label = "5h", percent = 20.0),
                    ),
                ),
            ),
            launchDefaults = null,
        )
        assertEquals(
            listOf(
                AccountOption(
                    id = "system",
                    agent = "claude",
                    email = "solo@x.test",
                    isDeviceDefault = true,
                    health = AgentHealth.Ok,
                    limits = AccountLimits(fiveHour = 0.2, week = 0.0),
                ),
            ),
            options,
        )
    }

    @Test
    fun `skips signed-out logins and retired agents`() {
        val options = flatten()
        assertTrue(options.none { it.id == "system" })
        assertTrue(options.none { it.agent == "pi" })
        assertEquals(emptyList<AccountOption>(), AccountOptions.flatten(null, null, null))
        assertNull(AccountOptions.default(emptyList()))
    }
}
