package com.exponential.app

import android.content.Context
import android.graphics.Bitmap
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import com.exponential.app.domain.ChatExpress
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.TranscriptGrouping
import com.exponential.app.ui.chat.ChatConversation
import com.exponential.app.ui.chat.ChatRowMeta
import com.exponential.app.ui.chat.rememberExpressActionSink
import com.exponential.app.ui.components.LocalOverlayToaster
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.ToastHost
import com.exponential.app.ui.components.Toaster
import com.exponential.app.ui.theme.ExponentialTheme
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

/**
 * #EXP-2073 on a device, no backend (seeded rows): THE transcript draws an
 * agent text's Express blocks as surface cards between its prose bubbles
 * (one sender group), THE marker under a failed block, the skeleton / partial
 * surface of a block still streaming, and a press posts THE action (body +
 * card) that comes back as the person's row. With `-e proofShots true` each
 * state lands in the app's `files/express-proof/`.
 */
class ExpressBlocksDeviceTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val context: Context get() = InstrumentationRegistry.getInstrumentation().targetContext

    private fun shot(name: String) {
        if (InstrumentationRegistry.getArguments().getString("proofShots") != "true") return
        compose.waitForIdle()
        Thread.sleep(800)
        val bitmap = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot() ?: return
        val dir = File(context.filesDir, "express-proof").apply { mkdirs() }
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun show(content: @Composable () -> Unit) {
        compose.runOnUiThread {
            compose.activity.setContent {
                ExponentialTheme {
                    val toaster = remember { Toaster() }
                    CompositionLocalProvider(
                        LocalContentColor provides MaterialTheme.colorScheme.onBackground,
                        LocalToaster provides toaster,
                        LocalOverlayToaster provides toaster,
                    ) {
                        Box(Modifier.fillMaxSize().statusBarsPadding()) {
                            content()
                            ToastHost(toaster)
                        }
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    private fun waitFor(matcher: androidx.compose.ui.test.SemanticsMatcher, count: Int = 1) {
        compose.waitUntil(15_000) { compose.onAllNodes(matcher, useUnmergedTree = true).fetchSemanticsNodes().size >= count }
    }

    private fun user(id: String, body: String) = SessionTree.SessionChatRow.MessageRow(id, "user", VIEWER, body, queued = false)
    private fun agent(id: String, body: String) = SessionTree.SessionChatRow.MessageRow(id, "agent", null, body, queued = false)

    @Composable
    private fun Transcript(rows: List<SessionTree.SessionChatRow>, onPost: (ChatExpress.ActionResult.Post) -> Unit = {}) {
        ChatConversation(
            rows = rows,
            rootId = "r",
            meta = { row -> ChatRowMeta(authorName = if (row.author == "user") "You" else "Agent", time = TIMES[row.id] ?: "09:41") },
            onOpenCard = {},
            onAnswer = { _, _, _ -> },
            onDecidePlan = { _, _, _ -> },
            canUnqueue = { false },
            onUnqueue = {},
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 24.dp),
            onSurfaceAction = rememberExpressActionSink(onPost),
            chat = TranscriptGrouping.Chat("r", "Agent", null, null),
            viewerId = VIEWER,
            startAtTop = true,
        )
    }

    @Test
    fun historyDrawsProseAndTwoBlocks() {
        show { Transcript(listOf(user("u1", "How is the release?"), agent("a1", HISTORY))) }
        waitFor(hasTestTag(com.exponential.app.domain.Chats.TestIds.SURFACE_CARD), 2)
        waitFor(hasText("Build 1482 is green"))
        shot("history")
    }

    @Test
    fun aFailedBlockDrawsTheMarker() {
        show { Transcript(listOf(user("u1", "Show me the checks"), agent("a2", FAILED))) }
        waitFor(hasText(ChatExpress.Copy.BLOCK_FAILED))
        waitFor(hasText("All 214 tests passed"))
        shot("failed")
    }

    @Test
    fun aBlockMidStream() {
        // A live feed row while its text streams: the prose, the surface so
        // far (a sentence typing in), then a block whose surface has not
        // started yet (THE skeleton).
        show {
            Box(Modifier.padding(16.dp)) {
                com.exponential.app.ui.chat.ExpressSegments(
                    rowKey = "n:s1:msg_07",
                    segments = com.exponential.app.ui.chat.rememberExpressSegments("n:s1:msg_07", STREAMING, final = false)!!,
                ) { text, first, last ->
                    com.exponential.app.ui.chat.MessageBubble(text = text, own = false, time = if (last) "09:42" else null, author = if (first) "Agent" else null, tail = first)
                }
            }
        }
        waitFor(hasText("Deploying build 1482 to stag", substring = true))
        shot("mid-stream")
    }

    @Test
    fun aPressPostsTheActionAndItsRowComesBack() {
        var posted: ChatExpress.ActionResult.Post? = null
        show {
            var rows by remember { mutableStateOf(listOf(user("u1", "Ready to ship?"), agent("a3", PRESS))) }
            Transcript(rows) { post ->
                posted = post
                // The server's row for the press: the body's bubble, else THE marker.
                rows = rows + if (post.body.isEmpty()) {
                    SessionTree.SessionChatRow.ActionMarker("p1", VIEWER, post.card.name)
                } else {
                    user("p1", post.body)
                }
            }
        }
        waitFor(hasText("Ship it"))
        compose.onAllNodes(hasText("Ship it"), useUnmergedTree = true)[0].performClick()
        waitFor(hasText("Ship it"), 2)
        compose.onAllNodes(hasText("Hold"), useUnmergedTree = true)[0].performClick()
        waitFor(hasText("You pressed hold"))
        shot("action-press")
        val card = posted!!.card
        assertEquals("hold", card.name)
        assertEquals("m:a3#1", card.surfaceId)
        assertEquals("m:a3#1", card.block)
        assertEquals("a3", card.messageId)
    }

    private companion object {
        const val VIEWER = "viewer"
        val TIMES = mapOf("u1" to "09:40", "a1" to "09:41", "a2" to "09:41", "a3" to "09:41", "p1" to "09:42")

        val HISTORY = """
            Here is where the release stands:
            <a2ui>
            root = Stack(children=[title, tiles])
            title = Heading(text="Build 1482 is green", level="h3")
            tiles = Stack(direction="horizontal", gap="md", children=[tests, time])
            tests = StatTile(label="Tests", value="214 passed")
            time = StatTile(label="Duration", value="6 m 12 s")
            </a2ui>
            And the rollout so far:
            <a2ui>
            root = Stack(children=[staging, prod])
            staging = Progress(value=100, label="Staging", tone="success")
            prod = Progress(value=0, label="Production", valueText="waiting for you")
            </a2ui>
            Want me to ship it?
        """.trimIndent()

        val FAILED = """
            The checks:
            <a2ui>
            root = Text(text="All 214 tests passed")
            </a2ui>
            And the coverage chart:
            <a2ui>
            root = CoverageChart(values=[1, 2, 3])
            </a2ui>
            The chart did not come through, the numbers are in the PR.
        """.trimIndent()

        val STREAMING = "Rolling out now.\n<a2ui>\nroot = Stack(children=[a, b])\na = Heading(text=\"Build 1482\", level=\"h3\")\nb = Text(text=\"Deploying build 1482 to stag"

        val PRESS = """
            Build 1482 is ready.
            <a2ui>
            root = Stack(direction="horizontal", gap="sm", children=[ship, hold])
            ship = Button(label="Ship it", on={press: Event("reply", userMessage="Ship it")})
            hold = Button(label="Hold", variant="outline", on={press: Event("hold")})
            </a2ui>
        """.trimIndent()
    }
}
