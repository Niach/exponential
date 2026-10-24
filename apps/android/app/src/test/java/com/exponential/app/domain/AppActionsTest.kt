package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// VAPP-120: replays `packages/domain-contract/fixtures/app-actions.json`
// (web, desktop and iOS replay the same cases) and locks the list, the tool
// map and the copy to the fixture.
class AppActionsTest {
    private fun read(relative: String): JsonValue {
        val candidates = listOf("../../../$relative", "../../$relative", relative)
        val file = candidates.map(::File).firstOrNull { it.isFile } ?: error("$relative not found from ${File(".").absolutePath}")
        return JsonValue.parse(file.readText())
    }

    private val fixture = read("packages/domain-contract/fixtures/app-actions.json")

    private fun cases(key: String): List<JsonValue> = fixture[key]!!.array!!

    private fun strings(v: JsonValue?): List<String> = v!!.array!!.map { it.string!! }

    private fun nullable(v: JsonValue?): JsonValue? = v?.takeIf { it != JsonValue.Null }

    @Test
    fun `the list, the tools and the copy match the fixture`() {
        assertEquals(fixture["prefix"]!!.string, AppActions.PREFIX)
        assertEquals(strings(fixture["functions"]), AppActions.ALL)
        val tools = fixture["tools"]!!.obj!!
        assertEquals(tools.keys, AppActions.TOOLS.keys)
        for ((name, target) in tools) {
            assertEquals(name, AppActions.Target(target["tool"]!!.string!!, nullable(target["op"])?.string), AppActions.TOOLS[name])
        }
        val copy = fixture["copy"]!!
        assertEquals(copy["title"]!!.string, AppActions.Copy.TITLE)
        assertEquals(copy["hint"]!!.string, AppActions.Copy.HINT)
        assertEquals(copy["none"]!!.string, AppActions.Copy.NONE)
        assertEquals(copy["granted"]!!.string, AppActions.Copy.GRANTED)
        for (excluded in strings(fixture["excluded"])) {
            assert(AppActions.ALL.none { it.startsWith("${AppActions.PREFIX}$excluded.") }) { excluded }
        }
    }

    @Test
    fun parseCases() {
        for (case in cases("parseCases")) {
            val name = case["name"]!!.string!!
            val got = AppActions.parseActionName(name)
            val expected = nullable(case["expected"])
            if (expected == null) {
                assertNull(name, got)
            } else {
                assertEquals(name, AppActions.Target(expected["tool"]!!.string!!, nullable(expected["op"])?.string), got)
            }
        }
    }

    @Test
    fun grantCases() {
        for (case in cases("grantCases")) {
            val name = case["name"]!!.string!!
            assertEquals(name, case["expected"] == JsonValue.Bool(true), AppActions.grantAllows(strings(case["granted"]), name))
        }
    }

    @Test
    fun declaredCases() {
        for (case in cases("declaredCases")) {
            val functions = strings(case["functions"])
            assertEquals(functions.toString(), strings(case["expected"]), AppActions.declaredActions(functions))
        }
    }

    @Test
    fun consentCases() {
        for (case in cases("consentCases")) {
            val name = case["name"]!!.string!!
            val got = AppActions.actionConsent(name, case["args"]!!.obj!!)
            val expected = nullable(case["expected"])
            if (expected == null) {
                assertNull(name, got)
                continue
            }
            val gotJson = got?.let {
                JsonValue.of(mapOf("tool" to it.tool, "title" to it.title, "destructive" to it.destructive, "args" to it.args))
            }
            assertEquals(name, expected.json, gotJson?.json)
        }
    }

    @Test
    fun `the title is the display row's, the name otherwise`() {
        assertEquals("Set issue status", AppActions.actionTitle("exponential.issues.update_status"))
        assertEquals("harness.toast", AppActions.actionTitle("harness.toast"))
    }
}
