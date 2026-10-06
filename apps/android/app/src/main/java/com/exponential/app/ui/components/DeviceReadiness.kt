package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.domain.DeviceReadiness
import com.exponential.app.domain.DoctorGlyph
import com.exponential.app.domain.DoctorGroup
import com.exponential.app.domain.DoctorRow
import com.exponential.app.domain.DoctorTone
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.session.AgentLoginSheet
import com.exponential.app.ui.session.AgentLoginTarget
import com.exponential.app.ui.session.DeviceCommandUiState
import com.exponential.app.ui.session.DeviceSettingsViewModel
import com.exponential.app.ui.session.agentUpdateCommandKey
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis

// EXP-1196/1218/1219: THE device readiness block, rendered off the row model
// (`DeviceReadiness.groups`, fixture-locked): a filled group band per group
// (the `optional` tag as plain muted text on it), one flat row per item —
// state glyph, label, the device-written detail, at most ONE trailing pill —
// and the `computer_use` item as a bare SwitchRow. No subtitles, no captions.

/**
 * The whole block. [onAction] runs a row's offered action; [onComputerUseChange]
 * = the switch's write (null disables it). [busyKeys] = rows whose action is in
 * flight (their pill spins).
 */
@Composable
fun DeviceReadinessBlock(
    groups: List<DoctorGroup>,
    onAction: (DoctorRow) -> Unit,
    onComputerUseChange: ((Boolean) -> Unit)?,
    modifier: Modifier = Modifier,
    busyKeys: Set<String> = emptySet(),
) {
    Column(modifier = modifier.fillMaxWidth().testTag("device-readiness")) {
        groups.forEachIndexed { index, group ->
            if (index > 0) Spacer(Modifier.height(8.dp))
            SectionHeader(
                group.label,
                modifier = Modifier.padding(horizontal = 16.dp),
                trailing = group.tag?.let { tag ->
                    {
                        Text(
                            tag,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                },
            )
            OptionGroup {
                group.rows.forEachIndexed { rowIndex, row ->
                    if (rowIndex > 0) GroupDivider()
                    if (row.isSwitch) {
                        SwitchRow(
                            title = row.label,
                            checked = row.switchOn,
                            onCheckedChange = { onComputerUseChange?.invoke(it) },
                            enabled = onComputerUseChange != null,
                        )
                    } else {
                        DeviceReadinessRow(
                            row = row,
                            onAction = onAction,
                            busy = row.key in busyKeys,
                        )
                    }
                }
            }
        }
    }
}

/**
 * ONE row of the block — also the composer's single-row mode and the
 * own-device list's caption ([showAction] = false there: the row opens the
 * device sheet, where the action lives).
 */
@Composable
fun DeviceReadinessRow(
    row: DoctorRow,
    onAction: (DoctorRow) -> Unit,
    modifier: Modifier = Modifier,
    busy: Boolean = false,
    showAction: Boolean = true,
    horizontalPadding: Dp = 16.dp,
    verticalPadding: Dp = 12.dp,
    /** The own-device list's caption size (null = the block's bodyLarge). */
    labelStyle: TextStyle? = null,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .testTag("device-readiness-row-${row.key}")
            .padding(
                start = horizontalPadding + if (row.indented) 20.dp else 0.dp,
                end = if (showAction && row.action != null) 12.dp else horizontalPadding,
                top = verticalPadding,
                bottom = verticalPadding,
            ),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        glyphIcon(row.glyph)?.let { glyph ->
            Icon(
                glyph,
                contentDescription = row.state,
                tint = toneColor(row.tone),
                modifier = Modifier.size(16.dp),
            )
        }
        Text(
            row.label,
            style = labelStyle ?: MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
        )
        Text(
            row.detail.orEmpty(),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        val label = row.actionLabel
        if (showAction && row.action != null && label != null) {
            GlassPill(
                label,
                size = PillSize.Sm,
                primary = row.primary,
                enabled = !busy,
                loading = busy,
                onClick = { onAction(row) },
                modifier = Modifier.testTag("device-readiness-action-${row.key}"),
            )
        }
    }
}

/** The fixture glyphs by CONCEPT: check / alert / dash / x. */
private fun glyphIcon(glyph: DoctorGlyph): ImageVector? = when (glyph) {
    DoctorGlyph.Check -> ExpIcons.uiCheck
    DoctorGlyph.Alert -> ExpIcons.uiWarning
    DoctorGlyph.Dash -> ExpIcons.uiMinus
    DoctorGlyph.X -> ExpIcons.uiClose
    DoctorGlyph.None -> null
}

@Composable
private fun toneColor(tone: DoctorTone): Color = when (tone) {
    DoctorTone.Success -> DesignTokens.Semantic.Green
    DoctorTone.Warning -> DesignTokens.Semantic.Yellow
    DoctorTone.Destructive -> DesignTokens.Semantic.Red
    DoctorTone.Muted -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
}

/**
 * The composer's single-row mode (EXP-1196 rule: for the picked device + agent
 * when that agent cannot run there, ONLY that agent's row — the Git row when
 * Git is the failure — with its action). A phone is always another device, so
 * only `update` (`agent_update`) and `sign_in` (the remote sign-in sheet)
 * carry a pill. Falls back to [fallback] text when the device reports no
 * doctor (an older build) or the doctor sees nothing wrong.
 */
@Composable
fun DeviceNotReadyRow(
    device: SteerDevice,
    agent: String,
    fallback: String,
    modifier: Modifier = Modifier,
    fallbackStyle: TextStyle = MaterialTheme.typography.labelSmall,
) {
    val row = device.doctor?.let { DeviceReadiness.failingRow(it, agent, remote = true) }
    if (row == null) {
        Text(
            fallback,
            style = fallbackStyle,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            modifier = modifier.testTag("launch-not-ready-note"),
        )
        return
    }
    val commands: DeviceSettingsViewModel = hiltViewModel()
    val states by commands.commandStates.collectAsStateWithLifecycle()
    var loginTarget by remember { mutableStateOf<AgentLoginTarget?>(null) }
    val updateState = states[agentUpdateCommandKey(device.deviceId, row.key)]
    Column(modifier = modifier.testTag("launch-not-ready-row")) {
        DeviceReadinessRow(
            row = row,
            busy = updateState is DeviceCommandUiState.Sending ||
                updateState is DeviceCommandUiState.Running,
            horizontalPadding = 0.dp,
            verticalPadding = 4.dp,
            onAction = {
                when (it.action) {
                    DeviceReadiness.ACTION_UPDATE ->
                        commands.agentUpdate(device.deviceId, it.key, device.online)
                    DeviceReadiness.ACTION_SIGN_IN ->
                        loginTarget = AgentLoginTarget(device = device, agent = it.key)
                }
            },
        )
        (updateState as? DeviceCommandUiState.Failed)?.let {
            Text(
                it.message,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.error,
            )
        }
    }
    loginTarget?.let { target ->
        AgentLoginSheet(
            target = target,
            onDismiss = { loginTarget = null },
            liveDevice = device,
        )
    }
}
