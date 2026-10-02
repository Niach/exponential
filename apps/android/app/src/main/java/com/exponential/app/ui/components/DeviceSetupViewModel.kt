package com.exponential.app.ui.components

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
import javax.inject.Singleton
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.withTimeoutOrNull

/**
 * EXP-1169 join rule, the same at every invite-accept surface: a caller who
 * owns NO device gets the device setup step after joining; one who owns a
 * machine (or whose own-device state has not resolved yet, null) behaves as
 * before. Loading counts as "owns one" so nobody is trapped on a spinner.
 */
fun needsJoinDeviceStep(ownDevices: List<SteerDevice>?): Boolean = ownDevices?.isEmpty() == true

/**
 * The caller's OWN machines off the synced `devices` shape, exactly as the
 * Devices tab builds them ([steerDeviceFlow]): same rows, same online-ness,
 * same 30s ticker. Teammates' shared servers are filtered out (`isMine`).
 * null means the shape's first snapshot has not landed.
 *
 * ONE source for the [DeviceSetup] block's list and the join rule's one-shot
 * read ([needsJoinDeviceStep]), so the step a joiner sees and the decision to
 * show it can never disagree.
 */
@Singleton
class OwnDevices @Inject constructor(
    private val auth: AuthRepository,
    private val holder: DatabaseHolder,
    private val selection: TeamSelection,
) {
    fun flow(): Flow<List<SteerDevice>?> =
        steerDeviceFlow(accountDatabaseFlow(auth, holder), selection.selectedId, auth.userId)
            .map { rows -> rows?.filter { it.isMine } }

    /** The join rule over the first RESOLVED local value (a null is the
     *  shape's snapshot still landing, not an answer). Bounded, so a join
     *  never hangs on it: a miss reads as "owns one". */
    suspend fun needsJoinDeviceStep(): Boolean =
        needsJoinDeviceStep(withTimeoutOrNull(1_500) { flow().first { it != null } })
}

/**
 * Backs the [DeviceSetup] block (EXP-725, EXP-1169): the own-device list, null
 * while loading, so a cold start shows nothing rather than flashing the empty
 * state at a user who does have one. The hosts that need the list too (the
 * onboarding step's Skip/Continue label) read the same instance.
 */
@HiltViewModel
class DeviceSetupViewModel @Inject constructor(
    auth: AuthRepository,
    ownDevices: OwnDevices,
) : ViewModel() {

    /** The instance the server install snippet points at; null = no copy pill. */
    val instanceOrigin: StateFlow<String?> = auth.instanceUrl

    val devices: StateFlow<List<SteerDevice>?> =
        ownDevices.flow()
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)
}
