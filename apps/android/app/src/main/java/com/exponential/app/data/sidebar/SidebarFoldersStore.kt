package com.exponential.app.data.sidebar

import android.content.Context
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.preferencesDataStore
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.SidebarLayoutEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.SidebarFolders
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

private val Context.sidebarFolderDataStore by preferencesDataStore(name = "sidebar_folders")

/**
 * #EXP-1832 (fixtures/sidebar-folders.json): the phone's READ-ONLY view of the
 * member's sidebar layout. The synced `sidebar_layout` rows of a team +
 * section ([entries]) and the folders' expanded flags, LOCAL to this device
 * (DataStore, keyed by `SidebarFolders.openKey`; no stored flag = open). The
 * phone never writes the layout (`sidebarLayout.save` is the IDE's and web's).
 */
@Singleton
class SidebarFoldersStore @Inject constructor(
    @ApplicationContext private val context: Context,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** Every stored expanded flag, read eagerly so a sheet opens folded right. */
    val openFlags: StateFlow<Map<String, Boolean>> = context.sidebarFolderDataStore.data
        .map { prefs -> prefs.asMap().entries.mapNotNull { (key, value) -> (value as? Boolean)?.let { key.name to it } }.toMap() }
        .stateIn(scope, SharingStarted.Eagerly, emptyMap())

    /** The section's layout rows in [teamId]. */
    fun entries(teamId: String, section: String): Flow<List<SidebarFolders.Entry>> =
        dbFlow.scopedQuery(emptyList<SidebarLayoutEntity>()) { it.sidebarLayoutDao().observe(teamId, section) }
            .map { rows -> rows.map(SidebarLayoutEntity::toEntry) }

    /** Folds / unfolds one folder on this device. */
    fun toggle(teamId: String, folderId: String) {
        val key = booleanPreferencesKey(SidebarFolders.openKey(teamId, folderId))
        scope.launch {
            context.sidebarFolderDataStore.edit { it[key] = !(it[key] ?: true) }
        }
    }

    @EntryPoint
    @InstallIn(SingletonComponent::class)
    interface Access {
        fun sidebarFoldersStore(): SidebarFoldersStore
    }
}

/** No stored flag = open. */
fun Map<String, Boolean>.isFolderOpen(teamId: String, folderId: String): Boolean =
    this[SidebarFolders.openKey(teamId, folderId)] ?: true
