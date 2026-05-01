package com.exponential.app.data.auth

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

private const val KEY_INSTANCE_URL = "instance_url"
private const val KEY_TOKEN = "session_token"
private const val KEY_USER_EMAIL = "user_email"

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

    fun setInstanceUrl(url: String) {
        val normalized = normalizeBaseUrl(url)
        store.set(KEY_INSTANCE_URL, normalized)
        _instanceUrl.value = normalized
    }

    fun clearInstanceUrl() {
        store.set(KEY_INSTANCE_URL, null)
        _instanceUrl.value = null
    }

    fun setToken(token: String, email: String?) {
        store.set(KEY_TOKEN, token)
        store.set(KEY_USER_EMAIL, email)
        _token.value = token
        _userEmail.value = email
    }

    fun clearToken() {
        store.set(KEY_TOKEN, null)
        store.set(KEY_USER_EMAIL, null)
        _token.value = null
        _userEmail.value = null
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
