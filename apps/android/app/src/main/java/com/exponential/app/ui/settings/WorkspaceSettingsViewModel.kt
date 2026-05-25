package com.exponential.app.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.domain.DomainContract
import com.exponential.app.data.api.LabelsApi
import com.exponential.app.data.api.UpdateLabelInput
import com.exponential.app.data.api.UpdateWorkspaceInput
import com.exponential.app.data.api.WorkspaceInvitesApi
import com.exponential.app.data.api.WorkspaceMembersApi
import com.exponential.app.data.api.WorkspacesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.WorkspaceEntity
import com.exponential.app.data.db.WorkspaceInviteEntity
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
    private val holder: DatabaseHolder,
    private val membersApi: WorkspaceMembersApi,
    private val invitesApi: WorkspaceInvitesApi,
    private val labelsApi: LabelsApi,
    private val workspacesApi: WorkspacesApi,
) : ViewModel() {

    private val accountId = auth.activeAccountId.value ?: ""
    private val db = holder.database(forAccountId = accountId)

    private val workspaceFlow = selection.selectedId.flatMapLatest { id ->
        if (id == null) flowOf(null)
        else db.workspaceDao().observeAll().map { list -> list.firstOrNull { it.id == id } }
    }

    private val membersFlow = selection.selectedId.flatMapLatest { id ->
        if (id == null) flowOf(emptyList()) else db.workspaceMemberDao().observeByWorkspace(id)
    }
    private val invitesFlow = selection.selectedId.flatMapLatest { id ->
        if (id == null) flowOf(emptyList()) else db.workspaceInviteDao().observeByWorkspace(id)
    }
    private val labelsFlow = selection.selectedId.flatMapLatest { id ->
        if (id == null) flowOf(emptyList()) else db.labelDao().observeByWorkspace(id)
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
            db.userDao().observeAll(),
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
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { membersApi.updateRole(accountId, memberId, role) }
            .onFailure { _transient.value = it.message }
    }

    fun removeMember(memberId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { membersApi.remove(accountId, memberId) }
            .onFailure { _transient.value = it.message }
    }

    fun createInvite(role: String = DomainContract.workspaceRoleMember) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { invitesApi.create(accountId, workspaceId, role) }
            .onSuccess { _createdInviteToken.value = it.token }
            .onFailure { _transient.value = it.message }
    }

    fun revokeInvite(id: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { invitesApi.revoke(accountId, id) }
            .onFailure { _transient.value = it.message }
    }

    fun consumeCreatedInvite() {
        _createdInviteToken.value = null
    }

    fun deleteLabel(labelId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.delete(accountId, workspaceId, labelId) }
            .onFailure { _transient.value = it.message }
    }

    fun renameLabel(labelId: String, name: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching { labelsApi.update(accountId, UpdateLabelInput(workspaceId, labelId, name = name)) }
            .onFailure { _transient.value = it.message }
    }

    fun consumeTransient() { _transient.value = null }

    fun setPublic(isPublic: Boolean) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching {
            workspacesApi.update(
                accountId,
                UpdateWorkspaceInput(
                    id = workspaceId,
                    isPublic = isPublic,
                    // Default policy when first enabling — matches the web flow.
                    publicWritePolicy = if (isPublic) DomainContract.publicWritePolicyMembers else null,
                )
            )
        }.onFailure { _transient.value = it.message }
    }

    fun setPublicWritePolicy(policy: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        val workspaceId = selection.selectedId.value ?: return@launch
        runCatching {
            workspacesApi.update(accountId, UpdateWorkspaceInput(id = workspaceId, publicWritePolicy = policy))
        }.onFailure { _transient.value = it.message }
    }
}
