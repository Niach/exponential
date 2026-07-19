package com.exponential.app.data

import com.exponential.app.data.auth.SecureStore
import com.exponential.app.data.push.DeepLinkBus
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

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
