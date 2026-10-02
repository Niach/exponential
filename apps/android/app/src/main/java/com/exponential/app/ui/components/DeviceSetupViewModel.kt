package com.exponential.app.ui.components

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.api.DeviceApproval
import com.exponential.app.data.api.DevicesApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.DeviceCodeRules
import com.exponential.app.ui.steer.steerDeviceFlow
import dagger.hilt.android.lifecycle.HiltViewModel
import java.time.Instant
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
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
 * The server card's state (EXP-1169, web `device-setup.tsx` parity): the
 * one-time install [token] (null = the plain command), whether the command was
 * [copied] (only then does the code field exist), and the code approval.
 */
data class ServerCardState(
    val token: String? = null,
    val copied: Boolean = false,
    val code: String = "",
    val busy: Boolean = false,
    val error: String? = null,
    val approved: Boolean = false,
)

/**
 * Backs the [DeviceSetup] block (EXP-725, EXP-1169): the own-device list, null
 * while loading, so a cold start shows nothing rather than flashing the empty
 * state at a user who does have one. The hosts that need the list too (the
 * onboarding step's Skip/Continue label) read the same instance.
 *
 * The server card lives here too, so all three hosts share it: [activate]
 * mints the install token while the block is shown (reminted on expiry, any
 * failure silent), [deactivate] drops it with the rest of the card state.
 */
@HiltViewModel
class DeviceSetupViewModel @Inject constructor(
    private val auth: AuthRepository,
    ownDevices: OwnDevices,
    private val devicesApi: DevicesApi,
    private val authApi: AuthApi,
) : ViewModel() {

    /** The instance the server install snippet points at; null = no command box. */
    val instanceOrigin: StateFlow<String?> = auth.instanceUrl

    private val _server = MutableStateFlow(ServerCardState())
    val server: StateFlow<ServerCardState> = _server.asStateFlow()

    val devices: StateFlow<List<SteerDevice>?> =
        ownDevices.flow()
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private var mintJob: Job? = null
    private var approveJob: Job? = null

    /** The block entered composition: mint, and remint whenever the token lapses. */
    fun activate() {
        mintJob?.cancel()
        mintJob = viewModelScope.launch {
            while (true) {
                val accountId = auth.activeAccountId.value ?: return@launch
                val minted = try {
                    devicesApi.createInstallToken(accountId)
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Exception) {
                    // Network, 429: the plain command stays, no error shown.
                    return@launch
                }
                _server.update { it.copy(token = minted.token) }
                val expiresAt = runCatching { Instant.parse(minted.expiresAt).toEpochMilli() }
                    .getOrNull() ?: return@launch
                delay((expiresAt - System.currentTimeMillis()).coerceAtLeast(0))
                _server.update { it.copy(token = null) }
            }
        }
    }

    /** The block left: drop the token and reset the card. */
    fun deactivate() {
        mintJob?.cancel()
        mintJob = null
        approveJob?.cancel()
        approveJob = null
        _server.value = ServerCardState()
    }

    fun onCopied() {
        _server.update { it.copy(copied = true) }
    }

    fun onCodeChange(input: String) {
        _server.update { it.copy(code = DeviceCodeRules.normalizeUserCode(input), error = null) }
    }

    fun approve() {
        val state = _server.value
        if (state.busy || !DeviceCodeRules.isCompleteUserCode(state.code)) return
        val accountId = auth.activeAccountId.value ?: return
        _server.update { it.copy(busy = true, error = null) }
        approveJob = viewModelScope.launch {
            val outcome = try {
                authApi.approveDeviceCode(accountId, state.code)
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                null
            }
            _server.update {
                when (outcome) {
                    DeviceApproval.Approved -> it.copy(busy = false, approved = true)
                    DeviceApproval.AlreadyUsed -> it.copy(busy = false, error = DeviceSetupCopy.CODE_USED)
                    is DeviceApproval.Refused ->
                        it.copy(busy = false, error = DeviceCodeRules.errorMessage(outcome.error))
                    null -> it.copy(busy = false, error = DeviceSetupCopy.FAILED)
                }
            }
        }
    }
}
