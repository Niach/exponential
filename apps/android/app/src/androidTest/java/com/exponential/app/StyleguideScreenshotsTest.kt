package com.exponential.app

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeUp
import androidx.test.espresso.Espresso
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.rule.GrantPermissionRule
import com.exponential.app.domain.AgentHome
import com.exponential.app.domain.SearchList
import androidx.activity.compose.setContent
import com.exponential.app.ui.gettingstarted.GettingStartedCopy
import com.exponential.app.ui.gettingstarted.GettingStartedStepKey
import com.exponential.app.ui.styleguide.FeedbackStatesScreen
import com.exponential.app.ui.styleguide.FeedbackStatesScreenTestId
import com.exponential.app.ui.styleguide.ThreadCardsKind
import com.exponential.app.ui.styleguide.ThreadCardsScreen
import com.exponential.app.ui.theme.ExponentialTheme
import com.exponential.app.domain.IssueDraftPage
import com.exponential.app.ui.onboarding.OnboardingTestHooks
import androidx.compose.runtime.Composable
import com.exponential.app.ui.styleguide.AgentsCaptureKind
import com.exponential.app.ui.styleguide.AgentsCaptures
import com.exponential.app.ui.styleguide.HomeCaptureKind
import com.exponential.app.ui.styleguide.HomeCaptures
import com.exponential.app.ui.styleguide.SessionCaptureKind
import com.exponential.app.ui.styleguide.SessionCaptures
import com.exponential.app.ui.styleguide.ThreadCaptureKind
import com.exponential.app.ui.styleguide.ThreadCaptures
import com.exponential.app.ui.styleguide.ThreadsCaptureKind
import com.exponential.app.ui.styleguide.ThreadsCaptures
import org.junit.After
import org.junit.Assume.assumeFalse
import org.junit.Assume.assumeTrue
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.ClassRule
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith
import tools.fastlane.screengrab.locale.LocaleTestRule

/**
 * STYLEGUIDE capture suite (EXP-566), driven by fastlane screengrab
 * (`bundle exec fastlane styleguide_screenshots` in apps/android — see
 * fastlane/Screengrabfile-styleguide).
 *
 * Where [StoreScreenshotsTest] spends a capped budget on the eight shots that
 * sell the product, this one photographs the app's ORDINARY surfaces as the
 * cross-platform design reference. The `sg_*` names are a byte-exact contract
 * shared with the other clients — never rename one here alone, and never add
 * one without its paired shot in iOS StyleguideScreenshots.swift
 * (`packages/view-catalog/src/views.test.ts` gates both directions AND requires
 * the two platforms' `sg_*` sets to be identical):
 *
 *   sg_sign-in · sg_board-switcher · sg_onboarding-create-team ·
 *   sg_board-empty ·
 *   sg_board-bulk-edit · sg_issue-comments · sg_issue-properties ·
 *   sg_issue-create · sg_search · sg_my-issues · sg_agents ·
 *   sg_chat · sg_composer-menu · sg_chat-issues ·
 *   sg_machine-settings · sg_threads ·
 *   sg_session-row · sg_pr-row · sg_guide · sg_guide-section ·
 *   sg_settings-root · sg_settings-team ·
 *   sg_settings-account · sg_onboarding · sg_onboarding-invite ·
 *   sg_onboarding-devices · sg_agent-home · sg_search-list ·
 *   sg_getting-started · sg_feedback-states · sg_work-card ·
 *   sg_script-card · sg_comment-call-card · sg_thread · sg_agent-channel
 *
 * EXP-1295 retired `sg_chat-action` and `sg_action-create` (the builtin
 * actions and the action create sheet left the catalog), renamed the
 * Threads tab `sg_threads` and the agent channel `sg_agent-channel`;
 * `sg_pr-row` = the issue page's pull-request row, `sg_guide` /
 * `sg_guide-section` = the Guide tab of the thread holding APP-14.
 *
 * EXP-1296 (Threads v2) split the suite in two tests. [captureStyleguideScreenshots]
 * walks the live app against the seeded backend; [captureOnlyScreens] needs NO
 * server and NO sign-in: each screen of the session-tree model is drawn by its
 * real composables over fixture data (the ui/styleguide Captures files) and set as
 * the activity's content — sg_agent-home, sg_chat-issues,
 * sg_composer-menu, sg_threads (the Threads tab), sg_thread, sg_agent-channel
 * (an agent's channel), sg_session-row (the session cards),
 * the feedback states and the thread cards. It also writes Guide-only
 * extras under NON-catalog names (`x_*`, never imported) through [extra].
 * A scoped run (`shots:`) naming only capture shots skips the live walk.
 *
 * EXP-1259 added the last four (iOS parity): the Agent HOME the app lands on
 * (the greeting, the composer, Recent), the `#` typeahead's search-list
 * sheet, Getting started's block (the Settings entry's sheet) and the
 * Feedback states screen ([com.exponential.app.ui.styleguide.FeedbackStatesScreen],
 * set as the activity's content at the very end of the walk). EXP-1270
 * retired sg_action-suggestions with the seeds.
 *
 * EXP-642 reshuffled the front of the set: the old `sg_instance-picker` shot IS
 * the cloud chooser a first-run user meets, so it took over the `sg_sign-in`
 * name, and the password-form shot that used to carry it is gone (the form is a
 * self-hosting detail, not the sign-in surface). EXP-1267: the suite signs in
 * with a reused session ([ScreenshotSession]); the login flow is the fallback. EXP-566 had earlier retired
 * `sg_settings-personal` in favour of sg_settings-root + sg_settings-account.
 *
 * Prereqs: the seeded local backend (apps/web/scripts/seed-screenshots.ts)
 * reachable from the emulator (default http://10.0.2.2:5173, override via the
 * `instanceUrl` instrumentation argument / SCREENGRAB_INSTANCE_URL) PLUS, since
 * EXP-642, the relay stub: `bun run screenshots:desktop` (apps/web) registers
 * the demo user's OWN device row, which is what sg_machine-settings (gated
 * `isMine && registered`) and the three sg_chat* shots photograph (EXP-825: the
 * Agent page composer, the ONE launcher), and whose heartbeat announces the
 * agent accounts sg_agents photographs under its device rows (EXP-829,
 * folded into the machines by EXP-909). No steer RELAY traffic is
 * needed beyond that registration — nothing here watches a live session.
 *
 * Every shot gates on genuinely seeded content rather than on a screen merely
 * existing, so a stale/missing seed fails the run instead of quietly shipping
 * an empty state. The sign-in / polling / settle /
 * capture machinery lives in [ScreenshotFlow]; its KDoc carries the
 * synchronization notes, and [ScreenshotFlow.screenshot] honours the lane's
 * optional `shots` allowlist.
 */
@RunWith(AndroidJUnit4::class)
class StyleguideScreenshotsTest {

    companion object {
        // screengrab switches the device locale per the Screengrabfile
        // `locales` list; must be a @ClassRule so it wraps the activity launch.
        @ClassRule @JvmField
        val localeTestRule = LocaleTestRule()

        /** The catalog shots [captureOnlyScreens] owns (no server, no sign-in). */
        private val CAPTURE_SHOTS = setOf(
            "sg_agent-home", "sg_chat-issues", "sg_composer-menu",
            "sg_threads", "sg_thread", "sg_agent-channel", "sg_session-row",
            "sg_feedback-states", "sg_work-card",
            "sg_script-card", "sg_comment-call-card", "sg_settings-team-agent",
        )

        /** Guide-only extras: NOT catalog views (the importer ignores them). */
        private val EXTRA_SHOTS = setOf(
            "x_session-view", "x_issue-start", "x_agents-list",
        )

        // APP-5 — the only seeded issue with a comment thread.
        private const val SHOWCASE_ISSUE_TITLE = ScreenshotFlow.SHOWCASE_ISSUE_TITLE

        // A fragment of APP-5's third comment (the demo user's own). Scrolling
        // to it lands the newest comments on screen; matching it also proves
        // the thread actually synced instead of photographing a bare header.
        private const val SHOWCASE_COMMENT_FRAGMENT = "Deferral PR is merged"

        // EXP-642: the same query the web and desktop `search` recipes type, so
        // the three shots of that view compare like for like.
        private const val SEARCH_QUERY = "cold start"
        private const val SEARCH_RESULT_TITLE = SHOWCASE_ISSUE_TITLE

        // APP-14: assigned to the demo user and, unlike APP-3 / APP-6, NOT
        // quoted by any seeded notification — so matching it proves the My
        // Issues segment is showing, not the Inbox one.
        private const val MY_ISSUE_TITLE = "Group board issues by assignee"

        // APP-14: the seed's realPr issue, so its thread's Guide has files
        // from GitHub. Addressed by row tag so the lazy board list can
        // scroll to it.
        private const val GUIDE_ISSUE_ROW = "issue-row-APP-14"

        private const val TEAM_NAME = "Acme"
        private const val TEAM_BOARD_NAME = "Mobile App"

        // The seed's SECOND board: created empty on purpose, so the "no issues
        // yet" state is photographable without deleting anything (EXP-642).
        private const val EMPTY_BOARD_NAME = "Launch Marketing"

        // Two backlog issues — the bulk-edit selection. Addressed by row tag so
        // the lazy list can scroll to them (their titles are far down the list).
        private const val BULK_FIRST_ROW = "issue-row-APP-11"
        private const val BULK_SECOND_ROW = "issue-row-APP-13"

        // One of the three seeded team actions, listed on the Actions segment.
        private const val SEEDED_ACTION_NAME = "Nightly test triage"

        // EXP-825: the two codeable issues the sg_chat-issues shot chips —
        // APP-3 and APP-6, the same pair the web `chat-issues` recipe seeds
        // (apps/web/scripts/lib/demo-ids.ts), neither with a running session.
        private const val CHAT_FIRST_ISSUE_TITLE = "Dark mode contrast pass across settings"
        private const val CHAT_SECOND_ISSUE_TITLE = "Push notification deep links open the wrong tab"
        // The close-out the seed's freshest agent-ended run carries (EXP-637) —
        // only the EXPANDED row shows it, so it is the post-tap gate.

        // The device `bun run screenshots:desktop` registers for the demo user.
        private const val DEMO_DEVICE_NAME = "Alex's MacBook Pro"

        private const val DEMO_EMAIL = ScreenshotFlow.DEMO_EMAIL

        private const val NAV_TIMEOUT = ScreenshotFlow.NAV_TIMEOUT
        private const val SYNC_TIMEOUT = ScreenshotFlow.SYNC_TIMEOUT
        private const val FIRST_SYNC_TIMEOUT = ScreenshotFlow.FIRST_SYNC_TIMEOUT
        private const val NETWORK_TIMEOUT = ScreenshotFlow.NETWORK_TIMEOUT
    }

    private val permissionRule: GrantPermissionRule = ScreenshotFlow.permissionRule()

    private val composeRule = createAndroidComposeRule<MainActivity>()

    // EXP-1267: no account at launch (the untouched instance picker for
    // sg_sign-in) — the app is no longer reinstalled between runs.
    private val sessionRule = ScreenshotSessionRule(email = null)

    // Permission grant and the signed-out state must land before the activity
    // (and its permission request) launches.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(permissionRule).around(sessionRule).around(composeRule)

    private val flow by lazy { ScreenshotFlow(composeRule) }

    private val instanceUrl: String = ScreenshotFlow.instanceUrl()

    /** Set at the very end of the walk — see [assertRequestedShotsWereReached]. */
    private var finished = false

    /** The ids this test method owns; the typo check only looks at those. */
    private var ownsShot: (String) -> Boolean = { true }

    @Before
    fun setUp() {
        ScreenshotFlow.useUiAutomatorScreenshots()
    }

    /**
     * Fails the run when a `shots` id was never reached — almost always a typo,
     * which would otherwise look like a perfectly green empty run. Guarded on
     * the walk having completed so an earlier failure is not buried under a
     * second one.
     */
    @After
    fun assertRequestedShotsWereReached() {
        if (!finished) return
        val missing = ScreenshotFlow.unreachedShots().filter(ownsShot)
        assertTrue(
            "EXP-642 shots: ${missing.joinToString(", ")} — no such shot in this suite",
            missing.isEmpty(),
        )
    }

    @Test
    fun captureStyleguideScreenshots() {
        // The capture-only shots are [captureOnlyScreens]' — a run scoped to
        // them alone needs no backend, so the live walk steps aside.
        ownsShot = { it !in CAPTURE_SHOTS && it !in EXTRA_SHOTS }
        assumeFalse(ScreenshotFlow.requestedOnlyAmong(CAPTURE_SHOTS + EXTRA_SHOTS))
        // --- sg_sign-in: the pre-login server chooser. [sessionRule] drops
        // every account before launch (EXP-1267: no reinstall any more), so the
        // cold launch always lands on the untouched chooser (cloud buttons +
        // the demoted self-hosted link). Photograph it BEFORE anything signs
        // in — it is the same surface the web and desktop `sign-in` shots show.
        flow.awaitInstancePicker()
        flow.settle()
        flow.screenshot("sg_sign-in")

        // The password form is deliberately NOT photographed any more
        // (EXP-642). EXP-1267: the demo session (reused across runs) is
        // signed in in-process and the open picker follows it into the app;
        // the login form only as the fallback.
        if (!ScreenshotSession.signIn(instanceUrl, DEMO_EMAIL, ScreenshotFlow.DEMO_PASSWORD)) {
            flow.chooseInstance(instanceUrl)
            flow.awaitLoginScreen()
            flow.submitLogin()
        }

        // The app lands on the Agent page (EXP-1253); sg_agent-home is the
        // capture-only screen now ([captureOnlyScreens]), this only gates the
        // first sync.
        flow.waitFor(hasTestTag(AgentHome.HOME_TEST_ID), FIRST_SYNC_TIMEOUT)
        flow.settle()

        // The app lands on the Agent tab; the board shots start on Issues.
        flow.waitFor(hasTestTag("tab-issues"), SYNC_TIMEOUT)
        composeRule.onNode(hasTestTag("tab-issues")).performClick()

        // --- Board: wait out session fetch + first Electric sync, then let the
        // transient "Syncing…" pill clear (it photobombed a board shot once —
        // EXP-348).
        flow.waitFor(hasText(SHOWCASE_ISSUE_TITLE), FIRST_SYNC_TIMEOUT)
        flow.waitForGone(hasText("Syncing", substring = true), FIRST_SYNC_TIMEOUT)
        flow.settle()

        // --- Board switcher: the board-name control in the pinned nav row
        // opens the server → team → board sheet. The trigger's "Switch board"
        // is a contentDescription and the sheet's is a Text, so hasText matches
        // the sheet unambiguously; gate additionally on the team block header.
        composeRule.onNode(hasContentDescription("Switch board")).performClick()
        flow.waitFor(hasText("Switch board"), NAV_TIMEOUT)
        flow.waitFor(hasText(TEAM_NAME), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_board-switcher")

        // --- Set up a team: the switcher's own "New team" row (EXP-698 r5).
        // The row CLOSES the switcher and opens the create-or-join sheet — two
        // stacked bottom sheets is a dead end on Android — so this waits on the
        // new sheet's tag rather than on the switcher going away, and it
        // dismisses the switcher on the way for free. The extra wait on the
        // first card's title keeps the shot off a half-drawn sheet.
        composeRule.onNode(hasTestTag("board-switcher-new-team")).performClick()
        flow.waitFor(hasTestTag("team-setup-sheet"), NAV_TIMEOUT)
        // P3: the sheet opens on the choice page; "Create a team" pushes the
        // create page, which is what this view photographs.
        flow.waitFor(hasTestTag("team-setup-create"), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag("team-setup-create")).performClick()
        flow.waitFor(hasText("Create team"), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_onboarding-create-team")
        // EXP-687: sheets carry no Cancel pill — back (like a swipe down)
        // dismisses, and an unsubmitted form leaves no team behind.
        Espresso.pressBack()
        flow.waitForGone(hasTestTag("team-setup-sheet"), NAV_TIMEOUT)
        flow.waitForGone(hasText("Switch board"), NAV_TIMEOUT)
        flow.settle(longer = true)

        // --- Board empty state: the seed's second board is created empty for
        // exactly this shot, so nothing has to be deleted to reach the state.
        switchBoard(EMPTY_BOARD_NAME)
        flow.waitFor(hasText("No issues yet"), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_board-empty")
        switchBoard(TEAM_BOARD_NAME)
        flow.waitFor(hasText(SHOWCASE_ISSUE_TITLE), SYNC_TIMEOUT)
        flow.settle()

        // --- Bulk edit: a long press enters multi-select, a plain tap on a
        // second row adds it. Both rows sit in the backlog group at the bottom
        // of a LAZY list, so scroll the list to each by row tag first — an
        // off-screen lazy row is not composed and has no node to address.
        composeRule.onNode(hasTestTag("issue-list"))
            .performScrollToNode(hasTestTag(BULK_FIRST_ROW))
        composeRule.onNode(hasTestTag(BULK_FIRST_ROW)).performTouchInput { longClick() }
        flow.waitFor(hasTestTag("bulk-selection-bar"), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag("issue-list"))
            .performScrollToNode(hasTestTag(BULK_SECOND_ROW))
        composeRule.onNode(hasTestTag(BULK_SECOND_ROW)).performClick()
        flow.settle()
        flow.screenshot("sg_board-bulk-edit")
        // Leave selection mode — every later shot assumes the plain list.
        composeRule.onNode(hasContentDescription("Clear selection")).performClick()
        flow.waitForGone(hasTestTag("bulk-selection-bar"), NAV_TIMEOUT)
        flow.settle()

        // --- Issue comments: APP-5 is the only seeded thread. The detail body
        // is a plain verticalScroll Column (not lazy), so every timeline row is
        // composed and performScrollTo reaches it. `comment-thread-header` is
        // the "Activity" label, the deliberate hook mirroring the iOS
        // accessibility id.
        composeRule.onAllNodes(hasText(SHOWCASE_ISSUE_TITLE)).onFirst().performClick()
        flow.waitFor(hasText("Startup profiling", substring = true), NAV_TIMEOUT)
        // The face strip as the store lane's 2_issue-detail shows it: the
        // live run's Run tab, then the Guide tab its diff brings (EXP-875;
        // the screen dials the diff from the Issue face too).
        flow.waitFor(hasTestTag("work-face-run"), SYNC_TIMEOUT)
        flow.waitForOptional(hasTestTag("work-face-guide"), SYNC_TIMEOUT)
        flow.waitFor(hasTestTag("comment-thread-header"), SYNC_TIMEOUT)
        flow.waitFor(hasText(SHOWCASE_COMMENT_FRAGMENT, substring = true), SYNC_TIMEOUT)
        composeRule.onNode(hasTestTag("comment-thread-header")).performScrollTo()
        composeRule.onAllNodes(hasText(SHOWCASE_COMMENT_FRAGMENT, substring = true))
            .onFirst()
            .performScrollTo()
        flow.settle()
        flow.screenshot("sg_issue-comments")

        // --- Issue properties: still on APP-5. The bottom bar's leading circle
        // opens the combined sheet (moderators only — the demo user owns the
        // team); its rows are the real content, the GlassSheet title alone
        // renders before they do.
        composeRule.onNode(hasContentDescription("Issue properties")).performClick()
        flow.waitFor(hasText("Properties"), NAV_TIMEOUT)
        flow.waitFor(hasText("Priority"), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_issue-properties")
        Espresso.pressBack()
        flow.waitForGone(hasText("Priority"), NAV_TIMEOUT)
        flow.settle(longer = true)

        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.waitFor(hasContentDescription("Switch board"), NAV_TIMEOUT)
        flow.settle()

        // --- Create issue: the compose circle opens the New issue PAGE
        // (EXP-1170), left via its header's "Back". This shot types NOTHING
        // into it, so leaving writes no draft row.
        composeRule.onNode(hasContentDescription("New issue")).performClick()
        flow.waitFor(hasTestTag("create-issue-title-field"), NAV_TIMEOUT)
        flow.waitFor(hasText(IssueDraftPage.HEADER), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_issue-create")
        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.waitFor(hasContentDescription("Switch board"), NAV_TIMEOUT)
        flow.settle()

        // --- Search: EXP-686 moved it off the bottom bar into the board
        // header, so it is a PUSHED screen now — tagged,
        // because "Search" also reads as the field's own placeholder. The
        // field is the only editable node on it, so hasSetTextAction
        // addresses it without a tag. Gate on a real hit — an unseeded
        // backend would render "No issues match".
        composeRule.onNode(hasTestTag("board-search")).performClick()
        flow.waitFor(hasText("Search"), NAV_TIMEOUT)
        composeRule.onNode(hasSetTextAction()).performTextInput(SEARCH_QUERY)
        Espresso.closeSoftKeyboard()
        flow.waitFor(hasText(SEARCH_RESULT_TITLE, substring = true), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_search")
        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.waitFor(hasContentDescription("Switch board"), NAV_TIMEOUT)
        flow.settle()

        // --- sg_pr-row: THE pull-request row (EXP-1248/1291). An issue page
        // lists its pull requests as rows; APP-14 owns the seed's real open
        // PR. The detail body is a plain verticalScroll Column, so
        // performScrollTo reaches the row.
        composeRule.onNode(hasTestTag("issue-list"))
            .performScrollToNode(hasTestTag(GUIDE_ISSUE_ROW))
        composeRule.onNode(hasTestTag(GUIDE_ISSUE_ROW)).performClick()
        flow.waitFor(hasTestTag("issue-pr-row"), SYNC_TIMEOUT)
        composeRule.onAllNodes(hasTestTag("issue-pr-row")).onFirst().performScrollTo()
        flow.settle()
        flow.screenshot("sg_pr-row")

        // --- sg_guide / sg_guide-section: the thread's Guide (EXP-1251/1295).
        // The issue's Threads row opens the thread holding it; its strip's
        // Guide tab: APP-14's real PR has no report, so its whole diff is ONE
        // Changes section; that row opens the section page. A detour no later
        // shot needs (it pops back to the issue), so a scoped run that did
        // not ask for it skips it whole — its GitHub fetch is the walk's one
        // network dependency.
        val wantsGuide = ScreenshotFlow.isShotWanted("sg_guide")
        val wantsGuideSection = ScreenshotFlow.isShotWanted("sg_guide-section")
        if (wantsGuide || wantsGuideSection) {
            flow.waitFor(hasTestTag("issue-thread-chip"), SYNC_TIMEOUT)
            composeRule.onAllNodes(hasTestTag("issue-thread-chip")).onFirst().performScrollTo()
            composeRule.onAllNodes(hasTestTag("issue-thread-chip")).onFirst().performClick()
            flow.waitFor(hasTestTag("thread-tab-guide"), NAV_TIMEOUT)
            composeRule.onNode(hasTestTag("thread-tab-guide")).performClick()
            flow.waitFor(hasTestTag("guide-changes-row"), NETWORK_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_guide")
            // At rest the Changes row can sit under the floating Merge capsule
            // (APP-14 is a stack member: `Merge stack`), and a centre click
            // would land on the capsule. Swipe the Guide to its end, where the
            // bottom inset lifts every row clear of the bar.
            composeRule.onNode(hasTestTag("work-guide")).performTouchInput { swipeUp() }
            flow.settle()
            composeRule.onAllNodes(hasTestTag("guide-changes-row")).onFirst().performClick()
            flow.waitFor(hasTestTag("guide-section"), NAV_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_guide-section")
            composeRule.onNode(hasTestTag("guide-section-back")).performClick()
            flow.waitFor(hasTestTag("guide-changes-row"), NAV_TIMEOUT)
            // Thread → the issue page.
            Espresso.pressBack()
            flow.waitForGone(hasTestTag("thread-tabs"), NAV_TIMEOUT)
            flow.waitFor(hasTestTag("issue-pr-row"), NAV_TIMEOUT)
            flow.settle()
        }
        // Issue → board.
        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.waitFor(hasContentDescription("Switch board"), NAV_TIMEOUT)
        flow.settle()

        // --- My Issues: the Inbox tab opens on the Inbox segment (EXP-58);
        // the segmented control's label is the only handle on it.
        composeRule.onNode(hasTestTag("tab-mywork")).performClick()
        flow.waitFor(hasText("My issues"), NAV_TIMEOUT)
        composeRule.onAllNodes(hasText("My issues")).onFirst().performClick()
        flow.waitFor(hasText(MY_ISSUE_TITLE, substring = true), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_my-issues")

        // --- Devices (EXP-686, the renamed Agents surface): since EXP-642
        // this lane needs the relay stub (`screenshots:desktop`) — the demo
        // user's own device row is what the next three shots are taken from,
        // and an empty machines list is not a useful reference shot either.
        // EXP-909 retired `sg_usage` with the cross-device Accounts section:
        // each device row now lists its OWN logins, so `sg_agents` is the
        // accounts shot too.
        composeRule.onNode(hasTestTag("tab-devices")).performClick()
        flow.waitFor(hasText("Devices"), NAV_TIMEOUT)
        flow.waitFor(hasText(DEMO_DEVICE_NAME, substring = true), NETWORK_TIMEOUT)
        // EXP-944: device rows are collapsed by default; the shot opens the
        // demo device so its logins and their usage are in frame.
        composeRule.onAllNodes(hasText(DEMO_DEVICE_NAME, substring = true)).onFirst().performClick()
        flow.waitFor(hasTestTag("device-login-row"), NETWORK_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_agents")

        // --- sg_chat / sg_chat-issues / sg_chat-action: the Agent page
        // (EXP-825, the ONE launcher). The bottom bar's Chat arm switches to it
        // on the default machine (the EXP-909 follow-up retired the device
        // row's play glyph — a device list starts nothing): an empty composer
        // is a chat; the `#` tool checks issues (two chips, a batch); the ▶
        // tool picks the builtin, which SWAPS the chips for one action chip.
        // Nothing is ever submitted — a run would land on a real machine.
        // Every step gates on the state it produced (a chip, a label), never on
        // the page alone, so a swallowed tap fails the run instead of
        // duplicating a shot.
        composeRule.onNode(hasTestTag("chat-button")).performClick()
        flow.waitFor(hasTestTag("agent-composer"), NAV_TIMEOUT)
        // EXP-827: the submit is the round send glyph, so its contract label
        // ("Start chat") is its NAME — still the proof that the composer
        // resolved its subject.
        flow.waitFor(hasContentDescription("Start chat"), NAV_TIMEOUT)
        // The machine pool resolves after the page; the options pills are
        // the demo machine's, so the shot must not precede them.
        flow.waitForGone(hasText("No desktop online", substring = true), NETWORK_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_chat")

        // (sg_session-row moved to the capture-only screens at the end: the
        // Recent sheet behind the history glyph lists THREADS since EXP-1284.)

        // --- sg_search-list: the `#` typeahead's sheet (SLOP-28). Typing `#`
        // in the composer opens THE search list as a bottom sheet, its input
        // focused and prefilled with the token after the `#`.
        composeRule.onNode(hasTestTag("agent-composer-field")).performTextInput("Explain #")
        flow.waitFor(hasTestTag("issue-ref-search-sheet"), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag(SearchList.INPUT_TEST_ID)).performTextInput("cold")
        flow.waitFor(
            hasText(SHOWCASE_ISSUE_TITLE, substring = true) and hasAnyAncestor(hasTestTag("issue-ref-search-sheet")),
            SYNC_TIMEOUT,
        )
        flow.settle()
        flow.screenshot("sg_search-list")
        // The sheet's input holds the keyboard: the first Back only closes it.
        Espresso.closeSoftKeyboard()
        Espresso.pressBack()
        flow.waitForGone(hasTestTag("issue-ref-search-sheet"), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag("agent-composer-field")).performTextReplacement("")
        Espresso.closeSoftKeyboard()
        flow.settle()
        // A tab now (Back would leave the app): the Devices tab returns.
        composeRule.onNode(hasTestTag("tab-devices")).performClick()
        flow.waitForGone(hasTestTag("agent-page"), NAV_TIMEOUT)
        flow.waitFor(hasText(DEMO_DEVICE_NAME, substring = true), SYNC_TIMEOUT)
        flow.settle(longer = true)

        // --- Machine settings: own, registered machines only — the gear is
        // absent otherwise, which is why the relay stub is a prerequisite.
        // EXP-909 follow-up: the gear IS the row's only control (the ⋯ menu
        // and its "Device settings" entry are gone ×4), and Update and Remove
        // are sections inside the sheet it opens.
        composeRule.onAllNodes(hasTestTag("device-settings-button")).onFirst().performClick()
        flow.waitFor(hasTestTag("device-settings-sheet"), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_machine-settings")
        // EXP-694: the sheet autosaves and carries no bottom button at all —
        // back (like a swipe down) dismisses it, same as the start sheet.
        Espresso.pressBack()
        flow.waitForGone(hasTestTag("device-settings-sheet"), NAV_TIMEOUT)
        flow.settle(longer = true)

        // --- The Actions surface: four shots off its bottom-bar tab (EXP-1187:
        // the More menu is gone on phones). EXP-1270: the Suggestions segment
        // is gone, the surface is the list.
        composeRule.onNode(hasTestTag("tab-actions")).performClick()
        flow.waitFor(hasText(SEEDED_ACTION_NAME, substring = true), SYNC_TIMEOUT)

        // --- The action page: a row opens ONE action (EXP-1291: a saved
        // prompt, no tabs). A pushed detail: back pops to the Actions tab.
        composeRule.onAllNodes(hasText(SEEDED_ACTION_NAME, substring = true)).onFirst().performClick()
        flow.waitFor(hasTestTag("action-detail-title"), NAV_TIMEOUT)
        Espresso.pressBack()
        flow.waitForGone(hasTestTag("action-detail-title"), NAV_TIMEOUT)
        flow.settle(longer = true)

        // --- Settings root: the gear lives on the board root, not on a profile
        // menu. Header matches keep ignoreCase so they hold whichever case
        // SectionHeader renders (sentence case since EXP-698).
        composeRule.onNode(hasContentDescription("Issues")).performClick()
        flow.waitFor(hasContentDescription("Settings"), NAV_TIMEOUT)
        composeRule.onNode(hasContentDescription("Settings")).performClick()
        flow.waitFor(hasText("Servers", ignoreCase = true), NAV_TIMEOUT)
        flow.waitFor(hasText("Teams", ignoreCase = true), NAV_TIMEOUT)
        flow.waitFor(hasText(TEAM_NAME), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_settings-root")

        // --- sg_getting-started: the block behind Settings' entry (EXP-1270).
        // The row shows while the demo team has open steps (the seed never
        // sets up a server), and opens the block in a bottom sheet.
        flow.waitFor(hasTestTag(GettingStartedCopy.ENTRY_TEST_ID), SYNC_TIMEOUT)
        composeRule.onNode(hasTestTag(GettingStartedCopy.ENTRY_TEST_ID)).performClick()
        flow.waitFor(hasTestTag("getting-started-sheet"), NAV_TIMEOUT)
        flow.waitFor(hasTestTag(GettingStartedCopy.rowTestId(GettingStartedStepKey.Board)), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_getting-started")
        Espresso.pressBack()
        flow.waitForGone(hasTestTag("getting-started-sheet"), NAV_TIMEOUT)
        flow.settle(longer = true)

        // --- Team settings: one tap deeper (the Teams section), then back out
        // for the account screen.
        composeRule.onAllNodes(hasText(TEAM_NAME)).onFirst().performClick()
        flow.waitFor(hasText("Boards", ignoreCase = true), NAV_TIMEOUT)
        flow.waitFor(hasText("Repositories", ignoreCase = true), NAV_TIMEOUT)
        flow.waitFor(hasText(TEAM_BOARD_NAME, substring = true), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_settings-team")

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
        flow.screenshot("sg_settings-account")

        // A scoped run that wants none of the three onboarding shots skips the
        // identity switch altogether: the capture-only screens below need
        // nothing from the app.
        if (ScreenshotFlow.anyRequested("sg_onboarding", "sg_onboarding-invite", "sg_onboarding-devices")) {
            // --- Onboarding: LAST on purpose, because it switches the signed-in
            // identity. Accounts are keyed per (instance, userId) —
            // `ServerAccount.makeId` — so "Add server" pointed at the SAME instance
            // signs the newcomer in ALONGSIDE the demo user rather than replacing
            // them (AccountDeduplicator only prunes TOKENLESS duplicate rows), and
            // nothing above has to be redone if this step ever regresses.
            //
            // The newcomer is a member of nothing with a null
            // `onboardingCompletedAt`, so AppNavHost routes straight into the
            // wizard. NOTHING is submitted: creating a team or accepting an invite
            // would mutate the seed and burn the invite the other lanes photograph.
            //
            // EXP-1267: the newcomer's reused session is signed in in-process,
            // beside the demo account (exactly what "Add server" did); the Add
            // server + login form path is only the fallback.
            if (!ScreenshotSession.signIn(
                    instanceUrl,
                    ScreenshotFlow.NEWCOMER_EMAIL,
                    ScreenshotFlow.NEWCOMER_PASSWORD,
                    keepOtherAccounts = true,
                )
            ) {
                composeRule.onNode(hasContentDescription("Back")).performClick()
                flow.waitFor(hasText("Servers", ignoreCase = true), NAV_TIMEOUT)
                composeRule.onAllNodes(hasText("Add server")).onFirst().performClick()
                flow.chooseInstance(instanceUrl)
                flow.awaitLoginScreen()
                flow.submitLogin(ScreenshotFlow.NEWCOMER_EMAIL, ScreenshotFlow.NEWCOMER_PASSWORD)
            }
            flow.waitFor(hasText("Get started"), SYNC_TIMEOUT)
            composeRule.onAllNodes(hasText("Get started")).onFirst().performClick()
            // P3: the team step is web's choice page — the mark, "Welcome to
            // Exponential", Create a team / Join a team.
            flow.waitFor(hasText("Welcome to Exponential"), NAV_TIMEOUT)
            flow.waitFor(hasTestTag("team-setup-join"), NAV_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_onboarding")

            // --- Onboarding, the two later steps (EXP-725). Reaching them for
            // real means creating a team AND a board, which would mutate the
            // shared seed, so the wizard's capture seam presets the step instead
            // (OnboardingTestHooks — inert outside this process) and the lane
            // signs in as the STARTER: the seed's third identity, who owns a
            // board-less team and never finished the wizard (a board would let
            // the server backfill the flag). The newcomer above
            // is a member of nothing, and the invite + devices steps both need a
            // team to exist.
            OnboardingTestHooks.startStep = "invite"
            // The wizard's persistent escape hatch (every step carries it now) —
            // signing the newcomer out frees the login flow to bring the starter
            // in beside the still-signed-in demo account.
            composeRule.onAllNodes(hasText("Sign out")).onFirst().performClick()
            // EXP-1267: the starter's reused session, signed in in-process; the
            // login form only as the fallback below.
            if (!ScreenshotSession.signIn(
                    instanceUrl,
                    ScreenshotFlow.STARTER_EMAIL,
                    ScreenshotFlow.STARTER_PASSWORD,
                    keepOtherAccounts = true,
                )
            ) {
                // Where the Sign out lands depends on what else holds a token.
                // Invalidating the ACTIVE row's session keeps the row and flips
                // AppNavHost's `needsAuth`, so the usual outcome is the login form
                // for the same instance; the instance picker shows when no account
                // is left at all, and the demo user's app when the router fell back
                // to them — the servers list then adds one the same way as above.
                when {
                    // EXP-857: the resolved login screen leads with "Continue with
                    // email"; awaitLoginScreen below opens the form behind it.
                    flow.waitForOptional(hasTestTag("login-continue-with-email"), NAV_TIMEOUT) -> Unit
                    flow.waitForOptional(hasText(ScreenshotFlow.SELF_HOST_LINK), NAV_TIMEOUT) ->
                        flow.chooseInstance(instanceUrl)
                    else -> {
                        composeRule.onNode(hasContentDescription("Issues")).performClick()
                        flow.waitFor(hasContentDescription("Settings"), NAV_TIMEOUT)
                        composeRule.onNode(hasContentDescription("Settings")).performClick()
                        flow.waitFor(hasText("Servers", ignoreCase = true), NAV_TIMEOUT)
                        composeRule.onAllNodes(hasText("Add server")).onFirst().performClick()
                        flow.chooseInstance(instanceUrl)
                    }
                }
                flow.awaitLoginScreen()
                flow.submitLogin(ScreenshotFlow.STARTER_EMAIL, ScreenshotFlow.STARTER_PASSWORD)
            }
            // The welcome page is step 0 for everyone; the preset only takes
            // effect once the team step has resolved a team.
            if (flow.waitForOptional(hasText("Get started"), SYNC_TIMEOUT)) {
                composeRule.onAllNodes(hasText("Get started")).onFirst().performClick()
            }

            // --- Invite step: header + the invite-link creator, before a link is
            // minted (generating one would leave a live invite in the seed).
            // The starter's teams resolve off a cold first sync.
            flow.waitFor(hasTestTag("onboarding-invite-step"), FIRST_SYNC_TIMEOUT)
            flow.waitFor(hasTestTag("invite-generate"), SYNC_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_onboarding-invite")

            // --- Devices step: the two install cards plus "Your devices". The
            // starter has no machine of their own, so this photographs the empty
            // state, which is what a first-run user actually sees.
            composeRule.onAllNodes(hasText("Skip for now")).onFirst().performClick()
            flow.waitFor(hasTestTag("onboarding-devices-step"), NAV_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_onboarding-devices")
        }

        finished = true
    }

    /**
     * EXP-1296: the capture-only screens — no backend, no sign-in. Each is
     * drawn by its real composables over fixture data and set as the
     * activity's content, so any one of them is reachable alone.
     */
    @Test
    fun captureOnlyScreens() {
        ownsShot = { it in CAPTURE_SHOTS || it in EXTRA_SHOTS }
        assumeTrue(ScreenshotFlow.anyRequested(*(CAPTURE_SHOTS + EXTRA_SHOTS).toTypedArray()))

        fun show(testId: String, content: @Composable () -> Unit) {
            composeRule.runOnUiThread {
                composeRule.activity.setContent {
                    ExponentialTheme {
                        // The app's screens sit in a Scaffold that provides the
                        // content colour; a capture's bare Text needs the same.
                        CompositionLocalProvider(LocalContentColor provides MaterialTheme.colorScheme.onBackground) {
                            content()
                        }
                    }
                }
            }
            flow.waitFor(hasTestTag(testId), NAV_TIMEOUT)
            flow.settle()
        }
        fun show(kind: HomeCaptureKind) = show(kind.testId) { HomeCaptures(kind) }
        fun show(kind: ThreadCaptureKind) = show(kind.testId) { ThreadCaptures(kind) }
        fun show(kind: AgentsCaptureKind) = show(kind.testId) { AgentsCaptures(kind) }
        fun show(kind: SessionCaptureKind) = show(kind.testId) { SessionCaptures(kind) }

        // --- sg_agent-home: the Agent page — the dawn mark behind the title,
        // the composer at the top of the page.
        show(HomeCaptureKind.Home)
        flow.screenshot("sg_agent-home")

        // --- sg_chat-issues: the composer carrying two issue chips.
        show(HomeCaptureKind.ChatIssues)
        flow.screenshot("sg_chat-issues")

        // --- sg_composer-menu: the composer's "+" menu sheet, every row.
        show(HomeCaptureKind.ComposerMenu)
        flow.screenshot("sg_composer-menu")

        // --- sg_threads: the Threads tab — agents band + Needs you · Running
        // · Recent.
        show(ThreadsCaptureKind.Threads.testId) { ThreadsCaptures(ThreadsCaptureKind.Threads) }
        flow.screenshot("sg_threads")

        // --- sg_thread · sg_agent-channel · sg_session-row: a thread's
        // Conversation, an agent's channel, one session card per state.
        show(ThreadCaptureKind.Thread)
        flow.screenshot("sg_thread")
        show(ThreadCaptureKind.AgentChannel)
        flow.screenshot("sg_agent-channel")
        show(ThreadCaptureKind.SessionCards)
        flow.screenshot("sg_session-row")

        // --- sg_feedback-states: the six feedback kinds (EXP-1275). A
        // capture-only screen, every kind drawn by its real primitive and the
        // error toast raised on appear.
        show(FeedbackStatesScreenTestId) { FeedbackStatesScreen() }
        flow.screenshot("sg_feedback-states")

        // --- sg_work-card · sg_script-card · sg_comment-call-card: the run
        // thread's cards (EXP-1240 / EXP-1279 / EXP-1241), each drawn by its
        // real composable on a capture-only screen.
        fun showThreadCards(kind: ThreadCardsKind) {
            show(kind.testId) { ThreadCardsScreen(kind) }
        }
        showThreadCards(ThreadCardsKind.Work)
        flow.screenshot("sg_work-card")
        showThreadCards(ThreadCardsKind.Script)
        flow.screenshot("sg_script-card")
        showThreadCards(ThreadCardsKind.Comment)
        flow.screenshot("sg_comment-call-card")

        // --- sg_settings-team-agent: one team agent's settings page.
        show(AgentsCaptureKind.AgentPage)
        flow.screenshot("sg_settings-team-agent")

        // --- Guide-only extras (never catalog views): the session view, an
        // issue with its Start circle and the agents list.
        show(SessionCaptureKind.Session)
        extra("x_session-view")
        show(SessionCaptureKind.IssueStart)
        extra("x_issue-start")
        show(AgentsCaptureKind.AgentsList)
        extra("x_agents-list")

        finished = true
    }

    /** A Guide-only extra: through the same allowlisted wrapper, under a name
     *  the catalog gate (which reads the literal shot-name calls) never sees. */
    private fun extra(name: String) {
        check(name in EXTRA_SHOTS) { "unknown extra shot $name" }
        flow.screenshot(name)
    }

    /**
     * Board switcher → the named board. The sheet's rows carry the board name
     * as plain text; the first match is the row.
     */
    /** Any composer issue chip: the tag carries the IDENTIFIER, so a prefix match. */
    private fun hasIssueChip(): SemanticsMatcher =
        SemanticsMatcher("has an agent-composer issue chip") { node ->
            node.config.getOrNull(SemanticsProperties.TestTag)
                ?.startsWith("agent-composer-chip-issue-") == true
        }

    private fun switchBoard(name: String) {
        composeRule.onNode(hasContentDescription("Switch board")).performClick()
        flow.waitFor(hasText("Switch board"), NAV_TIMEOUT)
        flow.waitFor(hasText(name, substring = true), SYNC_TIMEOUT)
        composeRule.onAllNodes(hasText(name, substring = true)).onFirst().performClick()
        flow.waitForGone(hasText("Switch board"), NAV_TIMEOUT)
        flow.settle(longer = true)
    }
}
