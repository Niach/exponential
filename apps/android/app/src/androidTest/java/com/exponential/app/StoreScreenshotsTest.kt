package com.exponential.app

import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.rule.GrantPermissionRule
import org.junit.Before
import org.junit.ClassRule
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith
import tools.fastlane.screengrab.Screengrab
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
 * The sign-in flow, the polling helpers and the diff expansion live in
 * [ScreenshotFlow], shared with [StyleguideScreenshotsTest]; its KDoc carries
 * the synchronization notes.
 */
@RunWith(AndroidJUnit4::class)
class StoreScreenshotsTest {

    companion object {
        // screengrab switches the device locale per the Screengrabfile `locales`
        // list; must be a @ClassRule so it wraps the activity launch.
        @ClassRule @JvmField
        val localeTestRule = LocaleTestRule()

        private const val SHOWCASE_ISSUE_TITLE = ScreenshotFlow.SHOWCASE_ISSUE_TITLE

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

        private const val NAV_TIMEOUT = ScreenshotFlow.NAV_TIMEOUT
        private const val SYNC_TIMEOUT = ScreenshotFlow.SYNC_TIMEOUT
    }

    private val permissionRule: GrantPermissionRule = ScreenshotFlow.permissionRule()

    private val composeRule = createAndroidComposeRule<MainActivity>()

    // Permission grant must land before the activity (and its permission
    // request) launches.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(permissionRule).around(composeRule)

    private val flow by lazy { ScreenshotFlow(composeRule) }

    private val instanceUrl: String = ScreenshotFlow.instanceUrl()

    @Before
    fun setUp() {
        ScreenshotFlow.useUiAutomatorScreenshots()
    }

    @Test
    fun captureStoreScreenshots() {
        // --- Instance picker + login: the email field only shows once
        // /api/auth-config resolved.
        flow.signIn(instanceUrl)

        // --- Board: wait out session fetch + first Electric sync until the
        // showcase issue row is on screen (in-progress group sits at the top),
        // then until the transient "Syncing…" pill has cleared — it photobombed
        // the board shot once (EXP-348).
        flow.waitFor(hasText(SHOWCASE_ISSUE_TITLE), SYNC_TIMEOUT)
        flow.waitForGone(hasText("Syncing", substring = true), SYNC_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("1_board")

        // --- Issue detail: open APP-5 and wait for its markdown description.
        // The live session row above the thread is what makes this shot read
        // "an agent is coding on this right now" instead of "live steering is
        // unavailable on this instance".
        composeRule.onAllNodes(hasText(SHOWCASE_ISSUE_TITLE)).onFirst().performClick()
        flow.waitFor(hasText("Startup profiling", substring = true), NAV_TIMEOUT)
        flow.waitFor(hasText("Coding now", substring = true), SYNC_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("2_issue-detail")

        // --- Live steering: the session row's chevron is only clickable for
        // the session's own owner (EXP-312). Gate on the FEED tag, not the
        // screen: it shows "Connecting…" / "Waiting for activity…" placeholders
        // until the first relay frame lands.
        composeRule.onNode(hasContentDescription("Watch live")).performClick()
        flow.waitFor(hasTestTag("agent-feed"), SYNC_TIMEOUT)
        // An EMPTY feed still renders the container (a relay the emulator can't
        // reach leaves the view "Reconnecting…" with nothing in it), so the tag
        // alone would happily photograph a blank screen. Gate on real content:
        // from the emulator the relay has to be ws://10.0.2.2:4002, not
        // localhost — see the Screengrabfile prereqs.
        flow.waitFor(hasText(FEED_QUESTION_FRAGMENT, substring = true), SYNC_TIMEOUT)
        flow.settle(longer = true)
        Screengrab.screenshot("4_steering")
        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.settle()

        // --- Start-coding dialog: from a repo-backed issue the demo user is
        // NOT already coding on. Needs an online desktop — without one the
        // circle is disabled and the sheet never opens.
        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.waitFor(hasText(START_CODING_ISSUE_TITLE), NAV_TIMEOUT)
        composeRule.onAllNodes(hasText(START_CODING_ISSUE_TITLE)).onFirst().performClick()
        flow.waitFor(hasContentDescription("Start coding"), NAV_TIMEOUT)
        composeRule.onNode(hasContentDescription("Start coding")).performClick()
        flow.waitFor(hasTestTag("start-coding-sheet"), NAV_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("3_start-coding")
        composeRule.onAllNodes(hasText("Cancel")).onFirst().performClick()
        // The sheet animates out over the detail — let it finish before the
        // back press, or the tap lands on the dismissing scrim.
        flow.settle(longer = true)
        composeRule.onNode(hasContentDescription("Back")).performClick()

        // --- PR review: the Reviews rows open the Changes page directly. The
        // file list comes from GitHub via issues.prFiles — the seed points
        // APP-14 at a real public PR so there is an actual diff to show.
        flow.waitFor(hasContentDescription("Reviews"), NAV_TIMEOUT)
        composeRule.onNode(hasContentDescription("Reviews")).performClick()
        flow.waitFor(hasText(REVIEW_ISSUE_TITLE, substring = true), SYNC_TIMEOUT)
        composeRule.onAllNodes(hasText(REVIEW_ISSUE_TITLE, substring = true)).onFirst().performClick()
        flow.waitFor(hasTestTag("changes-file-row"), SYNC_TIMEOUT)
        flow.expandDiffFiles()
        flow.settle()
        Screengrab.screenshot("5_review")

        // --- Actions (EXP-253): the list rides the Agents header pill; the
        // seed inserts three team actions above the two client builtins.
        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.waitFor(hasContentDescription("Agents"), NAV_TIMEOUT)
        composeRule.onNode(hasContentDescription("Agents")).performClick()
        flow.waitFor(hasTestTag("open-actions"), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag("open-actions")).performClick()
        flow.waitFor(hasTestTag("action-row"), SYNC_TIMEOUT)
        flow.waitFor(hasText("Update dependencies", substring = true), SYNC_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("6_actions")
        composeRule.onNode(hasContentDescription("Back")).performClick()

        // --- My Work tab (EXP-58: Inbox + My Issues merged behind a
        // segmented control; Inbox is the default segment, seeded with 5
        // notifications, 3 unread). Wait for a real group row — capturing
        // "You're all caught up" would silently ship an empty screenshot.
        composeRule.onNode(hasContentDescription("My Work")).performClick()
        flow.waitFor(hasText(SHOWCASE_ISSUE_TITLE, substring = true), SYNC_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("7_inbox")

        // --- Support inbox: the tab only exists because the seed flips the
        // team's helpdesk_enabled on; threads come from tRPC polling.
        composeRule.onNode(hasContentDescription("Support")).performClick()
        flow.waitFor(hasTestTag("support-thread-row"), SYNC_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("8_support")
    }
}
