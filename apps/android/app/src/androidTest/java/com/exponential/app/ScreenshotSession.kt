package com.exponential.app

import android.util.Log
import androidx.test.platform.app.InstrumentationRegistry
import com.exponential.app.data.api.SignInResult
import com.exponential.app.data.auth.ServerAccount
import dagger.hilt.android.EntryPointAccessors
import java.io.File
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.rules.ExternalResource

/**
 * Signs the screenshot suites in WITHOUT the login UI (EXP-1267).
 *
 * The suites used to reinstall the app before every run (screengrab
 * `reinstall_app`) and type the credentials into the instance picker and the
 * login form. The test runs in the app's own process, so it signs in through
 * the app's REAL login path instead — `AuthApi.signInWithPassword` (mint) or
 * `AuthApi.completeLogin` (reuse), the calls LoginViewModel makes — reached via
 * the debug-only [ScreenshotSessionEntryPoint]. The app is no longer
 * reinstalled, so its per-user Room caches survive between runs as well.
 *
 * The session token is REUSED across runs and test cases: it is cached in the
 * app's cache dir on THIS device (so two lanes on two emulators never share
 * one) and re-validated with one `get-session` read before use. A reseed
 * deletes the demo users, so that read comes back empty and a fresh token is
 * minted with the seed's password. Only when the instance cannot be reached or
 * rejects the password does [signIn] return false; it then leaves the app
 * signed out so the suite's login-UI fallback starts from the instance picker.
 */
object ScreenshotSession {
    private const val TAG = "EXP-1267"
    private const val REQUEST_TIMEOUT_MS = 15_000L

    private fun hooks(): ScreenshotSessionEntryPoint =
        EntryPointAccessors.fromApplication(
            InstrumentationRegistry.getInstrumentation().targetContext.applicationContext,
            ScreenshotSessionEntryPoint::class.java,
        )

    /** Drops every account: the next launch lands on the untouched instance picker. */
    fun signOutEverything() {
        val auth = hooks().authRepository()
        auth.accounts.value.map { it.id }.forEach(auth::removeAccount)
    }

    /**
     * Signs [email] in and makes them the ACTIVE account. Unless
     * [keepOtherAccounts], every other account is dropped too, so the app
     * shows exactly one server (a fresh install's state). Returns whether the
     * app now holds a session for [email]; false = use the login UI.
     */
    fun signIn(
        instanceUrl: String,
        email: String,
        password: String,
        keepOtherAccounts: Boolean = false,
    ): Boolean = runBlocking {
        val hooks = hooks()
        val auth = hooks.authRepository()
        val api = hooks.authApi()
        val base = instanceUrl.trim().trimEnd('/')
        val cache = cacheFile(base, email)

        val cached = cache.takeIf { it.exists() }?.readText()?.trim()?.takeIf { it.isNotEmpty() }
        val reusable = cached != null && withTimeoutOrNull(REQUEST_TIMEOUT_MS) {
            api.fetchSession(base, cached)
        }?.email.equals(email, ignoreCase = true)

        val signedIn = if (reusable) {
            auth.setInstanceUrl(base)
            val ok = withTimeoutOrNull(REQUEST_TIMEOUT_MS) {
                api.completeLogin(base, cached!!, userIdHint = null, emailHint = email, isAdminHint = false)
            } == true
            if (ok) Log.i(TAG, "session: reusing the cached session for $email")
            ok
        } else {
            auth.setInstanceUrl(base)
            val result = withTimeoutOrNull(REQUEST_TIMEOUT_MS) {
                api.signInWithPassword(base, email, password)
            }
            if (result is SignInResult.Success) {
                cache.parentFile?.mkdirs()
                cache.writeText(result.token)
                Log.i(TAG, "session: minted a new session for $email")
                true
            } else {
                Log.w(TAG, "session: could not sign $email in at $base ($result) — falling back to the login UI")
                false
            }
        }

        val active = auth.accounts.value.firstOrNull { it.id == auth.activeAccountId.value }
        if (!signedIn || active?.token == null || !active.userEmail.equals(email, ignoreCase = true)) {
            cache.delete()
            if (keepOtherAccounts) {
                // Drop only the pending row this attempt added.
                auth.removeAccount(ServerAccount.makeId(base))
            } else {
                signOutEverything()
            }
            return@runBlocking false
        }
        if (!keepOtherAccounts) {
            auth.accounts.value.filter { it.id != active.id }.forEach { auth.removeAccount(it.id) }
        }
        true
    }

    private fun cacheFile(base: String, email: String): File {
        val key = "$base-$email".map { if (it.isLetterOrDigit()) it else '_' }.joinToString("")
        val dir = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "exp-sessions")
        return File(dir, "$key.token")
    }
}

/**
 * Runs BEFORE the activity launches (chain it outside the compose rule):
 * signs [email] in through [ScreenshotSession], or — with a null [email] —
 * leaves the app with no account at all (the untouched instance picker).
 * [signedIn] reports whether the fast path held; false = drive the login UI.
 */
class ScreenshotSessionRule(
    private val email: String?,
    private val password: String? = null,
) : ExternalResource() {
    var signedIn: Boolean = false
        private set

    override fun before() {
        signedIn = if (email == null) {
            ScreenshotSession.signOutEverything()
            false
        } else {
            ScreenshotSession.signIn(ScreenshotFlow.instanceUrl(), email, password.orEmpty())
        }
    }
}
