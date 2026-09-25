package com.exponential.app.ui.workflows

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.WorkflowEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.WorkflowQuestions
import com.exponential.app.domain.WorkflowView
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

// EXP-981: the selected team's workflows, LIVE off the synced `workflows`
// shape — three bands (Running / Draft / Done, `WorkflowView.band`) of flat
// rows, newest first inside each, empty bands hidden. Read-only: everything a
// workflow can be told to do lives on its detail screen.

/** One band of the list, already ordered. Never emitted empty. */
data class WorkflowBandRows(
    val band: WorkflowView.Band,
    val workflows: List<WorkflowEntity>,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class WorkflowsViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
    selection: TeamSelection,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The team the list is scoped to — a workflow is team work. */
    val teamId: StateFlow<String?> = selection.selectedId

    private val workflows: StateFlow<List<WorkflowEntity>> =
        combine(dbFlow, selection.selectedId) { db, teamId -> db to teamId }
            .flatMapLatest { (db, teamId) ->
                if (db == null || teamId == null) {
                    flowOf(emptyList())
                } else {
                    // The DAO already answers newest first, which IS the order
                    // inside a band.
                    db.workflowDao().observeByTeam(teamId)
                }
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val bands: StateFlow<List<WorkflowBandRows>> = workflows
        .map { rows ->
            val byBand = rows.groupBy { WorkflowView.band(it.status) }
            WorkflowView.BANDS.mapNotNull { band ->
                byBand[band]?.takeIf { it.isNotEmpty() }?.let { WorkflowBandRows(band, it) }
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // Every synced run, whoever owns it: a question from a run on a teammate's
    // shared runner still waits for a person.
    private val sessions: StateFlow<List<CodingSessionEntity>> =
        dbFlow.scopedQuery(emptyList<CodingSessionEntity>()) { it.codingSessionDao().observeAll() }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** EXP-1069: any workflow of the team has an open question (the Agent page's red dot). */
    val needsYou: StateFlow<Boolean> = combine(workflows, sessions) { rows, sessionRows ->
        workflowsNeedYou(rows, sessionRows)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)
}

/** Any of [workflows] has an open question among [sessions] — every run of it, not only the caller's. */
internal fun workflowsNeedYou(workflows: List<WorkflowEntity>, sessions: List<CodingSessionEntity>): Boolean {
    val ids = workflows.mapTo(HashSet()) { it.id }
    val byWorkflow = sessions.filter { it.workflowId in ids }.groupBy { it.workflowId }
    return byWorkflow.any { (id, rows) -> id != null && WorkflowQuestions.open(rows, id).isNotEmpty() }
}
