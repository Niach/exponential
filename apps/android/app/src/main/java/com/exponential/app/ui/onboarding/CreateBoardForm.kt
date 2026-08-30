package com.exponential.app.ui.onboarding

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.BoardRepositoryChoice
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.IconPicker
import com.exponential.app.ui.components.RepositorySelector
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.LabelPalette
import com.exponential.app.ui.theme.TextEmphasis

private const val DEFAULT_COLOR = "#6366f1"

/**
 * The create-board form's own field state, hoisted out of [CreateBoardForm] so
 * a HOST can own the submit button (EXP-687: `CreateBoardSheet` pins it to the
 * sheet's bottom edge, while the onboarding wizard keeps its inline one).
 */
@Stable
class CreateBoardFormState {
    var name by mutableStateOf("")
    var prefix by mutableStateOf("")

    /** Once the user hand-edits the prefix, stop auto-deriving from the name. */
    var prefixEdited by mutableStateOf(false)
    var color by mutableStateOf(DEFAULT_COLOR)
    var iconName by mutableStateOf("square-kanban")
    var repository by mutableStateOf<BoardRepositoryChoice?>(null)

    /** Repo is always optional now, so creation only needs a name + prefix. */
    val canCreate: Boolean get() = name.isNotBlank() && prefix.isNotBlank()
}

@Composable
fun rememberCreateBoardFormState(): CreateBoardFormState = remember { CreateBoardFormState() }

/**
 * First letters of each word, uppercased, capped at 4 (the server cap,
 * REV-4) — mirrors web
 * `derivePrefix` (apps/web/src/lib/board.ts) byte-for-byte. The server
 * (boards.create, EXP-46 hardening) requires a letter-led alphanumeric
 * prefix, so symbol initials and leading digits are dropped; symbol/digit-only
 * names derive "" and the form requires a hand-typed prefix before submit.
 */
private fun derivePrefix(name: String): String =
    name.split(Regex("[\\s\\-_]+"))
        .mapNotNull { it.firstOrNull()?.toString() }
        .joinToString("")
        .replace(Regex("[^A-Za-z0-9]"), "")
        .replace(Regex("^[0-9]+"), "")
        .uppercase()
        .take(4)

// Reusable create-board form: one plain form of name, prefix, color, icon
// and an ALWAYS-optional repository (coding/PR affordances gate on its
// presence, never on a type). The create call sends `icon` (not the legacy
// `type`). Owns its own [CreateBoardViewModel] for repo loading + the
// create call.
//
// `minimal` (the onboarding wizard, per the shared iOS/Android onboarding spec)
// reduces the form to name + icon + repository: the prefix keeps auto-deriving
// from the name and the color stays at the default — all editable later.
//
// `showSubmit = false` hands the create button to the host (the sheet pins it);
// the host then drives it from the same [CreateBoardFormState] it passes in.
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun CreateBoardForm(
    accountId: String,
    teamId: String,
    onCreated: (boardId: String) -> Unit,
    modifier: Modifier = Modifier,
    submitLabel: String = "Create board",
    minimal: Boolean = false,
    showSubmit: Boolean = true,
    form: CreateBoardFormState = rememberCreateBoardFormState(),
    viewModel: CreateBoardViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    LaunchedEffect(teamId) {
        viewModel.loadRepos(teamId)
    }

    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val canCreate = form.canCreate && !state.submitting

    Column(modifier = modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        // Caption-labelled glass fields — iOS CreateBoardForm parity (EXP-577).
        // Name, with the icon picker LEFT of the input (EXP-584 — web, desktop
        // and iOS share the row). The shared curated picker (EXP-273/575) is
        // the same one an `icon` action input uses in the Start-coding sheet.
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Board name", style = MaterialTheme.typography.labelMedium, color = secondary)
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxWidth(),
            ) {
                IconPicker(
                    selected = form.iconName,
                    onSelect = { form.iconName = it },
                    accentColor = parseColor(form.color),
                )
                GlassTextField(
                    value = form.name,
                    onValueChange = {
                        form.name = it
                        if (!form.prefixEdited) form.prefix = derivePrefix(it)
                    },
                    singleLine = true,
                    placeholder = "e.g. Backend API",
                    modifier = Modifier.weight(1f),
                )
            }
        }

        if (!minimal) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Prefix", style = MaterialTheme.typography.labelMedium, color = secondary)
                GlassTextField(
                    value = form.prefix,
                    onValueChange = {
                        form.prefixEdited = true
                        form.prefix = it.uppercase().take(4)
                    },
                    singleLine = true,
                    placeholder = "e.g. API",
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Characters),
                    textStyle = LocalTextStyle.current.copy(fontFamily = FontFamily.Monospace),
                    modifier = Modifier.fillMaxWidth(),
                )
            }

            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Color", style = MaterialTheme.typography.labelMedium, color = secondary)
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    LabelPalette.colors.forEach { swatch ->
                        val selected = swatch.equals(form.color, ignoreCase = true)
                        Box(
                            modifier = Modifier
                                .size(28.dp)
                                .background(parseColor(swatch), CircleShape)
                                .then(
                                    if (selected) Modifier.border(2.dp, MaterialTheme.colorScheme.onSurface, CircleShape)
                                    else Modifier,
                                )
                                .clickable { form.color = swatch },
                            contentAlignment = Alignment.Center,
                        ) {
                            if (selected) {
                                Icon(ExpIcons.uiCheck, contentDescription = null, tint = Color.White, modifier = Modifier.size(16.dp))
                            }
                        }
                    }
                }
            }
        }

        // Repository is ALWAYS optional on every board.
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Repository (optional)", style = MaterialTheme.typography.labelMedium, color = secondary)
            // A failed registry load must not read as "no repos connected" —
            // show the error with a retry instead of the selector's empty
            // state (EXP-46).
            val reposError = state.reposError
            if (reposError != null) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        reposError,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                        modifier = Modifier.weight(1f),
                    )
                    TextButton(onClick = { viewModel.loadRepos(teamId) }) {
                        Text("Retry")
                    }
                }
            } else {
                RepositorySelector(
                    accountId = accountId,
                    teamId = teamId,
                    repos = state.repos,
                    loading = state.loadingRepos,
                    selection = form.repository,
                    onSelect = { form.repository = it },
                )
            }
        }

        state.error?.let { message ->
            Text(message, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
        }
        state.limitError?.let { message ->
            Row(
                verticalAlignment = Alignment.Top,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Icon(ExpIcons.uiInfo, contentDescription = null, modifier = Modifier.size(16.dp), tint = MaterialTheme.colorScheme.primary)
                Text(message, style = MaterialTheme.typography.bodySmall, color = secondary)
            }
        }

        if (showSubmit) {
            GlassSubmitButton(
                label = if (state.submitting) "Creating…" else submitLabel,
                enabled = canCreate,
                onClick = {
                    // Repo is optional — send whatever (if any) is selected.
                    viewModel.create(
                        teamId,
                        form.name,
                        form.prefix,
                        form.color,
                        form.iconName,
                        form.repository,
                        onCreated,
                    )
                },
            )
        }
    }
}
