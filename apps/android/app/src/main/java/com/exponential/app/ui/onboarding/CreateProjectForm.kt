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
import androidx.compose.material3.Switch
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
import com.exponential.app.data.api.ProjectRepositoryChoice
import com.exponential.app.ui.components.ProjectIconGlyphs
import com.exponential.app.ui.components.ProjectTemplate
import com.exponential.app.ui.components.ProjectTemplates
import com.exponential.app.ui.components.RepositorySelector
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.LabelPalette
import com.exponential.app.ui.theme.TextEmphasis

private const val DEFAULT_COLOR = "#6366f1"

/**
 * First letters of each word, uppercased, capped at 5 — mirrors web
 * `derivePrefix` (apps/web/src/lib/project.ts) byte-for-byte. The server
 * (projects.create, EXP-46 hardening) requires a letter-led alphanumeric
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

// Reusable create-project form. Since the project-type collapse (EXP-121) every
// project is the same shape: a template quickstart pre-sets the public toggle,
// the stored icon and whether the repo section leads — then one form (name,
// prefix, color, icon, optional repository, public toggle). A repository is
// ALWAYS optional; coding/PR affordances gate on its presence, never on type.
// The create call sends `isPublic` + `icon` (not the legacy `type`). Owns its
// own [CreateProjectViewModel] for repo loading + the create call.
//
// `minimal` (the onboarding wizard, per the shared iOS/Android onboarding spec)
// reduces the form to template + name + repository: the prefix keeps
// auto-deriving from the name, the color/icon stay at the template default and
// the public toggle is hidden — all editable later.
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
    // The selected template only seeds isPublic/icon/repo-visibility; those are
    // then independently editable, so they live in their own state.
    var template by remember { mutableStateOf(ProjectTemplates.first()) }
    var isPublic by remember { mutableStateOf(template.isPublic) }
    var iconName by remember { mutableStateOf(template.iconName) }
    var showRepo by remember { mutableStateOf(template.suggestsRepo) }
    var repository by remember { mutableStateOf<ProjectRepositoryChoice?>(null) }

    LaunchedEffect(workspaceId) { viewModel.loadRepos(workspaceId) }

    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    // Repo is always optional now, so creation only needs a name + prefix.
    val canCreate = name.isNotBlank() && prefix.isNotBlank() && !state.submitting

    Column(modifier = modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Start from", style = MaterialTheme.typography.labelMedium, color = secondary)
            ProjectTemplates.forEach { info ->
                ProjectTemplateCard(
                    info = info,
                    selected = info === template,
                    onClick = {
                        template = info
                        isPublic = info.isPublic
                        iconName = info.iconName
                        showRepo = info.suggestsRepo
                    },
                )
            }
        }

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

            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Icon", style = MaterialTheme.typography.labelMedium, color = secondary)
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    ProjectIconGlyphs.forEach { (glyphName, glyph) ->
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
        }

        // Repository is optional on every project. The section leads for the Dev
        // template; other templates can reveal it with the connect button.
        if (showRepo) {
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
                        TextButton(onClick = { viewModel.loadRepos(workspaceId) }) {
                            Text("Retry")
                        }
                    }
                } else {
                    RepositorySelector(
                        accountId = accountId,
                        workspaceId = workspaceId,
                        repos = state.repos,
                        loading = state.loadingRepos,
                        selection = repository,
                        onSelect = { repository = it },
                    )
                }
            }
        } else if (!minimal) {
            TextButton(onClick = { showRepo = true }) {
                Text("Connect a GitHub repository")
            }
        }

        if (!minimal) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Public board", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurface)
                    Text(
                        "Anyone with the link can read issues and comments.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
                Spacer(Modifier.width(8.dp))
                Switch(checked = isPublic, onCheckedChange = { isPublic = it })
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
                // Repo is optional — send whatever (if any) is selected, only
                // meaningful while the repo section is shown.
                val repo = if (showRepo) repository else null
                viewModel.create(workspaceId, name, prefix, color, isPublic, iconName, repo, onCreated)
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

// One selectable template card: icon + label + one-line description, with a
// primary-colored border + check when selected (mirrors the color swatches).
@Composable
private fun ProjectTemplateCard(
    info: ProjectTemplate,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val borderColor =
        if (selected) MaterialTheme.colorScheme.primary
        else MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .border(
                if (selected) 2.dp else 1.dp,
                borderColor,
                RoundedCornerShape(12.dp),
            )
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            info.icon,
            contentDescription = null,
            tint = if (selected) MaterialTheme.colorScheme.primary
            else MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.size(22.dp),
        )
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                info.label,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                info.description,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        if (selected) {
            Spacer(Modifier.width(8.dp))
            Icon(
                Icons.Filled.Check,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(18.dp),
            )
        }
    }
}
