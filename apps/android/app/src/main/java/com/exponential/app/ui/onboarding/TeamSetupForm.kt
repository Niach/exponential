package com.exponential.app.ui.onboarding

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard

/**
 * What the create-or-join form's OWNER (the onboarding wizard, or
 * [TeamSetupSheet]) knows: whether a call is in flight and the two cards'
 * errors. The typed text lives in the form itself — a caller only ever gets
 * the submitted string back.
 */
data class TeamSetupFormState(
    val busy: Boolean = false,
    val createError: String? = null,
    val joinError: String? = null,
)

/**
 * The ONE create-or-join team form (EXP-188, EXP-698) — a 1:1 port of iOS
 * `TeamSetupView`: two glass cards, each with its own description, field,
 * error line and full-width submit. Signups get no auto-created team, so this
 * is what the first-run wizard's team step and the zero-team empty state on
 * the Issues home both render.
 *
 * Only one submit can be in flight at a time ([TeamSetupFormState.busy]
 * disables both), and the busy LABEL lands on the card that was actually
 * submitted — the form remembers which one, so a create never reads
 * "Joining…".
 */
@Composable
fun TeamSetupForm(
    state: TeamSetupFormState,
    onCreate: (String) -> Unit,
    onJoin: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var teamName by remember { mutableStateOf("") }
    var inviteInput by remember { mutableStateOf("") }
    var pending by remember { mutableStateOf<PendingSubmit?>(null) }
    // A finished call (success or failure) releases the label.
    LaunchedEffect(state.busy) { if (!state.busy) pending = null }

    val creating = state.busy && pending == PendingSubmit.Create
    val joining = state.busy && pending == PendingSubmit.Join

    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        // Create a team
        Column(
            modifier = Modifier.fillMaxWidth().glassCard().padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            CardTitle("Create a team")
            CardDescription("Start fresh. You become the owner and can invite teammates later.")
            GlassTextField(
                value = teamName,
                onValueChange = { teamName = it },
                singleLine = true,
                placeholder = "e.g. Acme Inc",
                enabled = !state.busy,
                modifier = Modifier.fillMaxWidth(),
            )
            CardError(state.createError)
            // EXP-698: ONE primary chrome on both cards. These were a Material
            // `Button` and an `OutlinedButton`, so the two disabled first-run
            // actions sitting one above the other rendered as two different
            // controls — one filled, one an empty outline.
            GlassSubmitButton(
                label = if (creating) "Creating…" else "Create team",
                onClick = {
                    pending = PendingSubmit.Create
                    onCreate(teamName)
                },
                enabled = !state.busy && teamName.isNotBlank(),
            )
        }

        // Join a team
        Column(
            modifier = Modifier.fillMaxWidth().glassCard().padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            CardTitle("Join a team")
            CardDescription("Ask a teammate for an invite link and paste it below.")
            GlassTextField(
                value = inviteInput,
                onValueChange = { inviteInput = it },
                singleLine = true,
                placeholder = "Invite link or token",
                enabled = !state.busy,
                // A token is neither a sentence nor a word — autocapitalizing
                // or "correcting" a pasted link silently breaks the accept.
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.None,
                    autoCorrectEnabled = false,
                ),
                modifier = Modifier.fillMaxWidth(),
            )
            CardError(state.joinError)
            GlassSubmitButton(
                label = if (joining) "Joining…" else "Join team",
                onClick = {
                    pending = PendingSubmit.Join
                    onJoin(inviteInput)
                },
                enabled = !state.busy && inviteInput.isNotBlank(),
            )
        }
    }
}

/** Which card is waiting on its call — drives the busy label, nothing else. */
private enum class PendingSubmit { Create, Join }

@Composable
private fun CardTitle(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.titleSmall.copy(fontWeight = FontWeight.SemiBold),
        color = MaterialTheme.colorScheme.onSurface,
    )
}

@Composable
private fun CardDescription(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
    )
}

/** Each card owns its own error line, under its own field (iOS parity). */
@Composable
private fun CardError(message: String?) {
    if (message == null) return
    Text(
        message,
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.error.copy(alpha = 0.8f),
    )
}
