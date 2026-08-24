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
 * cross-platform design reference: the instance picker, sign-in, the board
 * switcher and filters, comments, the properties sheet, create issue, search,
 * my issues, agents, the four Actions surfaces, reviews, a support thread and
 * the three settings screens. The `sg_*` names are a byte-exact contract
 * shared with the other clients — never rename one here alone, and never add
 * one without its paired shot in iOS StyleguideScreenshots.swift:
 *
 *   sg_instance-picker · sg_sign-in · sg_board-switcher · sg_board-filters ·
 *   sg_issue-comments · sg_issue-properties · sg_issue-create · sg_search ·
 *   sg_my-issues · sg_agents · sg_action-create · sg_automations-list ·
 *   sg_automations · sg_action-suggestions · sg_reviews · sg_support-thread ·
 *   sg_settings-root · sg_settings-team · sg_settings-account
 *
 * EXP-566 retired `sg_settings-personal`: it photographed this settings ROOT
 * while iOS photographed the server detail, so the pair compared two different
 * screens. It is now sg_settings-root plus sg_settings-account, and this suite
 * navigates into ServerDetailScreen for the latter.
 *
 * Prereqs are the store suite's MINUS live steering: no shot needs a relay or
 * an online desktop, only the seeded local backend
 * (apps/web/scripts/seed-screenshots.ts) reachable from the emulator (default
 * http://10.0.2.2:5173, override via the `instanceUrl` instrumentation
 * argument / SCREENGRAB_INSTANCE_URL).
 *
 * Every shot gates on genuinely seeded content rather than on a screen merely
 * existing, so a stale/missing seed fails the run instead of quietly shipping
 * an empty state. The two exceptions are called out where they happen
 * (sg_agents, and the automations pair, whose rows are newer than this suite).
 * The sign-in / polling / settle machinery lives in [ScreenshotFlow]; its
 * KDoc carries the synchronization notes.
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

        // One of the three seeded team actions, listed on the Actions segment.
        private const val SEEDED_ACTION_NAME = "Nightly test triage"

        private const val DEMO_EMAIL = ScreenshotFlow.DEMO_EMAIL

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
        // --- Instance picker: the Screengrabfile reinstalls the app, so the
        // cold launch always lands on the untouched chooser (cloud buttons +
        // the demoted self-hosted link). Photograph it BEFORE the flow reveals
        // the URL field.
        flow.awaitInstancePicker()
        flow.settle()
        Screengrab.screenshot("sg_instance-picker")

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

        // --- Board switcher: the board-name control in the pinned nav row
        // opens the server → team → board sheet. The trigger's "Switch board"
        // is a contentDescription and the sheet's is a Text, so hasText matches
        // the sheet unambiguously; gate additionally on the team block header.
        composeRule.onNode(hasContentDescription("Switch board")).performClick()
        flow.waitFor(hasText("Switch board"), NAV_TIMEOUT)
        flow.waitFor(hasText(TEAM_NAME), SYNC_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_board-switcher")
        Espresso.pressBack()
        flow.waitForGone(hasText("Switch board"), NAV_TIMEOUT)
        flow.settle(longer = true)

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

        // --- Issue properties: still on APP-5. The bottom bar's leading circle
        // opens the combined sheet (moderators only — the demo user owns the
        // team); its rows are the real content, the GlassSheet title alone
        // renders before they do.
        composeRule.onNode(hasContentDescription("Issue properties")).performClick()
        flow.waitFor(hasText("Properties"), NAV_TIMEOUT)
        flow.waitFor(hasText("Priority"), NAV_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_issue-properties")
        Espresso.pressBack()
        flow.waitForGone(hasText("Priority"), NAV_TIMEOUT)
        flow.settle(longer = true)

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

        // --- The Actions surface: four shots off one push. Actions has no tab
        // of its own (the bar is full at six), so the entry rides the Agents
        // header — the iOS twin rides the Agents toolbar. The segment is
        // rememberSaveable and the route is popped afterwards, so it always
        // opens on the Actions segment (iOS persists it in AppStorage and has
        // to re-select it).
        composeRule.onNode(hasTestTag("open-actions")).performClick()
        flow.waitFor(hasText(SEEDED_ACTION_NAME, substring = true), SYNC_TIMEOUT)

        // --- Create action: "New action" rides the "Actions · count" section
        // header (EXP-574). The sheet is only photographed, never submitted —
        // creating would start a real builtin run on somebody's machine.
        composeRule.onNode(hasTestTag("new-action")).performClick()
        flow.waitFor(hasTestTag("create-action-sheet"), NAV_TIMEOUT)
        flow.waitFor(hasText("What should this action do?", substring = true), NAV_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_action-create")
        composeRule.onAllNodes(hasText("Cancel")).onFirst().performClick()
        flow.waitForGone(hasTestTag("create-action-sheet"), NAV_TIMEOUT)
        flow.settle(longer = true)

        // --- Automations list: gated on the segment's OWN content rather than
        // on a seeded automation name — the `automations` rows are newer than
        // this suite, so an older seed shows the empty state, which is still a
        // legitimate capture of this surface (unlike a half-synced list).
        composeRule.onAllNodes(hasText("Automations")).onFirst().performClick()
        if (!flow.waitForOptional(hasTestTag("automation-row"), SYNC_TIMEOUT)) {
            android.util.Log.w(
                "EXP-566",
                "sg_automations-list: no automation rows — reseed with `bun run seed:screenshots`",
            )
            flow.waitFor(hasText("No automations yet.", substring = true), NAV_TIMEOUT)
        }
        flow.settle()
        Screengrab.screenshot("sg_automations-list")

        // --- Automation editor: the owner-only "New automation" entry, present
        // in both the populated header and the empty state. (iOS additionally
        // hides it when the backend has no STEER_RELAY_URL and falls back to
        // editing a seeded row; here it is owner-gated only.)
        composeRule.onAllNodes(hasTestTag("new-automation")).onFirst().performClick()
        flow.waitFor(hasTestTag("automation-form-sheet"), NAV_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_automations")
        composeRule.onAllNodes(hasText("Cancel")).onFirst().performClick()
        flow.waitForGone(hasTestTag("automation-form-sheet"), NAV_TIMEOUT)
        flow.settle(longer = true)

        // --- Suggestions: shipped constants (ACTION_SUGGESTIONS), not seeded
        // rows — this one can be gated hard on a card.
        composeRule.onAllNodes(hasText("Suggestions")).onFirst().performClick()
        flow.waitFor(hasTestTag("suggestion-row"), NAV_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_action-suggestions")
        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.waitFor(hasTestTag("open-actions"), NAV_TIMEOUT)
        flow.settle()

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

        // --- Settings root: the gear lives on the board root, not on a profile
        // menu. SectionHeader UPPERCASES its title, so every header match here
        // must pass ignoreCase.
        composeRule.onNode(hasContentDescription("Issues")).performClick()
        flow.waitFor(hasContentDescription("Settings"), NAV_TIMEOUT)
        composeRule.onNode(hasContentDescription("Settings")).performClick()
        flow.waitFor(hasText("Servers", ignoreCase = true), NAV_TIMEOUT)
        flow.waitFor(hasText("Teams", ignoreCase = true), NAV_TIMEOUT)
        flow.waitFor(hasText(TEAM_NAME), SYNC_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_settings-root")

        // --- Team settings: one tap deeper (the Teams section), then back out
        // for the account screen.
        composeRule.onAllNodes(hasText(TEAM_NAME)).onFirst().performClick()
        flow.waitFor(hasText("Boards", ignoreCase = true), NAV_TIMEOUT)
        flow.waitFor(hasText("Repositories", ignoreCase = true), NAV_TIMEOUT)
        flow.waitFor(hasText(TEAM_BOARD_NAME, substring = true), SYNC_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_settings-team")

        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.waitFor(hasText("Servers", ignoreCase = true), NAV_TIMEOUT)

        // --- Account settings: there is no separate profile screen (EXP-311) —
        // the signed-in identity, sign out and delete account live on the
        // server row's detail. The row is titled by the SERVER with the email
        // below, so the email addresses it; "Sign out" only exists there.
        composeRule.onAllNodes(hasText(DEMO_EMAIL, substring = true)).onFirst().performClick()
        flow.waitFor(hasText("Sign out"), NAV_TIMEOUT)
        flow.waitFor(hasText("Delete account", substring = true), NAV_TIMEOUT)
        flow.settle()
        Screengrab.screenshot("sg_settings-account")
    }
}
