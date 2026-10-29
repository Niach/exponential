package com.exponential.app

import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import com.exponential.app.domain.Chats
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.issue.START_CODING_READY_TAG
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeDown
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.swipeUp
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
 * Automated Play Store screenshots, driven by fastlane screengrab
 * (`bundle exec fastlane screenshots` in apps/android — see fastlane/Screengrabfile).
 *
 * Drives the REAL app UI end-to-end against a locally seeded backend
 * (apps/web/scripts/seed-screenshots.ts): signed in by [ScreenshotSessionRule]
 * (a reused session, EXP-1267; instance picker → password login only as the
 * fallback) → board → issue detail → live steering → the Agent page composer → PR review →
 * inbox (NEW-7: the actions slide left with the Actions tab). Play caps phone screenshots at 8; EXP-393
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
 * them the steering and start-coding shots are unreachable and the issue
 * detail renders "Live steering is unavailable on this instance."
 *
 * The sign-in flow, the polling helpers, the diff expansion and the capture
 * wrapper live in [ScreenshotFlow], shared with [StyleguideScreenshotsTest];
 * its KDoc carries the synchronization notes. `popRects = true` additionally
 * writes the pop-out rect sidecar the store compositor crops from (EXP-627, see
 * [PopRects]), and [ScreenshotFlow.screenshot] honours the lane's optional
 * `shots` allowlist (EXP-642).
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

        // A fragment of the question screenshot-desktop.ts ends its transcript
        // on (DEMO_FEED_QUESTION) — proof that relay frames actually arrived.
        // It has to be the LAST event: the feed is a bottom-anchored lazy list,
        // so anything above the fold is never composed and no matcher sees it.
        private const val FEED_QUESTION_FRAGMENT = "Lazy-load the markdown editor too"

        // The thread's child session the stand-in desktop publishes to the
        // relay (DEMO_SESSION_ID); the root's own card comes first and has no
        // feed of its own.
        private const val STEERED_SESSION_TITLE = "Profile the launch path"

        private const val NAV_TIMEOUT = ScreenshotFlow.NAV_TIMEOUT
        private const val SYNC_TIMEOUT = ScreenshotFlow.SYNC_TIMEOUT
        private const val FIRST_SYNC_TIMEOUT = ScreenshotFlow.FIRST_SYNC_TIMEOUT
        private const val NETWORK_TIMEOUT = ScreenshotFlow.NETWORK_TIMEOUT
    }

    private val permissionRule: GrantPermissionRule = ScreenshotFlow.permissionRule()

    private val composeRule = createAndroidComposeRule<MainActivity>()

    // EXP-1267: the demo session (reused across runs) is in place before the
    // activity launches, so the app opens signed in — no login UI.
    private val sessionRule = ScreenshotSessionRule(ScreenshotFlow.DEMO_EMAIL, ScreenshotFlow.DEMO_PASSWORD)

    // Permission grant and the session must land before the activity (and its
    // permission request) launches.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(permissionRule).around(sessionRule).around(composeRule)

    private val flow by lazy { ScreenshotFlow(composeRule) }

    private val instanceUrl: String = ScreenshotFlow.instanceUrl()

    /** Set at the very end of the walk — see [assertRequestedShotsWereReached]. */
    private var finished = false

    @Before
    fun setUp() {
        ScreenshotFlow.useUiAutomatorScreenshots()
        // EXP-812: the server mints the relay dial URL from its own
        // STEER_RELAY_URL. A capture host keeps that on ws://localhost:4002 —
        // which the browser lanes need and the emulator cannot reach — so the
        // 4_steering shot dials the host alias instead.
        ScreenshotFlow.applySteerRelayOverride()
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
    fun captureStoreScreenshots() {
        // --- Signed in by [sessionRule]; the instance picker + login form only
        // as the fallback (the email field shows once /api/auth-config resolved).
        if (!sessionRule.signedIn) flow.signIn(instanceUrl)
        // The app lands on the Agent tab; the board shot starts on Issues.
        flow.waitFor(hasTestTag(Chats.TestIds.TAB_BOARD), SYNC_TIMEOUT)
        composeRule.onNode(hasTestTag(Chats.TestIds.TAB_BOARD)).performClick()

        // --- Board: wait out session fetch + first Electric sync until the
        // showcase issue row is on screen (in-progress group sits at the top),
        // then until the transient "Syncing…" pill has cleared — it photobombed
        // the board shot once (EXP-348).
        flow.waitFor(hasText(SHOWCASE_ISSUE_TITLE), FIRST_SYNC_TIMEOUT)
        flow.waitForGone(hasText("Syncing", substring = true), FIRST_SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("1_board", popRects = true)

        // EXP-1154: from here on the flow drives the clock. The live run's
        // spinners and feed animations never let an auto-advancing clock go
        // idle.
        flow.pauseClock()

        // --- Issue detail: open APP-5 and wait for its markdown description.
        // The live session row above the thread is what makes this shot read
        // "an agent is coding on this right now" instead of "live steering is
        // unavailable on this instance".
        composeRule.onAllNodes(hasText(SHOWCASE_ISSUE_TITLE)).onFirst().performClick()
        flow.waitFor(hasText("Startup profiling", substring = true), NAV_TIMEOUT)
        // EXP-1303: Threads v2 dropped the Work screen's face strip; the issue
        // names its threads instead, and the live run's thread chip is what
        // proves it (iOS `StoreScreenshots` parity).
        flow.waitFor(hasTestTag("issue-thread-chip"), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("2_issue-detail", popRects = true)

        // --- Live steering: the chip opens the thread; its child's session
        // card (by title: the root's own card comes first) opens the session's
        // raw work (the pending question card still in place).
        // Gate on the FEED tag, not the screen: it shows "Connecting…" /
        // "Waiting for activity…" placeholders until the first relay frame
        // lands.
        composeRule.onAllNodes(hasTestTag("issue-thread-chip")).onFirst().performScrollTo()
        composeRule.onAllNodes(hasTestTag("issue-thread-chip")).onFirst().performClick()
        flow.waitFor(hasTestTag(SessionTree.ThreadCopy.TEST_ID_CONVERSATION), SYNC_TIMEOUT)
        // EXP-1323: the conversation opens at its live tail, so the child's
        // card sits above the fold and the lazy list never composes it; drag
        // back up to it like the styleguide suite and iOS (a real drag, a
        // programmatic scroll is re-pinned to the tail).
        val steeredCard = hasTestTag(SessionTree.CardCopy.TEST_ID_CARD) and
            hasText(STEERED_SESSION_TITLE, substring = true)
        repeat(6) {
            val shown = runCatching { composeRule.onAllNodes(steeredCard).onFirst().assertIsDisplayed() }.isSuccess
            if (!shown) {
                composeRule.onNode(hasTestTag(SessionTree.ThreadCopy.TEST_ID_CONVERSATION))
                    .performTouchInput { swipeDown(startY = centerY, endY = bottom - 200f) }
                flow.settle()
            }
        }
        flow.waitFor(steeredCard, SYNC_TIMEOUT)
        composeRule.onAllNodes(steeredCard).onFirst().performClick()
        flow.waitFor(hasTestTag("agent-feed"), NETWORK_TIMEOUT)
        // An EMPTY feed still renders the container (a relay the emulator can't
        // reach leaves the view "Reconnecting…" with nothing in it), so the tag
        // alone would happily photograph a blank screen. Gate on real content:
        // the dial goes to ws://10.0.2.2:4002 via the setUp override, whatever
        // the server minted (EXP-812) — see the Screengrabfile prereqs.
        flow.waitFor(hasText(FEED_QUESTION_FRAGMENT, substring = true), NETWORK_TIMEOUT)
        flow.settle(longer = true)
        flow.screenshot("4_steering", popRects = true)
        // Session → issue → board: the session page is a page OF the thread
        // (its strip stays on screen), so one Back leaves the thread.
        Espresso.pressBack()
        flow.waitForGone(hasTestTag("thread-tabs"), NAV_TIMEOUT)
        composeRule.onNode(hasContentDescription("Back")).performClick()
        flow.settle()

        // --- Start coding (EXP-825): from a repo-backed issue the demo user
        // is NOT already coding on, the bottom bar's start circle PUSHES the
        // Agent page with that issue chipped into the composer (the ONE
        // launcher). Needs an online desktop — EXP-1121: until every
        // readiness step is met the circle opens the "Ready to code?"
        // checklist instead, and it is inert while the inputs load, so the
        // flow waits for the READY circle. The store slide keeps its filename.
        flow.waitFor(hasText(START_CODING_ISSUE_TITLE), NAV_TIMEOUT)
        composeRule.onAllNodes(hasText(START_CODING_ISSUE_TITLE)).onFirst().performClick()
        flow.waitFor(hasTestTag(START_CODING_READY_TAG), NETWORK_TIMEOUT)
        composeRule.onNode(hasTestTag(START_CODING_READY_TAG)).performClick()
        flow.waitFor(hasTestTag("agent-composer"), NAV_TIMEOUT)
        // The chip proves the seed landed, not just the page.
        flow.waitFor(hasTestTag("agent-composer-chip-issue-APP-3"), SYNC_TIMEOUT)
        // The machine pool resolves after the page (steer config + the devices
        // shape); a shot before that reads "No desktop online".
        flow.waitForGone(hasText("No desktop online", substring = true), NETWORK_TIMEOUT)
        flow.settle()
        flow.screenshot("3_start-coding", popRects = true)
        // A pushed detail: back pops to the issue, then Back to the board.
        Espresso.pressBack()
        flow.settle(longer = true)
        composeRule.onNode(hasContentDescription("Back")).performClick()
        // The clock is paused (EXP-1154): pump until the pop lands on the
        // board and its tab bar is back (the retired 5_review block's first
        // waitFor used to do this).
        flow.waitFor(hasTestTag("tab-threads"), NAV_TIMEOUT)
        flow.settle()

        // S-3: the PR review slide (5_review) is retired with the Guide tab
        // it walked (stories replace the Guide; iOS retired 05_review too).

        // NEW-7: the Actions slide is gone with the tab (prompts replaced
        // actions: the composer's `/` menu and Settings → Prompts).

        // --- Inbox (thread 58cb32df: a page the Chats header's inbox circle
        // pushes; seeded with 5 notifications, 3 unread). Wait for a real
        // group row — capturing "You're all caught up" would silently ship
        // an empty screenshot.
        composeRule.onNode(hasTestTag("tab-threads")).performClick()
        flow.waitFor(hasTestTag(com.exponential.app.domain.Chats.TestIds.INBOX_BUTTON), NAV_TIMEOUT)
        composeRule.onNode(hasTestTag(com.exponential.app.domain.Chats.TestIds.INBOX_BUTTON)).performClick()
        flow.waitFor(hasText(SHOWCASE_ISSUE_TITLE, substring = true), SYNC_TIMEOUT)
        flow.settle()
        flow.screenshot("7_inbox", popRects = true)

        // (SLOP-4: the 8th slide, the Support inbox, is gone with the
        // surface — a reporter's conversation is the issue's comments now.)
        finished = true
    }
}
