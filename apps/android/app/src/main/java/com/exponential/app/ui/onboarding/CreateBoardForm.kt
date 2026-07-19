package com.exponential.app.ui.onboarding

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AutoAwesome
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.BoardRepositoryChoice
import com.exponential.app.ui.components.BoardIconGlyphs
import com.exponential.app.ui.components.RepositorySelector
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.LabelPalette
import com.exponential.app.ui.theme.TextEmphasis

private const val DEFAULT_COLOR = "#6366f1"

/**
 * First letters of each word, uppercased, capped at 5 — mirrors web
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
        .take(5)

// Reusable create-board form: one plain form of name, prefix, color, icon
// and an ALWAYS-optional repository (coding/PR affordances gate on its
// presence, never on a type). The create call sends `icon` (not the legacy
// `type`). Owns its own [CreateBoardViewModel] for repo loading + the
// create call.
//
// `minimal` (the onboarding wizard, per the shared iOS/Android onboarding spec)
// reduces the form to name + icon + repository: the prefix keeps auto-deriving
// from the name and the color stays at the default — all editable later.
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun CreateBoardForm(
    accountId: String,
    teamId: String,
    onCreated: (boardId: String) -> Unit,
    modifier: Modifier = Modifier,
    submitLabel: String = "Create board",
    minimal: Boolean = false,
    viewModel: CreateBoardViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    var name by remember { mutableStateOf("") }
    var prefix by remember { mutableStateOf("") }
    // Once the user hand-edits the prefix, stop auto-deriving from the name.
    var prefixEdited by remember { mutableStateOf(false) }
    var color by remember { mutableStateOf(DEFAULT_COLOR) }
    var iconName by remember { mutableStateOf("square-kanban") }
    var repository by remember { mutableStateOf<BoardRepositoryChoice?>(null) }

    LaunchedEffect(teamId) {
        viewModel.loadRepos(teamId)
    }

    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    // Repo is always optional now, so creation only needs a name + prefix.
    val canCreate = name.isNotBlank() && prefix.isNotBlank() && !state.submitting

    Column(modifier = modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        OutlinedTextField(
            value = name,
            onValueChange = {
                name = it
                if (!prefixEdited) prefix = derivePrefix(it)
            },
            singleLine = true,
            label = { Text("Board name") },
            placeholder = { Text("e.g. Backend API") },
            modifier = Modifier.fillMaxWidth(),
        )

        if (!minimal) {
            OutlinedTextField(
                value = prefix,
                onValueChange = {
                    prefixEdited = true
                    prefix = it.uppercase().take(10)
                },
                singleLine = true,
                label = { Text("Prefix") },
                placeholder = { Text("e.g. API") },
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Characters),
                modifier = Modifier.fillMaxWidth(),
            )

            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Color", style = MaterialTheme.typography.labelMedium, color = secondary)
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    LabelPalette.colors.forEach { swatch ->
                        val selected = swatch.equals(color, ignoreCase = true)
                        Box(
                            modifier = Modifier
                                .size(28.dp)
                                .background(parseColor(swatch), CircleShape)
                                .then(
                                    if (selected) Modifier.border(2.dp, MaterialTheme.colorScheme.onSurface, CircleShape)
                                    else Modifier,
                                )
                                .clickable { color = swatch },
                            contentAlignment = Alignment.Center,
                        ) {
                            if (selected) {
                                Icon(Icons.Filled.Check, contentDescription = null, tint = Color.White, modifier = Modifier.size(16.dp))
                            }
                        }
                    }
                }
            }
        }

        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Icon", style = MaterialTheme.typography.labelMedium, color = secondary)
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                BoardIconGlyphs.forEach { (glyphName, glyph) ->
                    val selected = glyphName == iconName
                    Box(
                        modifier = Modifier
                            .size(36.dp)
                            .border(
                                if (selected) 2.dp else 1.dp,
                                if (selected) parseColor(color)
                                else MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
                                RoundedCornerShape(10.dp),
                            )
                            .clickable { iconName = glyphName },
                        contentAlignment = Alignment.Center,
                    ) {
                        Icon(
                            glyph,
                            contentDescription = glyphName,
                            tint = if (selected) parseColor(color)
                            else MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            modifier = Modifier.size(20.dp),
                        )
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
                    selection = repository,
                    onSelect = { repository = it },
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
                Icon(Icons.Filled.AutoAwesome, contentDescription = null, modifier = Modifier.size(16.dp), tint = MaterialTheme.colorScheme.primary)
                Text(message, style = MaterialTheme.typography.bodySmall, color = secondary)
            }
        }

        Button(
            onClick = {
                // Repo is optional — send whatever (if any) is selected.
                viewModel.create(teamId, name, prefix, color, iconName, repository, onCreated)
            },
            enabled = canCreate,
            modifier = Modifier.fillMaxWidth(),
        ) {
            if (state.submitting) {
                CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp, color = MaterialTheme.colorScheme.onPrimary)
                Spacer(Modifier.width(8.dp))
                Text("Creating…")
            } else {
                Text(submitLabel)
            }
        }
    }
}
