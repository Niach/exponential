package com.exponential.app.ui.onboarding

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// The onboarding create-board form presented as a bottom sheet for the app's
// empty states (no boards yet). Resolves the target team itself — callers
// that already know it pass `teamId`; the account-level empty states pass
// null and the default team is ensured. On success it records the board as
// last-used and hands the id back to the caller.
@OptIn(ExperimentalMaterial3Api::class)
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

    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = GlassTokens.BackgroundBottom,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp)
                .padding(bottom = 24.dp),
        ) {
            Text(
                "Create board",
                style = MaterialTheme.typography.titleMedium,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.padding(bottom = 16.dp),
            )
            val ws = resolvedTeamId
            val acct = accountId
            if (ws == null || acct == null) {
                // ensureDefault can fail (offline, server error) — without this
                // branch the sheet would spin "Setting up your team…"
                // forever (EXP-46); surface the error with a retry instead.
                val setupError = state.error
                if (setupError != null) {
                    Column(
                        modifier = Modifier.fillMaxWidth().padding(vertical = 32.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        Text(
                            setupError,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.error,
                        )
                        OutlinedButton(onClick = { viewModel.ensureTeam(teamId) }) {
                            Text("Retry")
                        }
                    }
                } else {
                    Column(
                        modifier = Modifier.fillMaxWidth().padding(vertical = 32.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        CircularProgressIndicator(modifier = Modifier.size(24.dp), strokeWidth = 2.dp)
                        Text(
                            "Setting up your team…",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                }
            } else {
                CreateBoardForm(
                    accountId = acct,
                    teamId = ws,
                    onCreated = { boardId ->
                        viewModel.rememberCreated(boardId)
                        onCreated(boardId)
                    },
                    viewModel = viewModel,
                )
            }
        }
    }
}
