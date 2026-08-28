package com.exponential.app.domain

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DeviceEntity
import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-484: the agent auth + usage presentation rules, on the SAME fixture and
 * the same test names as web, iOS and the desktop. These strings and
 * thresholds are the cross-client contract.
 */
class AgentUsagePresentationTest {

    private val nowMs = Instant.parse("2026-08-28T10:00:00Z").toEpochMilli()

    private val usageJson = """
        {
          "fetchedAt": "2026-08-28T09:58:00Z",
          "stale": false,
          "windows": [
            {"key": "session", "label": "5h", "percent": 42, "resetsAt": "2026-08-28T12:10:30Z"},
            {"key": "weekly", "label": "Week", "percent": 78, "resetsAt": "2026-09-01T00:00:00Z"},
            {"key": "model:fable", "label": "Fable", "percent": 96, "resetsAt": null}
          ]
        }
    """.trimIndent()

    private val accountsJson = """
        {
          "claude": {"signedIn": true, "email": "danny@yourev.at", "plan": "max", "checkedAt": "2026-08-28T09:58:00Z"},
          "codex": {"signedIn": false, "checkedAt": "2026-08-28T09:58:00Z"},
          "pi": {"signedIn": true, "plan": "anthropic (oauth)", "checkedAt": "2026-08-28T09:58:00Z"}
        }
    """.trimIndent()

    private fun usageMapJson(agent: String = "claude") = """{"$agent": $usageJson}"""

    @Test
    fun `parse clamps percent and drops malformed windows`() {
        val usage = AgentUsagePresentation.parseUsage(usageJson)!!
        assertEquals("2026-08-28T09:58:00Z", usage.fetchedAt)
        assertFalse(usage.stale)
        assertEquals(listOf("session", "weekly", "model:fable"), usage.windows.map { it.key })
        assertEquals("Fable", usage.windows[2].label)
        assertEquals(96.0, usage.windows[2].percent, 0.001)
        assertNull(usage.windows[2].resetsAt)

        // Out-of-range percents clamp; a keyless / non-object window is dropped
        // instead of failing the whole snapshot.
        val messy = AgentUsagePresentation.parseUsage(
            """
            {
              "fetchedAt": "2026-08-28T09:58:00Z",
              "stale": true,
              "windows": [
                {"key": "session", "percent": 150},
                {"key": "weekly", "percent": -5},
                {"label": "keyless", "percent": 10},
                "nonsense",
                {"key": "credits"}
              ]
            }
            """.trimIndent(),
        )!!
        assertTrue(messy.stale)
        assertEquals(listOf("session", "weekly", "credits"), messy.windows.map { it.key })
        assertEquals(100.0, messy.windows[0].percent, 0.001)
        assertEquals(0.0, messy.windows[1].percent, 0.001)
        assertEquals("", messy.windows[2].label)

        // Unusable input is null, never an empty shell.
        assertNull(AgentUsagePresentation.parseUsage(null))
        assertNull(AgentUsagePresentation.parseUsage(""))
        assertNull(AgentUsagePresentation.parseUsage("not json"))
        assertNull(AgentUsagePresentation.parseUsage("""["nope"]"""))

        val map = AgentUsagePresentation.parseUsageMap(usageMapJson())!!
        assertEquals(setOf("claude"), map.keys)
        assertEquals(3, map.getValue("claude").windows.size)
        assertNull(AgentUsagePresentation.parseUsageMap("nope"))
    }

    @Test
    fun `selectWindow prefers remembered key else max percent`() {
        val usage = AgentUsagePresentation.parseUsage(usageJson)

        // No preference: the busiest window.
        assertEquals("model:fable", AgentUsagePresentation.selectWindow(usage)?.key)
        assertEquals("model:fable", AgentUsagePresentation.selectWindow(usage, null)?.key)
        // A remembered key that is still reported wins.
        assertEquals("session", AgentUsagePresentation.selectWindow(usage, "session")?.key)
        // One that is no longer reported falls back to the busiest.
        assertEquals("model:fable", AgentUsagePresentation.selectWindow(usage, "missing")?.key)

        assertNull(AgentUsagePresentation.selectWindow(null))
        assertNull(
            AgentUsagePresentation.selectWindow(
                AgentUsagePresentation.parseUsage("""{"fetchedAt": "2026-08-28T09:58:00Z"}"""),
            ),
        )
    }

    @Test
    fun `severity thresholds at 75 and 95`() {
        assertEquals(AgentUsageSeverity.Normal, AgentUsagePresentation.severity(0.0))
        assertEquals(AgentUsageSeverity.Normal, AgentUsagePresentation.severity(42.0))
        assertEquals(AgentUsageSeverity.Normal, AgentUsagePresentation.severity(74.9))
        assertEquals(AgentUsageSeverity.Warning, AgentUsagePresentation.severity(75.0))
        assertEquals(AgentUsageSeverity.Warning, AgentUsagePresentation.severity(94.9))
        assertEquals(AgentUsageSeverity.Danger, AgentUsagePresentation.severity(95.0))
        assertEquals(AgentUsageSeverity.Danger, AgentUsagePresentation.severity(100.0))
        assertEquals(AgentUsageSeverity.Danger, AgentUsagePresentation.severity(150.0))
    }

    @Test
    fun `fresh inside 15 minutes`() {
        fun iso(deltaMs: Long) = Instant.ofEpochMilli(nowMs + deltaMs).toString()
        assertTrue(AgentUsagePresentation.isFresh(iso(-14 * 60_000L), nowMs))
        assertFalse(AgentUsagePresentation.isFresh(iso(-15 * 60_000L), nowMs))
        assertFalse(AgentUsagePresentation.isFresh(iso(-60 * 60_000L), nowMs))
        // A machine clock running ahead is still fresh.
        assertTrue(AgentUsagePresentation.isFresh(iso(30_000L), nowMs))
        // Fail closed.
        assertFalse(AgentUsagePresentation.isFresh(null, nowMs))
        assertFalse(AgentUsagePresentation.isFresh("whenever", nowMs))
        // The Postgres text form Electric delivers parses too.
        assertTrue(AgentUsagePresentation.isFresh("2026-08-28 09:58:00+00", nowMs))
    }

    @Test
    fun `reset countdown strings`() {
        assertEquals(
            "resets in 2h 10m",
            AgentUsagePresentation.resetCountdown("2026-08-28T12:10:30Z", nowMs),
        )
        assertEquals(
            "resets in 3d 14h",
            AgentUsagePresentation.resetCountdown("2026-09-01T00:00:00Z", nowMs),
        )
        assertEquals(
            "resets in 45m",
            AgentUsagePresentation.resetCountdown("2026-08-28T10:45:00Z", nowMs),
        )
        // A zero smaller unit is dropped, never rendered as `2h 0m` / `3d 0h`.
        assertEquals(
            "resets in 2h",
            AgentUsagePresentation.resetCountdown("2026-08-28T12:00:00Z", nowMs),
        )
        assertEquals(
            "resets in 3d",
            AgentUsagePresentation.resetCountdown("2026-08-31T10:00:00Z", nowMs),
        )
        assertEquals(
            "resets soon",
            AgentUsagePresentation.resetCountdown("2026-08-28T10:00:30Z", nowMs),
        )
        assertEquals(
            "resets soon",
            AgentUsagePresentation.resetCountdown("2026-08-28T09:00:00Z", nowMs),
        )
        assertNull(AgentUsagePresentation.resetCountdown(null, nowMs))
        assertNull(AgentUsagePresentation.resetCountdown("whenever", nowMs))
    }

    @Test
    fun `account captions`() {
        val accounts = AgentUsagePresentation.parseAccounts(accountsJson)!!
        assertEquals(setOf("claude", "codex", "pi"), accounts.keys)

        assertEquals(
            "signed in as danny@yourev.at · max",
            AgentUsagePresentation.accountCaption(accounts["claude"]),
        )
        assertEquals("signed out", AgentUsagePresentation.accountCaption(accounts["codex"]))
        // pi has no email — its provider line IS the caption.
        assertEquals("anthropic (oauth)", AgentUsagePresentation.accountCaption(accounts["pi"]))
        assertEquals(
            "signed in as danny@yourev.at",
            AgentUsagePresentation.accountCaption(
                AgentAccount(signedIn = true, email = "danny@yourev.at"),
            ),
        )
        assertEquals("signed in", AgentUsagePresentation.accountCaption(AgentAccount(signedIn = true)))
        assertEquals("unknown", AgentUsagePresentation.accountCaption(null))

        assertEquals(
            "claude · signed in as danny@yourev.at · max",
            AgentUsagePresentation.accountRow("claude", accounts["claude"]),
        )
        assertEquals("codex · signed out", AgentUsagePresentation.accountRow("codex", accounts["codex"]))
        assertEquals("pi · anthropic (oauth)", AgentUsagePresentation.accountRow("pi", accounts["pi"]))
        assertEquals("claude · unknown", AgentUsagePresentation.accountRow("claude", null))

        assertNull(AgentUsagePresentation.parseAccounts(null))
        assertNull(AgentUsagePresentation.parseAccounts("nope"))
    }

    @Test
    fun `session usage hidden cases`() {
        fun session(
            status: String = "running",
            agent: String? = "claude",
            deviceId: String? = "dev-1",
            userId: String = "me",
        ) = CodingSessionEntity(
            id = "sess-1",
            issueId = "issue-1",
            teamId = "team-1",
            userId = userId,
            deviceId = deviceId,
            status = status,
            agent = agent,
            startedAt = "2026-08-28T09:00:00Z",
            createdAt = "2026-08-28T09:00:00Z",
            updatedAt = "2026-08-28T09:00:00Z",
        )

        fun device(
            id: String = "row-1",
            userId: String = "me",
            deviceId: String = "dev-1",
            usage: String? = usageMapJson(),
        ) = DeviceEntity(
            id = id,
            userId = userId,
            deviceId = deviceId,
            label = "macbook",
            agentUsage = usage,
        )

        val devices = listOf(device())

        // The happy path: a live run with a recorded agent on a machine
        // reporting fresh usage for it.
        val usage = AgentUsagePresentation.sessionUsage(session(), devices, nowMs)
        assertNotNull(usage)
        assertEquals(3, usage!!.windows.size)
        assertNotNull(AgentUsagePresentation.sessionUsage(session(status = "in_review"), devices, nowMs))

        // Hidden: the run is over, it never recorded an agent, it has no
        // device stamp, no row matches, the machine reports another agent, the
        // numbers went stale, or the snapshot has no windows.
        assertNull(AgentUsagePresentation.sessionUsage(session(status = "ended"), devices, nowMs))
        assertNull(AgentUsagePresentation.sessionUsage(session(agent = null), devices, nowMs))
        assertNull(AgentUsagePresentation.sessionUsage(session(deviceId = null), devices, nowMs))
        assertNull(AgentUsagePresentation.sessionUsage(session(), emptyList(), nowMs))
        assertNull(AgentUsagePresentation.sessionUsage(session(agent = "codex"), devices, nowMs))
        assertNull(
            AgentUsagePresentation.sessionUsage(
                session(),
                devices,
                nowMs + 20 * 60_000L,
            ),
        )
        assertNull(
            AgentUsagePresentation.sessionUsage(
                session(),
                listOf(
                    device(
                        usage = """{"claude": {"fetchedAt": "2026-08-28T09:58:00Z", "windows": []}}""",
                    ),
                ),
                nowMs,
            ),
        )
        assertNull(AgentUsagePresentation.sessionUsage(session(), listOf(device(usage = null)), nowMs))

        // A shared machine appears once per user — the OWNER's row wins, the
        // resolveSessionDevice rule.
        val shared = listOf(
            device(id = "row-mate", userId = "mate", usage = null),
            device(id = "row-mine", userId = "me"),
        )
        assertNotNull(AgentUsagePresentation.sessionUsage(session(), shared, nowMs))
    }

    @Test
    fun `parses an agent login publication`() {
        // Codex publishes a device code alongside the URL; claude only the URL.
        val both = parseAgentLoginResult(
            """{"agent":"codex","phase":"url","url":"https://auth.openai.com/device","code":"ABCD-EFGHI"}""",
        )
        assertEquals("https://auth.openai.com/device", both?.url)
        assertEquals("ABCD-EFGHI", both?.code)

        val urlOnly = parseAgentLoginResult(
            """{"agent":"claude","phase":"url","url":"https://claude.ai/oauth/authorize?x=1"}""",
        )
        assertEquals("https://claude.ai/oauth/authorize?x=1", urlOnly?.url)
        assertNull(urlOnly?.code)

        // Anything else falls through to the ordinary command caption.
        assertNull(parseAgentLoginResult("Removed 3 worktrees."))
        assertNull(parseAgentLoginResult("""{"agent":"claude","phase":"url"""))
        assertNull(parseAgentLoginResult(null))
    }
}
