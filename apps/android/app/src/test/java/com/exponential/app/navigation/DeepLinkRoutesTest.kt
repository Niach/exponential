package com.exponential.app.navigation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * A push tap must open the issue the notification is about (EXP-528). The
 * drain used to navigate with launchSingleTop, which replaces the top entry
 * and reuses its id — hence its ViewModelStore, hence an IssueDetailViewModel
 * still holding the PREVIOUS issue's id — so a tap arriving while another
 * issue was open re-rendered that other issue. Deduping on the concrete route
 * instead is what lets the drain push a fresh entry while a re-tap for what is
 * already on screen stays a no-op.
 */
class DeepLinkRoutesTest {

    private fun args(vararg pairs: Pair<String, String>): (String) -> String? {
        val map = pairs.toMap()
        return { name -> map[name] }
    }

    // MARK: - concreteRoute

    @Test
    fun `fills the issue route pattern`() {
        assertEquals(
            "issue/abc",
            DeepLinkRoutes.concreteRoute("issue/{issueId}", args("issueId" to "abc")),
        )
    }

    @Test
    fun `fills the other deep-link route patterns`() {
        assertEquals(
            "support/t1",
            DeepLinkRoutes.concreteRoute("support/{threadId}", args("threadId" to "t1")),
        )
        assertEquals(
            "invite/tok",
            DeepLinkRoutes.concreteRoute("invite/{token}", args("token" to "tok")),
        )
    }

    @Test
    fun `an argument-less pattern fills to itself`() {
        assertEquals("home", DeepLinkRoutes.concreteRoute("home", args()))
    }

    @Test
    fun `fills a placeholder that is not the last segment`() {
        assertEquals(
            "issue/abc/changes",
            DeepLinkRoutes.concreteRoute("issue/{issueId}/changes", args("issueId" to "abc")),
        )
    }

    @Test
    fun `fills every placeholder and not just the first`() {
        assertEquals(
            "a/1/b/2",
            DeepLinkRoutes.concreteRoute("a/{one}/b/{two}", args("one" to "1", "two" to "2")),
        )
    }

    @Test
    fun `an unresolvable placeholder yields null`() {
        assertNull(DeepLinkRoutes.concreteRoute("issue/{issueId}", args()))
    }

    @Test
    fun `a null pattern yields null`() {
        assertNull(DeepLinkRoutes.concreteRoute(null, args("issueId" to "abc")))
    }

    @Test
    fun `an unclosed placeholder yields null`() {
        assertNull(DeepLinkRoutes.concreteRoute("issue/{issueId", args("issueId" to "abc")))
    }

    // MARK: - isOnTop

    @Test
    fun `a push for another issue is not already on top`() {
        // The regression itself: issue A open, notification for issue B.
        assertFalse(
            DeepLinkRoutes.isOnTop("issue/{issueId}", args("issueId" to "A"), "issue/B")
        )
    }

    @Test
    fun `a push for the issue already open is on top`() {
        assertTrue(
            DeepLinkRoutes.isOnTop("issue/{issueId}", args("issueId" to "A"), "issue/A")
        )
    }

    @Test
    fun `another destination is never on top`() {
        // Cold start and every bottom-bar root: the drain must navigate.
        assertFalse(DeepLinkRoutes.isOnTop("home", args(), "issue/A"))
        assertFalse(
            DeepLinkRoutes.isOnTop("support/{threadId}", args("threadId" to "A"), "issue/A")
        )
    }

    @Test
    fun `the diff page of the same issue is not the issue route`() {
        assertFalse(
            DeepLinkRoutes.isOnTop("issue/{issueId}/changes", args("issueId" to "A"), "issue/A")
        )
    }

    @Test
    fun `an unidentifiable top is never on top`() {
        // No graph yet, or an argument the entry doesn't carry — push, never drop.
        assertFalse(DeepLinkRoutes.isOnTop(null, args(), "issue/A"))
        assertFalse(DeepLinkRoutes.isOnTop("issue/{issueId}", args(), "issue/A"))
    }
}
