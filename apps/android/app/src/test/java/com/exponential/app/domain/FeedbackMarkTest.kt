package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-1275 (EXP-1259 D8): the feedback states, replayed off
// `feedback-states.json` (web `feedback-mark.test.ts`).
class FeedbackMarkTest {
    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("feedback-states.json")).jsonObject

    private fun JsonObject.obj(key: String): JsonObject = getValue(key).jsonObject
    private fun JsonObject.string(key: String): String = getValue(key).jsonPrimitive.content
    private fun JsonObject.num(key: String): Double = getValue(key).jsonPrimitive.double

    @Test
    fun `markStripeSpan replays every case`() {
        val cases = fixture.obj("mark").getValue("cases").jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val phase = case.num("phase")
            val spans = case.getValue("spans").jsonArray
            spans.forEachIndexed { index, want ->
                val got = FeedbackStates.markStripeSpan(index, phase)
                if (want is JsonNull) {
                    assertNull("phase $phase stripe $index", got)
                } else {
                    val pair = (want as JsonArray).map { it.jsonPrimitive.double }
                    val span = got ?: error("phase $phase stripe $index: expected $pair, got null")
                    assertEquals("phase $phase stripe $index from", pair[0], span.from, 1e-9)
                    assertEquals("phase $phase stripe $index to", pair[1], span.to, 1e-9)
                }
            }
        }
    }

    @Test
    fun `the stripes are the fixture's`() {
        val mark = fixture.obj("mark")
        val stripes = mark.getValue("stripes").jsonArray.map { it.jsonObject }
        assertEquals(stripes.size, FeedbackStates.STRIPES.size)
        stripes.zip(FeedbackStates.STRIPES).forEach { (want, got) ->
            val cubic = want.getValue("cubic").jsonArray.map { p ->
                p.jsonArray.let { it[0].jsonPrimitive.double to it[1].jsonPrimitive.double }
            }
            assertEquals(cubic, got.cubic)
            val draw = want.getValue("draw").jsonArray.map { it.jsonPrimitive.double }
            val erase = want.getValue("erase").jsonArray.map { it.jsonPrimitive.double }
            assertEquals(draw[0] to draw[1], got.draw)
            assertEquals(erase[0] to erase[1], got.erase)
        }
        assertEquals(mark.getValue("loopMs").jsonPrimitive.int, FeedbackStates.MARK_LOOP_MS)
        assertEquals(mark.getValue("samples").jsonPrimitive.int, FeedbackStates.MARK_SAMPLES)
        assertEquals(mark.num("stripeWidth"), FeedbackStates.MARK_STRIPE_WIDTH, 0.0)
        val sizes = mark.obj("sizes")
        assertEquals(sizes.getValue("inline").jsonPrimitive.int, FeedbackStates.MARK_SIZE_INLINE)
        assertEquals(sizes.getValue("status").jsonPrimitive.int, FeedbackStates.MARK_SIZE_STATUS)
        assertEquals(sizes.getValue("styleguide").jsonPrimitive.int, FeedbackStates.MARK_SIZE_STYLEGUIDE)
    }

    @Test
    fun `markStripePoint walks the cubic end to end`() {
        FeedbackStates.STRIPES.forEachIndexed { index, stripe ->
            val start = FeedbackStates.markStripePoint(index, 0.0)
            val end = FeedbackStates.markStripePoint(index, 1.0)
            assertEquals(stripe.cubic[0].first, start.first, 1e-9)
            assertEquals(stripe.cubic[0].second, start.second, 1e-9)
            assertEquals(stripe.cubic[3].first, end.first, 1e-9)
            assertEquals(stripe.cubic[3].second, end.second, 1e-9)
        }
    }

    @Test
    fun `the constants and copy are the fixture's`() {
        val ids = fixture.obj("testIds")
        assertEquals(ids.string("spinner"), FeedbackStates.SPINNER_TEST_ID)
        assertEquals(ids.string("pending"), FeedbackStates.PENDING_TEST_ID)
        assertEquals(ids.string("skeleton"), FeedbackStates.SKELETON_TEST_ID)
        assertEquals(ids.string("mark"), FeedbackStates.MARK_TEST_ID)
        assertEquals(ids.string("blocking"), FeedbackStates.BLOCKING_TEST_ID)
        assertEquals(ids.string("starting"), FeedbackStates.STARTING_TEST_ID)

        val inline = fixture.obj("inline")
        assertEquals(inline.getValue("spinner").jsonPrimitive.int, FeedbackStates.INLINE_SPINNER)
        assertEquals(inline.getValue("gap").jsonPrimitive.int, FeedbackStates.INLINE_GAP)
        val words = inline.obj("words")
        assertEquals(words.string("saving"), FeedbackStates.WORD_SAVING)
        assertEquals(words.string("saved"), FeedbackStates.WORD_SAVED)
        assertEquals(words.string("loading"), FeedbackStates.WORD_LOADING)

        val pending = fixture.obj("pending")
        assertEquals(pending.getValue("delayMs").jsonPrimitive.long, FeedbackStates.PENDING_DELAY_MS)
        assertEquals(pending.getValue("spinner").jsonPrimitive.int, FeedbackStates.PENDING_SPINNER)
        assertEquals(
            pending.obj("words").mapValues { it.value.jsonPrimitive.content },
            FeedbackStates.PENDING_WORDS,
        )

        val placeholder = fixture.obj("placeholder")
        assertEquals(placeholder.getValue("rows").jsonPrimitive.int, FeedbackStates.PLACEHOLDER_ROWS)
        assertEquals(placeholder.getValue("barHeight").jsonPrimitive.int, FeedbackStates.PLACEHOLDER_BAR_HEIGHT)
        assertEquals(placeholder.getValue("radius").jsonPrimitive.int, FeedbackStates.PLACEHOLDER_RADIUS)
        assertEquals(placeholder.getValue("dot").jsonPrimitive.int, FeedbackStates.PLACEHOLDER_DOT)
        assertEquals(placeholder.getValue("pulseMs").jsonPrimitive.int, FeedbackStates.PLACEHOLDER_PULSE_MS)
        val opacity = placeholder.getValue("opacity").jsonArray.map { it.jsonPrimitive.double }
        assertEquals(listOf(FeedbackStates.PLACEHOLDER_OPACITY_LOW, FeedbackStates.PLACEHOLDER_OPACITY_HIGH), opacity)
        assertEquals(placeholder.num("reducedOpacity"), FeedbackStates.PLACEHOLDER_REDUCED_OPACITY, 0.0)

        val blocking = fixture.obj("blocking")
        assertEquals(blocking.num("backdropOpacity"), FeedbackStates.BLOCKING_BACKDROP_OPACITY, 0.0)
        assertEquals(blocking.getValue("markSize").jsonPrimitive.int, FeedbackStates.BLOCKING_MARK_SIZE)
        assertEquals(blocking.getValue("gap").jsonPrimitive.int, FeedbackStates.BLOCKING_GAP)
        val copy = blocking.obj("copy")
        assertEquals(copy.string("switchTeam"), FeedbackStates.BLOCKING_SWITCH_TEAM)
        assertEquals(copy.string("switchAccount"), FeedbackStates.BLOCKING_SWITCH_ACCOUNT)
        assertEquals(copy.string("signOut"), FeedbackStates.BLOCKING_SIGN_OUT)

        val starting = fixture.obj("starting")
        assertEquals(starting.string("label"), FeedbackStates.STARTING_LABEL)
        assertEquals(starting.getValue("timeoutMs").jsonPrimitive.long, FeedbackStates.STARTING_TIMEOUT_MS)
        assertEquals(starting.getValue("watchMs").jsonPrimitive.long, FeedbackStates.STARTING_WATCH_MS)
        assertEquals(starting.getValue("dedupeMs").jsonPrimitive.long, FeedbackStates.STARTING_DEDUPE_MS)
        val toast = starting.obj("timeoutToast")
        assertEquals(toast.string("title"), FeedbackStates.STARTING_TIMEOUT_TITLE)
        assertEquals(toast.string("description"), FeedbackStates.STARTING_TIMEOUT_DESCRIPTION)
        assertEquals(toast.string("action"), FeedbackStates.RETRY)
        assertEquals("Starting on macbook", FeedbackStates.startingLabel("macbook"))
    }

    @Test
    fun `the error titles follow the pattern`() {
        val errors = fixture.obj("errors")
        assertEquals(errors.string("titlePattern"), FeedbackStates.ERROR_TITLE_PATTERN)
        assertEquals(errors.string("retry"), FeedbackStates.RETRY)
        // EXP-1391: the refusal posted into the thread ends with `handoff`.
        assertEquals(errors.obj("mergeRefusal").string("handoff"), FeedbackStates.MERGE_HANDOFF)
        for (element in errors.getValue("cases").jsonArray) {
            val case = element.jsonObject
            if (case.string("kind") != "action") continue
            assertEquals(
                case.string("name"),
                case.string("title"),
                FeedbackStates.errorTitle(case.string("verb"), case.string("thing")),
            )
        }
    }

    @Test
    fun `every error case renders where the fixture says, background work stays quiet`() {
        val cases = fixture.obj("errors").getValue("cases").jsonArray.map { it.jsonObject }
        for (case in cases) {
            assertEquals(case.string("name"), case.string("render"), FeedbackStates.errorSurface(case.string("kind")))
        }
        assertTrue(cases.any { it.string("kind") == "background" })
    }
}
