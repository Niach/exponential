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
import com.exponential.app.data.push.PushDeepLinks
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
                    teamSlug = parsed.teamSlug,
                    identifier = parsed.identifier,
                )
                is WebLinks.Parsed.Invite -> deepLinkBus.openInvite(parsed.token)
                null -> {}
            }
            return
        }
        if (data.scheme != "exponential") return
        // A push-built link carries its recipient (REV2-81); switch to that
        // account before publishing so the id resolves against the right
        // local database. Absent on every other producer of these links.
        val linkUserId = data.getQueryParameter(PushDeepLinks.PARAM_USER_ID)
        when (data.host) {
            "oauth-return" -> handleOauthReturn(data)
            "issue" -> data.pathSegments.firstOrNull()?.let {
                if (switchToPushAccount(linkUserId)) deepLinkBus.openIssue(it)
            }
            "invite" -> data.pathSegments.firstOrNull()?.let { deepLinkBus.openInvite(it) }
            // support_reply push taps (EXP-180): straight to the ticket.
            "support" -> data.pathSegments.firstOrNull()?.let {
                if (switchToPushAccount(linkUserId)) deepLinkBus.openSupportThread(it)
            }
            // Fired by the server's post-GitHub-App-install page: closes the
            // Custom Tab (singleTask clear-top) and lands back on the repo
            // picker, which consumes this and re-fetches the repo list. The
            // `error` slug marks a FAILED connect (EXP-390) — dropped before,
            // which made every failure a silent no-op.
            "github-connected" -> deepLinkBus.openGithubConnected(data.getQueryParameter("error")?.ifEmpty { null })
        }
    }

    // Route a backgrounded push tap's launcher-intent extras to its target.
    // Same account resolution as FcmService.onMessageReceived (REV2-81): a push
    // for a non-active but still signed-in account switches to that account
    // first instead of being dropped; one for an account that's gone stays
    // dropped, since its ids would dead-end in no local database at all.
    // AccountStore loads synchronously, so the resolution is valid even during
    // onCreate. AppNavHost parks the bus target until the auth token is ready,
    // so a cold-start tap navigates post-login.
    private fun handlePushExtras(intent: Intent) {
        val target = PushDeepLinks.target(
            type = intent.getStringExtra("type"),
            issueId = intent.getStringExtra("issueId"),
            threadId = intent.getStringExtra("threadId"),
        ) ?: return
        if (!switchToPushAccount(intent.getStringExtra("userId"))) return
        // Belt-and-braces beside the savedInstanceState gate: an in-process
        // recreation reuses this same Intent instance via getIntent().
        when (target) {
            is PushDeepLinks.Target.Issue -> {
                intent.removeExtra("issueId")
                deepLinkBus.openIssue(target.id)
            }
            is PushDeepLinks.Target.SupportThread -> {
                intent.removeExtra("threadId")
                deepLinkBus.openSupportThread(target.id)
            }
        }
    }

    /**
     * Make the account a push belongs to active, returning false when it
     * belongs to no signed-in account here (nothing to open). A null
     * [targetUserId] is a link from any other producer — or a server predating
     * the hint — and stays on the active account.
     */
    private fun switchToPushAccount(targetUserId: String?): Boolean {
        val account = PushDeepLinks.resolveAccount(
            accounts = authRepository.accounts.value,
            activeAccountId = authRepository.activeAccountId.value,
            targetUserId = targetUserId,
        )
        return when (account) {
            PushDeepLinks.Account.Active -> true
            is PushDeepLinks.Account.Switch -> {
                authRepository.switchAccount(account.id)
                true
            }
            PushDeepLinks.Account.Unknown -> false
        }
    }

    private fun handleOauthReturn(data: android.net.Uri) {
        // Failure handoff (REV2-53): every failing branch of the web hop now
        // deep-links back with `error=<reason>` instead of stranding the user
        // on an https page the Custom Tab can't hand back. Surface it on the
        // login screen (which mirrors + consumes reportLoginError).
        val error = oauthReturnParam(data, "error")
        if (error != null) {
            authRepository.consumeOauthVerifier() // this attempt is over
            authRepository.reportLoginError(oauthErrorMessage(error))
            return
        }
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

    // Human copy for the server's error reason (REV2-53). Mirrors the web's
    // `oauthErrorMessage` and iOS `LoginViewModel.oauthErrorMessage`; unknown
    // reasons get the generic line so a new server slug is never shown raw.
    private fun oauthErrorMessage(reason: String): String = when (reason) {
        "access_denied" -> "Sign-in was cancelled."
        "state_missing", "state_invalid", "state_mismatch", "state_not_found",
        "please_restart_the_process" -> "That sign-in link expired. Please try again."
        "no_session", "session_cookie_missing" ->
            "Sign-in didn't complete on the server. Please try again."
        else -> "Couldn't complete sign-in. Please try again."
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
