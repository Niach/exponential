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
        health: AgentHealth = AgentHealthRules.derived(signedIn),
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
                row(deviceId = "a", agent = "codex", plan = "openai-codex (oauth)"),
                row(deviceId = "b", agent = "codex", plan = "openai-codex (oauth)"),
                row(deviceId = "a", agent = "claude", signedIn = false, email = "x@y.z"),
                row(deviceId = "b", agent = "claude", signedIn = false),
            ),
        ) { false }
        assertEquals(
            listOf("codex:a:system", "codex:b:system", "claude:a:system", "claude:b:system"),
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
        val groups = listOf("zeta", "claude", "codex", "alpha").map { agent ->
            AgentAccountsRows.accountUsageGroups(listOf(row(deviceId = "a", agent = agent))) { false }.single()
        }
        val sections = AgentAccountsRows.sections(groups)
        assertEquals(listOf("claude", "codex", "alpha", "zeta"), sections.map { it.agent })
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
            device(deviceId = "their-server", userId = "them", sharedTeamIds = listOf("team-1"), kind = "server"),
            device(deviceId = "their-desktop", userId = "them", sharedTeamIds = listOf("team-1"), kind = "desktop"),
            device(deviceId = "other-team", userId = "them", sharedTeamIds = listOf("team-2"), kind = "server"),
            // FEED-33: one server shared with both teams reads on both pages.
            device(deviceId = "both", userId = "them", sharedTeamIds = listOf("team-1", "team-2"), kind = "server"),
        )
        assertEquals(
            listOf("mine", "their-server", "both"),
            AgentAccountsRows.sectionDevices(rows, "me", "team-1").map { it.deviceId },
        )
        assertEquals(
            listOf("mine", "other-team", "both"),
            AgentAccountsRows.sectionDevices(rows, "me", "team-2").map { it.deviceId },
        )
        assertEquals(listOf("mine"), AgentAccountsRows.sectionDevices(rows, "me", null).map { it.deviceId })
        assertTrue(AgentAccountsRows.sectionDevices(rows, null, "team-1").isEmpty())
    }

    // ── EXP-849: health ──────────────────────────────────────────────────────

    @Test
    fun `a group carries the worst health of its machines`() {
        val groups = AgentAccountsRows.accountUsageGroups(
            listOf(
                row(deviceId = "a", agent = "claude", email = "me@acme.test", health = AgentHealth.Ok),
                row(deviceId = "b", agent = "claude", email = "me@acme.test", health = AgentHealth.NeedsRelogin),
            ),
        ) { false }
        assertEquals(1, groups.size)
        assertEquals(AgentHealth.NeedsRelogin, groups[0].health)
        assertEquals("Needs re-login", AgentAccountsRows.healthBadge(groups[0]))
    }

    @Test
    fun `a signed-out group says so in its badge, never in its caption`() {
        // EXP-862: the caption is the account's IDENTITY, never a status —
        // "Not signed in" as a title said what the badge already says and
        // buried the only identifying thing the row had.
        val groups = AgentAccountsRows.accountUsageGroups(
            listOf(row(deviceId = "a", agent = "claude", signedIn = false)),
        ) { false }
        assertEquals(AgentHealth.SignedOut, groups[0].health)
        assertEquals("Default", AgentAccountsRows.caption(groups[0]))
        assertEquals("Signed out", AgentAccountsRows.healthBadge(groups[0]))
    }

    @Test
    fun `an expired credential leads like a signed-out one`() {
        val groups = AgentAccountsRows.sortAccountGroupsAttentionFirst(
            AgentAccountsRows.accountUsageGroups(
                listOf(
                    row(deviceId = "a", agent = "claude", email = "hot@acme.test", usage = usage("2026-08-28T11:00:00.000Z", 96)),
                    row(
                        deviceId = "b",
                        agent = "claude",
                        email = "dead@acme.test",
                        health = AgentHealth.NeedsRelogin,
                        usage = usage("2026-08-28T11:00:00.000Z", 2),
                    ),
                ),
            ) { false },
        )
        assertEquals(
            listOf("claude:dead@acme.test", "claude:hot@acme.test"),
            groups.map { it.key },
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
    // ── EXP-849: the machine row's chips (the SETUP/REPAIR surface) ──────────

    @Test
    fun `device account chips list every login the machine holds, active first`() {
        val accounts = """{"codex":{"signedIn":true,"email":"c@acme.test","plan":"plus"},""" +
            """"claude":{"signedIn":true,"email":"a@acme.test","health":"ok","profiles":[""" +
            """{"id":"work","signedIn":true,"email":"b@acme.test","label":"Work","health":"needs_relogin"},""" +
            """{"id":"system","active":true,"signedIn":true,"email":"a@acme.test","health":"ok"}]}}"""
        val chips = AgentAccountsRows.deviceAccountChips(
            parseAgentAccounts(accounts),
        )
        // Contract agent order (claude before codex), the ACTIVE login of each
        // agent first, and a profile-less agent yields its ambient account.
        assertEquals(
            listOf("claude:system", "claude:work", "codex:system"),
            chips.map { it.key },
        )
        val ambient = chips[0]
        assertTrue(ambient.active)
        assertEquals("Default", ambient.profileLabel)
        assertEquals("a@acme.test", ambient.email)
        assertEquals(AgentHealth.Ok, ambient.health)
        val work = chips[1]
        assertFalse(work.active)
        assertEquals("Work", work.profileLabel)
        assertEquals(AgentHealth.NeedsRelogin, work.health)
        // The pre-profile machine's single ambient account is always "active".
        assertTrue(chips[2].active)
        assertEquals("Default", chips[2].profileLabel)
    }

    @Test
    fun `a machine chip names the agent and the login`() {
        val chips = AgentAccountsRows.deviceAccountChips(
            parseAgentAccounts("""{"claude":{"signedIn":true,"email":"a@acme.test"}}"""),
        )
        assertEquals(
            "Claude · a@acme.test",
            AgentAccountsRows.machineChipLabel(chips[0]) { it.replaceFirstChar(Char::uppercase) },
        )
        // No email: the plan an agent reports instead of an address.
        val plan = AgentAccountsRows.deviceAccountChips(
            parseAgentAccounts("""{"claude":{"signedIn":true,"plan":"max"}}"""),
        )
        assertEquals("claude · max", AgentAccountsRows.machineChipLabel(plan[0]) { it })
        // Signed out: the profile's own label, never "signed in".
        val out = AgentAccountsRows.deviceAccountChips(
            parseAgentAccounts("""{"claude":{"signedIn":false}}"""),
        )
        assertEquals("claude · Default", AgentAccountsRows.machineChipLabel(out[0]) { it })
    }

    @Test
    fun `the chip menu offers the repairs that state allows`() {
        fun chip(
            signedIn: Boolean,
            active: Boolean,
            health: AgentHealth,
            profileId: String = "work",
        ) = DeviceAccountChip(
            key = "claude:$profileId",
            agent = "claude",
            profileId = profileId,
            profileLabel = "Work",
            signedIn = signedIn,
            active = active,
            email = null,
            plan = null,
            health = health,
        )
        // EXP-862: signed out or expired = a sign-in and nothing else. A dead
        // credential is never "set as default": it would not work.
        val signedOut = chip(signedIn = false, active = true, health = AgentHealth.SignedOut)
        assertEquals(
            listOf("Sign in"),
            AgentAccountsRows.chipActions(signedOut, canSwitchAccount = true, canRemoveAccount = true),
        )
        val expired = chip(signedIn = true, active = false, health = AgentHealth.NeedsRelogin)
        assertEquals(
            listOf("Sign in"),
            AgentAccountsRows.chipActions(expired, canSwitchAccount = true, canRemoveAccount = true),
        )
        // Healthy and not the machine's login: both entries.
        val other = chip(signedIn = true, active = false, health = AgentHealth.Ok)
        assertEquals(
            listOf("Set as default", "Remove account"),
            AgentAccountsRows.chipActions(other, canSwitchAccount = true, canRemoveAccount = true),
        )
        // Healthy and already the default: only the removal.
        val current = chip(signedIn = true, active = true, health = AgentHealth.Ok)
        assertEquals(
            listOf("Remove account"),
            AgentAccountsRows.chipActions(current, canSwitchAccount = true, canRemoveAccount = true),
        )
    }

    @Test
    fun `the caps gate their own entries, and the ambient login is never removable`() {
        fun chip(profileId: String, active: Boolean) = DeviceAccountChip(
            key = "claude:$profileId",
            agent = "claude",
            profileId = profileId,
            profileLabel = "Default",
            signedIn = true,
            active = active,
            email = null,
            plan = null,
            health = AgentHealth.Ok,
        )
        val other = chip("work", active = false)
        // `agent_profile_use` shipped in desktop/CLI 0.14.38 and
        // `agent_profile_remove` in EXP-862; the server refuses either without
        // its cap, so an older machine simply does not offer that entry.
        assertEquals(
            listOf("Remove account"),
            AgentAccountsRows.chipActions(other, canSwitchAccount = false, canRemoveAccount = true),
        )
        assertEquals(
            listOf("Set as default"),
            AgentAccountsRows.chipActions(other, canSwitchAccount = true, canRemoveAccount = false),
        )
        // The AMBIENT login is the agent CLI's own config dir — not ours to
        // delete, whatever the machine advertises.
        assertTrue(
            AgentAccountsRows.chipActions(
                chip("system", active = true),
                canSwitchAccount = true,
                canRemoveAccount = true,
            ).isEmpty(),
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
        assertEquals(
            listOf("claude"),
            AgentAccountsRows.sections(
                AgentAccountsRows.accountUsageGroups(rows) { false },
            ).map { it.agent },
        )
        assertEquals(
            listOf("claude:system"),
            AgentAccountsRows.deviceAccountChips(parseAgentAccounts(stale.agentAccounts)).map { it.key },
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
    fun `add account offers only my online machines that can sign in`() {
        val mine = steerDevice("mine")
        val candidates = listOf(
            mine,
            steerDevice("theirs", mine = false),
            steerDevice("offline", online = false),
            // Too old to take the command: `agent-login` is strictly gated.
            steerDevice("old", caps = emptyList()),
            steerDevice("ancient", caps = null),
            // Online and capable, but nothing installed to sign in to.
            steerDevice("bare", agents = emptyList()),
        )
        assertEquals(
            listOf("mine"),
            AgentAccountsRows.addAccountDevices(candidates).map { it.deviceId },
        )

        // An agent filter keeps only the machines that have it installed —
        // signed out counts, that is the whole point of a sign-in.
        val codexBox = steerDevice("codex-box", agents = emptyList(), unauthedAgents = listOf("codex"))
        val both = listOf(mine, codexBox)
        assertEquals(
            listOf("codex-box"),
            AgentAccountsRows.addAccountDevices(both, agent = "codex").map { it.deviceId },
        )
        assertEquals(
            listOf("mine"),
            AgentAccountsRows.addAccountDevices(both, agent = "claude").map { it.deviceId },
        )

        // `exclude` drops the machines that already hold the account — the
        // per-account "+" chip offers the rest.
        assertEquals(
            listOf("codex-box"),
            AgentAccountsRows.addAccountDevices(both, exclude = setOf("mine")).map { it.deviceId },
        )
        assertTrue(
            AgentAccountsRows.addAccountDevices(both, exclude = setOf("mine", "codex-box")).isEmpty(),
        )
    }

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
        assertEquals(
            "Claude Code account 3",
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
