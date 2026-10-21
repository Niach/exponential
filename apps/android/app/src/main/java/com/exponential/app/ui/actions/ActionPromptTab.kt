package com.exponential.app.ui.actions

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.PromptDto
import com.exponential.app.data.api.PromptFields
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.Prompts
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.IconPicker
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis

// The action editor (EXP-694; EXP-1291/EXP-1296: an action is a SAVED
// PROMPT, so its page IS this editor). NEW-2: prompts replace actions — icon
// + name and the markdown PROMPT only (no composer hint, no inputs), on the
// grouped card stack every form uses. [ActionPromptForm] is the one form —
// the page edits a fetched row with it, `New action` creates with it.
//
// The synced `prompts` shape excludes the ≤64KB body, so the page fetches
// the row when it opens (`prompts.get`) and writes it back with
// `prompts.update`. Writes are owner-gated server-side; a member gets the
// same fields read-only, no Save, no Delete.

// Mirrors the router's zod limits (apps/web/src/lib/trpc/prompts.ts) so a
// too-long value is impossible rather than a round-trip refusal.
private const val MAX_ACTION_NAME = 255
private const val MAX_ACTION_BODY = 64 * 1024

/** The editor's field state (one per page / create sheet). */
@Stable
class ActionFormState {
    var name by mutableStateOf("")
    var icon by mutableStateOf("")
    var body by mutableStateOf("")

    fun seed(row: PromptDto) {
        name = row.name
        icon = row.icon.orEmpty()
        body = row.body
    }

    fun fields(): PromptFields = PromptFields(
        name = name.trim(),
        icon = icon.takeIf { it.isNotEmpty() },
        body = body,
    )

    fun differsFrom(row: PromptDto): Boolean =
        name.trim() != row.name ||
            icon != row.icon.orEmpty() ||
            body != row.body

    val valid: Boolean
        get() = name.isNotBlank() && body.isNotBlank()
}

@Composable
fun rememberActionFormState(): ActionFormState = remember { ActionFormState() }

/**
 * THE action form: identity row, prompt. [bodyLoading]
 * parks the prompt (the `prompts.get` fetch); [bodySlot] replaces it (the
 * fetch's empty state + Retry).
 */
@Composable
internal fun ActionPromptForm(
    form: ActionFormState,
    editable: Boolean,
    modifier: Modifier = Modifier,
    bodyLoading: Boolean = false,
    bodySlot: (@Composable () -> Unit)? = null,
) {
    Column(modifier = modifier) {
        // Icon + name on one row (S7) — the shape web/desktop/iOS render.
        OptionGroup {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (editable) {
                    IconPicker(selected = form.icon, onSelect = { form.icon = it }, allowsNone = true)
                } else {
                    Box(modifier = Modifier.size(36.dp), contentAlignment = Alignment.Center) {
                        Icon(
                            form.icon.takeIf { it.isNotEmpty() }?.let { ExpIcons.byName(it) }
                                ?: ExpIcons.actionDefault,
                            contentDescription = null,
                            modifier = Modifier.size(20.dp),
                            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        )
                    }
                }
                Spacer(Modifier.width(12.dp))
                GlassTextField(
                    value = form.name,
                    onValueChange = { form.name = it.take(MAX_ACTION_NAME) },
                    modifier = Modifier.weight(1f).testTag("action-edit-name"),
                    placeholder = "Name",
                    singleLine = true,
                    enabled = editable,
                )
            }
        }
        Spacer(Modifier.height(8.dp))

        // The prompt: monospace and tall.
        OptionGroup {
            when {
                // EXP-1275: the fetch = skeleton rows, never "Loading prompt…".
                bodyLoading -> SkeletonRows(dot = false, modifier = Modifier.padding(vertical = 4.dp))
                bodySlot != null -> bodySlot()
                else -> GlassTextField(
                    value = form.body,
                    onValueChange = { form.body = it.take(MAX_ACTION_BODY) },
                    modifier = Modifier.fillMaxWidth().testTag("action-edit-body"),
                    placeholder = "Prompt",
                    minLines = 8,
                    enabled = editable,
                    bordered = false,
                    textStyle = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                )
            }
        }
    }
}

/** The action page's editor: fetch, edit, Save changes, Delete (owner). */
@Composable
fun ActionPromptTab(
    actionId: String,
    modifier: Modifier = Modifier,
    onDeleted: () -> Unit = {},
    viewModel: ActionEditViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val isOwner by viewModel.isTeamOwner.collectAsStateWithLifecycle()

    // One fetch per page visit: a body a teammate changed is re-read.
    LaunchedEffect(actionId) { viewModel.load(actionId) }
    FailedActionToast(state.failure, onShown = viewModel::clearFailure)

    val form = rememberActionFormState()
    // Seed ONCE per fetched row: a successful save rewrites `state.action`,
    // and re-seeding would fight a person who kept typing.
    var seededId by remember { mutableStateOf<String?>(null) }
    val loaded = state.action
    LaunchedEffect(loaded?.id) {
        val row = loaded ?: return@LaunchedEffect
        if (seededId == row.id) return@LaunchedEffect
        seededId = row.id
        form.seed(row)
    }

    val editable = isOwner && loaded != null && !state.saving
    val canSave = editable && loaded != null && form.differsFrom(loaded) && form.valid
    var confirmDelete by remember { mutableStateOf(false) }

    Column(
        modifier = modifier
            .fillMaxSize()
            .testTag("action-prompt-tab")
            .verticalScroll(rememberScrollState())
            .padding(top = 8.dp),
    ) {
        ActionPromptForm(
            form = form,
            editable = editable,
            bodyLoading = state.loading,
            bodySlot = if (state.loadFailed) {
                {
                    // FEED-79: the fetch ran on open, so no toast: the empty
                    // state + Retry (a failed Retry tap toasts).
                    Column(
                        horizontalAlignment = Alignment.CenterHorizontally,
                        modifier = Modifier.fillMaxWidth().padding(vertical = 16.dp),
                    ) {
                        Text(
                            "Prompt unavailable",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        TextButton(onClick = { viewModel.load(actionId, retried = true) }) {
                            Text(FeedbackStates.RETRY)
                        }
                    }
                }
            } else {
                null
            },
        )

        // A member can read the prompt but not change it: no Save at all.
        if (isOwner) {
            Spacer(Modifier.height(12.dp))
            GlassSubmitButton(
                label = "Save changes",
                enabled = canSave,
                onClick = save@{
                    val row = loaded ?: return@save
                    viewModel.save(row.id, form.fields())
                },
                modifier = Modifier
                    .padding(horizontal = 16.dp)
                    .testTag("action-prompt-save"),
                icon = if (state.saving) {
                    {
                        CircularProgressIndicator(
                            modifier = Modifier.size(16.dp),
                            strokeWidth = 2.dp,
                            color = LocalContentColor.current,
                        )
                    }
                } else {
                    null
                },
            )
            if (loaded != null) {
                Spacer(Modifier.height(12.dp))
                GlassPill(
                    "Delete action",
                    icon = ExpIcons.uiDelete,
                    onClick = { confirmDelete = true },
                    contentColor = DesignTokens.Semantic.Red,
                    modifier = Modifier
                        .padding(horizontal = 16.dp)
                        .fillMaxWidth()
                        .testTag("action-delete"),
                )
            }
        }
        Spacer(Modifier.height(24.dp))
    }

    if (confirmDelete && loaded != null) {
        val prompt = Prompts.DeleteAction.prompt(loaded.name)
        PromptAlert(
            prompt = prompt,
            onDismiss = { confirmDelete = false },
            handlers = prompt.actions.filter { it.role != Prompts.Role.Cancel }.associate { action ->
                action.id to {
                    confirmDelete = false
                    viewModel.delete(loaded.id, onDeleted)
                }
            },
        )
    }
}
