package com.exponential.app.domain

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentAccountProfile
import com.exponential.app.data.api.DeviceOwner
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.data.api.AgentUsageWindow
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
 * (`usage_bar.rs`): one row per profile a MACHINE reports, in its own order,
 * and an identity label (the email) that never doubles as a status.
 */
class AgentAccountsRowsTest {

    private val nowMs = Instant.parse("2026-08-28T12:00:00Z").toEpochMilli()

    private fun row(
        deviceId: String,
        agent: String,
        profileId: String = "work",
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
    ) = AgentProfileUsageRow(
        key = "$deviceId:$agent:$profileId",
        deviceId = deviceId,
        deviceLabel = deviceLabel,
        mine = mine,
        online = online,
        agent = agent,
        profileId = profileId,
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
    fun `agent profile usage rows never synthesize a login from the top level`() {
        // A device that reports top-level fields (or only usage) but no
        // profiles has no login to list: the ambient one is never a row.
        val studio = device(
            deviceId = "dev-1",
            label = "Studio",
            agentAccounts = """{"claude":{"signedIn":true,"email":"dev@acme.test","plan":"max","checkedAt":"2026-08-28T11:00:00.000Z","importable":{"email":"dev@acme.test"}}}""",
            agentUsage = """{"claude":${usageJson("2026-08-28T11:55:00.000Z", "session", 42)},"codex":${usageJson("2026-08-28T11:55:00.000Z", "weekly", 8)}}""",
            agentUsageAt = "2026-08-28T11:30:00.000Z",
        )
        assertTrue(AgentAccountsRows.agentProfileUsageRows(listOf(studio), "me") { true }.isEmpty())

        // A teammate's shared machine is never "mine", and the online-ness is
        // the caller's to decide.
        val theirs = device(
            deviceId = "dev-2",
            label = "Server",
            userId = "someone-else",
            sharedTeamIds = listOf("team-1"),
            kind = "server",
            agentAccounts = """{"claude":{"signedIn":true,"checkedAt":"","profiles":[{"id":"p-1","active":true,"signedIn":true,"checkedAt":""}]}}""",
            agentUsageAt = "2026-08-28T11:30:00.000Z",
        )
        val shared = AgentAccountsRows.agentProfileUsageRows(listOf(theirs), "me") { false }
        assertEquals(listOf("dev-2:claude:p-1"), shared.map { it.key })
        assertFalse(shared[0].mine)
        assertFalse(shared[0].online)
        // An empty `checkedAt` is nothing to say: the device's `agent_usage_at`
        // is the fallback, never an "as of " with no date.
        assertEquals("2026-08-28T11:30:00.000Z", shared[0].checkedAt)

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
                    {"id":"p-dev","label":"Default","active":true,"signedIn":true,"email":"dev@acme.test","plan":"max","checkedAt":"2026-08-28T11:10:00.000Z"},
                    {"id":"work","label":"Work","active":false,"signedIn":true,"email":"alex@northwind.dev","plan":"team",
                     "usage":${usageJson("2026-08-28T11:50:00.000Z", "weekly", 9)}},
                    {"id":"old","active":false,"signedIn":false}
                  ]}}
            """.trimIndent(),
            agentUsage = """{"claude":${usageJson("2026-08-28T11:55:00.000Z", "session", 73)}}""",
        )
        val rows = AgentAccountsRows.agentProfileUsageRows(listOf(studio), "me") { true }
        assertEquals(
            listOf("dev-1:claude:p-dev", "dev-1:claude:work", "dev-1:claude:old"),
            rows.map { it.key },
        )
        // Only the ACTIVE profile falls back to the pre-profile slot.
        val active = rows[0]
        assertTrue(active.active)
        // A legacy `label` key from an older device is ignored: the email names it.
        assertEquals("dev@acme.test", AgentAccountsRows.loginLabel(active))
        assertEquals(73, AgentAccountsRows.peakPercent(active.usage))
        assertEquals("2026-08-28T11:10:00.000Z", active.checkedAt)
        // A profile with its own numbers keeps them.
        val work = rows[1]
        assertFalse(work.active)
        assertEquals("alex@northwind.dev", work.email)
        assertEquals("team", work.plan)
        assertEquals(9, AgentAccountsRows.peakPercent(work.usage))
        // No own stamp: the account's probe stamp is the fallback.
        assertEquals("2026-08-28T11:00:00.000Z", work.checkedAt)
        // An inactive profile with no address: "No email", no fallback numbers.
        val old = rows[2]
        assertEquals("No email", AgentAccountsRows.loginLabel(old))
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
            """{"id":"p-a","active":true,"signedIn":true,"email":"a@acme.test","health":"ok"},""" +
            """{"id":"work","signedIn":true,"email":"b@acme.test","health":"needs_relogin"}]}}"""
        val rows = AgentAccountsRows.agentProfileUsageRows(
            listOf(device(deviceId = "macbook", agentAccounts = accounts)),
            "me",
        ) { true }
        assertEquals(AgentHealth.Ok, rows.first { it.profileId == "p-a" }.health)
        assertEquals(AgentHealth.NeedsRelogin, rows.first { it.profileId == "work" }.health)
    }
    // ── EXP-909: the Devices page's per-device fold ─────────────────────────

    @Test
    fun `device logins keep the device's order, in contract agent order`() {
        val accounts = """{"codex":{"signedIn":true,"email":"c@acme.test","plan":"plus","profiles":[""" +
            """{"id":"cx","active":true,"signedIn":true,"email":"c@acme.test","plan":"plus"}]},""" +
            """"claude":{"signedIn":true,"email":"a@acme.test","health":"ok","profiles":[""" +
            """{"id":"work","signedIn":true,"email":"b@acme.test","health":"needs_relogin"},""" +
            """{"id":"p-a","active":true,"signedIn":true,"email":"a@acme.test","health":"ok"}]}}"""
        val rows = AgentAccountsRows.deviceLoginRows(
            steerDevice(deviceId = "studio", label = "Studio", agentAccounts = parseAgentAccounts(accounts)),
        )
        // Contract agent order (claude before codex), then the order the
        // device SENDS — the active login is not hoisted.
        assertEquals(
            listOf("studio:claude:work", "studio:claude:p-a", "studio:codex:cx"),
            rows.map { it.key },
        )
        val work = rows[0]
        assertFalse(work.active)
        assertEquals(AgentHealth.NeedsRelogin, work.health)
        // The device meta rides every row — the fold never re-derives it.
        assertTrue(work.mine)
        assertTrue(work.online)
        assertEquals("Studio", work.deviceLabel)
        val active = rows[1]
        assertTrue(active.active)
        assertEquals("a@acme.test", active.email)
        assertEquals(AgentHealth.Ok, active.health)
        // A profile-less agent yields nothing, whatever its top level says.
        val bare = AgentAccountsRows.deviceLoginRows(
            steerDevice(
                deviceId = "studio",
                agentAccounts = mapOf("codex" to AgentAccount(signedIn = true, email = "c@acme.test")),
            ),
        )
        assertTrue(bare.isEmpty())
        // The sort is stable within an agent: attention and activity never reorder.
        val ordered = AgentAccountsRows.sortDeviceLogins(
            listOf(
                row(deviceId = "d", agent = "codex", profileId = "c1"),
                row(deviceId = "d", agent = "claude", profileId = "fine", active = false),
                row(deviceId = "d", agent = "claude", profileId = "dead", active = false, health = AgentHealth.NeedsRelogin),
                row(deviceId = "d", agent = "claude", profileId = "last", active = true),
            ),
        )
        assertEquals(listOf("fine", "dead", "last", "c1"), ordered.map { it.profileId })
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
        // Neither: "No email" — never the profile's internal label (EXP-1013).
        assertEquals(
            "No email",
            AgentAccountsRows.loginLabel(
                row(deviceId = "d", agent = "claude", signedIn = false),
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
        ) = row(
            deviceId = "studio",
            agent = "claude",
            signedIn = signedIn,
            health = health,
            active = active,
        )
        // EXP-944: a dead credential IS removable; the removal deletes a profile
        // dir and the credential's state never decided whether that is possible.
        val signedOut = chip(signedIn = false, active = true, health = AgentHealth.SignedOut)
        assertEquals(
            listOf("Sign in", "Remove account"),
            AgentAccountsRows.chipActions(
                signedOut,
                canRemoveAccount = true,
                canAgentLogin = true,
            ),
        )
        val expired = chip(signedIn = true, active = false, health = AgentHealth.NeedsRelogin)
        assertEquals(
            listOf("Sign in", "Remove account"),
            AgentAccountsRows.chipActions(
                expired,
                canRemoveAccount = true,
                canAgentLogin = true,
            ),
        )
        // EXP-1158: healthy, last used or not, the menu is the same — there
        // is no "make it the default" entry any more.
        listOf(true, false).forEach { active ->
            assertEquals(
                listOf("Remove account"),
                AgentAccountsRows.chipActions(
                    chip(signedIn = true, active = active, health = AgentHealth.Ok),
                    canRemoveAccount = true,
                    canAgentLogin = true,
                ),
            )
        }
    }

    @Test
    fun `the caps gate their own entries`() {
        fun chip(profileId: String, active: Boolean) = row(
            deviceId = "studio",
            agent = "claude",
            profileId = profileId,
            health = AgentHealth.Ok,
            active = active,
        )
        val other = chip("work", active = false)
        // `agent_profile_remove` shipped in EXP-862; the server refuses it
        // without its cap, so an older machine simply does not offer it.
        assertEquals(
            listOf("Remove account"),
            AgentAccountsRows.chipActions(
                other,
                canRemoveAccount = true,
                canAgentLogin = true,
            ),
        )
        assertTrue(
            AgentAccountsRows.chipActions(
                other,
                canRemoveAccount = false,
                canAgentLogin = true,
            ).isEmpty(),
        )
        // The last used login is removable like any other: every row is a
        // profile dir Exponential created.
        assertEquals(
            listOf("Remove account"),
            AgentAccountsRows.chipActions(
                chip("p-a", active = true),
                canRemoveAccount = true,
                canAgentLogin = true,
            ),
        )
        // `agent_profile_remove` ALSO needs `agent-login` server-side: a
        // machine advertising `account-remove` without it offers no removal.
        assertTrue(
            AgentAccountsRows.chipActions(
                other,
                canRemoveAccount = true,
                canAgentLogin = false,
            ).isEmpty(),
        )
    }

    // EXP-1137: a build with the sign-out body offers "Sign out" on every
    // signed-in login; the fixed order ×4 is sign in, sign out, remove.
    @Test
    fun `a build that signs out offers it`() {
        fun chip(
            signedIn: Boolean,
            active: Boolean,
            health: AgentHealth,
        ) = row(
            deviceId = "mint",
            agent = "codex",
            signedIn = signedIn,
            health = health,
            active = active,
        )
        val all = { row: AgentProfileUsageRow ->
            AgentAccountsRows.chipActions(
                row,
                canRemoveAccount = true,
                canAgentLogin = true,
                canSignOutAccount = true,
            )
        }
        // A named login, healthy: every entry but the sign-in.
        assertEquals(
            listOf("Sign out", "Remove account"),
            all(chip(signedIn = true, active = false, health = AgentHealth.Ok)),
        )
        // A revoked credential still signs out: that is how it leaves.
        assertEquals(
            listOf("Sign in", "Sign out", "Remove account"),
            all(chip(signedIn = true, active = false, health = AgentHealth.NeedsRelogin)),
        )
        // A signed-out named login has nothing to sign out of.
        assertEquals(
            listOf("Sign in", "Remove account"),
            all(chip(signedIn = false, active = false, health = AgentHealth.SignedOut)),
        )
        // The sign-out cap alone never removes a profile, and `agent-login`
        // is required for everything.
        assertEquals(
            listOf("Sign out"),
            AgentAccountsRows.chipActions(
                chip(signedIn = true, active = false, health = AgentHealth.Ok),
                canRemoveAccount = false,
                canAgentLogin = true,
                canSignOutAccount = true,
            ),
        )
        assertTrue(
            AgentAccountsRows.chipActions(
                chip(signedIn = true, active = true, health = AgentHealth.Ok),
                canRemoveAccount = true,
                canAgentLogin = false,
                canSignOutAccount = true,
            ).isEmpty(),
        )
    }

    @Test
    fun `the sign-out confirm is the pinned sentence`() {
        assertEquals(
            "Sign me@example.com out on mint? The login stays listed so it can sign in " +
                "again; the account itself is untouched.",
            AgentAccountsRows.signOutConfirm("me@example.com", "mint"),
        )
        assertEquals(
            "That machine runs an older Exponential app that cannot sign agent accounts out. Update it first.",
            AgentAccountsRows.SIGN_OUT_OLD_APP,
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
            agentAccounts = """{"pi":{"signedIn":true,"email":"pi@acme.test","profiles":[{"id":"p","active":true,"signedIn":true}]},""" +
                """"claude":{"signedIn":true,"email":"a@acme.test","profiles":[{"id":"c","active":true,"signedIn":true,"email":"a@acme.test"}]}}""",
            agentUsage = """{"pi":${usageJson("2026-08-28T11:55:00.000Z", "weekly", 99)}}""",
        )
        val rows = AgentAccountsRows.agentProfileUsageRows(listOf(stale), "me") { true }
        assertEquals(listOf("old-box:claude:c"), rows.map { it.key })
        // …and the per-device fold drops it just as hard.
        assertEquals(
            listOf("old-box:claude:c"),
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

    // ── EXP-862: "Add account" — who can take a sign-in ──

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

    // The sign-in sheet's self-close rule: a fresh `lastLoginAt` against the
    // baseline taken when the command was queued.
    private fun profile(id: String, email: String, lastLoginAt: String? = null) =
        AgentAccountProfile(id = id, email = email, signedIn = true, lastLoginAt = lastLoginAt)

    @Test
    fun `a login lands on a fresh lastLoginAt`() {
        val before = AgentAccount(
            signedIn = true,
            profiles = listOf(
                profile("a", "a@acme.test", "2026-10-01T00:00:00Z"),
                profile("b", "b@acme.test"),
                profile("c", "c@acme.test", "2026-09-01T00:00:00Z"),
            ),
        )
        val baseline = AgentAccountsRows.loginBaseline(before)
        assertEquals(
            mapOf("a" to "2026-10-01T00:00:00Z", "b" to null, "c" to "2026-09-01T00:00:00Z"),
            baseline,
        )
        assertNull(AgentAccountsRows.loginBaseline(null)["a"])
        // Nothing moved: nothing landed.
        assertNull(AgentAccountsRows.loginLanding(before, baseline, "c"))
        assertNull(AgentAccountsRows.loginLanding(null, baseline, "c"))

        // C was signed in again as C: the intended login, no duplicate.
        val repaired = before.copy(
            profiles = before.profiles!!.map {
                if (it.id == "c") it.copy(lastLoginAt = "2026-10-10T00:00:00Z") else it
            },
        )
        assertEquals(
            AgentAccountsRows.LoginLanding("c", "c@acme.test", duplicate = false),
            AgentAccountsRows.loginLanding(repaired, baseline, "c"),
        )

        // Add account with a NEW email: a new id with a stamp, no duplicate.
        val added = before.copy(
            profiles = before.profiles!! + profile("d", "d@acme.test", "2026-10-10T00:00:00Z"),
        )
        assertEquals(
            AgentAccountsRows.LoginLanding("d", "d@acme.test", duplicate = false),
            AgentAccountsRows.loginLanding(added, baseline, null),
        )
        // A new id with no stamp yet has not landed.
        val unstamped = before.copy(profiles = before.profiles!! + profile("e", "e@acme.test"))
        assertNull(AgentAccountsRows.loginLanding(unstamped, baseline, null))
    }

    @Test
    fun `a login that refreshed another profile is a duplicate`() {
        val before = AgentAccount(
            signedIn = true,
            profiles = listOf(
                profile("a", "a@acme.test", "2026-10-01T00:00:00Z"),
                profile("b", "b@acme.test"),
                profile("c", "c@acme.test"),
            ),
        )
        val baseline = AgentAccountsRows.loginBaseline(before)
        // Sign in on C, but the browser signed in as A: A refreshed, C untouched.
        val refreshedA = before.copy(
            profiles = before.profiles!!.map {
                if (it.id == "a") it.copy(lastLoginAt = "2026-10-10T00:00:00Z") else it
            },
        )
        val onC = AgentAccountsRows.loginLanding(refreshedA, baseline, "c")!!
        assertEquals(AgentAccountsRows.LoginLanding("a", "a@acme.test", duplicate = true), onC)
        // Add account as an email the device already holds: a duplicate too.
        assertTrue(AgentAccountsRows.loginLanding(refreshedA, baseline, null)!!.duplicate)
        // B had never stamped: its first stamp under another intent still counts.
        val refreshedB = before.copy(
            profiles = before.profiles!!.map {
                if (it.id == "b") it.copy(lastLoginAt = "2026-10-10T00:00:00Z") else it
            },
        )
        assertTrue(AgentAccountsRows.loginLanding(refreshedB, baseline, "c")!!.duplicate)
        assertEquals(
            "a@acme.test was already added. Refreshed it.",
            AgentAccountsRows.alreadyAddedToast(onC.email!!),
        )
    }
}
