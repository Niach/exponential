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

    // While the user is going through the "add server" flow we locally clear instanceUrl/token
    // so AppRoot routes back to InstanceScreen, without touching AccountStore. cancelAddServer()
    // restores the prior active account.
    private val _isAddingServer = MutableStateFlow(false)
    val isAddingServer: StateFlow<Boolean> = _isAddingServer.asStateFlow()

    fun setInstanceUrl(url: String) {
        val normalized = normalizeBaseUrl(url)
        accountStore.upsertAndActivate(normalized)
        _isAddingServer.value = false
        republish()
    }

    fun clearInstanceUrl() {
        val id = accountStore.activeAccountId.value ?: return
        accountStore.remove(id)
        republish()
    }

    fun setToken(token: String, email: String?, userId: String? = null, isAdmin: Boolean = false) {
        accountStore.updateActiveToken(token = token, email = email, name = null, userId = userId, isAdmin = isAdmin)
        republish()
    }

    fun clearToken() {
        accountStore.clearActiveToken()
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

    fun startAddServer() {
        _isAddingServer.value = true
        _instanceUrl.value = null
        _token.value = null
        _userEmail.value = null
        _userId.value = null
        _isAdmin.value = false
    }

    fun cancelAddServer() {
        _isAddingServer.value = false
        republish()
    }

    private fun republish() {
        val active = accountStore.activeAccount
        _instanceUrl.value = active?.instanceUrl
        _token.value = active?.token
        _userEmail.value = active?.userEmail
        _userId.value = active?.userId
        _isAdmin.value = active?.isAdmin ?: false
        _isAddingServer.value = false
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
