package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.float
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1324: [Trace] replays `fixtures/trace.json` and compares the WHOLE
 * expected line, like the TS reference's own test, desktop `steer::trace` and
 * iOS `TraceTests`.
 */
class TraceTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("trace.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.lng(key: String): Long? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.long

    private fun JsonObject.bool(key: String): Boolean = getValue(key).jsonPrimitive.boolean

    private fun JsonObject.obj(key: String): JsonObject? =
        this[key]?.takeUnless { it is JsonNull }?.jsonObject

    private fun cases(kind: String): List<JsonObject> =
        fixture.getValue("cases").jsonObject.getValue(kind).jsonArray.map { it.jsonObject }

    private fun line(json: JsonObject?): TraceLineModel? = json?.let {
        TraceLineModel(
            icon = it.str("icon")!!,
            title = it.str("title")!!,
            description = it.str("description"),
            status = TraceStatus.fromWire(it.str("status")),
        )
    }

    @Test
    fun `tool lines`() {
        val cases = cases("tool")
        assertTrue(cases.size >= 10)
        for (case in cases) {
            val input = case.obj("input")!!
            val actual = Trace.tool(
                toolKind = input.str("toolKind"),
                name = input.str("name")!!,
                detail = input.str("detail"),
                settled = input.bool("settled"),
                failed = input.bool("failed"),
            )
            assertEquals(case.str("name"), line(case.obj("expected")), actual)
        }
    }

    @Test
    fun `reasoning lines`() {
        for (case in cases("reasoning")) {
            val input = case.obj("input")!!
            val actual = Trace.reasoning(input.bool("running"), input.lng("durationMs"))
            assertEquals(case.str("name"), line(case.obj("expected")), actual)
        }
    }

    @Test
    fun `subagent lines`() {
        for (case in cases("subagent")) {
            val input = case.obj("input")!!
            val actual = Trace.subagent(
                label = input.str("label")!!,
                running = input.bool("running"),
                failed = input.bool("failed"),
                durationMs = input.lng("durationMs"),
            )
            assertEquals(case.str("name"), line(case.obj("expected")), actual)
        }
    }

    @Test
    fun `test results and the failed line`() {
        val cases = cases("tests")
        assertTrue(cases.size >= 9)
        for (case in cases) {
            val name = case.str("name")
            val expected = case.obj("expected")!!
            val results = Trace.parseTestResults(case.str("output"))
            val want = expected.obj("results")?.let {
                TestResults(
                    passed = it.getValue("passed").jsonPrimitive.int,
                    failed = it.getValue("failed").jsonPrimitive.int,
                    total = it.getValue("total").jsonPrimitive.int,
                    suite = it.str("suite"),
                )
            }
            assertEquals(name, want, results)
            assertEquals(name, line(expected.obj("line")), Trace.failedTests(results))
        }
    }

    /** `clean` collapses UNICODE whitespace like the TS reference's `\s` (an
     *  NBSP or a narrow no-break space is a space, not a glyph). */
    @Test
    fun `clean collapses unicode whitespace`() {
        assertEquals("bun test", Trace.clean("bun test"))
        assertEquals("a b c", Trace.clean(" a  b \t\n c　"))
        assertEquals("", Trace.clean("  "))
        assertEquals("", Trace.clean(null))
    }

    @Test
    fun `the spec constants match the fixture`() {
        val geometry = fixture.getValue("geometry").jsonObject
        fun geo(json: JsonObject) = TraceGeometry(
            fontSize = json.getValue("fontSize").jsonPrimitive.int,
            lineHeight = json.getValue("lineHeight").jsonPrimitive.int,
            icon = json.getValue("icon").jsonPrimitive.int,
            gap = json.getValue("gap").jsonPrimitive.int,
            lineGap = json.getValue("lineGap").jsonPrimitive.int,
            chevronGap = json.getValue("chevronGap").jsonPrimitive.int,
            contentTop = json.getValue("contentTop").jsonPrimitive.int,
            contentBottom = json.getValue("contentBottom").jsonPrimitive.int,
            railInset = json.getValue("railInset").jsonPrimitive.int,
            railWidth = json.getValue("railWidth").jsonPrimitive.int,
            contentInset = json.getValue("contentInset").jsonPrimitive.int,
            contentTrailing = json.getValue("contentTrailing").jsonPrimitive.int,
        )
        assertEquals(geo(geometry.getValue("default").jsonObject), TraceSpec.DEFAULT)
        assertEquals(geo(geometry.getValue("sm").jsonObject), TraceSpec.SM)
        assertEquals(geometry.getValue("descriptionAlpha").jsonPrimitive.float, TraceSpec.DESCRIPTION_ALPHA)
        assertEquals(geometry.getValue("pendingOpacity").jsonPrimitive.float, TraceSpec.PENDING_OPACITY)
        val code = geometry.getValue("codeBlock").jsonObject
        fun code(key: String) = code.getValue(key).jsonPrimitive.float
        assertEquals(code("border"), TraceSpec.CodeBlock.BORDER.toFloat())
        assertEquals(code("fillAlpha"), TraceSpec.CodeBlock.FILL_ALPHA)
        assertEquals(code("padding"), TraceSpec.CodeBlock.PADDING.toFloat())
        assertEquals(code("contentTopWithHeader"), TraceSpec.CodeBlock.CONTENT_TOP_WITH_HEADER.toFloat())
        assertEquals(code("titleSize"), TraceSpec.CodeBlock.TITLE_SIZE.toFloat())
        assertEquals(code("fontSize"), TraceSpec.CodeBlock.FONT_SIZE.toFloat())
        assertEquals(code("lineHeight"), TraceSpec.CodeBlock.LINE_HEIGHT)
        assertEquals(code("copyButton"), TraceSpec.CodeBlock.COPY_BUTTON.toFloat())
        assertEquals(code("copyIcon"), TraceSpec.CodeBlock.COPY_ICON.toFloat())
        assertEquals(code("headerActionsOutset"), TraceSpec.CodeBlock.HEADER_ACTIONS_OUTSET.toFloat())
        assertEquals(code("maxLines"), TraceSpec.CodeBlock.MAX_LINES.toFloat())
        assertEquals(code("destructiveBorderAlpha"), TraceSpec.CodeBlock.DESTRUCTIVE_BORDER_ALPHA)

        val icons = fixture.getValue("icons").jsonObject
        fun icon(key: String) = icons.getValue(key).jsonPrimitive.content
        assertEquals(icon("tool"), TraceSpec.ICON_TOOL)
        assertEquals(icon("reasoning"), TraceSpec.ICON_REASONING)
        assertEquals(icon("subagent"), TraceSpec.ICON_SUBAGENT)
        assertEquals(icon("failed"), TraceSpec.ICON_FAILED)
        assertEquals(icon("chevron"), TraceSpec.ICON_CHEVRON)

        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("output"), TraceSpec.OUTPUT)
        assertEquals(copy.str("copy"), TraceSpec.COPY)
        assertEquals(copy.str("copied"), TraceSpec.COPIED)

        val tool = fixture.getValue("tool").jsonObject
        assertEquals(tool.str("unnamed"), TraceSpec.TOOL_UNNAMED)
        val verbs = tool.getValue("verbs").jsonObject
        assertEquals(verbs.keys, TraceSpec.VERBS.keys)
        for ((kind, json) in verbs) {
            val v = json.jsonObject
            assertEquals(
                kind,
                TraceSpec.Verbs(v.str("running")!!, v.str("settled")!!, v.str("blank")!!, v.str("icon")!!),
                TraceSpec.VERBS.getValue(kind),
            )
        }
    }
}
