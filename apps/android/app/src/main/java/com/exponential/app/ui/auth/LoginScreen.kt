package com.exponential.app.ui.auth

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
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
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.R
import com.exponential.app.data.api.AuthWebUrls
import com.exponential.app.ui.components.GlassOAuthButton
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.icons.ExpIcons

@Composable
fun LoginScreen(
    instanceUrl: String,
    onLoggedIn: () -> Unit,
    onChangeInstance: () -> Unit,
    viewModel: LoginViewModel = hiltViewModel(),
) {
    var email by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var code by remember { mutableStateOf("") }
    val state by viewModel.state.collectAsStateWithLifecycle()
    val context = LocalContext.current

    if (state.successEmail != null) {
        onLoggedIn()
    }

    // The passkey path's browser handoff (EXP-857): a one-shot URL the
    // ViewModel raises when the on-device ceremony can't run. The Custom Tab
    // ends on the same exponential://oauth-return deep link MainActivity
    // already redeems.
    LaunchedEffect(state.fallbackUrl) {
        state.fallbackUrl?.let { url ->
            CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(url))
            viewModel.consumeFallbackUrl()
        }
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
            // EXP-857: the one login title on all four clients. This screen no
            // longer says "Sign in" anywhere.
            "Continue to Exponential",
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
                // "Continue with email" covers both email paths: the one-time
                // code when the instance can mail, the password form otherwise.
                val emailAvailable = config.emailOtpEnabled || config.passwordEnabled
                val codeFlow = config.emailOtpEnabled && !state.usePassword
                val busyLabel = when (state.busy) {
                    LoginBusy.SendingCode -> "Sending code…"
                    else -> "Checking…"
                }

                // Provider buttons: the shared glass OAuth button with the
                // "Continue with …" wording every client shares (EXP-577/857).
                if (config.appleLoginEnabled) {
                    GlassOAuthButton(
                        label = "Continue with Apple",
                        onClick = {
                            viewModel.appleStartUrl()?.let { url ->
                                CustomTabsIntent.Builder().build()
                                    .launchUrl(context, Uri.parse(url))
                            }
                        },
                    ) {
                        // Monochrome Apple mark, tinted with the current content
                        // color (the Compose "currentColor").
                        Icon(
                            painter = painterResource(R.drawable.ic_apple),
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                            tint = LocalContentColor.current,
                        )
                    }
                    Spacer(Modifier.height(8.dp))
                }

                if (config.googleLoginEnabled) {
                    GlassOAuthButton(
                        label = "Continue with Google",
                        onClick = {
                            viewModel.googleStartUrl()?.let { url ->
                                CustomTabsIntent.Builder().build()
                                    .launchUrl(context, Uri.parse(url))
                            }
                        },
                    ) {
                        // Official multi-color "G" — tint must stay Unspecified so
                        // the brand colors aren't overridden.
                        Icon(
                            painter = painterResource(R.drawable.ic_google),
                            contentDescription = null,
                            modifier = Modifier.size(17.dp),
                            tint = Color.Unspecified,
                        )
                    }
                    Spacer(Modifier.height(8.dp))
                }

                config.oidcProviders.forEach { provider ->
                    GlassOAuthButton(
                        label = "Continue with ${provider.name}",
                        onClick = {
                            viewModel.oidcStartUrl(provider.id)?.let { url ->
                                CustomTabsIntent.Builder().build()
                                    .launchUrl(context, Uri.parse(url))
                            }
                        },
                    ) {}
                    Spacer(Modifier.height(8.dp))
                }

                if (emailAvailable && state.emailStep == LoginEmailStep.Hidden) {
                    GlassOAuthButton(
                        label = "Continue with email",
                        onClick = { viewModel.continueWithEmail() },
                        modifier = Modifier.testTag("login-continue-with-email"),
                    ) {
                        Icon(
                            imageVector = ExpIcons.uiMail,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                            tint = LocalContentColor.current,
                        )
                    }
                    Spacer(Modifier.height(8.dp))
                }

                if (emailAvailable && state.emailStep != LoginEmailStep.Hidden) {
                    if (hasOauth) {
                        Spacer(Modifier.height(8.dp))
                        HorizontalDivider()
                        Spacer(Modifier.height(16.dp))
                    }

                    if (codeFlow) {
                        if (state.emailStep == LoginEmailStep.CodeSent) {
                            Text(
                                "We sent a 6-digit code to ${state.codeEmail}.",
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                            Spacer(Modifier.height(12.dp))
                            GlassTextField(
                                value = code,
                                onValueChange = { code = it },
                                singleLine = true,
                                placeholder = "Code",
                                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .testTag("login-code-field"),
                            )
                            Spacer(Modifier.height(16.dp))
                            Button(
                                onClick = { viewModel.verifyCode(code) },
                                enabled = !state.loading && code.isNotBlank(),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .testTag("login-submit-button"),
                            ) {
                                Text(if (state.loading) busyLabel else "Continue")
                            }
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.Center,
                            ) {
                                TextButton(
                                    onClick = { viewModel.resendCode() },
                                    enabled = !state.loading,
                                    modifier = Modifier.testTag("login-resend-code"),
                                ) {
                                    Text("Resend code")
                                }
                                TextButton(
                                    onClick = {
                                        code = ""
                                        viewModel.changeEmail()
                                    },
                                    enabled = !state.loading,
                                    modifier = Modifier.testTag("login-change-email"),
                                ) {
                                    Text("Use a different email")
                                }
                            }
                        } else {
                            GlassTextField(
                                value = email,
                                onValueChange = { email = it },
                                singleLine = true,
                                placeholder = "Email",
                                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .testTag("login-email-field"),
                            )
                            Spacer(Modifier.height(16.dp))
                            Button(
                                onClick = { viewModel.sendCode(email.trim()) },
                                enabled = !state.loading && email.isNotBlank(),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .testTag("login-submit-button"),
                            ) {
                                Text(if (state.loading) busyLabel else "Send code")
                            }
                            // An instance with both paths on keeps the password
                            // form one tap away; the code flow is primary.
                            if (config.passwordEnabled) {
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    horizontalArrangement = Arrangement.Center,
                                ) {
                                    TextButton(
                                        onClick = { viewModel.usePasswordInstead() },
                                        modifier = Modifier.testTag("login-use-password"),
                                    ) {
                                        Text("Use a password instead")
                                    }
                                }
                            }
                        }
                    } else {
                        GlassTextField(
                            value = email,
                            onValueChange = { email = it },
                            singleLine = true,
                            placeholder = "Email",
                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email),
                            modifier = Modifier
                                .fillMaxWidth()
                                .testTag("login-email-field"),
                        )
                        Spacer(Modifier.height(12.dp))
                        GlassTextField(
                            value = password,
                            onValueChange = { password = it },
                            singleLine = true,
                            placeholder = "Password",
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
                            Text(if (state.loading) busyLabel else "Continue")
                        }

                        // Sign-up and password reset are web flows on every native
                        // client (desktop parity) — hand off to a Custom Tab, and
                        // only for what the server publishes as available.
                        if (config.passwordResetEnabled || config.signupEnabled) {
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.Center,
                            ) {
                                if (config.passwordResetEnabled) {
                                    TextButton(
                                        onClick = {
                                            CustomTabsIntent.Builder().build().launchUrl(
                                                context,
                                                Uri.parse(AuthWebUrls.forgotPassword(instanceUrl)),
                                            )
                                        },
                                        modifier = Modifier.testTag("login-forgot-password-link"),
                                    ) {
                                        Text("Forgot password?")
                                    }
                                }
                                if (config.signupEnabled) {
                                    TextButton(
                                        onClick = {
                                            CustomTabsIntent.Builder().build().launchUrl(
                                                context,
                                                Uri.parse(AuthWebUrls.register(instanceUrl)),
                                            )
                                        },
                                        modifier = Modifier.testTag("login-create-account-link"),
                                    ) {
                                        Text("Create account")
                                    }
                                }
                            }
                        }
                    }

                    Spacer(Modifier.height(8.dp))
                }

                if (config.passkeyEnabled) {
                    GlassOAuthButton(
                        label = "Login with passkey",
                        onClick = {
                            context.findActivity()?.let { viewModel.startPasskeyLogin(it) }
                        },
                        modifier = Modifier.testTag("login-passkey-button"),
                    ) {
                        Icon(
                            imageVector = ExpIcons.authPasskey,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                            tint = LocalContentColor.current,
                        )
                    }
                    Spacer(Modifier.height(8.dp))
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

// CredentialManager needs the hosting Activity (it shows a system sheet), and
// LocalContext hands back a wrapper inside a ComponentActivity.
private fun Context.findActivity(): Activity? {
    var ctx: Context = this
    while (ctx is ContextWrapper) {
        if (ctx is Activity) return ctx
        ctx = ctx.baseContext
    }
    return null
}
