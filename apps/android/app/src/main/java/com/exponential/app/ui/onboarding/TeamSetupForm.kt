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
import com.exponential.app.ui.components.FieldError
import com.exponential.app.ui.components.ExponentialMark
import com.exponential.app.ui.theme.GlassTokens
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.width
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import com.exponential.app.ui.components.GlassPill
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
                // EXP-1554: web's two standard outline buttons (h-9, 10dp
                // apart), not the taller provider-button recipe.
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    ChoiceButton(
                        label = TeamSetupCopy.CREATE_TITLE,
                        icon = ExpIcons.uiAdd,
                        onClick = { page = TeamSetupPage.Create },
                        modifier = Modifier.testTag("team-setup-create"),
                    )
                    ChoiceButton(
                        label = TeamSetupCopy.JOIN_TITLE,
                        icon = ExpIcons.uiLink,
                        onClick = { page = TeamSetupPage.Join },
                        modifier = Modifier.testTag("team-setup-join"),
                    )
                }
            }
            TeamSetupPage.Create -> {
                PageTitle(TeamSetupCopy.CREATE_TITLE)
                FieldLabel(TeamSetupCopy.CREATE_LABEL)
                GlassTextField(
                    value = teamName,
                    onValueChange = { teamName = it },
                    singleLine = true,
                    placeholder = TeamSetupCopy.CREATE_PLACEHOLDER,
                    enabled = !state.busy,
                    modifier = Modifier.fillMaxWidth().testTag("team-setup-name"),
                )
                PageError(state.createError)
                PageFooter(onBack = { page = TeamSetupPage.Choice }, busy = state.busy) {
                    GlassPill(
                        label = if (state.busy) "Creating…" else TeamSetupCopy.CREATE_BUTTON,
                        onClick = { onCreate(teamName) },
                        enabled = !state.busy && teamName.isNotBlank(),
                        primary = true,
                    )
                }
            }
            TeamSetupPage.Join -> {
                PageTitle(TeamSetupCopy.JOIN_TITLE)
                FieldLabel(TeamSetupCopy.JOIN_LABEL)
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
                PageFooter(onBack = { page = TeamSetupPage.Choice }, busy = state.busy) {
                    GlassPill(
                        label = if (state.busy) "Joining…" else TeamSetupCopy.JOIN_BUTTON,
                        onClick = { onJoin(inviteInput) },
                        enabled = !state.busy && inviteInput.isNotBlank(),
                        primary = true,
                    )
                }
            }
        }
    }
}

/** The team step's words (web `wizard.tsx` ChoiceStep / CreateTeamStep / JoinStep). */
object TeamSetupCopy {
    const val WELCOME_TITLE = "Welcome to Exponential"
    const val CREATE_TITLE = "Create a team"
    const val CREATE_LABEL = "Team name"
    const val CREATE_PLACEHOLDER = "e.g. Acme Inc"
    const val CREATE_BUTTON = "Create team"
    const val JOIN_TITLE = "Join a team"
    const val JOIN_LABEL = "Invite link"
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

/** The field's label above its input (web `<Label>`, iOS `fieldLabel`). */
@Composable
private fun FieldLabel(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.Medium),
        color = MaterialTheme.colorScheme.onSurface,
        modifier = Modifier.fillMaxWidth(),
    )
}

/** Web's step footer ×4: the ghost "← Back" leading, the page's compact
 *  primary trailing (hugging its label), in ONE row. */
@Composable
private fun PageFooter(onBack: () -> Unit, busy: Boolean, primary: @Composable () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        modifier = Modifier.fillMaxWidth().padding(top = 4.dp),
    ) {
        TextButton(
            onClick = onBack,
            enabled = !busy,
            colors = ButtonDefaults.textButtonColors(
                contentColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            ),
        ) {
            Icon(ExpIcons.uiBack, contentDescription = null, modifier = Modifier.size(16.dp))
            Spacer(Modifier.width(6.dp))
            Text(TeamSetupCopy.BACK)
        }
        Spacer(Modifier.weight(1f))
        primary()
    }
}

/** Web's standard outline button on the choice page: full width, h-9-ish
 *  (40dp), glass fill + hairline, glyph then label. */
@Composable
private fun ChoiceButton(label: String, icon: ImageVector, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val shape = GlassTokens.ButtonShape
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(GlassTokens.CardFill, shape)
            .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, shape)
            .clickable(onClick = onClick)
            .padding(vertical = 10.dp),
    ) {
        Icon(icon, contentDescription = null, tint = Color.White, modifier = Modifier.size(16.dp))
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.Medium),
            color = Color.White,
        )
    }
}

/** Each page owns its own error line, under its own field (invalid input, FEED-79). */
@Composable
private fun PageError(message: String?) {
    FieldError(message, modifier = Modifier.fillMaxWidth())
}
