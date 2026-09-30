package com.exponential.app.ui.settings

import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.R
import com.exponential.app.data.api.SignInMethodsDto
import com.exponential.app.data.api.SignInPasskeyDto
import com.exponential.app.data.api.SignInProviderDto
import com.exponential.app.data.api.canRemovePasskey
import com.exponential.app.data.api.canUnlink
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetDefaults
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/**
 * EXP-1126: Settings › server › Sign-in methods (web `SignInMethodsSection` +
 * `PasskeysSection` twin). ONE list of every way into the account — the code to
 * the primary email (changeable), the instance's providers with Link/Unlink,
 * a password row while one is set — then the passkeys band. Natives list and
 * remove passkeys; creating one is web-only. Every call names [accountId]
 * (the screen may show a non-active account).
 */
@Composable
fun SignInMethodsSection(accountId: String, viewModel: ServerDetailViewModel) {
    val context = LocalContext.current
    var unlinkTarget by remember { mutableStateOf<SignInProviderDto?>(null) }
    var passkeyTarget by remember { mutableStateOf<SignInPasskeyDto?>(null) }

    Column(Modifier.fillMaxWidth()) {
        SectionHeader("Sign-in methods")
        when (val state = viewModel.methods) {
            SignInMethodsState.Loading -> MethodsHint("Loading…")
            is SignInMethodsState.Error -> MethodsHint(state.message, isError = true)
            is SignInMethodsState.Ready -> {
                val methods = state.methods
                EmailCodeRow(methods, onChange = { viewModel.openChangeEmail() })
                methods.providers.forEach { provider ->
                    GroupDivider()
                    ProviderRow(
                        methods = methods,
                        provider = provider,
                        linking = viewModel.linkingProvider == provider.id,
                        linkEnabled = viewModel.linkingProvider == null,
                        onLink = {
                            viewModel.startLink(accountId, provider.id) { url ->
                                CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(url))
                            }
                        },
                        onUnlink = {
                            viewModel.removeError = null
                            unlinkTarget = provider
                        },
                    )
                }
            }
        }
        viewModel.methodsNotice?.let { notice ->
            MethodsHint(notice.message, isError = notice.isError)
        }
    }

    val ready = (viewModel.methods as? SignInMethodsState.Ready)?.methods
    if (ready != null) {
        Column(Modifier.fillMaxWidth()) {
            SectionHeader("Passkeys")
            if (ready.passkeys.isEmpty()) {
                MethodsHint("No passkeys yet. Add one from the web app.")
            } else {
                ready.passkeys.forEachIndexed { i, passkey ->
                    if (i > 0) GroupDivider()
                    PasskeyRow(
                        passkey = passkey,
                        enabled = canRemovePasskey(ready),
                        onRemove = {
                            viewModel.removeError = null
                            passkeyTarget = passkey
                        },
                    )
                }
            }
        }
    }

    unlinkTarget?.let { target ->
        val isPassword = target.kind == "password"
        ConfirmRemoveDialog(
            title = if (isPassword) "Remove your password?" else "Unlink ${target.name}?",
            body = if (isPassword) {
                "You will no longer be able to sign in with a password. Your other sign-in methods keep working."
            } else {
                "${target.name} will no longer sign you in. You can link it again any time; " +
                    "your other sign-in methods keep working."
            },
            confirmLabel = if (isPassword) "Remove" else "Unlink",
            busy = viewModel.removing,
            error = viewModel.removeError,
            onConfirm = { viewModel.unlink(accountId, target.id) { unlinkTarget = null } },
            onDismiss = { unlinkTarget = null },
        )
    }

    passkeyTarget?.let { target ->
        ConfirmRemoveDialog(
            title = "Remove this passkey?",
            body = "\"${target.name?.takeIf { it.isNotBlank() } ?: "Passkey"}\" will no longer sign you in. " +
                "The copy on your device stays until you delete it there.",
            confirmLabel = "Remove",
            busy = viewModel.removing,
            error = viewModel.removeError,
            onConfirm = { viewModel.deletePasskey(accountId, target.id) { passkeyTarget = null } },
            onDismiss = { passkeyTarget = null },
        )
    }

    viewModel.changeEmail?.let { state ->
        ChangeEmailSheet(
            state = state,
            onSendCode = { viewModel.sendEmailChangeCode(accountId, it) },
            onConfirm = { viewModel.confirmEmailChange(accountId, it) },
            onBack = { viewModel.backToEmailAddress() },
            onDismiss = { viewModel.dismissChangeEmail() },
        )
    }
}

@Composable
private fun EmailCodeRow(methods: SignInMethodsDto, onChange: () -> Unit) {
    MethodRow(
        glyph = {
            Icon(
                ExpIcons.uiMail,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        },
        title = "Email code",
        subtitle = if (methods.emailOtpEnabled) methods.email else "Sign-in codes are off on this instance",
        trailing = if (methods.emailOtpEnabled) {
            { RowButton("Change", onClick = onChange, testTag = "sign-in-email-change") }
        } else {
            null
        },
    )
}

@Composable
private fun ProviderRow(
    methods: SignInMethodsDto,
    provider: SignInProviderDto,
    linking: Boolean,
    linkEnabled: Boolean,
    onLink: () -> Unit,
    onUnlink: () -> Unit,
) {
    val blocked = provider.linked && !canUnlink(methods, provider)
    val subtitle = when {
        !provider.linked -> "Not linked"
        blocked -> "Linked · your only way to sign in"
        !provider.available -> "Linked · no longer offered on this instance"
        else -> formatMethodDate(provider.linkedAt)?.let { "Linked $it" } ?: "Linked"
    }
    MethodRow(
        glyph = { ProviderGlyph(provider.kind) },
        title = provider.name,
        subtitle = subtitle,
        trailing = {
            if (provider.linked) {
                RowButton(
                    label = if (provider.kind == "password") "Remove" else "Unlink",
                    onClick = onUnlink,
                    enabled = !blocked,
                    destructive = true,
                    testTag = "sign-in-unlink-${provider.id}",
                )
            } else {
                RowButton(
                    label = if (linking) "Redirecting…" else "Link",
                    onClick = onLink,
                    enabled = linkEnabled && provider.available,
                    testTag = "sign-in-link-${provider.id}",
                )
            }
        },
    )
}

@Composable
private fun PasskeyRow(passkey: SignInPasskeyDto, enabled: Boolean, onRemove: () -> Unit) {
    val added = formatMethodDate(passkey.createdAt)?.let { "added $it" }
    val subtitle = listOfNotNull(added, if (passkey.backedUp) "synced across your devices" else null)
        .joinToString(" · ")
    MethodRow(
        glyph = {
            Icon(
                ExpIcons.authPasskey,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        },
        title = passkey.name?.takeIf { it.isNotBlank() } ?: "Passkey",
        subtitle = subtitle.ifBlank { null },
        trailing = {
            RowButton("Remove", onClick = onRemove, enabled = enabled, destructive = true)
        },
    )
}

/** Brand marks exactly as the login screen draws them; OIDC and password carry none. */
@Composable
private fun ProviderGlyph(kind: String) {
    when (kind) {
        "apple" -> Icon(
            painter = painterResource(R.drawable.ic_apple),
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = LocalContentColor.current,
        )
        "google" -> Icon(
            painter = painterResource(R.drawable.ic_google),
            contentDescription = null,
            modifier = Modifier.size(17.dp),
            tint = Color.Unspecified,
        )
        else -> Unit
    }
}

@Composable
private fun MethodRow(
    glyph: @Composable () -> Unit,
    title: String,
    subtitle: String?,
    trailing: (@Composable () -> Unit)?,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .flatRow()
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        Box(Modifier.size(24.dp), contentAlignment = Alignment.Center) { glyph() }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(
                title,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (subtitle != null) {
                Text(
                    subtitle,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (trailing != null) {
            Spacer(Modifier.width(8.dp))
            trailing()
        }
    }
}

@Composable
private fun RowButton(
    label: String,
    onClick: () -> Unit,
    enabled: Boolean = true,
    destructive: Boolean = false,
    testTag: String? = null,
) {
    OutlinedButton(
        onClick = onClick,
        enabled = enabled,
        // The web's `size="sm"` outline button: a compact pill beside the row text.
        contentPadding = PaddingValues(horizontal = 14.dp, vertical = 0.dp),
        modifier = Modifier
            .height(32.dp)
            .then(if (testTag != null) Modifier.testTag(testTag) else Modifier),
    ) {
        Text(
            label,
            style = MaterialTheme.typography.labelMedium,
            color = when {
                !enabled -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary)
                destructive -> MaterialTheme.colorScheme.error
                else -> MaterialTheme.colorScheme.onSurface
            },
        )
    }
}

@Composable
private fun MethodsHint(text: String, isError: Boolean = false) {
    Text(
        text,
        style = MaterialTheme.typography.bodySmall,
        color = if (isError) {
            MaterialTheme.colorScheme.error
        } else {
            MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
        },
        modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
    )
}

@Composable
private fun ConfirmRemoveDialog(
    title: String,
    body: String,
    confirmLabel: String,
    busy: Boolean,
    error: String?,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(body)
                if (error != null) Text(error, color = MaterialTheme.colorScheme.error)
            }
        },
        confirmButton = {
            TextButton(onClick = onConfirm, enabled = !busy) {
                Text(if (busy) "Removing…" else confirmLabel, color = MaterialTheme.colorScheme.error)
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Cancel") }
        },
    )
}

/** The change-email sheet (web `ChangeEmailDialog` twin): address, then the mailed code. */
@Composable
private fun ChangeEmailSheet(
    state: ChangeEmailState,
    onSendCode: (String) -> Unit,
    onConfirm: (String) -> Unit,
    onBack: () -> Unit,
    onDismiss: () -> Unit,
) {
    var email by rememberSaveable { mutableStateOf(state.newEmail) }
    var code by rememberSaveable { mutableStateOf("") }
    val busy = state.sending || state.verifying
    val onCodeStep = state.step == ChangeEmailStep.Code
    GlassSheet(
        title = "Change email",
        onDismiss = onDismiss,
        primaryAction = if (onCodeStep) {
            SheetPrimaryAction(
                label = if (state.verifying) "Checking…" else "Confirm",
                onClick = { onConfirm(code) },
                enabled = code.trim().length >= 6,
                loading = state.verifying,
            )
        } else {
            SheetPrimaryAction(
                label = if (state.sending) "Sending…" else "Send code",
                onClick = { onSendCode(email) },
                enabled = email.isNotBlank(),
                loading = state.sending,
            )
        },
    ) {
        Column(
            modifier = Modifier.padding(horizontal = GlassSheetDefaults.HorizontalPadding),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                if (onCodeStep) {
                    "Enter the 6-digit code we sent to ${state.newEmail}. It expires in 10 minutes."
                } else {
                    "We'll send a code to the new address. Sign-in codes, notifications and @mentions use it " +
                        "once confirmed; your old address stops working for sign-in."
                },
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (onCodeStep) {
                GlassTextField(
                    value = code,
                    onValueChange = { value -> code = value.filter { it.isDigit() } },
                    singleLine = true,
                    placeholder = "Code",
                    enabled = !busy,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
                    modifier = Modifier.fillMaxWidth().testTag("change-email-code-field"),
                )
            } else {
                GlassTextField(
                    value = email,
                    onValueChange = { email = it },
                    singleLine = true,
                    placeholder = "you@example.com",
                    enabled = !busy,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email),
                    modifier = Modifier.fillMaxWidth().testTag("change-email-field"),
                )
            }
            if (state.error != null) {
                Text(
                    state.error,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
            if (onCodeStep) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.Center,
                ) {
                    TextButton(onClick = { onSendCode(state.newEmail) }, enabled = !busy) {
                        Text("Resend code")
                    }
                    TextButton(
                        onClick = {
                            code = ""
                            onBack()
                        },
                        enabled = !busy,
                    ) {
                        Text("Use a different email")
                    }
                }
            }
        }
    }
}

private val METHOD_DATE: DateTimeFormatter = DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM)

/** A wire timestamp as a short local date, or null when absent/unparseable. */
private fun formatMethodDate(value: String?): String? {
    val ms = value?.let { WireTimestamps.parseEpochMs(it) } ?: return null
    return Instant.ofEpochMilli(ms).atZone(ZoneId.systemDefault()).toLocalDate().format(METHOD_DATE)
}
