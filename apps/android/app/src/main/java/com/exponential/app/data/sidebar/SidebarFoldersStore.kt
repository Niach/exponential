package com.exponential.app.data.sidebar

import android.content.Context
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.preferencesDataStore
import com.exponential.app.data.api.SidebarLayoutApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.SidebarLayoutEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.SidebarFolders
import com.exponential.app.domain.SidebarTree
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
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull

private val Context.sidebarFolderDataStore by preferencesDataStore(name = "sidebar_folders")

/**
 * #EXP-1832 (fixtures/sidebar-folders.json): the member's sidebar layout: the
 * synced `sidebar_layout` rows of a team + section ([entries]) and the
 * expanded flags, LOCAL to this device (DataStore, keyed by
 * `SidebarFolders.openKey` / `SidebarTree.teamOpenKey`; no stored flag = open).
 * #EXP-2109: the Teams tab's menus WRITE it ([save], [rename], [remove]): the
 * result shows at once ([teamEntries]'s overlay) until the synced rows carry
 * it; a refused write throws and the overlay goes.
 */
@Singleton
class SidebarFoldersStore @Inject constructor(
    @ApplicationContext private val context: Context,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val api: SidebarLayoutApi,
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

    private class Pending(
        val teamId: String,
        val apply: (List<SidebarFolders.Entry>) -> List<SidebarFolders.Entry>,
        val shown: (List<SidebarFolders.Entry>) -> Boolean,
    )

    private val pending = MutableStateFlow(emptyList<Pending>())

    /** One write at a time: a rename never overtakes its folder's save. */
    private val writes = Mutex()

    private val syncedByTeam: Flow<Map<String, List<SidebarFolders.Entry>>> =
        dbFlow.scopedQuery(emptyList<SidebarLayoutEntity>()) { it.sidebarLayoutDao().observeSection(SidebarTree.SECTION) }
            .map { rows -> rows.groupBy({ it.teamId }, SidebarLayoutEntity::toEntry) }

    /** Every team's mixed list ([SidebarTree.SECTION]), the writes in flight applied. */
    val teamEntries: Flow<Map<String, List<SidebarFolders.Entry>>> = combine(syncedByTeam, pending) { synced, ops ->
        ops.fold(synced) { all, op -> all + (op.teamId to op.apply(all[op.teamId].orEmpty())) }
    }

    private suspend fun write(op: Pending, send: suspend (accountId: String) -> Unit) {
        val accountId = auth.activeAccountId.value ?: return
        pending.update { it + op }
        try {
            writes.withLock { send(accountId) }
            // The synced rows take over; a lost sync drops the overlay anyway.
            withTimeoutOrNull(SYNC_WAIT_MS) { syncedByTeam.first { op.shown(it[op.teamId].orEmpty()) } }
        } finally {
            pending.update { it - op }
        }
    }

    /** `sidebarMove` / `sidebarUngroup` / a new folder's rows. */
    suspend fun save(teamId: String, rows: SidebarFolders.Writes) {
        if (rows.upserts.isEmpty() && rows.deletes.isEmpty()) return
        write(Pending(teamId, { SidebarFolders.applyWrites(it, rows) }, { SidebarFolders.shows(it, rows) })) { account ->
            api.save(account, teamId, SidebarTree.SECTION, rows)
        }
    }

    suspend fun rename(teamId: String, folderId: String, name: String) {
        val saved = SidebarFolders.folderName(name)
        write(
            Pending(
                teamId,
                { rows -> rows.map { if (it.id == folderId) it.copy(name = saved) else it } },
                { rows -> rows.any { it.id == folderId && it.name == saved } },
            ),
        ) { account -> api.rename(account, folderId, saved) }
    }

    /** Remove from sidebar: the target's entry goes. */
    suspend fun remove(teamId: String, targetId: String) {
        write(
            Pending(teamId, { rows -> rows.filter { it.targetId != targetId } }, { rows -> rows.none { it.targetId == targetId } }),
        ) { account -> api.remove(account, targetId) }
    }

    /** Folds / unfolds one folder on this device. */
    fun toggle(teamId: String, folderId: String) = flip(SidebarFolders.openKey(teamId, folderId))

    /** Folds / unfolds one team on this device. */
    fun toggleTeam(teamId: String) = flip(SidebarTree.teamOpenKey(teamId))

    fun openTeam(teamId: String) {
        scope.launch {
            context.sidebarFolderDataStore.edit { it[booleanPreferencesKey(SidebarTree.teamOpenKey(teamId))] = true }
        }
    }

    private fun flip(name: String) {
        val key = booleanPreferencesKey(name)
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

fun Map<String, Boolean>.isTeamOpen(teamId: String): Boolean = this[SidebarTree.teamOpenKey(teamId)] ?: true

private const val SYNC_WAIT_MS = 5_000L
