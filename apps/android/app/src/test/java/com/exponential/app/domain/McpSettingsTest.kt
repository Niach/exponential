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
        assertEquals(fixture.str("overview"), McpSettings.OVERVIEW)
        assertEquals(fixture.str("builtinPill"), McpSettings.BUILTIN_PILL)
        assertEquals(fixture.str("toolsConnect"), McpSettings.TOOLS_CONNECT)
        assertEquals(fixture.str("toolsStdio"), McpSettings.TOOLS_STDIO)
        assertEquals(fixture.str("toolsFailed"), McpSettings.TOOLS_FAILED)
        assertEquals(fixture.str("toolsNone"), McpSettings.TOOLS_NONE)
        assertEquals(fixture.str("retry"), McpSettings.RETRY)
        val builtins = fixture.getValue("builtins").jsonObject
        assertEquals(builtins.keys.toList(), McpSettings.BUILTINS.map { it.id })
        for (b in McpSettings.BUILTINS) {
            val want = builtins.getValue(b.id).jsonObject
            assertEquals(want.str("name"), b.name)
            assertEquals(want.str("path"), b.path)
        }
        assertEquals(fixture.str("revoke"), McpSettings.REVOKE)
        assertEquals(fixture.str("activity"), McpSettings.ACTIVITY)
        assertEquals(fixture.str("everyoneElse"), McpSettings.EVERYONE_ELSE)
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
        assertEquals(
            ids,
            linkedMapOf(
                "page" to McpSettings.TestIds.PAGE,
                "overview" to McpSettings.TestIds.OVERVIEW,
                "servers" to McpSettings.TestIds.SERVERS,
                "builtin" to McpSettings.TestIds.BUILTIN,
                "appsServer" to McpSettings.TestIds.APPS_SERVER,
                "serverTools" to McpSettings.TestIds.SERVER_TOOLS,
                "access" to McpSettings.TestIds.ACCESS,
                "activity" to McpSettings.TestIds.ACTIVITY,
                "clients" to McpSettings.TestIds.CLIENTS,
            ),
        )
    }

    @Test
    fun runsOnCases() {
        assertEquals(fixture.str("runsOn"), McpSettings.RUNS_ON)
        for (case in fixture.getValue("runsOnCases").jsonArray.map { it.jsonObject }) {
            assertEquals(case.str("name"), case.str("expected"), McpSettings.runsOnCaption(case.str("device")))
        }
    }

    @Test
    fun serverOrderCases() {
        for (case in fixture.getValue("serverOrder").jsonObject.getValue("cases").jsonArray.map { it.jsonObject }) {
            assertEquals(
                case.str("name"),
                case.strings("expected"),
                McpSettings.serverOrder(case.getValue("appTools").jsonPrimitive.boolean, case.strings("servers")),
            )
        }
    }

    @Test
    fun toolsStateCases() {
        for (case in fixture.getValue("toolsStateCases").jsonArray.map { it.jsonObject }) {
            assertEquals(
                case.str("name"),
                case.str("expected"),
                McpSettings.toolsState(case.str("transport")!!, case.str("status")!!).wire,
            )
        }
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
    fun builtinCases() {
        assertEquals(fixture.str("builtinFilter"), McpSettings.BUILTIN_FILTER_PLACEHOLDER)
        for (case in fixture.getValue("builtinCases").jsonArray.map { it.jsonObject }) {
            val name = case.str("name")
            val bands = McpSettings.builtinToolBands(case.str("query")!!)
            val expected = case.getValue("expected").jsonArray.map { it.jsonObject }
            assertEquals(name, expected.map { it.str("id") }, bands.map { it.id })
            expected.zip(bands).forEach { (want, band) ->
                assertEquals(name, want.str("label"), band.label)
                assertEquals(name, want.getValue("count").jsonPrimitive.content.toInt(), band.tools.size)
                if (want.containsKey("rows")) assertEquals(name, want.strings("rows"), band.tools.map { it.name })
            }
        }
    }

    @Test
    fun `the flat built-in list never lists a retired alias row`() {
        val tools = McpSettings.builtinTools()
        assertEquals(McpSettings.builtinToolBands("").sumOf { it.tools.size }, tools.size)
        assertEquals(emptyList<String>(), tools.map { it.name }.filter { "team_config" in it })
    }

    @Test
    fun `tool count, level labels and the built-in urls`() {
        assertEquals("1 tool", McpSettings.toolCountLabel(1))
        assertEquals("3 tools", McpSettings.toolCountLabel(3))
        assertEquals("Read only", McpSettings.levelLabel("read"))
        assertEquals("Full", McpSettings.levelLabel("write"))
        assertEquals("https://x.at/api/mcp", McpSettings.builtinEndpoint("https://x.at/", McpSettings.BUILTIN_EXPONENTIAL))
        assertEquals("https://x.at/api/mcp/apps", McpSettings.builtinEndpoint("https://x.at", McpSettings.BUILTIN_APPS))
        assertEquals("Could not list its tools: HTTP 500", McpSettings.toolsFailedLine("HTTP 500"))
    }
}
