package com.exponential.app.domain

import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.ui.threads.ThreadRowPrLead
import com.exponential.app.ui.threads.threadRowPrLead
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1309/1318: ONE quiet word per Threads row + the rail's Active band —
 * replays `threads-list.json` `wordCases`, `leadCases`, `stackCases` and `railCases`
 * (TS `session-tree.test.ts`).
 */
class ThreadRowWordTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("threads-list.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.bool(key: String): Boolean = this[key]?.jsonPrimitive?.boolean ?: false

    private fun cases(key: String): List<JsonObject> = fixture.getValue(key).jsonArray.map { it.jsonObject }

    private fun stackPr(json: JsonObject) =
        SessionTree.StackPr(json.str("id")!!, json.str("state")!!, json.str("basePullRequestId"))

    @Test
    fun `every word case matches the fixture`() {
        val cases = cases("wordCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val actual = SessionTree.threadRowWord(
                states = input.getValue("pullRequests").jsonArray.map { it.jsonObject.str("state")!! },
                busy = input.bool("busy"),
                needsYou = input.bool("needsYou"),
                inStack = input.bool("inStack"),
            )
            assertEquals(case.str("name"), case.str("expected"), actual?.wire)
        }
    }

    /** Release train 2026-10-23 (NEW-41): `leadCases`, the row lead's PR glyph. */
    @Test
    fun `every lead case matches the fixture`() {
        val cases = cases("leadCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val states = case.getValue("prStates").jsonArray.map { it.jsonPrimitive.content }
            val actual = when (threadRowPrLead(states)) {
                ThreadRowPrLead.Open -> "open"
                ThreadRowPrLead.Merged -> "merged"
                null -> null
            }
            assertEquals(case.str("name"), case.str("expected"), actual)
        }
    }

    @Test
    fun `every stack case matches the fixture`() {
        val cases = cases("stackCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val prs = input.getValue("pullRequests").jsonArray.map { stackPr(it.jsonObject) }
            val team = (input["teamPrs"] as? JsonArray)?.map { stackPr(it.jsonObject) } ?: prs
            assertEquals(case.str("name"), case.getValue("expected").jsonPrimitive.boolean, SessionTree.threadInStack(prs, team))
        }
    }

    @Test
    fun `every rail case matches the fixture`() {
        val cases = cases("railCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val groups = input.getValue("groups").jsonArray.map { element ->
                val group = element.jsonObject
                SessionTree.ThreadsListGroup(
                    key = SessionTree.ThreadsGroupKey.entries.first { it.wire == group.str("key") },
                    title = group.str("title")!!,
                    rows = group.getValue("rows").jsonArray.map {
                        val row = it.jsonObject
                        SessionTree.ThreadsListRow(
                            sessionId = row.str("sessionId")!!,
                            teamId = row.str("teamId")!!,
                            title = row.str("title")!!,
                            busy = row.bool("busy"),
                            needsYou = row.bool("needsYou"),
                            activityAt = row.str("activityAt")!!,
                            deviceId = row.str("deviceId"),
                        )
                    },
                )
            }
            val words = input.getValue("words").jsonObject
            val actual = SessionTree.railThreads(groups) { row ->
                words.str(row.sessionId)?.let { key -> SessionTree.ThreadRowWord.entries.first { it.wire == key } }
            }
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                actual.map { it.sessionId },
            )
        }
    }

    @Test
    fun `the entity overload reads the same stack`() {
        val lower = PullRequestEntity(id = "a", state = DomainContract.prStateOpen)
        val upper = PullRequestEntity(id = "b", state = DomainContract.prStateOpen, basePullRequestId = "a")
        assertEquals(
            SessionTree.ThreadRowWord.Stack,
            SessionTree.threadRowWord(false, false, listOf(upper), listOf(lower, upper)),
        )
    }
}
