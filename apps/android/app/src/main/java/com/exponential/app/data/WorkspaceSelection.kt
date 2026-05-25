package com.exponential.app.data

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
class WorkspaceSelection @Inject constructor() {
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
}
