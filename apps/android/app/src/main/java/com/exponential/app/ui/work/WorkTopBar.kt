package com.exponential.app.ui.work

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.SessionDotTone
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.issue.LiveGreen
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.PulsingDot
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.session.LostGray

// EXP-893: the Work screen's top bar — IDENTICAL across its faces, so it
// never jumps: back · a small session-state dot + the identifier (an issue
// subject) or the session's title (an issue-less run) · the trailing verbs.
// Stop / Resume show on the Run face ONLY; the issue `…` menu on every face
// of an issue subject. No second caption line, no plan chip, no Reconnect.

/** The trailing verb the Run face wears — see `primaryAction`. */
enum class WorkBarVerb { Stop, Resume }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WorkTopBar(
    title: String,
    /** The shown session's state; null draws no dot (no live run). */
    dotTone: SessionDotTone?,
    /** EXP-848: the running dot pulses only while the agent is mid-turn. */
    dotBusy: Boolean,
    onBack: () -> Unit,
    verb: WorkBarVerb?,
    verbEnabled: Boolean,
    onVerb: () -> Unit,
    /** The issue `…` menu, for an issue subject. */
    menu: (@Composable () -> Unit)?,
) {
    CenterAlignedTopAppBar(
        title = {
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (dotTone != null) {
                    SessionToneDot(dotTone, busy = dotBusy)
                    Spacer(Modifier.width(8.dp))
                }
                Text(
                    title,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.testTag("work-title"),
                )
            }
        },
        navigationIcon = { TopBarBackButton(onClick = onBack) },
        actions = {
            when (verb) {
                // EXP-818: ONE word for ending a run, wherever it is watched
                // from — a red pill beside the `…`, confirm-gated by the host.
                WorkBarVerb.Stop -> GlassPill(
                    "Stop",
                    icon = ExpIcons.codingStop,
                    size = PillSize.Sm,
                    tint = MaterialTheme.colorScheme.error,
                    enabled = verbEnabled,
                    onClick = onVerb,
                    modifier = Modifier.padding(end = 8.dp).testTag("stop-run"),
                )
                // EXP-773: only on the machine that still holds the run's
                // worktree (`resumeTargetFor`).
                WorkBarVerb.Resume -> GlassPill(
                    "Resume",
                    icon = ExpIcons.runResume,
                    size = PillSize.Sm,
                    enabled = verbEnabled,
                    onClick = onVerb,
                    modifier = Modifier.padding(end = 8.dp).testTag("resume-run"),
                )
                null -> Unit
            }
            menu?.invoke()
        },
        colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
    )
}

/** The ×4 session-dot palette (`SessionDotTone`) as the list rows draw it. */
@Composable
fun SessionToneDot(tone: SessionDotTone, busy: Boolean) {
    when (tone) {
        SessionDotTone.Running -> if (busy) PulsingDot() else StaticDot(LiveGreen)
        SessionDotTone.Review -> StaticDot(ReviewGreen)
        SessionDotTone.NeedsInput -> StaticDot(NeedsInputAmber)
        SessionDotTone.Done -> StaticDot(DoneBlue)
        SessionDotTone.Muted -> StaticDot(LostGray)
    }
}
