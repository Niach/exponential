package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// #EXP-1830: the home widgets' exponential:// links (iOS HomeWidgets.Links +
// LaunchTarget(url:)).
class WidgetLinksTest {
    private fun parse(host: String?, path: List<String> = emptyList(), query: Map<String, String> = emptyMap()) =
        WidgetLinks.parse(host, path) { query[it] }

    @Test
    fun `hosts without an id`() {
        assertEquals(WidgetLinks.Target.Home, parse("home"))
        assertEquals(WidgetLinks.Target.MyIssues, parse("my-issues"))
        assertEquals(WidgetLinks.Target.NeedsYou, parse("needs-you"))
    }

    @Test
    fun `board and issue need their id`() {
        assertEquals(WidgetLinks.Target.Board("b1"), parse("board", listOf("b1")))
        assertEquals(WidgetLinks.Target.Issue("i1"), parse("issue", listOf("i1")))
        assertNull(parse("board"))
        assertNull(parse("issue", listOf("")))
    }

    @Test
    fun `a needs-you row opens its root by opens on its message`() {
        assertEquals(
            WidgetLinks.Target.NeedsYouRow("r1", "m1", NeedsYou.Opens.Plan),
            parse("session", listOf("r1"), mapOf("opens" to "plan", "message" to "m1")),
        )
        assertEquals(
            WidgetLinks.Target.NeedsYouRow("r1", "m1", NeedsYou.Opens.Results),
            parse("session", listOf("r1"), mapOf("opens" to "results", "message" to "m1")),
        )
        // A push's session link (no `opens`) is not a widget link.
        assertNull(parse("session", listOf("r1")))
        assertNull(parse("session", listOf("r1"), mapOf("opens" to "nope", "message" to "m1")))
        assertNull(parse("session", listOf("r1"), mapOf("opens" to "card")))
    }

    @Test
    fun `anything else is not a widget link`() {
        assertNull(parse("inbox"))
        assertNull(parse("device", listOf("d1")))
        assertNull(parse(null))
    }

    @Test
    fun `the builders produce what parse reads`() {
        assertEquals("exponential://board/b1", WidgetLinks.board("b1"))
        assertEquals("exponential://my-issues", WidgetLinks.MY_ISSUES)
        assertEquals("exponential://needs-you", WidgetLinks.NEEDS_YOU)
        assertEquals("exponential://home", WidgetLinks.HOME)
        assertEquals("exponential://issue/i1", WidgetLinks.issue("i1"))
        assertEquals("exponential://session/r1?opens=card&message=m1", WidgetLinks.needsYouRow("r1", "m1", NeedsYou.Opens.Card))
    }
}
