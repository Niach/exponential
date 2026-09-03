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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.ui.components.GlassSheet

/**
 * The zero-team entry point (EXP-188, EXP-698): the create-or-join form in the
 * shared sheet shell, replacing the two stock Material dialogs the Issues home
 * used to raise. Two inline submits, so the chrome's pinned primary slot stays
 * empty — iOS `TeamSetupSheet` draws exactly this.
 *
 * The callbacks fire on success; the team selection is already switched by the
 * view model, so a caller only has to close the sheet.
 */
@Composable
fun TeamSetupSheet(
    onDismiss: () -> Unit,
    onCreated: (teamId: String) -> Unit,
    onJoined: () -> Unit,
    viewModel: TeamSetupViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    LaunchedEffect(viewModel) {
        viewModel.reset()
        viewModel.events.collect { event ->
            when (event) {
                is TeamSetupEvent.Created -> onCreated(event.teamId)
                TeamSetupEvent.Joined -> onJoined()
            }
        }
    }

    GlassSheet(title = "Set up a team", onDismiss = onDismiss, primaryAction = null) {
        // GlassSheet bounds its content slot but never scrolls it — the caller
        // owns the scroller. The sheet already applies the ime + system-bar
        // insets, so none are added here.
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp)
                // The styleguide lane's `sg_onboarding-create-team` anchor: the
                // switcher's "New team" row closes itself before opening this,
                // so the lane waits on the sheet appearing rather than on a
                // delay.
                .testTag("team-setup-sheet"),
        ) {
            TeamSetupForm(
                state = TeamSetupFormState(
                    busy = state.busy,
                    createError = state.createError,
                    joinError = state.joinError,
                ),
                onCreate = viewModel::createTeam,
                onJoin = viewModel::joinTeam,
            )
        }
    }
}
