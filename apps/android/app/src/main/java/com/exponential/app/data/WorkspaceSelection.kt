package com.exponential.app.data

import com.exponential.app.data.auth.SecureStore
import com.exponential.app.data.push.DeepLinkBus
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

// App-wide selected-workspace state. Hoisted out of HomeViewModel so the
// drawer (rendered above the NavHost) and the project list screen (rendered
// inside the NavHost) read the same value. Both observe the same Room
// tables, so switching here propagates everywhere.
@Singleton
class WorkspaceSelection @Inject constructor(
    private val secureStore: SecureStore,
) {
    private val _selectedId = MutableStateFlow<String?>(null)
    val selectedId: StateFlow<String?> = _selectedId.asStateFlow()

    fun select(id: String) {
        _selectedId.value = id
    }

    // (The old pendingProjectId / pendingWorkspaceSettings handoff flags are
    // gone: feature ViewModels scope to the active account reactively, so
    // cross-server taps switch the account and navigate immediately — no
    // key(activeAccountId) rebuild to hand state across anymore.)

    // Last project the user opened, persisted per account so the share-target
    // picker can pre-select a sensible default. Account-keyed to avoid
    // pre-selecting a project that belongs to a different server's DB.
    fun rememberLastProject(accountId: String, projectId: String) {
        if (accountId.isBlank()) return
        secureStore.set(lastProjectKey(accountId), projectId)
    }

    fun lastProject(accountId: String): String? =
        if (accountId.isBlank()) null else secureStore.get(lastProjectKey(accountId))

    private fun lastProjectKey(accountId: String) = "last_project_id_$accountId"

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
