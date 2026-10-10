package com.exponential.app.ui.components

import android.content.Intent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.DeviceCodeRules
import kotlinx.coroutines.delay
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
import com.exponential.app.domain.DeviceReadiness
import com.exponential.app.domain.LaunchDeviceRules
import com.exponential.app.ui.gettingstarted.GettingStartedCopy
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.issue.compactRelativeTime
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
 * getting-started checklist's words verbatim, then, for the onboarding hosts
 * only ([listDevices]), the caller's own machines as they arrive over the
 * synced devices shape. A row taps into the same [DeviceSettingsSheet] the
 * Devices tab opens, so signing an agent in never needs a detour. The Add
 * device sheet opens over the tab that already lists them, so it shows the
 * cards alone. No header and no trailing button: those belong to the host.
 */
@Composable
fun DeviceSetup(
    instanceOrigin: String?,
    modifier: Modifier = Modifier,
    listDevices: Boolean = true,
    viewModel: DeviceSetupViewModel = hiltViewModel(),
) {
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val server by viewModel.server.collectAsStateWithLifecycle()
    var settingsTarget by remember { mutableStateOf<String?>(null) }
    // The install token lives exactly as long as the block is on screen.
    DisposableEffect(viewModel) {
        viewModel.activate()
        onDispose { viewModel.deactivate() }
    }

    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        InstallCard(
            icon = ExpIcons.uiDevice,
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
            // P7: web's outline "All platforms" beside the download ×4.
            secondaryLabel = "All platforms",
            onSecondary = {
                runCatching {
                    context.startActivity(
                        Intent(Intent.ACTION_VIEW, AppConstants.DESKTOP_RELEASES_URL.toUri()),
                    )
                }
            },
        )
        ServerCard(
            instanceOrigin = instanceOrigin,
            state = server,
            onCopied = viewModel::onCopied,
            onCodeChange = viewModel::onCodeChange,
            onApprove = viewModel::approve,
        )

        if (!listDevices) return@Column
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
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    title: String,
    description: String,
    actionLabel: String,
    actionIcon: androidx.compose.ui.graphics.vector.ImageVector,
    onAction: (() -> Unit)?,
    secondaryLabel: String? = null,
    onSecondary: (() -> Unit)? = null,
) {
    Column(
        modifier = Modifier.fillMaxWidth().glassCard().padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        CardHeader(icon = icon, title = title)
        Text(
            description,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            GlassPill(label = actionLabel, icon = actionIcon, onClick = onAction, enabled = onAction != null)
            if (secondaryLabel != null && onSecondary != null) {
                GlassPill(label = secondaryLabel, onClick = onSecondary)
            }
        }
    }
}

/** A card's header on every client: the concept icon, then the title. */
@Composable
private fun CardHeader(icon: androidx.compose.ui.graphics.vector.ImageVector, title: String) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.size(16.dp),
        )
        Text(
            title,
            style = MaterialTheme.typography.titleSmall.copy(fontWeight = FontWeight.SemiBold),
            color = MaterialTheme.colorScheme.onSurface,
        )
    }
}

/**
 * The web box wraps with `break-all`: a long token breaks where the line
 * ends, not at the nearest word boundary (which strands the trailing
 * backslash on a line of its own). Compose has no such mode, so the DISPLAYED
 * text gets a zero-width break opportunity after every character. The
 * clipboard never sees this string.
 */
private fun breakAnywhere(text: String): String =
    text.lineSequence().joinToString("\n") { line -> line.toList().joinToString("\u200B") }

/**
 * The server card (EXP-1169, the same on all four clients): title, one line of
 * why, the install command in a box with an icon-only copy control, and, once
 * the command was copied, the field that approves the code the CLI prints.
 * The box shows the command on fixed lines; the clipboard gets ONE line,
 * carrying the minted `EXP_INSTALL_TOKEN` when there is one. No resolved
 * origin means no instance to point the daemon at, so no box at all (a
 * `EXP_INSTANCE= sh` command would be broken).
 */
@Composable
private fun ServerCard(
    instanceOrigin: String?,
    state: ServerCardState,
    onCopied: () -> Unit,
    onCodeChange: (String) -> Unit,
    onApprove: () -> Unit,
) {
    val clipboard = LocalClipboardManager.current
    var justCopied by remember { mutableStateOf(false) }
    LaunchedEffect(justCopied) {
        if (justCopied) {
            delay(1_500)
            justCopied = false
        }
    }
    Column(
        modifier = Modifier.fillMaxWidth().glassCard().padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        CardHeader(icon = ExpIcons.uiServer, title = GettingStartedCopy.SERVER_TITLE)
        Text(
            GettingStartedCopy.SERVER_DESCRIPTION,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        if (instanceOrigin == null) return@Column
        Box(modifier = Modifier.fillMaxWidth().glassRow().testTag("install-snippet")) {
            Text(
                breakAnywhere(AppConstants.serverInstallSnippetDisplayed(instanceOrigin, state.token)),
                style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(start = 12.dp, top = 12.dp, bottom = 12.dp, end = 44.dp),
            )
            CircleIconButton(
                icon = if (justCopied) ExpIcons.uiCheck else ExpIcons.uiCopy,
                contentDescription = DeviceSetupCopy.COPY_COMMAND,
                onClick = {
                    clipboard.setText(
                        AnnotatedString(AppConstants.serverInstallSnippet(instanceOrigin, state.token)),
                    )
                    justCopied = true
                    onCopied()
                },
                borderless = true,
                glyphSize = 16.dp,
                modifier = Modifier.align(Alignment.TopEnd).padding(4.dp),
            )
        }
        if (state.approved) {
            Text(
                DeviceSetupCopy.APPROVED,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
        } else if (state.copied) {
            Text(
                DeviceSetupCopy.CODE_LABEL,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            val canApprove = !state.busy && DeviceCodeRules.isCompleteUserCode(state.code)
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                GlassTextField(
                    value = state.code,
                    onValueChange = onCodeChange,
                    modifier = Modifier.weight(1f).testTag("device-code-field"),
                    placeholder = DeviceSetupCopy.CODE_PLACEHOLDER,
                    singleLine = true,
                    textStyle = MaterialTheme.typography.bodyLarge.copy(
                        fontFamily = FontFamily.Monospace,
                        letterSpacing = 2.sp,
                    ),
                    keyboardOptions = KeyboardOptions(
                        capitalization = KeyboardCapitalization.Characters,
                        autoCorrectEnabled = false,
                        keyboardType = KeyboardType.Ascii,
                        imeAction = ImeAction.Done,
                    ),
                    keyboardActions = KeyboardActions(onDone = { if (canApprove) onApprove() }),
                )
                GlassSubmitButton(
                    label = DeviceSetupCopy.APPROVE,
                    onClick = onApprove,
                    enabled = canApprove,
                    modifier = Modifier.width(112.dp),
                )
            }
            state.error?.let { error ->
                Text(
                    error,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }
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
    // EXP-1196: a machine that reports a doctor speaks for itself — its rows
    // that need attention (glyph, label, detail; the action lives in the
    // settings sheet this row opens) replace the "{agents} not signed in"
    // caption. No doctor (an older build), or nothing flagged: the caption.
    val doctorRows = device.doctor
        ?.takeIf { online }
        ?.let { DeviceReadiness.attentionRows(it) }
        .orEmpty()
    val blockedCaption = if (doctorRows.isEmpty()) LaunchDeviceRules.blockedCaption(device) else null
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
                alpha = if (online && blockedCaption == null && doctorRows.isEmpty()) {
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
            doctorRows.forEach { row ->
                DeviceReadinessRow(
                    row = row,
                    onAction = {},
                    showAction = false,
                    horizontalPadding = 0.dp,
                    verticalPadding = 2.dp,
                    labelStyle = MaterialTheme.typography.bodySmall,
                )
            }
            if (doctorRows.isEmpty()) Row(
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
                        device.lastSeenAt != null -> "Last seen ${compactRelativeTime(device.lastSeenAt)}"
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
