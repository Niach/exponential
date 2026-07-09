package com.exponential.app

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.lifecycle.lifecycleScope
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.data.share.ShareIntentParser
import com.exponential.app.navigation.AppNavHost
import com.exponential.app.ui.theme.ExponentialTheme
import dagger.hilt.android.AndroidEntryPoint
import javax.inject.Inject
import kotlinx.coroutines.launch

@AndroidEntryPoint
class MainActivity : ComponentActivity() {
    @Inject lateinit var authRepository: AuthRepository
    @Inject lateinit var authApi: AuthApi
    @Inject lateinit var deepLinkBus: DeepLinkBus

    private val notificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { /* ignored */ }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
        )
        handleIntent(intent)
        maybeRequestNotificationPermission()
        setContent {
            ExponentialTheme {
                AppNavHost()
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleIntent(intent)
    }

    private fun handleIntent(intent: Intent?) {
        // Shared content (ACTION_SEND/_MULTIPLE) arrives with a null data URI, so
        // this must run before the exponential:// deep-link guard below. Parse +
        // copy images while the read grant is still live (see ShareIntentParser).
        if (intent != null && ShareIntentParser.isShareIntent(intent)) {
            val payload = ShareIntentParser.parse(this, intent) ?: return
            deepLinkBus.openShare(payload.text, payload.subject, payload.imageUris)
            return
        }
        val data = intent?.data ?: return
        if (data.scheme != "exponential") return
        when (data.host) {
            "oauth-return" -> handleOauthReturn(data)
            "issue" -> data.pathSegments.firstOrNull()?.let { deepLinkBus.openIssue(it) }
            "invite" -> data.pathSegments.firstOrNull()?.let { deepLinkBus.openInvite(it) }
            // Fired by the server's post-GitHub-App-install page: closes the
            // Custom Tab (singleTask clear-top) and lands back on the repo
            // picker, which consumes this and re-fetches the repo list.
            "github-connected" -> deepLinkBus.openGithubConnected()
        }
    }

    private fun handleOauthReturn(data: android.net.Uri) {
        // Token is in the URL fragment so it never lands in server logs.
        //
        // Pull the *encoded* fragment and decode once with Uri.decode (URI-style,
        // `+` stays literal). data.fragment + URLDecoder.decode would form-decode
        // `+` → space and corrupt the base64 HMAC signature better-auth appends
        // to session cookies (signed cookie value is `${id}.${btoa(HMAC)}` and
        // btoa emits `+` `/` `=`). A mangled signature fails HMAC verification
        // in better-auth's bearer plugin and every authed request 401s.
        val encodedFragment = data.encodedFragment ?: return
        val token = encodedFragment
            .split("&")
            .map { it.split("=", limit = 2) }
            .firstOrNull { it.firstOrNull() == "token" }
            ?.getOrNull(1)
            ?.let { android.net.Uri.decode(it) }
            ?: return
        lifecycleScope.launch {
            // completeLogin resolves the userId (session fetch, retried) and
            // captures the onboarding flag in the same step, persisting the token
            // as a per-user account. If no userId can be resolved it stores
            // nothing — the app stays on login rather than key the wrong account.
            val account = authRepository.accounts.value
                .firstOrNull { it.id == authRepository.activeAccountId.value }
                ?: return@launch
            val ok = authApi.completeLogin(
                baseUrl = account.instanceUrl,
                token = token,
                userIdHint = account.userId,
                emailHint = account.userEmail,
                isAdminHint = account.isAdmin,
            )
            // No userId could be resolved → nothing was persisted and the app
            // stays on login; surface the same message the password path uses so
            // the return isn't a silent no-op.
            if (!ok) {
                authRepository.reportLoginError("Couldn't verify your account. Please try again.")
            }
        }
    }

    private fun maybeRequestNotificationPermission() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        val granted = ContextCompat.checkSelfPermission(
            this,
            Manifest.permission.POST_NOTIFICATIONS,
        ) == PackageManager.PERMISSION_GRANTED
        if (!granted) notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
    }
}
