package com.exponential.app.ui.spike

import android.os.SystemClock
import android.util.Log
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.exponential.app.MainActivity
import com.exponential.app.ScreenshotFlow
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith

/**
 * VAPP-4 spike checks on the taffy kitchen sink. The screen opens through the
 * same `exp.devScreen` seam `adb shell am start --es exp.devScreen …` uses
 * (DevScreens); signed out it renders as an overlay, so no backend is needed.
 */
@RunWith(AndroidJUnit4::class)
class VappSpikeTest {

    private val composeRule = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(ScreenshotFlow.permissionRule()).around(composeRule)

    private fun open(screen: String) {
        composeRule.runOnUiThread {
            DevScreens.consume()
            DevScreens.open(screen)
        }
        composeRule.waitUntil(timeoutMillis = 20_000) {
            composeRule.onAllNodes(hasTestTag("vapp-surface")).fetchSemanticsNodes().isNotEmpty()
        }
        composeRule.waitUntil(timeoutMillis = 10_000) { captionText().contains("nodes") }
        composeRule.waitForIdle()
    }

    private fun captionText(): String = runCatching {
        composeRule.onNodeWithTag("vapp-caption", useUnmergedTree = true).fetchSemanticsNode()
            .config.getOrNull(SemanticsProperties.Text)?.joinToString("") { it.text } ?: ""
    }.getOrDefault("")

    private fun textOf(tag: String): String {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true).fetchSemanticsNode()
        node.config.getOrNull(SemanticsProperties.EditableText)?.let { return it.text }
        return node.config.getOrNull(SemanticsProperties.Text)?.joinToString("") { it.text } ?: ""
    }

    @Test
    fun typingFortyCharsFast() {
        open("kitchen-sink")
        val expected = "abcdefghijklmnopqrstuvwxyz0123456789ABCD"
        composeRule.onNodeWithTag("echo-field").performScrollTo().performClick()
        val t0 = SystemClock.elapsedRealtime()
        for (c in expected) {
            composeRule.onNodeWithTag("echo-field").performTextInput(c.toString())
        }
        val typedMs = SystemClock.elapsedRealtime() - t0
        composeRule.waitUntil(timeoutMillis = 2_000) { textOf("echo-host") == "host: $expected" }
        SystemClock.sleep(400)
        composeRule.waitForIdle()
        val field = textOf("echo-field")
        val host = textOf("echo-host")
        Log.i("VappSpike", "typing: injected=performTextInput x40 typed_ms=$typedMs field=\"$field\" host=\"$host\"")
        assertEquals(expected, field)
        assertEquals("host: $expected", host)
    }

    private data class Entry(val label: String, val top: Float, val left: Float)

    private fun labelOf(node: SemanticsNode): String? {
        val c = node.config
        c.getOrNull(SemanticsProperties.EditableText)?.let { e ->
            val ph = c.getOrNull(SemanticsProperties.Text)?.joinToString("") { it.text }
            return e.text.ifEmpty { ph ?: "" }.ifEmpty { null }
        }
        c.getOrNull(SemanticsProperties.Text)?.joinToString("") { it.text }?.takeIf { it.isNotBlank() }?.let { return it }
        c.getOrNull(SemanticsProperties.ContentDescription)?.joinToString(" ")?.takeIf { it.isNotBlank() }?.let { return it }
        return null
    }

    private fun collect(node: SemanticsNode, out: MutableList<Entry>) {
        labelOf(node)?.let {
            val b = node.boundsInRoot
            out += Entry(it, b.top, b.left)
        }
        node.children.forEach { collect(it, out) }
    }

    @Test
    fun semanticsOrder() {
        open("kitchen-sink")
        val surface = composeRule.onNodeWithTag("vapp-surface", useUnmergedTree = true).fetchSemanticsNode()
        val tree = mutableListOf<Entry>()
        surface.children.forEach { collect(it, tree) }
        val treeOrder = tree.map { it.label }
        val geometric = tree.sortedWith(compareBy<Entry>({ it.top }, { it.left })).map { it.label }
        Log.i("VappSpike", "a11y tree order: ${JSONArray(treeOrder)}")
        Log.i("VappSpike", "a11y geometric order: ${JSONArray(geometric)}")
        Log.i("VappSpike", "a11y geometric == tree: ${geometric == treeOrder}")
        // The avatar (its description + the initials text) leads; the check
        // starts after it, like the lane contract's expected pre-order.
        val afterAvatar = treeOrder.dropWhile { it == "Alex Chen" || it == "AC" }
        val first6 = afterAvatar.take(6)
        Log.i("VappSpike", "a11y first6: ${JSONObject(mapOf("first6" to JSONArray(first6)))}")
        assertEquals("Reddit radar", first6[0])
        assertTrue(first6[1], first6[1].startsWith("Kitchen sink"))
        assertEquals(listOf("3", "Scan now", "Sources", "r/selfhosted"), first6.subList(2, 6))
    }

    @Test
    fun benchTiming() {
        open("kitchen-sink-bench")
        val readings = mutableListOf<String>()
        try {
            for (memo in listOf(false, true)) {
                DevScreens.hostMemo = memo
                repeat(5) { i ->
                    // Tap = one more FULL layout pass (the caption bumps a tick
                    // read in measure, which nudges the viewport by 0.01 dp).
                    composeRule.onNodeWithTag("vapp-caption").performClick()
                    composeRule.waitForIdle()
                    SystemClock.sleep(300)
                    val r = captionText()
                    readings += r
                    Log.i("VappSpike", "bench reading memo=$memo ${i + 1}: $r")
                }
            }
        } finally {
            DevScreens.hostMemo = true
        }
        assertTrue(readings.all { it.contains("nodes") })
    }
}
