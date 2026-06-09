package com.exponential.app.data.auth

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

@Singleton
class AuthRepository @Inject constructor(
    private val accountStore: AccountStore,
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

    fun setToken(
        token: String,
        email: String?,
        userId: String? = null,
        isAdmin: Boolean = false,
        onboardingCompletedAt: String? = null,
        // Only true when onboardingCompletedAt was actually read from the server;
        // false keeps the account out of the wizard (legacy / unknown).
        onboardingKnown: Boolean = false,
    ) {
        accountStore.updateActiveToken(
            token = token,
            email = email,
            name = null,
            userId = userId,
            isAdmin = isAdmin,
            onboardingCompletedAt = onboardingCompletedAt,
            onboardingKnown = onboardingKnown,
        )
        republish()
    }

    fun clearToken() {
        accountStore.clearActiveToken()
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
