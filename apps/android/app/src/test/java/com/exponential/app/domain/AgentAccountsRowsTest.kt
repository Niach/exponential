package com.exponential.app.domain

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentAccountProfile
import com.exponential.app.data.api.DeviceOwner
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.data.api.AgentUsageWindow
import com.exponential.app.data.api.SYSTEM_PROFILE_ID
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.DeviceEntity
import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-829/EXP-909: the Devices page's per-login rules, on the same fixtures
 * and the same test names as web (`agent-usage.test.ts`) and the desktop
 * (`usage_bar.rs`): one row per login a MACHINE holds, its active login first,
 * and an identity label that never doubles as a status.
 */
class AgentAccountsRowsTest {

    private val nowMs = Instant.parse("2026-08-28T12:00:00Z").toEpochMilli()

    private fun row(
        deviceId: String,
        agent: String,
        profileId: String = SYSTEM_PROFILE_ID,
        deviceLabel: String = deviceId,
        mine: Boolean = true,
        online: Boolean = true,
        signedIn: Boolean = true,
        email: String? = null,
        plan: String? = null,
        usage: AgentUsage? = null,
        checkedAt: String? = null,
        health: AgentHealth = AgentHealthRules.derived(signedIn),
        active: Boolean = true,
        profileLabel: String = "Default",
    ) = AgentProfileUsageRow(
        key = "$deviceId:$agent:$profileId",
        deviceId = deviceId,
        deviceLabel = deviceLabel,
        mine = mine,
        online = online,
        agent = agent,
        profileId = profileId,
        profileLabel = profileLabel,
        active = active,
        signedIn = signedIn,
        health = health,
        email = email,
        plan = plan,
        usage = usage,
        checkedAt = checkedAt,
    )

    private fun usage(fetchedAt: String, percent: Int, stale: Boolean = false) = AgentUsage(
        fetchedAt = fetchedAt,
        stale = stale,
        windows = listOf(AgentUsageWindow(key = "weekly", label = "Week", percent = percent.toDouble())),
    )

    private fun usageJson(fetchedAt: String, key: String, percent: Int) =
        """{"fetchedAt":"$fetchedAt","stale":false,"windows":[{"key":"$key","label":"$key","percent":$percent,"resetsAt":null}]}"""

    private fun device(
        deviceId: String,
        label: String = deviceId,
        userId: String = "me",
        sharedTeamIds: List<String> = emptyList(),
        kind: String = "desktop",
        agentAccounts: String? = null,
        agentUsage: String? = null,
        agentUsageAt: String? = null,
        caps: String? = null,
        lastSeenAt: String? = "2026-08-28T11:59:00Z",
    ) = DeviceEntity(
        id = "row-$deviceId",
        userId = userId,
        deviceId = deviceId,
        label = label,
        kind = kind,
        caps = caps,
        agentAccounts = agentAccounts,
        agentUsage = agentUsage,
        agentUsageAt = agentUsageAt,
        lastSeenAt = lastSeenAt,
        sharedTeamIds = sharedTeamIds,
    )

    // ── agentProfileUsageRows ────────────────────────────────────────────────

    @Test
    fun `agent profile usage rows fall back to the system profile`() {
        val studio = device(
            deviceId = "dev-1",
            label = "Studio",
            agentAccounts = """{"claude":{"signedIn":true,"email":"dev@acme.test","plan":"max","checkedAt":"2026-08-28T11:00:00.000Z"}}""",
            agentUsage = """{"claude":${usageJson("2026-08-28T11:55:00.000Z", "session", 42)},"codex":${usageJson("2026-08-28T11:55:00.000Z", "weekly", 8)}}""",
            agentUsageAt = "2026-08-28T11:30:00.000Z",
        )
        val rows = AgentAccountsRows.agentProfileUsageRows(listOf(studio), "me") { true }
        assertEquals(listOf("dev-1:claude:system", "dev-1:codex:system"), rows.map { it.key })

        val claude = rows[0]
        assertEquals("claude", claude.agent)
        assertEquals(SYSTEM_PROFILE_ID, claude.profileId)
        assertEquals("Default", claude.profileLabel)
        assertTrue("the ambient login is always the active one", claude.active)
        assertTrue(claude.mine)
        assertTrue(claude.online)
        assertTrue(claude.signedIn)
        assertEquals("Studio", claude.deviceLabel)
        assertEquals("dev@acme.test", claude.email)
        assertEquals("max", claude.plan)
        assertEquals(42, AgentAccountsRows.peakPercent(claude.usage))
        // The account's own probe stamp wins over the row's usage stamp.
        assertEquals("2026-08-28T11:00:00.000Z", claude.checkedAt)

        // codex reported numbers but no account: a signed-out row.
        val codex = rows[1]
        assertFalse(codex.signedIn)
        assertNull(codex.email)
        assertEquals(8, AgentAccountsRows.peakPercent(codex.usage))
        // No account to date it: the device's `agent_usage_at` is the fallback.
        assertEquals("2026-08-28T11:30:00.000Z", codex.checkedAt)

        // A teammate's shared machine is never "mine", and the online-ness is
        // the caller's to decide.
        val theirs = device(
            deviceId = "dev-2",
            label = "Server",
            userId = "someone-else",
            sharedTeamIds = listOf("team-1"),
            kind = "server",
            agentAccounts = """{"claude":{"signedIn":true,"checkedAt":""}}""",
        )
        val shared = AgentAccountsRows.agentProfileUsageRows(listOf(theirs), "me") { false }
        assertEquals(1, shared.size)
        assertFalse(shared[0].mine)
        assertFalse(shared[0].online)
        // An empty `checkedAt` is nothing to say, never an "as of " with no date.
        assertNull(shared[0].checkedAt)

        // A machine that reported nothing at all contributes no rows.
        val quiet = device(deviceId = "dev-3")
        assertTrue(AgentAccountsRows.agentProfileUsageRows(listOf(quiet), "me") { true }.isEmpty())
    }

    @Test
    fun `agent profile usage rows read every profile`() {
        val studio = device(
            deviceId = "dev-1",
            label = "Studio",
            agentAccounts = """
                {"claude":{"signedIn":true,"email":"dev@acme.test","plan":"max","checkedAt":"2026-08-28T11:00:00.000Z",
                  "profiles":[
                    {"id":"system","label":"Default","active":true,"signedIn":true,"email":"dev@acme.test","plan":"max","checkedAt":"2026-08-28T11:10:00.000Z"},
                    {"id":"work","label":"Work","active":false,"signedIn":true,"email":"alex@northwind.dev","plan":"team",
                     "usage":${usageJson("2026-08-28T11:50:00.000Z", "weekly", 9)}},
                    {"id":"old","active":false,"signedIn":false}
                  ]}}
            """.trimIndent(),
            agentUsage = """{"claude":${usageJson("2026-08-28T11:55:00.000Z", "session", 73)}}""",
        )
        val rows = AgentAccountsRows.agentProfileUsageRows(listOf(studio), "me") { true }
        assertEquals(
            listOf("dev-1:claude:system", "dev-1:claude:work", "dev-1:claude:old"),
            rows.map { it.key },
        )
        // Only the ACTIVE profile falls back to the pre-profile slot.
        val active = rows[0]
        assertTrue(active.active)
        assertEquals("Default", active.profileLabel)
        assertEquals(73, AgentAccountsRows.peakPercent(active.usage))
        assertEquals("2026-08-28T11:10:00.000Z", active.checkedAt)
        // A profile with its own numbers keeps them.
        val work = rows[1]
        assertFalse(work.active)
        assertEquals("Work", work.profileLabel)
        assertEquals("alex@northwind.dev", work.email)
        assertEquals("team", work.plan)
        assertEquals(9, AgentAccountsRows.peakPercent(work.usage))
        // No own stamp: the account's probe stamp is the fallback.
        assertEquals("2026-08-28T11:00:00.000Z", work.checkedAt)
        // An inactive, label-less profile: its id names it, no fallback numbers.
        val old = rows[2]
        assertEquals("old", old.profileLabel)
        assertFalse(old.signedIn)
        assertNull(old.usage)
    }

    // ── EXP-817: the auto-refresh gate ───────────────────────────────────────

    @Test
    fun `refresh needs my own online machine with the cap`() {
        val capable = listOf("agent-login", "agent-usage-refresh")
        assertTrue(AgentAccountsRows.canRefresh(row(deviceId = "a", agent = "claude"), capable))
        assertFalse(AgentAccountsRows.canRefresh(row(deviceId = "a", agent = "claude", mine = false), capable))
        assertFalse(AgentAccountsRows.canRefresh(row(deviceId = "a", agent = "claude", online = false), capable))
        assertFalse(AgentAccountsRows.canRefresh(row(deviceId = "a", agent = "claude"), listOf("agent-login")))
        assertFalse(AgentAccountsRows.canRefresh(row(deviceId = "a", agent = "claude"), null))
    }

    @Test
    fun `a refresh inside the floor is refused`() {
        // Fetched two minutes ago: allowed again three minutes from now.
        val recent = usage("2026-08-28T11:58:00.000Z", 10)
        assertEquals(nowMs + 3 * 60_000L, AgentAccountsRows.refreshAllowedAt(recent, nowMs))
        // Past the floor, no fetch on record, or an unreadable stamp: right now.
        assertNull(AgentAccountsRows.refreshAllowedAt(usage("2026-08-28T11:50:00.000Z", 10), nowMs))
        assertNull(AgentAccountsRows.refreshAllowedAt(null, nowMs))
        assertNull(AgentAccountsRows.refreshAllowedAt(AgentUsage(fetchedAt = "not a stamp"), nowMs))
        // A stamp in the future reads as just fetched.
        assertEquals(
            nowMs + 60_000L + AgentAccountsRows.RATE_LIMITED_FLOOR_MS,
            AgentAccountsRows.refreshAllowedAt(usage("2026-08-28T12:01:00.000Z", 10), nowMs),
        )
    }

    @Test
    fun `profile health rides the synced rows`() {
        val accounts = """{"claude":{"signedIn":true,"email":"a@acme.test","health":"ok","profiles":[""" +
            """{"id":"system","active":true,"signedIn":true,"email":"a@acme.test","health":"ok"},""" +
            """{"id":"work","signedIn":true,"email":"b@acme.test","health":"needs_relogin"}]}}"""
        val rows = AgentAccountsRows.agentProfileUsageRows(
            listOf(device(deviceId = "macbook", agentAccounts = accounts)),
            "me",
        ) { true }
        assertEquals(AgentHealth.Ok, rows.first { it.profileId == "system" }.health)
        assertEquals(AgentHealth.NeedsRelogin, rows.first { it.profileId == "work" }.health)
    }
    // ── EXP-909: the Devices page's per-device fold ─────────────────────────

    @Test
    fun `device logins lead with the active login, in contract agent order`() {
        val accounts = """{"codex":{"signedIn":true,"email":"c@acme.test","plan":"plus"},""" +
            """"claude":{"signedIn":true,"email":"a@acme.test","health":"ok","profiles":[""" +
            """{"id":"work","signedIn":true,"email":"b@acme.test","label":"Work","health":"needs_relogin"},""" +
            """{"id":"system","active":true,"signedIn":true,"email":"a@acme.test","health":"ok"}]}}"""
        val rows = AgentAccountsRows.deviceLoginRows(
            steerDevice(deviceId = "studio", label = "Studio", agentAccounts = parseAgentAccounts(accounts)),
        )
        // Contract agent order (claude before codex), the ACTIVE login of each
        // agent first, and a profile-less agent yields its ambient account.
        assertEquals(
            listOf("studio:claude:system", "studio:claude:work", "studio:codex:system"),
            rows.map { it.key },
        )
        val ambient = rows[0]
        assertTrue(ambient.active)
        assertEquals("Default", ambient.profileLabel)
        assertEquals("a@acme.test", ambient.email)
        assertEquals(AgentHealth.Ok, ambient.health)
        // The device meta rides every row — the fold never re-derives it.
        assertTrue(ambient.mine)
        assertTrue(ambient.online)
        assertEquals("Studio", ambient.deviceLabel)
        val work = rows[1]
        assertFalse(work.active)
        assertEquals("Work", work.profileLabel)
        assertEquals(AgentHealth.NeedsRelogin, work.health)
        // The pre-profile machine's single ambient account is always "active".
        assertTrue(rows[2].active)
        assertEquals("Default", rows[2].profileLabel)
        // Behind the active login, the ones that need attention lead.
        val ordered = AgentAccountsRows.sortDeviceLogins(
            listOf(
                row(deviceId = "d", agent = "claude", profileId = "fine", active = false, profileLabel = "Fine"),
                row(
                    deviceId = "d",
                    agent = "claude",
                    profileId = "dead",
                    active = false,
                    profileLabel = "Dead",
                    health = AgentHealth.NeedsRelogin,
                ),
                row(deviceId = "d", agent = "claude", profileId = "system", active = true),
            ),
        )
        assertEquals(listOf("system", "dead", "fine"), ordered.map { it.profileId })
    }

    @Test
    fun `the login label is the identity, never the status`() {
        assertEquals(
            "a@acme.test",
            AgentAccountsRows.loginLabel(row(deviceId = "d", agent = "claude", email = "a@acme.test", plan = "max")),
        )
        // No email: the bare plan an agent reports instead of an address.
        assertEquals(
            "max",
            AgentAccountsRows.loginLabel(row(deviceId = "d", agent = "claude", plan = "max")),
        )
        // Neither: the login's own label — never "Not signed in" / "signed in".
        assertEquals(
            "Work",
            AgentAccountsRows.loginLabel(
                row(deviceId = "d", agent = "claude", signedIn = false, profileLabel = "Work"),
            ),
        )
        // …and the badge is where the status lives.
        assertEquals(
            "Signed out",
            AgentAccountsRows.healthBadge(row(deviceId = "d", agent = "claude", signedIn = false)),
        )
        assertNull(AgentAccountsRows.healthBadge(row(deviceId = "d", agent = "claude")))
    }

    @Test
    fun `the chip menu offers the repairs that state allows`() {
        // EXP-909: the login ROW is the only shape the menu is built from
        // now — the cross-device chip type is gone with its section.
        fun chip(
            signedIn: Boolean,
            active: Boolean,
            health: AgentHealth,
            profileId: String = "work",
        ) = row(
            deviceId = "studio",
            agent = "claude",
            profileId = profileId,
            signedIn = signedIn,
            health = health,
            active = active,
            profileLabel = "Work",
        )
        // EXP-862: signed out or expired = a sign-in and nothing else. A dead
        // credential is never "set as default": it would not work.
        val signedOut = chip(signedIn = false, active = true, health = AgentHealth.SignedOut)
        assertEquals(
            listOf("Sign in"),
            AgentAccountsRows.chipActions(
                signedOut,
                canSwitchAccount = true,
                canRemoveAccount = true,
                canAgentLogin = true,
            ),
        )
        val expired = chip(signedIn = true, active = false, health = AgentHealth.NeedsRelogin)
        assertEquals(
            listOf("Sign in"),
            AgentAccountsRows.chipActions(
                expired,
                canSwitchAccount = true,
                canRemoveAccount = true,
                canAgentLogin = true,
            ),
        )
        // Healthy and not the machine's login: both entries.
        val other = chip(signedIn = true, active = false, health = AgentHealth.Ok)
        assertEquals(
            listOf("Set as default", "Remove account"),
            AgentAccountsRows.chipActions(
                other,
                canSwitchAccount = true,
                canRemoveAccount = true,
                canAgentLogin = true,
            ),
        )
        // Healthy and already the default: only the removal.
        val current = chip(signedIn = true, active = true, health = AgentHealth.Ok)
        assertEquals(
            listOf("Remove account"),
            AgentAccountsRows.chipActions(
                current,
                canSwitchAccount = true,
                canRemoveAccount = true,
                canAgentLogin = true,
            ),
        )
    }

    @Test
    fun `the caps gate their own entries, and the ambient login is never removable`() {
        fun chip(profileId: String, active: Boolean) = row(
            deviceId = "studio",
            agent = "claude",
            profileId = profileId,
            health = AgentHealth.Ok,
            active = active,
        )
        val other = chip("work", active = false)
        // `agent_profile_use` shipped in desktop/CLI 0.14.38 and
        // `agent_profile_remove` in EXP-862; the server refuses either without
        // its cap, so an older machine simply does not offer that entry.
        assertEquals(
            listOf("Remove account"),
            AgentAccountsRows.chipActions(
                other,
                canSwitchAccount = false,
                canRemoveAccount = true,
                canAgentLogin = true,
            ),
        )
        assertEquals(
            listOf("Set as default"),
            AgentAccountsRows.chipActions(
                other,
                canSwitchAccount = true,
                canRemoveAccount = false,
                canAgentLogin = true,
            ),
        )
        // The AMBIENT login is the agent CLI's own config dir — not ours to
        // delete, whatever the machine advertises.
        assertTrue(
            AgentAccountsRows.chipActions(
                chip("system", active = true),
                canSwitchAccount = true,
                canRemoveAccount = true,
                canAgentLogin = true,
            ).isEmpty(),
        )
        // `agent_profile_remove` ALSO needs `agent-login` server-side: a
        // machine advertising `account-remove` without it offers no removal.
        assertEquals(
            listOf("Set as default"),
            AgentAccountsRows.chipActions(
                other,
                canSwitchAccount = true,
                canRemoveAccount = true,
                canAgentLogin = false,
            ),
        )
    }

    @Test
    fun `the remove confirm names the login, the machine and what survives`() {
        assertEquals(
            "Delete Claude Code · dev@acme.test on Studio? The login is removed from " +
                "this device only; the account itself is untouched.",
            AgentAccountsRows.removeAccountConfirm("Claude Code · dev@acme.test", "Studio"),
        )
    }

    // ── EXP-849: a retired agent id never renders ────────────────────────────

    @Test
    fun `an agent outside the contract is never a row, a chip or a section`() {
        // A desktop below the version floor keeps heart-beating `pi` in every
        // one of these columns; nothing this build draws may name it.
        val stale = device(
            deviceId = "old-box",
            agentAccounts = """{"pi":{"signedIn":true,"email":"pi@acme.test"},"claude":{"signedIn":true,"email":"a@acme.test"}}""",
            agentUsage = """{"pi":${usageJson("2026-08-28T11:55:00.000Z", "weekly", 99)}}""",
        )
        val rows = AgentAccountsRows.agentProfileUsageRows(listOf(stale), "me") { true }
        assertEquals(listOf("old-box:claude:system"), rows.map { it.key })
        // …and the per-device fold drops it just as hard.
        assertEquals(
            listOf("old-box:claude:system"),
            AgentAccountsRows.deviceLoginRows(
                steerDevice(
                    deviceId = "old-box",
                    agentAccounts = parseAgentAccounts(stale.agentAccounts),
                ),
            ).map { it.key },
        )
        // …and the machine's badge is its CLAUDE health, never pi's.
        assertNull(
            AgentHealthRules.badgeLabel(
                AgentHealthRules.deviceWorst(parseAgentAccounts(stale.agentAccounts))!!,
            ),
        )
    }

    // ── EXP-862: "Add account" — who can take a sign-in, and where it lands ──

    /** A relay/registry row as the Add-account rules see it. */
    private fun steerDevice(
        deviceId: String,
        label: String = deviceId,
        mine: Boolean = true,
        online: Boolean = true,
        agents: List<String>? = listOf("claude"),
        unauthedAgents: List<String> = emptyList(),
        caps: List<String>? = listOf("agent-login"),
        agentAccounts: Map<String, AgentAccount>? = null,
    ) = SteerDevice(
        deviceId = deviceId,
        deviceLabel = label,
        agents = agents,
        unauthedAgents = unauthedAgents,
        caps = caps,
        online = online,
        owner = if (mine) null else DeviceOwner(id = "someone-else", name = "Alex"),
        agentAccounts = agentAccounts,
    )

    @Test
    fun `addable agents are the installed ones, signed in or out, in contract order`() {
        assertEquals(
            listOf("claude", "codex"),
            // Reported out of order and split across the two lists.
            AgentAccountsRows.addableAgents(
                steerDevice("dev", agents = listOf("codex"), unauthedAgents = listOf("claude")),
            ),
        )
        // A duplicate across both lists is one agent, and a retired id from a
        // machine below the version floor is never offered.
        assertEquals(
            listOf("claude"),
            AgentAccountsRows.addableAgents(
                steerDevice("dev", agents = listOf("claude", "pi"), unauthedAgents = listOf("claude")),
            ),
        )
        assertTrue(
            AgentAccountsRows.addableAgents(steerDevice("dev", agents = emptyList())).isEmpty(),
        )
    }

    @Test
    fun `a new login takes the ambient slot only while it is signed out`() {
        // Nothing reported at all: the ambient config dir is free.
        assertEquals(
            AgentAccountsRows.LoginTarget(profileId = SYSTEM_PROFILE_ID, newProfileLabel = null),
            AgentAccountsRows.addAccountLoginTarget(steerDevice("dev"), "claude", "Claude Code account 2"),
        )
        // The agent is known but signed out there: still the ambient slot.
        val signedOut = steerDevice(
            "dev",
            agentAccounts = mapOf("claude" to AgentAccount(signedIn = false)),
        )
        assertEquals(
            SYSTEM_PROFILE_ID,
            AgentAccountsRows.addAccountLoginTarget(signedOut, "claude", "Claude Code account 2").profileId,
        )
        // Signed in on the ambient login: a SECOND account gets its own
        // profile, never the agent CLI's own config dir.
        val signedIn = steerDevice(
            "dev",
            agentAccounts = mapOf("claude" to AgentAccount(signedIn = true, email = "dev@acme.test")),
        )
        assertEquals(
            AgentAccountsRows.LoginTarget(profileId = null, newProfileLabel = "Claude Code account 2"),
            AgentAccountsRows.addAccountLoginTarget(signedIn, "claude", "Claude Code account 2"),
        )
        // The PROFILE's own flag wins over the pre-profile slot: an ambient
        // entry that says signed out frees the slot even when the account
        // header (another profile's identity) claims signed in.
        val ambientSignedOut = steerDevice(
            "dev",
            agentAccounts = mapOf(
                "claude" to AgentAccount(
                    signedIn = true,
                    profiles = listOf(
                        AgentAccountProfile(id = SYSTEM_PROFILE_ID, signedIn = false),
                        AgentAccountProfile(id = "work", signedIn = true, active = true),
                    ),
                ),
            ),
        )
        assertEquals(
            SYSTEM_PROFILE_ID,
            AgentAccountsRows.addAccountLoginTarget(ambientSignedOut, "claude", "x").profileId,
        )
        // A machine with only OTHER agents signed in keeps claude's slot free.
        val otherAgent = steerDevice(
            "dev",
            agentAccounts = mapOf("codex" to AgentAccount(signedIn = true)),
        )
        assertEquals(
            SYSTEM_PROFILE_ID,
            AgentAccountsRows.addAccountLoginTarget(otherAgent, "claude", "x").profileId,
        )
    }

    @Test
    fun `the next profile label counts past the logins the machine reports`() {
        // Nothing reported: the ambient login still counts as the first.
        assertEquals(
            "Claude Code account 2",
            AgentAccountsRows.nextProfileLabel(steerDevice("dev"), "claude", "Claude Code"),
        )
        val twoProfiles = steerDevice(
            "dev",
            agentAccounts = mapOf(
                "claude" to AgentAccount(
                    signedIn = true,
                    profiles = listOf(
                        AgentAccountProfile(id = SYSTEM_PROFILE_ID, signedIn = true, active = true),
                        AgentAccountProfile(id = "work", signedIn = true),
                    ),
                ),
            ),
        )
        // A profile whose label is not an "account N" takes no number.
        assertEquals(
            "Claude Code account 2",
            AgentAccountsRows.nextProfileLabel(twoProfiles, "claude", "Claude Code"),
        )
        // The label follows the AGENT asked about, not the machine's busiest.
        assertEquals(
            "Codex account 2",
            AgentAccountsRows.nextProfileLabel(twoProfiles, "codex", "Codex"),
        )
        // An empty profile list is the same "one ambient login" floor.
        val empty = steerDevice(
            "dev",
            agentAccounts = mapOf("claude" to AgentAccount(signedIn = true, profiles = emptyList())),
        )
        assertEquals(
            "Claude Code account 2",
            AgentAccountsRows.nextProfileLabel(empty, "claude", "Claude Code"),
        )
    }

    @Test
    fun `the next profile label takes the smallest free number`() {
        fun device(vararg labels: String) = steerDevice(
            "dev",
            agentAccounts = mapOf(
                "claude" to AgentAccount(
                    signedIn = true,
                    profiles = listOf(
                        AgentAccountProfile(id = SYSTEM_PROFILE_ID, signedIn = true, active = true),
                    ) + labels.mapIndexed { i, label ->
                        AgentAccountProfile(id = "p$i", label = label, signedIn = true)
                    },
                ),
            ),
        )
        // "account 2" was removed: its number is free again, never a duplicate 3.
        assertEquals(
            "Claude Code account 2",
            AgentAccountsRows.nextProfileLabel(device("Claude Code account 3"), "claude", "Claude Code"),
        )
        assertEquals(
            "Claude Code account 4",
            AgentAccountsRows.nextProfileLabel(
                device("Claude Code account 2", "Claude Code account 3"),
                "claude",
                "Claude Code",
            ),
        )
    }

    @Test
    fun `a profile label is trimmed and clamped to the server limit`() {
        assertEquals("work", AgentAccountsRows.clampProfileLabel("  work "))
        assertEquals(64, AgentAccountsRows.clampProfileLabel("x".repeat(80)).length)
    }

    // EXP-862: the sign-in sheet's self-close rule, on the TARGETED login.
    @Test
    fun loginLandedReadsTheTargetedLogin() {
        val ambientOut = AgentAccountProfile(id = SYSTEM_PROFILE_ID, signedIn = false)
        val ambientIn = AgentAccountProfile(id = SYSTEM_PROFILE_ID, signedIn = true, health = "ok")
        val work = AgentAccountProfile(id = "p-work", label = "Work", signedIn = true, health = "ok")
        val stale = AgentAccountProfile(id = "p-stale", label = "Stale", signedIn = true, health = "needs_relogin")

        assertFalse(AgentAccountsRows.loginLanded(null, null, null))
        // The ambient login: its own `system` row decides, never the header.
        assertFalse(
            AgentAccountsRows.loginLanded(AgentAccount(signedIn = true, profiles = listOf(ambientOut, work)), SYSTEM_PROFILE_ID, null),
        )
        assertTrue(
            AgentAccountsRows.loginLanded(AgentAccount(signedIn = true, profiles = listOf(ambientIn)), SYSTEM_PROFILE_ID, null),
        )
        // No profile rows at all: the top-level fields, minus needs_relogin.
        assertTrue(AgentAccountsRows.loginLanded(AgentAccount(signedIn = true, health = "ok"), null, null))
        assertFalse(AgentAccountsRows.loginLanded(AgentAccount(signedIn = true, health = "needs_relogin"), null, null))
        // A named profile is its own state; a refused one has not landed.
        val account = AgentAccount(signedIn = true, profiles = listOf(ambientIn, work, stale))
        assertTrue(AgentAccountsRows.loginLanded(account, "p-work", null))
        assertFalse(AgentAccountsRows.loginLanded(account, "p-stale", null))
        assertFalse(AgentAccountsRows.loginLanded(account, "p-missing", null))
        // A NEW profile lands when a row carrying its label is usable.
        assertTrue(AgentAccountsRows.loginLanded(account, null, " Work "))
        assertFalse(AgentAccountsRows.loginLanded(account, null, "Stale"))
        assertFalse(AgentAccountsRows.loginLanded(account, null, "Nowhere"))
        assertFalse(AgentAccountsRows.loginLanded(account, null, "  "))
    }
}
