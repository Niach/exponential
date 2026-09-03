package com.exponential.app

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
import androidx.compose.ui.test.performTouchInput
import androidx.test.espresso.Espresso
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.rule.GrantPermissionRule
import org.junit.After
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
 *   sg_board-filters · sg_board-empty ·
 *   sg_board-bulk-edit · sg_issue-comments · sg_issue-properties ·
 *   sg_issue-create · sg_search · sg_my-issues · sg_agents ·
 *   sg_start-coding-actions · sg_start-coding-chat ·
 *   sg_machine-settings · sg_action-create · sg_automations-list ·
 *   sg_automations · sg_action-suggestions · sg_reviews ·
 *   sg_support-thread · sg_settings-root · sg_settings-team ·
 *   sg_settings-account · sg_onboarding
 *
 * EXP-642 reshuffled the front of the set: the old `sg_instance-picker` shot IS
 * the cloud chooser a first-run user meets, so it took over the `sg_sign-in`
 * name, and the password-form shot that used to carry it is gone (the form is a
 * self-hosting detail, not the sign-in surface). The login flow itself is
 * unchanged — the suite still signs in with it. EXP-566 had earlier retired
 * `sg_settings-personal` in favour of sg_settings-root + sg_settings-account.
 *
 * Prereqs: the seeded local backend (apps/web/scripts/seed-screenshots.ts)
 * reachable from the emulator (default http://10.0.2.2:5173, override via the
 * `instanceUrl` instrumentation argument / SCREENGRAB_INSTANCE_URL) PLUS, since
 * EXP-642, the relay stub: `bun run screenshots:desktop` (apps/web) registers
 * the demo user's OWN device row, which is what sg_machine-settings (gated
 * `isMine && registered`) and the two sg_start-coding-* shots photograph. No
 * steer RELAY traffic is needed beyond that registration — nothing here watches
 * a live session.
 *
 * Every shot gates on genuinely seeded content rather than on a screen merely
 * existing, so a stale/missing seed fails the run instead of quietly shipping
 * an empty state. The exception is called out where it happens (the automations
 * pair, whose rows are newer than this suite). The sign-in / polling / settle /
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

        // APP-15: an open PR in the Reviews queue.
        private const val REVIEW_ISSUE_TITLE = "Batch-edit labels from the board"

        // The most recently updated seeded support thread, and the last line of
        // its conversation.
        private const val SUPPORT_REPORTER = "Emma Fischer"
        private const val SUPPORT_MESSAGE_FRAGMENT = "thank you for the quick turnaround"

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

        // The close-out the seed's freshest agent-ended run carries (EXP-637) —
        // only the EXPANDED row shows it, so it is the post-tap gate.

        // The device `bun run screenshots:desktop` registers for the demo user.
        private const val DEMO_DEVICE_NAME = "Alex's MacBook Pro"

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

    /** Set at the very end of the walk — see [assertRequestedShotsWereReached]. */
    private var finished = false

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
        val missing = ScreenshotFlow.unreachedShots()
        assertTrue(
            "EXP-642 shots: ${missing.joinToString(", ")} — no such shot in this suite",
            missing.isEmpty(),
        )
    }

    @Test
    fun captureStyleguideScreenshots() {
        // --- sg_sign-in: the pre-login server chooser. The Screengrabfile
        // reinstalls the app, so the cold launch always lands on the untouched
        // chooser (cloud buttons + the demoted self-hosted link). Photograph it
        // BEFORE the flow reveals the URL field — it is the same surface the
        // web and desktop `sign-in` shots show.
        flow.awaitInstancePicker()
        flow.settle()
        flow.screenshot("sg_sign-in")

        // The password form is deliberately NOT photographed any more
        // (EXP-642) — the lane still drives it to get signed in.
        flow.chooseInstance(instanceUrl)
        flow.awaitLoginScreen()
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
        flow.screenshot("sg_board-switcher")

        // --- Create team: the switcher's own "New team" row (EXP-698 r5).
        // The row CLOSES the sheet and opens the name dialog — two stacked
        // bottom surfaces is a dead end on Android — so this waits on the
        // dialog's tag rather than on the sheet going away, and it dismisses
        // the sheet on the way for free.
        composeRule.onNode(hasTestTag("board-switcher-new-team")).performClick()
        flow.waitFor(hasTestTag("create-team-dialog"), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_onboarding-create-team")
        // Cancel, not back: the next shot starts from the plain board list, and
        // an unconfirmed dialog must leave no team behind.
        composeRule.onNode(hasText("Cancel")).performClick()
        flow.waitForGone(hasTestTag("create-team-dialog"), NAV_TIMEOUT)
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
        flow.screenshot("sg_board-filters")
        // The sheet animates out over the board — let it finish before the next
        // tap, or it lands on the dismissing scrim.
        Espresso.pressBack()
        flow.waitForGone(hasText("Clear all"), NAV_TIMEOUT)
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
        flow.waitFor(hasContentDescription("Filters"), NAV_TIMEOUT)
        flow.settle()

        // --- Create issue: the compose circle on the bottom bar only exists
        // while a board is in view, and CreateIssueScreen dismisses via
        // "Cancel" (it has no "Back" node).
        composeRule.onNode(hasContentDescription("New issue")).performClick()
        flow.waitFor(hasTestTag("create-issue-title-field"), NAV_TIMEOUT)
        flow.waitFor(hasText("New Issue"), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_issue-create")
        composeRule.onNode(hasContentDescription("Cancel")).performClick()
        flow.waitFor(hasContentDescription("Filters"), NAV_TIMEOUT)
        flow.settle()

        // --- Search: EXP-686 moved it off the bottom bar into the board
        // header (next to Filters), so it is a PUSHED screen now — tagged,
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
        flow.waitFor(hasContentDescription("Filters"), NAV_TIMEOUT)
        flow.settle()

        // --- My Issues: the My Work tab opens on the Inbox segment (EXP-58);
        // the segmented control's label is the only handle on it.
        composeRule.onNode(hasContentDescription("My Work")).performClick()
        flow.waitFor(hasText("My Issues"), NAV_TIMEOUT)
        composeRule.onAllNodes(hasText("My Issues")).onFirst().performClick()
        flow.waitFor(hasText(MY_ISSUE_TITLE, substring = true), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_my-issues")

        // --- Devices (EXP-686, the renamed Agents surface): since EXP-642
        // this lane needs the relay stub (`screenshots:desktop`) — the demo
        // user's own device row is what the next three shots are taken from,
        // and an empty machines list is not a useful reference shot either.
        composeRule.onNode(hasTestTag("tab-devices")).performClick()
        flow.waitFor(hasText("Devices"), NAV_TIMEOUT)
        flow.waitFor(hasText(DEMO_DEVICE_NAME, substring = true), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_agents")

        // --- Start coding, Actions + Chat tabs: the machine row's play glyph
        // opens the unified launch sheet. The tabs carry testTags because
        // "Actions" and "Chat" also read as ordinary nodes elsewhere in the
        // sheet. Nothing is ever submitted — a run would land on a real machine.
        composeRule.onAllNodes(hasContentDescription("Start coding")).onFirst().performClick()
        flow.waitFor(hasTestTag("start-coding-sheet"), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag("start-coding-tab-actions")).performClick()
        flow.waitFor(hasText(SEEDED_ACTION_NAME, substring = true), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_start-coding-actions")
        // EXP-698: the Chat tap has to be PROVEN to have landed — this shot
        // once came out byte-identical to the Actions one above, i.e. a
        // silently swallowed tap wrote the previous screen twice. Gate on the
        // two fields only the Chat tab renders, so a tap that does not land
        // fails the run instead of duplicating a shot.
        composeRule.onAllNodes(hasTestTag("start-coding-tab-chat")).onFirst().performClick()
        flow.waitFor(hasText("Prompt"), NAV_TIMEOUT)
        flow.waitFor(hasText("Repository"), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_start-coding-chat")
        // EXP-687: sheets carry no Cancel pill — back (like a swipe down)
        // dismisses.
        Espresso.pressBack()
        flow.waitForGone(hasTestTag("start-coding-sheet"), NAV_TIMEOUT)
        flow.settle(longer = true)

        // --- Machine settings: own, registered machines only — the row menu is
        // absent otherwise, which is why the relay stub is a prerequisite.
        composeRule.onAllNodes(hasContentDescription("Machine actions")).onFirst().performClick()
        composeRule.onAllNodes(hasText("Edit")).onFirst().performClick()
        flow.waitFor(hasTestTag("device-settings-sheet"), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_machine-settings")
        // EXP-694: the sheet autosaves and carries no bottom button at all —
        // back (like a swipe down) dismisses it, same as the start sheet.
        Espresso.pressBack()
        flow.waitForGone(hasTestTag("device-settings-sheet"), NAV_TIMEOUT)
        flow.settle(longer = true)

        // --- The Actions surface: four shots off one tab. EXP-686 gave it its
        // own bottom-bar entry (it used to ride the Agents header). The segment
        // is rememberSaveable, so re-select the Actions one explicitly by tag
        // rather than trusting where a previous visit left it.
        composeRule.onNode(hasTestTag("tab-actions")).performClick()
        composeRule.onNode(hasTestTag("actions-segment-actions")).performClick()
        flow.waitFor(hasText(SEEDED_ACTION_NAME, substring = true), SYNC_TIMEOUT)

        // --- Create action: "New action" rides the "Actions · count" section
        // header (EXP-574). The sheet is only photographed, never submitted —
        // creating would start a real builtin run on somebody's machine.
        composeRule.onNode(hasTestTag("new-action")).performClick()
        flow.waitFor(hasTestTag("create-action-sheet"), NAV_TIMEOUT)
        flow.waitFor(hasText("What should this action do?", substring = true), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_action-create")
        // EXP-687: sheets carry no Cancel pill — back (like a swipe down)
        // dismisses.
        Espresso.pressBack()
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
        flow.screenshot("sg_automations-list")

        // --- Automation editor: the owner-only "New automation" entry, present
        // in both the populated header and the empty state. (iOS additionally
        // hides it when the backend has no STEER_RELAY_URL and falls back to
        // editing a seeded row; here it is owner-gated only.)
        composeRule.onAllNodes(hasTestTag("new-automation")).onFirst().performClick()
        flow.waitFor(hasTestTag("automation-form-sheet"), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_automations")
        // EXP-687: sheets carry no Cancel pill — back (like a swipe down)
        // dismisses.
        Espresso.pressBack()
        flow.waitForGone(hasTestTag("automation-form-sheet"), NAV_TIMEOUT)
        flow.settle(longer = true)

        // --- Suggestions: shipped constants (ACTION_SUGGESTIONS), not seeded
        // rows — this one can be gated hard on a card.
        composeRule.onAllNodes(hasText("Suggestions")).onFirst().performClick()
        flow.waitFor(hasTestTag("suggestion-row"), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_action-suggestions")
        flow.settle()

        // --- Reviews: the cross-board open-PR queue; the seed leaves four open
        // PRs, of which APP-15 is the stable anchor.
        composeRule.onNode(hasContentDescription("Reviews")).performClick()
        flow.waitFor(hasText("Reviews"), NAV_TIMEOUT)
        flow.waitFor(hasText(REVIEW_ISSUE_TITLE, substring = true), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_reviews")

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
        flow.screenshot("sg_support-thread")
        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.settle()

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
        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.waitFor(hasText("Servers", ignoreCase = true), NAV_TIMEOUT)
        composeRule.onAllNodes(hasText("Add server")).onFirst().performClick()
        flow.chooseInstance(instanceUrl)
        flow.awaitLoginScreen()
        flow.submitLogin(ScreenshotFlow.NEWCOMER_EMAIL, ScreenshotFlow.NEWCOMER_PASSWORD)
        flow.waitFor(hasText("Get started"), SYNC_TIMEOUT)
        composeRule.onAllNodes(hasText("Get started")).onFirst().performClick()
        flow.waitFor(hasText("Set up your team"), NAV_TIMEOUT)
        // The mobile wizard shows the Create and Join cards on ONE step — this
        // single shot is the whole `onboarding` view on Android/iOS (there is no
        // separate create-team / join screen to photograph).
        flow.waitFor(hasText("Create team"), NAV_TIMEOUT)
        flow.settle()
        flow.screenshot("sg_onboarding")

        finished = true
    }

    /**
     * Board switcher → the named board. The sheet's rows carry the board name
     * as plain text; the first match is the row.
     */
    private fun switchBoard(name: String) {
        composeRule.onNode(hasContentDescription("Switch board")).performClick()
        flow.waitFor(hasText("Switch board"), NAV_TIMEOUT)
        flow.waitFor(hasText(name, substring = true), SYNC_TIMEOUT)
        composeRule.onAllNodes(hasText(name, substring = true)).onFirst().performClick()
        flow.waitForGone(hasText("Switch board"), NAV_TIMEOUT)
        flow.settle(longer = true)
    }
}
