package com.exponential.app.ui.work

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.ChangesFaceCounts
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.domain.DetailChrome
import com.exponential.app.domain.SessionDotTone
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.issue.LiveGreen
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.session.LostGray
import androidx.compose.foundation.background
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import com.exponential.app.domain.Diff
import com.exponential.app.domain.WorkFaceKind
import com.exponential.app.domain.changesFaceText
import com.exponential.app.domain.faceLabel
import com.exponential.app.domain.isLiveRun
import com.exponential.app.domain.issueRunWhen
import com.exponential.app.domain.pastRunByline
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.GlassSegmentedControlDefaults
import com.exponential.app.ui.components.LocalDetailHazeSourceActive
import com.exponential.app.ui.components.TabPager
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.session.PastRunRow

// EXP-1150: the Work screen's face TABS — the ONE segmented strip (the Inbox /
// My Issues control) naming every available face in its fixed order
// (`availableFaces` + `faceLabel`), plus the body SWIPE that moves to the
// neighbour (`swipeTarget`). It replaces the EXP-893 bottom-right switcher
// circle. The strip is part of the HEADER: [WorkFaceTabs] composes under the
// top bar's title row inside the Scaffold's `topBar` slot (8dp below it), so
// title and tabs read as one band that never
// moves between faces. The row is `[strip][Merge PR pill]`: with a merge
// the strip shrinks left and the pill trails at the row's end (every face).
// [WorkFaceFrame] hosts the face BODIES as a pager (EXP-1152: the neighbour
// follows the finger like native tabs), and the Changes segment wears the
// diff's `+N −M` once known (desktop `FaceToggle::diff`, [changesFaceCounts]).
// With two or more own runs the Run tab reads `Runs`, and tapping it while it
// is ALREADY selected opens the run menu under the strip (`<device> ·
// <when>`, a check on the shown run).

/**
 * EXP-1152: the face bodies as a native PAGER ([TabPager], EXP-1190 lifted the
 * generic sync out of here) — the neighbour follows the finger, a tab tap
 * animates there, a faces change snaps. [padding] passes straight through to
 * [content], called per PAGE.
 */
@Composable
fun WorkFaceFrame(
    faces: List<WorkFaceKind>,
    face: WorkFaceKind,
    padding: PaddingValues,
    onFace: (WorkFaceKind) -> Unit,
    content: @Composable (WorkFaceKind, PaddingValues) -> Unit,
) {
    TabPager(
        pages = faces,
        selected = face,
        onSelect = onFace,
        key = { it.name },
        modifier = Modifier.fillMaxSize(),
    ) { pageFace ->
        CompositionLocalProvider(LocalDetailHazeSourceActive provides (pageFace == face)) {
            content(pageFace, padding)
        }
    }
}

/**
 * The header's strip: 8dp under the title row. EXP-1162: no hairline closes
 * the band any more — the detail chrome's edge strip does. Nothing at all
 * with fewer than two faces.
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
    /** EXP-1152: the Changes face's diff counts; null = the word `Changes`. */
    changesCounts: ChangesFaceCounts? = null,
    /** EXP-1162: the tabs' state dots (`DetailChrome.faceDots`). */
    dots: Map<WorkFaceKind, SessionDotTone> = emptyMap(),
    /** EXP-1162: the shown run's agent (its Run mark); EXP-1184: its display
     *  state (`codingSessionDisplayState`) — working spark or state badge. */
    runAgent: String? = null,
    runState: CodingSessionDisplayState? = null,
) {
    // EXP-1154: the strip is the tabs alone, centred — Merge PR went back to
    // the floating bar ([MergeCapsule]).
    if (faces.size < 2) return
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
                    // The counts' ONE string is the segment's accessible name.
                    label = { f ->
                        if (f == WorkFaceKind.Changes && changesCounts != null) {
                            changesFaceText(changesCounts)
                        } else {
                            faceLabel(f, multipleRuns)
                        }
                    },
                    labelContent = { f ->
                        if (f == WorkFaceKind.Changes && changesCounts != null) {
                            val slot: @Composable (Color) -> Unit = { color ->
                                ChangesCountsLabel(changesCounts, color.alpha)
                            }
                            slot
                        } else {
                            null
                        }
                    },
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
                    // EXP-1162: the state the header title no longer wears,
                    // said out loud with the label. The Run tab wears the
                    // run's agent mark LEADING it; every other tone (an open
                    // PR's `Review`) a dot trailing it.
                    leading = { f ->
                        dots[f]?.takeIf { f == WorkFaceKind.Run }?.let { tone ->
                            { FaceMark(runAgent.orEmpty(), tone, runState) }
                        }
                    },
                    trailing = { f ->
                        dots[f]?.takeIf { f != WorkFaceKind.Run }?.let { tone -> { FaceDot(tone) } }
                    },
                    description = { f ->
                        DetailChrome.faceDotCaption(dots[f])?.let { caption ->
                            val name = if (f == WorkFaceKind.Changes && changesCounts != null) {
                                changesFaceText(changesCounts)
                            } else {
                                faceLabel(f, multipleRuns)
                            }
                            "$name, $caption"
                        }
                    },
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
        }
    }
}

/**
 * EXP-1162 / EXP-1184: the Run tab's mark, `faceMark` square and
 * `faceMarkGap` before the label — the ×4 run mark ([AgentRunMark]): the
 * agent's working mark while it works, else its brand mark with an amber
 * (needs input) / green (PR open) / blue (done) `faceMarkBadge`. [state] null
 * falls back to the tab's tone (amber only for `NeedsInput`). Only a live run
 * gets here.
 */
@Composable
private fun FaceMark(agent: String, tone: SessionDotTone, state: CodingSessionDisplayState?) {
    val shown = state ?: if (tone == SessionDotTone.NeedsInput) CodingSessionDisplayState.NeedsInput else null
    AgentRunMark(
        agent = agent,
        state = shown,
        size = DetailChrome.FACE_MARK.dp,
        badgeSize = DetailChrome.FACE_MARK_BADGE.dp,
        modifier = Modifier.testTag("work-face-run-mark"),
    )
    Spacer(Modifier.width(DetailChrome.FACE_MARK_GAP.dp))
}

/**
 * EXP-1162: a tab's state dot — `faceDot` wide, `faceDotGap` after the label,
 * in the session-dot table's colour for its tone.
 */
@Composable
private fun FaceDot(tone: SessionDotTone) {
    val color = when (tone) {
        SessionDotTone.Running -> LiveGreen
        SessionDotTone.NeedsInput -> NeedsInputAmber
        SessionDotTone.Review -> ReviewGreen
        SessionDotTone.Done -> DoneBlue
        SessionDotTone.Muted -> LostGray
    }
    Spacer(Modifier.width(DetailChrome.FACE_DOT_GAP.dp))
    Box(
        Modifier
            .size(DetailChrome.FACE_DOT.dp)
            .background(color, CircleShape),
    )
}

/**
 * EXP-1152: the Changes segment's label — the desktop `FaceToggle::diff` pair:
 * mono `+N` / `−M` in the diff's own add/delete tints, at the strip's label
 * size and constant weight so the segment never re-measures between faces.
 * [alpha] carries the strip's selected/unselected emphasis (EXP-698).
 */
@Composable
private fun ChangesCountsLabel(counts: ChangesFaceCounts, alpha: Float) {
    val size = MaterialTheme.typography.labelLarge.fontSize
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(
            Diff.additionsLabel(counts.additions),
            color = DesignTokens.Diff.AddFg.copy(alpha = alpha),
            fontFamily = FontFamily.Monospace,
            fontSize = size,
            fontWeight = GlassSegmentedControlDefaults.LabelWeight,
            maxLines = 1,
        )
        Text(
            Diff.deletionsLabel(counts.deletions),
            color = DesignTokens.Diff.DelFg.copy(alpha = alpha),
            fontFamily = FontFamily.Monospace,
            fontSize = size,
            fontWeight = GlassSegmentedControlDefaults.LabelWeight,
            maxLines = 1,
        )
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
