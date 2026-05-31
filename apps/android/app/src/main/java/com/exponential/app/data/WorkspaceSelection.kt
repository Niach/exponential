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

    // Set just before a cross-server `auth.switchAccount(id)` from the Home
    // tree when the user taps a project on a different server. After the
    // `key(activeAccountId)` rebuild, AuthenticatedShell's LaunchedEffect
    // reads this and navigates to `project/<id>` on the freshly-built
    // NavHost, then clears the field.
    private val _pendingProjectId = MutableStateFlow<String?>(null)
    val pendingProjectId: StateFlow<String?> = _pendingProjectId.asStateFlow()

    fun setPendingProject(projectId: String) {
        _pendingProjectId.value = projectId
    }

    fun consumePendingProject(): String? {
        val value = _pendingProjectId.value
        _pendingProjectId.value = null
        return value
    }

    // Same idea as pendingProjectId but for Settings → Workspaces taps on a
    // workspace that lives on a different server. The pre-set
    // `selectedId = workspaceId` already drives WorkspaceSettingsViewModel's
    // observers correctly; this flag just tells AuthenticatedShell that a
    // workspace-settings push is expected after the next account switch.
    private val _pendingWorkspaceSettings = MutableStateFlow(false)
    val pendingWorkspaceSettings: StateFlow<Boolean> = _pendingWorkspaceSettings.asStateFlow()

    fun setPendingWorkspaceSettings() {
        _pendingWorkspaceSettings.value = true
    }

    fun consumePendingWorkspaceSettings(): Boolean {
        val value = _pendingWorkspaceSettings.value
        _pendingWorkspaceSettings.value = false
        return value
    }

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

    // One-shot handoff for the share → create flow. MainActivity routes shared
    // content here via the nav layer; the project route consumes it once after
    // navigation lands and pre-fills the create sheet.
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
