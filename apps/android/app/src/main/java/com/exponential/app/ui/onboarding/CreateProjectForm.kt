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
import com.exponential.app.data.api.ProjectRepositoryChoice
import com.exponential.app.ui.components.RepositorySelector
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.LabelPalette
import com.exponential.app.ui.theme.TextEmphasis

private const val DEFAULT_COLOR = "#6366f1"

/** First letters of each word, uppercased, capped at 5 (mirrors web `derivePrefix`). */
private fun derivePrefix(name: String): String =
    name.split(Regex("[\\s\\-_]+"))
        .mapNotNull { it.firstOrNull()?.toString() }
        .joinToString("")
        .uppercase()
        .take(5)

// Reusable create-project form: name (auto-derives the prefix until the user
// edits it), prefix, color, and the required backing repository. Used by
// onboarding step 2 and the empty-state create sheets. Owns its own
// [CreateProjectViewModel] for repo loading + the create call.
//
// `minimal` (the onboarding wizard, per the shared iOS/Android onboarding spec)
// reduces the form to name + repository: the prefix keeps auto-deriving from
// the name and the color stays at the default — both editable later.
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun CreateProjectForm(
    accountId: String,
    workspaceId: String,
    onCreated: (projectId: String) -> Unit,
    modifier: Modifier = Modifier,
    submitLabel: String = "Create project",
    minimal: Boolean = false,
    viewModel: CreateProjectViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    var name by remember { mutableStateOf("") }
    var prefix by remember { mutableStateOf("") }
    // Once the user hand-edits the prefix, stop auto-deriving from the name.
    var prefixEdited by remember { mutableStateOf(false) }
    var color by remember { mutableStateOf(DEFAULT_COLOR) }
    var repository by remember { mutableStateOf<ProjectRepositoryChoice?>(null) }

    LaunchedEffect(workspaceId) { viewModel.loadRepos(workspaceId) }

    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val canCreate = name.isNotBlank() && prefix.isNotBlank() && repository != null && !state.submitting

    Column(modifier = modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        OutlinedTextField(
            value = name,
            onValueChange = {
                name = it
                if (!prefixEdited) prefix = derivePrefix(it)
            },
            singleLine = true,
            label = { Text("Project name") },
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
            Text("Repository (required)", style = MaterialTheme.typography.labelMedium, color = secondary)
            RepositorySelector(
                accountId = accountId,
                repos = state.repos,
                loading = state.loadingRepos,
                selection = repository,
                onSelect = { repository = it },
            )
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
                val repo = repository ?: return@Button
                viewModel.create(workspaceId, name, prefix, color, repo, onCreated)
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
