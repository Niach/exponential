package com.exponential.app.ui.instance

import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.AppConstants
import com.exponential.app.R

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
    // The cloud's real provider set (never hardcoded). Direct OAuth buttons
    // only render once the cloud auth-config has confirmed a provider is on.
    val directGoogle = cloudConfig?.googleLoginEnabled == true
    val directApple = cloudConfig?.appleLoginEnabled == true
    val hasDirectOauth = directGoogle || directApple

    // Set the instance to the cloud, then hand off to a Custom Tab preselecting
    // the provider (mobile-oauth-start honors ?provider=). onContinue also
    // routes to the full login screen so a cancelled browser tab lands there.
    // The view model builds the URL so each tap mints a fresh PKCE attempt.
    fun startCloudOAuth(provider: String) {
        onContinue(AppConstants.PUBLIC_CLOUD_URL)
        val url = viewModel.cloudStartUrl(provider)
        CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(url))
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .windowInsetsPadding(WindowInsets.systemBars)
            .padding(24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.Start,
    ) {
        Text(
            if (AppConstants.IS_STAGING) "Connect to Exp Staging" else "Connect to Exponential",
            style = MaterialTheme.typography.titleLarge,
            color = MaterialTheme.colorScheme.onSurface,
        )
        Spacer(Modifier.height(24.dp))

        if (!cloudAlreadyAdded) {
            if (hasDirectOauth) {
                // Cloud is the primary path: sign in directly with the
                // provider, no intermediate screen.
                if (directApple) {
                    OutlinedButton(
                        onClick = { startCloudOAuth("apple") },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Icon(
                            painter = painterResource(R.drawable.ic_apple),
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                            tint = LocalContentColor.current,
                        )
                        Spacer(Modifier.width(10.dp))
                        Text("Continue with Apple")
                    }
                    Spacer(Modifier.height(8.dp))
                }
                if (directGoogle) {
                    OutlinedButton(
                        onClick = { startCloudOAuth("google") },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        // Official multi-color "G" — tint stays Unspecified so
                        // the brand colors aren't overridden.
                        Icon(
                            painter = painterResource(R.drawable.ic_google),
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                            tint = Color.Unspecified,
                        )
                        Spacer(Modifier.width(10.dp))
                        Text("Continue with Google")
                    }
                    Spacer(Modifier.height(8.dp))
                }
            } else {
                // Offline / cloud config not yet loaded (or password-only
                // cloud): fall back to the generic cloud button, which routes
                // to the full login screen and its own config fetch + retry.
                Button(
                    onClick = { onContinue(AppConstants.PUBLIC_CLOUD_URL) },
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(if (AppConstants.IS_STAGING) "Use Staging Cloud" else "Use Exponential Cloud")
                }
                Spacer(Modifier.height(8.dp))
            }

            if (!showSelfHost) {
                TextButton(
                    onClick = { showSelfHost = true },
                    modifier = Modifier.align(Alignment.CenterHorizontally),
                ) {
                    Text(
                        "Use a self-hosted instance",
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }

        if (showSelfHost) {
            Spacer(Modifier.height(8.dp))
            Text(
                "Self-hosted? Enter the full URL of your server.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.height(8.dp))

            OutlinedTextField(
                value = input,
                onValueChange = { input = it },
                singleLine = true,
                placeholder = { Text("https://exp.example.com") },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("instance-url-field"),
            )

            Spacer(Modifier.height(16.dp))

            Button(
                onClick = { if (canSubmit) onContinue(input.text) },
                enabled = canSubmit,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Continue")
            }
        }

        if (showCancel && onCancel != null) {
            Spacer(Modifier.height(8.dp))
            TextButton(onClick = onCancel, modifier = Modifier.fillMaxWidth()) {
                Text("Cancel")
            }
        }
    }
}
