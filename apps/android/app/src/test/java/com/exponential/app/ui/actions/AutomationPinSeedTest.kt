package com.exponential.app.ui.actions

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentAccountProfile
import com.exponential.app.data.api.DeviceLaunchDefaults
import com.exponential.app.data.api.SteerDevice
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-995: the automation editor's pin seed ([seedAutomationPin]) — what the
 * Account row SHOWS is what Save STORES. A profile id is device-local, so a
 * switch of the bound machine re-seeds the pin from the NEW machine even when
 * it runs the same agent.
 */
class AutomationPinSeedTest {

    private fun login(id: String, email: String, active: Boolean = false) =
        AgentAccountProfile(id = id, label = id, signedIn = true, email = email, active = active, health = "ok")

    /** Laptop: claude (work = active, home) + codex (main); default = codex/main. */
    private val laptop = SteerDevice(
        deviceId = "laptop",
        agents = listOf("claude", "codex"),
        agentAccounts = mapOf(
            "claude" to AgentAccount(
                signedIn = true,
                profiles = listOf(login("work", "work@x.test", active = true), login("home", "home@x.test")),
            ),
            "codex" to AgentAccount(signedIn = true, profiles = listOf(login("main", "codex@x.test", active = true))),
        ),
        launchDefaults = DeviceLaunchDefaults(defaultAgent = "codex", defaultAccount = "main"),
    )

    /** Server: claude only, ONE login (`srv`), which is its default. */
    private val server = SteerDevice(
        deviceId = "server",
        agents = listOf("claude"),
        agentAccounts = mapOf(
            "claude" to AgentAccount(signedIn = true, profiles = listOf(login("srv", "srv@x.test", active = true))),
        ),
        launchDefaults = DeviceLaunchDefaults(defaultAgent = "claude", defaultAccount = "srv"),
    )

    /** Codex-only box, reporting no logins at all (ambient `system` rows). */
    private val codexBox = SteerDevice(deviceId = "box", agents = listOf("codex"))

    private val pinned = AutomationDraft(
        deviceId = "laptop",
        agent = "claude",
        account = "home",
        model = "opus",
        effort = "high",
    )

    @Test
    fun `a runnable pin on the same machine is left alone`() {
        assertNull(seedAutomationPin(pinned, laptop, deviceSwitched = false))
    }

    @Test
    fun `an unset pin seeds the machine's default account`() {
        val seeded = seedAutomationPin(AutomationDraft(deviceId = "laptop"), laptop, deviceSwitched = false)
        assertEquals(
            AutomationDraft(deviceId = "laptop", agent = "codex", account = "main", model = "", effort = ""),
            seeded,
        )
    }

    @Test
    fun `a device switch moves a runnable pin onto the new machine's login of the same agent`() {
        // The stale `home` profile is the laptop's; the server's claude login
        // is `srv`. Model and effort are per agent, so they survive.
        val seeded = seedAutomationPin(pinned.copy(deviceId = "server"), server, deviceSwitched = true)
        assertEquals(pinned.copy(deviceId = "server", account = "srv"), seeded)
    }

    @Test
    fun `a device switch to a machine without the agent seeds its default account`() {
        val seeded = seedAutomationPin(pinned.copy(deviceId = "box"), codexBox, deviceSwitched = true)
        assertEquals(
            AutomationDraft(deviceId = "box", agent = "codex", account = "", model = "", effort = ""),
            seeded,
        )
    }

    @Test
    fun `a device switch onto an ambient login stores no profile id`() {
        // Codex on the laptop → the codex box, which reports no profiles:
        // the pin becomes the `system` login, which stores as NULL.
        val codexPin = AutomationDraft(deviceId = "laptop", agent = "codex", account = "main", model = "gpt-5")
        val seeded = seedAutomationPin(codexPin.copy(deviceId = "box"), codexBox, deviceSwitched = true)
        assertEquals(codexPin.copy(deviceId = "box", account = ""), seeded)
    }
}
