package com.exponential.app.data

import com.exponential.app.data.auth.SecureStore
import com.exponential.app.data.push.DeepLinkBus
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first

// App-wide selected-team state. Hoisted out of HomeViewModel so the
// drawer (rendered above the NavHost) and the board list screen (rendered
// inside the NavHost) read the same value. Both observe the same Room
// tables, so switching here propagates everywhere.
@Singleton
class TeamSelection @Inject constructor(
    private val secureStore: SecureStore,
) {
    private val _selectedId = MutableStateFlow<String?>(null)
    val selectedId: StateFlow<String?> = _selectedId.asStateFlow()

    fun select(id: String) {
        _selectedId.value = id
    }

    // Single write point for a DEFAULT selection (EXP-166/EXP-168): only takes
    // effect while nothing is selected, so explicit switches (Settings →
    // Teams) and the onboarding/create-board selects always win over
    // the app-level bootstrap (AppViewModel.init).
    fun selectIfNull(id: String) {
        if (_selectedId.value == null) _selectedId.value = id
    }

    // Drop the selected team when the active account changes: the selected
    // id belongs to the previous account's DB, so the new account must resolve
    // its own default (AppViewModel's default-team bootstrap re-selects).
    // Without this the global selection leaks a team id across
    // users/servers.
    fun clearSelection() {
        _selectedId.value = null
    }

    // A cross-server team switch (chats.json: the Team tab, a push for another
    // server's team): the account switch clears the selection, so the target
    // team waits here, per account, for AppViewModel's default-team bootstrap
    // to select it (it wins over the last-board default).
    private val pendingTeams = java.util.concurrent.ConcurrentHashMap<String, String>()

    fun selectAfterAccountSwitch(accountId: String, teamId: String) {
        pendingTeams[accountId] = teamId
    }

    fun pendingTeam(accountId: String): String? = pendingTeams[accountId]

    fun clearPendingTeam(accountId: String) {
        pendingTeams.remove(accountId)
    }

    // (The old pendingBoardId / pendingTeamSettings handoff flags are
    // gone: feature ViewModels scope to the active account reactively, so
    // cross-server taps switch the account and navigate immediately — no
    // key(activeAccountId) rebuild to hand state across anymore.)

    // Last board the user opened, persisted per account so the share-target
    // picker can pre-select a sensible default and the Issues tab root can
    // resolve its current board. Account-keyed to avoid pre-selecting a
    // board that belongs to a different server's DB.
    //
    // SecureStore isn't observable, so a version counter lets reactive readers
    // (the Issues tab's current-board resolution) re-read after every write.
    private val _lastBoardVersion = MutableStateFlow(0)
    val lastBoardVersion: StateFlow<Int> = _lastBoardVersion.asStateFlow()

    fun rememberLastBoard(accountId: String, boardId: String) {
        if (accountId.isBlank()) return
        secureStore.set(lastBoardKey(accountId), boardId)
        _lastBoardVersion.value += 1
    }

    fun lastBoard(accountId: String): String? =
        if (accountId.isBlank()) null else secureStore.get(lastBoardKey(accountId))

    private fun lastBoardKey(accountId: String) = "last_board_id_$accountId"

    // Pending shared content for the share → create flow. MainActivity routes
    // shared content here via the nav layer; it lives in this app-singleton (not
    // route state) so backing out of the create screen and re-entering re-fills
    // the form. Consumed exactly once — on a successful create, an explicit
    // discard from the create screen, or cancel from the share picker.
    private val _pendingShare = MutableStateFlow<DeepLinkBus.Target.ShareContent?>(null)
    val pendingShare: StateFlow<DeepLinkBus.Target.ShareContent?> = _pendingShare.asStateFlow()

    fun setPendingShare(share: DeepLinkBus.Target.ShareContent) {
        _pendingShare.value = share
    }

    fun consumePendingShare(): DeepLinkBus.Target.ShareContent? {
        val value = _pendingShare.value
        _pendingShare.value = null
        return value
    }
}

/** A team switch, planned off the target account's boards ([planTeamSwitch]). */
data class TeamSwitch(val accountId: String, val teamId: String, val rememberBoardId: String?)

/**
 * chats.json: switching the ONE active team also moves the Board tab onto
 * that team (iOS `AppNavigator.applyTeam`): the remembered board becomes the
 * team's first board unless it is already one of the team's. Reads the
 * TARGET account's database, so another server's team works too.
 */
suspend fun TeamSelection.planTeamSwitch(
    holder: com.exponential.app.data.db.DatabaseHolder,
    accountId: String,
    teamId: String,
): TeamSwitch {
    val boards = runCatching {
        holder.database(forAccountId = accountId).boardDao().observeAll().first()
    }.getOrDefault(emptyList())
    return TeamSwitch(
        accountId = accountId,
        teamId = teamId,
        rememberBoardId = com.exponential.app.domain.boardToRememberOnTeamSwitch(boards, lastBoard(accountId), teamId),
    )
}

/**
 * Applies a [planTeamSwitch] synchronously. Same server: select, then
 * remember the board. Another server: remember the board and park the team
 * for that account BEFORE the switch, whose selection clear would otherwise
 * let the bootstrap pick that account's last-board team.
 */
fun TeamSelection.applyTeamSwitch(
    switch: TeamSwitch,
    auth: com.exponential.app.data.auth.AuthRepository,
) {
    if (switch.accountId != auth.activeAccountId.value) {
        switch.rememberBoardId?.let { rememberLastBoard(switch.accountId, it) }
        selectAfterAccountSwitch(switch.accountId, switch.teamId)
        auth.switchAccount(switch.accountId)
        return
    }
    if (selectedId.value != switch.teamId) select(switch.teamId)
    switch.rememberBoardId?.let { rememberLastBoard(switch.accountId, it) }
}
