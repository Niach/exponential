package com.exponential.app.ui.actions

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.AccountTree
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Bolt
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.ActionDto
import com.exponential.app.ui.issue.StartCodingSheet
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

// The Actions screen (EXP-253, view + run only — no create/edit on mobile):
// the selected team's action prompts, each with a Run affordance that opens
// the unified Start-coding sheet (EXP-257) preselected on that action — one
// launcher for issue runs AND action runs, with typed input fields and the
// full agent/model/effort/toggle options. The server-appended virtual builtin
// "Create action" pins first by its flag. After a successful send the screen
// waits for the desktop's synced coding_sessions row and jumps into the
// existing agent session viewer once.

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ActionsScreen(
    onBack: () -> Unit,
    onOpenSteer: (codingSessionId: String) -> Unit,
    viewModel: ActionsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val runState by viewModel.runState.collectAsStateWithLifecycle()
    val startedSessionId by viewModel.startedSessionId.collectAsStateWithLifecycle()
    val startCandidates by viewModel.startCandidates.collectAsStateWithLifecycle()

    // The action the unified sheet was opened for (non-null = sheet open).
    var sheetAction by remember { mutableStateOf<ActionDto?>(null) }

    // Re-poll device presence each time the screen comes to the foreground.
    LifecycleResumeEffect(Unit) {
        viewModel.refreshDevices()
        onPauseOrDispose { }
    }

    // The desktop picked the start up — jump into the live viewer ONCE.
    LaunchedEffect(startedSessionId) {
        startedSessionId?.let {
            viewModel.consumeStartedSession()
            onOpenSteer(it)
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Actions") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = Color.Transparent),
            )
        },
        containerColor = Color.Transparent,
    ) { padding ->
        Box(modifier = Modifier.padding(padding).fillMaxSize()) {
            when {
                state.actions.isEmpty() && state.loading ->
                    CircularProgressIndicator(modifier = Modifier.align(Alignment.Center))
                state.actions.isEmpty() && state.error != null ->
                    CenteredCaption(state.error ?: "")
                state.actions.isEmpty() -> ActionsEmptyState()
                else -> LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    val caption = runStateCaption(runState)
                    if (caption != null) {
                        item(key = "__run_state__") {
                            RunStateCaptionRow(
                                caption = caption,
                                showSpinner = runState is ActionRunState.Sending ||
                                    runState is ActionRunState.Sent,
                            )
                        }
                    }
                    // Builtin rows pin FIRST by the flag, never by sort order
                    // (EXP-257; the stable sort keeps server order otherwise).
                    items(
                        state.actions.sortedByDescending { it.isBuiltin },
                        key = { it.id },
                    ) { action ->
                        ActionRow(action = action, onRun = { sheetAction = action })
                    }
                }
            }
        }
    }

    val action = sheetAction
    if (action != null) {
        StartCodingSheet(
            devices = devices ?: emptyList(),
            issues = startCandidates,
            preselectedIds = emptySet(),
            preselectedActionId = action.id,
            onStart = viewModel::startCoding,
            onRunAction = viewModel::runAction,
            onDismiss = { sheetAction = null },
        )
    }
}

// One action: bolt glyph (a create + for the virtual builtin), name (+ a
// small repo indicator when the action clones a repository), optional
// description, and a trailing Run affordance.
@Composable
private fun ActionRow(action: ActionDto, onRun: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("action-row")
            .glassRow()
            .clickable(onClick = onRun)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            if (action.isBuiltin) Icons.Filled.Add else Icons.Filled.Bolt,
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    action.name,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (action.repositoryId != null) {
                    Spacer(Modifier.width(6.dp))
                    Icon(
                        Icons.Filled.AccountTree,
                        contentDescription = "Runs in a repository",
                        modifier = Modifier.size(12.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            }
            val description = action.description
            if (!description.isNullOrBlank()) {
                Text(
                    description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(5.dp),
            modifier = Modifier.padding(start = 8.dp),
        ) {
            Icon(
                Icons.Filled.PlayArrow,
                contentDescription = null,
                modifier = Modifier.size(15.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
            Text(
                "Run",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.primary,
            )
        }
    }
}

private data class RunCaption(val text: String, val isError: Boolean)

private fun runStateCaption(state: ActionRunState): RunCaption? = when (state) {
    is ActionRunState.Idle -> null
    is ActionRunState.Sending -> RunCaption("Sending start command…", false)
    is ActionRunState.Sent ->
        RunCaption("Start sent to ${state.deviceLabel} — waiting for the desktop…", false)
    is ActionRunState.Failed -> RunCaption(state.message, true)
}

@Composable
private fun RunStateCaptionRow(caption: RunCaption, showSpinner: Boolean) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        modifier = Modifier.padding(vertical = 2.dp),
    ) {
        if (showSpinner) {
            CircularProgressIndicator(
                modifier = Modifier.size(12.dp),
                strokeWidth = 2.dp,
                color = MaterialTheme.colorScheme.onSurface,
            )
        }
        Text(
            caption.text,
            style = MaterialTheme.typography.labelSmall,
            color = if (caption.isError) {
                MaterialTheme.colorScheme.error
            } else {
                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
            },
        )
    }
}

@Composable
private fun CenteredCaption(text: String) {
    Box(Modifier.fillMaxSize().padding(horizontal = 40.dp), contentAlignment = Alignment.Center) {
        Text(
            text,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun ActionsEmptyState() {
    Box(Modifier.fillMaxSize().padding(horizontal = 40.dp), contentAlignment = Alignment.Center) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(
                Icons.Filled.Bolt,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.size(28.dp),
            )
            Text(
                "No actions yet",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
            Text(
                "Actions are reusable prompts your team runs on a desktop. " +
                    "Team owners create them on the web or desktop app.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                textAlign = TextAlign.Center,
            )
        }
    }
}
