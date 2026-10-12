package com.exponential.app.ui.onboarding

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.EmptyStateCopy
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.SheetPrimaryAction

// The onboarding create-board form presented as a bottom sheet for the app's
// empty states (no boards yet). Resolves the target team itself — callers
// that already know it pass `teamId`; the account-level empty states pass
// null and the default team is ensured. On success it records the board as
// last-used and hands the id back to the caller.
@Composable
fun CreateBoardSheet(
    teamId: String?,
    onCreated: (boardId: String) -> Unit,
    onDismiss: () -> Unit,
    viewModel: CreateBoardViewModel = hiltViewModel(),
) {
    val resolvedTeamId by viewModel.teamId.collectAsStateWithLifecycle()
    val accountId by viewModel.accountId.collectAsStateWithLifecycle()
    val state by viewModel.state.collectAsStateWithLifecycle()

    LaunchedEffect(teamId) { viewModel.ensureTeam(teamId) }
    FailedActionToast(state.setupFailure, onShown = viewModel::clearSetupFailure)

    val form = rememberCreateBoardFormState()
    val ws = resolvedTeamId
    val acct = accountId
    // The shared sheet shell (EXP-687): drag handle, left title, and the
    // create button pinned to the bottom edge — no Cancel capsule, and the
    // form's own inline submit is handed over via `showSubmit = false`.
    GlassSheet(
        title = "New board",
        onDismiss = onDismiss,
        primaryAction = if (ws == null || acct == null) {
            null
        } else {
            SheetPrimaryAction(
                label = "Create board",
                enabled = form.canCreate,
                loading = state.submitting,
                onClick = {
                    // Repo is optional — send whatever (if any) is selected.
                    viewModel.create(
                        ws,
                        form.name,
                        form.prefix,
                        form.color,
                        form.iconName,
                        form.repository,
                        form.branch,
                    ) { boardId ->
                        viewModel.rememberCreated(boardId)
                        onCreated(boardId)
                    }
                },
            )
        },
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp),
        ) {
            if (ws == null || acct == null) {
                // The team resolve can fail (offline, server error, no team
                // yet — EXP-188) — without this branch the sheet would wait
                // forever (EXP-46): FEED-79, the empty state plus a toast
                // whose Retry resolves again. Still resolving = skeleton rows
                // (EXP-1275).
                if (state.setupFailed) {
                    EmptyState(EmptyStateCopy.noTeam)
                } else {
                    SkeletonRows(modifier = Modifier.padding(vertical = 16.dp), dot = false)
                }
            } else {
                CreateBoardForm(
                    accountId = acct,
                    teamId = ws,
                    onCreated = { boardId ->
                        viewModel.rememberCreated(boardId)
                        onCreated(boardId)
                    },
                    showSubmit = false,
                    form = form,
                    viewModel = viewModel,
                )
            }
        }
    }
}
