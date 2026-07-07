package com.exponential.app.ui.auth

import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.lifecycle.compose.collectAsStateWithLifecycle
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
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.exponential.app.R

@Composable
fun LoginScreen(
    instanceUrl: String,
    onLoggedIn: () -> Unit,
    onChangeInstance: () -> Unit,
    viewModel: LoginViewModel = hiltViewModel(),
) {
    var email by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    val state by viewModel.state.collectAsStateWithLifecycle()
    val context = LocalContext.current

    if (state.successEmail != null) {
        onLoggedIn()
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
            "Sign in",
            style = MaterialTheme.typography.titleLarge,
            color = MaterialTheme.colorScheme.onSurface,
        )
        Spacer(Modifier.height(4.dp))
        Text(
            instanceUrl,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        Spacer(Modifier.height(24.dp))

        when {
            state.configLoading -> {
                Box(
                    modifier = Modifier.fillMaxWidth(),
                    contentAlignment = Alignment.Center,
                ) {
                    CircularProgressIndicator()
                }
            }

            state.config == null -> {
                Text(
                    state.configError ?: "Failed to load auth config",
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodyMedium,
                )
                Spacer(Modifier.height(12.dp))
                TextButton(onClick = { viewModel.loadConfig() }) {
                    Text("Retry")
                }
            }

            else -> {
                val config = state.config!!
                val hasOauth = config.oidcProviders.isNotEmpty() ||
                    config.googleLoginEnabled || config.appleLoginEnabled

                if (config.appleLoginEnabled) {
                    OutlinedButton(
                        onClick = {
                            viewModel.appleStartUrl()?.let { url ->
                                CustomTabsIntent.Builder().build()
                                    .launchUrl(context, Uri.parse(url))
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        // Monochrome Apple mark, tinted with the current content
                        // color (the Compose "currentColor").
                        Icon(
                            painter = painterResource(R.drawable.ic_apple),
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                            tint = LocalContentColor.current,
                        )
                        Spacer(Modifier.width(10.dp))
                        Text("Sign in with Apple")
                    }
                    Spacer(Modifier.height(8.dp))
                }

                config.oidcProviders.forEach { provider ->
                    OutlinedButton(
                        onClick = {
                            viewModel.oidcStartUrl(provider.id)?.let { url ->
                                CustomTabsIntent.Builder().build()
                                    .launchUrl(context, Uri.parse(url))
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("Sign in with ${provider.name}")
                    }
                    Spacer(Modifier.height(8.dp))
                }

                if (config.googleLoginEnabled) {
                    OutlinedButton(
                        onClick = {
                            viewModel.googleStartUrl()?.let { url ->
                                CustomTabsIntent.Builder().build()
                                    .launchUrl(context, Uri.parse(url))
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        // Official multi-color "G" — tint must stay Unspecified so
                        // the brand colors aren't overridden.
                        Icon(
                            painter = painterResource(R.drawable.ic_google),
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                            tint = Color.Unspecified,
                        )
                        Spacer(Modifier.width(10.dp))
                        Text("Sign in with Google")
                    }
                    Spacer(Modifier.height(8.dp))
                }

                if (hasOauth && config.passwordEnabled) {
                    Spacer(Modifier.height(8.dp))
                    HorizontalDivider()
                    Spacer(Modifier.height(16.dp))
                }

                if (config.passwordEnabled) {
                    OutlinedTextField(
                        value = email,
                        onValueChange = { email = it },
                        singleLine = true,
                        placeholder = { Text("Email") },
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email),
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("login-email-field"),
                    )
                    Spacer(Modifier.height(12.dp))
                    OutlinedTextField(
                        value = password,
                        onValueChange = { password = it },
                        singleLine = true,
                        placeholder = { Text("Password") },
                        visualTransformation = PasswordVisualTransformation(),
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("login-password-field"),
                    )
                    Spacer(Modifier.height(16.dp))
                    Button(
                        onClick = { viewModel.signIn(email = email, password = password) },
                        enabled = !state.loading && email.isNotBlank() && password.isNotBlank(),
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("login-submit-button"),
                    ) {
                        Text(if (state.loading) "Signing in…" else "Sign in")
                    }
                }

                if (state.error != null) {
                    Spacer(Modifier.height(12.dp))
                    Text(
                        state.error!!,
                        color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            }
        }

        Spacer(Modifier.height(24.dp))

        TextButton(onClick = onChangeInstance) {
            Text("Connect to a different instance")
        }
    }
}
