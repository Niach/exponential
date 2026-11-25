package com.exponential.app.data

import com.exponential.app.data.auth.SecureStore
import com.exponential.app.data.push.DeepLinkBus
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

// What the shell remembers across screens: the last-used board per account
// (the New issue page's default) and a share on its way into the app. There is
// NO active team (#EXP-2109): a screen's team is derived from what it shows.
@Singleton
class ShellState @Inject constructor(
    private val secureStore: SecureStore,
) {
    // Last board the user opened, persisted per account so the share-target
    // picker and the New issue page can pre-select a sensible default. Account-keyed to avoid pre-selecting a
    // board that belongs to a different server's DB.
    //
    // SecureStore isn't observable, so a version counter lets reactive readers
    // (the New issue page's board resolution) re-read after every write.
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
        _shareTeam.value = null
        _pendingShare.value = share
    }

    // The team picked on Share to for the pending share (null = its default,
    // the last-used board's team, else the first).
    private val _shareTeam = MutableStateFlow<String?>(null)
    val shareTeam: StateFlow<String?> = _shareTeam.asStateFlow()

    fun setShareTeam(teamId: String) {
        _shareTeam.value = teamId
    }

    fun consumePendingShare(): DeepLinkBus.Target.ShareContent? {
        val value = _pendingShare.value
        _pendingShare.value = null
        return value
    }

    // EXP-1635: a share headed for a composer (share-targets.json step 2): the
    // new chat page's, or one chat's. The composer takes it once, as a prefill;
    // nothing is sent until the viewer taps Send.
    private val _pendingChatShare = MutableStateFlow<ChatShare?>(null)
    val pendingChatShare: StateFlow<ChatShare?> = _pendingChatShare.asStateFlow()

    fun setPendingChatShare(share: ChatShare) {
        _pendingChatShare.value = share
    }

    /** The waiting share when [share] is still it (a composer takes its own once). */
    fun takePendingChatShare(share: ChatShare): Boolean =
        _pendingChatShare.compareAndSet(share, null)
}

/** Where a sharing shortcut leads: its team, then an agent pick or a chat. */
data class ChatShareRoute(val teamId: String, val pick: String? = null, val sessionId: String? = null)

/** A share on its way into a composer. */
data class ChatShare(
    val share: DeepLinkBus.Target.ShareContent,
    /** The new chat page's agent row pick (an agent id or `temporary`); null = a chat's composer. */
    val pick: String? = null,
    /** The chat whose composer takes it; null = the new chat page's. */
    val sessionId: String? = null,
    /** The team Share to was on (a plain chat starts there). */
    val teamId: String? = null,
)
