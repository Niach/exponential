package com.exponential.app.ui.instance

import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.AppConstants
import com.exponential.app.R
import com.exponential.app.ui.auth.AuthShell
import com.exponential.app.ui.auth.PendingLoginMethod
import com.exponential.app.ui.components.GlassOAuthButton
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis

@Composable
fun InstanceScreen(
    onContinue: (String) -> Unit,
    showCancel: Boolean = false,
    onCancel: (() -> Unit)? = null,
    // The cloud preset is hidden when a cloud account already exists in the
    // AccountStore. Re-activating it from the add-server flow re-runs
    // upsertAndActivate which races SyncManager's DB swap — easier to remove
    // the path entirely. Users can still switch back to the existing cloud
    // account from Settings.
    cloudAlreadyAdded: Boolean = false,
    viewModel: InstanceViewModel = hiltViewModel(),
) {
    var input by remember { mutableStateOf(TextFieldValue("https://")) }
    // Self-hosting is demoted (EXP-14): the URL field is hidden until the user
    // taps the small "self-hosted instance" link. When cloud is unavailable
    // (already added) the field is the only option, so it's shown outright.
    var showSelfHost by remember(cloudAlreadyAdded) { mutableStateOf(cloudAlreadyAdded) }
    val canSubmit = input.text.length > 8
    val context = LocalContext.current
    val state by viewModel.state.collectAsStateWithLifecycle()

    val cloudConfig = state.cloudConfig
    // Optimistic until probed (EXP-405): our cloud enables both providers, so
    // a missing config (first render, offline) means "show the buttons" — the
    // old probe-gated rendering flashed a generic "Use Exponential Cloud"
    // chooser on every fresh launch. Only an explicit "disabled" from the
    // probe hides a button.
    val directGoogle = cloudConfig?.googleLoginEnabled != false
    val directApple = cloudConfig?.appleLoginEnabled != false
    val directEmail = cloudConfig == null || cloudConfig.emailOtpEnabled || cloudConfig.passwordEnabled
    val directPasskey = cloudConfig?.passkeyEnabled != false

    // Set the instance to the cloud, then hand off to a Custom Tab preselecting
    // the provider (mobile-oauth-start honors ?provider=). onContinue also
    // routes to the full login screen so a cancelled browser tab lands there.
    // The view model builds the URL so each tap mints a fresh PKCE attempt.
    fun startCloudOAuth(provider: String) {
        onContinue(AppConstants.PUBLIC_CLOUD_URL)
        val url = viewModel.cloudStartUrl(provider)
        CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(url))
    }

    fun continueOnCloud(method: String) {
        PendingLoginMethod.value = method
        onContinue(AppConstants.PUBLIC_CLOUD_URL)
    }

    // The cloud chooser IS the login page (P1): web's AuthFormShell — the
    // mark over "Continue to Exponential", no subtitle, no card, the same
    // Continue list (Apple, Google, email, passkey per the cloud's config),
    // "Use a self-hosted instance" under it, Privacy · Terms closing it.
    AuthShell(title = "Continue to Exponential") {
        Column(
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (!cloudAlreadyAdded) {
                if (AppConstants.IS_STAGING) {
                    // Small build marker so testers can tell builds apart
                    // (iOS keeps the same line).
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
                        modifier = Modifier.fillMaxWidth().padding(bottom = 4.dp),
                    ) {
                        Icon(
                            ExpIcons.uiStaging,
                            contentDescription = null,
                            modifier = Modifier.size(14.dp),
                            tint = DesignTokens.Semantic.Orange,
                        )
                        Text(
                            "Staging · " + (Uri.parse(AppConstants.PUBLIC_CLOUD_URL).host ?: AppConstants.PUBLIC_CLOUD_URL),
                            style = MaterialTheme.typography.labelSmall,
                            color = DesignTokens.Semantic.Orange,
                        )
                    }
                }
                // Cloud is the primary path: sign in directly with the
                // provider, no intermediate screen, rendered immediately
                // (EXP-405 — the welcome screen IS the login).
                if (directApple) {
                    GlassOAuthButton(
                        label = "Continue with Apple",
                        onClick = { startCloudOAuth("apple") },
                    ) {
                        Icon(
                            painter = painterResource(R.drawable.ic_apple),
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                            tint = LocalContentColor.current,
                        )
                    }
                }
                if (directGoogle) {
                    GlassOAuthButton(
                        label = "Continue with Google",
                        onClick = { startCloudOAuth("google") },
                    ) {
                        // Official multi-color "G" — tint stays Unspecified so
                        // the brand colors aren't overridden.
                        Icon(
                            painter = painterResource(R.drawable.ic_google),
                            contentDescription = null,
                            modifier = Modifier.size(17.dp),
                            tint = Color.Unspecified,
                        )
                    }
                }
                // Email and passkey run on the cloud's login screen: pick the
                // cloud, and that screen opens the chosen method at once.
                if (directEmail) {
                    GlassOAuthButton(
                        label = "Continue with email",
                        onClick = { continueOnCloud(PendingLoginMethod.EMAIL) },
                        modifier = Modifier.testTag("instance-continue-with-email"),
                    ) {
                        Icon(
                            imageVector = ExpIcons.uiMail,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                            tint = LocalContentColor.current,
                        )
                    }
                }
                if (directPasskey) {
                    GlassOAuthButton(
                        label = "Login with passkey",
                        onClick = { continueOnCloud(PendingLoginMethod.PASSKEY) },
                        modifier = Modifier.testTag("instance-passkey-button"),
                    ) {
                        Icon(
                            imageVector = ExpIcons.authPasskey,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                            tint = LocalContentColor.current,
                        )
                    }
                }
                if (!showSelfHost) {
                    Text(
                        "Use a self-hosted instance",
                        style = MaterialTheme.typography.labelSmall,
                        color = Color.White.copy(alpha = TextEmphasis.Tertiary),
                        textAlign = TextAlign.Center,
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { showSelfHost = true }
                            .padding(vertical = 8.dp),
                    )
                }
            }

            if (showSelfHost) {
                Column(
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                    modifier = Modifier.padding(top = if (cloudAlreadyAdded) 0.dp else 8.dp),
                ) {
                    Text(
                        "Server URL",
                        style = MaterialTheme.typography.labelSmall,
                        color = Color.White.copy(alpha = TextEmphasis.Tertiary),
                    )
                    GlassTextField(
                        value = input,
                        onValueChange = { input = it },
                        singleLine = true,
                        placeholder = "https://exp.example.com",
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go),
                        keyboardActions = KeyboardActions(onGo = { if (canSubmit) onContinue(input.text) }),
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("instance-url-field"),
                    )
                }
                GlassSubmitButton(
                    label = "Continue",
                    enabled = canSubmit,
                    onClick = { if (canSubmit) onContinue(input.text) },
                )
            }

            if (showCancel && onCancel != null) {
                Text(
                    "Cancel",
                    style = MaterialTheme.typography.bodyLarge,
                    color = Color.White.copy(alpha = TextEmphasis.Secondary),
                    textAlign = TextAlign.Center,
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable(onClick = onCancel)
                        .padding(vertical = 12.dp),
                )
            }
        }
    }
}
