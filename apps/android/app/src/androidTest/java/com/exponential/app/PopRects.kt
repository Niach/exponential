package com.exponential.app

import android.util.Log
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.onRoot
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File

/**
 * Pop-out rect sidecars for the Play Store slide compositor (EXP-627) — the
 * Android twin of `apps/ios/ExponentialUITests/PopRects.swift`, byte-identical
 * in identifier names, inset and output shape.
 *
 * The store compositor enlarges ONE detail of each raw capture into a floating
 * "pop-out" panel. The crop used to be hand-tuned per shot and silently drifted
 * whenever the layout moved; this dumps the real on-screen geometry instead:
 * for every store shot, the union of the [IDENTIFIERS] test tags' bounds,
 * padded by [INSET_FRACTION] of the compose root, normalised to 0..1 against
 * that root.
 *
 * Normalising against the compose ROOT is exact here: MainActivity runs
 * `enableEdgeToEdge`, so the root spans the whole window — the same pixels
 * screengrab's UiAutomator strategy photographs.
 *
 * Output: `<externalFilesDir>/exp-pop/pop-<shot>.json`, i.e.
 * `/sdcard/Android/data/at.exponential/files/exp-pop/`. `fastlane screenshots`
 * `adb pull`s that directory next to the raw PNGs, and
 * `bun run screenshots:pop-sidecars -- --platform android` merges them into the
 * form-keyed `pop-<shot>.json` the compositor reads.
 *
 * Shape (one object per file, normalised 0..1, origin top-left):
 * ```json
 * { "shot": "2_issue-detail", "platform": "android",
 *   "device": "sdk_gphone64_arm64",
 *   "x": 0.04, "y": 0.31, "w": 0.92, "h": 0.28 }
 * ```
 *
 * A shot with no entry here — or whose tags are all absent from the tree — is
 * skipped silently (one log line, never a failure): a missing sidecar just
 * leaves the compositor on its hand-tuned default.
 */
object PopRects {

    private const val TAG = "EXP-627"

    /**
     * shot → the test tags whose union is the pop-out rect. Only the FIRST
     * match of each tag counts: `notification-row` and friends repeat down a
     * list, and the union of a whole list is the whole screen, which is not a
     * pop-out.
     *
     * The tags live on the product composables (`issue-description`,
     * `coding-now-row`, `start-coding-agent-picker`, `agent-feed-question`,
     * `pr-merge-bar`, `notification-row`, plus the pre-existing `action-row`,
     * `support-thread-row` and the EXP-642 `issue-row-<identifier>`) and are
     * mirrored 1:1 as iOS accessibility identifiers.
     */
    val IDENTIFIERS: Map<String, List<String>> = mapOf(
        "1_board" to listOf("issue-row-APP-5"),
        "2_issue-detail" to listOf("issue-description", "coding-now-row"),
        "3_start-coding" to listOf("start-coding-agent-picker"),
        "4_steering" to listOf("agent-feed-question"),
        "5_review" to listOf("pr-merge-bar"),
        "6_actions" to listOf("action-row"),
        "7_inbox" to listOf("notification-row"),
        "8_support" to listOf("support-thread-row"),
    )

    /**
     * Outward padding around the union, as a fraction of the root's width and
     * height — the pop-out panel needs a little air around what it magnifies.
     */
    const val INSET_FRACTION = 0.015f

    /**
     * Measures and writes the sidecar for [shot]. Call it immediately before
     * the matching `Screengrab.screenshot`, while the screen is settled —
     * [ScreenshotFlow.screenshot] does exactly that for the store suite.
     */
    fun dump(rule: ComposeTestRule, shot: String) {
        // A shot this run was told to skip must not leave a sidecar behind: it
        // would describe a screen the run never photographed.
        if (!ScreenshotFlow.isShotWanted(shot)) return
        val tags = IDENTIFIERS[shot]
        if (tags.isNullOrEmpty()) {
            Log.i(TAG, "pop rect: no tags registered for $shot — skipping")
            return
        }

        val root = runCatching { rule.onRoot().fetchSemanticsNode() }.getOrNull()
        val rootWidth = root?.size?.width?.toFloat() ?: 0f
        val rootHeight = root?.size?.height?.toFloat() ?: 0f
        if (rootWidth <= 0f || rootHeight <= 0f) {
            Log.w(TAG, "pop rect: no compose root bounds for $shot — skipping")
            return
        }

        var left = Float.MAX_VALUE
        var top = Float.MAX_VALUE
        var right = -Float.MAX_VALUE
        var bottom = -Float.MAX_VALUE
        var matched = false
        for (tag in tags) {
            val node = runCatching {
                rule.onAllNodes(hasTestTag(tag)).fetchSemanticsNodes().firstOrNull()
            }.getOrNull()
            if (node == null) {
                Log.i(TAG, "pop rect: $shot has no node \"$tag\"")
                continue
            }
            val bounds = node.boundsInRoot
            if (bounds.width <= 0f || bounds.height <= 0f) continue
            matched = true
            left = minOf(left, bounds.left)
            top = minOf(top, bounds.top)
            right = maxOf(right, bounds.right)
            bottom = maxOf(bottom, bounds.bottom)
        }
        if (!matched) {
            Log.i(TAG, "pop rect: $shot matched nothing — skipping")
            return
        }

        val padX = INSET_FRACTION * rootWidth
        val padY = INSET_FRACTION * rootHeight
        val x = ((left - padX) / rootWidth).coerceIn(0f, 1f)
        val y = ((top - padY) / rootHeight).coerceIn(0f, 1f)
        val w = ((right + padX) / rootWidth).coerceIn(0f, 1f) - x
        val h = ((bottom + padY) / rootHeight).coerceIn(0f, 1f) - y
        if (w <= 0f || h <= 0f) return

        write(shot, x, y, w, h)
    }

    private fun write(shot: String, x: Float, y: Float, w: Float, h: Float) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dir = File(context.getExternalFilesDir(null), "exp-pop")
        val device = android.os.Build.DEVICE ?: "emulator"
        val json = buildString {
            append("{")
            append("\"shot\":\"").append(shot).append("\",")
            append("\"platform\":\"android\",")
            append("\"device\":\"").append(device).append("\",")
            append("\"x\":").append(round4(x)).append(",")
            append("\"y\":").append(round4(y)).append(",")
            append("\"w\":").append(round4(w)).append(",")
            append("\"h\":").append(round4(h))
            append("}")
        }
        runCatching {
            dir.mkdirs()
            File(dir, "pop-$shot.json").writeText(json)
        }.onSuccess {
            Log.i(TAG, "pop rect: wrote pop-$shot.json $json")
        }.onFailure {
            Log.w(TAG, "pop rect: could not write $shot: ${it.message}")
        }
    }

    private fun round4(value: Float): String =
        String.format(java.util.Locale.US, "%.4f", value)
}
