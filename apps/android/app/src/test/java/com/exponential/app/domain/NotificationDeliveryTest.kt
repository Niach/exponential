package com.exponential.app.domain

import com.exponential.app.domain.NotificationDelivery as ND
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1815: [NotificationDelivery] replays EVERY `*Cases` array of
 * `fixtures/notification-delivery.json` (TS `notification-delivery.test.ts`,
 * desktop `domain::notification_delivery`, iOS `NotificationDelivery.swift`).
 */
class NotificationDeliveryTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("notification-delivery.json")).jsonObject
    }

    private fun cases(key: String): List<JsonObject> =
        fixture.getValue(key).jsonArray.map { it.jsonObject }.also { assertTrue("$key is empty", it.isNotEmpty()) }

    private fun JsonElement?.str(): String? = this?.takeUnless { it is JsonNull }?.jsonPrimitive?.content
    private fun JsonObject.str(key: String): String? = this[key].str()
    private fun JsonObject.name(): String = str("name")!!
    private fun JsonObject.input(): JsonObject = getValue("input").jsonObject

    private fun stringMap(obj: JsonElement?): Map<String, String>? =
        obj?.takeUnless { it is JsonNull }?.jsonObject?.mapValues { it.value.jsonPrimitive.content }

    private fun prefsOf(obj: JsonElement?): ND.SourcePrefs? {
        val o = obj?.takeUnless { it is JsonNull }?.jsonObject ?: return null
        return ND.SourcePrefs(
            assistant = o["assistant"]?.jsonPrimitive?.boolean,
            temporary = o["temporary"]?.jsonPrimitive?.boolean,
            agents = o["agents"]?.jsonObject?.mapValues { it.value.jsonPrimitive.boolean },
        )
    }

    @Test
    fun `every constant matches the fixture`() {
        val keys = stringMap(fixture["dataKeys"])!!
        assertEquals(
            keys,
            mapOf(
                "thread" to ND.Keys.THREAD, "source" to ND.Keys.SOURCE, "sourceName" to ND.Keys.SOURCE_NAME,
                "dismissType" to ND.Keys.DISMISS_TYPE, "ids" to ND.Keys.IDS, "threads" to ND.Keys.THREADS,
                "all" to ND.Keys.ALL, "teamId" to ND.Keys.TEAM_ID,
            ),
        )
        val limits = fixture.getValue("limits").jsonObject
        assertEquals(limits.getValue("maxIds").jsonPrimitive.int, ND.MAX_IDS)
        assertEquals(limits.getValue("maxThreads").jsonPrimitive.int, ND.MAX_THREADS)
        assertEquals(
            stringMap(fixture["copy"]),
            mapOf(
                "toastAction" to ND.Copy.TOAST_ACTION, "sessionsBand" to ND.Copy.SESSIONS_BAND,
                "assistant" to ND.Copy.ASSISTANT, "temporary" to ND.Copy.TEMPORARY, "agentsBand" to ND.Copy.AGENTS_BAND,
            ),
        )
        val channels = fixture.getValue("channels").jsonObject
        assertEquals(stringMap(channels["assistant"]), mapOf("id" to ND.Channels.ASSISTANT_ID, "name" to ND.Channels.ASSISTANT_NAME))
        assertEquals(stringMap(channels["temporary"]), mapOf("id" to ND.Channels.TEMPORARY_ID, "name" to ND.Channels.TEMPORARY_NAME))
        assertEquals(channels.str("agentPrefix"), ND.Channels.AGENT_PREFIX)
        assertEquals(stringMap(channels["agentsGroup"]), mapOf("id" to ND.Channels.AGENTS_GROUP_ID, "name" to ND.Channels.AGENTS_GROUP_NAME))
        assertEquals(channels.str("fallback"), ND.Channels.FALLBACK)
    }

    @Test
    fun threadCases() {
        for (case in cases("threadCases")) {
            val i = case.input()
            assertEquals(
                case.name(),
                case.str("expected"),
                ND.notificationThread(i.str("type")!!, i.str("issueId"), i.str("sessionRootId"), i.str("deviceId")),
            )
        }
    }

    @Test
    fun presentCases() {
        for (case in cases("presentCases")) {
            val i = case.input()
            val got = ND.notificationPresentation(
                platform = i.str("platform")!!,
                active = i.getValue("active").jsonPrimitive.boolean,
                viewing = i.str("viewing"),
                thread = i.str("thread")!!,
                muted = i.getValue("muted").jsonPrimitive.boolean,
            )
            assertEquals(case.name(), case.str("expected"), got.wire)
        }
    }

    /** The fixture's `rowsOfRule`. */
    private fun rowsOf(spec: JsonObject): List<ND.ReadRow> {
        val count = spec.getValue("count").jsonPrimitive.int
        val threads = spec["threads"]?.jsonArray?.map { it.jsonPrimitive.content }
        val threadCount = spec["threadCount"]?.jsonPrimitive?.int ?: 1
        return List(count) { i -> ND.ReadRow("n$i", threads?.get(i % threads.size) ?: "t${i % threadCount}") }
    }

    @Test
    fun dismissCases() {
        for (case in cases("dismissCases")) {
            val i = case.input()
            val rows = i["rows"]?.jsonArray?.map {
                val o = it.jsonObject
                ND.ReadRow(o.str("id")!!, o.str("thread")!!)
            } ?: rowsOf(i.getValue("rowsOf").jsonObject)
            assertEquals(
                case.name(),
                stringMap(case["expected"]),
                ND.dismissPayload(rows, i.getValue("all").jsonPrimitive.boolean, i.str("teamId")),
            )
        }
    }

    @Test
    fun matchCases() {
        for (case in cases("matchCases")) {
            val i = case.input()
            val d = i.getValue("delivered").jsonObject
            assertEquals(
                case.name(),
                case.getValue("expected").jsonPrimitive.boolean,
                ND.dismissMatches(
                    stringMap(i["payload"])!!,
                    ND.Delivered(d.str("notificationId"), d.str("thread"), d.str("teamId")),
                ),
            )
        }
    }

    @Test
    fun sourceCases() {
        for (case in cases("sourceCases")) {
            val i = case.input()
            assertEquals(
                case.name(),
                case.str("expected"),
                ND.notificationSource(i.str("agentId"), i.str("agentRole"), i.str("startedByAgentId"), i.str("startedByAgentRole")),
            )
        }
    }

    @Test
    fun allowCases() {
        for (case in cases("allowCases")) {
            val i = case.input()
            assertEquals(
                case.name(),
                case.getValue("expected").jsonPrimitive.boolean,
                ND.sourceAllowed(prefsOf(i["prefs"]), i.str("source")),
            )
        }
    }

    @Test
    fun toggleCases() {
        for (case in cases("toggleCases")) {
            val i = case.input()
            assertEquals(
                case.name(),
                prefsOf(case["expected"]),
                ND.toggleSourcePref(prefsOf(i["prefs"])!!, i.str("key")!!, i.getValue("on").jsonPrimitive.boolean),
            )
        }
    }

    @Test
    fun settingsCases() {
        for (case in cases("settingsCases")) {
            val i = case.input()
            val teams = i.getValue("teams").jsonArray.map { it.jsonObject }.map { ND.SettingsTeam(it.str("id")!!, it.str("name")!!) }
            val agents = i.getValue("agents").jsonArray.map { it.jsonObject }.map {
                ND.SettingsAgent(it.str("id")!!, it.str("teamId")!!, it.str("name")!!, it.str("role")!!)
            }
            val expected = case.getValue("expected").jsonArray.map { it.jsonObject }.map { g ->
                ND.SettingsGroup(
                    g.str("band")!!,
                    g.getValue("rows").jsonArray.map { it.jsonObject }.map { ND.SettingsRow(it.str("key")!!, it.str("label")!!, it.str("detail")) },
                )
            }
            assertEquals(case.name(), expected, ND.settingsRows(teams, agents))
        }
    }

    @Test
    fun channelCases() {
        for (case in cases("channelCases")) {
            val i = case.input()
            val e = case.getValue("expected").jsonObject
            assertEquals(
                case.name(),
                ND.AndroidChannel(e.str("id")!!, e.str("name"), e.str("group")),
                ND.androidChannel(i.str("source"), i.str("sourceName")),
            )
        }
    }

    @Test
    fun `every Cases array of the fixture is replayed here`() {
        val replayed = setOf(
            "threadCases", "presentCases", "dismissCases", "matchCases", "sourceCases",
            "allowCases", "toggleCases", "settingsCases", "channelCases",
        )
        assertEquals(fixture.keys.filter { it.endsWith("Cases") }.toSet(), replayed)
    }
}
