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
 * board → issue detail → live steering → Start-coding dialog → PR review →
 * actions → inbox → support inbox. Play caps phone screenshots at 8; EXP-393
 * spent the budget on the surfaces that actually differentiate the product,
 * dropping the comments / agents-list / reviews-list shots for start coding,
 * steering and the diff + merge bar (the iOS set is identical).
 *
 * The backend must be reachable from the emulator (default
 * http://10.0.2.2:5173, override via the `instanceUrl` instrumentation
 * argument / SCREENGRAB_INSTANCE_URL), and it needs live steering genuinely
 * switched on — a steer relay with STEER_RELAY_URL + STEER_RELAY_SECRET
 * exported for the web server (`docker compose --profile steer up -d`) plus
 * `bun run screenshots:desktop` left running for the whole capture. Without
 * them the steering and Start-coding shots are unreachable and the issue
 * detail renders "Live steering is unavailable on this instance."
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

        // APP-3: repo-backed, member-assigned, and deliberately WITHOUT a
        // coding session of its own — on an issue the demo user is already
        // coding, the bottom-bar circle links to that session instead of
        // offering the start action.
        private const val START_CODING_ISSUE_TITLE = "Dark mode contrast pass across settings"

        // APP-14: the open PR whose diff is fetched from GitHub for real.
        private const val REVIEW_ISSUE_TITLE = "Group board issues by assignee"

        // A fragment of the question screenshot-desktop.ts ends its transcript
        // on (DEMO_FEED_QUESTION) — proof that relay frames actually arrived.
        // It has to be the LAST event: the feed is a bottom-anchored lazy list,
        // so anything above the fold is never composed and no matcher sees it.
        private const val FEED_QUESTION_FRAGMENT = "Lazy-load the markdown editor too"

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
        // The live session row above the thread is what makes this shot read
        // "an agent is coding on this right now" instead of "live steering is
        // unavailable on this instance".
        composeRule.onAllNodes(hasText(SHOWCASE_ISSUE_TITLE)).onFirst().performClick()
        waitFor(hasText("Startup profiling", substring = true), NAV_TIMEOUT)
        waitFor(hasText("Coding now", substring = true), SYNC_TIMEOUT)
        settle()
        Screengrab.screenshot("2_issue-detail")

        // --- Live steering: the session row's chevron is only clickable for
        // the session's own owner (EXP-312). Gate on the FEED tag, not the
        // screen: it shows "Connecting…" / "Waiting for activity…" placeholders
        // until the first relay frame lands.
        composeRule.onNode(hasContentDescription("Watch live")).performClick()
        waitFor(hasTestTag("agent-feed"), SYNC_TIMEOUT)
        // An EMPTY feed still renders the container (a relay the emulator can't
        // reach leaves the view "Reconnecting…" with nothing in it), so the tag
        // alone would happily photograph a blank screen. Gate on real content:
        // from the emulator the relay has to be ws://10.0.2.2:4002, not
        // localhost — see the Screengrabfile prereqs.
        waitFor(hasText(FEED_QUESTION_FRAGMENT, substring = true), SYNC_TIMEOUT)
        settle(longer = true)
        Screengrab.screenshot("4_steering")
        composeRule.onNode(hasContentDescription("Back")).performClick()
        settle()

        // --- Start-coding dialog: from a repo-backed issue the demo user is
        // NOT already coding on. Needs an online desktop — without one the
        // circle is disabled and the sheet never opens.
        composeRule.onNode(hasContentDescription("Back")).performClick()
        waitFor(hasText(START_CODING_ISSUE_TITLE), NAV_TIMEOUT)
        composeRule.onAllNodes(hasText(START_CODING_ISSUE_TITLE)).onFirst().performClick()
        waitFor(hasContentDescription("Start coding"), NAV_TIMEOUT)
        composeRule.onNode(hasContentDescription("Start coding")).performClick()
        waitFor(hasTestTag("start-coding-sheet"), NAV_TIMEOUT)
        settle()
        Screengrab.screenshot("3_start-coding")
        composeRule.onAllNodes(hasText("Cancel")).onFirst().performClick()
        // The sheet animates out over the detail — let it finish before the
        // back press, or the tap lands on the dismissing scrim.
        settle(longer = true)
        composeRule.onNode(hasContentDescription("Back")).performClick()

        // --- PR review: the Reviews rows open the Changes page directly. The
        // file list comes from GitHub via issues.prFiles — the seed points
        // APP-14 at a real public PR so there is an actual diff to show.
        waitFor(hasContentDescription("Reviews"), NAV_TIMEOUT)
        composeRule.onNode(hasContentDescription("Reviews")).performClick()
        waitFor(hasText(REVIEW_ISSUE_TITLE, substring = true), SYNC_TIMEOUT)
        composeRule.onAllNodes(hasText(REVIEW_ISSUE_TITLE, substring = true)).onFirst().performClick()
        waitFor(hasTestTag("changes-file-row"), SYNC_TIMEOUT)
        expandDiffFiles()
        settle()
        Screengrab.screenshot("5_review")

        // --- Actions (EXP-253): the list rides the Agents header pill; the
        // seed inserts three team actions above the two client builtins.
        composeRule.onNode(hasContentDescription("Back")).performClick()
        waitFor(hasContentDescription("Agents"), NAV_TIMEOUT)
        composeRule.onNode(hasContentDescription("Agents")).performClick()
        waitFor(hasTestTag("open-actions"), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag("open-actions")).performClick()
        waitFor(hasTestTag("action-row"), SYNC_TIMEOUT)
        waitFor(hasText("Update dependencies", substring = true), SYNC_TIMEOUT)
        settle()
        Screengrab.screenshot("6_actions")
        composeRule.onNode(hasContentDescription("Back")).performClick()

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

    /**
     * Expands the diff's file sections so the review shot shows patches rather
     * than a bare filename list. Every file starts collapsed; the rows never
     * reorder, so index-addressing them is stable across the clicks.
     */
    private fun expandDiffFiles() {
        val rows = composeRule.onAllNodes(hasTestTag("changes-file-row"))
        val count = minOf(rows.fetchSemanticsNodes().size, 5)
        for (index in 0 until count) {
            runCatching { rows[index].performClick() }
        }
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
