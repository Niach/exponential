package com.exponential.app

import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.test.espresso.Espresso
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.exponential.app.data.steer.STEER_NO_FEED_IDLE_TITLE
import com.exponential.app.domain.AgentHome
import com.exponential.app.domain.Chats
import com.exponential.app.domain.SessionScreen
import com.exponential.app.domain.SessionTree
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith

/**
 * #EXP-1597 / #EXP-1598: the path that crashed after login, end to end, against
 * the seeded capture backend (`bun run seed:screenshots`, the same world as
 * the screenshot lanes; `instanceUrl` instrumentation argument like them):
 *
 * - signed OUT, through the REAL login UI (instance picker → email →
 *   password), onto the home page. The login screen's success used to navigate
 *   to Agent on the signed-out graph, which has no such route: an
 *   IllegalArgumentException whenever the token landed a frame after it.
 * - home → Chats → a temporary session (a chat no agent runs) → its page,
 *   and Back to the list.
 * - a chat's transcript: the cold-start chat's conversation shows its
 *   synced rows.
 */
@RunWith(AndroidJUnit4::class)
class LoginToSessionTest {

    private companion object {
        // seed-screenshots.ts `offlineSpike`: a root with no agent, i.e. a
        // Temporary session.
        const val TEMPORARY_CHAT_TITLE = "Spike: offline queue"
        // The cold-start chat (`coldStart`, APP-5).
        const val COLD_START_CHAT_TITLE = ScreenshotFlow.SHOWCASE_ISSUE_TITLE
        // Its last row: the agent's question (seed DEMO_FEED_QUESTION).
        const val COLD_START_TAIL_FRAGMENT = "Lazy-load the markdown editor too"
    }

    private val composeRule = createAndroidComposeRule<MainActivity>()

    // Signed out before the activity launches: the instance picker shows.
    private val sessionRule = ScreenshotSessionRule(email = null)

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(ScreenshotFlow.permissionRule()).around(sessionRule).around(composeRule)

    private val flow by lazy { ScreenshotFlow(composeRule) }

    @Test
    fun loginThenOpenATemporarySessionAndAChatTranscript() {
        flow.signIn(ScreenshotFlow.instanceUrl())

        // Home: the Agent page under the tab bar (the crash killed the app here).
        flow.waitFor(hasTestTag(AgentHome.HOME_TEST_ID), ScreenshotFlow.FIRST_SYNC_TIMEOUT)
        flow.waitFor(hasTestTag(Chats.TestIds.TAB_BOARD), ScreenshotFlow.SYNC_TIMEOUT)

        // Chats → the temporary session.
        composeRule.onNode(hasTestTag("tab-chats")).performClick()
        flow.waitFor(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), ScreenshotFlow.NAV_TIMEOUT)
        openChat(TEMPORARY_CHAT_TITLE)
        flow.waitFor(hasTestTag(SessionScreen.TestIds.TABS), ScreenshotFlow.NAV_TIMEOUT)
        // A temporary session's page IS its worker's live transcript: it
        // settles out of "Connecting…" into the feed, or (no device
        // publishing it) the idle placeholder.
        flow.waitFor(
            hasTestTag("agent-feed") or hasText(STEER_NO_FEED_IDLE_TITLE),
            ScreenshotFlow.NETWORK_TIMEOUT,
        )
        flow.settle()
        Espresso.pressBack()
        flow.waitFor(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), ScreenshotFlow.NAV_TIMEOUT)

        // A chat's Chat tab (#EXP-1732) = its ROOT's own session view: it
        // settles into the feed (or the idle placeholder), not an endless wait.
        openChat(COLD_START_CHAT_TITLE)
        flow.waitFor(hasTestTag(SessionScreen.TestIds.TABS), ScreenshotFlow.SYNC_TIMEOUT)
        flow.waitFor(
            hasTestTag("agent-feed") or hasText(STEER_NO_FEED_IDLE_TITLE),
            ScreenshotFlow.NETWORK_TIMEOUT,
        )
        Espresso.pressBack()
        flow.waitFor(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), ScreenshotFlow.NAV_TIMEOUT)
    }

    private fun openChat(title: String) {
        val row = hasTestTag(Chats.TestIds.CHAT_ROW) and hasText(title, substring = true)
        flow.waitForInList(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), row, ScreenshotFlow.SYNC_TIMEOUT)
        composeRule.onAllNodes(row).onFirst().performClick()
    }
}
