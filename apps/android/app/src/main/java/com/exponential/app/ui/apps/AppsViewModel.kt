package com.exponential.app.ui.apps

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.AppCreateInput
import com.exponential.app.data.api.AppUpdateInput
import com.exponential.app.data.api.AppsApi
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.api.isOfflineError
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.AppEntity
import com.exponential.app.data.db.AppRecordEntity
import com.exponential.app.data.db.AppVersionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.AppActions
import com.exponential.app.domain.AppExport
import com.exponential.app.domain.AppRecords
import com.exponential.app.domain.Apps
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.LaunchDeviceRules
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.steer.SessionSender
import com.exponential.app.ui.steer.steerDeviceFlow
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import at.exponential.ui.json.JsonValue
import kotlinx.coroutines.flow.flowOn

// VAPP-110 (fixtures/apps.json, web `hooks/use-apps.ts`): the SELECTED team's
// APPS off the synced `apps` shape (trashed rows hidden; Exponential ships no
// built-in apps), their versions off `app_versions`, and the member's
// EDITOR SESSION for one (a root with `app_id` + `user_id` = me).

/** A list entry: one synced app. */
data class AppListEntry(
    val id: String,
    override val slug: String,
    override val name: String,
    val icon: String?,
    val color: String?,
    val description: String,
    val headVersion: Int,
    val createdBy: String?,
    val teamId: String?,
    /** eApps V1 (#VAPP-210): the PUBLISHED package (JSON text); null = never published. */
    val packageJson: String?,
    /** eApps V1: the published version (0 = never published). */
    val publishedVersion: Int = 0,
    /** eApps V1: the head's package while it is ahead of the published one. */
    val draftPackageJson: String? = null,
) : Apps.ListLike {
    /** eApps V1: the two pointers (`Apps.shownVersion`, `listPill`, …). */
    val versions: Apps.Versions get() = Apps.Versions(headVersion, publishedVersion)

    /** eApps V1: the HEAD's package (the draft, else the published one). */
    val headPackageJson: String? get() = draftPackageJson ?: packageJson
}

fun AppEntity.toListEntry() = AppListEntry(
    id = id,
    slug = slug,
    name = name,
    icon = icon,
    color = color,
    description = description,
    headVersion = headVersion ?: 0,
    createdBy = createdBy,
    teamId = teamId,
    packageJson = packageJson?.takeIf { it.isNotBlank() && it != "null" },
    publishedVersion = publishedVersion ?: 0,
    draftPackageJson = draftPackageJson?.takeIf { it.isNotBlank() && it != "null" },
)

private fun Flow<ExponentialDatabase?>.isOwnerOf(team: Flow<String?>, userId: Flow<String?>): Flow<Boolean> =
    combine(this, team) { db, t -> db to t }
        .flatMapLatest { (db, t) -> if (db == null || t == null) flowOf(emptyList()) else db.teamMemberDao().observeByTeam(t) }
        .combine(userId) { members, me -> me != null && members.firstOrNull { it.userId == me }?.role == DomainContract.teamRoleOwner }

/** VAPP-187: the viewer's role in [team] (null = not a member / signed out),
 *  what `AppRecords.canChangeRecord` reads. */
private fun Flow<ExponentialDatabase?>.roleOf(team: Flow<String?>, userId: Flow<String?>): Flow<String?> =
    combine(this, team) { db, t -> db to t }
        .flatMapLatest { (db, t) -> if (db == null || t == null) flowOf(emptyList()) else db.teamMemberDao().observeByTeam(t) }
        .combine(userId) { members, me -> if (me == null) null else members.firstOrNull { it.userId == me }?.role }

/** VAPP-188: [team]'s member ids (the owner's Author picker). */
private fun Flow<ExponentialDatabase?>.memberIdsOf(team: Flow<String?>): Flow<List<String>> =
    combine(this, team) { db, t -> db to t }
        .flatMapLatest { (db, t) -> if (db == null || t == null) flowOf(emptyList()) else db.teamMemberDao().observeByTeam(t) }
        .map { members -> members.map { it.userId }.distinct() }

/** The Apps list + the New app / Import / Delete writes. */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class AppsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val selection: TeamSelection,
    private val api: AppsApi,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The selected team's apps by name (null = loading). */
    val apps: StateFlow<List<AppListEntry>?> = combine(dbFlow, selection.selectedId) { db, team -> db to team }
        .flatMapLatest { (db, team) ->
            if (db == null || team == null) flowOf(null) else db.appDao().observeByTeam(team).map { rows -> rows }
        }
        .map { rows -> rows?.let { Apps.sortApps(it.map(AppEntity::toListEntry)) } }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    val me: StateFlow<String?> = auth.userId

    val isOwner: StateFlow<Boolean> = dbFlow.isOwnerOf(selection.selectedId, auth.userId)
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    private val _busy = MutableStateFlow(false)
    val busy: StateFlow<Boolean> = _busy

    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    /** Members may manage their own apps, owners every app. */
    fun canManage(app: AppListEntry, me: String?, owner: Boolean): Boolean =
        owner || (me != null && app.createdBy == me)

    /** VAPP-176: New app → `apps.create {teamId}` = a DRAFT ("Untitled
     *  app"); [onCreated] opens its editor at once (no dialog). */
    fun create(onCreated: (String) -> Unit) {
        val team = selection.selectedId.value ?: return
        write("create", { create(onCreated) }) { accountId ->
            onCreated(api.create(accountId, AppCreateInput(teamId = team)).id)
        }
    }

    fun delete(app: AppListEntry, onDone: () -> Unit): Unit =
        write("delete", { delete(app, onDone) }) { accountId ->
            api.delete(accountId, app.id)
            onDone()
        }

    /** VAPP-121: app [appId]'s synced records (the export's switch + rows). */
    fun records(appId: String): Flow<List<AppRecordEntity>> =
        dbFlow.scopedQuery(emptyList<AppRecordEntity>()) { it.appRecordDao().observeByApp(appId) }

    /** VAPP-121: the server's issues with [text] (`apps.validateImport`);
     *  null = the check itself failed (the step keeps Import disabled). */
    suspend fun validateImport(text: String): List<AppExport.Issue>? {
        val team = selection.selectedId.value ?: return null
        val accountId = auth.activeAccountId.value ?: return null
        return try {
            api.validateImport(accountId, team, text).issues.map { AppExport.Issue(it.path, it.message) }
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            null
        }
    }

    /** VAPP-121: Import → `apps.import`; [onCreated] opens the new app's editor. */
    fun importApp(text: String, onCreated: (String) -> Unit) {
        val team = selection.selectedId.value ?: return
        write("import", { importApp(text, onCreated) }) { accountId ->
            onCreated(api.importApp(accountId, team, text).app.id)
        }
    }

    private fun write(verb: String, retry: () -> Unit, call: suspend (accountId: String) -> Unit) {
        if (_busy.value) return
        val accountId = auth.activeAccountId.value ?: return
        _busy.value = true
        viewModelScope.launch {
            try {
                call(accountId)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction(verb, "the app", trpcErrorMessage(t, ""), retry = retry)
            } finally {
                _busy.value = false
            }
        }
    }
}

/** A picked older version: its package loads on demand. */
data class AppViewing(val number: Int, val packageJson: String?)

/** One app's EDITOR (`app/{appId}`): the app, its versions, my editor session. */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class AppEditorViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val api: AppsApi,
    private val sessionsApi: SessionsApi,
    private val sender: SessionSender,
    private val accessApi: com.exponential.app.data.api.McpSettingsApi,
) : ViewModel() {

    val appId: String = savedStateHandle["appId"] ?: ""

    /** eApps R3: my app access (`appAccess.mine`; null = write until known):
     *  the Data tab disables what the server would refuse. */
    val myAccess: StateFlow<String?> = kotlinx.coroutines.flow.flow<String?> {
        val accountId = auth.activeAccountId.value ?: return@flow
        emit(runCatching { accessApi.appAccessMine(accountId, appId).level }.getOrNull())
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** VAPP-177: a requested tab (`editorTabs.search`; none today on
     *  Android's route) → the phone's tab and the `…` menu's sheet over it. */
    val initialTab: Apps.EditorTabPick = Apps.editorTab(savedStateHandle.get<String>(Apps.EditorTabs.SEARCH), phone = true)

    /** VAPP-122: the viewer (`records:<kind>?mine=1`; null = signed out). */
    val me: StateFlow<String?> = auth.userId

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The app; null while syncing or once gone. */
    val app: StateFlow<AppListEntry?> =
        dbFlow.scopedQuery<AppEntity?>(null) { it.appDao().observeById(appId) }
            .map { it?.toListEntry() }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** VAPP-120: a team OWNER (the only one who grants tools; with the
     *  creator, who may rename, describe and delete). */
    val isOwner: StateFlow<Boolean> = dbFlow.isOwnerOf(app.map { it?.teamId }.distinctUntilChanged(), auth.userId)
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /** VAPP-187: my role in the app's team (null = not a member): the Data
     *  tab disables the `…` of a record I may not change. */
    val myRole: StateFlow<String?> = dbFlow.roleOf(app.map { it?.teamId }.distinctUntilChanged(), auth.userId)
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** VAPP-120: the app's synced `granted_tools` (the Settings sheet's switches). */
    val grantedTools: StateFlow<List<String>> =
        dbFlow.scopedQuery<AppEntity?>(null) { it.appDao().observeById(appId) }
            .map { AppActions.grantedList(it?.grantedTools) }
            .distinctUntilChanged()
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** #VAPP-220: the PUBLISHED package's automations (`appAccess.appTools`),
     *  refetched when the published version moves; empty = no group. */
    val automations: StateFlow<List<Apps.AutomationRowInput>> =
        app.map { it?.publishedVersion }.distinctUntilChanged().flatMapLatest { published ->
            kotlinx.coroutines.flow.flow {
                val accountId = auth.activeAccountId.value
                if (published == null || accountId == null) {
                    emit(emptyList())
                    return@flow
                }
                emit(
                    runCatching { accessApi.appTools(accountId, appId).automations.mapNotNull { it.toRowInput() } }
                        .getOrElse { if (it is CancellationException) throw it else emptyList() },
                )
            }
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** VAPP-121: the app's synced records (Export's switch + rows). */
    val records: StateFlow<List<AppRecordEntity>> =
        dbFlow.scopedQuery(emptyList<AppRecordEntity>()) { it.appRecordDao().observeByApp(appId) }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Newest first. */
    val versions: StateFlow<List<AppVersionEntity>> =
        dbFlow.scopedQuery(emptyList<AppVersionEntity>()) { it.appVersionDao().observeByApp(appId) }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val users: StateFlow<Map<String, UserEntity>> =
        dbFlow.scopedQuery(emptyList<UserEntity>()) { it.userDao().observeAll() }
            .map { rows -> rows.associateBy { it.id } }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    /** VAPP-188: the app's team members by name (`AppRecords.authorOptions`),
     *  what an OWNER's Author picker offers. */
    val authorOptions: StateFlow<List<AppRecords.Member>> =
        combine(dbFlow.memberIdsOf(app.map { it?.teamId }.distinctUntilChanged()), users) { ids, people ->
            AppRecords.authorOptions(ids.map { id -> AppRecords.Member(id, userDisplayName(people[id], id)) })
        }.distinctUntilChanged().flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The caller's editor session ROOT for the app (null = none yet). */
    val root: StateFlow<SessionEntity?> = combine(
        dbFlow.scopedQuery(emptyList<SessionEntity>()) { it.sessionDao().observeAll() },
        auth.userId,
    ) { rows, me ->
        if (me == null) null else rows.firstOrNull { it.appId == appId && it.userId == me && it.parentId == null }
    }.distinctUntilChanged().flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** The device the FIRST send runs on: my default online one, else the first. */
    val deviceId: StateFlow<String?> =
        steerDeviceFlow(dbFlow, app.map { it?.teamId }.distinctUntilChanged(), auth.userId)
            .map { devices -> LaunchDeviceRules.resolve(devices.orEmpty().filter { it.online }, null, null)?.deviceId }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private val _sending = MutableStateFlow(false)
    val sending: StateFlow<Boolean> = _sending

    private val _viewing = MutableStateFlow<AppViewing?>(null)
    /** The version the preview shows instead of the head (null = the head). */
    val viewing: StateFlow<AppViewing?> = _viewing

    private val _restoring = MutableStateFlow(false)
    val restoring: StateFlow<Boolean> = _restoring

    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure

    private val _toast = MutableStateFlow<String?>(null)
    /** A success line to toast once (Restore). */
    val toast: StateFlow<String?> = _toast

    fun clearFailure() {
        _failure.value = null
    }

    fun clearToast() {
        _toast.value = null
    }

    /** The first message: `sessions.send({appId, text, deviceId})` opens my editor session. */
    fun send(text: String, onSent: () -> Unit) {
        if (text.isBlank() || _sending.value) return
        _sending.value = true
        viewModelScope.launch {
            try {
                sender.send(SessionsSendInput.toApp(appId, text, deviceId.value))
                onSent()
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("send", "the message", trpcErrorMessage(t, "")) { send(text, onSent) }
            } finally {
                _sending.value = false
            }
        }
    }

    /** eApps V1: my editor session is live (its root exists, not ended), so
     *  the preview follows the head (`Apps.shownVersion`). */
    val editing: Boolean get() = root.value?.isEnded == false

    /** Preview version [number] (the preview's default clears the pick; the
     *  head's and the published package ride on the synced row). */
    fun pick(number: Int) {
        val entry = app.value ?: return
        if (number == Apps.shownVersion(entry.versions, editing, null)) {
            _viewing.value = null
            return
        }
        _viewing.value = AppViewing(number, null)
        if (number == entry.headVersion || number == entry.publishedVersion) return
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            try {
                val pkg = api.versionPackage(accountId, appId, number)
                if (_viewing.value?.number == number) _viewing.value = AppViewing(number, pkg)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                if (_viewing.value?.number == number) _viewing.value = null
            }
        }
    }

    fun clearViewing() {
        _viewing.value = null
    }

    private val _publishing = MutableStateFlow<Int?>(null)
    /** eApps V1: the version being published (null = none). */
    val publishing: StateFlow<Int?> = _publishing

    /** eApps V1 `apps.publish` of version [number] (the menu's head, a
     *  Versions row's or the banner's): toast `copy.publishedToast`, the
     *  preview back on its default; a failure = the failed-action toast. */
    fun publish(number: Int) {
        if (_publishing.value != null) return
        val accountId = auth.activeAccountId.value ?: return
        _publishing.value = number
        viewModelScope.launch {
            try {
                val result = api.publish(accountId, appId, number)
                _toast.value = Apps.publishedToast(result.published)
                _viewing.value = null
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("publish", "the version", trpcErrorMessage(t, "")) { publish(number) }
            } finally {
                _publishing.value = null
            }
        }
    }

    /** Restore the viewed version as a NEW head (`apps.restore`). */
    fun restore() {
        restoreVersion(_viewing.value?.number ?: return)
    }

    /** VAPP-177: Restore version [number] (a Versions row's button or the
     *  viewing banner's) as a NEW head; the preview goes back to the head. */
    fun restoreVersion(number: Int) {
        if (_restoring.value) return
        val accountId = auth.activeAccountId.value ?: return
        _restoring.value = true
        viewModelScope.launch {
            try {
                val result = api.restore(accountId, appId, number)
                _toast.value = Apps.restored(result.restored, result.version)
                _viewing.value = null
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("restore", "the version", trpcErrorMessage(t, "")) { restoreVersion(number) }
            } finally {
                _restoring.value = false
            }
        }
    }

    private val _deleting = MutableStateFlow(false)
    val deleting: StateFlow<Boolean> = _deleting

    /** VAPP-177 Settings: a patch of name / icon / colour / description
     *  (the creator or an owner; a failure = the failed-action toast). */
    fun update(name: String? = null, icon: String? = null, color: String? = null, description: String? = null) {
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            try {
                api.update(accountId, AppUpdateInput(id = appId, name = name, icon = icon, color = color, description = description))
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("save", "the app", trpcErrorMessage(t, "")) { update(name, icon, color, description) }
            }
        }
    }

    private val _grantBusy = MutableStateFlow<String?>(null)
    /** The tool whose grant is in flight (its switch waits). */
    val grantBusy: StateFlow<String?> = _grantBusy

    /** VAPP-120 / VAPP-156: an owner's switch writes the WHOLE grant list
     *  (grantable names only, VAPP-158) with the version on screen (eApps V1:
     *  `Apps.grantVersion`, the published one, else the head); a version
     *  published meanwhile refuses (CONFLICT, the toast) and resyncs. */
    fun setGrant(name: String, on: Boolean) {
        val entry = app.value ?: return
        if (!isOwner.value || !AppActions.actionGrantable(name) || _grantBusy.value != null) return
        val accountId = auth.activeAccountId.value ?: return
        val kept = grantedTools.value.filter { it != name && AppActions.actionGrantable(it) }
        val next = if (on) kept + name else kept
        _grantBusy.value = name
        viewModelScope.launch {
            try {
                api.update(accountId, AppUpdateInput(id = appId, headVersion = Apps.grantVersion(entry.versions), grantedTools = next))
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("save", "the app", trpcErrorMessage(t, ""))
            } finally {
                _grantBusy.value = null
            }
        }
    }

    /** Delete app (the creator or an owner) → [onDone] leaves for the list. */
    fun delete(onDone: () -> Unit) {
        if (_deleting.value) return
        val accountId = auth.activeAccountId.value ?: return
        _deleting.value = true
        viewModelScope.launch {
            try {
                api.delete(accountId, appId)
                onDone()
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("delete", "the app", trpcErrorMessage(t, "")) { delete(onDone) }
            } finally {
                _deleting.value = false
            }
        }
    }

    private val _recordSaving = MutableStateFlow(false)
    /** VAPP-178: a Data form's Save in flight. */
    val recordSaving: StateFlow<Boolean> = _recordSaving

    private val _recordError = MutableStateFlow<String?>(null)
    /** VAPP-178: the server's BAD_REQUEST (the schema's issues), shown
     *  INLINE under the form; anything else is the failed-action toast. */
    val recordError: StateFlow<String?> = _recordError

    fun clearRecordError() {
        _recordError.value = null
    }

    /** VAPP-178: the Data form's Save = `apps.records.upsert {id, kind, data,
     *  recordId?}` as the viewer (create without [recordId], else a shallow
     *  patch); [onDone] closes the form. VAPP-188: [author] = an owner's
     *  on-behalf pick (`AppRecords.formAuthor.send`; null = omitted). */
    fun saveRecord(kind: String, data: Map<String, JsonValue>, recordId: String?, author: String? = null, onDone: () -> Unit) {
        if (_recordSaving.value) return
        val accountId = auth.activeAccountId.value ?: return
        _recordSaving.value = true
        _recordError.value = null
        viewModelScope.launch {
            try {
                val body = Json.parseToJsonElement(JsonValue.Obj(data).orderedJson) as JsonObject
                val result = api.recordsUpsert(accountId, appId, kind, body, recordId, author, via = "editor")
                onDone()
                result.eventId?.let { _undo.tryEmit(RecordUndo(if (result.created) "create" else "update", listOf(it), "editor")) }
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                if (!isOfflineError(t) && t is TrpcException && t.code == "BAD_REQUEST" && !t.isPlanLimit) {
                    _recordError.value = trpcErrorMessage(t, "")
                } else {
                    _failure.value = FailedAction("save", "the record", trpcErrorMessage(t, ""))
                }
            } finally {
                _recordSaving.value = false
            }
        }
    }

    private val _recordDeleting = MutableStateFlow(false)
    val recordDeleting: StateFlow<Boolean> = _recordDeleting

    /** VAPP-178: a confirmed Delete = `apps.records.delete {id, recordId}`. */
    fun deleteRecord(recordId: String, onDone: () -> Unit) {
        if (_recordDeleting.value) return
        val accountId = auth.activeAccountId.value ?: return
        _recordDeleting.value = true
        viewModelScope.launch {
            try {
                val result = api.recordsDelete(accountId, appId, recordId, via = "editor")
                onDone()
                result.eventId?.let { _undo.tryEmit(RecordUndo("delete", listOf(it), "editor")) }
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                onDone()
                _failure.value = FailedAction("delete", "the record", trpcErrorMessage(t, "")) { deleteRecord(recordId) {} }
            } finally {
                _recordDeleting.value = false
            }
        }
    }

    private val _undo = MutableSharedFlow<RecordUndo>(extraBufferCapacity = 8)
    /** VAPP-215: a Data tab write to toast Undo for (the screen raises it, [recordUndo]). */
    val undo: SharedFlow<RecordUndo> = _undo

    /** VAPP-215: the toast's Undo = `apps.records.undo {id, eventIds, via}`;
     *  a refusal = the failed-action toast `Couldn't undo the change`. */
    fun undoRecords(eventIds: List<String>, via: String) {
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            try {
                api.recordsUndo(accountId, appId, eventIds, via)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction(AppRecords.HistoryCopy.UNDO_VERB, AppRecords.HistoryCopy.UNDO_THING, trpcErrorMessage(t, ""))
            }
        }
    }

    /** VAPP-215: record [recordId]'s History (newest first); a failed read = none. */
    suspend fun recordHistory(recordId: String): List<AppRecords.RecordEvent> {
        val accountId = auth.activeAccountId.value ?: return emptyList()
        return try {
            api.recordsHistory(accountId, appId, recordId).events.map { e ->
                AppRecords.RecordEvent(
                    id = e.id,
                    op = e.op,
                    before = e.before?.let { runCatching { JsonValue.parse(it.toString()).obj }.getOrNull() },
                    patch = e.patch?.let { runCatching { JsonValue.parse(it.toString()).obj }.getOrNull() },
                    actor = e.actor,
                    createdAt = e.createdAt,
                )
            }
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            emptyList()
        }
    }

    private val _recordRestoring = MutableStateFlow<String?>(null)
    /** VAPP-215: the History event whose Restore is in flight. */
    val recordRestoring: StateFlow<String?> = _recordRestoring

    /** VAPP-215: History's Restore = `apps.records.restore {id, recordId,
     *  eventId, via}`; [onDone] closes the form, then the Undo toast. */
    fun restoreRecord(recordId: String, eventId: String, onDone: () -> Unit) {
        if (_recordRestoring.value != null) return
        val accountId = auth.activeAccountId.value ?: return
        _recordRestoring.value = eventId
        viewModelScope.launch {
            try {
                val result = api.recordsRestore(accountId, appId, recordId, eventId, via = "editor")
                onDone()
                _undo.tryEmit(RecordUndo("update", result.eventIds, "editor"))
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("restore", "the record", trpcErrorMessage(t, ""))
            } finally {
                _recordRestoring.value = null
            }
        }
    }

    /** Leaving the editor ENDS my live editor session (its tree); the next
     *  send continues it. VAPP-176: leaving an EMPTY draft (no version, no
     *  editor session: nothing was ever sent) discards it (`apps.discardDraft`,
     *  a server-checked no-op otherwise). Outlives the screen (a detached scope). */
    fun leave() {
        val accountId = auth.activeAccountId.value ?: return
        val current = root.value
        if (current == null) {
            val entry = app.value
            if (entry != null && Apps.isDraft(entry.headVersion) && !_sending.value) {
                leaveScope.launch { runCatching { api.discardDraft(accountId, appId) } }
            }
            return
        }
        val live = current.takeIf { !it.isEnded } ?: return
        leaveScope.launch {
            runCatching { sessionsApi.end(accountId, live.id) }
        }
    }

    private companion object {
        val leaveScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    }
}
