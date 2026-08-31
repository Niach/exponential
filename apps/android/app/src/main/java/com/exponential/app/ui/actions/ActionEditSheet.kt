package com.exponential.app.ui.actions

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.IconPicker
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.StartCodingSheetViewModel
import com.exponential.app.ui.theme.TextEmphasis

// The action editor (EXP-694): mobile stopped being view + run only, so an
// action's icon, name, description, repository and markdown PROMPT are all
// editable here — the same field set as the web dialog
// (components/action-editor-dialog.tsx) and the desktop editor, on the same
// grouped card stack the create sheet uses (S7: one control set everywhere).
//
// The prompt is the reason this needs an API at all: the synced `actions`
// shape excludes the ≤64KB body, so the sheet fetches the row on open
// (`actions.get`) and writes it back with `actions.update`. Writes are
// owner-gated server-side; a member gets the same sheet read-only.

// Mirrors the router's zod limits (apps/web/src/lib/trpc/actions.ts) so a
// too-long value is impossible rather than a round-trip refusal.
private const val MAX_ACTION_NAME = 255
private const val MAX_ACTION_DESCRIPTION = 2048
private const val MAX_ACTION_BODY = 64 * 1024

@Composable
fun ActionEditSheet(
    actionId: String,
    onDismiss: () -> Unit,
    viewModel: ActionEditViewModel = hiltViewModel(),
    dataViewModel: StartCodingSheetViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val isOwner by viewModel.isTeamOwner.collectAsStateWithLifecycle()
    val teamRepos by dataViewModel.repos.collectAsStateWithLifecycle()

    // One fetch per PRESENTATION (iOS EditActionSheet's `.task`): the model is
    // nav-entry-scoped and outlives the sheet, so a cached row would re-show a
    // body a teammate has since changed — and strand a failed first load with
    // no retry. Disposal clears it again, so the next open never seeds from
    // the previous fetch.
    LaunchedEffect(actionId) { viewModel.load(actionId) }
    DisposableEffect(Unit) { onDispose { viewModel.reset() } }

    var name by remember { mutableStateOf("") }
    var description by remember { mutableStateOf("") }
    var icon by remember { mutableStateOf("") }
    var repoId by remember { mutableStateOf("") }
    var body by remember { mutableStateOf("") }

    // Seed the form ONCE per fetched row: `state.action` is rewritten by a
    // successful save too, and re-seeding then would stomp nothing but would
    // also fight a user who kept typing.
    var seededId by remember { mutableStateOf<String?>(null) }
    val loaded = state.action
    LaunchedEffect(loaded?.id) {
        val row = loaded ?: return@LaunchedEffect
        if (seededId == row.id) return@LaunchedEffect
        seededId = row.id
        name = row.name
        description = row.description.orEmpty()
        icon = row.icon.orEmpty()
        repoId = row.repositoryId.orEmpty()
        body = row.body
    }

    val editable = isOwner && loaded != null && !state.saving
    val dirty = loaded != null && (
        name.trim() != loaded.name ||
            description.trim() != loaded.description.orEmpty().trim() ||
            icon != loaded.icon.orEmpty() ||
            repoId != loaded.repositoryId.orEmpty() ||
            body != loaded.body
        )
    val canSave = editable && dirty && name.isNotBlank() && body.isNotBlank()

    GlassSheet(
        // A member can read the prompt but not change it — the title says so
        // instead of the sheet pretending to be an editor (web parity).
        title = if (isOwner) "Edit action" else "Action",
        onDismiss = onDismiss,
        modifier = Modifier.testTag("action-edit-sheet"),
        height = SheetHeight.Full,
        primaryAction = if (isOwner) {
            SheetPrimaryAction(
                label = "Save changes",
                enabled = canSave,
                loading = state.saving,
                onClick = save@{
                    val row = loaded ?: return@save
                    viewModel.save(
                        actionId = row.id,
                        name = name,
                        description = description,
                        icon = icon,
                        repositoryId = repoId,
                        body = body,
                        onDone = onDismiss,
                    )
                },
            )
        } else {
            null
        },
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState()),
        ) {
            // Icon + name on one row — byte-for-byte the create sheet's
            // identity row (S7), and the same shape web/desktop/iOS render.
            OptionGroup {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (editable) {
                        IconPicker(selected = icon, onSelect = { icon = it }, allowsNone = true)
                    } else {
                        // Read-only: the glyph without the picker's tap target.
                        Box(modifier = Modifier.size(36.dp), contentAlignment = Alignment.Center) {
                            Icon(
                                icon.takeIf { it.isNotEmpty() }?.let { ExpIcons.byName(it) }
                                    ?: ExpIcons.actionDefault,
                                contentDescription = null,
                                modifier = Modifier.size(20.dp),
                                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            )
                        }
                    }
                    Spacer(Modifier.width(12.dp))
                    GlassTextField(
                        value = name,
                        onValueChange = { name = it.take(MAX_ACTION_NAME) },
                        modifier = Modifier.weight(1f),
                        placeholder = "Name",
                        singleLine = true,
                        enabled = editable,
                    )
                }
            }
            Spacer(Modifier.height(8.dp))

            // Description and prompt live INSIDE the card stack (EXP-694): the
            // field's own fill/hairline would double the group's chrome, so
            // both drop it and the placeholder carries the title — hence the
            // bare "Name"/"Description"/"Prompt", the same three strings on
            // every client (web action-editor-dialog.tsx, the desktop editor,
            // iOS EditActionSheet).
            OptionGroup {
                GlassTextField(
                    value = description,
                    onValueChange = { description = it.take(MAX_ACTION_DESCRIPTION) },
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = "Description",
                    minLines = 2,
                    enabled = editable,
                    bordered = false,
                )
            }
            Spacer(Modifier.height(8.dp))

            OptionGroup {
                PickerRow(
                    label = "Repository",
                    value = when {
                        repoId.isEmpty() -> "None"
                        else -> teamRepos.firstOrNull { it.id == repoId }?.fullName ?: repoId
                    },
                    options = listOf("") + teamRepos.map { it.id },
                    selected = repoId,
                    optionLabel = { id ->
                        if (id.isEmpty()) {
                            "None"
                        } else {
                            teamRepos.firstOrNull { it.id == id }?.fullName ?: id
                        }
                    },
                    onSelect = { repoId = it },
                    enabled = editable,
                )
            }
            Spacer(Modifier.height(4.dp))
            Text(
                "With a repository the run clones it first; without one the agent works in a " +
                    "scratch directory.",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.padding(horizontal = 32.dp, vertical = 2.dp),
            )
            Spacer(Modifier.height(8.dp))

            // The prompt: monospace, tall, and parked while `actions.get` is
            // still in flight (the body is the one field sync can't hand us).
            OptionGroup {
                GlassTextField(
                    value = body,
                    onValueChange = { body = it.take(MAX_ACTION_BODY) },
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = if (state.loading) {
                        "Loading prompt…"
                    } else {
                        "Prompt"
                    },
                    minLines = 8,
                    enabled = editable,
                    bordered = false,
                    textStyle = MaterialTheme.typography.bodySmall.copy(
                        fontFamily = FontFamily.Monospace,
                    ),
                )
            }

            val error = state.error
            if (error != null) {
                Spacer(Modifier.height(4.dp))
                Text(
                    error,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(horizontal = 32.dp),
                )
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}
