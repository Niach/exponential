package com.exponential.app

import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.test.espresso.Espresso
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
 * STYLEGUIDE capture suite (EXP-566), driven by fastlane screengrab
 * (`bundle exec fastlane styleguide_screenshots` in apps/android — see
 * fastlane/Screengrabfile-styleguide).
 *
 * Where [StoreScreenshotsTest] spends a capped budget on the eight shots that
 * sell the product, this one photographs the app's ORDINARY surfaces as the
 * cross-platform design reference: sign-in, board filters, comments, create
 * issue, search, my issues, agents, reviews, a support thread and both
 * settings screens. The 11 shot names are a byte-exact contract shared with
 * the other clients — never rename one here alone.
 *
 * Prereqs are the store suite's MINUS live steering: no shot needs a relay or
 * an online desktop, only the seeded local backend
 * (apps/web/scripts/seed-screenshots.ts) reachable from the emulator (default
 * http://10.0.2.2:5173, override via the `instanceUrl` instrumentation
 * argument / SCREENGRAB_INSTANCE_URL).
 *
 * Every shot gates on genuinely seeded content rather than on a screen
 * merely existing, so a stale/missing seed fails the run instead of quietly
 * shipping an empty state. The sign-in / polling / settle machinery lives in
 * [ScreenshotFlow]; its KDoc carries the synchronization notes.
 */
@RunWith(AndroidJUnit4::class)
class StyleguideScreenshotsTest {

    companion object {
        // screengrab switches the device locale per the Screengrabfile
        // `locales` list; must be a @ClassRule so it wraps the activity launch.
        @ClassRule @JvmField
        val localeTestRule = LocaleTestRule()

        // APP-5 — the only seeded issue with a comment thread.
        private const val SHOWCASE_ISSUE_TITLE = ScreenshotFlow.SHOWCASE_ISSUE_TITLE

        // A fragment of APP-5's third comment (the demo user's own). Scrolling
        // to it lands the newest comments on screen; matching it also proves
        // the thread actually synced instead of photographing a bare header.
        private const val SHOWCASE_COMMENT_FRAGMENT = "Deferral PR is merged"

        // Multi-board search term: hits APP-4 / APP-7 / APP-14 / APP-15, so the
        // results list shows grouped board headers rather than a single row.
        private const val SEARCH_QUERY = "board"
        private const val SEARCH_RESULT_TITLE = "Batch-edit labels from the board"

        // APP-14: assigned to the demo user and, unlike APP-3 / APP-6, NOT
        // quoted by any seeded notification — so matching it proves the My
        // Issues segment is showing, not the Inbox one.
        private const val MY_ISSUE_TITLE = "Group board issues by assignee"

        // APP-15: an open PR in the Reviews queue.
        private const val REVIEW_ISSUE_TITLE = "Batch-edit labels from the board"

        // The most recently updated seeded support thread, and the last line of
        // its conversation.
        private const val SUPPORT_REPORTER = "Emma Fischer"
        private const val SUPPORT_MESSAGE_FRAGMENT = "thank you for the quick turnaround"

        private const val TEAM_NAME = "Acme"
        private const val TEAM_BOARD_NAME = "Mobile App"

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
    fun captureStyleguideScreenshots() {
        // --- Sign-in: point the app at the seeded backend, then photograph the
        // login screen BEFORE submitting. The password fields only render once
        // /api/auth-config resolved, so waiting on them is also the proof the
        // backend is reachable.
        flow.chooseInstance(instanceUrl)
        flow.awaitLoginScreen()
        flow.settle()
        Screengrab.screenshot("sg_sign-in")
        flow.submitLogin()

        // --- Board: wait out session fetch + first Electric sync, then let the
        // transient "Syncing…" pill clear (it photobombed a board shot once —
        // EXP-348).
        flow.waitFor(hasText(SHOWCASE_ISSUE_TITLE), SYNC_TIMEOUT)
        flow.waitForGone(hasText("Syncing", substring = true), SYNC_TIMEOUT)
        flow.settle()

        // --- Board filters: the badge button in the pinned nav row opens the
        // filter sheet. The trigger's "Filters" is a contentDescription and the
        // sheet's is a Text, so hasText matches the sheet unambiguously; gate
        // additionally on the three category rows.
        composeRule.onNode(hasContentDescription("Filters")).performClick()
        flow.waitFor(hasText("Filters"), NAV_TIMEOUT)
        flow.waitFor(hasText("Status"), NAV_TIMEOUT)
        flow.waitFor(hasText("Labels"), NAV_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_board-filters")
        // The sheet animates out over the board — let it finish before the next
        // tap, or it lands on the dismissing scrim.
        Espresso.pressBack()
        flow.waitForGone(hasText("Clear all"), NAV_TIMEOUT)
        flow.settle(longer = true)

        // --- Issue comments: APP-5 is the only seeded thread. The detail body
        // is a plain verticalScroll Column (not lazy), so every timeline row is
        // composed and performScrollTo reaches it. `comment-thread-header` is
        // the "Activity" label, the deliberate hook mirroring the iOS
        // accessibility id.
        composeRule.onAllNodes(hasText(SHOWCASE_ISSUE_TITLE)).onFirst().performClick()
        flow.waitFor(hasText("Startup profiling", substring = true), NAV_TIMEOUT)
        flow.waitFor(hasTestTag("comment-thread-header"), SYNC_TIMEOUT)
        flow.waitFor(hasText(SHOWCASE_COMMENT_FRAGMENT, substring = true), SYNC_TIMEOUT)
        composeRule.onNode(hasTestTag("comment-thread-header")).performScrollTo()
        composeRule.onAllNodes(hasText(SHOWCASE_COMMENT_FRAGMENT, substring = true))
            .onFirst()
            .performScrollTo()
        flow.settle()
        Screengrab.screenshot("sg_issue-comments")
        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.waitFor(hasContentDescription("Filters"), NAV_TIMEOUT)
        flow.settle()

        // --- Create issue: the compose circle on the bottom bar only exists
        // while a board is in view, and CreateIssueScreen dismisses via
        // "Cancel" (it has no "Back" node).
        composeRule.onNode(hasContentDescription("New issue")).performClick()
        flow.waitFor(hasTestTag("create-issue-title-field"), NAV_TIMEOUT)
        flow.waitFor(hasText("New Issue"), NAV_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_issue-create")
        composeRule.onNode(hasContentDescription("Cancel")).performClick()
        flow.waitFor(hasContentDescription("Filters"), NAV_TIMEOUT)
        flow.settle()

        // --- Search: the field is the only editable node on the screen, so
        // hasSetTextAction addresses it without a tag. Gate on a real hit —
        // an unseeded backend would render "No issues match".
        composeRule.onNode(hasContentDescription("Search")).performClick()
        flow.waitFor(hasText("Search"), NAV_TIMEOUT)
        composeRule.onNode(hasSetTextAction()).performTextInput(SEARCH_QUERY)
        Espresso.closeSoftKeyboard()
        flow.waitFor(hasText(SEARCH_RESULT_TITLE, substring = true), SYNC_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_search")

        // --- My Issues: the My Work tab opens on the Inbox segment (EXP-58);
        // the segmented control's label is the only handle on it.
        composeRule.onNode(hasContentDescription("My Work")).performClick()
        flow.waitFor(hasText("My Issues"), NAV_TIMEOUT)
        composeRule.onAllNodes(hasText("My Issues")).onFirst().performClick()
        flow.waitFor(hasText(MY_ISSUE_TITLE, substring = true), SYNC_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_my-issues")

        // --- Agents: the seeded coding_sessions rows sync over Electric, so
        // the list is populated with or without a steer relay.
        composeRule.onNode(hasContentDescription("Agents")).performClick()
        flow.waitFor(hasText("Agents"), NAV_TIMEOUT)
        flow.waitFor(hasTestTag("agent-session-row"), SYNC_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_agents")

        // --- Reviews: the cross-board open-PR queue; the seed leaves four open
        // PRs, of which APP-15 is the stable anchor.
        composeRule.onNode(hasContentDescription("Reviews")).performClick()
        flow.waitFor(hasText("Reviews"), NAV_TIMEOUT)
        flow.waitFor(hasText(REVIEW_ISSUE_TITLE, substring = true), SYNC_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_reviews")

        // --- Support thread: the tab only exists because the seed flips the
        // team's helpdesk_enabled on. Rows carry "<reporter> · <last message>"
        // as their subtitle, so the reporter name addresses Emma's thread
        // directly; fall back to the newest row (hers) if the subtitle is
        // elided on a narrow screen.
        composeRule.onNode(hasContentDescription("Support")).performClick()
        flow.waitFor(hasTestTag("support-thread-row"), SYNC_TIMEOUT)
        val reporterRow = hasText(SUPPORT_REPORTER, substring = true)
        if (flow.exists(reporterRow)) {
            composeRule.onAllNodes(reporterRow).onFirst().performClick()
        } else {
            composeRule.onAllNodes(hasTestTag("support-thread-row")).onFirst().performClick()
        }
        flow.waitFor(hasContentDescription("Ticket actions"), NAV_TIMEOUT)
        flow.waitFor(hasText(SUPPORT_MESSAGE_FRAGMENT, substring = true), SYNC_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_support-thread")
        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.settle()

        // --- Settings: the gear lives on the board root, not on a profile
        // menu, and team settings hang off the Teams section of the personal
        // settings screen.
        composeRule.onNode(hasContentDescription("Issues")).performClick()
        flow.waitFor(hasContentDescription("Settings"), NAV_TIMEOUT)
        composeRule.onNode(hasContentDescription("Settings")).performClick()
        flow.waitFor(hasText("Teams", ignoreCase = true), NAV_TIMEOUT)
        flow.waitFor(hasText(TEAM_NAME), SYNC_TIMEOUT)

        // Team settings first (it is one tap deeper), then back out for the
        // personal screen.
        composeRule.onAllNodes(hasText(TEAM_NAME)).onFirst().performClick()
        flow.waitFor(hasText("Boards", ignoreCase = true), NAV_TIMEOUT)
        flow.waitFor(hasText("Repositories", ignoreCase = true), NAV_TIMEOUT)
        flow.waitFor(hasText(TEAM_BOARD_NAME, substring = true), SYNC_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_settings-team")

        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.waitFor(hasText("Servers", ignoreCase = true), NAV_TIMEOUT)
        flow.waitFor(hasText("Teams", ignoreCase = true), NAV_TIMEOUT)
        flow.waitFor(hasText(TEAM_NAME), NAV_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_settings-personal")
    }
}
