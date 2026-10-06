package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.R
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.session.rememberWorkingMarkPulse
import com.exponential.app.ui.theme.LocalReduceMotion
import kotlinx.coroutines.delay

// EXP-1184: the agent's WORKING mark and a live run's state mark — the ×4
// rule (`fixtures/session-display.json`, web `AgentRunMark`/`ClaudeSpinner`
// in @exp/ui, desktop + iOS the same).

/** Claude's "writing" spark: [CLAUDE_SPARK_FRAMES] frames, hard cuts. */
private val ClaudeSparkFrames = intArrayOf(
    R.drawable.ic_agent_claude_writing_0,
    R.drawable.ic_agent_claude_writing_1,
    R.drawable.ic_agent_claude_writing_2,
    R.drawable.ic_agent_claude_writing_3,
    R.drawable.ic_agent_claude_writing_4,
    R.drawable.ic_agent_claude_writing_5,
    R.drawable.ic_agent_claude_writing_6,
    R.drawable.ic_agent_claude_writing_7,
)

/** One frame's hold, in ms (×4). */
internal const val CLAUDE_SPARK_FRAME_MS = 90L

/**
 * Claude at work: its own spark stepped frame by frame every
 * [CLAUDE_SPARK_FRAME_MS], looping, with hard cuts (no cross-fade). Reduced
 * motion holds the plain brand mark. The drawables carry the brand orange, so
 * nothing tints them.
 */
@Composable
fun ClaudeSparkSpinner(size: Dp, modifier: Modifier = Modifier) {
    if (LocalReduceMotion.current) {
        Icon(
            painterResource(R.drawable.ic_agent_claude),
            contentDescription = null,
            tint = Color.Unspecified,
            modifier = modifier.size(size).testTag("claude-spark-spinner"),
        )
        return
    }
    var frame by remember { mutableIntStateOf(0) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(CLAUDE_SPARK_FRAME_MS)
            frame = (frame + 1) % ClaudeSparkFrames.size
        }
    }
    Icon(
        painterResource(ClaudeSparkFrames[frame]),
        contentDescription = null,
        tint = Color.Unspecified,
        modifier = modifier.size(size).testTag("claude-spark-spinner"),
    )
}

/**
 * The agent at work: Claude's spark, or (any other agent) its brand mark with
 * the EXP-850 pulse. An unknown/blank agent reads as claude, the default.
 */
@Composable
fun AgentWorkingMark(
    agent: String?,
    size: Dp,
    modifier: Modifier = Modifier,
    fallbackTint: Color = LocalContentColor.current,
) {
    val id = agent?.trim()?.lowercase().orEmpty()
    if (id.isEmpty() || id == "claude") {
        ClaudeSparkSpinner(size, modifier)
        return
    }
    Icon(
        agentIconPainter(id),
        contentDescription = null,
        tint = agentIconTint(id, fallbackTint),
        modifier = modifier.size(size).alpha(rememberWorkingMarkPulse()),
    )
}

/** The badge colour each parked state wears; null = no badge. */
internal fun runMarkBadgeColor(state: CodingSessionDisplayState?): Color? = when (state) {
    CodingSessionDisplayState.NeedsInput -> NeedsInputAmber
    CodingSessionDisplayState.Review -> ReviewGreen
    CodingSessionDisplayState.Done -> DoneBlue
    else -> null
}

/**
 * A LIVE run's mark, wherever a run is named: [CodingSessionDisplayState.Working]
 * = the agent's working mark, no badge; NeedsInput / Review / Done = the brand
 * mark with an amber / green / blue badge at its top end corner. A null
 * [state] = a paused run: the bare mark.
 */
@Composable
fun AgentRunMark(
    agent: String?,
    state: CodingSessionDisplayState?,
    size: Dp,
    badgeSize: Dp,
    modifier: Modifier = Modifier,
) {
    val id = agent.orEmpty()
    Box(modifier = modifier.size(size)) {
        if (state == CodingSessionDisplayState.Working) {
            AgentWorkingMark(id, size)
        } else {
            Icon(
                agentIconPainter(id),
                contentDescription = null,
                tint = agentIconTint(id),
                modifier = Modifier.size(size),
            )
        }
        val badge = runMarkBadgeColor(state)
        if (badge != null) {
            Box(
                Modifier
                    .align(Alignment.TopEnd)
                    .offset(x = badgeSize / 3, y = -(badgeSize / 3))
                    .size(badgeSize)
                    .background(badge, CircleShape)
                    .testTag("run-mark-badge"),
            )
        }
    }
}

/** Sizes shared by the list rows' working mark (it rides the 8dp dot slot). */
internal val RowWorkingMarkSize = 12.dp
