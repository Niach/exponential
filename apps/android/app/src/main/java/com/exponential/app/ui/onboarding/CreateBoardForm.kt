package com.exponential.app.ui.onboarding

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.BoardRepositoryChoice
import com.exponential.app.ui.components.BoardRepoField
import com.exponential.app.ui.components.ColorPicker
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.IconPicker
import com.exponential.app.ui.components.TextFieldRow
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup

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
        private set

    /**
     * The board's own branch (EXP-712) — null follows the repo's default.
     * Only sent when a repository is selected; the server ignores it otherwise.
     */
    var branch by mutableStateOf<String?>(null)

    /** A repo change RESETS the branch — a pin belongs to ONE repo. */
    fun selectRepository(choice: BoardRepositoryChoice?) {
        if (choice == repository) return
        repository = choice
        branch = null
    }

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

    Column(modifier = modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        // EXP-862: ONE row — the icon picker, the colour picker and the name,
        // all at the same height (web `h-9`, desktop CTL_MD_H, iOS the same).
        // A board IS its glyph, its colour and its name; asking for them in
        // three stacked captioned blocks made a two-field form read as a page.
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
            ColorPicker(selected = form.color, onSelect = { form.color = it })
            // Every field is a GLASS ROW with its label inside it — no caption
            // floating above an input anywhere in this form.
            Column(modifier = Modifier.weight(1f).glassGroup()) {
                TextFieldRow(
                    label = "Name",
                    value = form.name,
                    onValueChange = {
                        form.name = it
                        if (!form.prefixEdited) form.prefix = derivePrefix(it)
                    },
                    placeholder = "e.g. Backend API",
                )
            }
        }

        if (!minimal) {
            Column(modifier = Modifier.fillMaxWidth().glassGroup()) {
                TextFieldRow(
                    label = "Prefix",
                    value = form.prefix,
                    onValueChange = {
                        form.prefixEdited = true
                        form.prefix = it.uppercase().take(4)
                    },
                    placeholder = "e.g. API",
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Characters),
                    textStyle = LocalTextStyle.current.copy(fontFamily = FontFamily.Monospace),
                )
            }
        }

        // Repository is ALWAYS optional on every board; the branch under it is
        // the board's own (EXP-712). A failed registry load rides the field's
        // error slot with a retry, so it can't read as "no repos connected"
        // (EXP-46).
        BoardRepoField(
            accountId = accountId,
            teamId = teamId,
            repos = state.repos,
            loading = state.loadingRepos,
            selection = form.repository,
            onSelect = { form.selectRepository(it) },
            branch = form.branch,
            onBranchChange = { form.branch = it },
            error = state.reposError,
            onRetry = { viewModel.loadRepos(teamId) },
        )

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
                        form.branch,
                        onCreated,
                    )
                },
            )
        }
    }
}
