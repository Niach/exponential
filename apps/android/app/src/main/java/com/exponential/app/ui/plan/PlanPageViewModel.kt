package com.exponential.app.ui.plan

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AnswerInput
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.thread.PlanAnswer
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject

/** What THE plan page draws: the plan card ([card] raw, [row] as THE thread
 *  row the Dock sheet answers), the asking session's name; [loading] until
 *  the first read. */
data class PlanPageState(
    val sessionId: String = "",
    val messageId: String = "",
    val card: JsonObject? = null,
    val row: SessionTree.ThreadRow.Plan? = null,
    val sessionName: String? = null,
    val loading: Boolean = true,
)

/**
 * #EXP-1493 THE plan page (plan-page.json): one plan card (`messageId`) of a
 * session (`sessionId`) off the synced `messages` row, the session's name off
 * its row; a decision goes through THE card-answer path (`sessions.answer`,
 * the inline card's call) and the page stays open.
 */
@HiltViewModel
class PlanPageViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val sessionsApi: SessionsApi,
) : ViewModel() {

    private val sessionId: String = savedStateHandle.get<String>("sessionId").orEmpty()
    private val messageId: String = savedStateHandle.get<String>("messageId").orEmpty()

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val state: StateFlow<PlanPageState> = combine(
        dbFlow.scopedQuery(emptyList()) { it.messageDao().observeByIds(listOf(messageId)) },
        dbFlow.scopedQuery<com.exponential.app.data.db.SessionEntity?>(null) { it.sessionDao().observeById(sessionId) },
    ) { rows, session ->
        val message = rows.firstOrNull()?.let { SessionTree.message(it, sessionId) }
        PlanPageState(
            sessionId = sessionId,
            messageId = messageId,
            card = message?.card,
            row = message?.let { SessionTree.askRow(it) as? SessionTree.ThreadRow.Plan },
            sessionName = session?.title,
            loading = false,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), PlanPageState(sessionId, messageId))

    private val _failure = MutableStateFlow<FailedAction?>(null)
    /** A refused decision, raised once as the fixture's toast. */
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    /** THE card-answer path (the inline card's `sessions.answer`). */
    fun decidePlan(messageId: String, decision: PlanAnswer, settled: (Boolean) -> Unit) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                settled(false)
                return@launch
            }
            try {
                sessionsApi.answer(
                    accountId,
                    AnswerInput(
                        messageId = messageId,
                        decision = if (decision.approved) "approved" else "rejected",
                        fresh = if (decision.approved && decision.fresh) true else null,
                        feedback = decision.feedback,
                        answers = decision.answers?.let(AnswerInput::planAnswers),
                        surfaceData = decision.surfaceData,
                    ),
                )
                settled(true)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                settled(false)
                _failure.value = FailedAction("answer", "the plan", trpcErrorMessage(t, ""))
            }
        }
    }
}
