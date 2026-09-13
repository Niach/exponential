package com.exponential.app.ui.session

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
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import com.exponential.app.domain.AgentAccountsRows
import androidx.compose.runtime.remember
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.domain.parseAgentLoginResult
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.agentLabel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-862: WHERE a sign-in lands — the machine, the agent, and either an
 * EXISTING login of that agent ([profileId]) or a NEW profile the machine
 * creates first ([newProfileLabel], the "Add account" path). Exactly one of the
 * two is ever set; the ambient login is `system`.
 */
internal data class AgentLoginTarget(
    val device: SteerDevice,
    val agent: String,
    val profileId: String? = null,
    val newProfileLabel: String? = null,
)

/**
 * EXP-862: THE sign-in of this client. A login never travels — the machine runs
 * the agent CLI's OWN flow and publishes its link (plus codex's device code)
 * back as the command result, which is what this sheet renders; claude's flow
 * hands a code back the other way, so the link gets a field and an "Enter code"
 * pill.
 *
 * One implementation for every entry point (a chip's "Sign in", an account's
 * `+`, the Add-account pill) — the device-settings sheet carries no accounts at
 * all since this wave, so there is nowhere else a login can appear.
 */
@Composable
internal fun AgentLoginSheet(
    target: AgentLoginTarget,
    onDismiss: () -> Unit,
    /** The LIVE row of the device the login runs on (the synced heartbeat),
     * so the sheet can see its own success; null keeps it open until
     * dismissed by hand. */
    liveDevice: SteerDevice? = null,
    viewModel: DeviceSettingsViewModel = hiltViewModel(),
) {
    val device = target.device
    val commandStates by viewModel.commandStates.collectAsStateWithLifecycle()

    // EXP-862: the sheet closes itself once the login it asked for is usable
    // on the device (web/desktop/iOS do the same). The FIRST observation is
    // the baseline: a login that was already there never counts as landing.
    val landed = AgentAccountsRows.loginLanded(
        liveDevice?.agentAccounts?.get(target.agent),
        target.profileId,
        target.newProfileLabel,
    )
    var baseline by remember(target) { mutableStateOf<Boolean?>(null) }
    LaunchedEffect(landed) {
        when (baseline) {
            null -> baseline = landed
            false -> if (landed) onDismiss()
            else -> Unit
        }
    }

    // The sheet IS the request: it queues the login once, on the machine the
    // caller named, and then follows it. A second open re-runs it, which is
    // what a stale or abandoned link needs anyway.
    LaunchedEffect(target) {
        device.rowId?.let { viewModel.bind(it) }
        viewModel.agentLogin(
            deviceId = device.deviceId,
            agent = target.agent,
            switchAccount = false,
            deviceOnline = device.online,
            profileId = target.profileId,
            newProfileLabel = target.newProfileLabel,
        )
    }

    val state = commandStates[agentLoginCommandKey(target.agent)]
    val codeState = commandStates[agentLoginCodeCommandKey(target.agent)]
    GlassSheet(
        title = "Sign in",
        onDismiss = onDismiss,
        modifier = Modifier.testTag("agent-login-sheet"),
    ) {
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp)) {
            Text(
                "${agentLabel(target.agent)} signs in on " +
                    "${device.deviceLabel.ifBlank { device.deviceId }}.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
            LoginResultCaption(
                agent = target.agent,
                state = state,
                codeState = codeState,
                // EXP-765: claude's browser hands a code back; the machine has
                // to be listening for it, which is the same gate as the login.
                canEnterCode = device.online && device.isMine,
                onEnterCode = { code ->
                    viewModel.agentLoginCode(device.deviceId, target.agent, code, device.online)
                },
            )
            CommandCaption(codeState)
            Spacer(Modifier.height(16.dp))
        }
    }
}

/**
 * What a queued `agent_login` command is doing (EXP-484). A completed one
 * publishes JSON — the machine's login URL, plus codex's device code — which
 * renders as an openable link and a copyable code; anything else (queued,
 * failed, a result that isn't a login publication) falls through to the
 * ordinary command caption.
 *
 * EXP-765: a link WITHOUT a code is claude's (`code=true` — the browser shows
 * the code and the CLI on the machine is still waiting for it), so when the
 * machine can take it back the link gets a field and an "Enter code" pill; the
 * `agent_login_code` command's own progress captions right below.
 */
@Composable
internal fun LoginResultCaption(
    agent: String,
    state: DeviceCommandUiState?,
    codeState: DeviceCommandUiState?,
    canEnterCode: Boolean,
    onEnterCode: (String) -> Unit,
) {
    val login = (state as? DeviceCommandUiState.Done)?.let { parseAgentLoginResult(it.message) }
    when {
        state is DeviceCommandUiState.Sending || state is DeviceCommandUiState.Running ->
            Text(
                "Waiting for the sign-in link…",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier.padding(top = 4.dp),
            )
        login != null -> {
            val uriHandler = LocalUriHandler.current
            val clipboard = LocalClipboardManager.current
            Spacer(Modifier.height(4.dp))
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth().clickable { uriHandler.openUri(login.url) },
            ) {
                Icon(
                    ExpIcons.uiExternalLink,
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
                Spacer(Modifier.width(6.dp))
                Text(
                    login.url,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.primary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            login.code?.let { code ->
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        code,
                        style = MaterialTheme.typography.bodyMedium,
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                    IconButton(onClick = { clipboard.setText(AnnotatedString(code)) }) {
                        Icon(
                            ExpIcons.uiCopy,
                            contentDescription = "Copy code",
                            modifier = Modifier.size(14.dp),
                            tint = MaterialTheme.colorScheme.onSurface.copy(
                                alpha = TextEmphasis.Tertiary,
                            ),
                        )
                    }
                }
            }
            // Only the codeless (claude) flow returns a code to us; codex's is
            // typed into the browser, so nothing comes back and nothing is
            // offered here.
            val returnsCode = login.code == null && canEnterCode
            Text(
                when {
                    login.code != null ->
                        "Open the link on any device and enter the code on the machine."
                    returnsCode ->
                        "Open the link on any device, then paste the code it shows here."
                    else -> "Open the link on any device."
                },
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
            if (returnsCode) {
                // Per agent: switching sign-ins must not carry a half-typed
                // code over to another agent's.
                key(agent) {
                    var draft by rememberSaveable(agent) { mutableStateOf("") }
                    val sending = codeState is DeviceCommandUiState.Sending ||
                        codeState is DeviceCommandUiState.Running
                    Spacer(Modifier.height(6.dp))
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        GlassTextField(
                            value = draft,
                            onValueChange = { draft = it },
                            placeholder = "Code from the browser",
                            singleLine = true,
                            keyboardOptions = KeyboardOptions(
                                autoCorrectEnabled = false,
                                capitalization = KeyboardCapitalization.None,
                            ),
                            textStyle = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.weight(1f),
                        )
                        GlassPill(
                            "Enter code",
                            onClick = {
                                val code = draft.trim()
                                if (code.isNotEmpty() && !sending) {
                                    onEnterCode(code)
                                    draft = ""
                                }
                            },
                            icon = ExpIcons.uiSignIn,
                            enabled = draft.isNotBlank() && !sending,
                            loading = sending,
                        )
                    }
                }
            }
        }
        else -> CommandCaption(state)
    }
}
