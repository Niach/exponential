package com.exponential.app.domain

import com.exponential.app.data.api.SteerDevice
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-836: the startability gate the machines list and the wizard share, and the
 * note the composer shows when a play button's machine cannot take the run.
 * Strings are web's verbatim (`my-machines.tsx`, `use-launch-composer.ts`).
 */
class LaunchDeviceRulesTest {

    private fun device(
        deviceId: String = "dev-1",
        label: String = "buildbox",
        online: Boolean = true,
        agents: List<String>? = listOf("claude"),
        unauthed: List<String> = emptyList(),
        isDefault: Boolean = false,
    ) = SteerDevice(
        deviceId = deviceId,
        deviceLabel = label,
        online = online,
        agents = agents,
        unauthedAgents = unauthed,
        isDefault = isDefault,
    )

    // ── Startability ────────────────────────────────────────────────────────

    @Test
    fun `a start needs an online machine with a runnable agent`() {
        assertTrue(LaunchDeviceRules.startable(device()))
        assertFalse(LaunchDeviceRules.startable(device(online = false)))
        // EXP-409: online, everything signed out.
        assertFalse(
            LaunchDeviceRules.startable(
                device(agents = emptyList(), unauthed = listOf("claude")),
            ),
        )
        // EXP-836: the case the old `!signInNeeded` gate let through — a
        // machine that reported NO agents at all kept its play button.
        assertFalse(LaunchDeviceRules.startable(device(agents = emptyList())))
        // An ABSENT advertisement is a pre-EXP-201 sender that runs claude.
        assertTrue(LaunchDeviceRules.startable(device(agents = null)))
    }

    @Test
    fun `the blocked caption names the reason, and only while online`() {
        assertNull(LaunchDeviceRules.blockedCaption(device()))
        // Offline is the last-seen caption's job, not this one's.
        assertNull(LaunchDeviceRules.blockedCaption(device(online = false, agents = emptyList())))
        assertEquals(
            "claude, codex not signed in",
            LaunchDeviceRules.blockedCaption(
                device(agents = emptyList(), unauthed = listOf("codex", "claude")),
            ),
        )
        assertEquals(
            LaunchDeviceRules.NO_RUNNABLE_AGENT,
            LaunchDeviceRules.blockedCaption(device(agents = emptyList())),
        )
        assertTrue(LaunchDeviceRules.signInNeeded(device(agents = emptyList(), unauthed = listOf("claude"))))
        assertFalse(LaunchDeviceRules.signInNeeded(device(agents = emptyList())))
    }

    // ── Precedence: request → pick → default → first ────────────────────────

    @Test
    fun `the request outranks the pick and the default`() {
        val pool = listOf(
            device(deviceId = "a"),
            device(deviceId = "b", isDefault = true),
            device(deviceId = "c"),
        )
        assertEquals("c", LaunchDeviceRules.resolve(pool, requested = "c", picked = "a")?.deviceId)
        assertEquals("a", LaunchDeviceRules.resolve(pool, requested = null, picked = "a")?.deviceId)
        // A request for a machine outside the pool falls through to the pick,
        // then the default, then the first row.
        assertEquals("a", LaunchDeviceRules.resolve(pool, requested = "gone", picked = "a")?.deviceId)
        assertEquals("b", LaunchDeviceRules.resolve(pool, requested = "gone", picked = null)?.deviceId)
        assertEquals(
            "a",
            LaunchDeviceRules.resolve(pool.filterNot { it.isDefault }, null, null)?.deviceId,
        )
        assertNull(LaunchDeviceRules.resolve(emptyList(), requested = "a", picked = null))
    }

    // ── The note ────────────────────────────────────────────────────────────

    @Test
    fun `an unhonoured request says which machine and why`() {
        val registry = listOf(
            device(deviceId = "mint-id", label = "mint", online = false),
            device(deviceId = "signed-out", label = "studio", agents = emptyList()),
            device(deviceId = "fine", label = "buildbox"),
        )
        // Honoured, or never made: nothing to say.
        assertNull(LaunchDeviceRules.requestNote("fine", "fine", registry))
        assertNull(LaunchDeviceRules.requestNote(null, "fine", registry))
        assertNull(LaunchDeviceRules.requestNote("", null, registry))
        // Still syncing: a machine that has not arrived yet is not a missing one.
        assertNull(LaunchDeviceRules.requestNote("mint-id", null, null))

        assertEquals(
            "mint is offline.",
            LaunchDeviceRules.requestNote("mint-id", "fine", registry),
        )
        assertEquals(
            "No agent is signed in on studio.",
            LaunchDeviceRules.requestNote("signed-out", "fine", registry),
        )
        assertEquals(
            "That machine is no longer in your registry.",
            LaunchDeviceRules.requestNote("removed", "fine", registry),
        )
        // A label-less row falls back to its id, like every other device label.
        assertEquals(
            "dev-9 is offline.",
            LaunchDeviceRules.requestNote(
                "dev-9",
                "fine",
                listOf(device(deviceId = "dev-9", label = "", online = false)),
            ),
        )
    }
}
