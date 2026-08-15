package com.exponential.app.data.auth

import com.exponential.app.data.db.DatabaseHolder
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

@Singleton
class AuthRepository @Inject constructor(
    private val accountStore: AccountStore,
    private val databaseHolder: DatabaseHolder,
) {
    val accounts: StateFlow<List<ServerAccount>> = accountStore.accounts
    val activeAccountId: StateFlow<String?> = accountStore.activeAccountId

    // Effective fields are derived from the active account. We expose StateFlows so existing
    // call sites (HTTP clients, ShapeClient, SyncManager) keep working unchanged.
    private val _instanceUrl = MutableStateFlow(accountStore.activeAccount?.instanceUrl)
    val instanceUrl: StateFlow<String?> = _instanceUrl.asStateFlow()

    private val _token = MutableStateFlow(accountStore.activeAccount?.token)
    val token: StateFlow<String?> = _token.asStateFlow()

    private val _userEmail = MutableStateFlow(accountStore.activeAccount?.userEmail)
    val userEmail: StateFlow<String?> = _userEmail.asStateFlow()

    private val _userId = MutableStateFlow(accountStore.activeAccount?.userId)
    val userId: StateFlow<String?> = _userId.asStateFlow()

    private val _isAdmin = MutableStateFlow(accountStore.activeAccount?.isAdmin ?: false)
    val isAdmin: StateFlow<Boolean> = _isAdmin.asStateFlow()

    private val _onboardingCompletedAt = MutableStateFlow(accountStore.activeAccount?.onboardingCompletedAt)
    val onboardingCompletedAt: StateFlow<String?> = _onboardingCompletedAt.asStateFlow()

    // A login failure that happened outside the login screen's own request (the
    // OAuth deep-link return is handled in MainActivity, not LoginViewModel).
    // The login screen mirrors + consumes this so a failed OAuth resolve shows
    // an error instead of silently dropping the user back on the login form.
    private val _loginError = MutableStateFlow<String?>(null)
    val loginError: StateFlow<String?> = _loginError.asStateFlow()

    fun reportLoginError(message: String) {
        _loginError.value = message
    }

    fun consumeLoginError() {
        _loginError.value = null
    }

    // The PKCE verifier of the in-flight OAuth attempt (REV-13). In-memory
    // only — never persisted: it lives exactly from the start-URL build (a
    // Custom Tab launch) to the oauth-return deep link. Last-start-wins,
    // mirroring the desktop's PendingOAuth: starting a new attempt replaces
    // the old verifier, whose stale code could no longer be redeemed anyway.
    private var pendingOauthVerifier: String? = null

    /** Begin an OAuth attempt: mint + hold a verifier, return its S256 challenge. */
    fun beginOauthAttempt(): String {
        val verifier = OauthPkce.generateVerifier()
        pendingOauthVerifier = verifier
        return OauthPkce.challengeS256(verifier)
    }

    /** Read-and-clear the pending verifier (the code exchange is single-shot). */
    fun consumeOauthVerifier(): String? {
        val verifier = pendingOauthVerifier
        pendingOauthVerifier = null
        return verifier
    }

    fun setInstanceUrl(url: String) {
        val normalized = normalizeBaseUrl(url)
        accountStore.upsertAndActivate(normalized)
        republish()
    }

    fun clearInstanceUrl() {
        val id = accountStore.activeAccountId.value ?: return
        accountStore.remove(id)
        republish()
    }

    // `userId` is required: the login flow resolves it (session fetch + sign-in
    // body, retried) and fails the login rather than call this with a null user,
    // so the account can be keyed per-user. See [AccountStore.resolveActiveAccount].
    fun setToken(
        token: String,
        email: String?,
        userId: String,
        name: String? = null,
        isAdmin: Boolean = false,
        onboardingCompletedAt: String? = null,
        // Only true when onboardingCompletedAt was actually read from the server;
        // false keeps the account out of the wizard (legacy / unknown).
        onboardingKnown: Boolean = false,
    ) {
        val instanceUrl = accountStore.activeAccount?.instanceUrl
        accountStore.resolveActiveAccount(
            token = token,
            email = email,
            name = name,
            userId = userId,
            isAdmin = isAdmin,
            onboardingCompletedAt = onboardingCompletedAt,
            onboardingKnown = onboardingKnown,
        )
        // The resolve just re-keyed this instance to its per-user id, dropping
        // the pending URL-only row — but the pending id's Room files may exist
        // (AppViewModel's always-on flows open the active account's DB while
        // the user sits on the login screen), and nothing else ever sweeps
        // them (REV-15). Delete them here, unconditionally per login: that
        // also heals orphans left behind by earlier builds. Guarded on the id
        // being row-less, so a legacy still-URL-keyed account is never wiped.
        if (instanceUrl != null) {
            val pendingId = ServerAccount.makeId(instanceUrl)
            if (accountStore.accounts.value.none { it.id == pendingId }) {
                databaseHolder.deleteFiles(pendingId)
            }
        }
        republish()
    }

    fun clearToken() {
        accountStore.clearActiveToken()
        republish()
    }

    // Sign a SPECIFIC account out locally (keeping its row). The active
    // account's token is what AppNavHost's needsAuth gate reads, so clearing it
    // routes to login; a background account just stops syncing.
    fun clearToken(id: String) {
        accountStore.clearToken(id)
        republish()
    }

    // Mark the active account onboarded (after onboarding.complete succeeds) so the
    // nav gate stops showing the wizard without needing a fresh session fetch.
    fun markOnboardingCompleted(completedAtIso: String) {
        val id = accountStore.activeAccountId.value ?: return
        accountStore.setOnboardingCompletedAt(id, completedAtIso)
        republish()
    }

    fun switchAccount(id: String) {
        accountStore.setActive(id)
        republish()
    }

    fun removeAccount(id: String) {
        accountStore.remove(id)
        republish()
    }

    private fun republish() {
        val active = accountStore.activeAccount
        _instanceUrl.value = active?.instanceUrl
        _token.value = active?.token
        _userEmail.value = active?.userEmail
        _userId.value = active?.userId
        _isAdmin.value = active?.isAdmin ?: false
        _onboardingCompletedAt.value = active?.onboardingCompletedAt
    }

    private fun normalizeBaseUrl(input: String): String {
        val trimmed = input.trim().trimEnd('/')
        return if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) {
            trimmed
        } else {
            "https://$trimmed"
        }
    }
}
