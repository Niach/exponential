package com.exponential.app.ui.actions

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.IssueStatusCategory
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.ui.components.picker.LabelPickerLabel
import com.exponential.app.ui.components.picker.StatusPickerStatus
import com.exponential.app.ui.components.toPickerLabel
import com.exponential.app.ui.components.toPickerRow
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

/**
 * The label and status rows behind the EXP-530 event-filter pickers, as the
 * PICKER's own contract rows (EXP-1021): a label is a coloured dot and a
 * status is its resolved glyph in its tone, in every sheet that lists them,
 * and the automation form is no place to start drawing either as a bare name.
 *
 * Owned here rather than beside the launcher's other lookup sources because
 * those feed `board`/`repo`/`pr` action INPUTS, whose options are id + name by
 * contract — the colour and the resolved glyph are this form's needs alone.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class AutomationFilterOptionsViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
    selection: TeamSelection,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** Live, team-scoped labels — the `label_added` filter's rows. */
    val labels: StateFlow<List<LabelPickerLabel>> = combine(
        dbFlow.scopedQuery(emptyList<LabelEntity>()) { it.labelDao().observeAll() },
        selection.selectedId,
    ) { rows, teamId ->
        if (teamId == null) {
            emptyList()
        } else {
            rows.filter { it.teamId == teamId }
                .sortedBy { it.name.lowercase() }
                .map { it.toPickerLabel() }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * The team's REAL status rows in canonical display order — the
     * `status_changed` filter's rows. Constructed fallbacks (no row id) can't
     * be a filter target, and duplicate is never pickable (the web
     * buildStatusOptions rule).
     */
    val statuses: StateFlow<List<StatusPickerStatus>> = combine(
        dbFlow,
        selection.selectedId,
    ) { db, teamId -> db to teamId }
        .flatMapLatest { (db, teamId) ->
            if (db == null || teamId == null) {
                flowOf(emptyList())
            } else {
                db.issueStatusDao().observeByTeam(teamId).map { rows: List<IssueStatusEntity> ->
                    IssueStatusResolver.teamStatuses(rows)
                        .filter { it.rowId != null && it.category != IssueStatusCategory.Duplicate }
                        .map { it.toPickerRow() }
                }
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())
}
