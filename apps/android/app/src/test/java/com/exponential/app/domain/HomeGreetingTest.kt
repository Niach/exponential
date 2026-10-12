package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-1253 (EXP-1259 D1): the Agent home, replayed off `composer-home.json`
// (web `home-greeting.test.ts`, same case names).
class HomeGreetingTest {
    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("composer-home.json")).jsonObject

    private fun JsonObject.string(key: String): String = getValue(key).jsonPrimitive.content
    private fun JsonObject.int(key: String): Int = getValue(key).jsonPrimitive.int

    @Test
    fun `homeGreeting replays every case`() {
        val cases = fixture.getValue("cases").jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val user = case.getValue("user").let { if (it is JsonNull) null else it.jsonPrimitive.content }
            assertEquals(
                case.string("name"),
                case.string("expected"),
                AgentHome.homeGreeting(case.string("local"), user),
            )
        }
    }

    @Test
    fun `the buckets, copy and geometry are the fixture's`() {
        val buckets = fixture.getValue("buckets").jsonArray.map { it.jsonObject }
        assertEquals(
            buckets.map {
                HomeGreetingBucket(
                    it.string("id"),
                    it.int("from"),
                    it.int("to"),
                    it.getValue("variants").jsonArray.map { v -> v.jsonPrimitive.content },
                )
            },
            AgentHome.BUCKETS,
        )
        assertEquals(fixture.string("placeholder"), AgentHome.PLACEHOLDER)
        assertEquals(fixture.int("dayStartsAtHour"), AgentHome.DAY_STARTS_AT_HOUR)
        val recent = fixture.getValue("recent").jsonObject
        assertEquals(recent.string("label"), AgentHome.RECENT_LABEL)
        assertEquals(recent.int("count"), AgentHome.RECENT_COUNT)
        val g = fixture.getValue("geometry").jsonObject
        assertEquals(g.int("mark"), AgentHome.MARK)
        assertEquals(g.int("markPhone"), AgentHome.MARK_PHONE)
        assertEquals(g.int("gap"), AgentHome.GAP)
        assertEquals(g.int("headline"), AgentHome.HEADLINE)
        assertEquals(g.int("headlinePhone"), AgentHome.HEADLINE_PHONE)
        assertEquals(g.int("headlineWeight"), AgentHome.HEADLINE_WEIGHT)
        assertEquals(g.int("headlineToCard"), AgentHome.HEADLINE_TO_CARD)
        assertEquals(g.int("optionsToRecent"), AgentHome.OPTIONS_TO_RECENT)
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.string("home"), AgentHome.HOME_TEST_ID)
        assertEquals(ids.string("greeting"), AgentHome.GREETING_TEST_ID)
        assertEquals(ids.string("mark"), AgentHome.MARK_TEST_ID)
        assertEquals(ids.string("recent"), AgentHome.RECENT_TEST_ID)
        assertEquals(ids.string("recentRow"), AgentHome.RECENT_ROW_TEST_ID)
    }

    private fun run(
        id: String,
        updatedAt: String,
        teamId: String = "team-1",
        userId: String = "me",
        parentSessionId: String? = null,
        resumedFromId: String? = null,
        status: String = "ended",
    ) = CodingSessionEntity(
        id = id,
        teamId = teamId,
        userId = userId,
        parentSessionId = parentSessionId,
        resumedFromId = resumedFromId,
        status = status,
        startedAt = updatedAt,
        createdAt = updatedAt,
        updatedAt = updatedAt,
    )

    @Test
    fun `recent lists my last three top-level runs of the team, newest first`() {
        val sessions = listOf(
            run("a", "2026-10-01T10:00:00Z"),
            run("b", "2026-10-03T10:00:00Z", status = "running"),
            run("c", "2026-10-02T10:00:00Z"),
            run("d", "2026-10-04T10:00:00Z"),
            run("child", "2026-10-05T10:00:00Z", parentSessionId = "d"),
            run("other-team", "2026-10-06T10:00:00Z", teamId = "team-2"),
            run("teammate", "2026-10-06T10:00:00Z", userId = "them"),
        )
        assertEquals(
            listOf("d", "b", "c"),
            AgentHome.recentRuns(sessions, "team-1", "me").map { it.id },
        )
    }

    @Test
    fun `a resume succession is one recent run`() {
        val sessions = listOf(
            run("first", "2026-10-01T10:00:00Z"),
            run("resumed", "2026-10-02T10:00:00Z", resumedFromId = "first", status = "running"),
            run("other", "2026-10-01T12:00:00Z"),
        )
        assertEquals(
            listOf("resumed", "other"),
            AgentHome.recentRuns(sessions, "team-1", "me").map { it.id },
        )
    }

    /** The final review: every client replays composer-home.json `recentCases` (ids in order). */
    @Test
    fun `recent replays the fixture cases`() {
        val cases = fixture.getValue("recentCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val sessions = case.getValue("sessions").jsonArray.map { it.jsonObject }.map { row ->
                fun opt(key: String) = row[key]?.takeIf { it !is JsonNull }?.jsonPrimitive?.content
                CodingSessionEntity(
                    id = row.string("id"),
                    teamId = row.string("teamId"),
                    userId = row.string("userId"),
                    parentSessionId = opt("parentSessionId"),
                    resumedFromId = opt("resumedFromId"),
                    status = "ended",
                    startedAt = row.string("createdAt"),
                    createdAt = row.string("createdAt"),
                    updatedAt = row.string("updatedAt"),
                )
            }
            assertEquals(
                case.string("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                AgentHome.recentRuns(sessions, case.string("teamId"), case.string("userId")).map { it.id },
            )
        }
    }

    @Test
    fun `no team or no user lists nothing`() {
        val sessions = listOf(run("a", "2026-10-01T10:00:00Z"))
        assertEquals(emptyList<CodingSessionEntity>(), AgentHome.recentRuns(sessions, null, "me"))
        assertEquals(emptyList<CodingSessionEntity>(), AgentHome.recentRuns(sessions, "team-1", null))
    }
}
