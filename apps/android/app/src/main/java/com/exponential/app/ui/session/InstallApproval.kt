package com.exponential.app.ui.session

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.Chats
import com.exponential.app.ui.theme.AppBackground
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.enet.EnetNodeManager
import com.exponential.app.data.sealed.InstallTrustSnapshot
import com.exponential.app.data.sealed.TrustRow
import com.exponential.app.domain.InstallApprovalView
import com.exponential.app.ui.ci.CiSurface
import com.exponential.app.ui.components.ClaimedFailedActionToast
import com.exponential.app.ui.components.FailedAction
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonPrimitive

// #EXP-1950 (fixtures/install-trust.json, decisions a1 + b1; web
// `components/devices/install-approval.tsx`, iOS `InstallApproval.swift`): the
// approval of a new install and a team-mate's reset, drawn ONLY from core
// genUI parts ([InstallApprovalView]: Section, Stack, Row, Badge, Button,
// EmptyState) through [CiSurface], so the four platforms paint the same tree.
// Rows: Needs you, the phone's Devices too. Only rows `InstallTrust.reconcile`
// yields, never the server's word. A failed action = the toast
// (feedback-states.json). Approve / Remove only on a person's press.

/** This install's trust for the ACTIVE account, for the views (the keeper's snapshot). */
@HiltViewModel
class InstallTrustViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val nodes: EnetNodeManager,
    holder: DatabaseHolder,
) : ViewModel() {
    val snapshot: StateFlow<InstallTrustSnapshot> = combine(auth.activeAccountId, nodes.trust.snapshots) { accountId, snapshots ->
        accountId?.let { snapshots[it] } ?: InstallTrustSnapshot.NONE
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), InstallTrustSnapshot.NONE)

    /** A team-mate's name by user id (a Reset row's title leads with it). */
    val memberNames: StateFlow<Map<String, String>> = accountDatabaseFlow(auth, holder)
        .scopedQuery(emptyList()) { it.userDao().observeAll() }
        .map { users -> users.mapNotNull { user -> user.name?.let { user.id to it } }.toMap() }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure

    fun claimFailure(failure: FailedAction): Boolean = _failure.compareAndSet(failure, null)

    fun approve(row: TrustRow) = run("approve") { accountId -> nodes.trust.approve(accountId, row) }

    fun remove(row: TrustRow) = run("remove") { accountId -> nodes.trust.remove(accountId, row) }

    private fun run(verb: String, body: suspend (String) -> Unit) {
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch(Dispatchers.IO) {
            try {
                body(accountId)
            } catch (err: CancellationException) {
                throw err
            } catch (err: Throwable) {
                _failure.value = FailedAction(verb, "the install", trpcErrorMessage(err, ""))
            }
        }
    }
}

/** The band of installs waiting for this install's approval (nothing when none). */
@Composable
fun PendingInstalls(
    rows: List<TrustRow>,
    modifier: Modifier = Modifier,
    memberName: (String) -> String? = { null },
    onApprove: (TrustRow) -> Unit,
    onRemove: (TrustRow) -> Unit,
) {
    if (rows.isEmpty()) return
    CiSurface(
        surfaceId = InstallApprovalView.TestIds.PENDING,
        tree = InstallApprovalView.pendingTree(rows.map { it to InstallApprovalView.title(it, memberName(it.userId)) }),
        modifier = modifier.fillMaxWidth().testTag(InstallApprovalView.TestIds.PENDING),
    ) { name, context, _ ->
        val id = (context?.get("id") as? JsonPrimitive)?.content ?: return@CiSurface
        val row = rows.firstOrNull { it.installId == id } ?: return@CiSurface
        when (name) {
            InstallApprovalView.Events.APPROVE -> onApprove(row)
            InstallApprovalView.Events.REMOVE -> onRemove(row)
        }
    }
}

/** This install, not approved yet: THE page EmptyState with its own code. */
@Composable
fun InstallWaiting(code: String, modifier: Modifier = Modifier) {
    CiSurface(
        surfaceId = InstallApprovalView.TestIds.WAITING,
        tree = InstallApprovalView.waitingTree(code),
        modifier = modifier.fillMaxWidth().testTag(InstallApprovalView.TestIds.WAITING),
    )
}

/** The live band: the active account's rows, Approve / Remove through the keeper. */
@Composable
fun LivePendingInstalls(modifier: Modifier = Modifier, viewModel: InstallTrustViewModel = hiltViewModel()) {
    val snapshot by viewModel.snapshot.collectAsStateWithLifecycle()
    val names by viewModel.memberNames.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    ClaimedFailedActionToast(failure, viewModel::claimFailure)
    PendingInstalls(
        rows = snapshot.rows,
        modifier = modifier,
        memberName = { names[it] },
        onApprove = viewModel::approve,
        onRemove = viewModel::remove,
    )
}

/**
 * Where an encrypted chat would open on an install that waits for approval
 * and holds no key of it: the waiting state instead of locked rows.
 */
@Composable
fun InstallWaitingGate(sealed: Boolean, holdsKey: Boolean, viewModel: InstallTrustViewModel = hiltViewModel()) {
    val snapshot by viewModel.snapshot.collectAsStateWithLifecycle()
    val code = snapshot.code
    if (!sealed || holdsKey || !snapshot.waiting || code == null) return
    // Under the chat header (its back stays), over the locked transcript.
    AppBackground(Modifier.statusBarsPadding().padding(top = Chats.Shell.ChatHeader.ROW.dp)) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            InstallWaiting(code)
        }
    }
}
