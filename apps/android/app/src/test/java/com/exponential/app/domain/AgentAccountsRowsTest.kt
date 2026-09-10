package com.exponential.app.domain

import com.exponential.app.data.api.AgentUsage
import com.exponential.app.data.api.AgentUsageWindow
import com.exponential.app.data.api.SYSTEM_PROFILE_ID
import com.exponential.app.data.db.DeviceEntity
import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-829: the Devices page's Accounts section rule, on the same fixtures
 * and the same test names as web (`agent-usage.test.ts`, `accountUsageGroups`)
 * and the desktop (`usage_bar.rs`): one row per account, the freshest
 * machine's numbers, attention first.
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
    ) = AgentProfileUsageRow(
        key = "$deviceId:$agent:$profileId",
        deviceId = deviceId,
        deviceLabel = deviceLabel,
        mine = mine,
        online = online,
        agent = agent,
        profileId = profileId,
        profileLabel = "Default",
        active = true,
        signedIn = signedIn,
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
        sharedTeamId: String? = null,
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
        sharedTeamId = sharedTeamId,
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
            sharedTeamId = "team-1",
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
        assertEquals("Studio · Work", AgentAccountsRows.chipLabel(work))
        assertEquals("Studio", AgentAccountsRows.chipLabel(active))
    }

    // ── accountUsageGroups ───────────────────────────────────────────────────

    @Test
    fun `merges the same email across machines, freshest report first`() {
        val groups = AgentAccountsRows.accountUsageGroups(
            listOf(
                row(
                    deviceId = "server", agent = "claude", email = "Dev@Acme.test", plan = "max",
                    online = false,
                    usage = usage("2026-08-26T10:00:00.000Z", 69),
                    checkedAt = "2026-08-27T10:00:00.000Z",
                ),
                row(
                    deviceId = "macbook", agent = "claude", email = "dev@acme.test",
                    usage = usage("2026-08-28T11:00:00.000Z", 75),
                    checkedAt = "2026-08-28T11:00:00.000Z",
                ),
                row(
                    deviceId = "mint", agent = "claude", email = "other@acme.test",
                    usage = usage("2026-08-28T11:30:00.000Z", 46),
                ),
            ),
        ) { false }
        assertEquals(listOf("claude:dev@acme.test", "claude:other@acme.test"), groups.map { it.key })
        val shared = groups[0]
        // The chips: online machines lead.
        assertEquals(listOf("macbook", "server"), shared.rows.map { it.deviceId })
        // The numbers are the FRESHEST member's, the plan the first one named.
        assertEquals(75, AgentAccountsRows.peakPercent(shared.usage))
        assertEquals("max", shared.plan)
        assertEquals("2026-08-28T11:00:00.000Z", shared.checkedAt)
        assertNull(shared.refreshTarget)
    }

    @Test
    fun `keeps email-less and signed-out rows apart — nothing to merge on`() {
        val groups = AgentAccountsRows.accountUsageGroups(
            listOf(
                row(deviceId = "a", agent = "pi", plan = "openai-codex (oauth)"),
                row(deviceId = "b", agent = "pi", plan = "openai-codex (oauth)"),
                row(deviceId = "a", agent = "claude", signedIn = false, email = "x@y.z"),
                row(deviceId = "b", agent = "claude", signedIn = false),
            ),
        ) { false }
        assertEquals(
            listOf("pi:a:system", "pi:b:system", "claude:a:system", "claude:b:system"),
            groups.map { it.key },
        )
        assertFalse(groups[2].signedIn)
    }

    @Test
    fun `prefers a non-stale report on a tie and a report with windows over none`() {
        val groups = AgentAccountsRows.accountUsageGroups(
            listOf(
                row(
                    deviceId = "a", agent = "claude", email = "dev@acme.test",
                    usage = usage("2026-08-28T11:00:00.000Z", 10, stale = true),
                ),
                row(
                    deviceId = "b", agent = "claude", email = "dev@acme.test",
                    usage = usage("2026-08-28T11:00:00.000Z", 20),
                ),
                row(deviceId = "c", agent = "claude", email = "dev@acme.test"),
            ),
        ) { false }
        assertEquals(1, groups.size)
        assertEquals(20, AgentAccountsRows.peakPercent(groups[0].usage))
        // Windows beat none on an otherwise equal pair.
        val withWindows = AgentUsage(fetchedAt = "2026-08-28T11:00:00.000Z", windows = usage("x", 5).windows)
        val without = AgentUsage(fetchedAt = "2026-08-28T11:00:00.000Z")
        val pair = AgentAccountsRows.accountUsageGroups(
            listOf(
                row(deviceId = "a", agent = "claude", email = "dev@acme.test", usage = without),
                row(deviceId = "b", agent = "claude", email = "dev@acme.test", usage = withWindows),
            ),
        ) { false }
        assertEquals(5, AgentAccountsRows.peakPercent(pair[0].usage))
    }

    @Test
    fun `targets the refresh at the eligible member with the freshest numbers`() {
        val groups = AgentAccountsRows.accountUsageGroups(
            listOf(
                row(
                    deviceId = "stale-but-capable", agent = "claude", email = "dev@acme.test",
                    usage = usage("2026-08-26T10:00:00.000Z", 69),
                ),
                row(
                    deviceId = "fresh-and-capable", agent = "claude", email = "dev@acme.test",
                    usage = usage("2026-08-28T11:00:00.000Z", 75),
                ),
                row(
                    deviceId = "freshest-but-not-mine", agent = "claude", email = "dev@acme.test",
                    mine = false,
                    usage = usage("2026-08-28T11:30:00.000Z", 75),
                ),
            ),
        ) { it.mine }
        assertEquals("fresh-and-capable", groups[0].refreshTarget?.deviceId)
        // The group's own numbers still come from the freshest member of all.
        assertEquals("2026-08-28T11:30:00.000Z", groups[0].usage?.fetchedAt)
    }

    @Test
    fun `orders groups attention first — signed out, then danger, then the rest`() {
        val groups = AgentAccountsRows.sortAccountGroupsAttentionFirst(
            AgentAccountsRows.accountUsageGroups(
                listOf(
                    row(deviceId = "a", agent = "claude", email = "low@acme.test", usage = usage("2026-08-28T11:00:00.000Z", 10)),
                    row(deviceId = "a", agent = "codex", email = "hot@acme.test", usage = usage("2026-08-28T11:00:00.000Z", 96)),
                    row(deviceId = "b", agent = "claude", signedIn = false),
                    row(deviceId = "a", agent = "claude", email = "mid@acme.test", usage = usage("2026-08-28T11:00:00.000Z", 60)),
                ),
            ) { false },
        )
        assertEquals(
            listOf("claude:b:system", "codex:hot@acme.test", "claude:mid@acme.test", "claude:low@acme.test"),
            groups.map { it.key },
        )
    }

    @Test
    fun `folds the synced device rows end to end`() {
        // Two machines, one login: the page shows ONE row with two chips.
        val devices = listOf("macbook", "server").map { id ->
            device(
                deviceId = id,
                agentAccounts = """{"claude":{"signedIn":true,"email":"dev@acme.test","plan":"max","checkedAt":"2026-08-28T11:00:00.000Z"}}""",
                agentUsage = """{"claude":${usageJson("2026-08-28T11:00:00.000Z", "weekly", 75)}}""",
                caps = """["agent-usage-refresh"]""",
            )
        }
        val rows = AgentAccountsRows.agentProfileUsageRows(devices, "me") { true }
        val groups = AgentAccountsRows.accountUsageGroups(rows) { true }
        assertEquals(1, groups.size)
        assertEquals(2, groups[0].rows.size)
        assertEquals("macbook", groups[0].refreshTarget?.deviceId)
        assertEquals("dev@acme.test", AgentAccountsRows.caption(groups[0]))
    }

    // ── The section's own rules ──────────────────────────────────────────────

    @Test
    fun `sections follow the contract agent order`() {
        val groups = listOf("pi", "zeta", "claude", "codex", "alpha").map { agent ->
            AgentAccountsRows.accountUsageGroups(listOf(row(deviceId = "a", agent = agent))) { false }.single()
        }
        val sections = AgentAccountsRows.sections(groups)
        assertEquals(listOf("claude", "codex", "pi", "alpha", "zeta"), sections.map { it.agent })
    }

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
    fun `the section reads my machines plus the servers shared with the team`() {
        val rows = listOf(
            device(deviceId = "mine"),
            device(deviceId = "their-server", userId = "them", sharedTeamId = "team-1", kind = "server"),
            device(deviceId = "their-desktop", userId = "them", sharedTeamId = "team-1", kind = "desktop"),
            device(deviceId = "other-team", userId = "them", sharedTeamId = "team-2", kind = "server"),
        )
        assertEquals(
            listOf("mine", "their-server"),
            AgentAccountsRows.sectionDevices(rows, "me", "team-1").map { it.deviceId },
        )
        assertEquals(listOf("mine"), AgentAccountsRows.sectionDevices(rows, "me", null).map { it.deviceId })
        assertTrue(AgentAccountsRows.sectionDevices(rows, null, "team-1").isEmpty())
    }
}
