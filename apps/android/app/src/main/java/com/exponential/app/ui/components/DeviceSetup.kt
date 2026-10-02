package com.exponential.app.ui.components

import android.content.Intent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.AppConstants
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.domain.LaunchDeviceRules
import com.exponential.app.ui.gettingstarted.GettingStartedCopy
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.onboarding.OnboardingCopy
import com.exponential.app.ui.session.DeviceSettingsSheet
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard
import com.exponential.app.ui.theme.glassRow

/**
 * THE device setup block (EXP-725, EXP-1169): one block, four hosts. The
 * wizard's devices step (create path), the join step every invite-accept
 * surface shows a caller who owns no machine (both via
 * [com.exponential.app.ui.onboarding.OnboardingDevicesStep]), the Devices
 * tab's [AddDeviceSheet], and the coding readiness sheet's "Set up a server"
 * fix (the same sheet). Web and iOS draw the same block.
 *
 * Two install cards (the desktop IDE, the headless CLI daemon) reusing the
 * getting-started checklist's words verbatim, then the caller's own machines
 * as they arrive over the synced devices shape. A row taps into the same
 * [DeviceSettingsSheet] the Devices tab opens, so signing an agent in never
 * needs a detour. No header and no trailing button: those belong to the host.
 */
@Composable
fun DeviceSetup(
    instanceOrigin: String?,
    modifier: Modifier = Modifier,
    viewModel: DeviceSetupViewModel = hiltViewModel(),
) {
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    var settingsTarget by remember { mutableStateOf<String?>(null) }
    var copied by remember { mutableStateOf(false) }
    LaunchedEffect(copied) {
        if (copied) {
            kotlinx.coroutines.delay(2_000)
            copied = false
        }
    }

    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        InstallCard(
            title = GettingStartedCopy.DESKTOP_TITLE,
            description = GettingStartedCopy.DESKTOP_DESCRIPTION,
            actionLabel = GettingStartedCopy.DESKTOP_ACTION,
            actionIcon = ExpIcons.uiDownload,
            onAction = {
                runCatching {
                    context.startActivity(
                        Intent(Intent.ACTION_VIEW, AppConstants.DESKTOP_RELEASES_URL.toUri()),
                    )
                }
            },
        )
        // No resolved origin means no instance to point the daemon at:
        // copying `EXP_INSTANCE= sh` would hand over a broken command
        // (the Issues-tab checklist gates the same way).
        InstallCard(
            title = GettingStartedCopy.SERVER_TITLE,
            description = GettingStartedCopy.SERVER_DESCRIPTION,
            actionLabel = if (copied) OnboardingCopy.INVITE_COPIED else GettingStartedCopy.SERVER_ACTION,
            actionIcon = if (copied) ExpIcons.uiCheck else ExpIcons.uiCopy,
            onAction = instanceOrigin?.let { origin ->
                {
                    clipboard.setText(AnnotatedString(AppConstants.serverInstallSnippet(origin)))
                    copied = true
                }
            },
        )

        Text(
            OnboardingCopy.DEVICES_YOURS,
            style = MaterialTheme.typography.titleSmall.copy(fontWeight = FontWeight.SemiBold),
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.fillMaxWidth(),
        )
        val rows = devices.orEmpty()
        if (rows.isEmpty()) {
            Text(
                OnboardingCopy.DEVICES_NONE,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier
                    .fillMaxWidth()
                    .glassRow()
                    .padding(horizontal = 12.dp, vertical = 10.dp),
            )
        } else {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                rows.forEach { device ->
                    OwnDeviceRow(
                        device = device,
                        onClick = { settingsTarget = device.deviceId },
                    )
                }
            }
        }
    }

    // Same live re-resolution as the Devices tab: the row can vanish mid-edit
    // (removed elsewhere), and the sheet simply stops rendering.
    settingsTarget?.let { targetId ->
        devices?.firstOrNull { it.deviceId == targetId && it.isMine }?.let { target ->
            DeviceSettingsSheet(
                device = target,
                onDismiss = { settingsTarget = null },
            )
        }
    }
}

/** One install card: title, one line of why, one pill. */
@Composable
private fun InstallCard(
    title: String,
    description: String,
    actionLabel: String,
    actionIcon: androidx.compose.ui.graphics.vector.ImageVector,
    onAction: (() -> Unit)?,
) {
    Column(
        modifier = Modifier.fillMaxWidth().glassCard().padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            title,
            style = MaterialTheme.typography.titleSmall.copy(fontWeight = FontWeight.SemiBold),
            color = MaterialTheme.colorScheme.onSurface,
        )
        Text(
            description,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        GlassPill(label = actionLabel, icon = actionIcon, onClick = onAction, enabled = onAction != null)
    }
}

/**
 * One of the caller's machines: kind glyph, label, and the Devices-tab presence
 * caption (green dot Online / an amber reason / "Last seen …" / Offline).
 * Tapping opens the device settings sheet.
 *
 * EXP-836: the reason comes from [LaunchDeviceRules.blockedCaption], the same
 * rule the machines list gates its play button on. A machine that reported no
 * runnable agent at all used to read a bare "Online" here while nothing could
 * start on it, which is exactly what this step exists to get fixed.
 */
@Composable
private fun OwnDeviceRow(device: SteerDevice, onClick: () -> Unit) {
    val online = device.online
    val blockedCaption = LaunchDeviceRules.blockedCaption(device)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            deviceIcon(device),
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(
                alpha = if (online && blockedCaption == null) {
                    TextEmphasis.Secondary
                } else {
                    TextEmphasis.Tertiary
                },
            ),
        )
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                device.deviceLabel.ifBlank { device.deviceId },
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                if (online) {
                    StaticDot(
                        if (blockedCaption != null) NeedsInputAmber else ReviewGreen,
                        size = 6.dp,
                    )
                }
                Text(
                    when {
                        blockedCaption != null -> blockedCaption
                        online -> "Online"
                        device.lastSeenAt != null -> "Last seen ${relativeTime(device.lastSeenAt)}"
                        else -> "Offline"
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = if (blockedCaption != null) {
                        NeedsInputAmber
                    } else {
                        MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
                    },
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}
