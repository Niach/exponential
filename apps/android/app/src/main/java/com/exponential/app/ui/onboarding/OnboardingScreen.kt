package com.exponential.app.ui.onboarding

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.togetherWith
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.FolderSpecial
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard

// First-run onboarding, following the shared iOS/Android spec (EXP-188):
//   1. Welcome — app name + one-line value prop + "Get started".
//   2. Team — resolving spinner, then create-or-join when the account has no
//      team (signups no longer get one). Creating advances; joining via a
//      pasted invite link completes onboarding and exits the wizard.
//   3. Create your first board — board name + optional repository picker
//      (with inline GitHub connect when nothing is installed yet).
//   4. Done — drops into the app.
// One primary action per step; completing the create marks onboarding done
// server-side (see OnboardingViewModel), the done step just navigates.
@Composable
fun OnboardingScreen(
    onDone: () -> Unit,
    viewModel: OnboardingViewModel = hiltViewModel(),
) {
    val accountId by viewModel.accountId.collectAsStateWithLifecycle()
    val teamId by viewModel.teamId.collectAsStateWithLifecycle()
    val preparing by viewModel.preparing.collectAsStateWithLifecycle()
    val error by viewModel.error.collectAsStateWithLifecycle()
    val done by viewModel.done.collectAsStateWithLifecycle()
    val needsTeamChoice by viewModel.needsTeamChoice.collectAsStateWithLifecycle()
    val teamSubmitting by viewModel.teamSubmitting.collectAsStateWithLifecycle()
    val teamError by viewModel.teamError.collectAsStateWithLifecycle()

    var step by remember { mutableIntStateOf(0) }

    LaunchedEffect(Unit) {
        viewModel.reconcile()
        viewModel.prepare()
    }
    // Reconcile self-heal: an account that already onboarded elsewhere skips the
    // wizard entirely (a completed create advances to the done step instead).
    LaunchedEffect(done) { if (done) onDone() }
    // The team step is a pass-through once a team resolves (existing membership
    // or a successful create) — advance straight to the board step.
    LaunchedEffect(step, teamId) { if (step == 1 && teamId != null) step = 2 }

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp, vertical = 40.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            AnimatedContent(
                targetState = step,
                transitionSpec = { fadeIn() togetherWith fadeOut() },
                label = "onboarding-step",
            ) { current ->
                when (current) {
                    0 -> WelcomeStep(onContinue = { step = 1 })
                    1 -> TeamStep(
                        preparing = preparing,
                        needsChoice = needsTeamChoice,
                        submitting = teamSubmitting,
                        prepareError = error,
                        actionError = teamError,
                        onRetry = { viewModel.prepare() },
                        onCreateTeam = { viewModel.createTeam(it) },
                        onJoinTeam = { viewModel.joinTeam(it) },
                    )
                    2 -> CreateBoardStep(
                        accountId = accountId,
                        teamId = teamId,
                        preparing = preparing,
                        error = error,
                        onRetry = { viewModel.prepare() },
                        onCreated = { boardId ->
                            viewModel.onBoardCreated(boardId)
                            step = 3
                        },
                    )
                    else -> DoneStep(onFinish = {
                        viewModel.finish()
                        onDone()
                    })
                }
            }
        }
    }
}

// Step 1 — Welcome: app name + one-line value prop, one primary action.
@Composable
private fun WelcomeStep(onContinue: () -> Unit) {
    Column(
        modifier = Modifier.widthIn(max = 460.dp).fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(
            modifier = Modifier.size(56.dp).glassCard(),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.Filled.FolderSpecial,
                contentDescription = null,
                modifier = Modifier.size(28.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
        }
        Spacer(Modifier.height(24.dp))
        Text(
            "Exponential",
            style = MaterialTheme.typography.headlineMedium,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(12.dp))
        Text(
            "Track issues and ship with your team.",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(32.dp))
        Button(onClick = onContinue, modifier = Modifier.fillMaxWidth()) {
            Text("Get started")
        }
    }
}

// Step 2 — Team (EXP-188): while getDefault resolves, a spinner; an account
// with a team auto-advances (the screen's LaunchedEffect); otherwise the
// create-or-join choice. Joining exits the wizard via the done flow.
@Composable
private fun TeamStep(
    preparing: Boolean,
    needsChoice: Boolean,
    submitting: Boolean,
    prepareError: String?,
    actionError: String?,
    onRetry: () -> Unit,
    onCreateTeam: (String) -> Unit,
    onJoinTeam: (String) -> Unit,
) {
    var teamName by remember { mutableStateOf("") }
    var inviteInput by remember { mutableStateOf("") }
    Column(
        modifier = Modifier.widthIn(max = 460.dp).fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            "Set up your team",
            style = MaterialTheme.typography.headlineSmall,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(12.dp))
        Text(
            "Create a team for your work, or join an existing one with an invite.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(28.dp))

        when {
            !needsChoice && prepareError != null -> {
                Text(
                    prepareError,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                    textAlign = TextAlign.Center,
                )
                Spacer(Modifier.height(12.dp))
                TextButton(onClick = onRetry) { Text("Retry") }
            }
            preparing || !needsChoice -> {
                Column(
                    modifier = Modifier.fillMaxWidth().padding(vertical = 32.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    CircularProgressIndicator()
                    Text(
                        "Checking your teams…",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            }
            else -> {
                Column(
                    modifier = Modifier.fillMaxWidth().glassCard().padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text("Create a team", style = MaterialTheme.typography.titleSmall)
                    OutlinedTextField(
                        value = teamName,
                        onValueChange = { teamName = it },
                        singleLine = true,
                        label = { Text("Team name") },
                        enabled = !submitting,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Button(
                        onClick = { onCreateTeam(teamName) },
                        enabled = !submitting && teamName.isNotBlank(),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(if (submitting) "Creating…" else "Create team")
                    }
                }
                Spacer(Modifier.height(16.dp))
                Column(
                    modifier = Modifier.fillMaxWidth().glassCard().padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text("Join a team", style = MaterialTheme.typography.titleSmall)
                    Text(
                        "Ask a teammate for an invite link and paste it here.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                    OutlinedTextField(
                        value = inviteInput,
                        onValueChange = { inviteInput = it },
                        singleLine = true,
                        label = { Text("Invite link or code") },
                        enabled = !submitting,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    OutlinedButton(
                        onClick = { onJoinTeam(inviteInput) },
                        enabled = !submitting && inviteInput.isNotBlank(),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(if (submitting) "Joining…" else "Join team")
                    }
                }
                if (actionError != null) {
                    Spacer(Modifier.height(12.dp))
                    Text(
                        actionError,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.error,
                        textAlign = TextAlign.Center,
                    )
                }
            }
        }
    }
}

// Step 3 — Create your first board: name + optional repository (with inline
// GitHub connect inside the picker when no installation exists yet).
@Composable
private fun CreateBoardStep(
    accountId: String?,
    teamId: String?,
    preparing: Boolean,
    error: String?,
    onRetry: () -> Unit,
    onCreated: (String) -> Unit,
) {
    Column(
        modifier = Modifier.widthIn(max = 460.dp).fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            "Create your first board",
            style = MaterialTheme.typography.headlineSmall,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(12.dp))
        Text(
            "Name your board — connecting a GitHub repository is optional.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(28.dp))

        when {
            preparing || accountId == null || teamId == null -> {
                if (error != null) {
                    Text(
                        error,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.error,
                        textAlign = TextAlign.Center,
                    )
                    Spacer(Modifier.height(12.dp))
                    TextButton(onClick = onRetry) { Text("Retry") }
                } else {
                    Column(
                        modifier = Modifier.fillMaxWidth().padding(vertical = 32.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        CircularProgressIndicator()
                        Text(
                            "Setting up your team…",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                }
            }
            else -> {
                CreateBoardForm(
                    accountId = accountId,
                    teamId = teamId,
                    onCreated = onCreated,
                    minimal = true,
                )
            }
        }
    }
}

// Step 4 — Done: confirmation, one action that drops into the app.
@Composable
private fun DoneStep(onFinish: () -> Unit) {
    Column(
        modifier = Modifier.widthIn(max = 460.dp).fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(
            modifier = Modifier.size(56.dp).glassCard(),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.Filled.Check,
                contentDescription = null,
                modifier = Modifier.size(28.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
        }
        Spacer(Modifier.height(24.dp))
        Text(
            "You're all set",
            style = MaterialTheme.typography.headlineSmall,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(12.dp))
        Text(
            "Your board is ready.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(32.dp))
        Button(onClick = onFinish, modifier = Modifier.fillMaxWidth()) {
            Text("Start tracking issues")
        }
    }
}
