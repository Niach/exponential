package com.exponential.app.ui.issue

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.IssueDescription
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.UpdateIssueInput
import com.exponential.app.data.db.IssueDao
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

@HiltViewModel
class IssueDetailViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val issueDao: IssueDao,
    private val issuesApi: IssuesApi,
) : ViewModel() {

    val issueId: String = savedStateHandle["issueId"] ?: ""

    val issue: StateFlow<IssueEntity?> = issueDao.observeById(issueId)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    fun updateTitle(title: String) {
        if (title.isBlank()) return
        viewModelScope.launch {
            runCatching {
                issuesApi.update(UpdateIssueInput(id = issueId, title = title.trim()))
            }
        }
    }

    fun updateDescription(text: String) {
        viewModelScope.launch {
            runCatching {
                issuesApi.update(
                    UpdateIssueInput(id = issueId, description = IssueDescription(text))
                )
            }
        }
    }

    fun updateStatus(status: IssueStatus) {
        viewModelScope.launch {
            runCatching {
                issuesApi.update(UpdateIssueInput(id = issueId, status = status.wire))
            }
        }
    }

    fun updatePriority(priority: IssuePriority) {
        viewModelScope.launch {
            runCatching {
                issuesApi.update(UpdateIssueInput(id = issueId, priority = priority.wire))
            }
        }
    }

    fun updateDueDate(date: String?) {
        viewModelScope.launch {
            runCatching {
                issuesApi.update(UpdateIssueInput(id = issueId, dueDate = date))
            }
        }
    }

    fun delete(onDeleted: () -> Unit) {
        viewModelScope.launch {
            runCatching { issuesApi.delete(issueId) }.onSuccess { onDeleted() }
        }
    }
}
