package com.exponential.app.ui.issue

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.AltRoute
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassSection

// The single compact agent/PR card on the issue detail (EXP-156) — it replaces
// the old separate SteerPanel + ChangesSection. One glass section stacking the
// applicable rows: a live "Coding now" session, the "Start coding" launcher (or
// a "no desktop online" hint), and the PR / branch summary linking to the
// dedicated Changes page. No inline Close PR, no GitHub link, no diff counts,
// no branchDiff fetch — merge/close moved to the Changes review page.

// The remote-start progress state — relocated here from the deleted SteerPanel.
// `isBatch` drives the "follow it in the Agents tab" caption for a 2+ run.
sealed interface SteerStartState {
    data object Idle : SteerStartState
    data object Sending : SteerStartState
    data class Sent(val deviceLabel: String, val isBatch: Boolean) : SteerStartState
    data class Failed(val message: String) : SteerStartState
}

internal val LiveGreen = Color(0xFF34D399)

@Composable
fun AgentPrCard(
    issue: IssueEntity,
    session: CodingSessionEntity?,
    sessionOwner: UserEntity?,
    steerEnabled: Boolean?,
    isMember: Boolean,
    devices: List<SteerDevice>?,
    startState: SteerStartState,
    startCandidates: List<StartIssueOption>,
    onStart: (SteerDevice, List<String>, SteerStartOptions) -> Unit,
    onWatch: (String) -> Unit,
    onOpenChanges: () -> Unit,
) {
    var sheetOpen by remember { mutableStateOf(false) }

    val startVisible = session == null && isMember && steerEnabled == true
    val showStartButton = startVisible && !devices.isNullOrEmpty()
    val showStartHint = startVisible && devices != null && devices.isEmpty()
    val hasPr = !issue.prUrl.isNullOrBlank()
    val hasBranch = !hasPr && !issue.branch.isNullOrBlank()

    val hasContent = session != null || showStartButton || showStartHint || hasPr || hasBranch

    if (hasContent) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .glassSection()
                .padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            if (session != null) {
                SessionRow(
                    session = session,
                    sessionOwner = sessionOwner,
                    steerEnabled = steerEnabled,
                    isMember = isMember,
                    onWatch = onWatch,
                )
            }
            if (startVisible) {
                StartSection(
                    devices = devices,
                    startState = startState,
                    onOpenSheet = { sheetOpen = true },
                )
            }
            if (hasPr) {
                PrRow(prState = issue.prState, prNumber = issue.prNumber, onOpenChanges = onOpenChanges)
            } else if (hasBranch) {
                BranchRow(branch = issue.branch!!, onOpenChanges = onOpenChanges)
            }
        }
    }

    if (sheetOpen) {
        StartCodingSheet(
            devices = devices ?: emptyList(),
            issues = startCandidates,
            preselectedIds = setOf(issue.id),
            onStart = onStart,
            onDismiss = { sheetOpen = false },
        )
    }
}

// Live session: pulsing dot + "Coding now" + who/where, tapping into the steer
// viewer when steering is available; an inert caption when it's off.
@Composable
private fun SessionRow(
    session: CodingSessionEntity,
    sessionOwner: UserEntity?,
    steerEnabled: Boolean?,
    isMember: Boolean,
    onWatch: (String) -> Unit,
) {
    val watchable = isMember && steerEnabled == true
    Column {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .let { if (watchable) it.clickable { onWatch(session.id) } else it },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            PulsingDot()
            Spacer(Modifier.width(8.dp))
            Text(
                "Coding now",
                style = MaterialTheme.typography.labelLarge,
                color = LiveGreen,
            )
            Spacer(Modifier.width(8.dp))
            val who = userDisplayName(sessionOwner, session.userId)
            val device = session.deviceLabel?.takeIf { it.isNotBlank() }
            Text(
                "· $who" + if (device != null) " · $device" else "",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            if (watchable) {
                Icon(
                    Icons.AutoMirrored.Filled.KeyboardArrowRight,
                    contentDescription = "Watch live",
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
        if (isMember && steerEnabled == false) {
            Spacer(Modifier.height(6.dp))
            Text(
                "Live steering is unavailable on this instance.",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

// The "Start coding" launcher (or a hint when no desktop is online). devices
// null renders nothing (still loading); empty renders the hint; otherwise the
// button + the in-flight/sent/failed captions.
@Composable
private fun StartSection(
    devices: List<SteerDevice>?,
    startState: SteerStartState,
    onOpenSheet: () -> Unit,
) {
    if (devices == null) return
    if (devices.isEmpty()) {
        Text(
            "No desktop online — open the Exponential desktop app to run here.",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        return
    }

    val busy = startState is SteerStartState.Sending || startState is SteerStartState.Sent
    Column {
        Row(
            modifier = Modifier
                .glassButton()
                .clickable(enabled = !busy) { onOpenSheet() }
                .padding(horizontal = 14.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            if (startState is SteerStartState.Sending) {
                CircularProgressIndicator(
                    modifier = Modifier.size(14.dp),
                    strokeWidth = 2.dp,
                    color = MaterialTheme.colorScheme.onSurface,
                )
            } else {
                Icon(
                    Icons.Filled.PlayArrow,
                    contentDescription = null,
                    modifier = Modifier.size(16.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(
                        alpha = if (busy) TextEmphasis.Quaternary else TextEmphasis.Primary,
                    ),
                )
            }
            Text(
                if (devices.size == 1) {
                    "Start coding on ${devices[0].deviceLabel.ifBlank { devices[0].deviceId }}"
                } else {
                    "Start coding"
                },
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSurface.copy(
                    alpha = if (busy) TextEmphasis.Quaternary else TextEmphasis.Primary,
                ),
            )
        }
        when (startState) {
            is SteerStartState.Sent -> {
                Spacer(Modifier.height(6.dp))
                if (startState.isBatch) {
                    Text(
                        "Batch start sent to ${startState.deviceLabel} — follow it in the Agents tab.",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                } else {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(12.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        Text(
                            "Start sent to ${startState.deviceLabel} — waiting for the desktop…",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        )
                    }
                }
            }
            is SteerStartState.Failed -> {
                Spacer(Modifier.height(6.dp))
                Text(
                    startState.message,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
            else -> Unit
        }
    }
}

// Linked PR: a capitalized state pill + "PR #n", tapping into the Changes page.
@Composable
private fun PrRow(prState: String?, prNumber: Int?, onOpenChanges: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onOpenChanges),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (!prState.isNullOrBlank()) {
            Text(
                prState.replaceFirstChar { it.uppercase() },
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier
                    .glassButton()
                    .padding(horizontal = 8.dp, vertical = 3.dp),
            )
        }
        Text(
            prNumber?.let { "PR #$it" } ?: "Pull request",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.weight(1f),
        )
        Icon(
            Icons.AutoMirrored.Filled.KeyboardArrowRight,
            contentDescription = "View changes",
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

// Pushed branch, no PR yet: branch icon + mono branch, tapping into Changes.
@Composable
private fun BranchRow(branch: String, onOpenChanges: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onOpenChanges),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            Icons.AutoMirrored.Filled.AltRoute,
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Text(
            branch,
            style = MaterialTheme.typography.labelMedium.copy(fontFamily = FontFamily.Monospace),
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Icon(
            Icons.AutoMirrored.Filled.KeyboardArrowRight,
            contentDescription = "View changes",
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

// Pulsing live-session dot; internal so the Agents tab reuses the exact
// "Coding now" pulse (its import survives this relocation — same package).
@Composable
internal fun PulsingDot(size: androidx.compose.ui.unit.Dp = 8.dp) {
    val transition = rememberInfiniteTransition(label = "coding-now")
    val alpha by transition.animateFloat(
        initialValue = 1f,
        targetValue = 0.25f,
        animationSpec = infiniteRepeatable(
            animation = tween(900, easing = LinearEasing),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "coding-now-alpha",
    )
    Box(
        modifier = Modifier
            .size(size)
            .clip(CircleShape)
            .background(LiveGreen.copy(alpha = alpha)),
    )
}
