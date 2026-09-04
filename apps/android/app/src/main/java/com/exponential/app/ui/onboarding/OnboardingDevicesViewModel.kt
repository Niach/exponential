package com.exponential.app.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.ui.steer.steerDeviceFlow
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

/**
 * The wizard's devices step (EXP-725): the caller's OWN machines off the
 * synced `devices` shape, exactly as the Agents tab builds them
 * ([steerDeviceFlow]) — same rows, same online-ness, same 30s ticker.
 *
 * Teammates' shared servers are filtered out (`isMine`): the step's question
 * is "have YOU connected a machine", and the Continue button gates on that.
 * null means the shape's first snapshot has not landed, so a cold start shows
 * nothing rather than flashing the empty state at a user who does have one.
 */
@HiltViewModel
class OnboardingDevicesViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
    selection: TeamSelection,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val devices: StateFlow<List<SteerDevice>?> =
        steerDeviceFlow(dbFlow, selection.selectedId, auth.userId)
            .map { rows -> rows?.filter { it.isMine } }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)
}
