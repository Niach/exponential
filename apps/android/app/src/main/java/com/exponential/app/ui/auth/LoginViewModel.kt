package com.exponential.app.ui.auth

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.api.SignInResult
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class LoginState(
    val loading: Boolean = false,
    val error: String? = null,
    val successEmail: String? = null,
)

@HiltViewModel
class LoginViewModel @Inject constructor(
    private val api: AuthApi,
) : ViewModel() {
    private val _state = MutableStateFlow(LoginState())
    val state: StateFlow<LoginState> = _state.asStateFlow()

    fun signIn(email: String, password: String) {
        if (_state.value.loading) return
        _state.value = LoginState(loading = true)
        viewModelScope.launch {
            when (val result = api.signInWithPassword(email = email, password = password)) {
                is SignInResult.Success -> {
                    _state.value = LoginState(successEmail = result.email)
                }
                is SignInResult.Failure -> {
                    _state.value = LoginState(error = result.message)
                }
            }
        }
    }
}
