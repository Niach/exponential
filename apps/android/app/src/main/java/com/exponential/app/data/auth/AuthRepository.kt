package com.exponential.app.data.auth

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

private const val KEY_INSTANCE_URL = "instance_url"
private const val KEY_TOKEN = "session_token"
private const val KEY_USER_EMAIL = "user_email"
private const val KEY_USER_ID = "user_id"
private const val KEY_IS_ADMIN = "is_admin"

@Singleton
class AuthRepository @Inject constructor(
    private val store: SecureStore,
) {
    private val _instanceUrl = MutableStateFlow(store.get(KEY_INSTANCE_URL))
    val instanceUrl: StateFlow<String?> = _instanceUrl.asStateFlow()

    private val _token = MutableStateFlow(store.get(KEY_TOKEN))
    val token: StateFlow<String?> = _token.asStateFlow()

    private val _userEmail = MutableStateFlow(store.get(KEY_USER_EMAIL))
    val userEmail: StateFlow<String?> = _userEmail.asStateFlow()

    private val _userId = MutableStateFlow(store.get(KEY_USER_ID))
    val userId: StateFlow<String?> = _userId.asStateFlow()

    private val _isAdmin = MutableStateFlow(store.get(KEY_IS_ADMIN) == "true")
    val isAdmin: StateFlow<Boolean> = _isAdmin.asStateFlow()

    fun setInstanceUrl(url: String) {
        val normalized = normalizeBaseUrl(url)
        store.set(KEY_INSTANCE_URL, normalized)
        _instanceUrl.value = normalized
    }

    fun clearInstanceUrl() {
        store.set(KEY_INSTANCE_URL, null)
        _instanceUrl.value = null
    }

    fun setToken(token: String, email: String?, userId: String? = null, isAdmin: Boolean = false) {
        store.set(KEY_TOKEN, token)
        store.set(KEY_USER_EMAIL, email)
        store.set(KEY_USER_ID, userId)
        store.set(KEY_IS_ADMIN, if (isAdmin) "true" else "false")
        _token.value = token
        _userEmail.value = email
        _userId.value = userId
        _isAdmin.value = isAdmin
    }

    fun clearToken() {
        store.set(KEY_TOKEN, null)
        store.set(KEY_USER_EMAIL, null)
        store.set(KEY_USER_ID, null)
        store.set(KEY_IS_ADMIN, null)
        _token.value = null
        _userEmail.value = null
        _userId.value = null
        _isAdmin.value = false
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
