package com.exponential.app.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.data.api.LabelsApi
import com.exponential.app.data.api.UpdateLabelInput
import com.exponential.app.data.api.WorkspaceInvitesApi
import com.exponential.app.data.api.WorkspaceMembersApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.LabelDao
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserDao
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.WorkspaceDao
import com.exponential.app.data.db.WorkspaceEntity
import com.exponential.app.data.db.WorkspaceInviteDao
import com.exponential.app.data.db.WorkspaceInviteEntity
import com.exponential.app.data.db.WorkspaceMemberDao
import com.exponential.app.data.db.WorkspaceMemberEntity
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class MemberRow(val member: WorkspaceMemberEntity, val user: UserEntity?)

data class WorkspaceSettingsState(
    val workspace: WorkspaceEntity? = null,
    val members: List<MemberRow> = emptyList(),
    val invites: List<WorkspaceInviteEntity> = emptyList(),
    val labels: List<LabelEntity> = emptyList(),
    val currentUserId: String? = null,
    val transient: String? = null,
    val createdInviteToken: String? = null,
    val instanceUrl: String? = null,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class WorkspaceSettingsViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val selection: WorkspaceSelection,
    private val workspaceDao: WorkspaceDao,
    private val workspaceMemberDao: WorkspaceMemberDao,
    private val workspaceInviteDao: WorkspaceInviteDao,
    private val labelDao: LabelDao,
    private val userDao: UserDao,
    private val membersApi: WorkspaceMembersApi,
    private val invitesApi: WorkspaceInvitesApi,
    private val labelsApi: LabelsApi,
) : ViewModel() {

    private val workspaceFlow = selection.selectedId.flatMapLatest { id ->
        if (id == null) flowOf(null)
        else workspaceDao.observeAll().map { list -> list.firstOrNull { it.id == id } }
    }

    private val membersFlow = selection.selectedId.flatMapLatest { id ->
        if (id == null) flowOf(emptyList()) else workspaceMemberDao.observeByWorkspace(id)
    }
    private val invitesFlow = selection.selectedId.flatMapLatest { id ->
        if (id == null) flowOf(emptyList()) else workspaceInviteDao.observeByWorkspace(id)
    }
    private val labelsFlow = selection.selectedId.flatMapLatest { id ->
        if (id == null) flowOf(emptyList()) else labelDao.observeByWorkspace(id)
    }

    private val _transient = MutableStateFlow<String?>(null)
    private val _createdInviteToken = MutableStateFlow<String?>(null)
    val transient: StateFlow<String?> = _transient.asStateFlow()

    val state: StateFlow<WorkspaceSettingsState> = combine(
        listOf(
            workspaceFlow,
            membersFlow,
            invitesFlow,
            labelsFlow,
            userDao.observeAll(),
            auth.userId,
            auth.instanceUrl,
            _transient,
            _createdInviteToken,
        )
    ) { values ->
        @Suppress("UNCHECKED_CAST")
        val workspace = values[0] as WorkspaceEntity?
        @Suppress("UNCHECKED_CAST")
        val members = values[1] as List<WorkspaceMemberEntity>
        @Suppress("UNCHECKED_CAST")
        val invites = values[2] as List<WorkspaceInviteEntity>
        @Suppress("UNCHECKED_CAST")
        val labels = values[3] as List<LabelEntity>
        @Suppress("UNCHECKED_CAST")
        val users = values[4] as List<UserEntity>
        val currentUserId = values[5] as String?
        val instance = values[6] as String?
        val transient = values[7] as String?
        val invite = values[8] as String?
        WorkspaceSettingsState(
            workspace = workspace,
            members = members.map { m -> MemberRow(m, users.firstOrNull { it.id == m.userId }) },
            invites = invites,
            labels = labels,
            currentUserId = currentUserId,
            transient = transient,
            createdInviteToken = invite,
            instanceUrl = instance,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), WorkspaceSettingsState())

    fun updateRole(memberId: String, role: String) = viewModelScope.launch {
        runCatching { membersApi.updateRole(memberId, role) }
            .onFailure { _transient.value = it.message }
    }

    fun removeMember(memberId: String) = viewModelScope.launch {
        runCatching { membersApi.remove(memberId) }
            .onFailure { _transient.value = it.message }
    }

    fun createInvite(role: String = "member") = viewModelScope.launch {
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { invitesApi.create(workspaceId, role) }
            .onSuccess { _createdInviteToken.value = it.token }
            .onFailure { _transient.value = it.message }
    }

    fun revokeInvite(id: String) = viewModelScope.launch {
        runCatching { invitesApi.revoke(id) }
            .onFailure { _transient.value = it.message }
    }

    fun consumeCreatedInvite() {
        _createdInviteToken.value = null
    }

    fun deleteLabel(labelId: String) = viewModelScope.launch {
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.delete(workspaceId, labelId) }
            .onFailure { _transient.value = it.message }
    }

    fun renameLabel(labelId: String, name: String) = viewModelScope.launch {
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.update(UpdateLabelInput(workspaceId, labelId, name = name)) }
            .onFailure { _transient.value = it.message }
    }

    fun consumeTransient() { _transient.value = null }
}
