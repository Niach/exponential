package com.exponential.macrobenchmark

import android.content.Intent
import android.net.Uri
import androidx.benchmark.macro.CompilationMode
import androidx.benchmark.macro.ExperimentalMetricApi
import androidx.benchmark.macro.FrameTimingMetric
import androidx.benchmark.macro.MacrobenchmarkScope
import androidx.benchmark.macro.StartupMode
import androidx.benchmark.macro.TraceSectionMetric
import androidx.benchmark.macro.junit4.MacrobenchmarkRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.Direction
import androidx.test.uiautomator.UiObject2
import androidx.test.uiautomator.Until
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * EXP-1521 / EXP-1522 ("omega lag"): the two journeys Danny reported, as
 * frame-timing Macrobenchmarks against the `benchmark` build type.
 *
 * - [transcriptScroll]: fling a long session transcript up and back down.
 * - [sendThenBack]: type a message, send it and press back at once; the
 *   frames until the previous screen settles must not freeze (the 2–3 s stall).
 *
 * The app must already be signed in to an instance where the chat named by
 * the `chatId` instrumentation argument holds a session card titled
 * `sessionTitle` whose device publishes a long feed (README.md: the
 * screenshot seed + `SCREENSHOT_FEED_REPEAT=40 bun run screenshots:desktop`).
 * Both journeys open the chat through the `exponential://session/<id>` deep
 * link, then the session from its card.
 */
@RunWith(AndroidJUnit4::class)
class SessionBenchmarks {
    @get:Rule
    val rule = MacrobenchmarkRule()

    private val chatId: String
        get() = InstrumentationRegistry.getArguments().getString("chatId") ?: DEFAULT_CHAT_ID

    private val sessionTitle: String
        get() = InstrumentationRegistry.getArguments().getString("sessionTitle") ?: DEFAULT_SESSION_TITLE

    @Test
    fun transcriptScroll() = rule.measureRepeated(
        packageName = PACKAGE,
        metrics = listOf(FrameTimingMetric()),
        compilationMode = CompilationMode.Full(),
        startupMode = StartupMode.WARM,
        iterations = ITERATIONS,
        setupBlock = { openSession() },
    ) {
        val feed = transcript().visibleBounds
        // Fast swipes through the middle of the feed (clear of the header and
        // the composer): up into the history, then back down.
        val x = feed.centerX()
        val top = feed.top + feed.height() / 4
        val bottom = feed.top + feed.height() * 3 / 4
        repeat(FLINGS) {
            device.swipe(x, top, x, bottom, SWIPE_STEPS)
            device.waitForIdle()
        }
        repeat(FLINGS) {
            device.swipe(x, bottom, x, top, SWIPE_STEPS)
            device.waitForIdle()
        }
    }

    /**
     * Type steadily into the composer of a LIVE session for ~10 s (Danny's
     * "micro-lag every few seconds"): the agent's periodic `turn` frames
     * must not recompose the field, and every character must land.
     */
    @Test
    fun typeWhileLive() = rule.measureRepeated(
        packageName = PACKAGE,
        metrics = listOf(FrameTimingMetric()),
        compilationMode = CompilationMode.Full(),
        startupMode = StartupMode.WARM,
        iterations = ITERATIONS,
        setupBlock = { openSession() },
    ) {
        val field = composerField()
        field.click()
        val typed = StringBuilder()
        repeat(TYPED_CHARS) { i ->
            val c = 'a' + (i % 26)
            device.executeShellCommand("input text $c")
            typed.append(c)
        }
        // The IME commits asynchronously: give the last keystroke a moment.
        var landed = ""
        val until = System.currentTimeMillis() + 3_000
        while (System.currentTimeMillis() < until) {
            landed = composerField().text.orEmpty()
            if (landed.endsWith(typed.toString())) break
            Thread.sleep(100)
        }
        check(landed.endsWith(typed.toString())) { "the composer lost or reordered characters: `$landed`" }
        field.text = ""
    }

    /**
     * Cold-open the chat, then the session: the join replays the whole feed,
     * which used to be folded on the main thread in one block (EXP-1522).
     * [SessionOpenFrameMetric] reads the frames after the card's tap: the
     * `Choreographer#doFrame%` Max is the cold process's first frame (EGL +
     * shader bring-up), not the session's.
     */
    @OptIn(ExperimentalMetricApi::class)
    @Test
    fun openSession() = rule.measureRepeated(
        packageName = PACKAGE,
        metrics = listOf(
            FrameTimingMetric(),
            SessionOpenFrameMetric(),
            TraceSectionMetric("Choreographer#doFrame%", TraceSectionMetric.Mode.Max),
            TraceSectionMetric("SteerConnection.commitStaging", TraceSectionMetric.Mode.Sum),
            TraceSectionMetric("SteerConnection.frame", TraceSectionMetric.Mode.Sum),
        ),
        compilationMode = CompilationMode.Full(),
        // Cold: a fresh connection joins and replays the whole feed.
        startupMode = StartupMode.COLD,
        iterations = ITERATIONS,
        setupBlock = { pressHome() },
    ) {
        openChat()
        openSessionCard()
    }

    /**
     * #EXP-1538: open a session the app already followed (back out, then
     * reopen its card): `Dock.openToVisible` = the Dock's first composition
     * until it holds a card (dock.json LOCAL FIRST: the first frame, not
     * after the eNet join replay).
     */
    @OptIn(ExperimentalMetricApi::class)
    @Test
    fun reopenSessionDock() = rule.measureRepeated(
        packageName = PACKAGE,
        metrics = listOf(TraceSectionMetric(DOCK_TRACE, TraceSectionMetric.Mode.Max)),
        compilationMode = CompilationMode.Full(),
        startupMode = StartupMode.WARM,
        iterations = ITERATIONS,
        setupBlock = {
            openSession()
            // On a phone the session is a page of its chat's pager: Back
            // leaves the chat, the Conversation tab returns to it.
            device.wait(Until.findObject(By.res(CONVERSATION_TAB_TAG)), WAIT_MS)?.click()
                ?: error("no Conversation tab")
            device.wait(Until.gone(By.res(FEED_TAG)), WAIT_MS)
            device.waitForIdle()
        },
    ) {
        openSessionCard()
        check(device.wait(Until.hasObject(By.res(DOCK_PILE_TAG)), WAIT_MS) == true) { "no Dock card" }
    }

    /** #EXP-1538: the same on a cold process (nothing known yet: the join replay). */
    @OptIn(ExperimentalMetricApi::class)
    @Test
    fun coldOpenSessionDock() = rule.measureRepeated(
        packageName = PACKAGE,
        metrics = listOf(TraceSectionMetric(DOCK_TRACE, TraceSectionMetric.Mode.Max)),
        compilationMode = CompilationMode.Full(),
        startupMode = StartupMode.COLD,
        iterations = ITERATIONS,
        setupBlock = { pressHome() },
    ) {
        openChat()
        openSessionCard()
        check(device.wait(Until.hasObject(By.res(DOCK_PILE_TAG)), WAIT_MS) == true) { "no Dock card" }
    }

    @OptIn(ExperimentalMetricApi::class)
    @Test
    fun sendThenBack() = rule.measureRepeated(
        packageName = PACKAGE,
        metrics = listOf(
            FrameTimingMetric(),
            // The longest frame while backing out (the 2–3 s freeze lands here).
            TraceSectionMetric("Choreographer#doFrame%", TraceSectionMetric.Mode.Max),
            TraceSectionMetric("SteerConnection.commitStaging", TraceSectionMetric.Mode.Sum),
            TraceSectionMetric("SteerConnection.frame", TraceSectionMetric.Mode.Sum),
            TraceSectionMetric("ChatsViewModel.groups", TraceSectionMetric.Mode.Sum),
            TraceSectionMetric("AgentSessionViewModel.header", TraceSectionMetric.Mode.Sum),
        ),
        compilationMode = CompilationMode.Full(),
        startupMode = StartupMode.WARM,
        iterations = ITERATIONS,
        setupBlock = { openSession() },
    ) {
        val field = composerField()
        field.click()
        field.text = "perf ${System.nanoTime() % 100_000}"
        device.waitForIdle()
        composerSubmit().click()
        // Straight back, the way Danny does it: the first press may only close
        // the keyboard, the second leaves the screen.
        device.pressBack()
        if (device.hasObject(By.res(FEED_TAG))) device.pressBack()
        device.wait(Until.gone(By.res(FEED_TAG)), WAIT_MS)
        device.waitForIdle()
    }

    private fun MacrobenchmarkScope.openSession() {
        pressHome()
        openChat()
        openSessionCard()
    }

    private fun MacrobenchmarkScope.openChat() {
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse("exponential://session/$chatId"))
            .setPackage(PACKAGE)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        startActivityAndWait(intent)
        check(device.wait(Until.hasObject(By.res(CONVERSATION_TAG)), WAIT_MS) == true) {
            "chat `$chatId` did not open: is the app signed in?"
        }
        device.waitForIdle()
    }

    private fun MacrobenchmarkScope.openSessionCard() {
        val conversation = device.wait(Until.findObject(By.res(CONVERSATION_TAG)), WAIT_MS)
            ?: error("chat `$chatId` did not open: is the app signed in?")
        conversation.setGestureMargin(device.displayWidth / 5)
        // The card must sit in the middle third: the transcript scrolls under
        // the floating header band and the composer band, and a card found
        // under either takes no tap.
        val top = device.displayHeight / 3
        val bottom = device.displayHeight * 2 / 3
        var card = device.findObject(By.res(SESSION_CARD_TAG).hasDescendant(By.text(sessionTitle)))
        var scrolls = 0
        while (scrolls++ < MAX_CARD_SCROLLS) {
            val y = card?.visibleBounds?.centerY()
            when {
                y == null -> conversation.scroll(Direction.UP, 0.8f)
                y < top -> conversation.scroll(Direction.UP, 0.3f)
                y > bottom -> conversation.scroll(Direction.DOWN, 0.3f)
                else -> break
            }
            // A tap while the transcript still settles from the scroll
            // only stops it (the card never opened: `agent-feed` not found).
            device.waitForIdle()
            card = device.findObject(By.res(SESSION_CARD_TAG).hasDescendant(By.text(sessionTitle)))
        }
        (card ?: error("no session card `$sessionTitle` in chat `$chatId`")).click()
        // One retry: the first tap can still land on a settling list.
        if (device.wait(Until.hasObject(By.res(FEED_TAG)), WAIT_MS / 3) != true) {
            device.findObject(By.res(SESSION_CARD_TAG).hasDescendant(By.text(sessionTitle)))?.click()
        }
        transcript()
        // The join replay has landed once an agent bubble paints.
        device.wait(Until.hasObject(By.res(BUBBLE_TAG)), WAIT_MS)
        device.waitForIdle()
    }

    private fun MacrobenchmarkScope.transcript(): UiObject2 =
        device.wait(Until.findObject(By.res(FEED_TAG)), WAIT_MS)
            ?: error("transcript `$FEED_TAG` not found: is the app signed in and the session synced?")

    /** The steer field (untagged: the EditText), behind its capsule when collapsed. */
    private fun MacrobenchmarkScope.composerField(): UiObject2 {
        device.findObject(By.clazz(EDIT_TEXT).pkg(PACKAGE))?.let { return it }
        device.wait(Until.findObject(By.res(CAPSULE_TAG)), WAIT_MS)?.click()
        return device.wait(Until.findObject(By.clazz(EDIT_TEXT).pkg(PACKAGE)), WAIT_MS)
            ?: error("composer field not found")
    }

    private fun MacrobenchmarkScope.composerSubmit(): UiObject2 =
        device.wait(Until.findObject(By.desc(SUBMIT_DESC).pkg(PACKAGE)), WAIT_MS)
            ?: error("composer submit `$SUBMIT_DESC` not found")

    private companion object {
        const val PACKAGE = "at.exponential"
        /** The screenshot seed's demo chat (`DEMO_CHAT_ID`) and its steered session. */
        const val DEFAULT_CHAT_ID = "5d7a2c90-3e41-4b8f-9c16-8a2e4f0b7d13"
        const val DEFAULT_SESSION_TITLE = "Profile the launch path"
        const val CONVERSATION_TAG = "chat-conversation"
        const val CONVERSATION_TAB_TAG = "chat-tab-conversation"
        const val SESSION_CARD_TAG = "session-card"
        const val FEED_TAG = "agent-feed"
        const val BUBBLE_TAG = "session-agent-bubble"
        const val DOCK_PILE_TAG = "dock-pile"
        const val DOCK_TRACE = "Dock.openToVisible"
        const val CAPSULE_TAG = "steer-composer-capsule"
        const val EDIT_TEXT = "android.widget.EditText"
        const val SUBMIT_DESC = "Send"
        const val MAX_CARD_SCROLLS = 12
        const val TYPED_CHARS = 60
        const val ITERATIONS = 5
        const val FLINGS = 5
        const val SWIPE_STEPS = 8
        const val WAIT_MS = 15_000L
    }
}
