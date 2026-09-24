package com.exponential.app.domain

import java.io.File as JavaFile
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1051: the context-window bar + legend, locked ×4 (web
 * `lib/context-layout.test.ts`, desktop `ui::context_layout`, iOS
 * `ContextLayoutPresentationTests`) against the ONE contract fixture —
 * same cases, same names. Porting a client means making this file's fixture
 * pass, nothing else.
 *
 * A case: `usage` (nullable), `segments` (nullable) and the whole expected
 * [ContextWindowView], or `null` when there is no window to draw.
 */
class ContextLayoutPresentationTest {

    private data class FixtureCase(
        val name: String,
        val usage: SessionUsageState?,
        val segments: List<ContextSegment>?,
        val expected: ContextWindowView?,
    )

    @Test
    fun `every fixture case folds exactly`() {
        val cases = cases()
        assertTrue("the fixture must not shrink", cases.size >= 12)
        for (case in cases) {
            val view = ContextLayoutPresentation.contextWindowView(case.usage, case.segments)
            val expected = case.expected
            if (expected == null) {
                assertNull(case.name, view)
                continue
            }
            assertNotNull(case.name, view)
            view!!
            assertEquals("${case.name}: headline", expected.headline, view.headline)
            assertEquals("${case.name}: percent", expected.percent, view.percent)
            assertEquals("${case.name}: severity", expected.severity, view.severity)
            assertEquals("${case.name}: ticks", expected.ticks, view.ticks)
            assertEquals("${case.name}: bar size", expected.bar.size, view.bar.size)
            expected.bar.forEachIndexed { index, slice ->
                val actual = view.bar[index]
                assertEquals("${case.name}: bar[$index] key", slice.key, actual.key)
                assertEquals("${case.name}: bar[$index] tone", slice.tone, actual.tone)
                // The geometry is a double; the strings beside it are the lock.
                assertEquals(
                    "${case.name}: bar[$index] percent",
                    slice.percent,
                    actual.percent,
                    0.005,
                )
            }
            assertEquals("${case.name}: legend size", expected.legend.size, view.legend.size)
            expected.legend.forEachIndexed { index, row ->
                assertEquals("${case.name}: legend[$index]", row, view.legend[index])
            }
        }
    }

    @Test
    fun `the fixture covers the rules a port has to get right`() {
        val cases = cases()
        assertEquals(cases.size, cases.map { it.name }.toSet().size)
        val views = cases.map { it.expected }
        // A null view (no usage / a zero window) and a real one both appear.
        assertTrue(views.any { it == null })
        assertTrue(views.any { it != null })
        // Every severity.
        for (level in AgentUsageSeverity.entries) {
            assertTrue("$level", views.any { it?.severity == level })
        }
        // Every contract segment key draws in at least one case, so a client
        // that forgets a label fails here rather than in someone's sheet.
        for (key in DomainContract.contextLayoutSegmentKeys) {
            assertTrue(key, views.any { view -> view?.bar?.any { it.key == key } == true })
        }
        // The derived rows ride EVERY non-null legend — they are computed,
        // never received, so they can never be missing.
        for (view in views.filterNotNull()) {
            assertEquals(
                listOf("conversation", "free"),
                view.legend.takeLast(2).map { it.key },
            )
            // The conversation is the bar's last slice, always; free is the track.
            assertEquals("conversation", view.bar.last().key)
            assertFalse(view.bar.any { it.key == "free" })
        }
    }

    @Test
    fun `never rescales an overshooting layout`() {
        // The device's estimates may add past the measured `contextUsed`; the
        // renderer clips at 100% rather than shrinking the layers to fit, so
        // the numbers a reader compares stay the numbers the device reported.
        val view = ContextLayoutPresentation.contextWindowView(
            SessionUsageState(contextUsed = 20_000, contextSize = 200_000),
            listOf(
                ContextSegment("base", 21_000, CONTEXT_SOURCE_MEASURED),
                ContextSegment("tools", 2_400, CONTEXT_SOURCE_ESTIMATED),
            ),
        )!!
        assertEquals(11.7, view.bar.sumOf { it.percent }, 0.00001)
        assertEquals(10.5, view.bar.first { it.key == "base" }.percent, 0.00001)
    }

    @Test
    fun `tokensCompact drops a trailing zero and keeps small counts raw`() {
        assertEquals("0", ContextLayoutPresentation.tokensCompact(0))
        assertEquals("600", ContextLayoutPresentation.tokensCompact(600))
        assertEquals("999", ContextLayoutPresentation.tokensCompact(999))
        assertEquals("1k", ContextLayoutPresentation.tokensCompact(1_000))
        assertEquals("1.5k", ContextLayoutPresentation.tokensCompact(1_500))
        assertEquals("21k", ContextLayoutPresentation.tokensCompact(21_000))
        assertEquals("37.4k", ContextLayoutPresentation.tokensCompact(37_400))
        assertEquals("134.7k", ContextLayoutPresentation.tokensCompact(134_700))
        // A negative count is a producer bug, never a negative label.
        assertEquals("0", ContextLayoutPresentation.tokensCompact(-500))
    }

    @Test
    fun `the contract pins the ticks and the title`() {
        // The first tick is the floor `exponential_sessions_compact` refuses
        // below; the other two are EXP-484's usage thresholds.
        assertEquals(50, DomainContract.contextLayoutCompactMinPercent)
        val view = ContextLayoutPresentation.contextWindowView(
            SessionUsageState(contextUsed = 1, contextSize = 100),
            null,
        )!!
        assertEquals(listOf(50, 75, 95), view.ticks)
        assertEquals(DomainContract.contextLayoutTitle, ContextLayoutPresentation.TITLE)
        assertEquals(
            listOf("conversation", "free"),
            DomainContract.contextLayoutDerivedKeys,
        )
    }

    // ── fixture ─────────────────────────────────────────────────────────────

    private fun cases(): List<FixtureCase> =
        Json.parseToJsonElement(fixtureJson()).jsonArray.map { element ->
            val case = element.jsonObject
            FixtureCase(
                name = case.getValue("name").jsonPrimitive.content,
                usage = case.nullable("usage")?.let { usage ->
                    SessionUsageState(
                        contextUsed = usage.getValue("contextUsed").jsonPrimitive.int,
                        contextSize = usage.getValue("contextSize").jsonPrimitive.int,
                    )
                },
                segments = case.getValue("segments").let { raw ->
                    if (raw is JsonNull) null else raw.jsonArray.map { entry ->
                        val segment = entry.jsonObject
                        ContextSegment(
                            key = segment.getValue("key").jsonPrimitive.content,
                            tokens = segment.getValue("tokens").jsonPrimitive.int,
                            source = segment.getValue("source").jsonPrimitive.content,
                            detail = segment["detail"]?.jsonPrimitive?.content,
                        )
                    }
                },
                expected = case.nullable("expected")?.let { expected ->
                    ContextWindowView(
                        headline = expected.getValue("headline").jsonPrimitive.content,
                        percent = expected.getValue("percent").jsonPrimitive.int,
                        severity = severityOf(expected.getValue("severity").jsonPrimitive.content),
                        ticks = expected.getValue("ticks").jsonArray.map { it.jsonPrimitive.int },
                        bar = expected.getValue("bar").jsonArray.map { entry ->
                            val slice = entry.jsonObject
                            ContextBarSlice(
                                key = slice.getValue("key").jsonPrimitive.content,
                                tone = slice.getValue("tone").jsonPrimitive.content,
                                percent = slice.getValue("percent").jsonPrimitive.double,
                            )
                        },
                        legend = expected.getValue("legend").jsonArray.map { entry ->
                            val row = entry.jsonObject
                            ContextLegendRow(
                                key = row.getValue("key").jsonPrimitive.content,
                                label = row.getValue("label").jsonPrimitive.content,
                                tone = row.getValue("tone").jsonPrimitive.content,
                                tokens = row.getValue("tokens").jsonPrimitive.content,
                                percent = row.getValue("percent").jsonPrimitive.content,
                                estimated = row.getValue("estimated").jsonPrimitive.boolean,
                                detail = row["detail"]?.jsonPrimitive?.content,
                            )
                        },
                    )
                },
            )
        }

    /** The object at [key], or null for an explicit `null` / an absent one. */
    private fun JsonObject.nullable(key: String): JsonObject? =
        this[key]?.takeIf { it !is JsonNull }?.jsonObject

    private fun severityOf(raw: String): AgentUsageSeverity = when (raw) {
        "danger" -> AgentUsageSeverity.Danger
        "warning" -> AgentUsageSeverity.Warning
        else -> AgentUsageSeverity.Normal
    }
}

/**
 * The contract fixture, located relative to the Gradle test working directory
 * (the `app` module dir) with fallbacks so the suite also runs from the
 * `apps/android` dir or the repo root.
 */
private fun fixtureJson(): String {
    val candidates = listOf(
        "../../../packages/domain-contract/fixtures/context-layout.json",
        "../../packages/domain-contract/fixtures/context-layout.json",
        "packages/domain-contract/fixtures/context-layout.json",
    )
    val file = candidates.map(::JavaFile).firstOrNull { it.isFile }
        ?: error("context-layout.json not found from ${JavaFile(".").absolutePath}")
    return file.readText()
}
