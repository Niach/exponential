package com.exponential.app

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.performSemanticsAction
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
import androidx.compose.ui.test.onLast
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.isRoot
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
import com.exponential.app.data.steer.STEER_NO_FEED_IDLE_TITLE
import com.exponential.app.domain.DevicesList
import com.exponential.app.domain.AgentHome
import com.exponential.app.domain.AppExport
import com.exponential.app.domain.AppRecords
import com.exponential.app.domain.Apps
import com.exponential.app.domain.AgentSettings
import com.exponential.app.domain.ComposerAgentRow
import com.exponential.app.domain.ComposerMenu
import com.exponential.app.domain.ComposerMenuRowId
import com.exponential.app.domain.SessionScreen
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.ResultsTab
import com.exponential.app.domain.SearchList
import androidx.activity.compose.setContent
import com.exponential.app.ui.gettingstarted.GettingStartedCopy
import com.exponential.app.ui.gettingstarted.GettingStartedStepKey
import com.exponential.app.ui.styleguide.ComposerAddFileSpecimenScreen
import com.exponential.app.ui.styleguide.ComposerAddFileSpecimenTestId
import com.exponential.app.ui.styleguide.FeedbackStatesScreen
import com.exponential.app.ui.styleguide.FeedbackStatesScreenTestId
import com.exponential.app.ui.styleguide.ShareComposerSpecimenScreen
import com.exponential.app.ui.styleguide.ShareComposerSpecimenTestId
import com.exponential.app.ui.styleguide.ShareToSpecimenScreen
import com.exponential.app.ui.styleguide.ShareToSpecimenTestId
import com.exponential.app.ui.styleguide.ChatCardsKind
import com.exponential.app.ui.styleguide.AppConsentSpecimenScreen
import com.exponential.app.ui.styleguide.AppConsentSpecimenTestId
import com.exponential.app.ui.styleguide.AskDockSpecimenScreen
import com.exponential.app.ui.styleguide.AskDockSpecimenScreenTestId
import com.exponential.app.ui.styleguide.DockCardSpecimenScreen
import com.exponential.app.ui.styleguide.DockCardSpecimenScreenTestId
import com.exponential.app.ui.styleguide.RecordFormFieldStatesSpecimenScreen
import com.exponential.app.ui.styleguide.RecordFormFieldStatesSpecimenTestId
import com.exponential.app.ui.styleguide.ChatCardsScreen
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
import com.exponential.app.ui.styleguide.CallPillSpecimenScreen
import com.exponential.app.ui.styleguide.CallPillSpecimenTestId
import com.exponential.app.ui.styleguide.VoiceCallSpecimenScreen
import com.exponential.app.ui.styleguide.VoiceCallSpecimenTestId
import com.exponential.app.ui.styleguide.DeviceReadinessSpecimenScreen
import com.exponential.app.ui.styleguide.DeviceReadinessSpecimenTestId
import com.exponential.app.ui.styleguide.PhoneTabBarSpecimenScreen
import com.exponential.app.domain.SettingsNav
import com.exponential.app.domain.Skills
import com.exponential.app.ui.styleguide.PhonePageHeaderSpecimenTestId
import com.exponential.app.ui.styleguide.PhonePageHeaderSpecimenScreen
import com.exponential.app.ui.styleguide.ChatHeaderSpecimenTestId
import com.exponential.app.ui.styleguide.ChatHeaderSpecimenScreen
import com.exponential.app.ui.styleguide.PhoneBoardMenuSpecimenTestId
import com.exponential.app.ui.styleguide.DeviceAttentionSpecimenScreen
import com.exponential.app.ui.styleguide.DeviceAttentionSpecimenTestId
import com.exponential.app.ui.styleguide.OsLiveActivityChipSpecimenScreen
import com.exponential.app.ui.styleguide.OsLiveActivityChipSpecimenTestId
import com.exponential.app.ui.styleguide.OsLiveActivitySpecimenScreen
import com.exponential.app.ui.styleguide.OsLiveActivitySpecimenTestId
import com.exponential.app.ui.styleguide.OsNotificationActionsSpecimenScreen
import com.exponential.app.ui.styleguide.OsNotificationActionsSpecimenTestId
import com.exponential.app.ui.styleguide.TeamAvatarSpecimenScreen
import com.exponential.app.ui.styleguide.TeamAvatarSpecimenTestId
import com.exponential.app.ui.styleguide.PhoneBoardMenuSpecimenScreen
import com.exponential.app.ui.styleguide.PhoneTabBarSpecimenTestId
import com.exponential.app.ui.styleguide.SchedulePickerSpecimenScreen
import com.exponential.app.ui.styleguide.SchedulePickerSpecimenTestId
import com.exponential.app.ui.styleguide.NeedsYouSpecimenScreen
import com.exponential.app.ui.styleguide.NeedsYouSpecimenTestId
import com.exponential.app.ui.styleguide.ResultsTabSpecimenScreen
import com.exponential.app.ui.styleguide.ResultsTabSpecimenTestId
import com.exponential.app.ui.styleguide.PlanPageSpecimenScreen
import com.exponential.app.ui.styleguide.PlanPageSpecimenTestId
import com.exponential.app.ui.styleguide.TraceSpecimenKind
import com.exponential.app.ui.styleguide.TraceSpecimenScreen
import com.exponential.app.ui.theme.ExponentialTheme
import com.exponential.app.domain.IssueDraftPage
import com.exponential.app.domain.Meeting
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
 *   sg_sign-in ·
 *   sg_board-empty ·
 *   sg_board-bulk-edit · sg_issue-comments · sg_issue-reporter-comments · sg_issue-properties ·
 *   sg_issue-create · sg_search · sg_my-issues · sg_agents ·
 *   sg_chat · sg_composer-menu · sg_chat-issues ·
 *   sg_machine-settings · sg_chats ·
 *   sg_session-row · sg_pr-row · sg_pr-checks ·
 *   sg_settings-root · sg_settings-team ·
 *   sg_settings-account · sg_onboarding · sg_onboarding-create-team ·
 *   sg_invite-accept · sg_onboarding-invite ·
 *   sg_onboarding-devices · sg_agent-home · sg_search-list ·
 *   sg_getting-started · sg_feedback-states · sg_work-card ·
 *   sg_script-card · sg_comment-call-card · sg_chat-screen · sg_agent-channel ·
 *   sg_session · sg_trace · sg_code-block · sg_ask-dock · sg_dock-card · sg_record-form-field-states · sg_plan-card ·
 *   sg_settings-agents · sg_settings-team-agent · sg_settings-prompts · sg_settings-prompt ·
 *   sg_settings-skills · sg_settings-skill ·
 *   sg_app-record-table · sg_app-record-form · sg_app-calendar · sg_app-timeline · sg_app-connect ·
 *   sg_app-publish · sg_app-versions-sweep · sg_app-version-preview · sg_app-record-history ·
 *   sg_app-record-undo · sg_app-csv-import · sg_app-settings-links · sg_app-settings-automations ·
 *   sg_device-attention · sg_os-live-activity · sg_os-live-activity-island · sg_os-notification-actions ·
 *   sg_apps · sg_app-run (VAPP-242) · sg_app · sg_chat-list · sg_phone-tab-bar · sg_chat-transcript ·
 *   sg_chat-menus · sg_chat-selection · sg_sealed-locked · sg_schedule-picker · sg_mention-pill · sg_app-consent ·
 *   sg_app-export · sg_app-import · sg_app-draft · sg_app-versions · sg_app-settings · sg_app-data · sg_voice-call · sg_call-controls · sg_call-pill · sg_settings-assistant · sg_device-readiness ·
 *   sg_chat-results · sg_plan-page · sg_needs-you · sg_results-tab ·
 *   sg_add-server · sg_drafts · sg_composer-slash-menu · sg_session-slash-menu ·
 *   sg_settings-assistant-meeting · sg_settings-general · sg_settings-members ·
 *   sg_settings-labels · sg_settings-repositories · sg_onboarding-join (EXP-1558) ·
 *   sg_repo (#EXP-1616)
 *
 * EXP-1295 retired `sg_chat-action` and `sg_action-create` (the builtin
 * actions and the action create sheet left the catalog), renamed the
 * Chats tab `sg_chats` and the agent channel `sg_agent-channel`;
 * `sg_pr-row` = the capture-only PR row specimen (EXP-1553). S-3 retired `sg_guide`
 * and `sg_guide-section` with the Guide tab (the Results tab replaced it).
 *
 * EXP-1296 split the suite in two tests. [captureStyleguideScreenshots]
 * walks the live app against the seeded backend; [captureOnlyScreens] needs NO
 * server and NO sign-in: the specimens with no seed counterpart (web and
 * desktop photograph styleguide islands for them) are drawn by their real
 * composables over fixture data (ui/styleguide) and set as the activity's
 * content — the feedback states and the chat cards. A scoped run (`shots:`) naming only capture
 * shots skips the live walk.
 *
 * EXP-1309: every session-tree surface WITH a seed counterpart is walked live,
 * so it reads the same seed web, desktop and iOS photograph and cannot drift:
 * sg_agent-home (the landing, the bottom bar over it), sg_composer-menu +
 * sg_chat-issues (the home composer's "+" and Implement issue), sg_chats
 * (the Chats tab), sg_agent-channel (Triage), sg_chat-screen (the cold-start
 * chat), sg_session ("Profile the launch path", its pending question in
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
            // EXP-1654: sg_work-card + sg_script-card left for the live
            // session walk (in the transcript, as web and desktop shoot them).
            "sg_feedback-states", "sg_comment-call-card",
            "sg_trace", "sg_code-block", "sg_ask-dock", "sg_dock-card", "sg_plan-card",
            "sg_record-form-field-states",
            // EXP-1635: the share flow's two steps + the + menu's sub-choice.
            "sg_share-targets", "sg_share-composer", "sg_composer-add-file",
            // EXP-1553: THE PR row (a tree, a stack, an unlinked PR).
            "sg_pr-row",
            // Session 20b7fdc2: THE session row (web's `session-row` island).
            "sg_session-row",
            // #EXP-1611: the Merge pill's check states + the Checks submenu.
            "sg_pr-checks",
            // Session 58cb32df: the Chats pieces + the minutes schedule.
            // #EXP-1645: the team mark and the General row.
            "sg_team-avatar",
            // #EXP-1643: the device cards' attention badge + THE account panel.
            "sg_device-attention",
            "sg_chat-list", "sg_phone-tab-bar", "sg_phone-board-menu", "sg_phone-page-header", "sg_chat-header", "sg_chat-transcript", "sg_chat-menus", "sg_schedule-picker",
            // #EXP-1388: the Chats multi-selection + the Archived entry.
            "sg_chat-selection",
            // EXP-1459: the locked state for sealed content.
            "sg_sealed-locked",
            // Session e4a04d22: the member pill in a chat.
            "sg_mention-pill",
            // VAPP-120: the consent card with a typed action's words.
            "sg_app-consent",
            // NEW-69 / S-3: the phone-call screen's states, and its round buttons.
            "sg_voice-call",
            "sg_call-controls",
            // EXP-1681: THE floating call pill a minimized call draws.
            "sg_call-pill",
            // EXP-1375: the Storage band + the composer's low-disk row.
            "sg_device-readiness",
            // #EXP-1493: THE plan page (plan-page.json `specimen`, phone).
            "sg_plan-page",
            // #EXP-1639 / #EXP-1641: the OS-drawn surfaces, mocked over the real models.
            "sg_os-live-activity", "sg_os-live-activity-island", "sg_os-notification-actions",
            // #FEED-191: THE Needs you entry + list, THE Results tab body.
            "sg_needs-you", "sg_results-tab",
        )

        /** #EXP-1384: the Board tab's root is up (its All issues chip). */
        private val BOARD_ROOT_TAG = "${Chats.TestIds.BOARD_CHIP}-all"

        /** VAPP-122: the seeded Working hours app, with records (web `DEMO_APP_SLUG`). */
        private const val DEMO_APP_SLUG = "working-hours"

        /** VAPP-176: the seeded Release checklist app, a DRAFT (no version, no
         *  editor session; Jonas's, so leaving it never discards it). */
        private const val DEMO_DRAFT_APP_SLUG = "release-checklist"

        /** The walk's rich-apps step takes these. */
        private val APP_RICH_SHOTS = arrayOf(
            "sg_app-record-table", "sg_app-record-form", "sg_app-calendar", "sg_app-timeline", "sg_app-connect",
            "sg_app-settings-links", "sg_app-settings-automations", "sg_app-publish", "sg_app-versions-sweep",
            "sg_app-version-preview", "sg_app-csv-import", "sg_app-record-history", "sg_app-record-undo",
        )

        /** The CSV import shot: two good rows, one with a stage the schema refuses. */
        private const val APP_CSV_CAPTURE_TEXT =
            "name,stage,domain\nInitech,lead,initech.example\nUmbrella,active,umbrella.example\nBad,nosuch,x"

        /** VAPP-121: app-export.json `capture.importText` and its one issue. */
        private const val APP_IMPORT_CAPTURE_TEXT = """{"format":"exponential-app/1","app":{"name":"Todos"}}"""
        private const val APP_IMPORT_CAPTURE_ISSUE = "/package: is required"

        // APP-5 — the only seeded issue with a comment list.
        private const val SHOWCASE_ISSUE_TITLE = ScreenshotFlow.SHOWCASE_ISSUE_TITLE

        // A fragment of APP-5's third comment (the demo user's own). Scrolling
        // to it lands the newest comments on screen; matching it also proves
        // the chat actually synced instead of photographing a bare header.
        private const val SHOWCASE_COMMENT_FRAGMENT = "Deferral PR is merged"
        /** #EXP-1754: the widget-filed issue (APP-19) and its deleted member's note. */
        private const val WIDGET_ISSUE_TITLE = "Can't sign in on the iPad app"
        private const val WIDGET_COMMENT_FRAGMENT = "Same spinner on an iPad Air here"

        // EXP-642: the same query the web and desktop `search` recipes type, so
        // the three shots of that view compare like for like.
        private const val SEARCH_QUERY = "by:alex"
        private const val SEARCH_RESULT_TITLE = SHOWCASE_ISSUE_TITLE

        // APP-14: assigned to the demo user and, unlike APP-3 / APP-6, NOT
        // quoted by any seeded notification — so matching it proves the My
        // Issues segment is showing, not the Inbox one.
        private const val MY_ISSUE_TITLE = "Group board issues by assignee"

        // APP-14: the seed's realPr issue (its open PR row). Addressed by
        // row tag so the lazy board list can scroll to it.
        private const val GUIDE_ISSUE_ROW = "issue-row-APP-14"

        // APP-15: its chat carries the seeded quick replies.
        private const val BATCH_GUIDE_ISSUE_ROW = "issue-row-APP-15"

        private const val TEAM_NAME = "Acme"
        private const val TEAM_BOARD_NAME = "Mobile App"

        // The seed's SECOND board: created empty on purpose, so the "no issues
        // yet" state is photographable without deleting anything (EXP-642).
        private const val EMPTY_BOARD_NAME = "Launch Marketing"

        // The bulk-edit selection. EXP-1558: web's `openBoardBulkEdit` picks
        // the board's first three rows (the Backlog's APP-6, APP-7, APP-9).
        // Addressed by row tag so the lazy list can scroll to them.
        private val BULK_ROWS = listOf("issue-row-APP-6", "issue-row-APP-7", "issue-row-APP-9")

        // EXP-825: the two codeable issues the sg_chat-issues shot chips —
        // APP-3 and APP-6, the same pair the web `chat-issues` recipe seeds
        // (apps/web/scripts/lib/demo-ids.ts), neither with a running session.
        // Picked through Implement issue's search list by a title prefix, so
        // the field's own text never matches the row's full title.
        private const val CHAT_FIRST_ISSUE_TITLE = "Dark mode contrast pass across settings"
        private const val CHAT_SECOND_ISSUE_TITLE = "Push notification deep links open the wrong tab"

        // EXP-1551: the chips the shot must show, in pick order (web: APP-3
        // then APP-6, `demoIds.issueA` / `issueB`).
        private val CHAT_ISSUE_IDENTS = listOf("APP-3", "APP-6")

        // EXP-1309: the seed's session tree (seedSessionTree in
        // apps/web/scripts/seed-screenshots.ts). The private running root
        // (the home's Running band and the Chats tab), the ended one
        // (Chats' Recent), the cold-start chat and its child session.
        // EXP-1341: a chat with linked issues is titled `#<first> <title>`
        // (` + <n-1>` with more, session-title.json `titles`), matched on the
        // issue title; the issue-less private spike keeps its stored title.
        private const val PRIVATE_CHAT_TITLE = "Spike: offline queue"
        private const val RECENT_CHAT_TITLE = "Fix flaky scroll restore on the issue list"
        private const val COLD_START_CHAT_TITLE = "Reduce cold start below 800 ms"
        private const val COLD_START_REPLY_FRAGMENT = "Board snapshot cache"
        private const val CHILD_SESSION_TITLE = "Profile the launch path"

        // The Triage agent: its chat and its settings page (seeded
        // instructions).
        private const val AGENT_NAME = "Triage"

        /** #EXP-1735: the seeded lead Assistant's member (the demo user). */
        private const val LEAD_NAME = "Alex Carter"
        private const val AGENT_INSTRUCTIONS_FRAGMENT = "Look after the boards"

        // EXP-1558: the seed's first team prompt (seed-screenshots.ts), the
        // gate for both `/` menus.
        private const val SLASH_PROMPT_NAME = "Release train"

        // EXP-1558: web's `typeIssueRef` token after `Explain #`: the seeded
        // board's key, so every issue matches.
        private const val ISSUE_REF_TOKEN = "APP"

        // The device `bun run screenshots:desktop` registers for the demo user.
        private const val DEMO_DEVICE_NAME = "Alex's MacBook Pro"

        private const val DEMO_EMAIL = ScreenshotFlow.DEMO_EMAIL

        private const val NAV_TIMEOUT = ScreenshotFlow.NAV_TIMEOUT
        private const val SYNC_TIMEOUT = ScreenshotFlow.SYNC_TIMEOUT
        private const val FIRST_SYNC_TIMEOUT = ScreenshotFlow.FIRST_SYNC_TIMEOUT

        /** EXP-1654: the board gate's hard ceiling while the first sync still moves. */
        private const val FIRST_SYNC_CEILING_MS = 180_000L

        /** EXP-1654: no new issue row for this long (and no Syncing pill) = settled. */
        private const val FIRST_SYNC_STALL_MS = 15_000L
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

    /** EXP-1558: the walk's [step]s that failed twice, failed at the walk's end. */
    private val failedSteps = mutableListOf<String>()

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

        // EXP-1558: from here on the walk is a run of [step]s, each starting
        // from a tab: a step that fails is retried once, then recorded, and
        // the walk carries on, so one flaky screen no longer costs every shot
        // after it (the lane keeps the shots taken; the test still fails).
        // (Session 20b7fdc2: sg_session-row is a capture-only specimen now.)

        // The app lands on the Agent tab; the board shots start on Issues.
        // --- Board: wait out session fetch + first Electric sync, then let the
        // transient "Syncing…" pill clear (it photobombed a board shot once —
        // EXP-348).
        //
        // EXP-1654: this is the walk's FIRST-SYNC gate, and every board-chain
        // step after it leans on it (their own waits are SYNC_TIMEOUT). A
        // fixed 60 s failed twice with a parallel lane loading the shared
        // backend, so [waitForFirstBoardSync] keeps waiting while the sync
        // visibly progresses and fails fast once it settled without APP-5.
        // The retry re-enters through the board root: a second Board-tab tap
        // on the board view opens the board menu instead.
        step("board") {
            flow.waitFor(hasTestTag(Chats.TestIds.TAB_BOARD), SYNC_TIMEOUT)
            // DISPLAYED, not merely composed: the walk arrives here from the
            // Chats tab, where the board's chips can still sit in the tree.
            val onBoardRoot = runCatching {
                composeRule.onAllNodes(hasTestTag(BOARD_ROOT_TAG)).onFirst().assertIsDisplayed()
            }.isSuccess
            if (!onBoardRoot) {
                composeRule.onNode(hasTestTag(Chats.TestIds.TAB_BOARD)).performClick()
                flow.settle()
            }
            if (flow.exists(hasTestTag(Chats.TestIds.BOARD_MENU))) {
                Espresso.pressBack()
                flow.waitForGone(hasTestTag(Chats.TestIds.BOARD_MENU), NAV_TIMEOUT)
            }
            waitForFirstBoardSync()
            flow.waitForGone(hasText("Syncing", substring = true), FIRST_SYNC_TIMEOUT)
            flow.settle()
        }

        // --- Board empty state: the seed's second board is created empty for
        // exactly this shot, so nothing has to be deleted to reach the state.
        step("board-empty") {
            openBoardRoot()
            switchBoard(EMPTY_BOARD_NAME)
            flow.waitFor(hasText("No issues yet"), SYNC_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_board-empty")
            switchBoard(TEAM_BOARD_NAME)
            flow.waitFor(hasText(SHOWCASE_ISSUE_TITLE), SYNC_TIMEOUT)
            flow.settle()
        }

        // --- Bulk edit: a long press enters multi-select, a plain tap on a
        // second row adds it. Both rows sit in the backlog group at the bottom
        // of a LAZY list, so scroll the list to each by row tag first — an
        // off-screen lazy row is not composed and has no node to address.
        // EXP-1558: web's selection, the board's first three rows.
        step("bulk-edit") {
            openBoardRoot()
            val (firstRow, otherRows) = BULK_ROWS.first() to BULK_ROWS.drop(1)
            composeRule.onNode(hasTestTag("issue-list")).performScrollToIndex(0)
            composeRule.onNode(hasTestTag("issue-list"))
                .performScrollToNode(hasTestTag(firstRow))
            composeRule.onNode(hasTestTag(firstRow)).performTouchInput { longClick() }
            flow.waitFor(hasTestTag("bulk-selection-bar"), NAV_TIMEOUT)
            otherRows.forEach { row ->
                composeRule.onNode(hasTestTag("issue-list")).performScrollToNode(hasTestTag(row))
                composeRule.onNode(hasTestTag(row)).performClick()
            }
            flow.waitFor(
                hasText("${BULK_ROWS.size}") and hasAnyAncestor(hasTestTag("bulk-selection-bar")),
                NAV_TIMEOUT,
            )
            composeRule.onNode(hasTestTag("issue-list")).performScrollToIndex(0)
            flow.settle()
            flow.screenshot("sg_board-bulk-edit")
            // Leave selection mode — every later shot assumes the plain list.
            composeRule.onNode(hasContentDescription("Clear selection")).performClick()
            flow.waitForGone(hasTestTag("bulk-selection-bar"), NAV_TIMEOUT)
            flow.settle()
        }

        // --- Issue comments: APP-5 is the only seeded chat. The detail body
        // is a plain verticalScroll Column (not lazy), so every timeline row is
        // composed and performScrollTo reaches it. `comment-replies-header` is
        // the "Activity" label, the deliberate hook mirroring the iOS
        // accessibility id.
        step("issue-comments") {
            openBoardRoot()
            composeRule.onAllNodes(hasText(SHOWCASE_ISSUE_TITLE)).onFirst().performClick()
            flow.waitFor(hasText("Startup profiling", substring = true), NAV_TIMEOUT)
            // EXP-1302: the session-tree redesign dropped the Work screen's face strip (the
            // issue names its chats instead), so the page has no Run tab to
            // wait for.
            flow.waitFor(hasTestTag("comment-replies-header"), SYNC_TIMEOUT)
            flow.waitFor(hasText(SHOWCASE_COMMENT_FRAGMENT, substring = true), SYNC_TIMEOUT)
            // --- Issue properties (EXP-1676: BEFORE the comments scroll: the page at
            // its top behind the sheet, as web-mobile): still on APP-5. The bottom bar's leading circle
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

            composeRule.onNode(hasTestTag("comment-replies-header")).performScrollTo()
            composeRule.onAllNodes(hasText(SHOWCASE_COMMENT_FRAGMENT, substring = true))
                .onFirst()
                .performScrollTo()
            // EXP-1558: web's frame is the END of the page's scroller (the newest
            // comments over the reply bar), an absolute point every run lands on;
            // drag the page to its end the same way.
            repeat(6) {
                composeRule.onRoot().performTouchInput {
                    swipeUp(startY = height * 0.7f, endY = height * 0.2f, durationMillis = 300)
                }
                flow.pump()
            }
            flow.settle()
            flow.screenshot("sg_issue-comments")

            composeRule.onNode(hasContentDescription("Back")).performClick()
            flow.waitFor(hasTestTag(BOARD_ROOT_TAG), NAV_TIMEOUT)
            flow.settle()
        }

        // --- #EXP-1754: APP-19, the widget-filed issue, at its comments: the
        // reporter's reply in the hue of their name, a deleted member's note grey.
        step("issue-reporter-comments") {
            openBoardRoot()
            composeRule.onAllNodes(hasText(WIDGET_ISSUE_TITLE)).onFirst().performScrollTo().performClick()
            flow.waitFor(hasTestTag("comment-replies-header"), SYNC_TIMEOUT)
            flow.waitFor(hasText(WIDGET_COMMENT_FRAGMENT, substring = true), SYNC_TIMEOUT)
            composeRule.onNode(hasTestTag("comment-replies-header")).performScrollTo()
            repeat(6) {
                composeRule.onRoot().performTouchInput {
                    swipeUp(startY = height * 0.7f, endY = height * 0.2f, durationMillis = 300)
                }
                flow.pump()
            }
            flow.settle()
            flow.screenshot("sg_issue-reporter-comments")

            composeRule.onNode(hasContentDescription("Back")).performClick()
            flow.waitFor(hasTestTag(BOARD_ROOT_TAG), NAV_TIMEOUT)
            flow.settle()
        }

        // --- Create issue: the compose circle opens the New issue PAGE
        // (EXP-1170), left via its header's "Back". This shot types NOTHING
        // into it, so leaving writes no draft row.
        step("issue-create") {
            openBoardRoot()
            composeRule.onNode(hasContentDescription("New issue")).performClick()
            flow.waitFor(hasTestTag("create-issue-title-field"), NAV_TIMEOUT)
            flow.waitFor(hasText(IssueDraftPage.HEADER), NAV_TIMEOUT)
            flow.settle()
            // EXP-1676: the keyboard down, the state the other clients photograph.
            Espresso.closeSoftKeyboard()
            flow.settle()
            flow.screenshot("sg_issue-create")
            composeRule.onNode(hasContentDescription("Back")).performClick()
            flow.waitFor(hasTestTag(BOARD_ROOT_TAG), NAV_TIMEOUT)
            flow.settle()
        }

        // EXP-1558: the Drafts chip (web-mobile's `?chip=drafts`): the demo
        // user's seeded drafts, the titled one and the untitled one.
        step("drafts") {
            openBoardRoot()
            if (ScreenshotFlow.anyRequested("sg_drafts")) {
                val draftsChip = hasTestTag("${Chats.TestIds.BOARD_CHIP}-drafts")
                flow.waitFor(draftsChip, SYNC_TIMEOUT)
                composeRule.onNode(draftsChip).performClick()
                flow.waitFor(hasText(IssueDraftPage.UNTITLED), SYNC_TIMEOUT)
                flow.settle()
                flow.screenshot("sg_drafts")
                composeRule.onNode(hasTestTag(BOARD_ROOT_TAG)).performClick()
                flow.waitFor(hasText(SHOWCASE_ISSUE_TITLE), NAV_TIMEOUT)
                flow.settle()
            }
        }

        // --- Search: EXP-686 moved it off the bottom bar into the board
        // header, so it is a PUSHED screen now — tagged,
        // because "Search" also reads as the field's own placeholder. The
        // field is the only editable node on it, so hasSetTextAction
        // addresses it without a tag. Gate on a real hit — an unseeded
        // backend would render "No issues match".
        step("search") {
            openBoardRoot()
            composeRule.onNode(hasTestTag(Chats.TestIds.SEARCH_BUTTON)).performClick()
            // EXP-1649: no "Search" title any more (web-mobile's header):
            // the field itself is the gate.
            flow.waitFor(hasSetTextAction(), NAV_TIMEOUT)
            composeRule.onNode(hasSetTextAction()).performTextInput(SEARCH_QUERY)
            Espresso.closeSoftKeyboard()
            flow.waitFor(hasText(SEARCH_RESULT_TITLE, substring = true), SYNC_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_search")
            composeRule.onNode(hasContentDescription("Back")).performClick()
            flow.waitFor(hasTestTag(BOARD_ROOT_TAG), NAV_TIMEOUT)
            flow.settle()
        }

        // --- APP-14's issue page (its chat opened the seed's real PR), the way into
        // its chat's Results tab below. EXP-1553: `sg_pr-row` is the
        // capture-only PR row specimen now (with the chat cards).
        step("chat-results") {
            openBoardRoot()
            composeRule.onNode(hasTestTag("issue-list"))
                .performScrollToNode(hasTestTag(GUIDE_ISSUE_ROW))
            composeRule.onNode(hasTestTag(GUIDE_ISSUE_ROW)).performClick()
            // EXP-1647: the issue page draws no PR row (web EXP-1293); its
            // chat chip is the gate.
            flow.waitFor(hasTestTag("issue-chat-chip"), SYNC_TIMEOUT)
            flow.settle()

            // --- sg_chat-results (results-tab.json): APP-14's chat (web's `$diffChatId`) on its Results
            // tab, the strip's last, icon-only: the Stack band + rail, the Changes
            // row over the real PR's GitHub diff, then the seeded results. The
            // Changes row is the gate (the diff is the walk's one network dependency).
            if (ScreenshotFlow.isShotWanted("sg_chat-results")) {
                flow.waitFor(hasTestTag("issue-chat-chip"), SYNC_TIMEOUT)
                composeRule.onAllNodes(hasTestTag("issue-chat-chip")).onFirst().performScrollTo()
                composeRule.onAllNodes(hasTestTag("issue-chat-chip")).onFirst().performClick()
                // #EXP-1732: THE session screen's Results tab.
                val resultsTab = hasTestTag(SessionScreen.TestIds.tab(SessionScreen.Tab.Results))
                flow.waitFor(resultsTab, NAV_TIMEOUT)
                composeRule.onNode(resultsTab).performClick()
                flow.waitFor(hasTestTag(ResultsTab.Tab.TestIds.SECTION), SYNC_TIMEOUT)
                flow.waitFor(hasTestTag(ResultsTab.Tab.TestIds.STACK), SYNC_TIMEOUT)
                flow.waitFor(hasTestTag(ResultsTab.Tab.TestIds.CHANGES), NETWORK_TIMEOUT)
                flow.settle(longer = true)
                flow.screenshot("sg_chat-results")
                // Chat → the issue page.
                Espresso.pressBack()
                flow.waitForGone(hasTestTag(SessionScreen.TestIds.TABS), NAV_TIMEOUT)
                flow.waitFor(hasTestTag("issue-chat-chip"), NAV_TIMEOUT)
                flow.settle()
            }

            // Issue → board.
            composeRule.onNode(hasContentDescription("Back")).performClick()
            flow.waitFor(hasTestTag(BOARD_ROOT_TAG), NAV_TIMEOUT)
            flow.settle()
        }

        // --- sg_chat-replies: the quick replies (EXP-1211). APP-15's
        // chat again, left on its Conversation: the seeded `replies`
        // card's row over the composer capsule.
        // EXP-1558: then sg_session-slash-menu, web's `typeSlashInSessionComposer`
        // on the same chat (DEMO_GUIDE_CHAT_ID): `/` in its composer = the
        // builtins, then the team's prompts.
        step("chat-replies") {
            openBoardRoot()
            if (ScreenshotFlow.anyRequested("sg_chat-replies", "sg_session-slash-menu")) {
                composeRule.onNode(hasTestTag("issue-list"))
                    .performScrollToNode(hasTestTag(BATCH_GUIDE_ISSUE_ROW))
                composeRule.onNode(hasTestTag(BATCH_GUIDE_ISSUE_ROW)).performClick()
                flow.waitFor(hasTestTag("issue-chat-chip"), SYNC_TIMEOUT)
                composeRule.onAllNodes(hasTestTag("issue-chat-chip")).onFirst().performScrollTo()
                composeRule.onAllNodes(hasTestTag("issue-chat-chip")).onFirst().performClick()
                flow.waitFor(hasTestTag(SessionTree.RepliesCopy.TEST_ID), SYNC_TIMEOUT)
                flow.settle()
                flow.screenshot("sg_chat-replies")
                if (ScreenshotFlow.anyRequested("sg_session-slash-menu")) {
                    // #EXP-1732: the Chat tab = the root's session view (its composer).
                    composeRule.onAllNodes(hasTestTag("steer-composer-capsule")).onFirst().performClick()
                    val field = hasSetTextAction()
                    flow.waitFor(field, NAV_TIMEOUT)
                    composeRule.onAllNodes(field).onFirst().performTextInput("/")
                    flow.waitFor(hasText("/compact", substring = true), NAV_TIMEOUT)
                    // The team's prompts follow the builtins (the seed's prompts).
                    flow.waitFor(hasText(SLASH_PROMPT_NAME), SYNC_TIMEOUT)
                    // Web's frame has no keyboard: put it away (the menu
                    // follows the draft, so it stays).
                    Espresso.closeSoftKeyboard()
                    flow.waitFor(hasText("/compact", substring = true), NAV_TIMEOUT)
                    flow.settle()
                    flow.screenshot("sg_session-slash-menu")
                    composeRule.onAllNodes(field).onFirst().performTextReplacement("")
                    Espresso.closeSoftKeyboard()
                    flow.settle()
                }
                // Chat → issue → board.
                repeat(3) {
                    if (flow.exists(hasTestTag(SessionScreen.TestIds.TABS))) {
                        Espresso.pressBack()
                        flow.settle()
                    }
                }
                flow.waitForGone(hasTestTag(SessionScreen.TestIds.TABS), NAV_TIMEOUT)
                composeRule.onNode(hasContentDescription("Back")).performClick()
                flow.waitFor(hasTestTag(BOARD_ROOT_TAG), NAV_TIMEOUT)
                flow.settle()
            }
        }

        // --- My Issues: session 58cb32df — the board's My issues chip
        // (chats.json `shell.boardChips`), the active team's assigned issues
        // grouped by status; back to All issues after.
        // Gated like the other self-contained steps: a scoped run that does
        // not want it never blocks the later shots on its seed.
        step("my-issues") {
            openBoardRoot()
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
        }

        // --- Devices (EXP-686, the renamed Agents surface): since EXP-642
        // this lane needs the relay stub (`screenshots:desktop`) — the demo
        // user's own device row is what the next three shots are taken from,
        // and an empty machines list is not a useful reference shot either.
        // EXP-909 retired `sg_usage` with the cross-device Accounts section:
        // each device row now lists its OWN logins, so `sg_agents` is the
        // accounts shot too.
        step("devices") {
            openDevices()
            flow.waitFor(hasText(DEMO_DEVICE_NAME, substring = true), NETWORK_TIMEOUT)
            // EXP-1560: every device is a card, its logins brand-mark icons.
            flow.waitFor(hasTestTag("device-login-icon"), NETWORK_TIMEOUT)
            // FEED-149: the demo user OWNS the demo team, so the seeded teammate's
            // shared server ("Acme build server") is listed (its ⋯ holds the
            // owner's Remove).
            flow.waitFor(hasText(DevicesList.Copy.TEAM), NETWORK_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_agents")

            // --- sg_add-server (EXP-1558): the band's Add device pill opens THE
            // device-setup block (web's `openAddServer`), gated on the install
            // one-liner (its token is minted on open).
            if (ScreenshotFlow.anyRequested("sg_add-server")) {
                composeRule.onAllNodes(hasText("Add device")).onFirst().performClick()
                flow.waitFor(hasTestTag("add-device-sheet"), NAV_TIMEOUT)
                // The box breaks the command anywhere (zero-width spaces), so
            // match it with them stripped; the minted token is the gate.
            flow.waitFor(hasTextIgnoringBreaks("EXP_INSTALL_TOKEN=expi_"), NETWORK_TIMEOUT)
                flow.settle(longer = true)
                flow.screenshot("sg_add-server")
                Espresso.pressBack()
                flow.settle()
                if (flow.exists(hasTestTag("add-device-sheet"))) Espresso.pressBack()
                flow.waitForGone(hasTestTag("add-device-sheet"), NAV_TIMEOUT)
                flow.settle()
            }
        }

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
        step("chat") {
            openDevices()
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
            // EXP-1558: web's token (`APP`), so the sheet lists every seeded issue.
            composeRule.onNode(hasTestTag(SearchList.INPUT_TEST_ID)).performTextInput(ISSUE_REF_TOKEN)
            val sheetRow = hasText("$ISSUE_REF_TOKEN-", substring = true) and
                hasAnyAncestor(hasTestTag("issue-ref-search-sheet"))
            val rowsDeadline = System.currentTimeMillis() + SYNC_TIMEOUT
            while (composeRule.onAllNodes(sheetRow).fetchSemanticsNodes().size < 3) {
                assertTrue("the # sheet never listed the $ISSUE_REF_TOKEN issues", System.currentTimeMillis() < rowsDeadline)
                flow.settle()
            }
            // Web's frame has no keyboard: put it away (the sheet stays).
            Espresso.closeSoftKeyboard()
            flow.waitFor(hasTestTag("issue-ref-search-sheet"), NAV_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_search-list")
            Espresso.closeSoftKeyboard()
            Espresso.pressBack()
            flow.settle()
            if (flow.exists(hasTestTag("issue-ref-search-sheet"))) Espresso.pressBack()
            flow.waitForGone(hasTestTag("issue-ref-search-sheet"), NAV_TIMEOUT)
            composeRule.onNode(hasTestTag("agent-composer-field")).performTextReplacement("")
            Espresso.closeSoftKeyboard()
            flow.settle()

            // --- sg_composer-slash-menu (EXP-1558): web's `typeSlashInComposer`,
            // `/` in the home composer = the team's prompts under the card. The
            // keyboard is put away like web's frame (the menu follows the text).
            if (ScreenshotFlow.anyRequested("sg_composer-slash-menu")) {
                composeRule.onNode(hasTestTag("agent-composer-field")).performTextInput("/")
                flow.waitFor(hasTestTag("agent-composer-slash-menu"), NAV_TIMEOUT)
                flow.waitFor(
                    hasText(SLASH_PROMPT_NAME) and hasAnyAncestor(hasTestTag("agent-composer-slash-menu")),
                    SYNC_TIMEOUT,
                )
                Espresso.closeSoftKeyboard()
                flow.waitFor(hasTestTag("agent-composer-slash-menu"), NAV_TIMEOUT)
                flow.settle()
                flow.screenshot("sg_composer-slash-menu")
                composeRule.onNode(hasTestTag("agent-composer-field")).performTextReplacement("")
                Espresso.closeSoftKeyboard()
                flow.waitForGone(hasTestTag("agent-composer-slash-menu"), NAV_TIMEOUT)
                flow.settle()
            }

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
            listOf(CHAT_FIRST_ISSUE_TITLE, CHAT_SECOND_ISSUE_TITLE).forEachIndexed { index, title ->
                // A prefix query: the field's own text never matches the row's
                // full title, so the match below is the row.
                composeRule.onNode(hasTestTag(SearchList.INPUT_TEST_ID))
                    .performTextReplacement(title.split(' ').take(2).joinToString(" "))
                val row = hasText(title, substring = true) and hasAnyAncestor(hasTestTag("agent-composer-issues-picker"))
                flow.waitFor(row, SYNC_TIMEOUT)
                composeRule.onAllNodes(row).onFirst().performClick()
                // EXP-1551: the pick landed before the next query replaces the
                // list (the footer counts the picks: "Implement 2 issues").
                val picked = index + 1
                val submit = hasTestTag(ComposerMenu.IMPLEMENT_SUBMIT_TEST_ID)
                flow.waitFor(
                    hasText(ComposerMenu.implementButtonLabel(picked)) and (submit or hasAnyAncestor(submit)),
                    NAV_TIMEOUT,
                )
            }
            Espresso.closeSoftKeyboard()
            flow.waitFor(hasTestTag(ComposerMenu.IMPLEMENT_SUBMIT_TEST_ID), NAV_TIMEOUT)
            composeRule.onNode(hasTestTag(ComposerMenu.IMPLEMENT_SUBMIT_TEST_ID)).performClick()
            flow.waitForGone(hasTestTag(ComposerMenu.MENU_TEST_ID), NAV_TIMEOUT)
            flow.waitFor(hasIssueChip(), NAV_TIMEOUT)
            // Both chips, APP-3 then APP-6 (the headline wraps, so neither is
            // scrolled off the edge).
            CHAT_ISSUE_IDENTS.forEach { ident ->
                flow.waitFor(hasTestTag("agent-composer-chip-issue-$ident"), NAV_TIMEOUT)
            }
            val chipLefts = CHAT_ISSUE_IDENTS.map { ident ->
                composeRule.onNode(hasTestTag("agent-composer-chip-issue-$ident"))
                    .fetchSemanticsNode().boundsInRoot.let { it.top * 100_000f + it.left }
            }
            check(chipLefts == chipLefts.sorted()) { "chips out of pick order: $CHAT_ISSUE_IDENTS" }
            flow.settle()
            flow.screenshot("sg_chat-issues")
            // A tab now (Back would leave the app): the Team tab's Devices row
            // returns.
            openDevices()
            flow.waitForGone(hasTestTag("agent-page"), NAV_TIMEOUT)
            flow.waitFor(hasText(DEMO_DEVICE_NAME, substring = true), SYNC_TIMEOUT)
            flow.settle(longer = true)
        }

        // --- Machine settings: own, registered machines only — the entry is
        // absent otherwise, which is why the relay stub is a prerequisite.
        // EXP-1560: the card's ⋯ menu carries "Device settings".
        step("machine-settings") {
            openDevices()
            flow.waitFor(hasText(DEMO_DEVICE_NAME, substring = true), SYNC_TIMEOUT)
            composeRule.onAllNodes(hasTestTag("device-actions")).onFirst().performClick()
            flow.waitFor(hasTestTag("device-settings-button"), NAV_TIMEOUT)
            composeRule.onNode(hasTestTag("device-settings-button")).performClick()
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
        }

        // --- sg_chats: the Chats tab (EXP-1291) over the seed's tree —
        // the Agents band, then Needs you · Running · Recent, each row ending
        // on its quiet word (EXP-1309). Gated on the private root's row (no
        // lock on list rows since EXP-1339) and on APP-14's open stack word,
        // so neither a half-synced tree nor PR-less rows are photographed
        // (`merged` has no text, EXP-1339: its lead alone turns violet).
        // Session 58cb32df: the CHATS tab (chats.json): the headline, the
        // agent bubbles, the chips and one flat row per root chat.
        step("chats") {
            composeRule.onNode(hasTestTag("tab-chats")).performClick()
            flow.waitFor(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), NAV_TIMEOUT)
            // S-3: the pinned Agents section (the Assistant first) pushes the
            // chats below the fold of the lazy list, so scroll to each gated row
            // (an off-screen lazy row has no node), then back to the top.
            val chatRow = hasTestTag(Chats.TestIds.CHAT_ROW)
            for ((title, row) in listOf(
                RECENT_CHAT_TITLE to hasText(RECENT_CHAT_TITLE, substring = true),
                PRIVATE_CHAT_TITLE to (chatRow and hasText(PRIVATE_CHAT_TITLE, substring = true)),
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
            val agentRow = hasTestTag("chat-agent-row") and hasText(AGENT_NAME)
            flow.waitFor(agentRow, SYNC_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_chats")
        }

        // --- sg_apps / sg_app-run / sg_app (VAPP-116, VAPP-242): #EXP-1384
        // made Apps the third TAB; it opens the selected team's Apps (no
        // built-in apps: the seeded apps, by name). A tap on the Working hours
        // row (VAPP-122, with records) RUNS it (sg_app-run: the published
        // package full screen under the detail header); its `…` › Edit app
        // opens the editor and the shot shows its PREVIEW tab (web-mobile's
        // `app` shot). Leaving it ends the editor session, exactly as the web
        // capture does.
        step("apps") {
            composeRule.onNode(hasTestTag("tab-apps")).performClick()
            flow.waitFor(hasTestTag(Apps.TestIds.LIST), NAV_TIMEOUT)
            val demoAppRow = hasTestTag("${Apps.TestIds.ROW}-$DEMO_APP_SLUG")
            flow.waitFor(demoAppRow, SYNC_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_apps")
            composeRule.onNode(demoAppRow).performClick()
            flow.waitFor(hasTestTag(Apps.TestIds.RUN), NAV_TIMEOUT)
            flow.settle(longer = true)
            flow.screenshot("sg_app-run")
            composeRule.onNode(hasTestTag(Apps.TestIds.RUN_MENU)).performClick()
            flow.waitFor(hasTestTag("app-run-edit"), NAV_TIMEOUT)
            composeRule.onNode(hasTestTag("app-run-edit")).performClick()
            flow.waitFor(hasTestTag(SessionScreen.TestIds.TABS), NAV_TIMEOUT)
            composeRule.onNode(hasTestTag(SessionScreen.TestIds.tab(SessionScreen.Tab.Preview))).performClick()
            flow.waitFor(hasTestTag(Apps.TestIds.PREVIEW), SYNC_TIMEOUT)
            flow.settle(longer = true)
            flow.screenshot("sg_app")
            // --- sg_app-data (VAPP-178, app-records.json `dataTab` PHONES): the
            // Data segment: the kind select + Add record over the picked kind's
            // rows (Working hours' seeded records, newest first).
            if (ScreenshotFlow.anyRequested("sg_app-data")) {
                composeRule.onNode(hasTestTag(SessionScreen.TestIds.tab(SessionScreen.Tab.Data))).performClick()
                flow.waitFor(hasTestTag(Apps.TestIds.DATA), NAV_TIMEOUT)
                flow.waitFor(hasTestTag(AppRecords.DataTestIds.ROW), SYNC_TIMEOUT)
                flow.settle()
                flow.screenshot("sg_app-data")
            }
            // #EXP-1650: web-mobile opens the `…` menu's sheets and Export over
            // the editor's Chat segment.
            if (ScreenshotFlow.anyRequested("sg_app-versions", "sg_app-settings", "sg_app-export")) {
                composeRule.onNode(hasTestTag(SessionScreen.TestIds.tab(SessionScreen.Tab.Chat))).performClick()
                flow.waitFor(hasTestTag(Apps.TestIds.CHAT), NAV_TIMEOUT)
                flow.settle()
            }
            // --- sg_app-versions / sg_app-settings (VAPP-177, #EXP-1732
            // session-screen.json): the Versions TAB (Working hours' three
            // versions, the head row `Current`, the others Preview + Restore),
            // then THE gear → Settings (identity, description, records line,
            // Tools, Export + Delete app), the pushed item over the editor.
            if (ScreenshotFlow.anyRequested("sg_app-versions")) {
                composeRule.onNode(hasTestTag(SessionScreen.TestIds.tab(SessionScreen.Tab.Versions))).performClick()
                flow.waitFor(hasTestTag(Apps.TestIds.VERSIONS), NAV_TIMEOUT)
                flow.waitFor(hasTestTag("${Apps.TestIds.VERSION_ROW}-1"), SYNC_TIMEOUT)
                flow.settle()
                flow.screenshot("sg_app-versions")
                composeRule.onNode(hasTestTag(SessionScreen.TestIds.tab(SessionScreen.Tab.Chat))).performClick()
                flow.waitForGone(hasTestTag(Apps.TestIds.VERSIONS), NAV_TIMEOUT)
                flow.settle()
            }
            if (ScreenshotFlow.anyRequested("sg_app-settings")) {
                composeRule.onNode(hasTestTag(SessionScreen.TestIds.GEAR)).performClick()
                flow.waitFor(hasTestTag(Apps.TestIds.SETTINGS), NAV_TIMEOUT)
                flow.waitFor(hasTestTag("app-settings-tools"), SYNC_TIMEOUT)
                Espresso.closeSoftKeyboard()
                flow.settle()
                flow.screenshot("sg_app-settings")
                Espresso.pressBack()
                flow.waitForGone(hasTestTag(Apps.TestIds.SETTINGS), NAV_TIMEOUT)
                flow.settle()
            }
            // --- sg_app-export (VAPP-121, app-export.json `capture`): the
            // editor's `…` → Export with "Include records" switched on.
            if (ScreenshotFlow.anyRequested("sg_app-export")) {
                composeRule.onNode(hasTestTag(Apps.TestIds.MENU)).performClick()
                flow.waitFor(hasTestTag("app-editor-export"), NAV_TIMEOUT)
                composeRule.onNode(hasTestTag("app-editor-export")).performClick()
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
            // The editor back to the run view, the run view back to the list.
            Espresso.pressBack()
            flow.waitFor(hasTestTag(Apps.TestIds.RUN), NAV_TIMEOUT)
            Espresso.pressBack()
            flow.waitFor(hasTestTag(Apps.TestIds.LIST), NAV_TIMEOUT)
            flow.settle()

            // --- sg_app-draft (VAPP-176, apps.json NEW APP): the Release
            // checklist row = a draft, its editor opens on the Chat tab: the
            // Draft pill, `What should this app do?` with the starter pills and
            // the composer open on `Describe your app…` (the keyboard put away
            // so the starters show, as on iOS).
            if (ScreenshotFlow.anyRequested("sg_app-draft")) {
                // VAPP-242: a draft's editor = its row's `…` › Edit app.
                // The `…` sits inside the clickable row, which merges its
                // descendants' semantics (and keeps its own tag): look the
                // menu up in the unmerged tree.
                val draftMenu = hasTestTag("${Apps.TestIds.ROW}-menu-$DEMO_DRAFT_APP_SLUG")
                flow.waitFor(draftMenu, SYNC_TIMEOUT, useUnmergedTree = true)
                composeRule.onNode(draftMenu, useUnmergedTree = true).performClick()
                flow.waitFor(hasTestTag("apps-row-edit"), NAV_TIMEOUT)
                composeRule.onNode(hasTestTag("apps-row-edit")).performClick()
                flow.waitFor(hasTestTag(SessionScreen.TestIds.TABS), NAV_TIMEOUT)
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

            // --- sg_app-import (VAPP-121, app-export.json `capture`): the band's
            // Import… holding the fixture's capture text and the server's ONE
            // issue. A scoped run not naming it skips it.
            if (ScreenshotFlow.anyRequested("sg_app-import")) {
                // VAPP-242: Import… = THE NEW MENU's second item (its own sheet).
                composeRule.onNode(hasTestTag(Apps.TestIds.NEW_APP)).performClick()
                flow.waitFor(hasTestTag(Apps.TestIds.IMPORT_APP), NAV_TIMEOUT)
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
            composeRule.onNode(hasTestTag("tab-chats")).performClick()
            flow.waitFor(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), NAV_TIMEOUT)
            flow.settle(longer = true)
        }

        // --- The rich demo apps (the seed's CRM, Leave planner, Ops desk, Team
        // dashboard): RecordTable + RecordForm, Calendar + Timeline, the Connect
        // banner, Public links + Accounts + Automations, Publish + the Versions
        // sweep + a version's preview, CSV import, History, Undo. Undo is LAST,
        // and tapped, so the record comes back.
        if (ScreenshotFlow.anyRequested(*APP_RICH_SHOTS)) {
            step("rich-apps") {
                composeRule.onNode(hasTestTag("tab-apps")).performClick()
                flow.waitFor(hasTestTag(Apps.TestIds.LIST), NAV_TIMEOUT)
                openAppRun("crm")
                composeRule.onNode(hasTestTag("${Apps.TestIds.PREVIEW_SCREENS}-contacts")).performClick()
                flow.waitFor(hasContentDescription("Ada Novak", substring = true), SYNC_TIMEOUT)
                flow.settle()
                flow.screenshot("sg_app-record-table")
                composeRule.onNode(hasContentDescription("Add")).performClick()
                flow.waitFor(hasContentDescription("Cancel"), NAV_TIMEOUT)
                flow.settle()
                flow.screenshot("sg_app-record-form")
                composeRule.onNode(hasContentDescription("Cancel")).performClick()
                flow.waitForGone(hasContentDescription("Cancel"), NAV_TIMEOUT)
                leaveAppRun()

                openAppRun("leave-planner")
                flow.waitFor(hasContentDescription("Request leave"), SYNC_TIMEOUT)
                flow.settle(longer = true)
                flow.screenshot("sg_app-calendar")
                composeRule.onNode(hasTestTag("${Apps.TestIds.PREVIEW_SCREENS}-timeline")).performClick()
                flow.waitFor(hasContentDescription("Who is out"), NAV_TIMEOUT)
                flow.settle(longer = true)
                flow.screenshot("sg_app-timeline")
                leaveAppRun()

                openAppRun("ops-desk")
                flow.waitFor(hasTestTag("exponential-ui-upstream-connect"), SYNC_TIMEOUT)
                flow.settle()
                flow.screenshot("sg_app-connect")
                openEditorFromRun()
                composeRule.onNode(hasTestTag(SessionScreen.TestIds.GEAR)).performClick()
                flow.waitFor(hasTestTag(Apps.TestIds.SETTINGS), NAV_TIMEOUT)
                flow.waitFor(hasText("Public links"), SYNC_TIMEOUT)
                flow.waitFor(hasText("Accounts"), SYNC_TIMEOUT)
                flow.waitFor(hasText("Webhooks"), SYNC_TIMEOUT)
                Espresso.closeSoftKeyboard()
                flow.settle()
                flow.screenshot("sg_app-settings-links")
                composeRule.onNode(hasText(Apps.Copy.MENU_DELETE)).performScrollTo()
                flow.settle()
                flow.screenshot("sg_app-settings-automations")
                Espresso.pressBack()
                flow.waitForGone(hasTestTag(Apps.TestIds.SETTINGS), NAV_TIMEOUT)
                flow.settle()
                Espresso.pressBack()
                flow.waitFor(hasTestTag(Apps.TestIds.RUN), NAV_TIMEOUT)
                leaveAppRun()

                // Never presses Publish: the seed's v3 stays ahead of v2.
                openAppRun("team-dashboard")
                openEditorFromRun()
                composeRule.onNode(hasTestTag(Apps.TestIds.MENU)).performClick()
                flow.waitFor(hasTestTag(Apps.TestIds.PUBLISH), SYNC_TIMEOUT)
                flow.settle()
                flow.screenshot("sg_app-publish")
                // #EXP-1732: Versions is a tab now: put the menu away first.
                Espresso.pressBack()
                flow.waitForGone(hasTestTag("app-menu-sheet"), NAV_TIMEOUT)
                composeRule.onNode(hasTestTag(SessionScreen.TestIds.tab(SessionScreen.Tab.Versions))).performClick()
                flow.waitFor(hasTestTag(Apps.TestIds.VERSIONS), NAV_TIMEOUT)
                flow.waitFor(hasText("Migrating records", substring = true), SYNC_TIMEOUT)
                flow.settle()
                flow.screenshot("sg_app-versions-sweep")
                composeRule.onAllNodes(hasTestTag(Apps.TestIds.VERSION_PREVIEW))[0].performClick()
                flow.waitFor(hasTestTag(Apps.TestIds.VIEWING), NAV_TIMEOUT)
                flow.settle(longer = true)
                flow.screenshot("sg_app-version-preview")
                Espresso.pressBack()
                flow.waitFor(hasTestTag(Apps.TestIds.RUN), NAV_TIMEOUT)
                leaveAppRun()

                openAppRun("crm")
                openEditorFromRun()
                composeRule.onNode(hasTestTag(SessionScreen.TestIds.tab(SessionScreen.Tab.Data))).performClick()
                flow.waitFor(hasTestTag(AppRecords.DataTestIds.ROW), SYNC_TIMEOUT)
                flow.settle()
                composeRule.onNode(hasTestTag(AppRecords.CsvTestIds.BUTTON)).performClick()
                flow.waitFor(hasTestTag(AppRecords.CsvTestIds.TEXT), NAV_TIMEOUT)
                composeRule.onNode(hasTestTag(AppRecords.CsvTestIds.TEXT)).performTextInput(APP_CSV_CAPTURE_TEXT)
                Espresso.closeSoftKeyboard()
                flow.waitFor(hasText("2 of 3 rows ready"), NAV_TIMEOUT)
                flow.settle()
                flow.screenshot("sg_app-csv-import")
                Espresso.pressBack()
                flow.settle()
                if (flow.exists(hasTestTag(AppRecords.CsvTestIds.DIALOG))) Espresso.pressBack()
                flow.waitForGone(hasTestTag(AppRecords.CsvTestIds.DIALOG), NAV_TIMEOUT)
                flow.settle()

                // A delete + its Undo give the record a History worth showing.
                deleteFirstRecord()
                undoRecordWrite()
                composeRule.onAllNodes(hasTestTag(AppRecords.DataTestIds.MENU))[0].performClick()
                flow.waitFor(hasTestTag("app-data-row-edit"), NAV_TIMEOUT)
                composeRule.onNode(hasTestTag("app-data-row-edit")).performClick()
                flow.waitFor(hasTestTag(AppRecords.DataTestIds.HISTORY_ROW), SYNC_TIMEOUT)
                flow.waitFor(hasText(AppRecords.HistoryCopy.RESTORED), SYNC_TIMEOUT)
                flow.settle()
                flow.screenshot("sg_app-record-history")
                Espresso.pressBack()
                flow.waitForGone(hasTestTag(AppRecords.DataTestIds.FORM), NAV_TIMEOUT)
                flow.settle()
                deleteFirstRecord()
                flow.settle()
                flow.screenshot("sg_app-record-undo")
                undoRecordWrite()

                Espresso.pressBack()
                flow.waitFor(hasTestTag(Apps.TestIds.RUN), NAV_TIMEOUT)
                leaveAppRun()
                composeRule.onNode(hasTestTag("tab-chats")).performClick()
                flow.waitFor(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), NAV_TIMEOUT)
                flow.settle(longer = true)
            }
        }

        // --- sg_agent-channel: an agent row opens its channel (the same
        // conversation screen): Triage's report (its bare FB-n chipped as
        // issue pills, EXP-760), the chat it started, the person's request
        // and the reply. The reply's clock proves the pinned zone.
        // VAPP-122: a scoped run that does not ask for this shot skips the
        // walk (the apps views alone must not wait on the channel).
        step("agent-channel") {
            composeRule.onNode(hasTestTag("tab-chats")).performClick()
            flow.waitFor(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), NAV_TIMEOUT)
            if (ScreenshotFlow.anyRequested("sg_agent-channel")) {
                // S-3: the pinned agent row.
                composeRule.onAllNodes(hasTestTag("chat-agent-row") and hasText(AGENT_NAME)).onFirst().performClick()
                // #EXP-1732: THE session screen; its Chat tab = the agent
                // root's own session view (the seeded messages are no feed).
                flow.waitFor(hasTestTag(SessionScreen.TestIds.TABS), NAV_TIMEOUT)
                flow.settle(longer = true)
                flow.screenshot("sg_agent-channel")
                Espresso.pressBack()
                flow.waitFor(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), NAV_TIMEOUT)
                flow.settle(longer = true)
            }
        }

        // --- sg_chat-screen: the cold-start chat (APP-5): the strip, the
        // Conversation with its session cards, the agent's reply and the
        // pending question card, the reply box. #EXP-1433: the strip never
        // scrolls — + and Conversation pinned left, Results right, the
        // issues + sessions between them behind ONE group tab.
        step("chat-screen") {
            composeRule.onNode(hasTestTag("tab-chats")).performClick()
            flow.waitFor(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), NAV_TIMEOUT)
            flow.settle(longer = true)
            val coldStartRow = hasTestTag(Chats.TestIds.CHAT_ROW) and
                hasText(COLD_START_CHAT_TITLE, substring = true)
            // S-3: the pinned Agents section pushes the row down the lazy list
            // (and under the floating bar): scroll it in, tap it clear of the bar.
            composeRule.onNode(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST)).performScrollToNode(coldStartRow)
            flow.settle()
            tapClearOfBar(coldStartRow)
            // #EXP-1732: THE session screen's tabs (the seed's issues + child
            // session counted on Issues / Sessions) over the Chat tab = the
            // ROOT's own session view.
            flow.waitFor(hasTestTag(SessionScreen.TestIds.TABS), NAV_TIMEOUT)
            // The stub holds the root as a stored replay with an empty
            // journal: its feed may settle on the empty state (iOS alike).
            flow.waitFor(
                hasTestTag("agent-feed") or hasText(STEER_NO_FEED_IDLE_TITLE) or
                    hasText("Waiting for activity", substring = true),
                NETWORK_TIMEOUT,
            )
            flow.settle()
            flow.screenshot("sg_chat-screen")

            // --- sg_session: its "Profile the launch path" card opens the raw
            // session view (EXP-1291), the stub relay's feed for the seed's child
            // session. Its pending question takes the composer's place
            // (EXP-1306), so the question card is the gate.
            // #EXP-1732: the child = a row of the Sessions tab; it pushes its view.
            composeRule.onNode(hasTestTag(SessionScreen.TestIds.tab(SessionScreen.Tab.Sessions))).performClick()
            val childRow = hasTestTag("${SessionScreen.TestIds.SESSIONS}-row") and
                hasText(CHILD_SESSION_TITLE, substring = true)
            flow.waitFor(childRow, SYNC_TIMEOUT)
            composeRule.onAllNodes(childRow).onFirst().performClick()
            // S-3: the relay's question is inline in the feed again (dock.json).
            flow.waitFor(hasTestTag("feed-question"), NETWORK_TIMEOUT)
            flow.settle(longer = true)
            flow.screenshot("sg_session")
            // --- sg_script-card · sg_work-card (EXP-1654): the SAME relay
            // feed web and desktop photograph, scrolled up from its tail to
            // the code-mode call's Script card, then the `cold-start-review`
            // workflow's Work card above it — in the session, never isolated.
            // Real drags: a programmatic scroll is re-pinned to the tail.
            if (ScreenshotFlow.anyRequested("sg_script-card", "sg_work-card")) {
                val feed = hasTestTag("agent-feed")
                // Short drags, a settle after each: a full-height fling (what
                // `waitForInList` swipes) carried the lazy feed past the card
                // between two polls.
                fun revealFeedCard(tag: String) {
                    // The feed holds more than one Work card: name the workflow's.
                    val card = if (tag == "work-card") {
                        hasTestTag(tag) and hasAnyDescendant(hasText("cold-start-review"))
                    } else {
                        hasTestTag(tag)
                    }
                    var drags = 0
                    while (drags < 24) {
                        val shown = runCatching {
                            composeRule.onAllNodes(card).onFirst().assertIsDisplayed()
                        }.isSuccess
                        if (shown) break
                        composeRule.onNode(feed)
                            .performTouchInput { swipeDown(startY = centerY, endY = centerY + 300f, durationMillis = 400) }
                        flow.settle()
                        drags++
                    }
                    flow.waitFor(card, SYNC_TIMEOUT)
                    flow.settle()
                }
                // The names stay literal: views.test.ts reads them off the source.
                revealFeedCard("script-card")
                flow.screenshot("sg_script-card")
                revealFeedCard("work-card")
                flow.screenshot("sg_work-card")
            }
            // Session → chat → Chats (#EXP-1732): a session of the chat's tree
            // is its pushed item (Back pops it), then one Back leaves the chat.
            Espresso.pressBack()
            flow.settle()
            if (flow.exists(hasTestTag(SessionScreen.TestIds.TABS))) Espresso.pressBack()
            flow.waitFor(hasTestTag(SessionTree.ListCopy.TEST_ID_LIST), NAV_TIMEOUT)
            flow.settle(longer = true)
        }

        // --- Settings root: session 58cb32df — the TEAM tab (chats.json
        // `shell`): the team, the team switch, its settings rows, then the
        // old root (Servers, General). Header matches keep ignoreCase so they
        // hold whichever case SectionHeader renders (sentence case since EXP-698).
        step("settings-root") {
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
        }

        // --- sg_settings-team: #EXP-1381 every Team row opens its OWN page
        // (settings-nav.json); the Boards page, anchored on the seeded board
        // it lists (iOS parity).
        step("settings-team") {
            openTeamRoot()
            tapClearOfBar(teamSettingsRow("Boards"))
            flow.waitFor(hasTestTag(SettingsNav.PAGE_TEST_ID), NAV_TIMEOUT)
            flow.waitFor(hasText(TEAM_BOARD_NAME, substring = true), SYNC_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_settings-team")
            Espresso.closeSoftKeyboard()
            Espresso.pressBack()
            flow.waitFor(hasTestTag("team-tab"), NAV_TIMEOUT)
            flow.settle()
        }

        // --- sg_settings-general · -members · -labels · -repositories
        // (EXP-1558): the Team page's other rows, each its OWN page
        // (settings-nav.json), anchored on seeded content like web's.
        step("settings-pages") {
            if (ScreenshotFlow.anyRequested("sg_settings-general")) {
                openTeamSettingsPage("General", hasText("Delete team"))
                flow.screenshot("sg_settings-general")
                leaveTeamSettingsPage()
            }
            if (ScreenshotFlow.anyRequested("sg_settings-members")) {
                openTeamSettingsPage("Members", hasText("mira@acme.dev", substring = true))
                flow.screenshot("sg_settings-members")
                leaveTeamSettingsPage()
            }
            if (ScreenshotFlow.anyRequested("sg_settings-labels")) {
                openTeamSettingsPage("Labels", hasText("Performance"))
                flow.screenshot("sg_settings-labels")
                leaveTeamSettingsPage()
            }
            if (ScreenshotFlow.anyRequested("sg_settings-repositories")) {
                openTeamSettingsPage("Repositories", hasText("acme/mobile-app", substring = true))
                flow.screenshot("sg_settings-repositories")
                leaveTeamSettingsPage()
            }
            // #EXP-1616 sg_repo (repo-page.json): the Repositories page's
            // seeded eCode repo `acme/engine` → its repo page, gated on the
            // Pull requests tab's seeded tree.
            if (ScreenshotFlow.anyRequested("sg_repo")) {
                openTeamSettingsPage("Repositories", hasText("acme/engine", substring = true))
                val engineRow = hasTestTag("repo-row-open") and hasText("acme/engine", substring = true)
                tapClearOfBar(engineRow)
                flow.waitFor(hasTestTag("repo-page"), NAV_TIMEOUT)
                flow.waitFor(hasText("Pack cursor for incremental fetches", substring = true), SYNC_TIMEOUT)
                flow.settle()
                flow.screenshot("sg_repo")
                Espresso.pressBack()
                flow.waitFor(hasTestTag(SettingsNav.PAGE_TEST_ID), NAV_TIMEOUT)
                leaveTeamSettingsPage()
            }
        }

        // --- sg_settings-team-agent: the Team page's Agents row → Triage: its
        // device, account, schedule and the seeded instructions (fetched, so
        // they are the gate).
        step("settings-team-agent") {
            openTeamRoot()
            tapClearOfBar(teamSettingsRow("Agents"))
            val settingsAgentRow = hasTestTag(SessionTree.ListCopy.TEST_ID_AGENT_ROW) and hasText(AGENT_NAME)
            flow.waitFor(settingsAgentRow, SYNC_TIMEOUT)
            // #EXP-1735: sg_settings-agents = the Agents page's top: the Lead
            // Assistant row naming the seeded lead (the demo user, read
            // through `agents.lead`) over the agents list.
            if (ScreenshotFlow.anyRequested("sg_settings-agents")) {
                val leadRow = hasTestTag(AgentSettings.Lead.TEST_ID_ROW)
                flow.waitFor(
                    (leadRow and hasText(LEAD_NAME, substring = true)) or
                        (hasAnyAncestor(leadRow) and hasText(LEAD_NAME, substring = true)),
                    NETWORK_TIMEOUT,
                )
                flow.settle()
                flow.screenshot("sg_settings-agents")
            }
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
        }

        // --- sg_settings-assistant (S-3, agent-settings.json `assistant`):
        // the Team page's Agents row → the Assistant (the agents list's first
        // row, the brand mark) = the viewer's OWN Assistant (seeded):
        // identity, Runs on, Check in (the gate), the Voice block; no
        // instructions (#EXP-1439). A detour: nothing is edited.
        // EXP-1558: then sg_settings-assistant-meeting, the same page with its
        // Meeting rows centred (web's `scrollToMeetingMode`).
        step("settings-assistant") {
            openTeamRoot()
            if (ScreenshotFlow.anyRequested("sg_settings-assistant", "sg_settings-assistant-meeting")) {
                tapClearOfBar(teamSettingsRow("Agents"))
                val assistantRow = hasTestTag(SessionTree.ListCopy.TEST_ID_AGENT_ROW) and
                    hasText(AgentSettings.Assistant.NAME)
                flow.waitFor(assistantRow, SYNC_TIMEOUT)
                tapClearOfBar(assistantRow)
                flow.waitFor(hasTestTag(AgentSettings.TEST_ID_PAGE), NAV_TIMEOUT)
                flow.waitFor(hasTestTag("assistant-check-in"), SYNC_TIMEOUT)
                flow.settle(longer = true)
                flow.screenshot("sg_settings-assistant")
                if (ScreenshotFlow.anyRequested("sg_settings-assistant-meeting")) {
                    val stopAfter = hasText(Meeting.Copy.AUTO_STOP_AFTER)
                    flow.waitFor(stopAfter, NETWORK_TIMEOUT)
                    centreOnScreen(stopAfter)
                    flow.settle()
                    flow.screenshot("sg_settings-assistant-meeting")
                }
                Espresso.closeSoftKeyboard()
                Espresso.pressBack()
                flow.waitFor(assistantRow, NAV_TIMEOUT)
                Espresso.closeSoftKeyboard()
                Espresso.pressBack()
                flow.waitFor(hasTestTag("team-tab"), NAV_TIMEOUT)
                flow.settle()
            }
        }

        // --- sg_settings-prompts / sg_settings-prompt (NEW-7): the Team page's
        // Prompts row (the band with the owner's New prompt pill over one row
        // per prompt), then New prompt: THE prompt page in create mode (web
        // `settings-prompt`), so neither shot needs a seeded prompt id.
        step("settings-prompts") {
            openTeamRoot()
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
        }

        // --- sg_settings-skills / sg_settings-skill (#EXP-1661): the Team
        // page's Skills row (the read-only list, skills.json `list`), then the
        // first skill's read-only page, gated on its fetched body.
        step("settings-skills") {
            openTeamRoot()
            tapClearOfBar(teamSettingsRow(Skills.TITLE))
            flow.waitFor(hasTestTag(Skills.LIST_TEST_ID), NAV_TIMEOUT)
            flow.waitFor(hasTestTag(Skills.ROW_TEST_ID), SYNC_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_settings-skills")
            composeRule.onAllNodes(hasTestTag(Skills.ROW_TEST_ID)).onFirst().performClick()
            flow.waitFor(hasTestTag("skill-body"), NETWORK_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_settings-skill")
            Espresso.pressBack()
            flow.waitFor(hasTestTag(Skills.LIST_TEST_ID), NAV_TIMEOUT)
            Espresso.pressBack()
            flow.waitFor(hasTestTag("team-tab"), NAV_TIMEOUT)
            flow.settle()
        }

        // --- Account settings: there is no separate profile screen (EXP-311) —
        // the signed-in identity, sign out and delete account live on the
        // server row's detail. The row is titled by the SERVER with the email
        // below, so the email addresses it; "Sign out" only exists there.
        step("settings-account") {
            openTeamRoot()
            tapClearOfBar(hasText(DEMO_EMAIL, substring = true))
            flow.waitFor(hasText("Sign out"), NAV_TIMEOUT)
            flow.waitFor(hasText("Delete account", substring = true), NAV_TIMEOUT)
            flow.settle()
            flow.screenshot("sg_settings-account")
        }

        // A scoped run that wants none of the three onboarding shots skips the
        // identity switch altogether: the capture-only screens below need
        // nothing from the app.
        step("onboarding", retry = false) {
            if (ScreenshotFlow.anyRequested("sg_onboarding", "sg_onboarding-create-team", "sg_invite-accept", "sg_onboarding-invite", "sg_onboarding-devices", "sg_onboarding-join")) {
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

                // --- sg_onboarding-create-team: the choice page's Create a team
                // (web's wizard CreateTeamStep), the newcomer's full-screen step
                // card, never the New team sheet. Nothing is typed or submitted;
                // Back returns to the choice page for the join shot.
                if (ScreenshotFlow.anyRequested("sg_onboarding-create-team")) {
                    composeRule.onNode(hasTestTag("team-setup-create")).performClick()
                    flow.waitFor(hasText("Team name"), NAV_TIMEOUT)
                    Espresso.closeSoftKeyboard()
                    flow.settle()
                    flow.screenshot("sg_onboarding-create-team")
                    composeRule.onAllNodes(hasText("Back")).onFirst().performClick()
                    flow.waitFor(hasTestTag("team-setup-join"), NAV_TIMEOUT)
                    flow.settle()
                }

                // --- sg_onboarding-join (EXP-1558): the choice page's Join a team
                // (web's `openOnboardingJoin`): the invite-link field, empty,
                // keyboard down; its Back returns to the choice. Nothing is sent.
                if (ScreenshotFlow.anyRequested("sg_onboarding-join")) {
                    composeRule.onNode(hasTestTag("team-setup-join")).performClick()
                    flow.waitFor(hasTestTag("team-setup-invite"), NAV_TIMEOUT)
                    Espresso.closeSoftKeyboard()
                    flow.settle()
                    flow.screenshot("sg_onboarding-join")
                    composeRule.onAllNodes(hasText("Back")).onFirst().performClick()
                    flow.waitFor(hasTestTag("team-setup-join"), NAV_TIMEOUT)
                    flow.settle()
                }

                // --- sg_invite-accept (#EXP-1632): the newcomer opening the seeded
                // invite link (`screenshots-demo-invite`, the token web and desktop
                // photograph) mid-wizard: the bus target MainActivity posts for an
                // `exponential://invite/<token>` intent pushes the invite route over
                // the wizard. The landing card only: Accept is NEVER pressed (it
                // would burn the token); Back returns to the wizard.
                if (ScreenshotFlow.anyRequested("sg_invite-accept")) {
                    composeRule.runOnUiThread {
                        composeRule.activity.deepLinkBus.openInvite("screenshots-demo-invite")
                    }
                    flow.waitFor(hasTestTag("invite-accept"), SYNC_TIMEOUT)
                    flow.settle()
                    flow.screenshot("sg_invite-accept")
                    Espresso.pressBack()
                    // Back lands on a wizard step (not always the team page); every
                    // step carries the Sign out escape hatch the starter block uses.
                    flow.waitFor(hasText("Sign out"), NAV_TIMEOUT)
                    flow.settle()
                }

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
        }

        // EXP-1558: the shots the walk did take are on the device (the lane
        // pulls them); the run still fails, naming every step that did not.
        assertTrue(
            "EXP-1558: ${failedSteps.size} step(s) failed:\n${failedSteps.joinToString("\n")}",
            failedSteps.isEmpty(),
        )
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

        // --- EXP-1635: step 1 of a share, the composer a pick lands on, the
        // + menu's Camera | Photos | File.
        show(ShareToSpecimenTestId) { ShareToSpecimenScreen() }
        flow.screenshot("sg_share-targets")
        show(ShareComposerSpecimenTestId) { ShareComposerSpecimenScreen() }
        flow.screenshot("sg_share-composer")
        show(ComposerAddFileSpecimenTestId) { ComposerAddFileSpecimenScreen() }
        flow.screenshot("sg_composer-add-file")

        // --- sg_feedback-states: the six feedback kinds (EXP-1275). A
        // capture-only screen, every kind drawn by its real primitive and the
        // error toast raised on appear.
        show(FeedbackStatesScreenTestId) { FeedbackStatesScreen() }
        flow.screenshot("sg_feedback-states")

        // --- sg_work-card · sg_script-card · sg_comment-call-card: the run
        // chat's cards (EXP-1240 / EXP-1279 / EXP-1241), each drawn by its
        // real composable on a capture-only screen.
        fun showChatCards(kind: ChatCardsKind) {
            show(kind.testId) { ChatCardsScreen(kind) }
        }
        // EXP-1654: sg_work-card + sg_script-card are the live session
        // walk's (step "chat-screen"); only the comment card stays capture-only.
        showChatCards(ChatCardsKind.Comment)
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

        // --- sg_record-form-field-states: Round 9 §6.6, one Input per check
        // severity with its message showing.
        show(RecordFormFieldStatesSpecimenTestId) { RecordFormFieldStatesSpecimenScreen() }
        flow.screenshot("sg_record-form-field-states")

        // --- sg_plan-card: THE rich plan / question card (EXP-1280,
        // plan-card.json): session-chat.json's rich case — the pending plan
        // (pictures, a tile question, a list question, the inline eUI
        // surface) and the answered multi question — by the real ChatAskCard.
        showChatCards(ChatCardsKind.PlanCard)
        flow.screenshot("sg_plan-card")

        // --- sg_pr-row (EXP-1553): THE PR row in its two shapes (a tree with
        // guides, a stack on its rail to master) + an unlinked PR — web's
        // `pr-row` entry, by the real PrList / StackRail.
        showChatCards(ChatCardsKind.PrRow)
        flow.screenshot("sg_pr-row")

        // --- sg_session-row (session 20b7fdc2): THE session row under the
        // Running caption — web's `session-row` island, by the real SessionRow.
        showChatCards(ChatCardsKind.SessionRow)
        flow.screenshot("sg_session-row")

        // --- sg_pr-checks (#EXP-1611, #EXP-1710): THE header merge STATUS
        // pill in its states (merge-train.json `statusPill`) + the `…`'s
        // Checks submenu pushed open over pr-checks.json `specimen`, by the
        // real MergeStatusPill / ChatMenuPanel.
        showChatCards(ChatCardsKind.PrChecks)
        flow.screenshot("sg_pr-checks")

        // --- Session 58cb32df (fixtures/chats.json): the Chats screen over
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
        show(TeamAvatarSpecimenTestId) { TeamAvatarSpecimenScreen() }
        flow.screenshot("sg_team-avatar")
        show(DeviceAttentionSpecimenTestId) { DeviceAttentionSpecimenScreen() }
        flow.screenshot("sg_device-attention")
        show(PhonePageHeaderSpecimenTestId) { PhonePageHeaderSpecimenScreen() }
        flow.screenshot("sg_phone-page-header")
        show(ChatHeaderSpecimenTestId) { ChatHeaderSpecimenScreen() }
        flow.screenshot("sg_chat-header")
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
        // Session e4a04d22: `@email` as the member's pill in a chat, an
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
        // --- sg_call-pill (EXP-1681): THE floating call pill, once per row.
        show(CallPillSpecimenTestId) { CallPillSpecimenScreen() }
        flow.screenshot("sg_call-pill")
        // --- sg_device-readiness (EXP-1375, device-doctor.json): the low-disk
        // case's block and the composer's low-disk row.
        show(DeviceReadinessSpecimenTestId) { DeviceReadinessSpecimenScreen() }
        flow.screenshot("sg_device-readiness")
        // --- sg_plan-page (#EXP-1493, plan-page.json `specimen`): THE plan
        // page, phone: the document from its top, the DockCard at the foot.
        show(PlanPageSpecimenTestId) { PlanPageSpecimenScreen() }
        flow.screenshot("sg_plan-page")
        // --- sg_needs-you · sg_results-tab (#FEED-191, needs-you.json `cases[1]`,
        // results-tab.json `tab.specimen`): the phone entry row + THE list, and
        // THE tab body over the committed sample pictures, no PR block.
        show(NeedsYouSpecimenTestId) { NeedsYouSpecimenScreen() }
        flow.screenshot("sg_needs-you")
        show(ResultsTabSpecimenTestId) { ResultsTabSpecimenScreen() }
        flow.screenshot("sg_results-tab")
        // --- sg_os-live-activity · sg_os-live-activity-island ·
        // sg_os-notification-actions (#EXP-1639, #EXP-1641): the OS draws
        // them (a promoted ProgressStyle Live Update + its status-bar chip;
        // Actions + RemoteInput), so these mock the shade over the real
        // LiveActivity / NotificationActions models and the fixtures' copy.
        show(OsLiveActivitySpecimenTestId) { OsLiveActivitySpecimenScreen() }
        flow.screenshot("sg_os-live-activity")
        show(OsLiveActivityChipSpecimenTestId) { OsLiveActivityChipSpecimenScreen() }
        flow.screenshot("sg_os-live-activity-island")
        show(OsNotificationActionsSpecimenTestId) { OsNotificationActionsSpecimenScreen() }
        flow.screenshot("sg_os-notification-actions")

        finished = true
    }

    /** EXP-1558: a text node holding [fragment] once its zero-width break
     *  opportunities (DeviceSetup's `breakAnywhere`) are stripped. */
    private fun hasTextIgnoringBreaks(fragment: String): SemanticsMatcher =
        SemanticsMatcher("has text containing '$fragment' (zero-width breaks stripped)") { node ->
            node.config.getOrNull(SemanticsProperties.Text).orEmpty()
                .any { it.text.replace("\u200B", "").contains(fragment) }
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

    /**
     * EXP-1558: one stretch of the live walk. A failure is retried once from
     * the tabs ([backToTabs]), then recorded in [failedSteps] and the walk
     * carries on; [captureStyleguideScreenshots] fails at its end with the
     * list. Every step therefore starts from a tab of its own.
     */
    /** The Apps list → that app's run view. */
    private fun openAppRun(slug: String) {
        val row = hasTestTag("${Apps.TestIds.ROW}-$slug")
        flow.waitFor(row, SYNC_TIMEOUT)
        composeRule.onNode(row).performClick()
        flow.waitFor(hasTestTag(Apps.TestIds.RUN), NAV_TIMEOUT)
        flow.settle()
    }

    /** The run view back to the Apps list. */
    private fun leaveAppRun() {
        Espresso.pressBack()
        flow.waitFor(hasTestTag(Apps.TestIds.LIST), NAV_TIMEOUT)
        flow.settle()
    }

    /** The run view's `…` › Edit app. */
    private fun openEditorFromRun() {
        composeRule.onNode(hasTestTag(Apps.TestIds.RUN_MENU)).performClick()
        flow.waitFor(hasTestTag("app-run-edit"), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag("app-run-edit")).performClick()
        flow.waitFor(hasTestTag(SessionScreen.TestIds.TABS), NAV_TIMEOUT)
        flow.settle()
    }

    /** The Data tab's first row › Delete + confirm, until the Undo toast shows. */
    private fun deleteFirstRecord() {
        composeRule.onAllNodes(hasTestTag(AppRecords.DataTestIds.MENU))[0].performClick()
        flow.waitFor(hasTestTag("app-data-row-delete"), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag("app-data-row-delete")).performClick()
        flow.waitFor(hasTestTag("app-data-delete-confirm"), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag("app-data-delete-confirm")).performClick()
        flow.waitFor(hasText(AppRecords.HistoryCopy.UNDO), SYNC_TIMEOUT)
    }

    /** Taps the toast's Undo: the record comes back. */
    private fun undoRecordWrite() {
        composeRule.onNode(hasText(AppRecords.HistoryCopy.UNDO)).performClick()
        flow.waitForGone(hasText(AppRecords.HistoryCopy.UNDO), SYNC_TIMEOUT)
        flow.settle()
    }

    private fun step(name: String, retry: Boolean = true, body: () -> Unit) {
        val attempts = if (retry) 2 else 1
        repeat(attempts) { attempt ->
            try {
                body()
                return
            } catch (error: Throwable) {
                android.util.Log.w("EXP-1558", "step $name failed (attempt ${attempt + 1}/$attempts)", error)
                if (attempt == attempts - 1) {
                    failedSteps += "$name: ${error.message?.lineSequence()?.firstOrNull() ?: error::class.simpleName}"
                }
                backToTabs()
            }
        }
    }

    /**
     * EXP-1558: unwind whatever a failed step left open (a sheet, a pushed
     * page, the keyboard, a board selection) until the tab bar is up with no
     * second window over it. Never presses Back on a bare tab root, which
     * would leave the app.
     */
    private fun backToTabs() {
        // A window over the tabs (a sheet, a menu) takes at most two Backs:
        // anything still "over" them after that is not ours to close.
        var popupBacks = 0
        repeat(6) {
            runCatching { Espresso.closeSoftKeyboard() }
            val popup = runCatching { composeRule.onAllNodes(isRoot()).fetchSemanticsNodes().size > 1 }
                .getOrDefault(false)
            if (!popup && runCatching { flow.exists(hasContentDescription("Clear selection")) }.getOrDefault(false)) {
                runCatching { composeRule.onNode(hasContentDescription("Clear selection")).performClick() }
                flow.settle()
            }
            val tabs = runCatching { flow.exists(hasTestTag(Chats.TestIds.TAB_TEAM)) }.getOrDefault(false)
            if (tabs && (!popup || popupBacks >= 2)) return
            if (tabs) popupBacks += 1
            runCatching { Espresso.pressBack() }
            flow.settle()
        }
    }

    /**
     * EXP-1654: the first sync's showcase row (APP-5) on the board. The base
     * budget is [FIRST_SYNC_TIMEOUT]; past it the wait goes on (up to
     * [FIRST_SYNC_CEILING_MS]) only while the sync is still visibly moving —
     * the "Syncing" pill is up, no issue row has landed yet, or the row count
     * grew in the last [FIRST_SYNC_STALL_MS]. A board that settled WITHOUT
     * the showcase (a wrong board, a seed without APP-5) fails at once with
     * what it saw, never after the ceiling; an EMPTY board (the persisted
     * last board left on Launch Marketing) is switched back once first.
     */
    private fun waitForFirstBoardSync() {
        val showcase = hasText(SHOWCASE_ISSUE_TITLE)
        val start = System.currentTimeMillis()
        if (flow.waitForOptional(showcase, FIRST_SYNC_TIMEOUT)) return
        val ceiling = start + FIRST_SYNC_CEILING_MS
        var lastRows = issueRowCount()
        var lastGrowth = System.currentTimeMillis()
        var switchedBack = false
        while (true) {
            if (flow.waitForOptional(showcase, 2_000L)) return
            val now = System.currentTimeMillis()
            val rows = issueRowCount()
            if (rows > lastRows) {
                lastRows = rows
                lastGrowth = now
            }
            val syncing = flow.exists(hasText("Syncing", substring = true))
            val emptyBoard = flow.exists(hasText("No issues yet"))
            // A run that died inside board-empty leaves Launch Marketing as
            // the persisted last board: switch back once, then judge again.
            if (emptyBoard && !syncing && !switchedBack) {
                switchedBack = true
                switchBoard(TEAM_BOARD_NAME)
                lastGrowth = System.currentTimeMillis()
                continue
            }
            val moving = syncing || (rows == 0 && !emptyBoard) || now - lastGrowth < FIRST_SYNC_STALL_MS
            if (!moving || now > ceiling) {
                throw androidx.compose.ui.test.ComposeTimeoutException(
                    "Showcase issue \"$SHOWCASE_ISSUE_TITLE\" missing after ${(now - start) / 1000}s — " +
                        "$rows issue rows, syncing=$syncing, emptyBoard=$emptyBoard " +
                        (if (now > ceiling) "(still syncing at the ceiling)" else "(the sync settled without it: wrong board or seed?)"),
                )
            }
        }
    }

    private fun issueRowCount(): Int =
        composeRule.onAllNodes(
            SemanticsMatcher("is an issue-row-*") { node ->
                node.config.getOrNull(SemanticsProperties.TestTag)?.startsWith("issue-row-") == true
            },
        ).fetchSemanticsNodes().size

    /** EXP-1558: the Board tab's root on the demo board, whatever a failed
     *  step left it on. */
    private fun openBoardRoot() {
        if (!flow.exists(hasTestTag(BOARD_ROOT_TAG))) {
            composeRule.onNode(hasTestTag(Chats.TestIds.TAB_BOARD)).performClick()
            flow.settle()
        }
        // On the board view the Board tab opens the board menu: close it.
        if (flow.exists(hasTestTag(Chats.TestIds.BOARD_MENU))) {
            Espresso.pressBack()
            flow.waitForGone(hasTestTag(Chats.TestIds.BOARD_MENU), NAV_TIMEOUT)
        }
        flow.waitFor(hasTestTag(BOARD_ROOT_TAG), NAV_TIMEOUT)
        if (!flow.exists(hasText(TEAM_BOARD_NAME))) switchBoard(TEAM_BOARD_NAME)
        if (!flow.exists(hasTestTag("issue-list"))) {
            composeRule.onNode(hasTestTag(BOARD_ROOT_TAG)).performClick()
        }
        flow.waitFor(hasTestTag("issue-list"), SYNC_TIMEOUT)
        composeRule.onNode(hasTestTag("issue-list")).performScrollToIndex(0)
        flow.waitFor(hasText(SHOWCASE_ISSUE_TITLE), SYNC_TIMEOUT)
        flow.settle()
    }

    /** EXP-1558: a Team-page row's own page (settings-nav.json), once [anchor] synced. */
    private fun openTeamSettingsPage(label: String, anchor: SemanticsMatcher) {
        openTeamRoot()
        tapClearOfBar(teamSettingsRow(label))
        flow.waitFor(hasTestTag(SettingsNav.PAGE_TEST_ID), NAV_TIMEOUT)
        flow.waitFor(anchor, SYNC_TIMEOUT)
        Espresso.closeSoftKeyboard()
        flow.settle()
    }

    private fun leaveTeamSettingsPage() {
        Espresso.closeSoftKeyboard()
        Espresso.pressBack()
        flow.waitFor(hasTestTag("team-tab"), NAV_TIMEOUT)
        flow.settle()
    }

    /**
     * EXP-1558: bring [matcher]'s node to the middle of the screen (web's
     * `scrollIntoView({ block: center })`): scroll it in, then drag the page
     * by the distance to the centre, slowly so it does not fling.
     */
    private fun centreOnScreen(matcher: SemanticsMatcher) {
        val node = composeRule.onAllNodes(matcher).onFirst()
        node.performScrollTo()
        flow.settle()
        val rootHeight = composeRule.onRoot().fetchSemanticsNode().size.height.toFloat()
        val centre = node.fetchSemanticsNode().boundsInRoot.center.y
        val dy = centre - rootHeight / 2f
        if (dy > 24f) {
            composeRule.onRoot().performTouchInput {
                swipe(Offset(centerX, centerY + dy / 2f), Offset(centerX, centerY - dy / 2f), durationMillis = 800)
            }
            flow.settle()
        }
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
        // #EXP-1645: on the Team page itself a tap on the tab opens the team
        // menu, which then swallows the next row's tap.
        if (!flow.exists(hasTestTag("team-tab"))) {
            composeRule.onNode(hasTestTag(Chats.TestIds.TAB_TEAM)).performClick()
            flow.settle()
        }
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
        // The row can sit under the floating tab bar once scrolled to (the
        // Apps row pushed it down): click through semantics, not a touch.
        composeRule.onAllNodes(row).onFirst().performScrollTo()
            .performSemanticsAction(SemanticsActions.OnClick)
        flow.waitFor(hasTestTag(SettingsNav.PAGE_TEST_ID), NAV_TIMEOUT)
    }
}
