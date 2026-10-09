package com.exponential.app.ui.onboarding

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.TextButton
import androidx.compose.ui.Alignment
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import com.exponential.app.ui.components.ExponentialMark
import com.exponential.app.ui.components.GlassOAuthButton
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.theme.TextEmphasis

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

/** The create-or-join form's three pages: the choice, then one form each. */
private enum class TeamSetupPage { Choice, Create, Join }

/**
 * The ONE create-or-join team form (EXP-188, EXP-698), shaped like web's
 * wizard (P3): a CHOICE page — optionally under the brand mark and "Welcome
 * to Exponential" — with two outline buttons, "Create a team" and "Join a
 * team", each pushing its own page (title, field, error, primary, Back). No
 * subtitles anywhere. Signups get no auto-created team, so this is what the
 * first-run wizard's team step and the zero-team sheet both render.
 *
 * Only one submit can be in flight at a time ([TeamSetupFormState.busy]).
 */
@Composable
fun TeamSetupForm(
    state: TeamSetupFormState,
    onCreate: (String) -> Unit,
    onJoin: (String) -> Unit,
    modifier: Modifier = Modifier,
    /** The wizard's brand heading over the choice page; the sheet has its own title. */
    showWelcome: Boolean = false,
) {
    var page by remember { mutableStateOf(TeamSetupPage.Choice) }
    var teamName by remember { mutableStateOf("") }
    var inviteInput by remember { mutableStateOf("") }

    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        when (page) {
            TeamSetupPage.Choice -> {
                if (showWelcome) {
                    ExponentialMark(size = 48.dp)
                    Spacer(Modifier.height(4.dp))
                    Text(
                        TeamSetupCopy.WELCOME_TITLE,
                        style = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.SemiBold),
                        color = MaterialTheme.colorScheme.onSurface,
                        textAlign = TextAlign.Center,
                    )
                    Spacer(Modifier.height(12.dp))
                }
                GlassOAuthButton(
                    label = TeamSetupCopy.CREATE_TITLE,
                    onClick = { page = TeamSetupPage.Create },
                    modifier = Modifier.testTag("team-setup-create"),
                ) {
                    Icon(ExpIcons.uiAdd, contentDescription = null, modifier = Modifier.size(16.dp))
                }
                GlassOAuthButton(
                    label = TeamSetupCopy.JOIN_TITLE,
                    onClick = { page = TeamSetupPage.Join },
                    modifier = Modifier.testTag("team-setup-join"),
                ) {
                    Icon(ExpIcons.uiLink, contentDescription = null, modifier = Modifier.size(16.dp))
                }
            }
            TeamSetupPage.Create -> {
                PageTitle(TeamSetupCopy.CREATE_TITLE)
                GlassTextField(
                    value = teamName,
                    onValueChange = { teamName = it },
                    singleLine = true,
                    placeholder = TeamSetupCopy.CREATE_PLACEHOLDER,
                    enabled = !state.busy,
                    modifier = Modifier.fillMaxWidth().testTag("team-setup-name"),
                )
                PageError(state.createError)
                GlassSubmitButton(
                    label = if (state.busy) "Creating…" else TeamSetupCopy.CREATE_BUTTON,
                    onClick = { onCreate(teamName) },
                    enabled = !state.busy && teamName.isNotBlank(),
                )
                BackLink(enabled = !state.busy) { page = TeamSetupPage.Choice }
            }
            TeamSetupPage.Join -> {
                PageTitle(TeamSetupCopy.JOIN_TITLE)
                GlassTextField(
                    value = inviteInput,
                    onValueChange = { inviteInput = it },
                    singleLine = true,
                    placeholder = TeamSetupCopy.JOIN_PLACEHOLDER,
                    enabled = !state.busy,
                    // A token is neither a sentence nor a word — autocapitalizing
                    // or "correcting" a pasted link silently breaks the accept.
                    keyboardOptions = KeyboardOptions(
                        capitalization = KeyboardCapitalization.None,
                        autoCorrectEnabled = false,
                    ),
                    modifier = Modifier.fillMaxWidth().testTag("team-setup-invite"),
                )
                PageError(state.joinError)
                GlassSubmitButton(
                    label = if (state.busy) "Joining…" else TeamSetupCopy.JOIN_BUTTON,
                    onClick = { onJoin(inviteInput) },
                    enabled = !state.busy && inviteInput.isNotBlank(),
                )
                BackLink(enabled = !state.busy) { page = TeamSetupPage.Choice }
            }
        }
    }
}

/** The team step's words (web `wizard.tsx` ChoiceStep / CreateTeamStep / JoinStep). */
object TeamSetupCopy {
    const val WELCOME_TITLE = "Welcome to Exponential"
    const val CREATE_TITLE = "Create a team"
    const val CREATE_PLACEHOLDER = "e.g. Acme Inc"
    const val CREATE_BUTTON = "Create team"
    const val JOIN_TITLE = "Join a team"
    const val JOIN_PLACEHOLDER = "Paste an invite link"
    const val JOIN_BUTTON = "Continue"
    const val BACK = "Back"
}

@Composable
private fun PageTitle(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.SemiBold),
        color = MaterialTheme.colorScheme.onSurface,
        textAlign = TextAlign.Center,
        modifier = Modifier.padding(bottom = 4.dp),
    )
}

@Composable
private fun BackLink(enabled: Boolean, onClick: () -> Unit) {
    TextButton(
        onClick = onClick,
        enabled = enabled,
        colors = ButtonDefaults.textButtonColors(
            contentColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        ),
    ) {
        Text(TeamSetupCopy.BACK)
    }
}

/** Each page owns its own error line, under its own field. */
@Composable
private fun PageError(message: String?) {
    if (message == null) return
    Text(
        message,
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.error.copy(alpha = 0.8f),
        modifier = Modifier.fillMaxWidth(),
    )
}
