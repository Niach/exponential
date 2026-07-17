package com.exponential.app.ui.session

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.SmartToy
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.issue.PulsingDot
import com.exponential.app.ui.issue.StartCodingSheet
import com.exponential.app.ui.issue.SteerStartState
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

/**
 * The Agents tab: a remote-start launcher over the caller's online desktops
 * (EXP-156) plus the coding sessions currently running across the active
 * account. Tapping a running row jumps straight into the live steer viewer when
 * the relay is configured; otherwise it falls back to the issue detail. The
 * trailing info button always opens the issue detail.
 */
@Composable
fun AgentsScreen(
    onOpenSteer: (codingSessionId: String) -> Unit,
    onOpenIssue: (issueId: String) -> Unit,
    viewModel: AgentsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val startState by viewModel.startState.collectAsStateWithLifecycle()
    val startCandidates by viewModel.startCandidates.collectAsStateWithLifecycle()

    // The device the launcher sheet was opened from (non-null = sheet open).
    var sheetDevice by remember { mutableStateOf<SteerDevice?>(null) }

    // Re-poll device presence each time the tab comes to the foreground.
    LifecycleResumeEffect(Unit) {
        viewModel.refreshDevices()
        onPauseOrDispose { }
    }

    val steerOn = state.steerEnabled == true

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            Text(
                "Agents",
                style = MaterialTheme.typography.headlineLarge,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
            )
            // No steer and nothing running → the full empty state (no devices
            // section to anchor a compact caption).
            if (!steerOn && state.rows.isEmpty()) {
                AgentsEmptyState()
            } else {
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    if (steerOn) {
                        item(key = "__desktops_header__") { SectionHeader("My desktops") }
                        val devs = devices
                        when {
                            // null = still loading; render nothing under the header.
                            devs == null -> Unit
                            devs.isEmpty() -> item(key = "__no_desktop__") {
                                HintRow("No desktop online — open the Exponential desktop app to run here.")
                            }
                            else -> items(devs, key = { "dev_${it.deviceId}" }) { device ->
                                DeviceRow(device = device, onStart = { sheetDevice = device })
                            }
                        }
                        val caption = startStateCaption(startState)
                        if (caption != null) {
                            item(key = "__start_state__") {
                                StartStateCaptionRow(caption = caption, showSpinner = startState is SteerStartState.Sending)
                            }
                        }
                        item(key = "__running_header__") { SectionHeader("Running") }
                    }

                    if (state.rows.isEmpty()) {
                        item(key = "__no_running__") {
                            Text(
                                "No agents running",
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                                modifier = Modifier.padding(vertical = 8.dp),
                            )
                        }
                    } else {
                        items(state.rows, key = { it.session.id }) { row ->
                            AgentSessionRow(
                                session = row.session,
                                issue = row.issue,
                                onClick = {
                                    if (state.steerEnabled == true) {
                                        onOpenSteer(row.session.id)
                                    } else {
                                        // Batch multi-issue sessions carry no issue.
                                        row.session.issueId?.let(onOpenIssue)
                                    }
                                },
                                onInfo = { row.session.issueId?.let(onOpenIssue) },
                            )
                        }
                    }
                }
            }
        }
    }

    val sheetDev = sheetDevice
    if (sheetDev != null) {
        StartCodingSheet(
            devices = devices ?: emptyList(),
            issues = startCandidates,
            preselectedIds = emptySet(),
            preferredDeviceId = sheetDev.deviceId,
            onStart = viewModel::startCoding,
            onDismiss = { sheetDevice = null },
        )
    }
}

@Composable
private fun SectionHeader(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        modifier = Modifier.padding(top = 8.dp, bottom = 2.dp),
    )
}

// One online desktop: a computer glyph + label, with a trailing "Start coding"
// pill that opens the launcher sheet pre-targeted at this device.
@Composable
private fun DeviceRow(device: SteerDevice, onStart: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .clickable(onClick = onStart)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            Icons.Filled.Computer,
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(12.dp))
        Text(
            device.deviceLabel.ifBlank { device.deviceId },
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(5.dp),
            modifier = Modifier.padding(start = 8.dp),
        ) {
            Icon(
                Icons.Filled.PlayArrow,
                contentDescription = null,
                modifier = Modifier.size(15.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
            Text(
                "Start coding",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.primary,
            )
        }
    }
}

@Composable
private fun HintRow(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        modifier = Modifier.padding(vertical = 4.dp),
    )
}

@Composable
private fun StartStateCaptionRow(caption: StartCaption, showSpinner: Boolean) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        modifier = Modifier.padding(vertical = 2.dp),
    ) {
        if (showSpinner) {
            CircularProgressIndicator(
                modifier = Modifier.size(12.dp),
                strokeWidth = 2.dp,
                color = MaterialTheme.colorScheme.onSurface,
            )
        }
        Text(
            caption.text,
            style = MaterialTheme.typography.labelSmall,
            color = if (caption.isError) {
                MaterialTheme.colorScheme.error
            } else {
                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
            },
        )
    }
}

private data class StartCaption(val text: String, val isError: Boolean)

private fun startStateCaption(state: SteerStartState): StartCaption? = when (state) {
    is SteerStartState.Idle -> null
    is SteerStartState.Sending -> StartCaption("Sending start command…", false)
    is SteerStartState.Sent ->
        if (state.isBatch) {
            StartCaption("Batch start sent to ${state.deviceLabel} — it'll appear below when the desktop picks up.", false)
        } else {
            StartCaption("Start sent to ${state.deviceLabel} — waiting for the desktop…", false)
        }
    is SteerStartState.Failed -> StartCaption(state.message, true)
}

@Composable
private fun AgentSessionRow(
    session: CodingSessionEntity,
    issue: IssueEntity?,
    onClick: () -> Unit,
    onInfo: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .clickable(onClick = onClick)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PulsingDot()
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    issue?.identifier ?: "…",
                    style = MaterialTheme.typography.labelMedium,
                    fontFamily = FontFamily.Monospace,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    // A batch run spans issues and carries no issue_id — name it
                    // as such rather than "not synced". Keep the not-yet-synced
                    // case for an issue-scoped session whose issue hasn't landed.
                    when {
                        issue != null -> issue.title
                        session.issueId == null -> "Batch run"
                        else -> "Issue not synced yet"
                    },
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            val device = session.deviceLabel?.takeIf { it.isNotBlank() } ?: "Desktop"
            Text(
                "$device · started ${relativeTime(session.startedAt)}",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        IconButton(onClick = onInfo) {
            Icon(
                Icons.Outlined.Info,
                contentDescription = "Open issue",
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

@Composable
private fun AgentsEmptyState() {
    Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(
                Icons.Filled.SmartToy,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.size(28.dp),
            )
            Text(
                "No agents running",
                style = MaterialTheme.typography.titleSmall,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                "Start coding on an issue from the desktop IDE — live sessions show up here.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                textAlign = TextAlign.Center,
            )
        }
    }
}
