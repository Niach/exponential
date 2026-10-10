package com.exponential.app.ui.issue

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The two relative-time wordings ×4: activity + comments = the LONG form
 * (web `formatDistanceToNowStrict`), list captions = the COMPACT form (web
 * inbox formatter, desktop `inbox::relative_time`).
 */
class RelativeTimeTest {
    private val s = 1000L
    private val m = 60 * s
    private val h = 60 * m
    private val d = 24 * h

    @Test
    fun `long form matches the web wording`() {
        assertEquals("20 seconds ago", longRelativeTime(20 * s))
        assertEquals("1 minute ago", longRelativeTime(1 * m))
        assertEquals("5 minutes ago", longRelativeTime(5 * m))
        assertEquals("2 hours ago", longRelativeTime(2 * h))
        assertEquals("1 day ago", longRelativeTime(26 * h))
        assertEquals("2 days ago", longRelativeTime(2 * d))
        assertEquals("2 months ago", longRelativeTime(60 * d))
        assertEquals("0 seconds ago", longRelativeTime(-5 * s))
    }

    @Test
    fun `compact form matches desktop inbox relative_time`() {
        assertEquals("just now", compactRelativeTime(20 * s))
        assertEquals("2m", compactRelativeTime(2 * m))
        assertEquals("3h", compactRelativeTime(3 * h))
        assertEquals("5h", compactRelativeTime(5 * h))
        assertEquals("1d", compactRelativeTime(26 * h))
        assertEquals("2d", compactRelativeTime(50 * h))
        assertEquals("just now", compactRelativeTime(-h))
    }
}
