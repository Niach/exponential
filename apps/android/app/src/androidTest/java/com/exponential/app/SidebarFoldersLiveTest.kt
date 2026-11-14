package com.exponential.app

import android.graphics.Bitmap
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.test.espresso.Espresso
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import com.exponential.app.domain.Apps
import com.exponential.app.domain.Chats
import java.io.File
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith

/**
 * #EXP-1832: PR pictures of the phone's READ-ONLY sidebar folders on the REAL
 * screens, signed in as the demo member of `seed:screenshots` (its board
 * folder "Product" and app folder "Ops tools"): the board menu with the
 * folder open, then folded (and unfolded again), and the Apps tab. Test-only;
 * run with `-e class com.exponential.app.SidebarFoldersLiveTest -e
 * instanceUrl …` and pull `files/sidebar-folders/<name>.png`.
 */
@RunWith(AndroidJUnit4::class)
class SidebarFoldersLiveTest {
    private val permissionRule: GrantPermissionRule = ScreenshotFlow.permissionRule()
    private val composeRule = createAndroidComposeRule<MainActivity>()
    private val sessionRule = ScreenshotSessionRule(email = null)

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(permissionRule).around(sessionRule).around(composeRule)

    private val flow by lazy { ScreenshotFlow(composeRule) }

    private fun capture(name: String) {
        flow.settle(longer = true)
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val bitmap = instrumentation.uiAutomation.takeScreenshot()
        val dir = File(instrumentation.targetContext.filesDir, "sidebar-folders").apply { mkdirs() }
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    @Test
    fun boardMenuAndApps() {
        val instanceUrl = ScreenshotFlow.instanceUrl()
        if (!ScreenshotSession.signIn(instanceUrl, ScreenshotFlow.DEMO_EMAIL, ScreenshotFlow.DEMO_PASSWORD)) {
            flow.signIn(instanceUrl)
        }
        flow.waitFor(hasTestTag(Chats.TestIds.TAB_BOARD), ScreenshotFlow.FIRST_SYNC_TIMEOUT)
        composeRule.onNode(hasTestTag(Chats.TestIds.TAB_BOARD)).performClick()
        flow.waitFor(hasTestTag("${Chats.TestIds.BOARD_CHIP}-all"), ScreenshotFlow.FIRST_SYNC_TIMEOUT)
        flow.settle(longer = true)

        // The board menu (the phone's board switcher): Mobile App, the
        // "Product" folder open with its two boards one level in.
        composeRule.onNode(hasTestTag(Chats.TestIds.TAB_BOARD)).performClick()
        flow.waitFor(hasTestTag(Chats.TestIds.BOARD_MENU), ScreenshotFlow.NAV_TIMEOUT)
        flow.waitFor(hasTestTag("board-menu-folder"), ScreenshotFlow.SYNC_TIMEOUT)
        capture("board-menu-open")
        composeRule.onAllNodes(hasTestTag("board-menu-folder")).onFirst().performClick()
        flow.waitFor(hasTestTag("sidebar-folder-closed"), ScreenshotFlow.NAV_TIMEOUT, useUnmergedTree = true)
        capture("board-menu-folded")
        composeRule.onAllNodes(hasTestTag("board-menu-folder")).onFirst().performClick()
        flow.waitFor(hasTestTag("sidebar-folder-open"), ScreenshotFlow.NAV_TIMEOUT, useUnmergedTree = true)
        Espresso.pressBack()
        flow.waitForGone(hasTestTag(Chats.TestIds.BOARD_MENU), ScreenshotFlow.NAV_TIMEOUT)

        // The Apps tab: the "Ops tools" folder holding Team dashboard + Working hours.
        composeRule.onNode(hasTestTag("tab-apps")).performClick()
        flow.waitFor(hasTestTag(Apps.TestIds.LIST), ScreenshotFlow.NAV_TIMEOUT)
        flow.waitFor(hasText("Ops tools"), ScreenshotFlow.SYNC_TIMEOUT)
        capture("apps")
    }
}
