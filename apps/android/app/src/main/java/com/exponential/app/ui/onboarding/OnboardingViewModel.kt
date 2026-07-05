package com.exponential.app.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.api.OnboardingApi
import com.exponential.app.data.auth.AuthRepository
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

// The mobile app is a pure companion (masterplan L26): workspaces and projects
// are created on the web or desktop app, so onboarding is a single informational
// screen instead of a create-project/issue wizard. `onboarding.complete` (and the
// local needsOnboarding flag) is flipped on Continue so the nav gate stops showing
// this screen. The server also backfills onboardingCompletedAt on session reads for
// users who already have a project in a non-public workspace (lib/auth/onboarding.ts),
// so a stale account self-heals via reconcile() before the user ever taps Continue.
@HiltViewModel
class OnboardingViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val authApi: AuthApi,
    private val onboardingApi: OnboardingApi,
) : ViewModel() {

    val instanceUrl: StateFlow<String?> = auth.instanceUrl

    private val _busy = MutableStateFlow(false)
    val busy: StateFlow<Boolean> = _busy.asStateFlow()

    private val _done = MutableStateFlow(false)
    val done: StateFlow<Boolean> = _done.asStateFlow()

    private var reconciled = false

    /** Re-read the session on appear so an account whose onboardingCompletedAt was
     * still null at login self-heals here instead of showing this screen again. */
    fun reconcile() {
        if (reconciled) return
        reconciled = true
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            val completedAt = runCatching { authApi.fetchSession(accountId)?.onboardingCompletedAt }
                .getOrNull() ?: return@launch
            auth.markOnboardingCompleted(completedAt)
            _done.value = true
        }
    }

    /** Continue from the informational screen — marks onboarding complete (like web).
     * Deliberately leaves `busy` set: the `done` flag navigates away, and re-enabling
     * the button first would open a double-submit window. */
    fun finish() {
        if (_busy.value) return
        viewModelScope.launch {
            _busy.value = true
            val accountId = auth.activeAccountId.value
            if (accountId != null) runCatching { onboardingApi.complete(accountId) }
            auth.markOnboardingCompleted(java.time.Instant.now().toString())
            _done.value = true
        }
    }
}
