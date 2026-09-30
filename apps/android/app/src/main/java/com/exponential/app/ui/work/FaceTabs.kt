package com.exponential.app.ui.work

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.material3.HorizontalDivider
import com.exponential.app.ui.theme.GlassTokens
import androidx.compose.foundation.layout.PaddingValues
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChanged
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
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.session.PastRunRow
import kotlin.math.abs

// EXP-1150: the Work screen's face TABS — the ONE segmented strip (the Inbox /
// My Issues control) naming every available face in its fixed order
// (`availableFaces` + `faceLabel`), plus the body SWIPE that moves to the
// neighbour (`swipeTarget`). It replaces the EXP-893 bottom-right switcher
// circle. The strip is part of the HEADER: [WorkFaceTabs] composes under the
// top bar's title row inside the Scaffold's `topBar` slot (8dp below it, ONE
// hairline under the strip), so title and tabs read as one band that never
// moves between faces. The row is `[strip][Merge PR pill]`: with a merge
// the strip shrinks left and the pill trails at the row's end (every face).
// [WorkFaceFrame] wraps the face BODY with the swipe.
// With two or more own runs the Run tab reads `Runs`, and tapping it while it
// is ALREADY selected opens the run menu under the strip (`<device> ·
// <when>`, a check on the shown run).

/** The minimum horizontal travel that counts as a face swipe. */
private val SwipeThreshold: Dp = 56.dp

/** The face body with the neighbour swipe; [padding] passes straight through. */
@Composable
fun WorkFaceFrame(
    faces: List<WorkFaceKind>,
    face: WorkFaceKind,
    padding: PaddingValues,
    onFace: (WorkFaceKind) -> Unit,
    content: @Composable (PaddingValues) -> Unit,
) {
    val latestOnFace by rememberUpdatedState(onFace)
    Box(modifier = Modifier.fillMaxSize().faceSwipe(faces, face) { latestOnFace(it) }) {
        content(padding)
    }
}

/**
 * The header's strip: 8dp under the title row, then ONE hairline closing the
 * band. Nothing at all (no strip, no hairline) with fewer than two faces.
 * [runs] feed the `Runs` menu (two or more = a menu).
 */
@Composable
fun WorkFaceTabs(
    faces: List<WorkFaceKind>,
    face: WorkFaceKind,
    onFace: (WorkFaceKind) -> Unit,
    runs: List<PastRunRow> = emptyList(),
    shownRunId: String? = null,
    onPickRun: (String) -> Unit = {},
    /** EXP-1150: the header's Merge PR ([MergePrHeaderPill]); null = none. */
    trailing: (@Composable () -> Unit)? = null,
) {
    if (faces.size < 2 && trailing == null) return
    val multipleRuns = runs.size >= 2
    var menuOpen by remember { mutableStateOf(false) }
    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Box(modifier = Modifier.weight(1f)) {
                if (faces.size >= 2) GlassSegmentedControl(
                    options = faces,
                    selected = face,
                    label = { faceLabel(it, multipleRuns) },
                    onSelect = { picked ->
                        // `selectable` fires for the ALREADY selected segment too:
                        // a second tap on `Runs` is the way into the run menu.
                        if (picked == WorkFaceKind.Run && face == WorkFaceKind.Run && multipleRuns) {
                            menuOpen = true
                        } else {
                            onFace(picked)
                        }
                    },
                    testTag = { faceTag(it) },
                    modifier = Modifier.testTag("work-face-tabs"),
                )
                // Anchored under the `Runs` segment: an invisible box spanning
                // exactly that segment's share of the strip hosts the menu.
                val runIndex = faces.indexOf(WorkFaceKind.Run)
                if (runIndex >= 0 && multipleRuns) {
                    Row(modifier = Modifier.matchParentSize()) {
                        if (runIndex > 0) Spacer(Modifier.weight(runIndex.toFloat()))
                        Box(Modifier.weight(1f).fillMaxHeight()) {
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
                        val after = faces.size - runIndex - 1
                        if (after > 0) Spacer(Modifier.weight(after.toFloat()))
                    }
                }
            }
            trailing?.invoke()
        }
        HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
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
