package com.exponential.app.ui.work

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChanged
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.SwipeDirection
import com.exponential.app.domain.WorkFaceKind
import com.exponential.app.domain.faceLabel
import com.exponential.app.domain.isLiveRun
import com.exponential.app.domain.issueRunWhen
import com.exponential.app.domain.pastRunByline
import com.exponential.app.domain.swipeTarget
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.GlassSegmentedControlDefaults
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.session.PastRunRow
import kotlin.math.abs

// EXP-1150: the Work screen's face TABS — the ONE segmented strip (the Inbox /
// My Issues control) directly under the top bar, naming every available face
// in its fixed order (`availableFaces` + `faceLabel`), plus the body SWIPE
// that moves to the neighbour (`swipeTarget`). It replaces the EXP-893
// bottom-right switcher circle. [WorkFaceFrame] is the one host both the Work
// screen and the workflow page mount ABOVE their `when (face)` body, so the
// strip keeps its position and height on every face. With two or more own
// runs the Run tab reads `Runs`, and tapping it while it is ALREADY selected
// opens the run menu under the strip (`<device> · <when>`, a check on the
// shown run).

/** The strip's block: its gap above, the control, its gap below. */
private val TabsTopGap: Dp = 4.dp
private val TabsBottomGap: Dp = 8.dp
private val TabsBlock: Dp = TabsTopGap + GlassSegmentedControlDefaults.Height + TabsBottomGap

/** The minimum horizontal travel that counts as a face swipe. */
private val SwipeThreshold: Dp = 56.dp

/**
 * The tabs over the face body. [padding] is the host Scaffold's; the body gets
 * it back with the strip's block added on top (only while the strip shows —
 * one face, no strip). [runs] feed the `Runs` menu (two or more = a menu).
 */
@Composable
fun WorkFaceFrame(
    faces: List<WorkFaceKind>,
    face: WorkFaceKind,
    padding: PaddingValues,
    onFace: (WorkFaceKind) -> Unit,
    runs: List<PastRunRow> = emptyList(),
    shownRunId: String? = null,
    onPickRun: (String) -> Unit = {},
    content: @Composable (PaddingValues) -> Unit,
) {
    val showTabs = faces.size >= 2
    val direction = LocalLayoutDirection.current
    val top = padding.calculateTopPadding()
    val latestOnFace by rememberUpdatedState(onFace)
    val inner = if (showTabs) {
        PaddingValues(
            start = padding.calculateStartPadding(direction),
            top = top + TabsBlock,
            end = padding.calculateEndPadding(direction),
            bottom = padding.calculateBottomPadding(),
        )
    } else {
        padding
    }
    Box(modifier = Modifier.fillMaxSize()) {
        Box(modifier = Modifier.fillMaxSize().faceSwipe(faces, face) { latestOnFace(it) }) {
            content(inner)
        }
        if (showTabs) {
            WorkFaceTabs(
                faces = faces,
                face = face,
                onFace = onFace,
                runs = runs,
                shownRunId = shownRunId,
                onPickRun = onPickRun,
                modifier = Modifier.padding(top = top),
            )
        }
    }
}

@Composable
private fun WorkFaceTabs(
    faces: List<WorkFaceKind>,
    face: WorkFaceKind,
    onFace: (WorkFaceKind) -> Unit,
    runs: List<PastRunRow>,
    shownRunId: String?,
    onPickRun: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val multipleRuns = runs.size >= 2
    var menuOpen by remember { mutableStateOf(false) }
    Box(
        modifier = modifier
            .fillMaxWidth()
            .padding(start = 16.dp, end = 16.dp, top = TabsTopGap, bottom = TabsBottomGap),
    ) {
        GlassSegmentedControl(
            options = faces,
            selected = face,
            label = { faceLabel(it, multipleRuns) },
            onSelect = { picked ->
                // `selectable` fires for the ALREADY selected segment too: a
                // second tap on `Runs` is the way into the run menu.
                if (picked == WorkFaceKind.Run && face == WorkFaceKind.Run && multipleRuns) {
                    menuOpen = true
                } else {
                    onFace(picked)
                }
            },
            testTag = { faceTag(it) },
            modifier = Modifier.testTag("work-face-tabs"),
        )
        GlassDropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
            runs.forEach { run ->
                RunMenuRow(
                    run = run,
                    shown = run.session.id == shownRunId,
                    onClick = {
                        menuOpen = false
                        onPickRun(run.session.id)
                    },
                )
            }
        }
    }
}

@Composable
private fun RunMenuRow(run: PastRunRow, shown: Boolean, onClick: () -> Unit) {
    val live = isLiveRun(run.session)
    GlassMenuItem(
        leadingIcon = when {
            shown -> ({ Icon(ExpIcons.uiCheck, contentDescription = null) })
            live -> ({ Icon(ExpIcons.codingRunning, contentDescription = null) })
            else -> ({ Icon(ExpIcons.navDevices, contentDescription = null) })
        },
        text = {
            // EXP-886: `<device> · Live` / `<device> · 2 h ago`.
            Text(
                pastRunByline(
                    deviceLabel = run.device.displayLabel,
                    timeLabel = issueRunWhen(
                        run.session,
                        endedRelative = relativeTime(run.session.endedAt ?: run.session.updatedAt),
                    ),
                ),
            )
        },
        onClick = onClick,
        modifier = Modifier.testTag("work-run-${run.session.id}"),
    )
}

private fun faceTag(face: WorkFaceKind): String = when (face) {
    WorkFaceKind.Issue -> "work-face-issue"
    WorkFaceKind.Run -> "work-face-run"
    WorkFaceKind.Changes -> "work-face-changes"
    WorkFaceKind.Results -> "work-face-results"
}

/**
 * The body swipe: OBSERVES the pointer on the Final pass without consuming
 * anything, and decides on release — at least [SwipeThreshold] sideways and
 * more than twice as far sideways as down. A drag a child already consumed
 * (the diff's horizontal code scroll, a text selection) is the child's, so
 * vertical lists, sideways code and the composer keep working.
 */
private fun Modifier.faceSwipe(
    faces: List<WorkFaceKind>,
    shown: WorkFaceKind,
    onFace: (WorkFaceKind) -> Unit,
): Modifier = pointerInput(faces, shown) {
    val threshold = SwipeThreshold.toPx()
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Final)
        val start: Offset = down.position
        var last = start
        var claimed = false
        while (true) {
            val event = awaitPointerEvent(PointerEventPass.Final)
            val change = event.changes.firstOrNull { it.id == down.id } ?: break
            if (change.isConsumed && change.positionChanged()) claimed = true
            last = change.position
            if (!change.pressed) break
        }
        if (claimed) return@awaitEachGesture
        val dx = last.x - start.x
        val dy = last.y - start.y
        if (abs(dx) >= threshold && abs(dx) > 2 * abs(dy)) {
            val direction = if (dx < 0) SwipeDirection.Left else SwipeDirection.Right
            swipeTarget(faces, shown, direction)?.let(onFace)
        }
    }
}
