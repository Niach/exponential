package com.exponential.app

import android.Manifest
import android.os.Build
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.espresso.Espresso
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import org.junit.Before
import org.junit.ClassRule
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith
import tools.fastlane.screengrab.Screengrab
import tools.fastlane.screengrab.UiAutomatorScreenshotStrategy
import tools.fastlane.screengrab.locale.LocaleTestRule

/**
 * Automated Play Store screenshots, driven by fastlane screengrab
 * (`bundle exec fastlane screenshots` in apps/android — see fastlane/Screengrabfile).
 *
 * Drives the REAL app UI end-to-end against a locally seeded backend
 * (apps/web/scripts/seed-screenshots.ts): instance picker → password login →
 * board → issue detail → comments → agents → actions → reviews → inbox →
 * support inbox. Play caps phone screenshots at 8, so the create-issue and
 * search shots gave way to agents/reviews/actions (EXP-348) — the surfaces
 * that actually differentiate the product. The backend must be reachable
 * from the emulator (default http://10.0.2.2:5173, override via the
 * `instanceUrl` instrumentation argument / SCREENGRAB_INSTANCE_URL).
 *
 * Synchronization notes:
 * - `waitUntil` polls semantics without requiring Compose idleness, so infinite
 *   animations (indeterminate progress spinners while auth-config / Electric
 *   sync load) cannot hang it — this is why every step gates on content
 *   appearing rather than on waitForIdle.
 * - Screenshots use [UiAutomatorScreenshotStrategy]: the default reflection
 *   -based strategy renders a blank window for Compose surfaces.
 */
@RunWith(AndroidJUnit4::class)
class StoreScreenshotsTest {

    companion object {
        // screengrab switches the device locale per the Screengrabfile `locales`
        // list; must be a @ClassRule so it wraps the activity launch.
        @ClassRule @JvmField
        val localeTestRule = LocaleTestRule()

        private const val DEFAULT_INSTANCE_URL = "http://10.0.2.2:5173"
        private const val DEMO_EMAIL = "demo@exponential.at"
        private const val DEMO_PASSWORD = "screenshots-demo"

        private const val SHOWCASE_ISSUE_TITLE = "Reduce cold start below 800 ms"

        private const val NAV_TIMEOUT = 30_000L
        private const val SYNC_TIMEOUT = 60_000L
    }

    // Pre-grant POST_NOTIFICATIONS (SDK 33+) so MainActivity's permission request
    // never shows a system dialog over the screenshots.
    private val permissionRule: GrantPermissionRule =
        if (Build.VERSION.SDK_INT >= 33) {
            GrantPermissionRule.grant(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            GrantPermissionRule.grant()
        }

    private val composeRule = createAndroidComposeRule<MainActivity>()

    // Permission grant must land before the activity (and its permission
    // request) launches.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(permissionRule).around(composeRule)

    private val instanceUrl: String =
        InstrumentationRegistry.getArguments().getString("instanceUrl") ?: DEFAULT_INSTANCE_URL

    @Before
    fun setUp() {
        Screengrab.setDefaultScreenshotStrategy(UiAutomatorScreenshotStrategy())
    }

    @Test
    fun captureStoreScreenshots() {
        // --- Instance picker: cloud is the primary path (EXP-14), so reveal
        // the self-hosted URL field, then point the app at the seeded backend.
        waitFor(hasText("Use a self-hosted instance"), NAV_TIMEOUT)
        composeRule.onNode(hasText("Use a self-hosted instance")).performClick()
        waitFor(hasTestTag("instance-url-field"), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag("instance-url-field")).apply {
            performTextClearance()
            performTextInput(instanceUrl)
        }
        composeRule.onNode(hasText("Continue")).performClick()

        // --- Login: the email field only shows once /api/auth-config resolved.
        waitFor(hasTestTag("login-email-field"), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag("login-email-field")).performTextInput(DEMO_EMAIL)
        composeRule.onNode(hasTestTag("login-password-field")).performTextInput(DEMO_PASSWORD)
        Espresso.closeSoftKeyboard()
        composeRule.onNode(hasTestTag("login-submit-button")).performClick()

        // --- Board: wait out session fetch + first Electric sync until the
        // showcase issue row is on screen (in-progress group sits at the top),
        // then until the transient "Syncing…" pill has cleared — it photobombed
        // the board shot once (EXP-348).
        waitFor(hasText(SHOWCASE_ISSUE_TITLE), SYNC_TIMEOUT)
        waitForGone(hasText("Syncing", substring = true), SYNC_TIMEOUT)
        settle()
        Screengrab.screenshot("1_board")

        // --- Issue detail: open APP-5 and wait for its markdown description.
        composeRule.onAllNodes(hasText(SHOWCASE_ISSUE_TITLE)).onFirst().performClick()
        waitFor(hasText("Startup profiling", substring = true), NAV_TIMEOUT)
        settle()
        Screengrab.screenshot("2_issue-detail")

        // --- Comments: scroll the (non-lazy) detail column down to the thread.
        // Gate on the stable test tag, NOT the header copy — the header text
        // already drifted once ("Comments (n)" → "Activity") and silently
        // killed the whole screengrab run at shot 3 (EXP-348).
        waitFor(hasTestTag("comment-thread-header"), SYNC_TIMEOUT)
        // Aim for the last seeded comment so the thread fills the viewport;
        // fall back to the section header if markdown splits the text node.
        runCatching {
            composeRule.onAllNodes(hasText("re-run the profiling", substring = true))
                .onFirst()
                .performScrollTo()
        }.recoverCatching {
            composeRule.onAllNodes(hasTestTag("comment-thread-header"))
                .onFirst()
                .performScrollTo()
        }
        settle()
        Screengrab.screenshot("3_comments")

        // --- Agents tab: the seed inserts a running session on APP-5 and an
        // in-review session with an open PR, both with a fresh heartbeat (the
        // liveness guard hides sessions past the staleness window).
        composeRule.onNode(hasContentDescription("Back")).performClick()
        waitFor(hasContentDescription("Agents"), NAV_TIMEOUT)
        composeRule.onNode(hasContentDescription("Agents")).performClick()
        waitFor(hasTestTag("agent-session-row"), SYNC_TIMEOUT)
        settle()
        Screengrab.screenshot("4_agents")

        // --- Actions (EXP-253): the list rides the Agents header pill; the
        // seed inserts three team actions above the two client builtins.
        composeRule.onNode(hasTestTag("open-actions")).performClick()
        waitFor(hasTestTag("action-row"), SYNC_TIMEOUT)
        waitFor(hasText("Update dependencies", substring = true), SYNC_TIMEOUT)
        settle()
        Screengrab.screenshot("6_actions")

        // --- Reviews tab: the team's open pull requests (4 seeded).
        composeRule.onNode(hasContentDescription("Back")).performClick()
        waitFor(hasContentDescription("Reviews"), NAV_TIMEOUT)
        composeRule.onNode(hasContentDescription("Reviews")).performClick()
        waitFor(hasText("Batch-edit labels from the board", substring = true), SYNC_TIMEOUT)
        settle()
        Screengrab.screenshot("5_reviews")

        // --- My Work tab (EXP-58: Inbox + My Issues merged behind a
        // segmented control; Inbox is the default segment, seeded with 5
        // notifications, 3 unread). Wait for a real group row — capturing
        // "You're all caught up" would silently ship an empty screenshot.
        composeRule.onNode(hasContentDescription("My Work")).performClick()
        waitFor(hasText(SHOWCASE_ISSUE_TITLE, substring = true), SYNC_TIMEOUT)
        settle()
        Screengrab.screenshot("7_inbox")

        // --- Support inbox: the tab only exists because the seed flips the
        // team's helpdesk_enabled on; threads come from tRPC polling.
        composeRule.onNode(hasContentDescription("Support")).performClick()
        waitFor(hasTestTag("support-thread-row"), SYNC_TIMEOUT)
        settle()
        Screengrab.screenshot("8_support")
    }

    /** Poll (without requiring Compose idleness) until [matcher] matches a node. */
    private fun waitFor(matcher: SemanticsMatcher, timeoutMillis: Long) {
        composeRule.waitUntil(timeoutMillis) {
            composeRule.onAllNodes(matcher).fetchSemanticsNodes().isNotEmpty()
        }
    }

    /** Poll until no node matches [matcher] anymore. */
    private fun waitForGone(matcher: SemanticsMatcher, timeoutMillis: Long) {
        composeRule.waitUntil(timeoutMillis) {
            composeRule.onAllNodes(matcher).fetchSemanticsNodes().isEmpty()
        }
    }

    /** Let animations / async images finish before capturing. */
    private fun settle(longer: Boolean = false) {
        Thread.sleep(if (longer) 2_000 else 1_000)
    }
}
