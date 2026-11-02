package com.exponential.app.ui.session

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.data.api.DeviceCommandDto
import com.exponential.app.data.api.DeviceLaunchDefaults
import com.exponential.app.data.api.DevicesApi
import com.exponential.app.data.api.agentLoginCancelCommand
import com.exponential.app.data.api.agentLoginCodeCommand
import com.exponential.app.data.api.agentLoginCommand
import com.exponential.app.data.api.agentUpdateCommand
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.flowOn

// The device-settings sheet's data + mutations (EXP-481): rename, team
// sharing, the server-authoritative launch-defaults edit — all of which work
// with the machine OFFLINE (the row is the truth; the machine converges) —
// and the durable command queue the remote sign-ins ride (EXP-484), whose
// progress is polled off `devices.getCommand`. EXP-1043 removed the worktree
// commands with the sheet's worktree section: worktrees are the IDE's.
//
// EXP-490: name and defaults AUTO-SAVE. Every edit queues its value and the
// mutation fires once the user pauses — each `devices.*` write nudges the
// machine over the relay, so a call per keystroke or per picker tap is not an
// option — with a flush on field blur and on sheet dismiss.

/** One issued device command's UI state, keyed by [DeviceSettingsViewModel.commandStates]. */
sealed interface DeviceCommandUiState {
    data object Sending : DeviceCommandUiState
    /** Queued server-side; the machine is offline and runs it on return. */
    data object Queued : DeviceCommandUiState
    /** Delivered and being worked (pending + machine online). */
    data object Running : DeviceCommandUiState
    data class Done(val message: String?) : DeviceCommandUiState
    data class Failed(val message: String) : DeviceCommandUiState
}

/**
 * WHICH sign-in a login slot belongs to: device × agent × login, the
 * [accountCommandKey] shape; no intended profile (Add account) = `new`. Keyed
 * this finely because a login's poll (up to 90s) outlives its sheet: keyed per
 * agent alone, an abandoned sign-in on one machine landed its link in another
 * machine's sheet.
 */
internal fun agentLoginSlot(
    deviceId: String,
    agent: String,
    profileId: String?,
): String = accountCommandKey(
    deviceId,
    agent,
    profileId?.trim()?.takeIf { it.isNotEmpty() } ?: ADD_ACCOUNT_SLOT,
)

/** [agentLoginSlot]'s login part for an "Add account" sign-in. */
private const val ADD_ACCOUNT_SLOT = "new"

/**
 * One sign-in's command key in [DeviceSettingsViewModel.commandStates]
 * (EXP-484), per [agentLoginSlot] — the prefix is what tells a `Done` result
 * apart from another command's plain-text summary, so its payload is parsed
 * as a login URL.
 */
fun agentLoginCommandKey(
    deviceId: String,
    agent: String,
    profileId: String?,
): String = LOGIN_KEY_PREFIX + agentLoginSlot(deviceId, agent, profileId)

/**
 * The same sign-in's CODE command key (EXP-765). Its own prefix keeps the
 * `login:` parsing honest: a `login-code:` result is plain text, never a login
 * publication.
 */
fun agentLoginCodeCommandKey(
    deviceId: String,
    agent: String,
    profileId: String?,
): String = LOGIN_CODE_KEY_PREFIX + agentLoginSlot(deviceId, agent, profileId)

/** One `agent_update` command's key in [DeviceSettingsViewModel.commandStates] (EXP-1196). */
fun agentUpdateCommandKey(deviceId: String, agent: String): String = "update:$deviceId:$agent"

/** The prefix [agentLoginCommandKey] builds. */
private const val LOGIN_KEY_PREFIX = "login:"

/** The prefix [agentLoginCodeCommandKey] builds — see [DeviceSettingsViewModel.issueCommand]. */
private const val LOGIN_CODE_KEY_PREFIX = "login-code:"

@HiltViewModel
class DeviceSettingsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val devicesApi: DevicesApi,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    // The devices ROW id the sheet is bound to (set from the composable —
    // idempotent, survives recomposition).
    private val boundRowId = MutableStateFlow<String?>(null)

    fun bind(deviceRowId: String?) {
        // The ViewModel outlives one sheet open — a pending edit must land on
        // the machine it was typed for, not on the next one bound here.
        val previous = boundRowId.value
        if (previous != null && previous != deviceRowId) flushPending()
        boundRowId.value = deviceRowId
    }

    /** The caller's teams — one sharing switch each (FEED-33). */
    val teams: StateFlow<List<TeamEntity>> =
        dbFlow.scopedQuery(emptyList<TeamEntity>()) { it.teamDao().observeAll() }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // Per-section busy flags (EXP-323 idiom).
    private val _nameBusy = MutableStateFlow(false)
    val nameBusy: StateFlow<Boolean> = _nameBusy

    private val _shareBusy = MutableStateFlow(false)
    val shareBusy: StateFlow<Boolean> = _shareBusy
    private val _defaultBusy = MutableStateFlow(false)
    val defaultBusy: StateFlow<Boolean> = _defaultBusy

    private val _defaultsBusy = MutableStateFlow(false)
    val defaultsBusy: StateFlow<Boolean> = _defaultsBusy

    // FEED-79: a failed write or device command (rename, defaults, sharing,
    // the default flag, update / sign-in) is ONE error toast. Several
    // surfaces may watch one instance, so a host [claimFailure]s it first.
    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure

    /** True for the ONE caller that takes [failure] (it then toasts it). */
    fun claimFailure(failure: FailedAction): Boolean = _failure.compareAndSet(failure, null)

    private fun fail(verb: String, thing: String, t: Throwable, retry: (() -> Unit)? = null) {
        _failure.value = FailedAction(verb, thing, trpcErrorMessage(t, ""), retry = retry)
    }

    // Command progress, keyed by the issuing slot ([agentLoginCommandKey]);
    // a terminal state stays until the next command on the same key.
    private val _commandStates = MutableStateFlow<Map<String, DeviceCommandUiState>>(emptyMap())
    val commandStates: StateFlow<Map<String, DeviceCommandUiState>> = _commandStates

    // The pending auto-saves: deviceId to the value the user last left behind.
    // Nulled only by a successful save (reference-identity compareAndSet, so a
    // newer edit made mid-flight survives) — a FAILED save keeps its input so
    // the blur/dismiss flush retries it instead of dropping the edit.
    private val renameInput = MutableStateFlow<Pair<String, String>?>(null)
    private val defaultsInput = MutableStateFlow<Pair<String, DeviceLaunchDefaults>?>(null)

    // What [flushPending] already wrote. Clearing the input does not cancel a
    // debounce window that is still counting down, so without this the timer
    // fires afterwards and repeats the call (and its relay nudge).
    private var flushedRename: Pair<String, String>? = null
    private var flushedDefaults: Pair<String, DeviceLaunchDefaults>? = null

    @OptIn(FlowPreview::class)
    private fun collectAutoSaves() {
        viewModelScope.launch {
            renameInput.filterNotNull().debounce(AUTOSAVE_DEBOUNCE_MS)
                .collect { if (it !== flushedRename) saveRenameNow(it) }
        }
        viewModelScope.launch {
            defaultsInput.filterNotNull().debounce(AUTOSAVE_DEBOUNCE_MS)
                .collect { if (it !== flushedDefaults) saveDefaultsNow(it) }
        }
    }

    init {
        collectAutoSaves()
    }

    /** Queue a rename; null (blank or unchanged input) drops the pending one. */
    fun queueRename(deviceId: String, label: String?) {
        renameInput.value = label?.let { deviceId to it }
    }

    fun queueDefaults(deviceId: String, defaults: DeviceLaunchDefaults) {
        defaultsInput.value = deviceId to defaults
    }

    fun hasPendingRename(): Boolean = renameInput.value != null || nameBusy.value

    fun hasPendingDefaults(): Boolean = defaultsInput.value != null || defaultsBusy.value

    /**
     * Save what is queued right now, on a process-lifetime scope: this fires
     * from the sheet's dispose, and a dismiss-then-navigate-away must not
     * cancel the final write mid-request.
     */
    fun flushPending() {
        val rename = renameInput.value
        val defaults = defaultsInput.value
        flushedRename = rename
        flushedDefaults = defaults
        renameInput.value = null
        defaultsInput.value = null
        if (rename != null) deviceSettingsFlushScope.launch { saveRenameNow(rename) }
        if (defaults != null) deviceSettingsFlushScope.launch { saveDefaultsNow(defaults) }
    }

    private suspend fun saveRenameNow(pending: Pair<String, String>) {
        val accountId = auth.activeAccountId.value ?: return
        _nameBusy.value = true
        runCatching { devicesApi.rename(accountId, pending.first, pending.second) }
            .fold(
                onSuccess = { renameInput.compareAndSet(pending, null) },
                onFailure = { t ->
                    if (t is CancellationException) throw t
                    fail("rename", "the device", t) { viewModelScope.launch { saveRenameNow(pending) } }
                },
            )
        _nameBusy.value = false
    }

    private suspend fun saveDefaultsNow(pending: Pair<String, DeviceLaunchDefaults>) {
        val accountId = auth.activeAccountId.value ?: return
        _defaultsBusy.value = true
        runCatching { devicesApi.setLaunchDefaults(accountId, pending.first, pending.second) }
            .fold(
                onSuccess = { defaultsInput.compareAndSet(pending, null) },
                onFailure = { t ->
                    if (t is CancellationException) throw t
                    fail("save", "the agent defaults", t) { viewModelScope.launch { saveDefaultsNow(pending) } }
                },
            )
        _defaultsBusy.value = false
    }

    /** FEED-33: toggle ONE team in or out of the machine's share set. */
    fun setShared(deviceId: String, teamId: String, shared: Boolean) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _shareBusy.value = true
            runCatching { devicesApi.setShared(accountId, deviceId, teamId, shared) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    fail("update", "sharing", t) { setShared(deviceId, teamId, shared) }
                }
            _shareBusy.value = false
        }
    }

    /**
     * EXP-622: flag/unflag this machine as the caller's default — the row every
     * device picker prefills. Written straight through (a single toggle, no
     * debounce); the server clears the flag on the caller's other machines and
     * the switch re-renders off the synced row.
     */
    fun setDefault(deviceId: String, isDefault: Boolean) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _defaultBusy.value = true
            runCatching { devicesApi.setDefault(accountId, deviceId, isDefault) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    fail("update", "the default device", t) { setDefault(deviceId, isDefault) }
                }
            _defaultBusy.value = false
        }
    }

    /**
     * EXP-484: ask the machine to run [agent]'s own sign-in flow and publish
     * the login URL (plus codex's device code) back as the command result —
     * no credential ever travels. [switchAccount] signs the current account
     * out first (which, for codex, revokes the token server-side — the sheet
     * confirms before calling). Same durable queue as every other command,
     * but only offered for an ONLINE machine that advertises the cap: a login
     * link parked until tomorrow would be expired anyway.
     */
    fun agentLogin(
        deviceId: String,
        agent: String,
        switchAccount: Boolean,
        deviceOnline: Boolean,
        /**
         * The login this sign-in is FOR — one of `agentAccounts[agent].profiles`;
         * null = "Add account". The device lands it on the profile its email
         * names either way; this only tells a duplicate apart. The command KEY
         * is per device × agent × login ([agentLoginCommandKey]), so an
         * abandoned sign-in's late result never lands in another sheet.
         */
        profileId: String? = null,
        /** EXP-1533: a long-lived claude login (`claude setup-token`). */
        longLived: Boolean = false,
        /** EXP-1626: the name a long-lived ADD gives its new profile. */
        name: String? = null,
    ) {
        val key = agentLoginCommandKey(deviceId, agent, profileId)
        // EXP-1626: a re-queue of the same sign-in (the mode toggle, the
        // refused-Add fallback, a Retry) replaces the run before it: stop
        // following it, and stop it on the machine unless it already failed.
        val run = LoginRun()
        loginRuns.put(key, run)?.let { stopLoginRun(it, deviceId, agent) }
        run.job = issueCommand(
            key = key,
            command = agentLoginCommand(deviceId, agent, switchAccount, profileId, longLived, name),
            deviceOnline = deviceOnline,
            verb = "start",
            thing = "the sign-in",
            // The toast's Retry re-queues the SAME login (long-lived included).
            retry = { agentLogin(deviceId, agent, switchAccount, deviceOnline, profileId, longLived, name) },
            // EXP-1600: an older device refusing a long-lived Add is no failure
            // to toast: the sign-in sheet re-queues it as the browser login.
            quiet = { longLived && profileId == null && isLongLivedRefused(it) },
            // EXP-1626: the link + code of a slow machine's long-lived sign-in
            // can take minutes; follow it as long as the machine itself would.
            pollDeadlineMs = LOGIN_POLL_DEADLINE_MS,
            onCreated = { id ->
                run.commandId = id
                // Closed while the row was still being queued: stop it now.
                if (run.stopped) {
                    sendLoginCancel(deviceId, agent, id)
                    throw CancellationException("sign-in closed")
                }
            },
            onState = { if (it is DeviceCommandUiState.Failed) run.failed = true },
        )
    }

    /** EXP-1626: one queued `agent_login` — what a cancel names, and its poll. */
    private class LoginRun {
        @Volatile var commandId: String? = null
        @Volatile var stopped = false
        @Volatile var failed = false
        @Volatile var job: Job? = null
    }

    // EXP-1626: the live sign-in per slot ([agentLoginCommandKey]).
    private val loginRuns = java.util.concurrent.ConcurrentHashMap<String, LoginRun>()

    /**
     * EXP-1626: the sign-in sheet closed. [landed] = the login committed on
     * the device (nothing to stop); otherwise, unless the login already
     * failed, queue `agent_login_cancel` so the machine stops that run (its
     * `claude setup-token` child and its staging) instead of holding it for
     * its 10-minute bound. Fire-and-forget on a process-lifetime scope: the
     * dismiss may clear this ViewModel mid-request.
     */
    fun closeAgentLogin(deviceId: String, agent: String, profileId: String?, landed: Boolean) {
        val run = loginRuns.remove(agentLoginCommandKey(deviceId, agent, profileId)) ?: return
        if (landed) {
            run.job?.cancel()
            return
        }
        stopLoginRun(run, deviceId, agent)
    }

    /** Stop following [run] and, unless it failed, cancel it on the machine. */
    private fun stopLoginRun(run: LoginRun, deviceId: String, agent: String) {
        run.stopped = true
        // Not queued yet: the run cancels itself once the server names its row.
        val commandId = run.commandId ?: return
        run.job?.cancel()
        if (!run.failed) sendLoginCancel(deviceId, agent, commandId)
    }

    private fun sendLoginCancel(deviceId: String, agent: String, loginCommandId: String) {
        val accountId = auth.activeAccountId.value ?: return
        deviceSettingsFlushScope.launch {
            runCatching {
                devicesApi.createCommand(accountId, agentLoginCancelCommand(deviceId, agent, loginCommandId))
            }
        }
    }

    /**
     * EXP-765: the return path for claude's sign-in — the browser shows an
     * authorization code, and this hands it to the login still waiting on the
     * machine, which types it into its prompt. Same durable queue as every
     * other command; a Done here also retires the login link (see
     * [issueCommand]), because the sign-in it belonged to is over.
     */
    fun agentLoginCode(
        deviceId: String,
        agent: String,
        code: String,
        deviceOnline: Boolean,
        /** The sign-in this code belongs to — the same login [agentLogin] targeted. */
        profileId: String? = null,
    ) {
        issueCommand(
            key = agentLoginCodeCommandKey(deviceId, agent, profileId),
            command = agentLoginCodeCommand(deviceId, agent, code),
            deviceOnline = deviceOnline,
            verb = "send",
            thing = "the code",
            pollDeadlineMs = LOGIN_POLL_DEADLINE_MS,
        )
    }

    /**
     * EXP-1196/1218: the readiness block's `update` action — the machine runs
     * [agent]'s own self-updater (`agent_update`). Keyed per device × agent
     * ([agentUpdateCommandKey]); the doctor re-reports the new version on the
     * next heartbeat, so a Done needs no caption of its own.
     */
    fun agentUpdate(deviceId: String, agent: String, deviceOnline: Boolean) {
        issueCommand(
            key = agentUpdateCommandKey(deviceId, agent),
            command = agentUpdateCommand(deviceId, agent),
            deviceOnline = deviceOnline,
            verb = "update",
            thing = "the agent",
            retry = { agentUpdate(deviceId, agent, deviceOnline) },
        )
    }

    // Queue the command and follow it to a terminal state on the shared
    // [runDeviceCommand] rails. Everything this adds is the keyed slot and
    // the Done bookkeeping.
    // A Failed command is also the screen's ONE error toast ([verb] [thing],
    // the device's or server's message), FEED-79.
    private fun issueCommand(
        key: String,
        command: kotlinx.serialization.json.JsonObject,
        deviceOnline: Boolean,
        verb: String,
        thing: String,
        retry: (() -> Unit)? = null,
        /** A failure the caller handles itself: no toast. */
        quiet: (String) -> Boolean = { false },
        pollDeadlineMs: Long = COMMAND_POLL_DEADLINE_MS,
        onCreated: (String) -> Unit = {},
        /** Every transition, before it lands in [commandStates]. */
        onState: (DeviceCommandUiState) -> Unit = {},
    ): Job =
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runDeviceCommand(
                devicesApi,
                accountId,
                command,
                deviceOnline,
                pollDeadlineMs = pollDeadlineMs,
                onCreated = onCreated,
            ) { state ->
                onState(state)
                if (state is DeviceCommandUiState.Failed && !quiet(state.message)) _failure.value = FailedAction(verb, thing, state.message, retry = retry)
                if (state is DeviceCommandUiState.Done) {
                    markDone(key, state.message)
                } else {
                    _commandStates.value = _commandStates.value + (key to state)
                }
            }
        }

    /**
     * Land a command's Done state. EXP-765: a completed `login-code:` also
     * retires that sign-in's `login:` state — the machine has the code, so the
     * link it published belongs to a sign-in that is finishing, and leaving it
     * on screen would invite a second, stale attempt. EXP-1626: only a Done
     * does: a FAILED code (the machine rejected it) leaves the link up for
     * another paste, and the failure is the toast.
     */
    private fun markDone(key: String, message: String?) {
        var next = _commandStates.value + (key to DeviceCommandUiState.Done(message))
        if (key.startsWith(LOGIN_CODE_KEY_PREFIX)) {
            next = next - (LOGIN_KEY_PREFIX + key.removePrefix(LOGIN_CODE_KEY_PREFIX))
        }
        _commandStates.value = next
    }
}

/**
 * EXP-849: issue ONE device command and follow it to a terminal state — the
 * same rules [DeviceSettingsViewModel] applies to its own queue, shared so the
 * Accounts section can run a remote sign-in without a second copy of the poll
 * loop. [onState] receives every transition. An OFFLINE machine stops at
 * [DeviceCommandUiState.Queued] (the row is durable, the machine runs it on
 * return) and so does a poll that outlives the deadline — a pending row is not
 * a failure.
 */
internal suspend fun runDeviceCommand(
    devicesApi: DevicesApi,
    accountId: String,
    command: kotlinx.serialization.json.JsonObject,
    deviceOnline: Boolean,
    /** How long an online machine's command is followed before it reads Queued. */
    pollDeadlineMs: Long = COMMAND_POLL_DEADLINE_MS,
    /** The queued row's id, as soon as the server returns it (EXP-1626: a cancel names it). */
    onCreated: (String) -> Unit = {},
    onState: (DeviceCommandUiState) -> Unit,
) {
    onState(DeviceCommandUiState.Sending)
    val created = runCatching { devicesApi.createCommand(accountId, command) }
        .getOrElse { t ->
            if (t is CancellationException) throw t
            onState(
                DeviceCommandUiState.Failed(
                    trpcErrorMessage(t, "The command could not be queued"),
                ),
            )
            return
        }
    onCreated(created.id)
    if (!deviceOnline) {
        onState(DeviceCommandUiState.Queued)
        return
    }
    onState(DeviceCommandUiState.Running)
    val deadline = System.currentTimeMillis() + pollDeadlineMs
    while (System.currentTimeMillis() < deadline) {
        delay(COMMAND_POLL_INTERVAL_MS)
        val row = runCatching { devicesApi.getCommand(accountId, created.id) }
            .getOrNull() ?: continue
        when (row.status) {
            DeviceCommandDto.STATUS_DONE -> {
                onState(DeviceCommandUiState.Done(row.result))
                return
            }
            DeviceCommandDto.STATUS_FAILED -> {
                onState(
                    DeviceCommandUiState.Failed(
                        row.result ?: "The device refused the command",
                    ),
                )
                return
            }
        }
    }
    onState(DeviceCommandUiState.Queued)
}

// Auto-saves fired while the sheet is closing must outlive the ViewModel:
// viewModelScope is cancelled when navigation clears it, which could abort the
// final flush mid-request. Process-lifetime, mirroring descriptionFlushScope.
private val deviceSettingsFlushScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

/** How long an edit rests before it is written (one relay nudge per write). */
private const val AUTOSAVE_DEBOUNCE_MS = 800L

private const val COMMAND_POLL_INTERVAL_MS = 2_000L
private const val COMMAND_POLL_DEADLINE_MS = 90_000L

/**
 * EXP-1626: `agent_login` / `agent_login_code` are followed for as long as the
 * machine keeps a sign-in run alive (it stops one nobody finishes after 10
 * minutes), so a slow machine's link never reads as a stale Queued.
 */
private const val LOGIN_POLL_DEADLINE_MS = 10 * 60_000L
