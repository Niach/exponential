package com.exponential.app.ui.work

import android.content.Intent
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import com.exponential.app.domain.SessionDotTone
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.TopBarActionButton
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
    /**
     * EXP-895: the face's own action, left of the `…` — the Changes face's
     * "open the PR on GitHub" circle, which moved off the floating bar so the
     * bar's leading slot could carry the changed-files sheet.
     */
    action: (@Composable () -> Unit)? = null,
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
            action?.invoke()
            menu?.invoke()
        },
        colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
    )
}

/**
 * EXP-895: "open the PR on GitHub" as a header action. It used to be the
 * Changes bar's leading circle; the changed-files sheet took that slot, and a
 * link OUT of the app belongs with the other header verbs anyway.
 */
@Composable
fun GithubHeaderAction(prUrl: String) {
    val context = LocalContext.current
    TopBarActionButton(
        icon = ExpIcons.uiGithub,
        contentDescription = "Open PR on GitHub",
        onClick = {
            // A device with no browser (a stripped emulator image) throws
            // rather than resolving the intent — a dead tap beats a crash.
            runCatching {
                val intent = Intent(Intent.ACTION_VIEW, prUrl.toUri())
                intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                context.startActivity(intent)
            }
        },
        // EXP-862: a secondary header control is the GHOST variant — the glass
        // circle is left to the primary actions.
        borderless = true,
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
