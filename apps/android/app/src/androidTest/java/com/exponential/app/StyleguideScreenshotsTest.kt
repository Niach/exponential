package com.exponential.app

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.hasAnyDescendant
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.swipe
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeDown
import androidx.compose.ui.test.swipeUp
import androidx.test.espresso.Espresso
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.rule.GrantPermissionRule
import com.exponential.app.domain.AgentHome
import com.exponential.app.domain.AppExport
import com.exponential.app.domain.AppRecords
import com.exponential.app.domain.Apps
import com.exponential.app.domain.AgentSettings
import com.exponential.app.domain.ComposerAgentRow
import com.exponential.app.domain.ComposerMenu
import com.exponential.app.domain.ComposerMenuRowId
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.Stories
import com.exponential.app.domain.SearchList
import androidx.activity.compose.setContent
import com.exponential.app.ui.gettingstarted.GettingStartedCopy
import com.exponential.app.ui.gettingstarted.GettingStartedStepKey
import com.exponential.app.ui.styleguide.FeedbackStatesScreen
import com.exponential.app.ui.styleguide.FeedbackStatesScreenTestId
import com.exponential.app.ui.styleguide.ThreadCardsKind
import com.exponential.app.ui.styleguide.AppConsentSpecimenScreen
import com.exponential.app.ui.styleguide.AppConsentSpecimenTestId
import com.exponential.app.ui.styleguide.AskDockSpecimenScreen
import com.exponential.app.ui.styleguide.AskDockSpecimenScreenTestId
import com.exponential.app.ui.styleguide.DockCardSpecimenScreen
import com.exponential.app.ui.styleguide.DockCardSpecimenScreenTestId
import com.exponential.app.ui.styleguide.ThreadCardsScreen
import com.exponential.app.domain.Chats
import com.exponential.app.ui.styleguide.ChatListSpecimenScreen
import com.exponential.app.ui.styleguide.ChatListSpecimenTestId
import com.exponential.app.ui.styleguide.ChatMenusSpecimenScreen
import com.exponential.app.ui.styleguide.ChatMenusSpecimenTestId
import com.exponential.app.ui.styleguide.ChatSelectionSpecimenScreen
import com.exponential.app.ui.styleguide.ChatSelectionSpecimenTestId
import com.exponential.app.ui.styleguide.SealedLockedSpecimenScreen
import com.exponential.app.ui.styleguide.SealedLockedSpecimenTestId
import com.exponential.app.ui.styleguide.ChatTranscriptSpecimenScreen
import com.exponential.app.ui.styleguide.ChatTranscriptSpecimenTestId
import com.exponential.app.ui.styleguide.MentionPillSpecimenScreen
import com.exponential.app.ui.styleguide.MentionPillSpecimenTestId
import com.exponential.app.ui.styleguide.CallControlsSpecimenScreen
import com.exponential.app.ui.styleguide.CallControlsSpecimenTestId
import com.exponential.app.ui.styleguide.VoiceCallSpecimenScreen
import com.exponential.app.ui.styleguide.VoiceCallSpecimenTestId
import com.exponential.app.ui.styleguide.DeviceReadinessSpecimenScreen
import com.exponential.app.ui.styleguide.DeviceReadinessSpecimenTestId
import com.exponential.app.ui.styleguide.PhoneTabBarSpecimenScreen
import com.exponential.app.domain.SettingsNav
import com.exponential.app.ui.styleguide.PhonePageHeaderSpecimenTestId
import com.exponential.app.ui.styleguide.PhonePageHeaderSpecimenScreen
import com.exponential.app.ui.styleguide.PhoneBoardMenuSpecimenTestId
import com.exponential.app.ui.styleguide.PhoneBoardMenuSpecimenScreen
import com.exponential.app.ui.styleguide.PhoneTabBarSpecimenTestId
import com.exponential.app.ui.styleguide.SchedulePickerSpecimenScreen
import com.exponential.app.ui.styleguide.SchedulePickerSpecimenTestId
import com.exponential.app.ui.styleguide.StoryRingSpecimenScreen
import com.exponential.app.ui.styleguide.PlanPageSpecimenScreen
import com.exponential.app.ui.styleguide.PlanPageSpecimenTestId
import com.exponential.app.ui.styleguide.StoryTabSpecimenScreen
import com.exponential.app.ui.styleguide.StoryTabSpecimenTestId
import com.exponential.app.ui.styleguide.StoryRingSpecimenTestId
import com.exponential.app.ui.styleguide.StoryViewerSpecimenScreen
import com.exponential.app.ui.styleguide.StoryViewerSpecimenTestId
import com.exponential.app.ui.styleguide.TraceSpecimenKind
import com.exponential.app.ui.styleguide.TraceSpecimenScreen
import com.exponential.app.ui.theme.ExponentialTheme
import com.exponential.app.domain.IssueDraftPage
import com.exponential.app.ui.onboarding.OnboardingTestHooks
import androidx.compose.runtime.Composable
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
import java.util.TimeZone
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
 *   sg_sign-in · sg_onboarding-create-team ·
 *   sg_board-empty ·
 *   sg_board-bulk-edit · sg_issue-comments · sg_issue-properties ·
 *   sg_issue-create · sg_search · sg_my-issues · sg_agents ·
 *   sg_chat · sg_composer-menu · sg_chat-issues ·
 *   sg_machine-settings · sg_threads ·
 *   sg_session-row · sg_pr-row ·
 *   sg_settings-root · sg_settings-team ·
 *   sg_settings-account · sg_onboarding · sg_onboarding-invite ·
 *   sg_onboarding-devices · sg_agent-home · sg_search-list ·
 *   sg_getting-started · sg_feedback-states · sg_work-card ·
 *   sg_script-card · sg_comment-call-card · sg_thread · sg_agent-channel ·
 *   sg_session · sg_trace · sg_code-block · sg_ask-dock · sg_dock-card · sg_plan-card · sg_session-xref ·
 *   sg_settings-team-agent · sg_settings-prompts · sg_settings-prompt ·
 *   sg_apps · sg_app · sg_chat-list · sg_phone-tab-bar · sg_chat-transcript ·
 *   sg_chat-menus · sg_chat-selection · sg_sealed-locked · sg_schedule-picker · sg_mention-pill · sg_app-consent ·
 *   sg_app-export · sg_app-import · sg_app-draft · sg_app-versions · sg_app-settings · sg_app-data · sg_voice-call · sg_call-controls · sg_settings-assistant · sg_device-readiness ·
 *   sg_story-ring · sg_story-viewer · sg_story-tab · sg_thread-stories · sg_plan-page
 *
 * EXP-1295 retired `sg_chat-action` and `sg_action-create` (the builtin
 * actions and the action create sheet left the catalog), renamed the
 * Threads tab `sg_threads` and the agent channel `sg_agent-channel`;
 * `sg_pr-row` = the issue page's pull-request row. S-3 retired `sg_guide`
 * and `sg_guide-section` with the Guide tab (stories replaced it).
 *
 * EXP-1296 (Threads v2) split the suite in two tests. [captureStyleguideScreenshots]
 * walks the live app against the seeded backend; [captureOnlyScreens] needs NO
 * server and NO sign-in: the specimens with no seed counterpart (web and
 * desktop photograph styleguide islands for them) are drawn by their real
 * composables over fixture data (ui/styleguide) and set as the activity's
 * content — the feedback states and the thread cards. A scoped run (`shots:`) naming only capture
 * shots skips the live walk.
 *
 * EXP-1309: every Threads v2 surface WITH a seed counterpart is walked live,
 * so it reads the same seed web, desktop and iOS photograph and cannot drift:
 * sg_agent-home (the landing, the bottom bar over it), sg_session-row (THE
 * row on the Threads tab, EXP-1311/1316: the home has no Running band), sg_composer-menu +
 * sg_chat-issues (the home composer's "+" and Implement issue), sg_threads
 * (the Threads tab), sg_agent-channel (Triage), sg_thread (the cold-start
 * thread), sg_session ("Profile the launch path", its pending question in
 * place of the composer) and sg_settings-team-agent (Settings → Agents →
 * Triage). The suite pins the process time zone to the seed's
 * ([ScreenshotFlow.DEMO_TIME_ZONE]) so message clocks read like web and iOS.
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
 * `isMine && registered`) and the sg_chat* shots photograph (EXP-825: the
 * Agent page composer, the ONE launcher), and whose heartbeat announces the
 * agent accounts sg_agents photographs under its device rows (EXP-829,
 * folded into the machines by EXP-909). It also publishes the seeded child
 * session's feed sg_session watches over the steer relay (EXP-1309).
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
            "sg_feedback-states", "sg_work-card",
            "sg_script-card", "sg_comment-call-card",
            "sg_trace", "sg_code-block", "sg_ask-dock", "sg_dock-card", "sg_plan-card",
            // S-3: THE inter-session link line.
            "sg_session-xref",
            // Thread 58cb32df: the Chats pieces + the minutes schedule.
            "sg_chat-list", "sg_phone-tab-bar", "sg_phone-board-menu", "sg_phone-page-header", "sg_chat-transcript", "sg_chat-menus", "sg_schedule-picker",
            // #EXP-1388: the Chats multi-selection + the Archived entry.
            "sg_chat-selection",
            // EXP-1459: the locked state for sealed content.
            "sg_sealed-locked",
            // Thread e4a04d22: the member pill in a chat.
            "sg_mention-pill",
            // VAPP-120: the consent card with a typed action's words.
            "sg_app-consent",
            // NEW-69 / S-3: the phone-call screen's states, and its round buttons.
            "sg_voice-call",
            "sg_call-controls",
            // EXP-1375: the Storage band + the composer's low-disk row.
            "sg_device-readiness",
            // S-3 STORIES: THE story ring row + THE story viewer.
            "sg_story-ring", "sg_story-viewer",
            // stories.json STORIES TAB: THE tab body (tabCases[1]).
            "sg_story-tab",
            // #EXP-1493: THE plan page (plan-page.json `specimen`, phone).
            "sg_plan-page",
        )

        /** #EXP-1384: the Board tab's root is up (its All issues chip). */
        private val BOARD_ROOT_TAG = "${Chats.TestIds.BOARD_CHIP}-all"

        /** VAPP-122: the seeded Working hours app, with records (web `DEMO_APP_SLUG`). */
        private const val DEMO_APP_SLUG = "working-hours"

        /** VAPP-176: the seeded Release checklist app, a DRAFT (no version, no
         *  editor session; Jonas's, so leaving it never discards it). */
        private const val DEMO_DRAFT_APP_SLUG = "release-checklist"

        /** VAPP-121: app-export.json `capture.importText` and its one issue. */
        private const val APP_IMPORT_CAPTURE_TEXT = """{"format":"exponential-app/1","app":{"name":"Todos"}}"""
        private const val APP_IMPORT_CAPTURE_ISSUE = "/package: is required"

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

        // APP-14: the seed's realPr issue (its open PR row). Addressed by
        // row tag so the lazy board list can scroll to it.
        private const val GUIDE_ISSUE_ROW = "issue-row-APP-14"

        // APP-15: its thread carries the seeded quick replies.
        private const val BATCH_GUIDE_ISSUE_ROW = "issue-row-APP-15"

        private const val TEAM_NAME = "Acme"
        private const val TEAM_BOARD_NAME = "Mobile App"

        // The seed's SECOND board: created empty on purpose, so the "no issues
        // yet" state is photographable without deleting anything (EXP-642).
        private const val EMPTY_BOARD_NAME = "Launch Marketing"

        // Two backlog issues — the bulk-edit selection. Addressed by row tag so
        // the lazy list can scroll to them (their titles are far down the list).
        private const val BULK_FIRST_ROW = "issue-row-APP-11"
        private const val BULK_SECOND_ROW = "issue-row-APP-13"

        // EXP-825: the two codeable issues the sg_chat-issues shot chips —
        // APP-3 and APP-6, the same pair the web `chat-issues` recipe seeds
        // (apps/web/scripts/lib/demo-ids.ts), neither with a running session.
        // Picked through Implement issue's search list by a title prefix, so
        // the field's own text never matches the row's full title.
        private const val CHAT_FIRST_ISSUE_TITLE = "Dark mode contrast pass across settings"
        private const val CHAT_SECOND_ISSUE_TITLE = "Push notification deep links open the wrong tab"

        // EXP-1309: the seed's session tree (seedSessionTree in
        // apps/web/scripts/seed-screenshots.ts). The private running root
        // (the home's Running band and the Threads tab), the ended one
        // (Threads' Recent), the cold-start thread and its child session.
        // EXP-1341: a thread with linked issues is titled `#<first> <title>`
        // (` + <n-1>` with more, session-title.json `titles`), matched on the
        // issue title; the issue-less private spike keeps its stored title.
        private const val PRIVATE_THREAD_TITLE = "Spike: offline queue"
        private const val RECENT_THREAD_TITLE = "Fix flaky scroll restore on the issue list"
        private const val COLD_START_THREAD_TITLE = "Reduce cold start below 800 ms"
        private const val COLD_START_REPLY_FRAGMENT = "Board snapshot cache"
        private const val CHILD_SESSION_TITLE = "Profile the launch path"

        // The Triage agent: its channel (DEMO_AGENT_REQUEST, answered at
        // DEMO_AGENT_CHANNEL_TIMES.reply = 07:14Z, 09:14 in DEMO_TIME_ZONE)
        // and its settings page (seeded instructions).
        private const val AGENT_NAME = "Triage"
        private const val AGENT_REQUEST_FRAGMENT = "Label the Android ones too"
        private const val AGENT_REPLY_CLOCK = "09:14"
        private const val AGENT_INSTRUCTIONS_FRAGMENT = "Look after the boards"

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

    /** The zone [setUp] replaced, put back by [restoreTimeZone]. */
    private var previousTimeZone: TimeZone? = null

    @Before
    fun setUp() {
        ScreenshotFlow.useUiAutomatorScreenshots()
        // EXP-1309: message clocks print in the seed's zone, never the emulator's.
        previousTimeZone = ScreenshotFlow.pinDemoTimeZone()
        // sg_session watches the stub's feed: dial the relay at the host alias.
        ScreenshotFlow.applySteerRelayOverride()
    }

    @After
    fun restoreTimeZone() {
        previousTimeZone?.let(ScreenshotFlow::restoreTimeZone)
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
        ownsShot = { it !in CAPTURE_SHOTS }
        assumeFalse(ScreenshotFlow.requestedOnlyAmong(CAPTURE_SHOTS))
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

        // --- sg_agent-home: the Agent page the app lands on (EXP-1253/1306)
        // under the bottom bar: the dawn standing on the composer card + the
        // demo device's options line, pinned to the bottom (EXP-1316: no
        // Running band). Gated on the device pool having resolved (the
        // options pills are its).
        flow.waitFor(hasTestTag(AgentHome.HOME_TEST_ID), FIRST_SYNC_TIMEOUT)
        flow.waitFor(hasTestTag(Chats.TestIds.TAB_BOARD), SYNC_TIMEOUT)
        // The Team tab wears the synced team's name (not the "?" placeholder):
        // before that the page still shows the board-less newcomer's state.
        flow.waitFor(
            (hasTestTag(Chats.TestIds.TAB_TEAM) and (hasText(TEAM_NAME) or hasAnyDescendant(hasText(TEAM_NAME)))) or
                (hasText(TEAM_NAME) and hasAnyAncestor(hasTestTag(Chats.TestIds.TAB_TEAM))),
            SYNC_TIMEOUT,
        )
        // S-3: THE agent row over the composer, the viewer's Assistant (the
        // brand mark, first) preselected: its label reads `To <name>`, never
        // `Temporary session`. The device lives inside the + menu now.
        flow.waitFor(hasTestTag(ComposerAgentRow.TEST_ID_ROW), SYNC_TIMEOUT)
        flow.waitFor(hasTestTag(ComposerAgentRow.TEST_ID_LABEL) and hasText("To ", substring = true), SYNC_TIMEOUT)
        flow.waitForGone(hasText("No desktop online", substring = true), NETWORK_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_agent-home")

        // --- sg_session-row: THE session row (EXP-1248/1311). EXP-1316: the
        // home lost its Running band, so the row's phone instance is the
        // Threads tab's, gated on the seeded cold-start thread waiting on me
        // (web/desktop photograph a styleguide island).
        composeRule.onNode(hasTestTag("tab-threads")).performClick()
        flow.waitFor(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), NAV_TIMEOUT)
        flow.waitFor(
            hasTestTag(Chats.TestIds.CHAT_ROW) and
                hasText(COLD_START_THREAD_TITLE, substring = true),
            SYNC_TIMEOUT,
        )
        flow.settle()
        flow.screenshot("sg_session-row")

        // The app lands on the Agent tab; the board shots start on Issues.
        flow.waitFor(hasTestTag(Chats.TestIds.TAB_BOARD), SYNC_TIMEOUT)
        composeRule.onNode(hasTestTag(Chats.TestIds.TAB_BOARD)).performClick()

        // --- Board: wait out session fetch + first Electric sync, then let the
        // transient "Syncing…" pill clear (it photobombed a board shot once —
        // EXP-348).
        flow.waitFor(hasText(SHOWCASE_ISSUE_TITLE), FIRST_SYNC_TIMEOUT)
        flow.waitForGone(hasText("Syncing", substring = true), FIRST_SYNC_TIMEOUT)
        flow.settle()

        // --- Set up a team: thread 58cb32df moved "New team" to the Team
        // tab's teams group; it opens the create-or-join sheet. The extra
        // wait on the first card's title keeps the shot off a half-drawn
        // sheet. (#EXP-1384: the board switcher view is gone — the board
        // menu is the capture-only `sg_phone-board-menu`.)
        composeRule.onNode(hasTestTag(Chats.TestIds.TAB_TEAM)).performClick()
        flow.waitFor(hasTestTag("team-new"), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag("team-new")).performClick()
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
        composeRule.onNode(hasTestTag(Chats.TestIds.TAB_BOARD)).performClick()
        flow.waitFor(hasTestTag(BOARD_ROOT_TAG), NAV_TIMEOUT)
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
        // EXP-1302: Threads v2 dropped the Work screen's face strip (the
        // issue names its threads instead), so the page has no Run tab to
        // wait for.
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
        flow.waitFor(hasTestTag(BOARD_ROOT_TAG), NAV_TIMEOUT)
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
        flow.waitFor(hasTestTag(BOARD_ROOT_TAG), NAV_TIMEOUT)
        flow.settle()

        // --- Search: EXP-686 moved it off the bottom bar into the board
        // header, so it is a PUSHED screen now — tagged,
        // because "Search" also reads as the field's own placeholder. The
        // field is the only editable node on it, so hasSetTextAction
        // addresses it without a tag. Gate on a real hit — an unseeded
        // backend would render "No issues match".
        composeRule.onNode(hasTestTag(Chats.TestIds.SEARCH_BUTTON)).performClick()
        flow.waitFor(hasText("Search"), NAV_TIMEOUT)
        composeRule.onNode(hasSetTextAction()).performTextInput(SEARCH_QUERY)
        Espresso.closeSoftKeyboard()
        flow.waitFor(hasText(SEARCH_RESULT_TITLE, substring = true), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_search")
        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.waitFor(hasTestTag(BOARD_ROOT_TAG), NAV_TIMEOUT)
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

        // --- sg_thread-stories (stories.json STORIES TAB): APP-14's thread
        // (web's `$diffThreadId`) on its Stories tab, the strip's last,
        // icon-only: the Stack band + rail, the Changes row over the real
        // PR's GitHub diff, then the seeded stories' tiles. The Changes row
        // is the gate (the diff is the walk's one network dependency).
        if (ScreenshotFlow.isShotWanted("sg_thread-stories")) {
            flow.waitFor(hasTestTag("issue-thread-chip"), SYNC_TIMEOUT)
            composeRule.onAllNodes(hasTestTag("issue-thread-chip")).onFirst().performScrollTo()
            composeRule.onAllNodes(hasTestTag("issue-thread-chip")).onFirst().performClick()
            flow.waitFor(hasTestTag(Stories.Tab.TestIds.TAB), NAV_TIMEOUT)
            composeRule.onNode(hasTestTag(Stories.Tab.TestIds.TAB)).performClick()
            flow.waitFor(hasTestTag(Stories.Tab.TestIds.TILE), SYNC_TIMEOUT)
            flow.waitFor(hasTestTag(Stories.Tab.TestIds.STACK), SYNC_TIMEOUT)
            flow.waitFor(hasTestTag(Stories.Tab.TestIds.CHANGES), NETWORK_TIMEOUT)
            flow.settle(longer = true)
            flow.screenshot("sg_thread-stories")
            // Thread → the issue page.
            Espresso.pressBack()
            flow.waitForGone(hasTestTag("thread-tabs"), NAV_TIMEOUT)
            flow.waitFor(hasTestTag("issue-pr-row"), NAV_TIMEOUT)
            flow.settle()
        }

        // Issue → board.
        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.waitFor(hasTestTag(BOARD_ROOT_TAG), NAV_TIMEOUT)
        flow.settle()

        // --- sg_thread-replies: the quick replies (EXP-1211). APP-15's
        // thread again, left on its Conversation: the seeded `replies`
        // card's row over the composer capsule.
        if (ScreenshotFlow.isShotWanted("sg_thread-replies")) {
            composeRule.onNode(hasTestTag("issue-list"))
                .performScrollToNode(hasTestTag(BATCH_GUIDE_ISSUE_ROW))
            composeRule.onNode(hasTestTag(BATCH_GUIDE_ISSUE_ROW)).performClick()
            flow.waitFor(hasTestTag("issue-thread-chip"), SYNC_TIMEOUT)
            composeRule.onAllNodes(hasTestTag("issue-thread-chip")).onFirst().performScrollTo()
            composeRule.onAllNodes(hasTestTag("issue-thread-chip")).onFirst().performClick()
            flow.waitFor(hasTestTag(SessionTree.RepliesCopy.TEST_ID), SYNC_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_thread-replies")
            // Thread → issue → board.
            Espresso.pressBack()
            flow.waitForGone(hasTestTag("thread-tabs"), NAV_TIMEOUT)
            composeRule.onNode(hasContentDescription("Back")).performClick()
            flow.waitFor(hasTestTag(BOARD_ROOT_TAG), NAV_TIMEOUT)
            flow.settle()
        }

        // --- My Issues: thread 58cb32df — the board's My issues chip
        // (chats.json `shell.boardChips`), the active team's assigned issues
        // grouped by status; back to All issues after.
        // Gated like the other self-contained steps: a scoped run that does
        // not want it never blocks the later shots on its seed.
        if (ScreenshotFlow.anyRequested("sg_my-issues")) {
            val mineChip = hasTestTag("${Chats.TestIds.BOARD_CHIP}-mine")
            flow.waitFor(mineChip, NAV_TIMEOUT)
            composeRule.onNode(mineChip).performClick()
            flow.waitFor(hasText(MY_ISSUE_TITLE, substring = true), SYNC_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_my-issues")
            composeRule.onNode(hasTestTag("${Chats.TestIds.BOARD_CHIP}-all")).performClick()
            flow.settle()
        }

        // --- Devices (EXP-686, the renamed Agents surface): since EXP-642
        // this lane needs the relay stub (`screenshots:desktop`) — the demo
        // user's own device row is what the next three shots are taken from,
        // and an empty machines list is not a useful reference shot either.
        // EXP-909 retired `sg_usage` with the cross-device Accounts section:
        // each device row now lists its OWN logins, so `sg_agents` is the
        // accounts shot too.
        openDevices()
        flow.waitFor(hasText(DEMO_DEVICE_NAME, substring = true), NETWORK_TIMEOUT)
        // A-16: nothing folds — every device's logins ride its line as chips.
        flow.waitFor(hasTestTag("device-login-chip"), NETWORK_TIMEOUT)
        // FEED-149: the demo user OWNS the demo team, so the seeded teammate's
        // shared server ("Acme build server") carries the owner's Remove.
        flow.waitFor(hasTestTag("device-remove-share-button"), NETWORK_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_agents")

        // --- sg_chat (then sg_search-list, sg_composer-menu, sg_chat-issues):
        // the Agent page (EXP-825, the ONE launcher). The bottom bar's Chat
        // arm switches to it on the default machine (the EXP-909 follow-up
        // retired the device row's play glyph — a device list starts
        // nothing): an empty composer is a chat; the "+" menu's Implement
        // issue checks issues (two chips, a batch).
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

        // --- sg_composer-menu / sg_chat-issues (iOS parity): the "+" is the
        // composer's ONE tool (EXP-1249), its sheet of the
        // `composer-menu.json` rows is sg_composer-menu. SLOP-28: Implement
        // issue › PUSHES THE search list inside the same sheet, whose
        // `Implement N issues` button closes the menu with the picks as the
        // subject: two issue chips. Nothing is submitted.
        // S-3: Temporary session first (the web recipe's step): the full
        // menu, its Device / Account / Model rows carrying the picks, the
        // demo device's name as the Device row's value.
        val temporaryCircle = ComposerAgentRow.optionTestId(ComposerAgentRow.TEMPORARY_PICK)
        flow.waitFor(hasTestTag(temporaryCircle), SYNC_TIMEOUT)
        composeRule.onNode(hasTestTag(temporaryCircle)).performClick()
        flow.waitFor(
            hasTestTag(ComposerAgentRow.TEST_ID_LABEL) and hasText(ComposerAgentRow.COPY_TEMPORARY),
            NAV_TIMEOUT,
        )
        flow.waitForGone(hasText("No desktop online", substring = true), NETWORK_TIMEOUT)
        composeRule.onNode(hasTestTag(ComposerMenu.PLUS_TEST_ID)).performClick()
        flow.waitFor(hasTestTag(ComposerMenu.MENU_TEST_ID), NAV_TIMEOUT)
        val implementRow = ComposerMenu.rowTestId(ComposerMenuRowId.ImplementIssue)
        flow.waitFor(hasTestTag(implementRow), NAV_TIMEOUT)
        flow.waitFor(
            hasText(DEMO_DEVICE_NAME, substring = true) and
                hasAnyAncestor(hasTestTag(ComposerMenu.rowTestId(ComposerMenuRowId.Device))),
            NETWORK_TIMEOUT,
        )
        flow.settle()
        flow.screenshot("sg_composer-menu")
        composeRule.onNode(hasTestTag(implementRow)).performClick()
        flow.waitFor(hasTestTag("agent-composer-issues-picker"), NAV_TIMEOUT)
        for (title in listOf(CHAT_FIRST_ISSUE_TITLE, CHAT_SECOND_ISSUE_TITLE)) {
            // A prefix query: the field's own text never matches the row's
            // full title, so the match below is the row.
            composeRule.onNode(hasTestTag(SearchList.INPUT_TEST_ID))
                .performTextReplacement(title.split(' ').take(2).joinToString(" "))
            val row = hasText(title, substring = true) and hasAnyAncestor(hasTestTag("agent-composer-issues-picker"))
            flow.waitFor(row, SYNC_TIMEOUT)
            composeRule.onAllNodes(row).onFirst().performClick()
        }
        Espresso.closeSoftKeyboard()
        flow.waitFor(hasTestTag(ComposerMenu.IMPLEMENT_SUBMIT_TEST_ID), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag(ComposerMenu.IMPLEMENT_SUBMIT_TEST_ID)).performClick()
        flow.waitForGone(hasTestTag(ComposerMenu.MENU_TEST_ID), NAV_TIMEOUT)
        flow.waitFor(hasIssueChip(), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_chat-issues")
        // A tab now (Back would leave the app): the Team tab's Devices row
        // returns.
        openDevices()
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
        // EXP-1302: the sheet's first Back can land while it is still
        // animating in and be swallowed; a second one closes it.
        flow.settle()
        if (flow.exists(hasTestTag("device-settings-sheet"))) Espresso.pressBack()
        flow.waitForGone(hasTestTag("device-settings-sheet"), NAV_TIMEOUT)
        flow.settle(longer = true)

        // --- sg_threads: the Threads tab (EXP-1291) over the seed's tree —
        // the Agents band, then Needs you · Running · Recent, each row ending
        // on its quiet word (EXP-1309). Gated on the private root's row (no
        // lock on list rows since EXP-1339) and on APP-14's open stack word,
        // so neither a half-synced tree nor PR-less rows are photographed
        // (`merged` has no text, EXP-1339: its lead alone turns violet).
        // Thread 58cb32df: the CHATS tab (chats.json): the headline, the
        // agent bubbles, the chips and one flat row per root chat.
        composeRule.onNode(hasTestTag("tab-threads")).performClick()
        flow.waitFor(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), NAV_TIMEOUT)
        // S-3: the pinned Agents section (the Assistant first) pushes the
        // chats below the fold of the lazy list, so scroll to each gated row
        // (an off-screen lazy row has no node), then back to the top.
        val threadRow = hasTestTag(Chats.TestIds.CHAT_ROW)
        for ((title, row) in listOf(
            RECENT_THREAD_TITLE to hasText(RECENT_THREAD_TITLE, substring = true),
            PRIVATE_THREAD_TITLE to (threadRow and hasText(PRIVATE_THREAD_TITLE, substring = true)),
        )) {
            val deadline = System.currentTimeMillis() + SYNC_TIMEOUT
            while (runCatching {
                    composeRule.onNode(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST)).performScrollToNode(row)
                }.isFailure
            ) {
                assertTrue("no chat row \"$title\"", System.currentTimeMillis() < deadline)
                flow.settle()
            }
        }
        composeRule.onNode(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST)).performScrollToIndex(0)
        flow.settle()
        val agentRow = hasTestTag(Chats.TestIds.AGENT_BUBBLE) and hasText(AGENT_NAME)
        flow.waitFor(agentRow, SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_threads")

        // --- sg_apps / sg_app (VAPP-116): #EXP-1384 made Apps the third
        // TAB; it opens the selected team's Apps (no built-in apps: the
        // seeded apps, by name); the Working hours app (VAPP-122, with
        // records) opens its editor and the shot shows its PREVIEW tab: the
        // app rendered through the host over its synced records (web-mobile's
        // `app` shot). Leaving it ends the editor session, exactly as the web
        // capture does.
        composeRule.onNode(hasTestTag("tab-apps")).performClick()
        flow.waitFor(hasTestTag(Apps.TestIds.LIST), NAV_TIMEOUT)
        val demoAppRow = hasTestTag("${Apps.TestIds.ROW}-$DEMO_APP_SLUG")
        flow.waitFor(demoAppRow, SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_apps")
        composeRule.onNode(demoAppRow).performClick()
        flow.waitFor(hasTestTag(Apps.TestIds.TABS), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag("app-editor-tab-preview")).performClick()
        flow.waitFor(hasTestTag(Apps.TestIds.PREVIEW), SYNC_TIMEOUT)
        flow.settle(longer = true)
        flow.screenshot("sg_app")
        // --- sg_app-versions / sg_app-settings (VAPP-177, apps.json PHONES):
        // the header's `…` → Versions (Working hours' three versions, the
        // head row `Current`, the others Preview + Restore) and → Settings
        // (identity, description, records line, Tools, Export + Delete app),
        // each the menu's sheet over the editor.
        if (ScreenshotFlow.anyRequested("sg_app-versions")) {
            composeRule.onNode(hasTestTag(Apps.TestIds.MENU)).performClick()
            flow.waitFor(hasTestTag("app-editor-menu-versions"), NAV_TIMEOUT)
            composeRule.onNode(hasTestTag("app-editor-menu-versions")).performClick()
            flow.waitFor(hasTestTag(Apps.TestIds.VERSIONS), NAV_TIMEOUT)
            flow.waitFor(hasTestTag(Apps.TestIds.VERSION_RESTORE), SYNC_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_app-versions")
            Espresso.pressBack()
            flow.waitForGone(hasTestTag(Apps.TestIds.VERSIONS), NAV_TIMEOUT)
            flow.settle()
        }
        if (ScreenshotFlow.anyRequested("sg_app-settings")) {
            composeRule.onNode(hasTestTag(Apps.TestIds.MENU)).performClick()
            flow.waitFor(hasTestTag("app-editor-menu-settings"), NAV_TIMEOUT)
            composeRule.onNode(hasTestTag("app-editor-menu-settings")).performClick()
            flow.waitFor(hasTestTag(Apps.TestIds.SETTINGS), NAV_TIMEOUT)
            flow.waitFor(hasTestTag("app-settings-tools"), SYNC_TIMEOUT)
            Espresso.closeSoftKeyboard()
            flow.settle()
            flow.screenshot("sg_app-settings")
            Espresso.pressBack()
            flow.waitForGone(hasTestTag(Apps.TestIds.SETTINGS), NAV_TIMEOUT)
            flow.settle()
        }
        // --- sg_app-data (VAPP-178, app-records.json `dataTab` PHONES): the
        // Data segment: the kind select + Add record over the picked kind's
        // rows (Working hours' seeded records, newest first).
        if (ScreenshotFlow.anyRequested("sg_app-data")) {
            composeRule.onNode(hasTestTag("app-editor-tab-${Apps.EditorTabs.DATA}")).performClick()
            flow.waitFor(hasTestTag(Apps.TestIds.DATA), NAV_TIMEOUT)
            flow.waitFor(hasTestTag(AppRecords.DataTestIds.ROW), SYNC_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_app-data")
        }
        Espresso.pressBack()
        flow.waitFor(hasTestTag(Apps.TestIds.LIST), NAV_TIMEOUT)
        flow.settle()

        // --- sg_app-draft (VAPP-176, apps.json NEW APP): the Release
        // checklist row = a draft, its editor opens on the Chat tab: the
        // Draft pill, `What should this app do?` with the starter pills and
        // the composer open on `Describe your app…` (the keyboard put away
        // so the starters show, as on iOS).
        if (ScreenshotFlow.anyRequested("sg_app-draft")) {
            val draftRow = hasTestTag("${Apps.TestIds.ROW}-$DEMO_DRAFT_APP_SLUG")
            flow.waitFor(draftRow, SYNC_TIMEOUT)
            composeRule.onNode(draftRow).performClick()
            flow.waitFor(hasTestTag(Apps.TestIds.TABS), NAV_TIMEOUT)
            flow.waitFor(hasTestTag(Apps.TestIds.STARTER), SYNC_TIMEOUT)
            flow.settle(longer = true)
            Espresso.closeSoftKeyboard()
            flow.settle()
            flow.screenshot("sg_app-draft")
            Espresso.pressBack()
            flow.settle()
            if (!flow.exists(hasTestTag(Apps.TestIds.LIST))) Espresso.pressBack()
            flow.waitFor(hasTestTag(Apps.TestIds.LIST), NAV_TIMEOUT)
            flow.settle()
        }

        // --- sg_app-export / sg_app-import (VAPP-121, app-export.json
        // `capture`): Working hours' row menu → Export with "Include records"
        // switched on; the band's Import… holding the fixture's capture text
        // and the server's ONE issue. A scoped run not naming them skips both.
        if (ScreenshotFlow.anyRequested("sg_app-export")) {
            composeRule.onNode(hasContentDescription("Menu for Working hours")).performClick()
            flow.waitFor(hasText(AppExport.Copy.EXPORT), NAV_TIMEOUT)
            composeRule.onNode(hasText(AppExport.Copy.EXPORT)).performClick()
            flow.waitFor(hasTestTag("app-export-dialog"), NAV_TIMEOUT)
            flow.waitFor(hasTestTag("app-export-records"), SYNC_TIMEOUT)
            composeRule.onNode(hasTestTag("app-export-records")).performClick()
            flow.settle()
            flow.screenshot("sg_app-export")
            Espresso.pressBack()
            flow.settle()
            if (flow.exists(hasTestTag("app-export-dialog"))) Espresso.pressBack()
            flow.waitForGone(hasTestTag("app-export-dialog"), NAV_TIMEOUT)
            flow.settle()
        }
        if (ScreenshotFlow.anyRequested("sg_app-import")) {
            // VAPP-176: Import… is the list band's own pill (its own sheet).
            composeRule.onNode(hasTestTag(Apps.TestIds.IMPORT_APP)).performClick()
            flow.waitFor(hasTestTag("app-import-text"), NAV_TIMEOUT)
            composeRule.onNode(hasTestTag("app-import-text")).performTextInput(APP_IMPORT_CAPTURE_TEXT)
            Espresso.closeSoftKeyboard()
            flow.waitFor(hasText(APP_IMPORT_CAPTURE_ISSUE), SYNC_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_app-import")
            Espresso.pressBack()
            flow.settle()
            if (flow.exists(hasTestTag("app-import-dialog"))) Espresso.pressBack()
            flow.waitForGone(hasTestTag("app-import-dialog"), NAV_TIMEOUT)
            flow.settle()
        }
        composeRule.onNode(hasTestTag("tab-threads")).performClick()
        flow.waitFor(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), NAV_TIMEOUT)
        flow.settle(longer = true)

        // --- sg_agent-channel: an agent row opens its channel (the same
        // conversation screen): Triage's report (its bare FB-n chipped as
        // issue pills, EXP-760), the thread it started, the person's request
        // and the reply. The reply's clock proves the pinned zone.
        // VAPP-122: a scoped run that does not ask for this shot skips the
        // walk (the apps views alone must not wait on the channel).
        if (ScreenshotFlow.anyRequested("sg_agent-channel")) {
            // S-3: the pinned agent row (its bubble wears a story ring and
            // opens the viewer, not the channel).
            composeRule.onAllNodes(hasTestTag("chat-agent-row") and hasText(AGENT_NAME)).onFirst().performClick()
            flow.waitFor(hasText(AGENT_REQUEST_FRAGMENT, substring = true), SYNC_TIMEOUT)
            flow.waitFor(hasText(AGENT_REPLY_CLOCK), SYNC_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_agent-channel")
            Espresso.pressBack()
            flow.waitFor(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), NAV_TIMEOUT)
            flow.settle(longer = true)
        }

        // --- sg_thread: the cold-start thread (APP-5): the strip, the
        // Conversation with its session cards, the agent's reply and the
        // pending question card, the reply box. #EXP-1433: the strip never
        // scrolls — + and Conversation pinned left, Stories right, the
        // issues + sessions between them behind ONE group tab.
        val coldStartRow = hasTestTag(Chats.TestIds.CHAT_ROW) and
            hasText(COLD_START_THREAD_TITLE, substring = true)
        // S-3: the pinned Agents section pushes the row down the lazy list
        // (and under the floating bar): scroll it in, tap it clear of the bar.
        composeRule.onNode(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST)).performScrollToNode(coldStartRow)
        flow.settle()
        tapClearOfBar(coldStartRow)
        flow.waitFor(hasTestTag(SessionTree.ThreadCopy.TEST_ID_CONVERSATION), NAV_TIMEOUT)
        flow.waitFor(hasText(COLD_START_REPLY_FRAGMENT, substring = true), SYNC_TIMEOUT)
        // S-3: the seeded question stays inline (the Dock takes no asks and
        // the Guide tab is gone; stories carry them).
        flow.waitFor(hasTestTag("plan-card"), SYNC_TIMEOUT)
        flow.waitFor(hasTestTag("thread-strip-zone"), SYNC_TIMEOUT)
        flow.waitFor(hasTestTag("thread-tab-add"), SYNC_TIMEOUT)
        // #EXP-1433: the seed's issues + child session = ONE group tab.
        flow.waitFor(hasTestTag(SessionTree.ThreadCopy.TEST_ID_GROUP), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_thread")

        // --- sg_session: its "Profile the launch path" card opens the raw
        // session view (EXP-1291), the stub relay's feed for the seed's child
        // session. Its pending question takes the composer's place
        // (EXP-1306), so the question card is the gate.
        val childCard = hasTestTag(SessionTree.CardCopy.TEST_ID_CARD) and
            hasText(CHILD_SESSION_TITLE, substring = true)
        // EXP-1323: the suggestions card pushes the card above the fold once
        // the conversation opens at its tail; drag back up to it like iOS (a
        // real drag: a programmatic scroll is re-pinned to the tail, and the
        // node's top edge sits under the floating header).
        repeat(6) {
            val shown = runCatching { composeRule.onAllNodes(childCard).onFirst().assertIsDisplayed() }.isSuccess
            if (!shown) {
                composeRule.onNode(hasTestTag(SessionTree.ThreadCopy.TEST_ID_CONVERSATION))
                    .performTouchInput { swipeDown(startY = centerY, endY = bottom - 200f) }
                flow.settle()
            }
        }
        flow.waitFor(childCard, SYNC_TIMEOUT)
        composeRule.onAllNodes(childCard).onFirst().performClick()
        // S-3: the relay's question is inline in the feed again (dock.json).
        flow.waitFor(hasTestTag("feed-question"), NETWORK_TIMEOUT)
        flow.settle(longer = true)
        flow.screenshot("sg_session")
        // Session → Threads: the session page is a page OF the thread (its
        // strip stays on screen), so one Back leaves the thread.
        Espresso.pressBack()
        flow.waitFor(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), NAV_TIMEOUT)
        flow.settle(longer = true)

        // --- Settings root: thread 58cb32df — the TEAM tab (chats.json
        // `shell`): the team, the team switch, its settings rows, then the
        // old root (Servers, General). Header matches keep ignoreCase so they
        // hold whichever case SectionHeader renders (sentence case since EXP-698).
        openTeamRoot()
        flow.waitFor(hasText("Servers", ignoreCase = true), NAV_TIMEOUT)
        flow.waitFor(hasText(TEAM_NAME), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_settings-root")

        // --- sg_getting-started: the block behind Settings' entry (EXP-1270).
        // The row shows while the demo team has open steps (the seed never
        // sets up a server), and opens the block in a bottom sheet.
        flow.waitFor(hasTestTag(GettingStartedCopy.ENTRY_TEST_ID), SYNC_TIMEOUT)
        tapClearOfBar(hasTestTag(GettingStartedCopy.ENTRY_TEST_ID))
        flow.waitFor(hasTestTag("getting-started-sheet"), NAV_TIMEOUT)
        flow.waitFor(hasTestTag(GettingStartedCopy.rowTestId(GettingStartedStepKey.Board)), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_getting-started")
        Espresso.pressBack()
        flow.waitForGone(hasTestTag("getting-started-sheet"), NAV_TIMEOUT)
        flow.settle(longer = true)

        // --- sg_settings-team: #EXP-1381 every Team row opens its OWN page
        // (settings-nav.json); the Boards page, anchored on the seeded board
        // it lists (iOS parity).
        tapClearOfBar(teamSettingsRow("Boards"))
        flow.waitFor(hasTestTag(SettingsNav.PAGE_TEST_ID), NAV_TIMEOUT)
        flow.waitFor(hasText(TEAM_BOARD_NAME, substring = true), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_settings-team")
        Espresso.closeSoftKeyboard()
        Espresso.pressBack()
        flow.waitFor(hasTestTag("team-tab"), NAV_TIMEOUT)
        flow.settle()

        // --- sg_settings-team-agent: the Team page's Agents row → Triage: its
        // device, account, schedule and the seeded instructions (fetched, so
        // they are the gate).
        tapClearOfBar(teamSettingsRow("Agents"))
        val settingsAgentRow = hasTestTag(SessionTree.ListCopy.TEST_ID_AGENT_ROW) and hasText(AGENT_NAME)
        flow.waitFor(settingsAgentRow, SYNC_TIMEOUT)
        tapClearOfBar(settingsAgentRow)
        flow.waitFor(hasTestTag(AgentSettings.TEST_ID_PAGE), NAV_TIMEOUT)
        flow.waitFor(hasTestTag("agent-settings-trigger"), SYNC_TIMEOUT)
        flow.waitFor(hasText(AGENT_INSTRUCTIONS_FRAGMENT, substring = true), NETWORK_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_settings-team-agent")
        Espresso.closeSoftKeyboard()
        Espresso.pressBack()
        flow.waitFor(settingsAgentRow, NAV_TIMEOUT)
        Espresso.closeSoftKeyboard()
        Espresso.pressBack()
        flow.waitFor(hasTestTag("team-tab"), NAV_TIMEOUT)
        flow.settle()

        // --- sg_settings-assistant (S-3, agent-settings.json `assistant`):
        // the Team page's Agents row → the Assistant (the agents list's first
        // row, the brand mark) = the viewer's OWN Assistant (seeded):
        // identity, Runs on, Check in (the gate), the Voice block; no
        // instructions (#EXP-1439). A detour: nothing is edited.
        if (ScreenshotFlow.anyRequested("sg_settings-assistant")) {
            tapClearOfBar(teamSettingsRow("Agents"))
            val assistantRow = hasTestTag(SessionTree.ListCopy.TEST_ID_AGENT_ROW) and
                hasText(AgentSettings.Assistant.NAME)
            flow.waitFor(assistantRow, SYNC_TIMEOUT)
            tapClearOfBar(assistantRow)
            flow.waitFor(hasTestTag(AgentSettings.TEST_ID_PAGE), NAV_TIMEOUT)
            flow.waitFor(hasTestTag("assistant-check-in"), SYNC_TIMEOUT)
            flow.settle(longer = true)
            flow.screenshot("sg_settings-assistant")
            Espresso.closeSoftKeyboard()
            Espresso.pressBack()
            flow.waitFor(assistantRow, NAV_TIMEOUT)
            Espresso.closeSoftKeyboard()
            Espresso.pressBack()
            flow.waitFor(hasTestTag("team-tab"), NAV_TIMEOUT)
            flow.settle()
        }

        // --- sg_settings-prompts / sg_settings-prompt (NEW-7): the Team page's
        // Prompts row (the band with the owner's New prompt pill over one row
        // per prompt), then New prompt: THE prompt page in create mode (web
        // `settings-prompt`), so neither shot needs a seeded prompt id.
        tapClearOfBar(teamSettingsRow("Prompts"))
        flow.waitFor(hasTestTag("prompts-new"), SYNC_TIMEOUT)
        composeRule.onNode(hasTestTag("prompts-new")).performScrollTo()
        flow.waitFor(hasTestTag("prompts-settings-list"), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_settings-prompts")
        tapClearOfBar(hasTestTag("prompts-new"))
        flow.waitFor(hasTestTag("prompt-page"), NAV_TIMEOUT)
        flow.waitFor(hasTestTag("prompt-edit-body"), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_settings-prompt")
        Espresso.closeSoftKeyboard()
        Espresso.pressBack()
        flow.waitFor(hasTestTag("prompts-new"), NAV_TIMEOUT)
        flow.settle()
        Espresso.closeSoftKeyboard()
        Espresso.pressBack()
        flow.waitFor(hasTestTag("team-tab"), NAV_TIMEOUT)

        // --- Account settings: there is no separate profile screen (EXP-311) —
        // the signed-in identity, sign out and delete account live on the
        // server row's detail. The row is titled by the SERVER with the email
        // below, so the email addresses it; "Sign out" only exists there.
        tapClearOfBar(hasText(DEMO_EMAIL, substring = true))
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
        ownsShot = { it in CAPTURE_SHOTS }
        assumeTrue(ScreenshotFlow.anyRequested(*CAPTURE_SHOTS.toTypedArray()))

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

        // --- sg_trace · sg_code-block: THE Trace (+ the Queue) and THE
        // CodeBlock (EXP-1324), the same specimens as the web islands, drawn
        // by the real primitives on a capture-only screen.
        show(TraceSpecimenKind.Trace.testId) { TraceSpecimenScreen(TraceSpecimenKind.Trace) }
        flow.screenshot("sg_trace")
        show(TraceSpecimenKind.CodeBlock.testId) { TraceSpecimenScreen(TraceSpecimenKind.CodeBlock) }
        flow.screenshot("sg_code-block")

        // --- sg_ask-dock: THE Dock (dock.json + ask-dock.json) — one waiting
        // plan, the pile, the pile fanned, the plan's sheet — drawn by the
        // real DockCard / DockPile / DockSheetSpecimenFrame on a capture-only
        // screen.
        show(AskDockSpecimenScreenTestId) { AskDockSpecimenScreen() }
        flow.screenshot("sg_ask-dock")

        // --- sg_dock-card: THE DockCard primitive (dock.json), the web
        // entry's rows: one card, a banner with its action, the pile, the
        // fan, the sheet frame.
        show(DockCardSpecimenScreenTestId) { DockCardSpecimenScreen() }
        flow.screenshot("sg_dock-card")

        // --- sg_plan-card: THE rich plan / question card (EXP-1280,
        // plan-card.json): session-thread.json's rich case — the pending plan
        // (pictures, a tile question, a list question, the inline eUI
        // surface) and the answered multi question — by the real ThreadAskCard.
        showThreadCards(ThreadCardsKind.PlanCard)
        flow.screenshot("sg_plan-card")

        // --- sg_session-xref: THE inter-session link (S-3) in its four
        // kinds, by the real SessionXref.
        showThreadCards(ThreadCardsKind.SessionXref)
        flow.screenshot("sg_session-xref")

        // --- Thread 58cb32df (fixtures/chats.json): the Chats screen over
        // the phone tab bar, the tab bar's states, a chat's transcript as
        // phone bubbles (a superseded plan, an agent's surface card, the
        // /compact and /new dividers), the chat + agent menus with the
        // delete confirm, and the schedule picker's `minutes` repeat — the
        // styleguide entries chat-list, phone-tab-bar, chat-transcript,
        // chat-menus and schedule-picker, by the real composables.
        show(ChatListSpecimenTestId) { ChatListSpecimenScreen() }
        flow.screenshot("sg_chat-list")
        show(PhoneTabBarSpecimenTestId) { PhoneTabBarSpecimenScreen() }
        flow.screenshot("sg_phone-tab-bar")
        // #EXP-1384: the board menu open over its strip, the page header.
        show(PhoneBoardMenuSpecimenTestId) { PhoneBoardMenuSpecimenScreen() }
        flow.screenshot("sg_phone-board-menu")
        show(PhonePageHeaderSpecimenTestId) { PhonePageHeaderSpecimenScreen() }
        flow.screenshot("sg_phone-page-header")
        // --- sg_app-consent (VAPP-120, app-actions.json): the consent card an
        // app raises for an ungranted typed action, worded for the operation,
        // drawn by the real card over a stand-in surface (the painted
        // surface's rows carry no text semantics, so no live walk).
        show(AppConsentSpecimenTestId) { AppConsentSpecimenScreen() }
        // The card's title: the operation's own words (app-actions.json).
        flow.waitFor(hasText("Allow this surface to comment on issue APP-15?"), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_app-consent")
        show(ChatTranscriptSpecimenTestId) { ChatTranscriptSpecimenScreen() }
        flow.screenshot("sg_chat-transcript")
        show(ChatMenusSpecimenTestId) { ChatMenusSpecimenScreen() }
        flow.screenshot("sg_chat-menus")
        // #EXP-1388: the Chats list selecting (the Archived entry first, two
        // rows selected) over THE bar in the tab bar's slot.
        show(ChatSelectionSpecimenTestId) { ChatSelectionSpecimenScreen() }
        flow.screenshot("sg_chat-selection")
        // EXP-1459: sealed content this install cannot open (yet): the
        // transcript's lock pills + the list's locked previews.
        show(SealedLockedSpecimenTestId) { SealedLockedSpecimenScreen() }
        flow.screenshot("sg_sealed-locked")
        show(SchedulePickerSpecimenTestId) { SchedulePickerSpecimenScreen() }
        flow.screenshot("sg_schedule-picker")
        // Thread e4a04d22: `@email` as the member's pill in a chat, an
        // unknown address as plain text (styleguide entry mention-pill).
        show(MentionPillSpecimenTestId) { MentionPillSpecimenScreen() }
        flow.screenshot("sg_mention-pill")
        // --- sg_voice-call (NEW-69 / S-3, voice-call.json): THE phone-call
        // screen the live call draws, once per `surfaceSpecimens` row.
        show(VoiceCallSpecimenTestId) { VoiceCallSpecimenScreen() }
        flow.screenshot("sg_voice-call")
        // --- sg_call-controls (S-3): the round icon-only call buttons.
        show(CallControlsSpecimenTestId) { CallControlsSpecimenScreen() }
        flow.screenshot("sg_call-controls")
        // --- sg_device-readiness (EXP-1375, device-doctor.json): the low-disk
        // case's block and the composer's low-disk row.
        show(DeviceReadinessSpecimenTestId) { DeviceReadinessSpecimenScreen() }
        flow.screenshot("sg_device-readiness")
        // --- sg_story-ring · sg_story-viewer (S-3, stories.json): the ring
        // row over static rings (the Assistant's, a waiting chat, an unseen
        // result, a seen one) + single rings + THE progress, and THE viewer
        // on a draft question (its eUI surface, two choices; timer frozen).
        show(StoryRingSpecimenTestId) { StoryRingSpecimenScreen() }
        flow.screenshot("sg_story-ring")
        show(StoryViewerSpecimenTestId) { StoryViewerSpecimenScreen() }
        flow.screenshot("sg_story-viewer")
        // --- sg_story-tab (stories.json `tab.specimen`): THE Stories tab
        // body over tabCases[1]'s ring, the phone size, no PR block.
        show(StoryTabSpecimenTestId) { StoryTabSpecimenScreen() }
        flow.screenshot("sg_story-tab")
        // --- sg_plan-page (#EXP-1493, plan-page.json `specimen`): THE plan
        // page, phone: the document from its top, the DockCard at the foot.
        show(PlanPageSpecimenTestId) { PlanPageSpecimenScreen() }
        flow.screenshot("sg_plan-page")

        finished = true
    }

    /** Any composer issue chip: the tag carries the IDENTIFIER, so a prefix match. */
    private fun hasIssueChip(): SemanticsMatcher =
        SemanticsMatcher("has an agent-composer issue chip") { node ->
            node.config.getOrNull(SemanticsProperties.TestTag)
                ?.startsWith("agent-composer-chip-issue-") == true
        }

    /**
     * #EXP-1384: the board menu → the named board. On the board view the
     * Board tab opens the menu; its rows carry the board name as plain text.
     */
    private fun switchBoard(name: String) {
        composeRule.onNode(hasTestTag(Chats.TestIds.TAB_BOARD)).performClick()
        flow.waitFor(hasTestTag(Chats.TestIds.BOARD_MENU), NAV_TIMEOUT)
        val row = hasTestTag(Chats.TestIds.BOARD_MENU_ROW) and hasText(name, substring = true)
        flow.waitFor(row, SYNC_TIMEOUT)
        composeRule.onAllNodes(row).onFirst().performClick()
        flow.waitForGone(hasTestTag(Chats.TestIds.BOARD_MENU), NAV_TIMEOUT)
        flow.settle(longer = true)
    }

    /** A Team-page settings row (settings-nav.json) by its label. */
    private fun teamSettingsRow(label: String): SemanticsMatcher =
        hasTestTag(SettingsNav.ROW_TEST_ID) and (hasText(label) or hasAnyDescendant(hasText(label)))

    /**
     * Tap a row on a page under the floating tab bar: `performClick` touches the
     * node's centre, so a row scrolled only just into view (under the bar) would
     * hit the bar instead. Scroll it in, then drag the page up by the overlap.
     */
    private fun tapClearOfBar(matcher: SemanticsMatcher) {
        val node = composeRule.onAllNodes(matcher).onFirst()
        node.performScrollTo()
        flow.settle()
        val bar = composeRule.onAllNodes(hasTestTag(Chats.TestIds.TAB_TEAM)).fetchSemanticsNodes()
        if (bar.isNotEmpty()) {
            // The tab sits inside the capsule; keep a margin above the capsule.
            val clearLine = bar.first().boundsInRoot.top - 48f
            val bottom = node.fetchSemanticsNode().boundsInRoot.bottom
            if (bottom > clearLine) {
                val dy = bottom - clearLine + 96f
                composeRule.onRoot().performTouchInput {
                    swipe(Offset(centerX, centerY), Offset(centerX, centerY - dy), durationMillis = 600)
                }
                flow.settle()
            }
        }
        node.performClick()
    }

    /**
     * The Team tab keeps its own stack: a page pushed from it earlier (Devices)
     * is what the tab shows again, so pop back to the Team page itself.
     */
    private fun openTeamRoot() {
        composeRule.onNode(hasTestTag(Chats.TestIds.TAB_TEAM)).performClick()
        flow.settle()
        repeat(3) {
            if (!flow.exists(hasTestTag("team-tab"))) {
                Espresso.pressBack()
                flow.settle()
            }
        }
        flow.waitFor(hasTestTag("team-tab"), NAV_TIMEOUT)
    }

    /** #EXP-1384: Devices = the Team page's Devices row (settings-nav.json), pushed. */
    private fun openDevices() {
        openTeamRoot()
        val devices = Chats.Copy.HEADERS.getValue("devices")
        val row = hasTestTag(SettingsNav.ROW_TEST_ID) and
            (hasText(devices) or hasAnyDescendant(hasText(devices)))
        composeRule.onAllNodes(row).onFirst().performScrollTo().performClick()
        flow.waitFor(hasTestTag(SettingsNav.PAGE_TEST_ID), NAV_TIMEOUT)
    }
}
