package com.exponential.app.ui.onboarding

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedContentTransitionScope
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
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
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.InviteLinkCard
import com.exponential.app.ui.components.InviteLinkViewModel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard

// First-run onboarding. EXP-725 made the four product steps identical on every
// client (team -> board -> invite -> devices); the phones wrap them in a
// welcome page and a done page:
//   0. Welcome — app name + one-line value prop + "Get started".
//   1. Team — resolving spinner, then create-or-join when the account has no
//      team (signups no longer get one). Creating advances; joining via a
//      pasted invite link completes onboarding and exits the wizard.
//   2. Create your first board — board name + optional repository picker
//      (with inline GitHub connect when nothing is installed yet).
//   3. Invite your teammates — the shared invite-link creator. SKIPPABLE, and
//      absent entirely at the seat cap (InviteLinkCard owns that rule).
//   4. Set up your devices — install pointers + the caller's own machines.
//      SKIPPABLE, and LAST on purpose: finishing it means leaving for another
//      machine, so nothing may depend on it.
//   5. Done — drops into the app.
// One primary action per step; completing the board create marks onboarding
// done server-side (see OnboardingViewModel), the done step just navigates.
// Copy for steps 2-5 lives in OnboardingCopy (drift-gated against web).
@Composable
fun OnboardingScreen(
    onDone: () -> Unit,
    viewModel: OnboardingViewModel = hiltViewModel(),
) {
    val accountId by viewModel.accountId.collectAsStateWithLifecycle()
    val instanceUrl by viewModel.instanceUrl.collectAsStateWithLifecycle()
    val teamId by viewModel.teamId.collectAsStateWithLifecycle()
    val preparing by viewModel.preparing.collectAsStateWithLifecycle()
    val error by viewModel.error.collectAsStateWithLifecycle()
    val done by viewModel.done.collectAsStateWithLifecycle()
    val needsTeamChoice by viewModel.needsTeamChoice.collectAsStateWithLifecycle()
    val teamSubmitting by viewModel.teamSubmitting.collectAsStateWithLifecycle()
    val teamCreateError by viewModel.teamCreateError.collectAsStateWithLifecycle()
    val teamJoinError by viewModel.teamJoinError.collectAsStateWithLifecycle()

    var step by remember { mutableIntStateOf(0) }
    // EXP-523: `transitionSpec` is a plain lambda, not a composable one, so the
    // reduce-motion flag is read here and captured.
    val reduceMotion = LocalReduceMotion.current

    LaunchedEffect(Unit) {
        viewModel.reconcile()
        viewModel.prepare()
    }
    // Reconcile self-heal: an account that already onboarded elsewhere skips the
    // wizard entirely (a completed create advances to the done step instead).
    LaunchedEffect(done) { if (done) onDone() }
    // The team step is a pass-through once a team resolves (existing membership
    // or a successful create) — advance straight to the board step.
    LaunchedEffect(step, teamId) {
        if (step == 1 && teamId != null) {
            // Capture-only (see OnboardingTestHooks): the screenshot lane can
            // photograph the later steps without creating a team or a board.
            step = when (OnboardingTestHooks.startStep) {
                "invite" -> 3
                "devices" -> 4
                else -> 2
            }
        }
    }

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
                // EXP-523: steps push in from the trailing edge and leave to
                // the leading one — the same direction AppNavHost pushes, and
                // iOS parity. Was an untuned default-duration cross-fade,
                // which gave no sense that the wizard had moved forward.
                transitionSpec = {
                    val forward = targetState >= initialState
                    val direction = if (forward) {
                        AnimatedContentTransitionScope.SlideDirection.Start
                    } else {
                        AnimatedContentTransitionScope.SlideDirection.End
                    }
                    val spec = Motion.standard<IntOffset>(reduceMotion)
                    slideIntoContainer(direction, spec)
                        .plus(fadeIn(Motion.standard(reduceMotion)))
                        .togetherWith(
                            slideOutOfContainer(direction, spec)
                                .plus(fadeOut(Motion.standard(reduceMotion)))
                        )
                },
                label = "onboarding-step",
            ) { current ->
                when (current) {
                    0 -> WelcomeStep(onContinue = { step = 1 })
                    1 -> TeamStep(
                        preparing = preparing,
                        needsChoice = needsTeamChoice,
                        submitting = teamSubmitting,
                        prepareError = error,
                        createError = teamCreateError,
                        joinError = teamJoinError,
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
                    3 -> InviteStep(teamId = teamId, onContinue = { step = 4 })
                    4 -> OnboardingDevicesStep(
                        instanceOrigin = instanceUrl,
                        onContinue = { step = 5 },
                    )
                    else -> DoneStep(onFinish = {
                        viewModel.finish()
                        onDone()
                    })
                }
            }

            // EXP-725: the escape hatch is PERSISTENT, not an error-path
            // afterthought. This route replaces the back stack and hides the
            // bottom bar, so an account the server refuses (a deleted user
            // 401s everything) previously had no way out from any step that
            // happened not to be showing an error.
            Spacer(Modifier.height(24.dp))
            TextButton(
                onClick = { viewModel.signOut() },
                colors = ButtonDefaults.textButtonColors(
                    contentColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                ),
            ) {
                Text(OnboardingCopy.SIGN_OUT)
            }
        }
    }
}

// Step 3 — Invite your teammates: the shared creator, one trailing action.
// The button is a SKIP until a link exists, because nothing has been done yet;
// once one is minted it reads Continue. At the seat cap InviteLinkCard renders
// nothing at all and the step is just its header plus Skip (store policy —
// no hint, no pointer at the web).
@Composable
private fun InviteStep(teamId: String?, onContinue: () -> Unit) {
    var linkMinted by remember { mutableStateOf(false) }
    Column(
        modifier = Modifier
            .widthIn(max = 460.dp)
            .fillMaxWidth()
            .testTag("onboarding-invite-step"),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            OnboardingCopy.INVITE_TITLE,
            style = MaterialTheme.typography.headlineSmall,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(12.dp))
        Text(
            OnboardingCopy.INVITE_SUBTITLE,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(28.dp))
        if (teamId != null) {
            val inviteViewModel = hiltViewModel<InviteLinkViewModel>(key = "invite-link:$teamId")
            LaunchedEffect(teamId) { inviteViewModel.bind(teamId) }
            val inviteState by inviteViewModel.state.collectAsStateWithLifecycle()
            LaunchedEffect(inviteState.inviteUrl) {
                if (inviteState.inviteUrl != null) linkMinted = true
            }
            InviteLinkCard(
                state = inviteState,
                onGenerate = inviteViewModel::generate,
            )
            Spacer(Modifier.height(28.dp))
        }
        GlassSubmitButton(
            label = if (linkMinted) OnboardingCopy.CONTINUE else OnboardingCopy.SKIP,
            onClick = onContinue,
        )
    }
}

// Step 0 — Welcome: app name + one-line value prop, one primary action.
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
                ExpIcons.navBoards,
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

// Step 1 — Team (EXP-188): while getDefault resolves, a spinner; an account
// with a team auto-advances (the screen's LaunchedEffect); otherwise the
// create-or-join choice. Joining exits the wizard via the done flow.
@Composable
private fun TeamStep(
    preparing: Boolean,
    needsChoice: Boolean,
    submitting: Boolean,
    prepareError: String?,
    createError: String?,
    joinError: String?,
    onRetry: () -> Unit,
    onCreateTeam: (String) -> Unit,
    onJoinTeam: (String) -> Unit,
) {
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
            "Create a team, or join one with an invite link from a teammate.",
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
                TextButton(onClick = onRetry) { Text(OnboardingCopy.RETRY) }
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
                // EXP-698: the same two glass cards the zero-team empty state
                // raises in `TeamSetupSheet` — one composable, one copy.
                TeamSetupForm(
                    state = TeamSetupFormState(
                        busy = submitting,
                        createError = createError,
                        joinError = joinError,
                    ),
                    onCreate = onCreateTeam,
                    onJoin = onJoinTeam,
                )
            }
        }
    }
}

// Step 2 — Create your first board: name + optional repository (with inline
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
            OnboardingCopy.BOARD_TITLE,
            style = MaterialTheme.typography.headlineSmall,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(12.dp))
        Text(
            OnboardingCopy.BOARD_SUBTITLE,
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
                    TextButton(onClick = onRetry) { Text(OnboardingCopy.RETRY) }
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

// Step 5 — Done: confirmation, one action that drops into the app.
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
                ExpIcons.uiCheck,
                contentDescription = null,
                modifier = Modifier.size(28.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
        }
        Spacer(Modifier.height(24.dp))
        Text(
            OnboardingCopy.DONE_TITLE,
            style = MaterialTheme.typography.headlineSmall,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(12.dp))
        Text(
            OnboardingCopy.DONE_BODY,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(32.dp))
        GlassSubmitButton(label = OnboardingCopy.DONE_BUTTON, onClick = onFinish)
    }
}
