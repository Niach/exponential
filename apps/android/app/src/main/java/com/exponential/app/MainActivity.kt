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
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.lifecycle.lifecycleScope
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.data.share.ShareIntentParser
import com.exponential.app.domain.WebLinks
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
        // Must run before super.onCreate(): applies postSplashScreenTheme so the
        // activity leaves Theme.Exponential.Splash once the content view is set.
        installSplashScreen()
        super.onCreate(savedInstanceState)
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
        )
        // Push-tap extras are only consumed on a FRESH delivery: a recreation
        // (config change or process-death restore) redelivers the same launcher
        // intent with the same extras; savedInstanceState != null identifies it.
        handleIntent(intent, allowPushExtras = savedInstanceState == null)
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
        // A singleTask re-delivery is always a fresh tap.
        handleIntent(intent, allowPushExtras = true)
    }

    private fun handleIntent(intent: Intent?, allowPushExtras: Boolean) {
        // Shared content (ACTION_SEND/_MULTIPLE) arrives with a null data URI, so
        // this must run before the exponential:// deep-link guard below. Parse +
        // copy images while the read grant is still live (see ShareIntentParser).
        if (intent != null && ShareIntentParser.isShareIntent(intent)) {
            val payload = ShareIntentParser.parse(this, intent) ?: return
            deepLinkBus.openShare(payload.text, payload.subject, payload.imageUris)
            return
        }
        val data = intent?.data
        if (data == null) {
            // EXP-172: notification-type FCM messages are rendered by the FCM
            // SDK while the app is backgrounded (FcmService never runs — see the
            // AndroidManifest note), and the tap launches this activity with the
            // data payload ({type, issueId, identifier, userId}) as plain
            // launcher-intent EXTRAS, not a data URI. Route it through the same
            // bus as the foreground-built PendingIntent path.
            if (intent != null && allowPushExtras) handlePushExtras(intent)
            return
        }
        // Verified App Links (EXP-92): https issue/invite URLs from the
        // manifest's autoVerify filter. Kept dumb here — resolution (slug +
        // identifier → local issue id) happens in AppNavHost, which already
        // parks targets until auth/sync are ready.
        if (data.scheme == "https" || data.scheme == "http") {
            when (val parsed = WebLinks.parsePath(data.path)) {
                is WebLinks.Parsed.IssueRef -> deepLinkBus.openWebIssueRef(
                    uri = data,
                    host = data.host ?: return,
                    workspaceSlug = parsed.workspaceSlug,
                    identifier = parsed.identifier,
                )
                is WebLinks.Parsed.Invite -> deepLinkBus.openInvite(parsed.token)
                null -> {}
            }
            return
        }
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

    // Route a backgrounded push tap's launcher-intent extras to the issue. Same
    // active-account guard as FcmService.onMessageReceived: only deep-link when
    // the push targets the ACTIVE account (another account's issue id would
    // dead-end in the wrong local database); servers predating the userId hint
    // omit it — keep the link then. AccountStore loads synchronously, so the
    // guard is valid even during onCreate. AppNavHost parks the bus target
    // until the auth token is ready, so a cold-start tap navigates post-login.
    private fun handlePushExtras(intent: Intent) {
        val issueId = intent.getStringExtra("issueId") ?: return
        val targetUserId = intent.getStringExtra("userId")
        if (targetUserId != null && targetUserId != authRepository.userId.value) return
        // Belt-and-braces beside the savedInstanceState gate: an in-process
        // recreation reuses this same Intent instance via getIntent().
        intent.removeExtra("issueId")
        deepLinkBus.openIssue(issueId)
    }

    private fun handleOauthReturn(data: android.net.Uri) {
        // New servers deliver a single-use PKCE `code` (REV-13) we redeem via
        // /api/mobile-oauth-exchange with the in-memory verifier; old servers
        // (self-hosted lag) still deliver the raw `token`. Both ride in the
        // fragment AND the query (EXP-21 — browsers drop the #fragment when
        // handing a custom scheme to the OS, so scan both).
        val code = oauthReturnParam(data, "code")
        if (code != null) {
            val verifier = authRepository.consumeOauthVerifier()
            if (verifier == null) {
                // A code arrived without an attempt this process started —
                // out-of-band (or interception replay); nothing to redeem with.
                authRepository.reportLoginError("Couldn't verify your account. Please try again.")
                return
            }
            lifecycleScope.launch {
                val account = authRepository.accounts.value
                    .firstOrNull { it.id == authRepository.activeAccountId.value }
                    ?: return@launch
                val token = authApi.exchangeOauthCode(account.instanceUrl, code, verifier)
                if (token == null) {
                    authRepository.reportLoginError("Couldn't verify your account. Please try again.")
                    return@launch
                }
                completeOauthLogin(token)
            }
            return
        }
        // Legacy path: raw token in the deep link (pre-PKCE servers only).
        val token = oauthReturnParam(data, "token") ?: return
        lifecycleScope.launch { completeOauthLogin(token) }
    }

    // Scan the *encoded* fragment first (primary form), then the encoded query,
    // and decode once with Uri.decode (URI-style, `+` stays literal).
    // data.fragment + URLDecoder.decode would form-decode `+` → space and
    // corrupt the base64 HMAC signature better-auth appends to session cookies
    // (signed cookie value is `${id}.${btoa(HMAC)}` and btoa emits `+` `/` `=`).
    // A mangled signature fails HMAC verification in better-auth's bearer
    // plugin and every authed request 401s. (PKCE codes are pure base64url and
    // decode-inert, but the legacy token path still needs this care.)
    private fun oauthReturnParam(data: android.net.Uri, key: String): String? {
        for (encoded in listOfNotNull(data.encodedFragment, data.encodedQuery)) {
            val value = encoded
                .split("&")
                .map { it.split("=", limit = 2) }
                .firstOrNull { it.firstOrNull() == key }
                ?.getOrNull(1)
                ?.let { android.net.Uri.decode(it) }
            if (!value.isNullOrEmpty()) return value
        }
        return null
    }

    private suspend fun completeOauthLogin(token: String) {
        // completeLogin resolves the userId (session fetch, retried) and
        // captures the onboarding flag in the same step, persisting the token
        // as a per-user account. If no userId can be resolved it stores
        // nothing — the app stays on login rather than key the wrong account.
        val account = authRepository.accounts.value
            .firstOrNull { it.id == authRepository.activeAccountId.value }
            ?: return
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

    private fun maybeRequestNotificationPermission() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        val granted = ContextCompat.checkSelfPermission(
            this,
            Manifest.permission.POST_NOTIFICATIONS,
        ) == PackageManager.PERMISSION_GRANTED
        if (!granted) notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
    }
}
