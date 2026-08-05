package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.InlineMark
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The editor-side chip transform (EXP-322). Compose wraps a
 * [androidx.compose.ui.text.input.VisualTransformation]'s offset mapping in
 * `ValidatingOffsetMapping`, which `check(result in 0..length)` in BOTH
 * directions and throws `IllegalStateException` — so range validity is the
 * contract these tests exist to nail down, alongside the caret behaviour that
 * makes the chip usable.
 */
class IssueChipTransformTest {

    private fun refs(vararg pairs: Pair<String, String>) = IssueRefHandler(
        candidates = pairs.map { (id, title) -> IssueRefTarget("id-$id", id, title) },
        onOpen = {},
    )

    private fun build(
        source: String,
        marks: List<InlineMark> = emptyList(),
        handler: IssueRefHandler? = refs("EXP-238" to "mobile chips broken"),
        enabled: Boolean = true,
    ) = IssueChipTransform.build(source, marks, handler, enabled)

    // --- identity cases -----------------------------------------------------

    @Test
    fun noHashIsIdentity() {
        val t = build("just words")
        assertTrue(t.isIdentity)
        assertEquals("just words", t.display)
        assertTrue(t.chips.isEmpty())
    }

    @Test
    fun unresolvedIdentifierIsIdentity() {
        val t = build("see #EXP-999 please")
        assertTrue(t.isIdentity)
        assertEquals("see #EXP-999 please", t.display)
    }

    /** EXP-423: the painter needs the resolved issue behind each chip. */
    @Test
    fun everyChipCarriesItsTarget() {
        val t = build("see #EXP-238 please")
        assertEquals(1, t.chips.size)
        assertEquals("EXP-238", t.chips[0].target.identifier)
    }

    @Test
    fun nullHandlerIsIdentity() {
        assertTrue(build("see #EXP-238", handler = null).isIdentity)
    }

    @Test
    fun disabledRowIsIdentity() {
        assertTrue(build("see #EXP-238", enabled = false).isIdentity)
    }

    @Test
    fun blankTitleKeepsTheBareToken() {
        val t = build("see #EXP-1", handler = refs("EXP-1" to "   "))
        assertTrue(t.isIdentity)
    }

    @Test
    fun tokenInsideInlineCodeIsIdentity() {
        val source = "see #EXP-238 now"
        val marks = listOf(InlineMark(4, 12, InlineKind.InlineCode))
        assertTrue(build(source, marks).isIdentity)
    }

    @Test
    fun tokenInsideALinkIsIdentity() {
        val source = "see #EXP-238 now"
        val marks = listOf(InlineMark(0, 12, InlineKind.Link, href = "https://x.test"))
        assertTrue(build(source, marks).isIdentity)
    }

    // --- display string -----------------------------------------------------

    @Test
    fun titleIsAppendedAfterTheToken() {
        val t = build("see #EXP-238 now")
        assertFalse(t.isIdentity)
        assertEquals("see #EXP-238 mobile chips broken now", t.display)
    }

    @Test
    fun displayMatchesTheReadRendererForARefOnlyLine() {
        val source = "see #EXP-238 now"
        val handler = refs("EXP-238" to "mobile chips broken")
        val refPills = resolvedRefPills(source, emptyList(), handler)
        val read = MentionDisplay.build(source, emptyList(), refPills)
        assertEquals(read.text, build(source, handler = handler).display)
    }

    @Test
    fun longTitlesTruncateWithAnEllipsis() {
        val long = "x".repeat(80)
        val t = build("#EXP-1", handler = refs("EXP-1" to long))
        val expected = "x".repeat(MAX_CHIP_TITLE_CHARS - 1) + "…"
        assertEquals("#EXP-1 $expected", t.display)
    }

    @Test
    fun mentionsAreNotSubstitutedInTheEditor() {
        val t = build("ping @ada@example.com about #EXP-238")
        assertTrue(t.display.startsWith("ping @ada@example.com about "))
    }

    @Test
    fun twoChipsOnOneLineBothInject() {
        val handler = refs("EXP-1" to "first", "EXP-2" to "second")
        val t = build("#EXP-1 and #EXP-2", handler = handler)
        assertEquals(2, t.chips.size)
        assertEquals("#EXP-1 first and #EXP-2 second", t.display)
    }

    // --- offset mapping contract -------------------------------------------

    private val corpus = listOf(
        "see #EXP-238 now",
        "#EXP-238",
        "#EXP-238 ",
        "a #EXP-1 b #EXP-2 c",
        "no refs here",
        "#EXP-1#EXP-2",
        "trailing #EXP-2",
    )

    private fun corpusHandler() =
        refs("EXP-238" to "mobile chips broken", "EXP-1" to "first", "EXP-2" to "second")

    @Test
    fun everyOffsetMapsInRangeInBothDirections() {
        for (source in corpus) {
            val t = build(source, handler = corpusHandler())
            for (o in 0..source.length) {
                val mapped = t.originalToTransformed(o)
                assertTrue("$source: o2t($o) = $mapped", mapped in 0..t.display.length)
            }
            for (q in 0..t.display.length) {
                val mapped = t.transformedToOriginal(q)
                assertTrue("$source: t2o($q) = $mapped", mapped in 0..source.length)
            }
        }
    }

    @Test
    fun outOfRangeInputsAreCoercedNotThrown() {
        val t = build("see #EXP-238 now")
        assertEquals(0, t.originalToTransformed(-5))
        assertEquals(t.display.length, t.originalToTransformed(9999))
        assertEquals(0, t.transformedToOriginal(-5))
        assertEquals(t.source.length, t.transformedToOriginal(9999))
    }

    @Test
    fun mappingIsMonotone() {
        for (source in corpus) {
            val t = build(source, handler = corpusHandler())
            var previous = -1
            for (o in 0..source.length) {
                val mapped = t.originalToTransformed(o)
                assertTrue("$source: o2t not monotone at $o", mapped >= previous)
                previous = mapped
            }
            previous = -1
            for (q in 0..t.display.length) {
                val mapped = t.transformedToOriginal(q)
                assertTrue("$source: t2o not monotone at $q", mapped >= previous)
                previous = mapped
            }
        }
    }

    @Test
    fun roundTripIsLosslessForEverySourceOffset() {
        for (source in corpus) {
            val t = build(source, handler = corpusHandler())
            for (o in 0..source.length) {
                assertEquals(
                    "$source: round trip at $o",
                    o,
                    t.transformedToOriginal(t.originalToTransformed(o)),
                )
            }
        }
    }

    @Test
    fun offsetsInsideTheTokenStayLinear() {
        // "see #EXP-238 now" — the token spans [4, 12).
        val t = build("see #EXP-238 now")
        for (o in 0..12) assertEquals(o, t.originalToTransformed(o))
    }

    @Test
    fun caretAtTheTokenEndRendersBeforeTheTitle() {
        val t = build("see #EXP-238 now")
        val chip = t.chips.single()
        assertEquals(chip.displayTokenEnd, t.originalToTransformed(chip.sourceEnd))
    }

    @Test
    fun offsetsInsideTheTitleSnapToTheTokenEnd() {
        val t = build("see #EXP-238 now")
        val chip = t.chips.single()
        for (q in chip.displayTokenEnd + 1..chip.displayEnd) {
            assertEquals(chip.sourceEnd, t.transformedToOriginal(q))
        }
    }

    @Test
    fun offsetsAfterASecondChipAccumulateBothDeltas() {
        val handler = refs("EXP-1" to "first", "EXP-2" to "second")
        val source = "#EXP-1 and #EXP-2 tail"
        val t = build(source, handler = handler)
        val injected = t.chips.sumOf { it.injectedLength }
        assertEquals(source.length + injected, t.originalToTransformed(source.length))
    }

    // --- mark remapping -----------------------------------------------------

    @Test
    fun aBoldMarkEndingAtTheTokenEndStillEndsThere() {
        val source = "see #EXP-238 now"
        val t = build(source)
        val chip = t.chips.single()
        assertEquals(chip.displayTokenEnd, t.originalToTransformed(12))
        assertEquals(0, t.originalToTransformed(0))
    }

    @Test
    fun marksAfterAChipShiftByTheInjectedLength() {
        val source = "see #EXP-238 now"
        val t = build(source)
        val injected = t.chips.single().injectedLength
        assertEquals(13 + injected, t.originalToTransformed(13))
        assertEquals(16 + injected, t.originalToTransformed(16))
    }

    // The identity instance must reuse the source string so the hot path
    // allocates nothing beyond the wrapper.
    @Test
    fun identityDisplayIsTheSourceInstance() {
        val source = "plain words"
        assertSame(source, build(source).display)
    }
}
