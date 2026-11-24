package com.exponential.app.navigation

import android.app.Application
import androidx.navigation.NavDestination
import androidx.navigation.NavDestinationBuilder
import androidx.navigation.NavGraphBuilder
import androidx.navigation.NavType
import androidx.navigation.Navigator
import androidx.navigation.createGraph
import androidx.navigation.testing.TestNavHostController
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/**
 * #ERR-27 memory budget: a notification tap lands by THE back stack rule
 * (#EXP-1830: the Chats root, then the target), so N taps leave ONE chat
 * entry, hence one ChatViewModel, never N. Each tap is still a NEW entry (a
 * fresh ViewModelStore: the EXP-528 stale-argument bug stays fixed).
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class DeepLinkBackStackTest {
    private lateinit var nav: TestNavHostController

    @Before
    fun setUp() {
        nav = TestNavHostController(RuntimeEnvironment.getApplication())
        nav.graph = nav.createGraph(startDestination = MainTabs.CHATS) {
            MainTabs.routes.forEach { screen(it) }
            screen(MainTabs.INBOX)
            screen(Routes.CHAT_PATTERN, "tab", "plan", "card")
            screen(Routes.SESSION_PATTERN)
            screen(ISSUE_ROUTE, "comment")
        }
    }

    /** A destination on the test navigator (any name resolves to one). */
    private fun NavGraphBuilder.screen(route: String, vararg optional: String) {
        val navigator = provider.getNavigator<Navigator<NavDestination>>("test")
        destination(
            NavDestinationBuilder(navigator, route).apply {
                optional.forEach { name ->
                    argument(name) {
                        type = NavType.StringType
                        nullable = true
                    }
                }
            },
        )
    }

    private fun routes() = nav.currentBackStack.value.map { it.destination.route }

    private fun chatEntries() = nav.currentBackStack.value.filter { it.destination.route == Routes.CHAT_PATTERN }

    @Test
    fun manyNotificationTapsLeaveOneChatOnTheRoot() {
        // Somewhere deep in another tab first.
        nav.selectTab(MainTabs.TEAMS)
        nav.navigate(Routes.chat("open"))
        val seen = HashSet<String>()
        repeat(40) { tap ->
            val id = "chat-${tap % 7}"
            nav.launchOnRoot(Routes.chat(id), marker = null)
            val top = nav.currentBackStackEntry!!
            assertEquals(id, top.arguments?.getString("sessionId"))
            assertTrue("every tap is a fresh entry (EXP-528)", seen.add(top.id))
            assertEquals(1, chatEntries().size)
        }
        assertEquals(listOf(null, MainTabs.CHATS, Routes.CHAT_PATTERN), routes())
    }

    @Test
    fun issueAndSessionTapsReplaceTooAndTheCommentIsKept() {
        nav.navigate(Routes.chat("a"))
        repeat(10) { nav.launchIssueOnRoot("issue-$it", marker = null, commentId = "c$it") }
        repeat(10) { nav.launchOnRoot(Routes.session("s-$it"), marker = null) }
        assertEquals(listOf(null, MainTabs.CHATS, Routes.SESSION_PATTERN), routes())
        nav.launchIssueOnRoot("i", marker = null, commentId = "c")
        assertEquals("c", nav.currentBackStackEntry?.arguments?.getString("comment"))
        // The same issue + comment again: left alone (the re-tap rule).
        val entry = nav.currentBackStackEntry!!.id
        nav.launchIssueOnRoot("i", marker = null, commentId = "c")
        assertEquals(entry, nav.currentBackStackEntry!!.id)
    }

    @Test
    fun anInAppOpenStillPushes() {
        // An inbox row or a link inside a screen pushes over it (Back returns there).
        repeat(3) { nav.navigateDeepLink(Routes.chat("c$it")) }
        assertEquals(3, chatEntries().size)
    }
}
