package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/** #VAPP-185: [McpSettings] == `settings-mcp.json`, every case replayed. */
class McpSettingsTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("settings-mcp.json")).jsonObject

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.strings(key: String): List<String> =
        getValue(key).jsonArray.map { it.jsonPrimitive.content }

    private fun JsonObject.map(key: String): Map<String, String> =
        getValue(key).jsonObject.mapValues { it.value.jsonPrimitive.content }

    private fun options(key: String): List<McpSettings.Option> =
        fixture.getValue(key).jsonArray.map { it.jsonObject }.map { McpSettings.Option(it.str("value")!!, it.str("label")!!) }

    @Test
    fun `the copy matches the fixture`() {
        assertEquals(fixture.str("title"), McpSettings.TITLE)
        assertEquals(fixture.strings("sections"), McpSettings.SECTIONS)
        assertEquals(fixture.map("labels"), McpSettings.LABELS)
        assertEquals(fixture.str("copy"), McpSettings.COPY)
        assertEquals(fixture.str("copied"), McpSettings.COPIED)
        assertEquals(fixture.str("endpointPath"), McpSettings.ENDPOINT_PATH)
        assertEquals(fixture.str("revoke"), McpSettings.REVOKE)
        assertEquals(fixture.str("activity"), McpSettings.ACTIVITY)
        assertEquals(options("scopes"), McpSettings.SCOPES)
        assertEquals(options("levels"), McpSettings.LEVELS)
        assertEquals(fixture.map("kindLabels"), McpSettings.KIND_LABELS)
        assertEquals(fixture.map("accessLabels"), McpSettings.ACCESS_LABELS)
        assertEquals(fixture.map("statusLabels"), McpSettings.STATUS_LABELS)
        assertEquals(fixture.str("never"), McpSettings.NEVER)
        assertEquals(fixture.str("builtinApp"), McpSettings.BUILTIN_APP)
        assertEquals(fixture.str("firstParty"), McpSettings.FIRST_PARTY)
        assertEquals(fixture.str("usedBy"), McpSettings.USED_BY)
        val toolCount = fixture.getValue("toolCount").jsonObject
        assertEquals(toolCount.str("one"), McpSettings.TOOL_COUNT_ONE)
        assertEquals(toolCount.str("other"), McpSettings.TOOL_COUNT_OTHER)
    }

    @Test
    fun `the test ids match the fixture`() {
        val ids = fixture.map("testIds")
        assertEquals(ids["page"], McpSettings.TestIds.PAGE)
        assertEquals(ids["endpoint"], McpSettings.TestIds.ENDPOINT)
        assertEquals(ids["servers"], McpSettings.TestIds.SERVERS)
        assertEquals(ids["clients"], McpSettings.TestIds.CLIENTS)
        assertEquals(ids["access"], McpSettings.TestIds.ACCESS)
        assertEquals(ids["builtin"], McpSettings.TestIds.BUILTIN)
        assertEquals(ids["activity"], McpSettings.TestIds.ACTIVITY)
    }

    @Test
    fun usedByCases() {
        for (case in fixture.getValue("usedByCases").jsonArray.map { it.jsonObject }) {
            assertEquals(case.str("name"), case.str("expected"), McpSettings.usedByCaption(case.strings("apps")))
        }
    }

    @Test
    fun clientCaptionCases() {
        for (case in fixture.getValue("clientCaptionCases").jsonArray.map { it.jsonObject }) {
            val client = case.getValue("client").jsonObject
            assertEquals(
                case.str("name"),
                case.str("expected"),
                McpSettings.clientCaption(
                    scope = case.str("scope")!!,
                    userName = client.str("userName")!!,
                    kinds = client.strings("kinds"),
                    appsAccess = client.str("appsAccess"),
                    lastUsed = client.str("lastUsed"),
                ),
            )
        }
    }

    @Test
    fun activityCaptionCases() {
        for (case in fixture.getValue("activityCaptionCases").jsonArray.map { it.jsonObject }) {
            val call = case.getValue("call").jsonObject
            assertEquals(
                case.str("name"),
                case.str("expected"),
                McpSettings.activityCaption(
                    owner = case.getValue("owner").jsonPrimitive.boolean,
                    appName = call.str("appName"),
                    userName = call.str("userName")!!,
                    clientName = call.str("clientName"),
                    `when` = call.str("when")!!,
                ),
            )
        }
    }

    @Test
    fun `tool count, level labels and the endpoint url`() {
        assertEquals("1 tool", McpSettings.toolCountLabel(1))
        assertEquals("3 tools", McpSettings.toolCountLabel(3))
        assertEquals("Read only", McpSettings.levelLabel("read"))
        assertEquals("Full", McpSettings.levelLabel("write"))
        assertEquals("https://x.at/api/mcp/apps", McpSettings.endpointUrl("https://x.at/"))
    }
}
