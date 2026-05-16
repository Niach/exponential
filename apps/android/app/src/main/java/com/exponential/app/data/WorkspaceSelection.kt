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
}
