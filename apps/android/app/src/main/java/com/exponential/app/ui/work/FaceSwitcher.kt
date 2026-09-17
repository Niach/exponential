package com.exponential.app.ui.work

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.Diff
import com.exponential.app.domain.RUN_FACE_LABEL
import com.exponential.app.domain.START_CODING_LABEL
import com.exponential.app.domain.SessionDotTone
import com.exponential.app.domain.SwitcherBadge
import com.exponential.app.domain.SwitcherMode
import com.exponential.app.domain.SwitcherTarget
import com.exponential.app.domain.WorkFaceKind
import com.exponential.app.domain.faceLabel
import com.exponential.app.domain.isLiveRun
import com.exponential.app.domain.issueRunWhen
import com.exponential.app.domain.pastRunByline
import com.exponential.app.ui.components.BarCircle
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.issue.LiveGreen
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.PulsingDot
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.session.LostGray
import com.exponential.app.ui.session.PastRunRow
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis

// EXP-893: the Work screen's bottom-right circle — the face SWITCHER. With one
// target it wears that destination's glyph and switches on tap; with two or
// more it wears the faces glyph and opens a menu above itself (✕ while open).
// The badge dot in its corner is the shown session's state off the Run face
// and a green "changes are waiting" dot on it (`switcherBadge`).

/**
 * EXP-931: where the switcher is standing.
 *
 * [Circle] is the work bar's glass circle. [Inline] is the EXPANDED composer's
 * control row, beside the usage ring and wearing exactly the ring's chrome:
 * while that composer is open it covers the bar, and with it the bar's
 * switcher — so the way to Issue / Changes / Results moves in there rather
 * than disappearing. Exactly ONE of the two is ever mounted.
 */
enum class FaceSwitcherVariant { Circle, Inline }

/**
 * The circle plus its menu. [runs] are the issue's own runs (EXP-886) the
 * menu names as `<device> · <when>` rows; [shownRunId] gets the check;
 * [diffStats] trails the Changes row with `+A −D` when the diff is a live one.
 */
@Composable
fun FaceSwitcher(
    mode: SwitcherMode,
    badge: SwitcherBadge?,
    /** EXP-848: the session badge pulses only while the agent is mid-turn. */
    badgeBusy: Boolean,
    runs: List<PastRunRow>,
    shownRunId: String?,
    diffStats: Diff.Totals?,
    onPick: (SwitcherTarget) -> Unit,
    modifier: Modifier = Modifier,
    variant: FaceSwitcherVariant = FaceSwitcherVariant.Circle,
    /**
     * EXP-931: the menu's open state, for a host that has to know. The
     * expanded composer collapses itself once nobody is typing in it, and a
     * menu opened from INSIDE it takes focus with it — which is not the reader
     * leaving, so the trigger must not unmount under its own open menu.
     */
    onMenuOpenChange: (Boolean) -> Unit = {},
) {
    if (mode is SwitcherMode.Hidden) return
    var menuOpen by remember { mutableStateOf(false) }
    val setMenuOpen: (Boolean) -> Unit = { open ->
        menuOpen = open
        onMenuOpenChange(open)
    }
    val glyph: ImageVector = when (mode) {
        is SwitcherMode.Toggle -> targetGlyph(mode.target)
        is SwitcherMode.Menu -> if (menuOpen) ExpIcons.uiClose else ExpIcons.workFaces
        SwitcherMode.Hidden -> ExpIcons.workFaces
    }
    val description = when (mode) {
        is SwitcherMode.Toggle -> targetLabel(mode.target)
        else -> if (menuOpen) "Close" else "Switch face"
    }
    val inline = variant == FaceSwitcherVariant.Inline
    val onClick: () -> Unit = {
        when (mode) {
            is SwitcherMode.Toggle -> onPick(mode.target)
            is SwitcherMode.Menu -> setMenuOpen(!menuOpen)
            SwitcherMode.Hidden -> Unit
        }
    }
    // EXP-931: the composer's control row is 20dp glyphs (the usage ring is),
    // the bar's is the glass circle's 22dp on white.
    val tint = if (inline) {
        MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    } else {
        Color.White
    }
    val content: @Composable () -> Unit = {
        Box(contentAlignment = Alignment.Center) {
            val reduceMotion = LocalReduceMotion.current
            AnimatedContent(
                targetState = glyph,
                transitionSpec = {
                    val spec = Motion.standard<Float>(reduceMotion)
                    fadeIn(spec) togetherWith fadeOut(spec)
                },
                label = "face-switcher-glyph",
            ) { icon ->
                Icon(
                    icon,
                    contentDescription = description,
                    modifier = Modifier.size(if (inline) 20.dp else 22.dp),
                    tint = tint,
                )
            }
            if (badge != null && !menuOpen) {
                Box(
                    modifier = Modifier
                        .align(Alignment.TopEnd)
                        .offset(x = 3.dp, y = (-3).dp)
                        // The dot rides its own opaque disc so it stays
                        // legible where it overlaps the glyph.
                        .size(12.dp)
                        .clip(CircleShape)
                        .background(GlassTokens.OpaqueCardFill),
                    contentAlignment = Alignment.Center,
                ) {
                    BadgeDot(badge, busy = badgeBusy)
                }
            }
        }
    }
    Box(modifier = modifier) {
        if (inline) {
            IconButton(
                onClick = onClick,
                modifier = Modifier.testTag("work-face-switcher"),
            ) { content() }
        } else {
            BarCircle(
                onClick = onClick,
                modifier = Modifier.testTag("work-face-switcher"),
            ) { content() }
        }
        if (mode is SwitcherMode.Menu) {
            GlassDropdownMenu(
                expanded = menuOpen,
                onDismissRequest = { setMenuOpen(false) },
            ) {
                mode.targets.forEach { target ->
                    SwitcherRow(
                        target = target,
                        runs = runs,
                        shownRunId = shownRunId,
                        diffStats = diffStats,
                        onClick = {
                            setMenuOpen(false)
                            onPick(target)
                        },
                    )
                }
            }
        }
    }
    // A switcher that leaves while its menu is open must not strand the host
    // in "a menu is up" — the state goes with it.
    DisposableEffect(Unit) { onDispose { onMenuOpenChange(false) } }
}

@Composable
private fun BadgeDot(badge: SwitcherBadge, busy: Boolean) {
    when (badge) {
        SwitcherBadge.Changes -> StaticDot(LiveGreen, size = 8.dp)
        is SwitcherBadge.Session -> when (badge.tone) {
            SessionDotTone.Running -> if (busy) PulsingDot(size = 8.dp) else StaticDot(LiveGreen, size = 8.dp)
            SessionDotTone.Review -> StaticDot(ReviewGreen, size = 8.dp)
            SessionDotTone.NeedsInput -> StaticDot(NeedsInputAmber, size = 8.dp)
            SessionDotTone.Done -> StaticDot(DoneBlue, size = 8.dp)
            SessionDotTone.Muted -> StaticDot(LostGray, size = 8.dp)
        }
    }
}

@Composable
private fun SwitcherRow(
    target: SwitcherTarget,
    runs: List<PastRunRow>,
    shownRunId: String?,
    diffStats: Diff.Totals?,
    onClick: () -> Unit,
) {
    when (target) {
        is SwitcherTarget.Face -> GlassMenuItem(
            leadingIcon = { Icon(targetGlyph(target), contentDescription = null) },
            text = { Text(targetLabel(target)) },
            trailingIcon = if (target.face == WorkFaceKind.Changes && diffStats != null) {
                {
                    // EXP-895: `+A −D` in mono off the SHARED labels (the
                    // deletion count's minus is U+2212, never a hyphen) and
                    // the shared tokens, only off a LIVE diff — the PR files
                    // page sums its own.
                    Row {
                        Text(
                            Diff.additionsLabel(diffStats.additions),
                            color = DesignTokens.Diff.AddFg,
                            fontFamily = FontFamily.Monospace,
                            style = MaterialTheme.typography.labelSmall,
                        )
                        Text(
                            " ${Diff.deletionsLabel(diffStats.deletions)}",
                            color = DesignTokens.Diff.DelFg,
                            fontFamily = FontFamily.Monospace,
                            style = MaterialTheme.typography.labelSmall,
                        )
                    }
                }
            } else {
                null
            },
            onClick = onClick,
            modifier = Modifier.testTag(faceTag(target.face)),
        )
        is SwitcherTarget.Run -> {
            val run = runs.firstOrNull { it.session.id == target.id }
            val onShow = target.id == shownRunId
            val live = run != null && isLiveRun(run.session)
            GlassMenuItem(
                leadingIcon = when {
                    onShow -> ({ Icon(ExpIcons.uiCheck, contentDescription = null) })
                    live -> ({ Icon(ExpIcons.codingRunning, contentDescription = null) })
                    else -> ({ Icon(ExpIcons.navDevices, contentDescription = null) })
                },
                text = {
                    Text(
                        if (run == null) {
                            RUN_FACE_LABEL
                        } else {
                            // EXP-886: `<device> · Live` / `<device> · 2 h ago`.
                            pastRunByline(
                                deviceLabel = run.device.displayLabel,
                                timeLabel = issueRunWhen(
                                    run.session,
                                    endedRelative = relativeTime(run.session.endedAt ?: run.session.updatedAt),
                                ),
                            )
                        },
                    )
                },
                onClick = onClick,
                modifier = Modifier.testTag("work-run-${target.id}"),
            )
        }
        SwitcherTarget.StartCoding -> GlassMenuItem(
            leadingIcon = { Icon(ExpIcons.actionRun, contentDescription = null) },
            text = { Text(START_CODING_LABEL) },
            onClick = onClick,
            modifier = Modifier.testTag("work-face-start"),
        )
    }
}

private fun faceTag(face: WorkFaceKind): String = when (face) {
    WorkFaceKind.Issue -> "work-face-issue"
    WorkFaceKind.Run -> "work-face-run"
    WorkFaceKind.Changes -> "work-face-changes"
    WorkFaceKind.Results -> "work-face-results"
}

/** The destination's glyph — what a single-target circle wears. */
private fun targetGlyph(target: SwitcherTarget): ImageVector = when (target) {
    is SwitcherTarget.Face -> when (target.face) {
        WorkFaceKind.Issue -> ExpIcons.uiIssue
        WorkFaceKind.Run -> ExpIcons.navDevices
        WorkFaceKind.Changes -> ExpIcons.codingDiff
        WorkFaceKind.Results -> ExpIcons.workResults
    }
    is SwitcherTarget.Run -> ExpIcons.navDevices
    SwitcherTarget.StartCoding -> ExpIcons.actionRun
}

private fun targetLabel(target: SwitcherTarget): String = when (target) {
    // The face's OWN label, so Results never drifts from `faceLabel`.
    is SwitcherTarget.Face -> faceLabel(target.face)
    is SwitcherTarget.Run -> RUN_FACE_LABEL
    SwitcherTarget.StartCoding -> START_CODING_LABEL
}
