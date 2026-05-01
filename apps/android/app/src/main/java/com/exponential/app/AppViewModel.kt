package com.exponential.app

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.auth.AuthRepository
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
            auth.clearToken()
            auth.clearInstanceUrl()
        }
    }

    fun signOut() {
        viewModelScope.launch { auth.clearToken() }
    }
}
