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
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Tv
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Icon
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
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassSection

// The steer panel on the issue detail (masterplan §5b/§5c, replaces the old
// AgentPlanPanel): with a running coding_sessions row it shows a live "Coding
// now" badge + "Watch live" (relay viewer); with none it offers "Start on my
// desktop" against the user's online devices (relay presence). Everything
// degrades to nothing when the relay is off or the caller isn't a member.

sealed interface SteerStartState {
    data object Idle : SteerStartState
    data object Sending : SteerStartState
    data class Sent(val deviceLabel: String) : SteerStartState
    data class Failed(val message: String) : SteerStartState
}

private val LiveGreen = Color(0xFF34D399)

@Composable
fun SteerPanel(
    session: CodingSessionEntity?,
    sessionOwner: UserEntity?,
    steerEnabled: Boolean?,
    isMember: Boolean,
    devices: List<SteerDevice>?,
    startState: SteerStartState,
    onStart: (SteerDevice, SteerStartOptions) -> Unit,
    onWatch: (String) -> Unit,
) {
    if (session != null) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .glassSection()
                .padding(12.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
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
                    who + if (device != null) " · $device" else "",
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    maxLines = 1,
                    modifier = Modifier.weight(1f),
                )
            }
            if (isMember && steerEnabled == true) {
                Spacer(Modifier.height(10.dp))
                Row(
                    modifier = Modifier
                        .glassButton()
                        .clickable { onWatch(session.id) }
                        .padding(horizontal = 14.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Icon(
                        Icons.Filled.Tv,
                        contentDescription = null,
                        modifier = Modifier.size(15.dp),
                        tint = MaterialTheme.colorScheme.onSurface,
                    )
                    Text(
                        "Watch live",
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                }
            } else if (isMember && steerEnabled == false) {
                Spacer(Modifier.height(6.dp))
                Text(
                    "Live steering is unavailable on this instance.",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
        return
    }

    // No running session: "Start on my desktop" — only when the relay is on,
    // the caller is a member, and at least one of their desktops is online.
    if (steerEnabled != true || !isMember || devices.isNullOrEmpty()) return

    var sheetOpen by remember { mutableStateOf(false) }
    val busy = startState is SteerStartState.Sending || startState is SteerStartState.Sent

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .glassSection()
            .padding(12.dp),
    ) {
        Row(
            modifier = Modifier
                .glassButton()
                .clickable(enabled = !busy) { sheetOpen = true }
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
                if (devices.size == 1) "Start coding on ${devices[0].deviceLabel}" else "Start on my desktop",
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSurface.copy(
                    alpha = if (busy) TextEmphasis.Quaternary else TextEmphasis.Primary,
                ),
            )
        }
        when (startState) {
            is SteerStartState.Sent -> {
                Spacer(Modifier.height(6.dp))
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
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

    // The Start-coding options sheet (EXP-149): model/effort/ultracode/
    // plan-mode + the device choice when several desktops are online.
    if (sheetOpen) {
        StartCodingSheet(
            devices = devices,
            onStart = onStart,
            onDismiss = { sheetOpen = false },
        )
    }
}

// Pulsing live-session dot; internal so the Agents tab reuses the exact
// "Coding now" pulse.
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
