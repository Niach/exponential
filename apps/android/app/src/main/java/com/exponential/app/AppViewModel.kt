package com.exponential.app

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.push.PushTokenManager
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class AppState(
    val instanceUrl: String? = null,
    val token: String? = null,
)

@HiltViewModel
class AppViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val syncManager: SyncManager,
    private val pushTokenManager: PushTokenManager,
) : ViewModel() {

    val state: StateFlow<AppState> = combine(
        auth.instanceUrl,
        auth.token,
    ) { url, token -> AppState(instanceUrl = url, token = token) }
        .stateIn(viewModelScope, SharingStarted.Eagerly, AppState())

    fun setInstanceUrl(url: String) {
        viewModelScope.launch { auth.setInstanceUrl(url) }
    }

    fun clearInstance() {
        viewModelScope.launch {
            pushTokenManager.unregisterAndForget()
            syncManager.signOut()
            auth.clearToken()
            auth.clearInstanceUrl()
        }
    }

    fun signOut() {
        viewModelScope.launch {
            pushTokenManager.unregisterAndForget()
            syncManager.signOut()
            auth.clearToken()
        }
    }
}
