package com.exponential.app.ui.home

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.auth.AuthRepository
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class HomeState(val email: String? = null)

@HiltViewModel
class HomeViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val api: AuthApi,
) : ViewModel() {
    private val _state = MutableStateFlow(HomeState(email = auth.userEmail.value))
    val state: StateFlow<HomeState> = _state.asStateFlow()

    fun loadSession() {
        viewModelScope.launch {
            val email = api.fetchSession() ?: auth.userEmail.value
            _state.value = HomeState(email = email)
        }
    }
}
