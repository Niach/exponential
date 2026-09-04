package com.exponential.app.ui.onboarding

import android.content.Intent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.AppConstants
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.gettingstarted.GettingStartedCopy
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.session.DeviceSettingsSheet
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard
import com.exponential.app.ui.theme.glassRow

/**
 * Wizard step 4 — "Set up your devices" (EXP-725), the LAST step because
 * finishing it means walking over to another machine.
 *
 * Two install cards (the desktop IDE, the headless CLI daemon) reusing the
 * getting-started checklist's words verbatim — one copy for the two surfaces
 * that say the same thing — then the caller's own machines as they arrive over
 * the synced devices shape. A row taps into the same [DeviceSettingsSheet] the
 * Agents tab opens, so signing an agent in never needs a detour.
 *
 * Skippable: the trailing button reads [OnboardingCopy.SKIP] until a machine
 * of the user's own has registered, then [OnboardingCopy.CONTINUE].
 */
@Composable
fun OnboardingDevicesStep(
    instanceOrigin: String?,
    onContinue: () -> Unit,
    viewModel: OnboardingDevicesViewModel = hiltViewModel(),
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
        modifier = Modifier
            .widthIn(max = 460.dp)
            .fillMaxWidth()
            .testTag("onboarding-devices-step"),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            OnboardingCopy.DEVICES_TITLE,
            style = MaterialTheme.typography.headlineSmall,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(12.dp))
        Text(
            OnboardingCopy.DEVICES_SUBTITLE,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(28.dp))

        Column(
            modifier = Modifier.fillMaxWidth(),
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
            // No resolved origin means no instance to point the daemon at —
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

        Spacer(Modifier.height(28.dp))
        // A machine of one's own is what this step is for; until one shows up
        // the only honest label is "skip".
        GlassSubmitButton(
            label = if (devices.orEmpty().isEmpty()) OnboardingCopy.SKIP else OnboardingCopy.CONTINUE,
            onClick = onContinue,
        )
    }

    // Same live re-resolution as the Agents tab: the row can vanish mid-edit
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
 * One of the caller's machines: kind glyph, label, and the Agents-tab presence
 * caption (green dot Online / amber "<agents> not signed in" / "Last seen …" /
 * Offline). Tapping opens the device settings sheet.
 */
@Composable
private fun OwnDeviceRow(device: SteerDevice, onClick: () -> Unit) {
    val online = device.online
    val unauthed = device.unauthedAgentIds
    val signInNeeded = online && !device.hasRunnableAgent && unauthed.isNotEmpty()
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            if (device.isServer) ExpIcons.uiServer else ExpIcons.uiDevice,
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(
                alpha = if (online && !signInNeeded) TextEmphasis.Secondary else TextEmphasis.Tertiary,
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
                if (online) StaticDot(if (signInNeeded) NeedsInputAmber else ReviewGreen, size = 6.dp)
                Text(
                    when {
                        signInNeeded -> "${unauthed.joinToString(", ")} not signed in"
                        online -> "Online"
                        device.lastSeenAt != null -> "Last seen ${relativeTime(device.lastSeenAt)}"
                        else -> "Offline"
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = if (signInNeeded) {
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
